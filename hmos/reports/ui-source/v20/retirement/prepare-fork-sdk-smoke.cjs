'use strict';
// Reuses the existing copy preparer without changing it or any older harness.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const JSON5 = require('C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony/ets/build-tools/ets-loader/node_modules/json5');
const projectSource = path.resolve(__dirname, '../../../..');
const sourcePrepare = path.join(projectSource, 'tool/checkpoint-sdk-smoke/prepare.cjs');
const hash = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const page = [
  "import { EditorDraftForkCoordinator, DraftForkHooks } from '../model/EditorDraftFork';",
  "import { DraftRecord, DraftScope, DraftForkLink, Values, EditorDraftCoordinator, copyScope } from '../model/EditorDraft';",
  '', '@Entry', '@Component', 'struct CheckpointSdkSmoke {',
  "  @State status: string = 'Independent API26 fork compile-only smoke';",
  '  private sequence: number = 0;',
  '  private async controlledSend(serialized: string): Promise<DraftRecord> {',
  "    throw new Error('Compile-only controlled provider: no transport, Store or native fork call');", '  }',
  '  private async compilePublicReferences(): Promise<void> {',
  "    const scope = new DraftScope(); scope.card_id = 'compile-card'; scope.draft_id = 'compile-parent';",
  '    const values = new Values(), restored = new DraftRecord(); restored.scope = scope; restored.values = values;',
  "    restored.operation_id = 'compile-parent-save'; restored.generation = '1'; restored.current_generation = '1';",
  "    restored.active = true; restored.current_active = true; restored.request_sha256 = 'a'.repeat(64);",
  '    const parent = new EditorDraftCoordinator(scope, values,',
  '      (serialized: string): Promise<DraftRecord> => this.controlledSend(serialized),',
  "      (): void => {}, (): string => 'compile-parent-' + (++this.sequence).toString(), restored);",
  "    const childScope = copyScope(scope); childScope.draft_id = 'compile-child';",
  '    const hooks: DraftForkHooks = {',
  '      send: (serialized: string): Promise<DraftRecord> => this.controlledSend(serialized),',
  "      changed: (): void => {}, operation: (): string => 'compile-child-' + (++this.sequence).toString(),",
  '      isCurrent: (): boolean => true', '    };',
  "    const fork = new EditorDraftForkCoordinator(parent, childScope, parent.current, 'compile-first', hooks);",
  '    const original: string = fork.original; const pending: string | undefined = fork.pending;',
  '    const first: Values = fork.firstRaw; const proof: DraftForkLink = fork.parentProof;',
  '    const frozenScope: DraftScope = fork.scope; const confirmed: DraftRecord | undefined = fork.confirmed;',
  '    const blocked: boolean = fork.unknown || fork.conflicted || fork.saving;',
  "    this.status = original.length.toString() + '/' + (pending?.length ?? 0).toString() + '/' + first.title.text + proof.parent_draft_id + frozenScope.card_id + (confirmed?.generation ?? '') + blocked.toString() + fork.error;",
  '    try { const receipt: DraftRecord = await fork.begin(); this.status = receipt.operation_id; } catch (_) {}',
  '    try { const repeated: DraftRecord = await fork.retry(); this.status = repeated.generation; } catch (_) {}',
  '    try {', '      const child: EditorDraftCoordinator = fork.openChild();',
  "      const retirement: string = fork.retirementCommand('compile-retire'); this.status = retirement;",
  '      child.pauseWrites(); child.resumeWrites(); child.dispose();', '    } catch (_) {}',
  '    try { fork.releaseRejectedParent(); } catch (_) {}',
  '    parent.pauseWrites(); parent.resumeWrites(); parent.dispose();', '  }',
  '  build() {', '    Column({ space: 12 }) {', '      Text(this.status)',
  "      Button('Compile-only controlled reference').onClick((): void => { this.compilePublicReferences(); })",
  '    }.width(\'100%\')', '  }', '}', ''
].join('\n');

function prepare(destination) {
  const base = require(sourcePrepare).prepare(destination || path.join(projectSource, '.build/checkpoint-sdk-smoke',
    'fork-dev20-' + new Date().toISOString().replace(/[:.]/g, '-')));
  const manifestPath = base.manifest, manifest = JSON.parse(fs.readFileSync(manifestPath, 'utf8'));
  const rewrites = [];
  function rewrite(relative, content) {
    const generated = manifest.generated_harness.find(item => item.path === relative);
    if (!generated) throw new Error('Only an explicit original generated wrapper may be rewritten: ' + relative);
    const target = path.join(base.project, relative), before = fs.readFileSync(target), bytes = Buffer.from(content);
    if (hash(before) !== generated.sha256 || before.length !== generated.bytes) throw new Error('Base wrapper changed before fork rewrite');
    fs.writeFileSync(target, bytes);
    rewrites.push({ path: relative, from: 'fresh output of tool/checkpoint-sdk-smoke/prepare.cjs',
      before_bytes: before.length, before_sha256: hash(before), after_bytes: bytes.length, after_sha256: hash(bytes) });
    generated.bytes = bytes.length; generated.sha256 = hash(bytes);
    generated.purpose = 'Independent fork compile-only generated wrapper; not product source';
  }
  const app = JSON5.parse(fs.readFileSync(path.join(base.project, 'AppScope/app.json5'), 'utf8'));
  app.app.bundleName = 'dev.morrow.hmos.draftforksdk'; app.app.vendor = 'Morrow Fork SDK Smoke';
  app.app.versionName = '0.1.0-draft-fork-sdk-smoke.20';
  rewrite('AppScope/app.json5', JSON.stringify(app, null, 2) + '\n');
  rewrite('entry/src/main/ets/pages/CheckpointSdkSmoke.ets', page);
  manifest.bundle_name = app.app.bundleName; manifest.smoke_version = app.app.versionName;
  manifest.fork_sdk_smoke = { schema: 1, source_prepare: sourcePrepare, source_prepare_sha256: hash(fs.readFileSync(sourcePrepare)),
    prepare_script_sha256: hash(fs.readFileSync(__filename)), rewritten_wrappers: rewrites,
    actual_models: ['EditorDraft.ets', 'EditorDraftFork.ets'].map(name => {
      const relative = 'entry/src/main/ets/model/' + name, input = manifest.copied_inputs.find(item => item.path === relative);
      if (!input) throw new Error('Actual model absent from fresh copy: ' + relative);
      return { path: relative, bytes: input.bytes, sha256: input.sha256 };
    }), scope: 'Compile-only actual constructor, public methods and View types; controlled provider never calls transport',
    cpp_native_scope: 'Existing CPP/static archives copied solely for isolated SDK compilation; no fork runtime/native identity qualification',
    runtime: 'NOT_RUN', device: 'NOT_RUN', installation: 'NOT_RUN' };
  fs.writeFileSync(manifestPath, JSON.stringify(manifest, null, 2) + '\n');
  for (const generated of manifest.generated_harness) {
    const bytes = fs.readFileSync(path.join(base.project, generated.path));
    if (bytes.length !== generated.bytes || hash(bytes) !== generated.sha256) throw new Error('Final generated wrapper mismatch');
  }
  return { ...base, bundle: manifest.bundle_name, smoke_version: manifest.smoke_version, actual_models: manifest.fork_sdk_smoke.actual_models,
    rewritten_wrappers: rewrites, runtime: 'NOT_RUN', scope: manifest.fork_sdk_smoke.scope };
}
module.exports = { prepare };
if (require.main === module) process.stdout.write(JSON.stringify(prepare(process.argv[2]), null, 2) + '\n');
