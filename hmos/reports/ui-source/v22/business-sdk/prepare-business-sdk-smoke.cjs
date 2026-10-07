'use strict';
// Reuses the immutable copy preparer; only new isolated wrappers are rewritten.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const JSON5 = require('C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony/ets/build-tools/ets-loader/node_modules/json5');
const source = path.resolve(__dirname, '../../../..'), baseScript = path.join(source, 'tool/checkpoint-sdk-smoke/prepare.cjs');
const sha = value => crypto.createHash('sha256').update(value).digest('hex');
const critical = ['EditorBusiness.ets', 'EditorDraft.ets', 'EditorFieldPolicy.ets', 'Workbench.ets', 'Attachments.ets', 'Markdown.ets']
  .map(name => 'entry/src/main/ets/model/' + name).concat('entry/src/main/cpp/types/libmorrow/index.d.ts');
const page = `import { EditorBusinessCoordinator, EditorBusinessFields, EditorBusinessCommit, EditorBusinessReply, EditorBusinessHooks, EditorBusinessSubmission } from '../model/EditorBusiness';
import { DraftRecord, Values } from '../model/EditorDraft';
import { EditorFieldPolicy } from '../model/EditorFieldPolicy';

@Entry
@Component
struct CheckpointSdkSmoke {
  @State status: string = 'Independent API26 business compile-only smoke';
  private async noTransport(_request: string): Promise<EditorBusinessReply> {
    throw new Error('Compile-only provider: no native request, Store or device transport');
  }
  private async references(): Promise<void> {
    const record = new DraftRecord(), values = new Values(); record.values = values;
    record.scope.card_id = 'compile-card'; record.scope.draft_id = 'compile-publication';
    record.operation_id = 'compile-save'; record.generation = '1'; record.current_generation = '1';
    record.active = true; record.current_active = true; record.request_sha256 = 'a'.repeat(64);
    const business = new EditorBusinessFields(); business.action = 'create'; business.id = record.scope.card_id;
    business.operation = 'compile-business'; business.category = values.category; business.stage = values.stage;
    const fields = new EditorFieldPolicy(async (_request: string): Promise<string> => {
      throw new Error('Compile-only field provider: no formatter or field runtime');
    });
    const hooks: EditorBusinessHooks = {
      send: (request: string): Promise<EditorBusinessReply> => this.noTransport(request), fields: fields,
      changed: (): void => {}, isCurrent: (): boolean => true, isExact: (): boolean => true
    };
    const coordinator: EditorBusinessCoordinator = await EditorBusinessCoordinator.prepare('create', business, record, hooks);
    const original: string = coordinator.originalRequest, saveWire: string = coordinator.originalSave;
    const submission: EditorBusinessSubmission = coordinator.submission, publication: DraftRecord = coordinator.publication;
    const confirmation: EditorBusinessCommit | undefined = coordinator.confirmed;
    const flags: boolean = coordinator.committed || coordinator.qualified || coordinator.unknown || coordinator.checkingUnknown ||
      coordinator.saving || coordinator.snapshotAbsent || coordinator.rejected || coordinator.mayConsume(values);
    this.status = original + saveWire + JSON.stringify(submission) + publication.operation_id + (confirmation?.revision ?? '') +
      flags.toString() + coordinator.error + (coordinator.pendingInspect ?? '');
    try { const first: EditorBusinessCommit | undefined = await coordinator.save(); this.status = first?.revision ?? ''; } catch (_) {}
    try { const retry: EditorBusinessCommit | undefined = await coordinator.retrySave(); this.status = retry?.revision ?? ''; } catch (_) {}
    try { const inspected: EditorBusinessCommit | undefined = await coordinator.inspect('1'); this.status = inspected?.revision ?? ''; } catch (_) {}
    try { const retry: EditorBusinessCommit | undefined = await coordinator.retryInspect(); this.status = retry?.revision ?? ''; } catch (_) {}
    try { const next: EditorBusinessCoordinator = await coordinator.continueTodos(business, publication, hooks); this.status = next.originalRequest; } catch (_) {}
    const restored: EditorBusinessCoordinator = await EditorBusinessCoordinator.restore(original, publication, hooks);
    this.status = restored.originalRequest;
  }
  build() {
    Column({ space: 12 }) { Text(this.status) }
  }
}
`;
function prepare(destination) {
  const base = require(baseScript).prepare(destination || path.join(source, '.build/checkpoint-sdk-smoke',
    'business-dev22-' + new Date().toISOString().replace(/[:.]/g, '-')));
  const manifest = JSON.parse(fs.readFileSync(base.manifest, 'utf8')), rewrites = [];
  function rewrite(relative, content) {
    const item = manifest.generated_harness.find(value => value.path === relative);
    if (!item) throw new Error('Rewrite only a fresh generated wrapper');
    const file = path.join(base.project, relative), before = fs.readFileSync(file), next = Buffer.from(content);
    if (before.length !== item.bytes || sha(before) !== item.sha256) throw new Error('Base wrapper identity changed');
    fs.writeFileSync(file, next); rewrites.push({ path: relative, from: 'Fresh base prepare.cjs generated wrapper',
      before_bytes: before.length, before_sha256: sha(before), after_bytes: next.length, after_sha256: sha(next) });
    item.bytes = next.length; item.sha256 = sha(next); item.purpose = 'Independent business SDK compile-only wrapper; not product source';
  }
  const app = JSON5.parse(fs.readFileSync(path.join(base.project, 'AppScope/app.json5'), 'utf8'));
  app.app.bundleName = 'dev.morrow.hmos.editorbusinesssdk'; app.app.vendor = 'Morrow Business SDK Smoke';
  app.app.versionCode = 2200022; app.app.versionName = '0.1.0-editor-business-sdk-smoke.22';
  rewrite('AppScope/app.json5', JSON.stringify(app, null, 2) + '\n');
  rewrite('entry/src/main/ets/pages/CheckpointSdkSmoke.ets', page);
  manifest.bundle_name = app.app.bundleName; manifest.smoke_version = app.app.versionName;
  const actual = critical.map(relative => {
    const item = manifest.copied_inputs.find(value => value.path === relative);
    if (!item) throw new Error('Missing actual compile input: ' + relative);
    return { path: relative, bytes: item.bytes, sha256: item.sha256 };
  });
  if (actual[0].sha256 !== '6469d1f2af4e2da6b9066b52917e658bc13ececb4ef2a4e6b92fbb4ad0d996cf') throw new Error('Frozen business model changed');
  manifest.business_sdk_smoke = { schema: 1, source_prepare: baseScript, source_prepare_sha256: sha(fs.readFileSync(baseScript)),
    prepare_script_sha256: sha(fs.readFileSync(__filename)), actual_compile_inputs: actual, rewritten_wrappers: rewrites,
    scope: 'Actual EditorBusiness prepare/restore construction and public methods; compile-only controlled providers; no Index integration',
    source_parallel_scope: 'All copied inputs are immutable snapshots. Concurrent product source changes are reported separately; business/type dependencies must remain exact.',
    cpp_native_scope: 'Copied existing CPP/archives solely for compile-only SDK. No new native adoption or native editor business runtime identity.',
    runtime: 'NOT_RUN', device: 'NOT_RUN', installation: 'NOT_RUN' };
  fs.writeFileSync(base.manifest, JSON.stringify(manifest, null, 2) + '\n');
  return { ...base, bundle: manifest.bundle_name, smoke_version: manifest.smoke_version,
    actual_compile_inputs: actual, rewritten_wrappers: rewrites, runtime: 'NOT_RUN' };
}
module.exports = { prepare };
if (require.main === module) process.stdout.write(JSON.stringify(prepare(process.argv[2]), null, 2) + '\n');
