'use strict';
// Reuses the immutable copy preparer; only new isolated wrappers are rewritten.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const JSON5 = require('C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony/ets/build-tools/ets-loader/node_modules/json5');
const source = path.resolve(__dirname, '../../../..'), baseScript = path.join(source, 'tool/checkpoint-sdk-smoke/prepare.cjs');
const sha = value => crypto.createHash('sha256').update(value).digest('hex');
const critical = ['model/EditorBusinessSession.ets','model/EditorBusiness.ets','model/EditorDraft.ets','model/EditorFieldPolicy.ets','pages/Index.ets','pages/EditorTodos.ets']
  .map(name => 'entry/src/main/ets/' + name).concat('entry/src/main/cpp/types/libmorrow/index.d.ts');
const page = `import { EditorBusinessSessionCoordinator, EditorBusinessSessionHooks, EditorBusinessSessionReply, EditorIntentProof, EditorIntentView, EditorIntentPage } from '../model/EditorBusinessSession';
import { EditorBusinessFields, EditorBusinessCommit } from '../model/EditorBusiness';
import { DraftRecord, Values } from '../model/EditorDraft';
import { EditorFieldPolicy } from '../model/EditorFieldPolicy';

@Entry
@Component
struct CheckpointSdkSmoke {
  @State status: string = 'Independent API26 Session compile-only smoke';
  private async noTransport(_wire: string): Promise<EditorBusinessSessionReply> {
    throw new Error('Compile-only provider: no native Store or device transport');
  }
  private async references(): Promise<void> {
    const publication = new DraftRecord(), values = new Values(); publication.values = values;
    publication.scope.card_id = 'compile-card'; publication.scope.draft_id = 'compile-draft';
    publication.operation_id = 'compile-publication'; publication.generation = '1'; publication.current_generation = '1';
    publication.active = true; publication.current_active = true; publication.request_sha256 = 'a'.repeat(64);
    const business = new EditorBusinessFields(); business.action = 'create'; business.id = publication.scope.card_id;
    business.operation = 'compile-business'; business.category = values.category; business.stage = values.stage;
    const fields = new EditorFieldPolicy(async (_wire: string): Promise<string> => {
      throw new Error('Compile-only field provider: no field runtime');
    });
    const hooks: EditorBusinessSessionHooks = {
      send: (wire: string): Promise<EditorBusinessSessionReply> => this.noTransport(wire), fields: fields,
      changed: (): void => {}, isCurrent: (): boolean => true, isExact: (): boolean => true, parentReady: (): boolean => true
    };
    const coordinator: EditorBusinessSessionCoordinator = await EditorBusinessSessionCoordinator.prepare('create', business, publication, 'compile-prepare', hooks);
    const proof: EditorIntentProof | undefined = coordinator.proof, view: EditorIntentView | undefined = coordinator.view;
    const receipt: EditorBusinessCommit | undefined = coordinator.confirmed;
    const flags: boolean = coordinator.saving || coordinator.unknown || coordinator.businessUnknown || coordinator.inspectionUnknown ||
      coordinator.businessRejected || coordinator.snapshotAbsent || coordinator.transportReady || coordinator.committed || coordinator.qualified ||
      coordinator.mayConsume(values);
    this.status = coordinator.originalRequest + coordinator.originalPrepare + coordinator.originalIssue + coordinator.originalCancel +
      coordinator.originalSave + coordinator.originalInspect + coordinator.phase + coordinator.pendingNative + coordinator.pendingRead +
      coordinator.error + coordinator.publication.operation_id + (receipt?.revision ?? '') + (view?.phase ?? '') + flags.toString();
    try { const prepared: EditorIntentView = await coordinator.prepareIntent(); this.status = prepared.phase; } catch (_) {}
    try { const retried: EditorIntentView = await coordinator.retryPrepare(); this.status = retried.phase; } catch (_) {}
    try { const issued: EditorIntentView = await coordinator.issueIntent(); this.status = issued.phase; } catch (_) {}
    try { const retried: EditorIntentView = await coordinator.retryIssue(); this.status = retried.phase; } catch (_) {}
    try { await coordinator.loadTransport(); } catch (_) {}
    try { const part: EditorIntentView = await coordinator.readPart('publication'); this.status = part.part; } catch (_) {}
    try { const part: EditorIntentView = await coordinator.retryRead(); this.status = part.part; } catch (_) {}
    try { const saved: EditorBusinessCommit | undefined = await coordinator.save(); this.status = saved?.revision ?? ''; } catch (_) {}
    try { const retried: EditorBusinessCommit | undefined = await coordinator.retrySave(); this.status = retried?.revision ?? ''; } catch (_) {}
    try { const inspected: EditorBusinessCommit | undefined = await coordinator.inspect(); this.status = inspected?.revision ?? ''; } catch (_) {}
    try { const retried: EditorBusinessCommit | undefined = await coordinator.retryInspect(); this.status = retried?.revision ?? ''; } catch (_) {}
    try { const cancelled: EditorIntentView = await coordinator.cancelPrepared('compile-cancel'); this.status = cancelled.phase; } catch (_) {}
    try { const retried: EditorIntentView = await coordinator.retryCancel(); this.status = retried.phase; } catch (_) {}
    try { const next: EditorBusinessSessionCoordinator = await coordinator.continueTodos(business, publication, 'compile-prepare-next', hooks); this.status = next.originalRequest; } catch (_) {}
    try { const page: EditorIntentPage = await EditorBusinessSessionCoordinator.discover('', 16, '', hooks); this.status = page.nextAfter; } catch (_) {}
    if (proof) {
      const restored: EditorBusinessSessionCoordinator = await EditorBusinessSessionCoordinator.restore(proof, hooks);
      this.status = restored.originalRequest;
    }
  }
  build() { Column() { Text(this.status) } }
}
`;
function prepare(destination) {
  const base = require(baseScript).prepare(destination || path.join(source, '.build/checkpoint-sdk-smoke',
    'dev24-business-session-' + new Date().toISOString().replace(/[:.]/g, '-')));
  const manifest = JSON.parse(fs.readFileSync(base.manifest, 'utf8')), rewrites = [];
  function rewrite(relative, content) {
    const item = manifest.generated_harness.find(value => value.path === relative);
    if (!item) throw new Error('Rewrite only a fresh generated wrapper');
    const file = path.join(base.project, relative), before = fs.readFileSync(file), next = Buffer.from(content);
    if (before.length !== item.bytes || sha(before) !== item.sha256) throw new Error('Base wrapper identity changed');
    fs.writeFileSync(file, next); rewrites.push({ path: relative, from: 'Fresh base prepare.cjs generated wrapper',
      before_bytes: before.length, before_sha256: sha(before), after_bytes: next.length, after_sha256: sha(next) });
    item.bytes = next.length; item.sha256 = sha(next); item.purpose = 'Independent Session SDK compile-only wrapper; not product source';
  }
  const app = JSON5.parse(fs.readFileSync(path.join(base.project, 'AppScope/app.json5'), 'utf8'));
  app.app.bundleName = 'dev.morrow.hmos.editorbusinesssessionsdk'; app.app.vendor = 'Morrow Session SDK Smoke';
  app.app.versionCode = 2400024; app.app.versionName = '0.1.0-editor-business-session-sdk-smoke.24';
  rewrite('AppScope/app.json5', JSON.stringify(app, null, 2) + '\n');
  rewrite('entry/src/main/ets/pages/CheckpointSdkSmoke.ets', page);
  manifest.bundle_name = app.app.bundleName; manifest.smoke_version = app.app.versionName;
  const actual = critical.map(relative => {
    const item = manifest.copied_inputs.find(value => value.path === relative);
    if (!item) throw new Error('Missing actual compile input: ' + relative);
    return { path: relative, bytes: item.bytes, sha256: item.sha256 };
  });
  if (actual[0].sha256 !== '792e95bc45d5d589a08810b4a3bed24fa0c6229612fb38fda18abd12a9821fbd') throw new Error('Frozen Session model changed');
  const expected = new Map([
    ['entry/src/main/ets/pages/Index.ets','aff77eb7f1c90e53ddf5d64dd89fb3a037dab98ac0b08ae0382090a2055efeaf'],
    ['entry/src/main/ets/pages/EditorTodos.ets','83b6c6a5c5811ead9fa14ebb23f9b5dea7f9a858fd0b5461181091e929473836'],
    ['entry/src/main/ets/model/EditorBusiness.ets','6469d1f2af4e2da6b9066b52917e658bc13ececb4ef2a4e6b92fbb4ad0d996cf'],
    ['entry/src/main/ets/model/EditorDraft.ets','a3ae1e99d198fcba3cdef741a6f138612ad16979be42c7a8809003f00d04f965'],
    ['entry/src/main/ets/model/EditorFieldPolicy.ets','10c3162e77a947335acf0c3df2e464c80195d0ac2bbc2922bbb09c0a8a626842']
  ]);
  for (const item of actual) if (expected.has(item.path) && expected.get(item.path) !== item.sha256) throw new Error('Frozen input changed: ' + item.path);
  manifest.session_sdk_smoke = { schema: 1, source_prepare: baseScript, source_prepare_sha256: sha(fs.readFileSync(baseScript)),
    prepare_script_sha256: sha(fs.readFileSync(__filename)), actual_compile_inputs: actual, rewritten_wrappers: rewrites,
    scope: 'Actual EditorBusinessSession prepare/discover/restore construction and all public methods; compile-only controlled providers; no Index integration',
    source_parallel_scope: 'All copied inputs are immutable snapshots. Concurrent product source changes are reported separately; business/type dependencies must remain exact.',
    cpp_native_scope: 'Copied existing CPP/archives solely for compile-only SDK. No new native adoption or native editor business runtime identity.',
    runtime: 'NOT_RUN', device: 'NOT_RUN', installation: 'NOT_RUN' };
  fs.writeFileSync(base.manifest, JSON.stringify(manifest, null, 2) + '\n');
  return { ...base, bundle: manifest.bundle_name, smoke_version: manifest.smoke_version,
    actual_compile_inputs: actual, rewritten_wrappers: rewrites, runtime: 'NOT_RUN' };
}
module.exports = { prepare };
if (require.main === module) process.stdout.write(JSON.stringify(prepare(process.argv[2]), null, 2) + '\n');


