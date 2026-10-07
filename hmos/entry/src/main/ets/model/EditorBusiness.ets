import { cryptoFramework } from '@kit.CryptoArchitectureKit';
import { util } from '@kit.ArkTS';
import { CardView } from './Workbench';
import { DraftRecord, Values, copyRecord, copyValues, nextGeneration, sameValues, validGeneration, validateRecord } from './EditorDraft';
import { EditorFieldPolicy } from './EditorFieldPolicy';

// Isolated trusted-development editor protocol. Native reconstructs the exact
// original Core command/history. This module does not grant protected capture,
// perform a draft handoff, rebase from current cards, or reconcile V2 row IDs.
const JSON_LIMIT: number = 512 * 1024;

export class EditorBusinessFields {
  action: string = ''; id: string = ''; operation: string = ''; source: string = '';
  title: string = ''; description: string = ''; hypothesis: string = ''; conclusion: string = '';
  todos: string = ''; category: string = ''; stage: string = '';
}
export class EditorPublicationProof {
  draft_id: string = ''; generation: string = ''; save_operation: string = ''; request_sha256: string = '';
}
export class EditorBusinessBaseline {
  operation: string = ''; revision: string = ''; command_sha256: string = ''; content_sha256: string = '';
  request_sha256: string = ''; publication_sha256: string = '';
}
export class EditorBusinessContinuation {
  root_request_json: string = ''; baseline: EditorBusinessBaseline = new EditorBusinessBaseline();
}
export class EditorBusinessSubmission {
  schema_version: number = 1; mode: string = '';
  business: EditorBusinessFields = new EditorBusinessFields();
  publication: EditorPublicationProof = new EditorPublicationProof();
  continuation: EditorBusinessContinuation | null = null;
}
export interface EditorBusinessCommit {
  commit_status: string; qualification: string; card_id: string; operation: string;
  source_revision: string; revision: string; event_id: string; command_sha256: string; content_sha256: string;
  request_sha256: string; publication_sha256: string; publication: EditorPublicationProof;
  historical_card: CardView | null; live_matches: boolean; live_revision: string;
}
export interface EditorBusinessReply {
  ok: boolean; error: string; effect: string; receipt_revision: string;
  editor_commit?: EditorBusinessCommit;
}
export interface EditorBusinessHooks {
  send: (serialized: string) => Promise<EditorBusinessReply>;
  fields: EditorFieldPolicy;
  changed: () => void;
  // isCurrent freezes the view/session owner; isExact also freezes its input
  // epoch. Comparing identical values alone cannot detect an away/back edit.
  isCurrent: () => boolean;
  isExact: () => boolean;
  hash?: (bytes: Uint8Array) => Promise<string>;
}
class SaveWire { request_json: string = ''; }
class SaveCommand { action: string = 'editor_save'; editor_save: SaveWire = new SaveWire(); }
class InspectWire { request_json: string = ''; expected_revision: string = ''; }
class InspectCommand { action: string = 'editor_commit_inspect'; editor_commit: InspectWire = new InspectWire(); }

function requireString(value: string): void {
  if (typeof value !== 'string') { throw new Error('Invalid editor string'); }
  for (let index: number = 0; index < value.length; index++) {
    const code = value.charCodeAt(index);
    if (code >= 0xD800 && code <= 0xDBFF) {
      const next = value.charCodeAt(++index);
      if (!(next >= 0xDC00 && next <= 0xDFFF)) { throw new Error('Incomplete Unicode editor input'); }
    } else if (code >= 0xDC00 && code <= 0xDFFF) { throw new Error('Incomplete Unicode editor input'); }
  }
}
function utf8(value: string): Uint8Array { requireString(value); return new util.TextEncoder().encodeInto(value); }
function budget(wire: string): void {
  if (utf8(wire).length > JSON_LIMIT) { throw new Error('Complete editor JSON bytes limit'); }
}
function digest(value: string): boolean { return typeof value === 'string' && /^[0-9a-f]{64}$/.test(value); }
function positive(value: string): boolean { return validGeneration(value) && value !== '0'; }
function source(value: string): boolean { return typeof value === 'string' && /^(?:[0-9a-f]{2})+$/.test(value); }
function identity(value: string): void {
  requireString(value); if (!value || utf8(value).length > 256 || /[\u0000-\u001f\u007f-\u009f/\\:]/.test(value)) {
    throw new Error('Invalid editor identity');
  }
}
function bytes(hex: string): Uint8Array {
  if (!source(hex)) { throw new Error('Invalid complete editor source'); }
  const result = new Uint8Array(hex.length / 2);
  for (let index: number = 0; index < result.length; index++) { result[index] = parseInt(hex.slice(index * 2, index * 2 + 2), 16); }
  return result;
}
// Canonical unsigned u64 little-endian without lossy Number/BigInt conversion.
function littleEndian(value: string): Uint8Array {
  if (!validGeneration(value)) { throw new Error('Invalid editor u64'); }
  const result = new Uint8Array(8); let remaining = value;
  for (let at: number = 0; at < 8; at++) {
    let quotient = '', remainder = 0;
    for (let index: number = 0; index < remaining.length; index++) {
      const digit = remainder * 10 + remaining.charCodeAt(index) - 48;
      const divided = Math.floor(digit / 256); remainder = digit % 256;
      if (quotient || divided) { quotient += divided.toString(); }
    }
    result[at] = remainder; remaining = quotient || '0';
  }
  if (remaining !== '0') { throw new Error('Editor u64 overflow'); }
  return result;
}
function append(target: number[], value: Uint8Array): void { for (const byte of value) { target.push(byte); } }
function frame(target: number[], value: string): void {
  const encoded = utf8(value); append(target, littleEndian(encoded.length.toString())); append(target, encoded);
}
async function hashBytes(value: Uint8Array): Promise<string> {
  const hash = cryptoFramework.createMd('SHA256'); await hash.update({ data: value });
  const result = await hash.digest(); let hex = '';
  for (const byte of result.data) { hex += byte.toString(16).padStart(2, '0'); }
  return hex;
}
function publicationBytes(card: string, proof: EditorPublicationProof): Uint8Array {
  const result: number[] = []; append(result, utf8('morrow.hmos.editor-publication.v1\u0000'));
  frame(result, card); frame(result, proof.draft_id); append(result, littleEndian(proof.generation));
  frame(result, proof.save_operation); append(result, bytes(proof.request_sha256)); return new Uint8Array(result);
}
function samePublication(a: EditorPublicationProof, b: EditorPublicationProof): boolean {
  return !!a && !!b && a.draft_id === b.draft_id && a.generation === b.generation &&
    a.save_operation === b.save_operation && a.request_sha256 === b.request_sha256;
}
function copySubmission(value: EditorBusinessSubmission): EditorBusinessSubmission {
  return JSON.parse(JSON.stringify(value)) as EditorBusinessSubmission;
}
function copyCommit(value: EditorBusinessCommit): EditorBusinessCommit {
  return JSON.parse(JSON.stringify(value)) as EditorBusinessCommit;
}
function validateProof(proof: EditorPublicationProof): void {
  if (!proof) { throw new Error('Missing editor publication'); }
  identity(proof.draft_id); identity(proof.save_operation);
  if (!positive(proof.generation) || !digest(proof.request_sha256)) { throw new Error('Invalid editor publication proof'); }
}
function validateSubmission(value: EditorBusinessSubmission): void {
  if (!value || value.schema_version !== 1 || !value.business ||
    !['create', 'edit', 'continued_todos'].includes(value.mode)) { throw new Error('Invalid editor Submission'); }
  const field = value.business; identity(field.id); identity(field.operation);
  if (field.id.startsWith('morrow-host-')) { throw new Error('Private editor card identity'); }
  for (const text of [field.action, field.source, field.title, field.description, field.hypothesis, field.conclusion,
    field.todos, field.category, field.stage]) { requireString(text); }
  validateProof(value.publication);
  if (value.mode === 'create' ? field.action !== 'create' || field.source !== '' || value.continuation !== null :
    field.action !== 'edit' || !source(field.source)) { throw new Error('Editor mode/source differs'); }
  if (value.mode === 'edit' && (field.todos !== '' || value.continuation !== null)) {
    throw new Error('Existing V2 editor does not accept legacy todos');
  }
  if (value.mode === 'continued_todos') {
    const context = value.continuation;
    if (!context || typeof context.root_request_json !== 'string' || !context.baseline) { throw new Error('Missing editor root/baseline'); }
    budget(context.root_request_json);
    const root = JSON.parse(context.root_request_json) as EditorBusinessSubmission;
    if (!root || root.mode !== 'create') { throw new Error('Editor continuation root must be an original create'); }
    validateSubmission(root);
    const baseline = context.baseline; identity(baseline.operation);
    if (root.business.id !== field.id || baseline.operation === field.operation || !positive(baseline.revision) ||
      ![baseline.command_sha256, baseline.content_sha256, baseline.request_sha256, baseline.publication_sha256].every(digest)) {
      throw new Error('Invalid editor continuation baseline');
    }
  }
}
function bindPublication(value: EditorBusinessSubmission, record: DraftRecord, current: boolean): void {
  validateRecord(record, true);
  const scope = record.scope, field = value.business;
  if (!scope || scope.card_id !== field.id || scope.draft_id !== value.publication.draft_id ||
    record.operation_id !== value.publication.save_operation || record.generation !== value.publication.generation ||
    record.request_sha256 !== value.publication.request_sha256 || !record.active || !positive(scope.source_revision) && scope.source_kind !== 1 ||
    !validGeneration(record.current_generation) || record.current_generation === '0' ||
    current && (!record.current_active || record.current_generation !== record.generation) ||
    (value.mode === 'create' ? scope.source_kind !== 1 || scope.source_revision !== '0' || scope.source !== '' :
      scope.source_kind !== 0 || scope.source !== field.source || !positive(scope.source_revision))) {
    throw new Error('Editor publication scope/history differs');
  }
  const raw = record.values;
  if (field.title !== raw.title.text || field.description !== raw.description.text || field.hypothesis !== raw.hypothesis.text ||
    field.conclusion !== raw.conclusion.text || field.todos !== raw.todos.text || field.category !== raw.category || field.stage !== raw.stage) {
    throw new Error('Editor business differs from complete published raw');
  }
  for (const text of [raw.title, raw.description, raw.hypothesis, raw.conclusion, raw.todos]) {
    if (text.composing_start !== -1 || text.composing_end !== -1) { throw new Error('Editor business composition is incomplete'); }
  }
  if (value.mode === 'continued_todos' && value.continuation?.baseline.revision !== scope.source_revision) {
    throw new Error('Editor continuation source revision differs');
  }
}
function validateCard(card: CardView, field: EditorBusinessFields, revision: string, publication: DraftRecord, strict: boolean): void {
  if (!card || card.id !== field.id || card.revision !== revision || !source(card.source) ||
    card.title !== field.title || card.description !== field.description || card.hypothesis !== field.hypothesis ||
    card.conclusion !== field.conclusion || strict && (card.category !== field.category || card.stage !== field.stage) ||
    typeof card.favorite !== 'boolean' || typeof card.deleted !== 'boolean' || card.deleted || !validGeneration(card.deleted_at) ||
    !Array.isArray(card.tasks) || card.tasks.length > 128 || !Array.isArray(card.assets) || card.assets.length !== publication.assets.length) {
    throw new Error('Editor historical Card projection differs');
  }
  requireString(card.category); requireString(card.stage);
  const tasks = new Set<string>();
  for (const task of card.tasks) {
    identity(task.id); requireString(task.text);
    if (tasks.has(task.id) || !task.text || utf8(task.text).length > 2048 || ![0, 1, 2].includes(task.completion)) {
      throw new Error('Invalid editor historical Task identities');
    }
    tasks.add(task.id);
  }
  const assets = new Set<string>();
  for (let index: number = 0; index < card.assets.length; index++) {
    const asset = card.assets[index], pin = publication.assets[index];
    if (!asset) { throw new Error('Incomplete editor historical asset inventory'); }
    identity(asset.id); requireString(asset.name); requireString(asset.kind); requireString(asset.media_type);
    if (assets.has(asset.id) || asset.id !== pin.selection.asset_id || asset.name !== pin.display_name ||
      asset.byte_length !== pin.byte_length || asset.sha256 !== pin.sha256 || asset.media_type !== pin.media_type || !digest(asset.sha256)) {
      throw new Error('Editor historical ordered asset inventory differs');
    }
    assets.add(asset.id);
  }
}

export class EditorBusinessCoordinator {
  private frozen: EditorBusinessSubmission;
  private raw: DraftRecord;
  private hooks: EditorBusinessHooks;
  private request: string;
  private saveWire: string;
  private requestHash: string = '';
  private publicationHash: string = '';
  private originalRevision: string;
  private phase: string;
  private receipt: EditorBusinessCommit | undefined = undefined;
  private work: Promise<EditorBusinessCommit | undefined> | undefined = undefined;
  private workKind: string = '';
  private inspectWire: string = '';
  private inspectUnknown: boolean = false;
  private absence: boolean = false;
  private lastError: string = '';

  private constructor(request: string, record: DraftRecord, hooks: EditorBusinessHooks, restored: boolean) {
    budget(request); this.request = request; this.frozen = JSON.parse(request) as EditorBusinessSubmission;
    validateSubmission(this.frozen); this.raw = copyRecord(record); bindPublication(this.frozen, this.raw, !restored);
    this.hooks = hooks; this.phase = restored ? 'unknown' : 'frozen';
    this.originalRevision = nextGeneration(this.raw.scope.source_revision);
    const command = new SaveCommand(); command.editor_save.request_json = request; this.saveWire = JSON.stringify(command); budget(this.saveWire);
    this.makeInspect(this.originalRevision);
  }

  static async prepare(mode: string, business: EditorBusinessFields, record: DraftRecord,
    hooks: EditorBusinessHooks): Promise<EditorBusinessCoordinator> {
    if (mode !== 'create' && mode !== 'edit') { throw new Error('Use an own qualified commit to continue legacy todos'); }
    const value = new EditorBusinessSubmission(); value.mode = mode;
    value.business = JSON.parse(JSON.stringify(business)) as EditorBusinessFields;
    value.publication.draft_id = record.scope.draft_id; value.publication.generation = record.generation;
    value.publication.save_operation = record.operation_id; value.publication.request_sha256 = record.request_sha256;
    return EditorBusinessCoordinator.prepareSubmission(value, record, hooks);
  }

  // Restoring an issued request retains its original bytes. Publication may be
  // an immutable historical active save whose live journal has since retired.
  static async restore(request: string, publication: DraftRecord, hooks: EditorBusinessHooks): Promise<EditorBusinessCoordinator> {
    const model = new EditorBusinessCoordinator(request, publication, hooks, true); await model.initialize(); return model;
  }

  private static async prepareSubmission(value: EditorBusinessSubmission, record: DraftRecord,
    hooks: EditorBusinessHooks): Promise<EditorBusinessCoordinator> {
    const model = new EditorBusinessCoordinator(JSON.stringify(value), record, hooks, false);
    const owned = (): boolean => hooks.isCurrent() && hooks.isExact();
    if (!owned()) { throw new Error('Editor proposal owner/input epoch changed'); }
    await hooks.fields.requireValues(copyValues(model.raw.values), owned);
    await model.initialize(); if (!owned()) { throw new Error('Editor proposal owner/input epoch changed'); } return model;
  }

  private async hash(value: Uint8Array): Promise<string> {
    const result = await (this.hooks.hash ? this.hooks.hash(value) : hashBytes(value));
    if (!digest(result)) { throw new Error('Editor SHA256 unavailable'); } return result;
  }
  private async initialize(): Promise<void> {
    this.requestHash = await this.hash(utf8(this.request));
    this.publicationHash = await this.hash(publicationBytes(this.frozen.business.id, this.frozen.publication));
  }
  get originalRequest(): string { return this.request; }
  get originalSave(): string { return this.saveWire; }
  get submission(): EditorBusinessSubmission { return copySubmission(this.frozen); }
  get publication(): DraftRecord { return copyRecord(this.raw); }
  get confirmed(): EditorBusinessCommit | undefined { return this.receipt ? copyCommit(this.receipt) : undefined; }
  get committed(): boolean { return !!this.receipt; }
  get qualified(): boolean { return this.receipt?.qualification === 'development_editor_wire_v1'; }
  get unknown(): boolean { return this.phase === 'unknown'; }
  get checkingUnknown(): boolean { return this.inspectUnknown; }
  get saving(): boolean { return this.work !== undefined; }
  get snapshotAbsent(): boolean { return this.absence; }
  get rejected(): boolean { return this.phase === 'rejected'; }
  get error(): string { return this.lastError; }
  get pendingInspect(): string | undefined { return this.inspectUnknown ? this.inspectWire : undefined; }

  mayConsume(values: Values): boolean {
    return this.qualified && this.hooks.isCurrent() && this.hooks.isExact() && sameValues(values, this.raw.values);
  }
  save(): Promise<EditorBusinessCommit | undefined> {
    if (this.work) { return this.workKind === 'save' ? this.work : Promise.reject(new Error('Editor inspection is already in flight')); }
    if (this.phase !== 'frozen' || !this.hooks.isCurrent() || !this.hooks.isExact()) {
      return Promise.reject(new Error('Editor save requires its frozen live proposal'));
    }
    return this.issue(this.saveWire, false, false);
  }
  retrySave(): Promise<EditorBusinessCommit | undefined> {
    if (this.work) { return this.workKind === 'save' ? this.work : Promise.reject(new Error('Editor inspection is already in flight')); }
    if (this.phase !== 'unknown') { return Promise.reject(new Error('No uncertain original editor save to retry')); }
    return this.issue(this.saveWire, false, true);
  }
  inspect(expectedRevision: string): Promise<EditorBusinessCommit | undefined> {
    if (this.work) { return Promise.reject(new Error('Editor request is already in flight')); }
    if (this.inspectUnknown) { return Promise.reject(new Error('Retry the original uncertain editor inspection explicitly')); }
    try { this.inspectWire = this.makeInspect(expectedRevision); } catch (error) {
      return Promise.reject(error instanceof Error ? error : new Error(String(error)));
    }
    return this.issue(this.inspectWire, true, this.unknown);
  }
  retryInspect(): Promise<EditorBusinessCommit | undefined> {
    if (this.work) { return this.workKind === 'inspect' ? this.work : Promise.reject(new Error('Editor save is already in flight')); }
    if (!this.inspectUnknown) { return Promise.reject(new Error('No uncertain original editor inspection to retry')); }
    return this.issue(this.inspectWire, true, this.unknown);
  }
  private makeInspect(expectedRevision: string): string {
    if (!positive(expectedRevision) || expectedRevision !== this.originalRevision) { throw new Error('Editor original receipt revision differs'); }
    const command = new InspectCommand(); command.editor_commit.request_json = this.request;
    command.editor_commit.expected_revision = expectedRevision; const wire = JSON.stringify(command); budget(wire); return wire;
  }

  // This creates only a new business proposal. A durable business-draft source
  // handoff is a separate native contract; a kind1 raw fork cannot enter here.
  async continueTodos(business: EditorBusinessFields, publication: DraftRecord,
    hooks: EditorBusinessHooks): Promise<EditorBusinessCoordinator> {
    const previous = this.receipt;
    if (this.work || this.inspectUnknown || !this.qualified || !previous?.historical_card || !previous.live_matches || !this.hooks.isCurrent() ||
      this.frozen.mode === 'edit' || business.id !== previous.card_id || business.operation === previous.operation ||
      business.source !== previous.historical_card.source) { throw new Error('Editor continuation needs its own exact live historical baseline'); }
    const value = new EditorBusinessSubmission(); value.mode = 'continued_todos';
    value.business = JSON.parse(JSON.stringify(business)) as EditorBusinessFields;
    value.publication.draft_id = publication.scope.draft_id; value.publication.generation = publication.generation;
    value.publication.save_operation = publication.operation_id; value.publication.request_sha256 = publication.request_sha256;
    value.continuation = new EditorBusinessContinuation();
    value.continuation.root_request_json = this.frozen.mode === 'create' ? this.request : this.frozen.continuation!.root_request_json;
    const baseline = value.continuation.baseline; baseline.operation = previous.operation; baseline.revision = previous.revision;
    baseline.command_sha256 = previous.command_sha256; baseline.content_sha256 = previous.content_sha256;
    baseline.request_sha256 = previous.request_sha256; baseline.publication_sha256 = previous.publication_sha256;
    const model = await EditorBusinessCoordinator.prepareSubmission(value, publication, hooks);
    if (!this.hooks.isCurrent()) { throw new Error('Editor continuation baseline owner changed'); } return model;
  }

  private issue(wire: string, inspection: boolean, wasUnknown: boolean): Promise<EditorBusinessCommit | undefined> {
    if (!inspection) { this.phase = 'pending'; }
    this.workKind = inspection ? 'inspect' : 'save';
    this.lastError = ''; this.absence = false;
    this.work = this.perform(wire, inspection, wasUnknown).finally(() => { this.work = undefined; this.workKind = ''; this.hooks.changed(); });
    this.hooks.changed(); return this.work;
  }
  private async validateCommit(commit: EditorBusinessCommit, inspection: boolean): Promise<boolean> {
    const field = this.frozen.business;
    if (!commit || commit.card_id !== field.id || commit.operation !== field.operation ||
      !samePublication(commit.publication, this.frozen.publication) || typeof commit.live_matches !== 'boolean') {
      throw new Error('Editor receipt identity/publication differs');
    }
    if (commit.commit_status === 'absent') {
      if (!inspection || commit.qualification !== 'absent' || commit.historical_card !== null || commit.live_matches ||
        [commit.source_revision, commit.revision, commit.event_id, commit.command_sha256, commit.content_sha256,
          commit.request_sha256, commit.publication_sha256, commit.live_revision].some((value: string): boolean => value !== '')) {
        throw new Error('Invalid editor snapshot absence');
      }
      return false;
    }
    if (commit.commit_status !== 'committed' || !['development_editor_wire_v1', 'legacy_semantic_only'].includes(commit.qualification) ||
      !inspection && commit.qualification !== 'development_editor_wire_v1' || commit.source_revision !== this.raw.scope.source_revision ||
      commit.revision !== this.originalRevision || !digest(commit.command_sha256) || !digest(commit.content_sha256) ||
      typeof commit.event_id !== 'string' || !commit.event_id || !(commit.live_revision === '' || positive(commit.live_revision)) ||
      commit.live_matches && commit.live_revision !== commit.revision || !commit.historical_card) {
      throw new Error('Editor historical receipt proof differs');
    }
    validateCard(commit.historical_card, field, commit.revision, this.raw, commit.qualification === 'development_editor_wire_v1');
    if (await this.hash(bytes(commit.historical_card.source)) !== commit.content_sha256) { throw new Error('Editor complete historical source digest differs'); }
    if (commit.qualification === 'development_editor_wire_v1' ? commit.request_sha256 !== this.requestHash ||
      commit.publication_sha256 !== this.publicationHash : commit.request_sha256 !== '' || commit.publication_sha256 !== '') {
      throw new Error('Editor exact original wire/publication digest differs');
    }
    if (this.receipt && (this.receipt.qualification !== commit.qualification || this.receipt.command_sha256 !== commit.command_sha256 ||
      this.receipt.content_sha256 !== commit.content_sha256 || this.receipt.event_id !== commit.event_id ||
      this.receipt.historical_card?.source !== commit.historical_card.source)) { throw new Error('Editor immutable historical receipt changed'); }
    return true;
  }
  private async perform(wire: string, inspection: boolean, wasUnknown: boolean): Promise<EditorBusinessCommit | undefined> {
    try {
      const received = await this.hooks.send(wire), encoded = JSON.stringify(received); budget(encoded);
      // Freeze the full DTO before asynchronous digest checks. Shared callers
      // cannot replace an identity/source between validation and consumption.
      const reply = JSON.parse(encoded) as EditorBusinessReply;
      if (!reply || reply.ok !== true) {
        if (!inspection && !wasUnknown && reply?.ok === false && reply.effect === 'not_committed') { this.phase = 'rejected'; }
        throw new Error(reply?.error || 'Editor complete reply unavailable');
      }
      if (reply.error !== '') { throw new Error('Editor success DTO contains an error'); }
      const commit = reply.editor_commit;
      if (!commit || (commit.commit_status === 'committed' ? reply.effect !== 'committed' || reply.receipt_revision !== commit.revision :
        !inspection || reply.effect !== 'not_committed' || reply.receipt_revision !== '')) {
        throw new Error('Editor business result is incomplete');
      }
      const found = await this.validateCommit(commit, inspection);
      if (!found) {
        if (this.receipt) { throw new Error('Editor known historical commitment cannot become absent'); }
        this.absence = true; this.inspectUnknown = false; return undefined;
      }
      this.receipt = copyCommit(commit); this.phase = 'committed'; this.inspectUnknown = false; this.lastError = '';
      return copyCommit(commit);
    } catch (error) {
      if (inspection) { this.inspectUnknown = true; }
      else if (!this.receipt && this.phase !== 'rejected') { this.phase = 'unknown'; }
      this.lastError = String(error); if (error instanceof Error) { throw error; } throw new Error(String(error));
    }
  }
}
