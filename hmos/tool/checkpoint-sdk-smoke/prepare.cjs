'use strict';
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const JSON5 = require('C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony/ets/build-tools/ets-loader/node_modules/json5');
const projectSource = path.resolve(__dirname, '../..');
const work = path.join(projectSource, '.build/checkpoint-sdk-smoke');
const hash = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const slash = value => value.replace(/\\/g, '/');
const omit = new Set(['build', '.cxx', '.hvigor', '.git', '.idea', '.test', 'node_modules']);

function prepare(destination) {
  const project = path.resolve(destination || path.join(work, 'dev20-' + new Date().toISOString().replace(/[:.]/g, '-')));
  if (!project.startsWith(work + path.sep) || fs.existsSync(project)) throw new Error('Use a fresh directory strictly inside checkpoint-sdk-smoke; preserve earlier evidence.');
  fs.mkdirSync(project, { recursive: true });
  const inputs = [], generated = [];
  function copy(relative, ancestors = new Set()) {
    const source = path.join(projectSource, relative), target = path.join(project, relative);
    const stat = fs.statSync(source), real = fs.realpathSync(source);
    if (stat.isDirectory()) {
      if (ancestors.has(real)) throw new Error('Dependency directory cycle: ' + relative);
      const next = new Set(ancestors); next.add(real); fs.mkdirSync(target, { recursive: true });
      for (const name of fs.readdirSync(source).sort()) if (!omit.has(name)) copy(path.join(relative, name), next);
    } else if (stat.isFile()) {
      const bytes = fs.readFileSync(source); fs.mkdirSync(path.dirname(target), { recursive: true }); fs.writeFileSync(target, bytes, { flag: 'wx' });
      const sha256 = hash(bytes); if (hash(fs.readFileSync(target)) !== sha256) throw new Error('Copy hash mismatch: ' + relative);
      inputs.push({ path: slash(relative), source_path: source, source_real_path: real, bytes: bytes.length, sha256, copied_sha256: sha256 });
    } else throw new Error('Unsupported source: ' + relative);
  }
  for (const relative of ['AppScope', 'entry', 'hvigor', 'oh_modules', 'build-profile.json5', 'code-linter.json5',
    'hvigorfile.ts', 'oh-package.json5', 'oh-package-lock.json5']) copy(relative);
  function write(relative, content) {
    const bytes = Buffer.from(content), target = path.join(project, relative); fs.mkdirSync(path.dirname(target), { recursive: true }); fs.writeFileSync(target, bytes);
    generated.push({ path: relative, bytes: bytes.length, sha256: hash(bytes), purpose: 'Independent compile-only harness; not product source' });
  }
  const app = JSON5.parse(fs.readFileSync(path.join(project, 'AppScope/app.json5'), 'utf8'));
  app.app.bundleName = 'dev.morrow.hmos.checkpointsdk'; app.app.vendor = 'Morrow SDK Smoke';
  app.app.versionCode = 2000020; app.app.versionName = '0.1.0-sdk-smoke.20';
  write('AppScope/app.json5', JSON.stringify(app, null, 2) + '\n');
  const module = JSON5.parse(fs.readFileSync(path.join(project, 'entry/src/main/module.json5'), 'utf8'));
  module.module.mainElement = 'CheckpointSmokeAbility';
  module.module.abilities[0].name = 'CheckpointSmokeAbility'; module.module.abilities[0].srcEntry = './ets/entryability/CheckpointSmokeAbility.ets';
  module.module.abilities[0].exported = false; delete module.module.abilities[0].skills;
  write('entry/src/main/module.json5', JSON.stringify(module, null, 2) + '\n');
  write('entry/src/main/resources/base/profile/main_pages.json', JSON.stringify({ src: ['pages/CheckpointSdkSmoke'] }, null, 2) + '\n');
  write('entry/src/main/ets/entryability/CheckpointSmokeAbility.ets', `import { UIAbility } from '@kit.AbilityKit';
import { window } from '@kit.ArkUI';
export default class CheckpointSmokeAbility extends UIAbility {
  onWindowStageCreate(stage: window.WindowStage): void { stage.loadContent('pages/CheckpointSdkSmoke'); }
}
`);
  write('entry/src/main/ets/pages/CheckpointSdkSmoke.ets', `import { EditorTodos } from './EditorTodos';
import { EditorInputPolicy } from '../model/EditorInputPolicy';
import { editorInputHash } from '../model/EditorInputHash';
import { EditorFieldPolicy } from '../model/EditorFieldPolicy';
import { TextValue, copyText } from '../model/EditorDraft';
import { samePasteTarget } from '../model/EditorPaste';
import native from 'libmorrow.so';

@Entry
@Component
struct CheckpointSdkSmoke {
  @State value: TextValue = new TextValue();
  @State revision: number = 0;
  @State digest: string = '';
  private input: EditorInputPolicy = new EditorInputPolicy((request: string): Promise<string> => native.editorInput(request), editorInputHash);
  private field: EditorFieldPolicy = new EditorFieldPolicy((request: string): Promise<string> => native.editorField(request));
  private capture(value: TextValue, owner: string): number | undefined {
    if (owner !== 'checkpoint-sdk-smoke') { return undefined; }
    this.value = copyText(value); return ++this.revision;
  }
  private owns(owner: string, revision: number, value: TextValue): boolean {
    return owner === 'checkpoint-sdk-smoke' && revision === this.revision && samePasteTarget(this.value, value);
  }
  private async probe(): Promise<void> {
    this.digest = await editorInputHash('SDK compile-only smoke');
    const old = new TextValue(), next = new TextValue(); next.text = 'sdk smoke';
    await this.input.format('title', old, next, (): boolean => true);
  }
  build() {
    Column({ space: 12 }) {
      Text('Independent API26 compile-only smoke')
      EditorTodos({ ownerKey: 'checkpoint-sdk-smoke', revision: this.revision, source: this.value,
        onCapture: (value: TextValue, owner: string): number | undefined => this.capture(value, owner),
        isCurrent: (owner: string, revision: number, value: TextValue): boolean => this.owns(owner, revision, value),
        count: async (text: string, owned: () => boolean): Promise<number> => (await this.field.checkField('todos', text, owned)).grapheme_count,
        format: (old: TextValue, next: TextValue, remaining: number, owned: () => boolean) => this.input.format('todos', old, next, owned, remaining)
      })
      Button('Compile reference to hash + native formatter').onClick((): void => { this.probe(); })
      Text(this.digest)
    }.width('100%')
  }
}
`);
  const manifest = { schema: 1, created_utc: new Date().toISOString(), project_source: projectSource, project,
    bundle_name: app.app.bundleName, smoke_version: app.app.versionName, source_version: JSON5.parse(fs.readFileSync(path.join(projectSource, 'AppScope/app.json5'), 'utf8')).app.versionName,
    sdk: '26.0.0', copied_inputs: inputs, generated_harness: generated, device: 'NOT_RUN', installation: 'NOT_RUN' };
  fs.writeFileSync(path.join(project, 'source-copy-manifest.json'), JSON.stringify(manifest, null, 2) + '\n', { flag: 'wx' });
  const generatedNames = new Set(generated.map(x => x.path));
  for (const input of inputs) if (!generatedNames.has(input.path) && hash(fs.readFileSync(path.join(project, input.path))) !== input.sha256) throw new Error('Post-prepare source mismatch: ' + input.path);
  return { project, manifest: path.join(project, 'source-copy-manifest.json'), copied_files: inputs.length,
    generated_files: generated.length, bundle: manifest.bundle_name, source_version: manifest.source_version, smoke_version: manifest.smoke_version,
    device: 'NOT_RUN', installation: 'NOT_RUN' };
}
module.exports = { prepare };
if (require.main === module) process.stdout.write(JSON.stringify(prepare(process.argv[2]), null, 2) + '\n');
