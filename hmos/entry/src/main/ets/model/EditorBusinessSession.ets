import { cryptoFramework } from '@kit.CryptoArchitectureKit';
import { util } from '@kit.ArkTS';
import { DraftBusinessProof, DraftRecord, Values, copyRecord, nextGeneration, validGeneration, validateBusinessProof, validateRecord } from './EditorDraft';
import { EditorFieldPolicy } from './EditorFieldPolicy';
import { EditorBusinessCoordinator, EditorBusinessFields, EditorBusinessCommit, EditorBusinessHooks,
  EditorBusinessReply } from './EditorBusiness';

// Development intent journal only. An issued intent registers transport bytes;
// it is neither a business receipt nor protected producer/handoff authority.
// The caller owns pausing its parent queue and freezes owner + input epoch in
// the hooks. This module never reads current cards or rebases a draft source.
const LIMIT: number = 512 * 1024;
export class EditorIntentProof {
  intent_id: string = ''; prepare_operation: string = ''; generation: string = '1';
  prepared_record_sha256: string = ''; request_sha256: string = '';
}
export interface EditorIntentView {
  proof: EditorIntentProof; card_id: string; business_operation: string; expected_revision: string;
  phase: string; current_generation: string; current_active: boolean; repeated: boolean; part: string;
  request_json: string; publication: DraftRecord | null; save_request_json: string; inspect_request_json: string;
  close_request_json: string; close_disposition: string; issue_operation: string;
  handoff_request_json?: string; retirement_request_json?: string;
}
export interface EditorBusinessSessionReply extends EditorBusinessReply {
  editor_intents?: EditorIntentView[]; intent_next_after?: string;
}
export interface EditorBusinessSessionHooks {
  send: (serialized: string) => Promise<EditorBusinessSessionReply>;
  fields: EditorFieldPolicy; changed: () => void; isCurrent: () => boolean; isExact: () => boolean;
  // Caller has paused writes for this exact parent/publication, with no write,
  // Unknown or conflict in flight. This is a queue gate, not storage authority.
  parentReady: () => boolean;
  hash?: (bytes: Uint8Array) => Promise<string>;
}
export interface EditorBusinessSessionOwnerHooks {
  changed: () => void; isCurrent: () => boolean; isExact: () => boolean; parentReady: () => boolean;
}
export interface EditorIntentPage { intents: EditorIntentView[]; nextAfter: string; }
class PrepareBody { operation_id: string = ''; request_json: string = ''; }
class PrepareCommand { action: string = 'editor_intent_prepare'; editor_intent: PrepareBody = new PrepareBody(); }
class IssueBody { intent: EditorIntentProof = new EditorIntentProof(); expected_generation: string = '1'; }
class IssueCommand { action: string = 'editor_intent_issue'; editor_intent_issue: IssueBody = new IssueBody(); }
class ReadBody { intent_id: string = ''; prepare_operation: string = ''; part: string = ''; }
class ReadCommand { action: string = 'editor_intent_read'; editor_intent_ref: ReadBody = new ReadBody(); }
class ListBody { after: string = ''; limit: number = 16; card_id: string = ''; }
class ListCommand { action: string = 'editor_intent_list'; editor_intent_query: ListBody = new ListBody(); }
class CloseBody {
  schema_version: number = 1; intent: EditorIntentProof = new EditorIntentProof(); expected_generation: string = '1';
  operation_id: string = ''; disposition: string = 'cancel_prepared';
}
class ExtendedCloseParent { proof: DraftBusinessProof = new DraftBusinessProof(); discard_operation: string = ''; }
class ExtendedCloseBody extends CloseBody { parent: ExtendedCloseParent | null = null; plan_operation: string = ''; }
class CloseWire { request_json: string = ''; }
class CloseCommand { action: string = 'editor_intent_close'; editor_intent_close: CloseWire = new CloseWire(); }
class RegisteredSaveBody { request_json: string = ''; intent: EditorIntentProof = new EditorIntentProof(); }
class RegisteredSave { action: string = ''; editor_save: RegisteredSaveBody = new RegisteredSaveBody(); }
class RegisteredInspectBody { request_json: string = ''; expected_revision: string = ''; }
class RegisteredInspect { action: string = ''; editor_commit: RegisteredInspectBody = new RegisteredInspectBody(); }

function scalar(value: string): void {
  if (typeof value !== 'string') { throw new Error('Invalid intent string'); }
  for (let i: number = 0; i < value.length; i++) {
    const code = value.charCodeAt(i);
    if (code >= 0xD800 && code <= 0xDBFF) {
      const low = value.charCodeAt(++i); if (!(low >= 0xDC00 && low <= 0xDFFF)) { throw new Error('Incomplete intent Unicode'); }
    } else if (code >= 0xDC00 && code <= 0xDFFF) { throw new Error('Incomplete intent Unicode'); }
  }
}
function encoded(value: string): Uint8Array { scalar(value); return new util.TextEncoder().encodeInto(value); }
function budget(value: string): void { if (encoded(value).length > LIMIT) { throw new Error('Complete intent JSON bytes limit'); } }
function id(value: string): void {
  scalar(value); if (!value || encoded(value).length > 256 || /[\u0000-\u001f\u007f-\u009f/\\:]/.test(value)) {
    throw new Error('Invalid intent identity');
  }
}
function hashFormat(value: string): boolean { return typeof value === 'string' && /^[0-9a-f]{64}$/.test(value); }
function exactKeys(value: Object, keys: string[]): void {
  if (!value || typeof value !== 'object' || Array.isArray(value) ||
    Object.keys(value).sort().join('|') !== keys.slice().sort().join('|')) { throw new Error('Incomplete intent envelope'); }
}
function validateProof(proof: EditorIntentProof): void {
  exactKeys(proof, ['intent_id', 'prepare_operation', 'generation', 'prepared_record_sha256', 'request_sha256']);
  id(proof.intent_id); id(proof.prepare_operation);
  if (!/^morrow-host-editor-intent-[0-9a-f]{64}$/.test(proof.intent_id) || proof.generation !== '1' ||
    !hashFormat(proof.prepared_record_sha256) || !hashFormat(proof.request_sha256)) {
    throw new Error('Invalid immutable intent proof');
  }
}
function proofCopy(value: EditorIntentProof): EditorIntentProof { return JSON.parse(JSON.stringify(value)) as EditorIntentProof; }
function proofEqual(a: EditorIntentProof, b: EditorIntentProof): boolean {
  return a.intent_id === b.intent_id && a.prepare_operation === b.prepare_operation && a.generation === b.generation &&
    a.prepared_record_sha256 === b.prepared_record_sha256 && a.request_sha256 === b.request_sha256;
}
function viewCopy(value: EditorIntentView): EditorIntentView { return JSON.parse(JSON.stringify(value)) as EditorIntentView; }
function validateView(view: EditorIntentView, part: string): void {
  if (!view || !view.proof) { throw new Error('Missing complete intent view'); }
  const keys = ['proof', 'card_id', 'business_operation', 'expected_revision', 'phase', 'current_generation', 'current_active',
    'repeated', 'part', 'request_json', 'publication', 'save_request_json', 'inspect_request_json', 'close_request_json',
    'close_disposition', 'issue_operation'];
  if (view.handoff_request_json !== undefined || view.retirement_request_json !== undefined) {
    keys.push('handoff_request_json', 'retirement_request_json');
    scalar(view.handoff_request_json!); scalar(view.retirement_request_json!);
  }
  exactKeys(view, keys);
  validateProof(view.proof); id(view.card_id); id(view.business_operation);
  if (!validGeneration(view.expected_revision) || view.expected_revision === '0') { throw new Error('Invalid expected business revision'); }
  if (!['prepared', 'issued', 'handoff_planned', 'close_planned', 'closed'].includes(view.phase) || view.part !== part || typeof view.current_active !== 'boolean' ||
    typeof view.repeated !== 'boolean' || !validGeneration(view.current_generation) ||
    (view.phase === 'prepared' ? view.current_generation !== '1' : view.phase === 'issued' ? view.current_generation !== '2' :
      view.phase === 'handoff_planned' ? view.current_generation !== '3' : view.phase === 'close_planned' ?
        !['3', '4'].includes(view.current_generation) : !['2', '4', '5'].includes(view.current_generation)) ||
    view.current_active !== (view.phase !== 'closed')) { throw new Error('Intent phase/current generation differs'); }
  for (const text of [view.request_json, view.save_request_json, view.inspect_request_json, view.close_request_json,
    view.close_disposition, view.issue_operation, view.handoff_request_json ?? '', view.retirement_request_json ?? '']) { scalar(text); }
  if (view.request_json !== '' && part !== 'submission' || view.publication !== null && part !== 'publication' ||
    view.save_request_json !== '' && part !== 'save' || view.inspect_request_json !== '' && part !== 'inspect' ||
    view.close_request_json !== '' && part !== 'close' || (view.handoff_request_json ?? '') !== '' && part !== 'handoff' ||
    (view.retirement_request_json ?? '') !== '' && part !== 'retirement') { throw new Error('Intent reply mixes payload parts'); }
  // This domain-derived identity is present even before issue or after cancel.
  id(view.issue_operation);
  if (view.phase === 'closed' ? !['cancel_prepared', 'saved_exact', 'handoff_retired'].includes(view.close_disposition) ||
    view.current_generation !== (view.close_disposition === 'cancel_prepared' ? '2' : view.close_disposition === 'saved_exact' ? '4' : '5') :
    view.phase === 'close_planned' ? !['saved_exact', 'handoff_retired'].includes(view.close_disposition) ||
      view.current_generation !== (view.close_disposition === 'saved_exact' ? '3' : '4') : view.close_disposition !== '') {
    throw new Error('Unsupported intent close disposition');
  }
}
function fullReply(reply: EditorBusinessSessionReply): EditorBusinessSessionReply {
  const wire = JSON.stringify(reply); budget(wire); return JSON.parse(wire) as EditorBusinessSessionReply;
}
function single(reply: EditorBusinessSessionReply, part: string): EditorIntentView {
  if (reply.ok !== true || reply.error !== '' || !Array.isArray(reply.editor_intents) || reply.editor_intents.length !== 1 || reply.intent_next_after !== '') {
    throw new Error(reply.error || 'Incomplete intent acknowledgement');
  }
  const value = reply.editor_intents[0]; validateView(value, part); return viewCopy(value);
}
function readWire(proof: EditorIntentProof, part: string): string {
  if (!['submission', 'publication', 'save', 'inspect', 'close', 'handoff', 'retirement'].includes(part)) { throw new Error('Unsupported intent part'); }
  const request = new ReadCommand(); request.editor_intent_ref.intent_id = proof.intent_id;
  request.editor_intent_ref.prepare_operation = proof.prepare_operation; request.editor_intent_ref.part = part;
  const wire = JSON.stringify(request); budget(wire); return wire;
}
function immutablePublication(record: DraftRecord): string {
  validateRecord(record, true); const copy = copyRecord(record);
  // Current state is a diagnostic, not authority to reject an immutable save.
  copy.current_generation = copy.generation; copy.current_active = copy.active; copy.repeated = false;
  return JSON.stringify(copy);
}
function littleEndian(value: string): Uint8Array {
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
  if (remaining !== '0') { throw new Error('Intent length overflow'); } return result;
}
function identityBytes(domain: string, first: string, second: string): Uint8Array {
  const result: number[] = [];
  for (const byte of encoded(domain)) { result.push(byte); }
  for (const value of [first, second]) {
    const bytes = encoded(value); for (const byte of littleEndian(bytes.length.toString())) { result.push(byte); }
    for (const byte of bytes) { result.push(byte); }
  }
  return new Uint8Array(result);
}
async function platformHash(bytes: Uint8Array): Promise<string> {
  const md = cryptoFramework.createMd('SHA256'); await md.update({ data: bytes }); const result = await md.digest(); let hex = '';
  for (const byte of result.data) { hex += byte.toString(16).padStart(2, '0'); } return hex;
}
function failure(error: Object): Error { return error instanceof Error ? error : new Error(String(error)); }

export function validateEditorIntentProof(proof: EditorIntentProof): void { validateProof(proof); }
export function sameEditorIntentProof(a: EditorIntentProof, b: EditorIntentProof): boolean { return proofEqual(a, b); }
export function copyEditorIntentProof(proof: EditorIntentProof): EditorIntentProof { return proofCopy(proof); }
export function validateEditorIntentView(view: EditorIntentView, part: string): void { validateView(view, part); }
export function editorIntentReadWire(proof: EditorIntentProof, part: string): string { validateProof(proof); return readWire(proof, part); }
function generationBefore(a: string, b: string): boolean { return a.length < b.length || a.length === b.length && a < b; }

export class EditorBusinessSessionCoordinator {
  private hooks: EditorBusinessSessionHooks;
  private model: EditorBusinessCoordinator | undefined = undefined;
  private prepareOperation: string;
  private prepareWire: string = ''; private issueWire: string = ''; private cancelWire: string = ''; private cancelLiteral: string = '';
  private requestHash: string = ''; private intentIdentity: string = ''; private issueIdentity: string = '';
  private acknowledged: EditorIntentView | undefined = undefined;
  private registeredSave: string = ''; private registeredInspect: string = ''; private mappedInspect: string = '';
  private nativeUnknown: string = ''; private readUnknown: string = ''; private readUnknownPart: string = '';
  private inFlight: boolean = false; private committedHint: boolean = false; private lastError: string = '';

  private constructor(prepareOperation: string, hooks: EditorBusinessSessionHooks) {
    id(prepareOperation); this.prepareOperation = prepareOperation;
    this.hooks = { send: hooks.send, fields: hooks.fields, changed: hooks.changed, isCurrent: hooks.isCurrent,
      isExact: hooks.isExact, parentReady: hooks.parentReady, hash: hooks.hash };
  }
  private businessHooks(): EditorBusinessHooks {
    return { fields: this.hooks.fields, changed: (): void => this.hooks.changed(), isCurrent: (): boolean => this.hooks.isCurrent(),
      isExact: (): boolean => this.hooks.isExact(), hash: this.hooks.hash, send: (wire: string): Promise<EditorBusinessReply> => this.sendBusiness(wire) };
  }
  rebindOwner(hooks: EditorBusinessSessionOwnerHooks): void {
    this.idle();
    // Keep the original sender, field protocol and hash provider. A new view
    // may only supply its explicit guards, without resetting durable facts.
    this.hooks.changed = hooks.changed; this.hooks.isCurrent = hooks.isCurrent;
    this.hooks.isExact = hooks.isExact; this.hooks.parentReady = hooks.parentReady; this.hooks.changed();
  }
  static async prepare(mode: string, business: EditorBusinessFields, publication: DraftRecord, prepareOperation: string,
    hooks: EditorBusinessSessionHooks): Promise<EditorBusinessSessionCoordinator> {
    const session = new EditorBusinessSessionCoordinator(prepareOperation, hooks);
    session.owned();
    session.model = await EditorBusinessCoordinator.prepare(mode, business, publication, session.businessHooks());
    await session.initialize(); session.owned();
    return session;
  }
  async continueTodos(business: EditorBusinessFields, publication: DraftRecord, prepareOperation: string,
    hooks: EditorBusinessSessionHooks): Promise<EditorBusinessSessionCoordinator> {
    this.idle(); const session = new EditorBusinessSessionCoordinator(prepareOperation, hooks);
    session.owned();
    session.model = await this.business().continueTodos(business, publication, session.businessHooks());
    await session.initialize(); session.owned();
    return session;
  }
  private async initialize(): Promise<void> {
    const model = this.business();
    if (this.prepareOperation === model.submission.business.operation || this.prepareOperation === model.publication.operation_id) {
      throw new Error('Intent operation identities must be independent');
    }
    this.requestHash = await this.hash(encoded(model.originalRequest));
    this.intentIdentity = 'morrow-host-editor-intent-' + await this.hash(identityBytes('morrow.hmos.editor-intent.v1\u0000',
      model.submission.business.id, model.submission.business.operation));
    this.issueIdentity = 'morrow-host-intent-issue-' + await this.hash(identityBytes('morrow.hmos.editor-intent-issue.v1\u0000',
      this.intentIdentity, this.prepareOperation));
    const request = new PrepareCommand(); request.editor_intent.operation_id = this.prepareOperation;
    request.editor_intent.request_json = model.originalRequest; this.prepareWire = JSON.stringify(request); budget(this.prepareWire);
  }
  private async hash(bytes: Uint8Array): Promise<string> {
    const result = await (this.hooks.hash ? this.hooks.hash(bytes) : platformHash(bytes));
    if (!hashFormat(result)) { throw new Error('Intent SHA256 unavailable'); } return result;
  }
  private business(): EditorBusinessCoordinator { if (!this.model) { throw new Error('Intent business has not restored'); } return this.model; }
  private idle(): void { if (this.inFlight || this.model?.saving) { throw new Error('Intent request is already in flight'); } }
  private owned(): void {
    if (!this.hooks.isCurrent() || !this.hooks.isExact()) { throw new Error('Intent owner/input epoch changed'); }
    if (!this.hooks.parentReady()) { throw new Error('Exact parent write queue is not paused/confirmed'); }
  }
  get originalRequest(): string { return this.business().originalRequest; }
  get originalPrepare(): string { return this.prepareWire; }
  get originalIssue(): string { return this.issueWire; }
  get originalCancel(): string { return this.cancelWire; }
  get originalSave(): string { return this.registeredSave; }
  get originalInspect(): string { return this.registeredInspect; }
  get proof(): EditorIntentProof | undefined { return this.acknowledged ? proofCopy(this.acknowledged.proof) : undefined; }
  get view(): EditorIntentView | undefined { return this.acknowledged ? viewCopy(this.acknowledged) : undefined; }
  get phase(): string { return this.acknowledged?.phase ?? 'unprepared'; }
  get pendingNative(): string { return this.nativeUnknown; }
  get pendingRead(): string { return this.readUnknown; }
  get saving(): boolean { return this.inFlight || this.business().saving; }
  get unknown(): boolean { return !!this.nativeUnknown || !!this.readUnknown || this.business().unknown || this.business().checkingUnknown; }
  get businessUnknown(): boolean { return this.business().unknown; }
  get inspectionUnknown(): boolean { return this.business().checkingUnknown; }
  get businessRejected(): boolean { return this.business().rejected; }
  get snapshotAbsent(): boolean { return this.business().snapshotAbsent; }
  get transportReady(): boolean {
    return this.phase === 'issued' && !this.nativeUnknown && !this.readUnknown && !!this.registeredSave && !!this.registeredInspect;
  }
  get committed(): boolean { return this.committedHint || this.business().committed; }
  get qualified(): boolean { return this.business().qualified; }
  get confirmed(): EditorBusinessCommit | undefined { return this.business().confirmed; }
  get publication(): DraftRecord { return this.business().publication; }
  get error(): string { return this.lastError || this.business().error; }
  mayConsume(values: Values): boolean { return this.business().mayConsume(values); }

  prepareIntent(): Promise<EditorIntentView> {
    this.idle(); this.owned(); if (this.acknowledged || this.nativeUnknown) { return Promise.reject(new Error('Retry original intent prepare explicitly')); }
    return this.mutate(this.prepareWire, 'prepare', false);
  }
  retryPrepare(): Promise<EditorIntentView> {
    this.idle(); if (this.nativeUnknown !== 'prepare') { return Promise.reject(new Error('No uncertain intent prepare')); }
    return this.mutate(this.prepareWire, 'prepare', true);
  }
  issueIntent(): Promise<EditorIntentView> {
    this.idle(); this.owned(); if (this.phase !== 'prepared' || this.nativeUnknown || this.readUnknown) {
      return Promise.reject(new Error('Intent issue needs confirmed prepared state'));
    }
    this.freezeIssue(); return this.mutate(this.issueWire, 'issue', false);
  }
  retryIssue(): Promise<EditorIntentView> {
    this.idle(); if (this.nativeUnknown !== 'issue' || !this.issueWire) { return Promise.reject(new Error('No uncertain intent issue')); }
    return this.mutate(this.issueWire, 'issue', true);
  }
  private freezeIssue(): void {
    if (!this.acknowledged) { throw new Error('Missing intent proof'); }
    const command = new IssueCommand(); command.editor_intent_issue.intent = proofCopy(this.acknowledged.proof);
    const wire = JSON.stringify(command); budget(wire);
    if (this.issueWire && this.issueWire !== wire) { throw new Error('Original intent issue changed'); } this.issueWire = wire;
  }
  private bind(view: EditorIntentView): void {
    const model = this.business();
    if (view.proof.intent_id !== this.intentIdentity || view.proof.prepare_operation !== this.prepareOperation || view.proof.request_sha256 !== this.requestHash ||
      view.card_id !== model.submission.business.id || view.business_operation !== model.submission.business.operation ||
      view.expected_revision !== nextGeneration(model.publication.scope.source_revision) ||
      view.issue_operation !== this.issueIdentity ||
      this.acknowledged && !proofEqual(view.proof, this.acknowledged.proof)) { throw new Error('Intent original identity differs'); }
    if (this.acknowledged && (generationBefore(view.current_generation, this.acknowledged.current_generation) ||
      view.current_generation === this.acknowledged.current_generation && view.phase !== this.acknowledged.phase ||
      this.acknowledged.phase === 'closed' && view.phase !== 'closed' ||
      this.acknowledged.close_disposition !== '' && view.close_disposition !== this.acknowledged.close_disposition)) {
      throw new Error('Intent known phase cannot be replaced');
    }
  }
  // Observe only a completely validated native metadata view after a handoff
  // step. This never changes the separate qualified business receipt.
  adoptIntentView(value: EditorIntentView): void {
    this.idle(); const view = viewCopy(value); validateView(view, 'summary'); this.bind(view);
    this.acknowledged = this.summary(view); this.hooks.changed();
  }
  private async mutate(wire: string, kind: string, wasUnknown: boolean): Promise<EditorIntentView> {
    this.inFlight = true; this.hooks.changed(); let noWrite = false;
    try {
      const reply = fullReply(await this.hooks.send(wire));
      if (!reply.ok && reply.effect === 'not_committed' && !wasUnknown) { noWrite = true; this.nativeUnknown = ''; throw new Error(reply.error || 'Intent rejected'); }
      const view = single(reply, 'summary'); this.bind(view);
      if (reply.effect !== 'committed' || reply.receipt_revision !== view.current_generation) { throw new Error('Intent metadata receipt differs'); }
      if (kind === 'issue' && view.phase !== 'issued' || kind === 'cancel' && view.phase !== 'closed') {
        throw new Error('Intent mutation acknowledgement phase differs');
      }
      this.acknowledged = view; this.nativeUnknown = ''; this.lastError = '';
      if (!this.issueWire) { this.freezeIssue(); } return viewCopy(view);
    } catch (error) {
      this.lastError = failure(error).message;
      // An explicit first no-write rejection is known. Lost/malformed receipts
      // and every uncertain retry retain the fixed original action.
      if (!noWrite) { this.nativeUnknown = kind; }
      throw failure(error);
    } finally { this.inFlight = false; this.hooks.changed(); }
  }
  async readPart(part: string): Promise<EditorIntentView> {
    this.idle(); if (!this.acknowledged || this.nativeUnknown || this.readUnknown) { throw new Error('Resolve original intent request before reading another part'); }
    return this.read(readWire(this.acknowledged.proof, part), part);
  }
  retryRead(): Promise<EditorIntentView> {
    this.idle(); if (!this.readUnknown) { return Promise.reject(new Error('No uncertain original intent read')); }
    return this.read(this.readUnknown, this.readUnknownPart);
  }
  private async read(wire: string, part: string): Promise<EditorIntentView> {
    this.inFlight = true; this.hooks.changed();
    try {
      const reply = fullReply(await this.hooks.send(wire)); const view = single(reply, part);
      if (reply.effect !== 'not_committed' || reply.receipt_revision !== '') { throw new Error('Intent read metadata receipt differs'); }
      this.bind(view); this.validatePart(view);
      this.acknowledged = this.summary(view); this.readUnknown = ''; this.readUnknownPart = ''; this.lastError = ''; return viewCopy(view);
    } catch (error) {
      this.readUnknown = wire; this.readUnknownPart = part; this.lastError = failure(error).message; throw failure(error);
    } finally { this.inFlight = false; this.hooks.changed(); }
  }
  private summary(view: EditorIntentView): EditorIntentView {
    const copy = viewCopy(view); copy.part = 'summary'; copy.request_json = ''; copy.publication = null;
    copy.save_request_json = ''; copy.inspect_request_json = ''; copy.close_request_json = '';
    if (copy.handoff_request_json !== undefined) { copy.handoff_request_json = ''; }
    if (copy.retirement_request_json !== undefined) { copy.retirement_request_json = ''; }
    return copy;
  }
  private validatePart(view: EditorIntentView): void {
    const model = this.business();
    if (view.part === 'submission' && view.request_json !== model.originalRequest) { throw new Error('Intent original Submission bytes differ'); }
    if (view.part === 'publication' && (!view.publication || immutablePublication(view.publication) !== immutablePublication(model.publication))) {
      throw new Error('Intent complete immutable publication differs');
    }
    if (view.part === 'save') {
      if (view.phase === 'prepared' || view.phase === 'closed' && view.close_disposition === 'cancel_prepared') {
        if (view.save_request_json !== '') { throw new Error('Unissued intent has save transport'); } return;
      }
      this.validateSave(view.save_request_json, view.proof);
      if (this.registeredSave && this.registeredSave !== view.save_request_json) { throw new Error('Registered save transport changed'); }
      this.registeredSave = view.save_request_json;
    }
    if (view.part === 'inspect') {
      if (view.phase === 'prepared' || view.phase === 'closed' && view.close_disposition === 'cancel_prepared') {
        if (view.inspect_request_json !== '') { throw new Error('Unissued intent has inspect transport'); } return;
      }
      this.validateInspect(view.inspect_request_json);
      if (this.registeredInspect && this.registeredInspect !== view.inspect_request_json) { throw new Error('Registered inspect transport changed'); }
      this.registeredInspect = view.inspect_request_json;
    }
    if (view.part === 'close') {
      if (view.phase !== 'closed' && view.phase !== 'close_planned') {
        if (view.close_request_json !== '') { throw new Error('Open intent has close transport'); } return;
      }
      budget(view.close_request_json); const close = JSON.parse(view.close_request_json) as CloseBody;
      validateProof(close.intent); id(close.operation_id);
      if (close.schema_version !== 1 || !proofEqual(close.intent, view.proof) || close.disposition !== view.close_disposition) {
        throw new Error('Original intent close differs');
      }
      if ([view.proof.prepare_operation, view.business_operation, model.publication.operation_id, view.issue_operation].includes(close.operation_id)) {
        throw new Error('Original close operation is not independent');
      }
      if (close.disposition === 'cancel_prepared') {
        exactKeys(close, ['schema_version', 'intent', 'expected_generation', 'operation_id', 'disposition']);
        if (close.expected_generation !== '1' || this.cancelLiteral && this.cancelLiteral !== view.close_request_json || this.committed) {
          throw new Error('Original intent cancellation differs');
        }
        this.cancelLiteral = view.close_request_json;
        const command = new CloseCommand(); command.editor_intent_close.request_json = this.cancelLiteral;
        this.cancelWire = JSON.stringify(command); budget(this.cancelWire);
      } else {
        exactKeys(close, ['schema_version', 'intent', 'expected_generation', 'operation_id', 'disposition', 'parent', 'plan_operation']);
        if (close.expected_generation !== (close.disposition === 'saved_exact' ? '2' : '3')) { throw new Error('Original close generation differs'); }
        const extended = close as ExtendedCloseBody; scalar(extended.plan_operation);
        if (close.disposition === 'saved_exact') {
          if (!extended.parent || extended.plan_operation !== '') { throw new Error('Saved-exact close lacks its exact parent'); }
          exactKeys(extended.parent, ['proof', 'discard_operation']); validateBusinessProof(extended.parent.proof); id(extended.parent.discard_operation);
          const original = model.submission.publication, parent = extended.parent.proof;
          if (parent.draft_id !== original.draft_id || parent.generation !== original.generation || parent.save_operation !== original.save_operation ||
            parent.request_sha256 !== original.request_sha256 || extended.parent.discard_operation === close.operation_id ||
            [parent.save_operation, view.proof.prepare_operation, view.business_operation, view.issue_operation].includes(extended.parent.discard_operation)) {
            throw new Error('Saved-exact close parent/publication differs');
          }
        } else if (extended.parent !== null || !extended.plan_operation) { throw new Error('Retired handoff close parent/plan differs'); }
        else {
          id(extended.plan_operation);
          if ([close.operation_id, view.proof.prepare_operation, view.business_operation, model.publication.operation_id,
            view.issue_operation].includes(extended.plan_operation)) { throw new Error('Original close plan operation differs'); }
        }
      }
    }
  }
  private validateSave(wire: string, proof: EditorIntentProof): void {
    budget(wire); const command = JSON.parse(wire) as RegisteredSave;
    exactKeys(command, ['action', 'editor_save']); exactKeys(command.editor_save, ['request_json', 'intent']); validateProof(command.editor_save.intent);
    if (command.action !== 'editor_save' || command.editor_save.request_json !== this.originalRequest || !proofEqual(command.editor_save.intent, proof)) {
      throw new Error('Registered original save envelope differs');
    }
  }
  private validateInspect(wire: string): void {
    budget(wire); const command = JSON.parse(wire) as RegisteredInspect;
    exactKeys(command, ['action', 'editor_commit']); exactKeys(command.editor_commit, ['request_json', 'expected_revision']);
    if (command.action !== 'editor_commit_inspect' || command.editor_commit.request_json !== this.originalRequest ||
      command.editor_commit.expected_revision !== nextGeneration(this.publication.scope.source_revision)) {
      throw new Error('Registered original inspect envelope differs');
    }
  }
  async loadTransport(): Promise<void> {
    if (this.phase === 'prepared' || this.phase === 'unprepared' ||
      this.phase === 'closed' && this.view?.close_disposition === 'cancel_prepared') { throw new Error('Only an issued intent registers business transport'); }
    await this.readPart('save'); await this.readPart('inspect');
  }
  private ready(): void {
    this.idle(); if (this.phase === 'prepared' || this.phase === 'unprepared' ||
      this.phase === 'closed' && this.view?.close_disposition === 'cancel_prepared' ||
      this.nativeUnknown || this.readUnknown || !this.registeredSave || !this.registeredInspect) {
      throw new Error('Complete issued native transport has not been read');
    }
  }
  save(): Promise<EditorBusinessCommit | undefined> {
    this.ready(); this.owned(); if (this.phase !== 'issued') { throw new Error('New business write needs the active issued intent'); }
    return this.business().save();
  }
  retrySave(): Promise<EditorBusinessCommit | undefined> {
    this.ready(); if (this.phase !== 'issued') { throw new Error('Closed/planned intent permits only historical inspection'); }
    return this.business().retrySave();
  }
  inspect(): Promise<EditorBusinessCommit | undefined> { this.ready(); return this.business().inspect(nextGeneration(this.publication.scope.source_revision)); }
  retryInspect(): Promise<EditorBusinessCommit | undefined> { this.ready(); return this.business().retryInspect(); }
  private async sendBusiness(wire: string): Promise<EditorBusinessReply> {
    if (this.phase === 'prepared' || this.phase === 'unprepared' ||
      this.phase === 'closed' && this.view?.close_disposition === 'cancel_prepared' ||
      !this.registeredSave || !this.registeredInspect || this.nativeUnknown || this.readUnknown) {
      throw new Error('Intent transport is not confirmed');
    }
    let registered = '';
    if (wire === this.business().originalSave) {
      if (this.phase !== 'issued') { throw new Error('Closed/planned intent permits only historical inspection'); }
      registered = this.registeredSave;
    }
    else {
      this.validateInspect(wire);
      if (this.mappedInspect && this.mappedInspect !== wire) { throw new Error('Original model inspection changed'); }
      this.mappedInspect = wire; registered = this.registeredInspect;
    }
    const received = await this.hooks.send(registered);
    // A native business committed effect is retained even if complete DTO
    // validation then fails. Only the existing model can qualify a receipt.
    if (received?.effect === 'committed') { this.committedHint = true; }
    return fullReply(received);
  }
  cancelPrepared(operation: string): Promise<EditorIntentView> {
    this.idle(); id(operation);
    if (this.phase !== 'prepared' || this.nativeUnknown || this.readUnknown || this.committed) {
      return Promise.reject(new Error('Only native prepared intent can be cancelled'));
    }
    if (operation === this.prepareOperation || operation === this.business().submission.business.operation ||
      operation === this.publication.operation_id || operation === this.issueIdentity) {
      return Promise.reject(new Error('Cancellation operation must be independent'));
    }
    const close = new CloseBody(); close.intent = proofCopy(this.acknowledged!.proof); close.operation_id = operation;
    const literal = JSON.stringify(close); budget(literal);
    if (this.cancelLiteral && this.cancelLiteral !== literal) { return Promise.reject(new Error('Original cancellation cannot change')); }
    this.cancelLiteral = literal; const command = new CloseCommand(); command.editor_intent_close.request_json = literal;
    this.cancelWire = JSON.stringify(command); budget(this.cancelWire); return this.mutate(this.cancelWire, 'cancel', false);
  }
  retryCancel(): Promise<EditorIntentView> {
    this.idle(); if (this.nativeUnknown !== 'cancel' || !this.cancelWire || this.committed) { return Promise.reject(new Error('No uncertain original intent cancellation')); }
    return this.mutate(this.cancelWire, 'cancel', true);
  }
  static async discover(after: string, limit: number, card: string, hooks: EditorBusinessSessionHooks): Promise<EditorIntentPage> {
    if (after) { id(after); } if (card) { id(card); }
    if (!Number.isInteger(limit) || limit < 1 || limit > 16) { throw new Error('Invalid intent page limit'); }
    const command = new ListCommand(); command.editor_intent_query.after = after; command.editor_intent_query.limit = limit;
    command.editor_intent_query.card_id = card; const wire = JSON.stringify(command); budget(wire);
    const reply = fullReply(await hooks.send(wire));
    if (reply.ok !== true || reply.error !== '' || reply.effect !== 'not_committed' || reply.receipt_revision !== '' || !Array.isArray(reply.editor_intents) ||
      reply.editor_intents.length > limit || typeof reply.intent_next_after !== 'string') {
      throw new Error(reply.error || 'Incomplete intent list');
    }
    const result: EditorIntentView[] = []; const seen: string[] = [];
    for (let index: number = 0; index < reply.editor_intents.length; index++) {
      const view = reply.editor_intents[index]; validateView(view, 'summary');
      if (seen.includes(view.proof.intent_id) || card && view.card_id !== card || view.proof.intent_id === after) { throw new Error('Intent page identities differ'); }
      const intentHash = await (hooks.hash ? hooks.hash(identityBytes('morrow.hmos.editor-intent.v1\u0000', view.card_id,
        view.business_operation)) : platformHash(identityBytes('morrow.hmos.editor-intent.v1\u0000', view.card_id, view.business_operation)));
      const issueHash = await (hooks.hash ? hooks.hash(identityBytes('morrow.hmos.editor-intent-issue.v1\u0000', view.proof.intent_id,
        view.proof.prepare_operation)) : platformHash(identityBytes('morrow.hmos.editor-intent-issue.v1\u0000', view.proof.intent_id, view.proof.prepare_operation)));
      if (!hashFormat(intentHash) || !hashFormat(issueHash) || view.proof.intent_id !== 'morrow-host-editor-intent-' + intentHash ||
        view.issue_operation !== 'morrow-host-intent-issue-' + issueHash) { throw new Error('Intent list domain identities differ'); }
      seen.push(view.proof.intent_id); result.push(viewCopy(view));
    }
    const cursor = reply.intent_next_after;
    if (cursor) { id(cursor); if (!result.length || cursor === after || cursor !== result[result.length - 1].proof.intent_id) { throw new Error('Intent page cursor differs'); } }
    return { intents: result, nextAfter: cursor };
  }
  static async restore(proof: EditorIntentProof, hooks: EditorBusinessSessionHooks): Promise<EditorBusinessSessionCoordinator> {
    const frozen = proofCopy(proof); validateProof(frozen);
    // Restoration performs only explicit reads. Failure leaves this immutable
    // ref available to the caller; retry restore(ref) never issues business.
    const parts: EditorIntentView[] = [];
    for (const part of ['submission', 'publication', 'save', 'inspect', 'close']) {
      const reply = fullReply(await hooks.send(readWire(frozen, part))); const view = single(reply, part);
      if (reply.effect !== 'not_committed' || reply.receipt_revision !== '') { throw new Error('Intent restore read metadata receipt differs'); }
      if (!proofEqual(view.proof, frozen) || parts.length && (view.card_id !== parts[0].card_id ||
        view.business_operation !== parts[0].business_operation || view.expected_revision !== parts[0].expected_revision ||
        view.phase !== parts[0].phase || view.current_generation !== parts[0].current_generation ||
        view.current_active !== parts[0].current_active || view.issue_operation !== parts[0].issue_operation)) {
        throw new Error('Intent restoration snapshot identities differ');
      }
      parts.push(view);
    }
    const session = new EditorBusinessSessionCoordinator(frozen.prepare_operation, hooks);
    if (!parts[1].publication) { throw new Error('Missing complete intent publication'); }
    session.model = await EditorBusinessCoordinator.restore(parts[0].request_json, parts[1].publication, session.businessHooks());
    await session.initialize(); session.acknowledged = session.summary(parts[0]);
    for (const view of parts) { session.bind(view); session.validatePart(view); }
    session.freezeIssue(); return session;
  }
}
