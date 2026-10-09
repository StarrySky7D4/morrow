import { cryptoFramework } from '@kit.CryptoArchitectureKit';
import { util } from '@kit.ArkTS';
import {
  AssetSelection, DraftBusinessLink, DraftBusinessProof, DraftRecord, DraftScope, EditorDraftCoordinator, TextValue, Values,
  copyAsset, copyBusinessLink, copyRecord, copyScope, copyText, copyValues, nextGeneration, sameBusinessLink,
  sameBusinessProof, sameValues, validGeneration, validateBusinessProof, validateRecord, validateValues
} from './EditorDraft';
import { EditorBusinessCommit } from './EditorBusiness';
import {
  EditorBusinessSessionCoordinator, EditorBusinessSessionReply, EditorIntentProof, EditorIntentView,
  copyEditorIntentProof, editorIntentReadWire, sameEditorIntentProof, validateEditorIntentProof, validateEditorIntentView
} from './EditorBusinessSession';

// Each native step has its own effect. Intent plan metadata is not a child or
// retirement receipt. Nothing here drains SDK input or installs a view owner.
const LIMIT: number = 512 * 1024;
export interface EditorBusinessHandoffReply extends EditorBusinessSessionReply { drafts?: DraftRecord[]; }
export interface EditorBusinessHandoffHooks {
  send: (wire: string) => Promise<EditorBusinessHandoffReply>;
  sendDraft: (wire: string) => Promise<DraftRecord>;
  changed: () => void; operation: () => string;
  isCurrent: () => boolean; isExact: () => boolean;
  importsReady: () => boolean;
  // The caller has actually installed this writer under its stable lease.
  childCurrent: (child: EditorDraftCoordinator) => boolean;
  hash?: (bytes: Uint8Array) => Promise<string>;
}
export class BusinessHandoffText {
  title: TextValue = new TextValue(); description: TextValue = new TextValue(); hypothesis: TextValue = new TextValue();
  conclusion: TextValue = new TextValue(); todos: TextValue = new TextValue(); category: string = ''; stage: string = '';
}
export class BusinessHandoffWrite {
  card_id: string = ''; draft_id: string = ''; source_kind: number = 0; source_revision: string = '';
  expected_generation: string = '0'; operation_id: string = ''; values: BusinessHandoffText = new BusinessHandoffText();
  assets: AssetSelection[] = [];
}
export class BusinessHandoffLiteral {
  schema_version: number = 1; intent: EditorIntentProof = new EditorIntentProof(); parent: DraftBusinessProof = new DraftBusinessProof();
  child: BusinessHandoffWrite = new BusinessHandoffWrite(); plan_operation: string = ''; retirement_operation: string = '';
}
export class BusinessRetirementLiteral {
  schema_version: number = 1; intent: EditorIntentProof = new EditorIntentProof(); plan_operation: string = '';
  handoff_request_sha256: string = ''; parent: DraftBusinessProof = new DraftBusinessProof(); child_draft_id: string = '';
  child_operation: string = ''; child_request_sha256: string = ''; operation_id: string = '';
}
class CloseParent { proof: DraftBusinessProof = new DraftBusinessProof(); discard_operation: string = ''; }
export class BusinessCloseLiteral {
  schema_version: number = 1; intent: EditorIntentProof = new EditorIntentProof(); expected_generation: string = '';
  operation_id: string = ''; disposition: string = ''; parent: CloseParent | null = null; plan_operation: string = '';
}
class LiteralBody { request_json: string = ''; }
class HandoffCommand { action: string = 'draft_continue_business'; business_handoff: LiteralBody = new LiteralBody(); }
class RetirementCommand { action: string = 'draft_continue_business_retire'; business_retirement: LiteralBody = new LiteralBody(); }
class CloseCommand { action: string = 'editor_intent_close'; editor_intent_close: LiteralBody = new LiteralBody(); }
class DraftHistoryCommand {
  action: string = 'draft_read_history'; id: string = ''; draft_id: string = ''; draft_operation: string = ''; generation: string = '';
}

function scalar(value: string): void {
  if (typeof value !== 'string') { throw new Error('Invalid complete business handoff string'); }
  for (let at: number = 0; at < value.length; at++) {
    const code = value.charCodeAt(at);
    if (code >= 0xD800 && code <= 0xDBFF) {
      const low = value.charCodeAt(++at); if (!(low >= 0xDC00 && low <= 0xDFFF)) { throw new Error('Incomplete business handoff Unicode'); }
    } else if (code >= 0xDC00 && code <= 0xDFFF) { throw new Error('Incomplete business handoff Unicode'); }
  }
}
function bytes(value: string): Uint8Array { scalar(value); return new util.TextEncoder().encodeInto(value); }
function budget(value: string): void { if (bytes(value).length > LIMIT) { throw new Error('Complete business handoff JSON bytes limit'); } }
function identity(value: string): void {
  scalar(value); if (!value || bytes(value).length > 256 || /[\u0000-\u001f\u007f-\u009f/\\:]/.test(value)) {
    throw new Error('Invalid business handoff identity');
  }
}
function digest(value: string): boolean { return typeof value === 'string' && /^[0-9a-f]{64}$/.test(value); }
function exactKeys(value: Object, keys: string[]): void {
  if (!value || typeof value !== 'object' || Array.isArray(value) ||
    Object.keys(value).sort().join('|') !== keys.slice().sort().join('|')) { throw new Error('Incomplete business handoff envelope'); }
}
function clone<T>(value: T): T { return JSON.parse(JSON.stringify(value)) as T; }
function sameScope(a: DraftScope, b: DraftScope): boolean {
  return a.card_id === b.card_id && a.draft_id === b.draft_id && a.source_kind === b.source_kind &&
    a.source_revision === b.source_revision && a.source === b.source;
}
function proof(record: DraftRecord): DraftBusinessProof {
  const result = new DraftBusinessProof(); result.draft_id = record.scope.draft_id; result.generation = record.generation;
  result.save_operation = record.operation_id; result.request_sha256 = record.request_sha256; validateBusinessProof(result); return result;
}
function textWire(values: Values): BusinessHandoffText {
  const value = new BusinessHandoffText(); value.title = copyText(values.title); value.description = copyText(values.description);
  value.hypothesis = copyText(values.hypothesis); value.conclusion = copyText(values.conclusion); value.todos = copyText(values.todos);
  value.category = values.category; value.stage = values.stage; return value;
}
function raw(write: BusinessHandoffWrite): Values {
  const value = new Values(); value.title = copyText(write.values.title); value.description = copyText(write.values.description);
  value.hypothesis = copyText(write.values.hypothesis); value.conclusion = copyText(write.values.conclusion); value.todos = copyText(write.values.todos);
  value.category = write.values.category; value.stage = write.values.stage;
  value.assets = write.assets.map((selected: AssetSelection): AssetSelection => copyAsset(selected)); return value;
}
function validateRaw(values: Values): void {
  exactKeys(values, ['title', 'description', 'hypothesis', 'conclusion', 'todos', 'category', 'stage', 'assets']);
  validateValues(values, true);
  for (const value of [values.title, values.description, values.hypothesis, values.conclusion, values.todos]) {
    exactKeys(value, ['text', 'selection_base', 'selection_extent', 'affinity', 'directional', 'composing_start', 'composing_end']); scalar(value.text);
  }
  scalar(values.category); scalar(values.stage);
  for (const selected of values.assets) {
    exactKeys(selected, ['origin', 'asset_id', 'aliases']); identity(selected.asset_id);
    for (const alias of selected.aliases) { scalar(alias); }
  }
}
function selectedRaw(input: Values, parent: DraftRecord): Values {
  validateRaw(input); const value = copyValues(input); let priorIndex = -1;
  for (const selected of value.assets) {
    const index = parent.assets.findIndex((pin): boolean => pin.selection.asset_id === selected.asset_id);
    const pin = index >= 0 ? parent.assets[index] : undefined;
    if (!pin || index <= priorIndex || selected.aliases.length !== pin.selection.aliases.length ||
      selected.aliases.some((alias: string, at: number): boolean => alias !== pin.selection.aliases[at])) {
      throw new Error('Business handoff selection/order/aliases lacks an exact confirmed parent pin');
    }
    priorIndex = index; selected.origin = 4;
  }
  return value;
}
function readSingle(reply: EditorBusinessHandoffReply, part: string): EditorIntentView {
  if (reply.ok !== true || reply.error !== '' || reply.effect !== 'not_committed' || reply.receipt_revision !== '' ||
    !Array.isArray(reply.editor_intents) || reply.editor_intents.length !== 1 || reply.intent_next_after !== '') {
    throw new Error(reply.error || 'Incomplete read-only business handoff intent part');
  }
  const view = reply.editor_intents[0]; validateEditorIntentView(view, part); return clone(view);
}
function summary(reply: EditorBusinessHandoffReply): EditorIntentView {
  if (reply.ok !== true || reply.error !== '' || !Array.isArray(reply.editor_intents) || reply.editor_intents.length !== 1 ||
    reply.intent_next_after !== undefined && reply.intent_next_after !== '') { throw new Error(reply.error || 'Incomplete business handoff intent acknowledgement'); }
  const value = reply.editor_intents[0]; validateEditorIntentView(value, 'summary'); return clone(value);
}
function handoffPhase(view: EditorIntentView): void {
  if (view.phase !== 'handoff_planned' && !(view.phase === 'close_planned' && view.close_disposition === 'handoff_retired') &&
    !(view.phase === 'closed' && view.close_disposition === 'handoff_retired')) { throw new Error('Business handoff metadata lineage differs'); }
}
function full(reply: EditorBusinessHandoffReply): EditorBusinessHandoffReply { const wire = JSON.stringify(reply); budget(wire); return JSON.parse(wire) as EditorBusinessHandoffReply; }
function errorValue(error: Object): Error { return error instanceof Error ? error : new Error(String(error)); }
async function platformHash(value: Uint8Array): Promise<string> {
  const md = cryptoFramework.createMd('SHA256'); await md.update({ data: value }); const valueHash = await md.digest(); let hex = '';
  for (const byte of valueHash.data) { hex += byte.toString(16).padStart(2, '0'); } return hex;
}

export class EditorBusinessHandoffCoordinator {
  private session: EditorBusinessSessionCoordinator; private parent: EditorDraftCoordinator | undefined;
  private hooks: EditorBusinessHandoffHooks; private parentRecord: DraftRecord | undefined;
  private committed: EditorBusinessCommit; private intent: EditorIntentProof;
  private proposal: BusinessHandoffLiteral | undefined; private frozenRaw: Values | undefined;
  private childScope: DraftScope | undefined; private handoffLiteral: string = ''; private handoffWire: string = '';
  private handoffHash: string = ''; private retirementLiteral: string = ''; private retirementWire: string = '';
  private closeLiteral: string = ''; private closeWire: string = ''; private closeMode: string = '';
  private firstRecord: DraftRecord | undefined; private currentChildRecord: DraftRecord | undefined;
  private retiredRecord: DraftRecord | undefined; private child: EditorDraftCoordinator | undefined;
  private firstEffect: boolean = false; private retirementEffect: boolean = false; private closeEffect: boolean = false;
  private closeConfirmed: boolean = false; private inFlight: boolean = false;
  private pendingKind: string = ''; private pendingReadWire: string = ''; private pendingReadPart: string = '';
  private parentHistoryWire: string = ''; private retirementHistoryWire: string = '';
  private pendingHistoryWire: string = ''; private pendingHistoryKind: string = '';
  private lastError: string = ''; private firstConflict: boolean = false;

  private constructor(session: EditorBusinessSessionCoordinator, parent: EditorDraftCoordinator | undefined, hooks: EditorBusinessHandoffHooks) {
    const commit = session.confirmed, intent = session.proof;
    if (session.saving || !session.qualified || !commit?.historical_card || !intent || !['issued', 'handoff_planned', 'close_planned', 'closed'].includes(session.phase)) {
      throw new Error('Business handoff requires its own strictly qualified historical result');
    }
    validateEditorIntentProof(intent); this.session = session; this.parent = parent; this.hooks = hooks;
    this.committed = clone(commit); this.intent = copyEditorIntentProof(intent);
    if (parent?.confirmed) { this.parentRecord = copyRecord(parent.confirmed); }
  }
  static async prepare(session: EditorBusinessSessionCoordinator, parent: EditorDraftCoordinator, childScope: DraftScope,
    firstRaw: Values, planOperation: string, firstOperation: string, retirementOperation: string,
    hooks: EditorBusinessHandoffHooks): Promise<EditorBusinessHandoffCoordinator> {
    const model = new EditorBusinessHandoffCoordinator(session, parent, hooks); model.requireParent(true);
    if (session.phase !== 'issued' || session.unknown || !sameValues(firstRaw, parent.current)) { throw new Error('Business handoff needs the latest complete owned raw'); }
    const record = model.parentRecord!, card = model.committed.historical_card!;
    if (childScope.card_id !== card.id || childScope.draft_id === record.scope.draft_id || childScope.source_kind !== 0 ||
      childScope.source_revision !== card.revision || childScope.source !== card.source) { throw new Error('Business child must use the exact historical result source'); }
    identity(childScope.draft_id); identity(planOperation); identity(firstOperation); identity(retirementOperation);
    model.independent([planOperation, firstOperation, retirementOperation]);
    model.frozenRaw = selectedRaw(firstRaw, record); model.childScope = copyScope(childScope);
    const proposal = new BusinessHandoffLiteral(); proposal.intent = copyEditorIntentProof(model.intent); proposal.parent = proof(record);
    proposal.plan_operation = planOperation; proposal.retirement_operation = retirementOperation;
    const write = proposal.child; write.card_id = childScope.card_id; write.draft_id = childScope.draft_id;
    write.source_revision = childScope.source_revision; write.operation_id = firstOperation; write.values = textWire(model.frozenRaw);
    write.assets = model.frozenRaw.assets.map((selected: AssetSelection): AssetSelection => copyAsset(selected)); model.proposal = proposal;
    model.handoffLiteral = JSON.stringify(proposal); const command = new HandoffCommand(); command.business_handoff.request_json = model.handoffLiteral;
    model.handoffWire = JSON.stringify(command); budget(model.handoffLiteral); budget(model.handoffWire);
    model.handoffHash = await model.hash(bytes(model.handoffLiteral)); model.requireParent(true);
    if (!sameValues(firstRaw, parent.current)) { throw new Error('Business handoff raw changed during freeze'); } return model;
  }
  static savedExact(session: EditorBusinessSessionCoordinator, parent: EditorDraftCoordinator, closeOperation: string,
    discardOperation: string, hooks: EditorBusinessHandoffHooks): EditorBusinessHandoffCoordinator {
    const model = new EditorBusinessHandoffCoordinator(session, parent, hooks); model.requireParent(true);
    const record = model.parentRecord!, publication = session.publication;
    if (session.phase !== 'issued' || session.unknown || parent.dirty || !sameBusinessProof(proof(record), proof(publication)) ||
      !sameValues(record.values, publication.values) || !session.mayConsume(parent.current)) {
      throw new Error('Saved-exact close cannot consume newer raw, selection or assets');
    }
    model.independent([closeOperation, discardOperation]); const close = new BusinessCloseLiteral(); close.intent = copyEditorIntentProof(model.intent);
    close.expected_generation = '2'; close.operation_id = closeOperation; close.disposition = 'saved_exact'; close.parent = new CloseParent();
    close.parent.proof = proof(record); close.parent.discard_operation = discardOperation;
    model.frozenRaw = copyValues(parent.current); model.freezeClose(close); return model;
  }
  private independent(operations: string[]): void {
    const used = [this.intent.prepare_operation, this.committed.operation, this.session.publication.operation_id,
      this.session.view?.issue_operation ?? '']; if (this.parentRecord) { used.push(this.parentRecord.operation_id); }
    const seen: string[] = [];
    for (const operation of operations) {
      identity(operation); if (seen.includes(operation) || used.includes(operation)) { throw new Error('Business handoff operation identities must be independent'); }
      seen.push(operation);
    }
  }
  private async hash(value: Uint8Array): Promise<string> {
    const result = await (this.hooks.hash ? this.hooks.hash(value) : platformHash(value)); if (!digest(result)) { throw new Error('Business handoff SHA256 unavailable'); }
    return result;
  }
  private idle(): void { if (this.inFlight || this.session.saving) { throw new Error('Business handoff request is already in flight'); } }
  private requireParent(exact: boolean): void {
    const parent = this.parent, record = parent?.confirmed, frozen = this.parentRecord;
    if (!parent || !record || !frozen || !this.hooks.isCurrent() || exact && !this.hooks.isExact() || !this.hooks.importsReady() ||
      parent.disposed || parent.saving || parent.unknown || parent.conflicted || !parent.writesPaused ||
      !record.active || !record.current_active || record.current_generation !== record.generation ||
      !sameBusinessProof(proof(record), proof(frozen)) || !sameScope(record.scope, frozen.scope) ||
      record.scope.card_id !== this.committed.card_id || record.scope.source_kind !== this.session.publication.scope.source_kind ||
      record.scope.source_revision !== this.session.publication.scope.source_revision || record.scope.source !== this.session.publication.scope.source) {
      throw new Error('Business handoff parent queue, imports or owner changed');
    }
    validateRecord(record, true);
  }
  get originalHandoff(): string { return this.handoffWire; }
  get originalHandoffLiteral(): string { return this.handoffLiteral; }
  get originalRetirement(): string { return this.retirementWire; }
  get originalClose(): string { return this.closeWire; }
  get firstRaw(): Values | undefined { return this.frozenRaw ? copyValues(this.frozenRaw) : undefined; }
  get scope(): DraftScope | undefined { return this.childScope ? copyScope(this.childScope) : undefined; }
  get confirmed(): DraftRecord | undefined { return this.firstRecord ? copyRecord(this.firstRecord) : undefined; }
  get currentConfirmed(): DraftRecord | undefined { return this.currentChildRecord ? copyRecord(this.currentChildRecord) : undefined; }
  get parentReference(): DraftBusinessProof | undefined { return this.proposal ? clone(this.proposal.parent) : undefined; }
  get retired(): DraftRecord | undefined { return this.retiredRecord ? copyRecord(this.retiredRecord) : undefined; }
  get firstCommitted(): boolean { return this.firstEffect; }
  get retirementCommitted(): boolean { return this.retirementEffect; }
  get closeCommitted(): boolean { return this.closeEffect; }
  get closed(): boolean { return this.closeConfirmed; }
  get conflicted(): boolean { return this.firstConflict; }
  get saving(): boolean { return this.inFlight; }
  get unknown(): boolean { return !!this.pendingKind || !!this.pendingReadWire || !!this.pendingHistoryWire; }
  get pendingNative(): string { return this.pendingKind; }
  get pendingRead(): string { return this.pendingReadWire; }
  get pendingHistory(): string { return this.pendingHistoryWire; }
  get error(): string { return this.lastError; }
  begin(): Promise<DraftRecord> {
    this.idle(); this.requireParent(false);
    if (!this.handoffWire || this.firstRecord || this.pendingKind || this.pendingReadWire) { return Promise.reject(new Error('Use the original business handoff reconciliation explicitly')); }
    return this.first(false);
  }
  retry(): Promise<DraftRecord> {
    this.idle(); if (!this.handoffWire || this.pendingKind !== 'handoff') { return Promise.reject(new Error('No uncertain original business handoff')); }
    return this.first(true);
  }
  private expectedLink(): DraftBusinessLink {
    if (!this.proposal) { throw new Error('Missing fixed business handoff plan'); }
    const link = new DraftBusinessLink(); link.parent = clone(this.proposal.parent);
    link.intent.intent_id = this.intent.intent_id; link.intent.prepare_operation = this.intent.prepare_operation;
    link.intent.generation = this.intent.generation; link.intent.prepared_record_sha256 = this.intent.prepared_record_sha256;
    link.intent.request_sha256 = this.intent.request_sha256;
    link.plan_operation = this.proposal.plan_operation; link.handoff_request_sha256 = this.handoffHash;
    link.child_operation = this.proposal.child.operation_id; link.committed_operation = this.committed.operation;
    link.committed_revision = this.committed.revision; link.command_sha256 = this.committed.command_sha256;
    link.content_sha256 = this.committed.content_sha256; link.request_sha256 = this.committed.request_sha256;
    link.publication_sha256 = this.committed.publication_sha256; return link;
  }
  private bindView(view: EditorIntentView): void {
    if (!sameEditorIntentProof(view.proof, this.intent) || view.card_id !== this.committed.card_id ||
      view.business_operation !== this.committed.operation || view.expected_revision !== this.committed.revision) {
      throw new Error('Business handoff intent identity differs');
    }
    this.session.adoptIntentView(view);
  }
  private validateChild(record: DraftRecord, first: boolean): void {
    validateRecord(record, true);
    if (!this.childScope || !this.frozenRaw || !record.business_link || !sameBusinessLink(record.business_link, this.expectedLink()) ||
      !sameScope(record.scope, this.childScope) || record.fork_link || record.fork_retirement || record.business_retirement || !record.active ||
      !validGeneration(record.generation) || record.generation === '0' ||
      (first || record.generation === '1') && (record.generation !== '1' || record.operation_id !== this.proposal!.child.operation_id ||
        record.consumed_imports.length !== 0 || !sameValues(record.values, this.frozenRaw))) {
      throw new Error('Business child receipt differs from exact fixed plan/history');
    }
    if (record.generation === '1' && this.retirementLiteral &&
      record.request_sha256 !== (JSON.parse(this.retirementLiteral) as BusinessRetirementLiteral).child_request_sha256) {
      throw new Error('Current first child canonical request differs from persisted retirement proof');
    }
    if (first && this.parentRecord) {
      for (const pin of record.assets) {
        const original = this.parentRecord.assets.find((old): boolean => old.selection.asset_id === pin.selection.asset_id);
        if (!original || original.display_name !== pin.display_name || original.media_type !== pin.media_type ||
          original.byte_length !== pin.byte_length || original.sha256 !== pin.sha256) { throw new Error('Business child changed a confirmed parent pin'); }
      }
    }
    if (!validGeneration(record.current_generation) || record.current_generation === '0') { throw new Error('Business child current generation differs'); }
  }
  private async first(wasUnknown: boolean): Promise<DraftRecord> {
    this.inFlight = true; this.hooks.changed();
    try {
      const received = await this.hooks.send(this.handoffWire); if (received?.effect === 'committed') { this.firstEffect = true; }
      const reply = full(received);
      if (!reply.ok && reply.effect === 'not_committed' && !wasUnknown) { throw new Error(reply.error || 'First child rejected; its durable plan must be inspected'); }
      const view = summary(reply);
      handoffPhase(view);
      if (!Array.isArray(reply.drafts) || reply.drafts.length !== 1 ||
        reply.effect !== 'committed' || reply.receipt_revision !== '1') { throw new Error('Business handoff child effect/receipt differs'); }
      const record = reply.drafts[0]; this.validateChild(record, true);
      if (this.firstRecord && (this.firstRecord.request_sha256 !== record.request_sha256 || !sameValues(this.firstRecord.values, record.values))) {
        throw new Error('Business immutable first child changed');
      }
      this.bindView(view); this.firstRecord = copyRecord(record); this.firstEffect = true; this.pendingKind = ''; this.lastError = '';
      if (!record.current_active || record.current_generation !== '1') {
        this.firstConflict = true; this.lastError = 'First child committed; current child changed and must be read without replacement';
        throw new Error(this.lastError);
      }
      this.currentChildRecord = copyRecord(record); return copyRecord(record);
    } catch (error) {
      if (!this.firstRecord || !this.firstConflict) { this.pendingKind = 'handoff'; }
      this.lastError = errorValue(error).message; throw errorValue(error);
    } finally { this.inFlight = false; this.hooks.changed(); }
  }
  // Capture a fresh lease/epoch cutoff at the caller. Newer complete S3 is
  // copied into the real child writer; neither first ACK nor old S2 replaces it.
  openChild(latestRaw: Values, guard: () => boolean): EditorDraftCoordinator {
    this.idle(); this.requireParent(false);
    const record = this.currentChildRecord;
    if (!record || this.firstConflict || !guard() || !sameValues(latestRaw, this.parent!.current)) { throw new Error('Business child install lacks the current complete input cutoff'); }
    validateRaw(latestRaw);
    if (!this.child) {
      this.child = new EditorDraftCoordinator(this.childScope!, latestRaw, this.hooks.sendDraft, this.hooks.changed, this.hooks.operation, record);
    } else { throw new Error('Business child writer was already opened'); }
    if (!guard() || !this.hooks.isCurrent()) { this.child.dispose(); this.child = undefined; throw new Error('Business child input cutoff changed'); }
    return this.child;
  }
  // Explicit current-journal read from Workbench is usable after restart or an
  // advanced first receipt. It never makes an inactive journal active again.
  openCurrentChild(record: DraftRecord, latestRaw: Values, guard: () => boolean): EditorDraftCoordinator {
    this.idle(); this.validateChild(record, false);
    if (!record.current_active || record.current_generation !== record.generation || !this.hooks.isCurrent() || !guard() || this.child) {
      throw new Error('Current business child is not owned and active');
    }
    validateRaw(latestRaw); const frozen = copyRecord(record);
    this.child = new EditorDraftCoordinator(frozen.scope, latestRaw, this.hooks.sendDraft, this.hooks.changed, this.hooks.operation, frozen);
    if (!guard() || !this.hooks.isCurrent()) { this.child.dispose(); this.child = undefined; throw new Error('Current child input cutoff changed'); }
    this.currentChildRecord = frozen; this.firstConflict = false;
    // A verified current native business15 record proves its immutable first
    // exists. Never replay generation1 merely to obtain a current writer.
    this.firstEffect = true; if (frozen.generation === '1') { this.firstRecord = copyRecord(frozen); }
    if (this.pendingKind === 'handoff') { this.pendingKind = ''; }
    return this.child;
  }
  private historyWire(kind: string): string {
    if (!this.proposal || !this.childScope) { throw new Error('Read the fixed handoff plan before its parent history'); }
    const command = new DraftHistoryCommand(); command.id = this.childScope.card_id;
    command.draft_id = this.proposal.parent.draft_id;
    command.draft_operation = kind === 'parent' ? this.proposal.parent.save_operation : this.proposal.retirement_operation;
    command.generation = kind === 'parent' ? this.proposal.parent.generation : nextGeneration(this.proposal.parent.generation);
    const wire = JSON.stringify(command); budget(wire);
    const previous = kind === 'parent' ? this.parentHistoryWire : this.retirementHistoryWire;
    if (previous && previous !== wire) { throw new Error('Original handoff history read identity changed'); }
    if (kind === 'parent') { this.parentHistoryWire = wire; } else { this.retirementHistoryWire = wire; }
    return wire;
  }
  private parentScope(record: DraftRecord): boolean {
    const original = this.session.publication.scope;
    return !!this.proposal && record.scope.card_id === this.committed.card_id && record.scope.draft_id === this.proposal.parent.draft_id &&
      record.scope.source_kind === original.source_kind && record.scope.source_revision === original.source_revision && record.scope.source === original.source;
  }
  private validateParentHistory(record: DraftRecord): void {
    validateRecord(record, true);
    if (!this.proposal || !this.frozenRaw || !record.active || !sameBusinessProof(proof(record), this.proposal.parent) ||
      !this.parentScope(record) || record.fork_retirement || record.business_retirement ||
      !validGeneration(record.current_generation) || record.current_generation === '0' ||
      record.current_generation.length < record.generation.length ||
      record.current_generation.length === record.generation.length && record.current_generation < record.generation ||
      record.current_generation === record.generation && !record.current_active) {
      throw new Error('Exact immutable handoff parent history differs');
    }
    selectedRaw(this.frozenRaw, record);
    if (this.parentRecord && (!sameScope(this.parentRecord.scope, record.scope) || !sameValues(this.parentRecord.values, record.values) ||
      this.parentRecord.request_sha256 !== record.request_sha256 || JSON.stringify(this.parentRecord.assets) !== JSON.stringify(record.assets) ||
      JSON.stringify(this.parentRecord.consumed_imports) !== JSON.stringify(record.consumed_imports))) {
      throw new Error('Known immutable handoff parent history changed');
    }
  }
  loadParentHistory(): Promise<DraftRecord> {
    this.idle(); return this.historyRead('parent');
  }
  async readRetirementHistory(): Promise<DraftRecord> {
    this.idle(); if (!this.parentRecord) { await this.loadParentHistory(); }
    return this.historyRead('retirement');
  }
  retryHistory(): Promise<DraftRecord> {
    this.idle(); if (!this.pendingHistoryWire) { return Promise.reject(new Error('No uncertain original handoff history read')); }
    return this.historyRead(this.pendingHistoryKind);
  }
  private async historyRead(kind: string): Promise<DraftRecord> {
    this.idle();
    const wire = this.historyWire(kind);
    if (this.pendingHistoryWire && this.pendingHistoryWire !== wire) { throw new Error('Retry the original handoff history read explicitly'); }
    this.inFlight = true; this.hooks.changed();
    try {
      const reply = full(await this.hooks.send(wire));
      if (reply.ok !== true || reply.error !== '' || reply.effect !== 'not_committed' || reply.receipt_revision !== '' ||
        !Array.isArray(reply.drafts) || reply.drafts.length !== 1) { throw new Error(reply.error || 'Incomplete exact handoff history read'); }
      const record = reply.drafts[0]; if (record.repeated !== true) { throw new Error('Exact history read is not an immutable repeated record'); }
      if (kind === 'parent') {
        this.validateParentHistory(record); this.parentRecord = copyRecord(record);
        if (this.firstRecord) { this.validateChild(this.firstRecord, true); }
      } else {
        this.validateRetirementRecord(record); this.retiredRecord = copyRecord(record); this.retirementEffect = true;
        if (this.pendingKind === 'retirement') { this.pendingKind = ''; }
      }
      this.pendingHistoryWire = ''; this.pendingHistoryKind = ''; this.lastError = ''; return copyRecord(record);
    } catch (error) {
      this.pendingHistoryWire = wire; this.pendingHistoryKind = kind; this.lastError = errorValue(error).message; throw errorValue(error);
    } finally { this.inFlight = false; this.hooks.changed(); }
  }
  observeParentRetirement(value: DraftRecord): void {
    this.idle(); budget(JSON.stringify(value)); const record = copyRecord(value); this.validateRetirementRecord(record);
    this.retiredRecord = record; this.retirementEffect = true;
    // Exact history settles the same fixed retirement, never any close result.
    if (this.pendingKind === 'retirement') { this.pendingKind = ''; }
    if (this.pendingHistoryKind === 'retirement') { this.pendingHistoryWire = ''; this.pendingHistoryKind = ''; }
    this.hooks.changed();
  }
  private validateRetirementRecord(record: DraftRecord): void {
    validateRecord(record, true);
    if (!this.proposal || !record.business_retirement || !sameBusinessLink(record.business_retirement.business_link, this.expectedLink()) ||
      record.business_retirement.child_draft_id !== this.proposal.child.draft_id ||
      record.business_retirement.operation_id !== this.proposal.retirement_operation || record.active || record.current_active ||
      record.current_generation !== record.generation || !this.parentRecord || !sameScope(record.scope, this.parentRecord.scope) ||
      !sameValues(record.values, this.parentRecord.values) ||
      record.consumed_imports.length !== this.parentRecord.consumed_imports.length ||
      record.consumed_imports.some((operation: string, index: number): boolean => operation !== this.parentRecord!.consumed_imports[index])) {
      throw new Error('Exact parent retirement history differs');
    }
    for (let index: number = 0; index < record.assets.length; index++) {
      const pin = record.assets[index], original = this.parentRecord.assets[index];
      if (!original || pin.pin_id !== original.pin_id || pin.display_name !== original.display_name || pin.media_type !== original.media_type ||
        pin.byte_length !== original.byte_length || pin.sha256 !== original.sha256) { throw new Error('Retired parent immutable pin metadata differs'); }
    }
  }
  async loadRetirement(): Promise<void> {
    this.idle(); if (!this.proposal || this.pendingReadWire) { throw new Error('Resolve the original handoff plan read first'); }
    const handoff = await this.read('handoff'); this.validatePlan(handoff);
    const retirement = await this.read('retirement'); this.validateRetirement(retirement.retirement_request_json ?? '');
  }
  retryRead(): Promise<EditorIntentView> {
    this.idle(); if (!this.pendingReadWire) { return Promise.reject(new Error('No uncertain original business handoff read')); }
    return this.read(this.pendingReadPart);
  }
  private async read(part: string): Promise<EditorIntentView> {
    const wire = editorIntentReadWire(this.intent, part);
    if (this.pendingReadWire && this.pendingReadWire !== wire) { throw new Error('Original business handoff read cannot change'); }
    this.inFlight = true; this.hooks.changed();
    try {
      const view = readSingle(full(await this.hooks.send(wire)), part);
      const meta = clone(view); meta.part = 'summary'; meta.request_json = ''; meta.publication = null;
      meta.save_request_json = ''; meta.inspect_request_json = ''; meta.close_request_json = '';
      meta.handoff_request_json = ''; meta.retirement_request_json = ''; this.bindView(meta);
      if (part === 'handoff') { this.validatePlan(view); }
      if (part === 'retirement') { this.validateRetirement(view.retirement_request_json ?? ''); }
      if (part === 'close' && this.closeLiteral && view.close_request_json !== this.closeLiteral) { throw new Error('Original business close literal changed'); }
      this.pendingReadWire = ''; this.pendingReadPart = ''; this.lastError = ''; return clone(view);
    } catch (error) { this.pendingReadWire = wire; this.pendingReadPart = part; this.lastError = errorValue(error).message; throw errorValue(error); }
    finally { this.inFlight = false; this.hooks.changed(); }
  }
  private validatePlan(view: EditorIntentView): void {
    handoffPhase(view);
    if ((view.handoff_request_json ?? '') !== this.handoffLiteral || !this.handoffLiteral) { throw new Error('Native durable handoff literal differs'); }
  }
  private validateRetirement(literal: string): void {
    budget(literal); const value = JSON.parse(literal) as BusinessRetirementLiteral;
    exactKeys(value, ['schema_version', 'intent', 'plan_operation', 'handoff_request_sha256', 'parent', 'child_draft_id',
      'child_operation', 'child_request_sha256', 'operation_id']); validateEditorIntentProof(value.intent); validateBusinessProof(value.parent);
    if (!this.proposal || value.schema_version !== 1 || !sameEditorIntentProof(value.intent, this.intent) ||
      value.plan_operation !== this.proposal.plan_operation || value.handoff_request_sha256 !== this.handoffHash ||
      !sameBusinessProof(value.parent, this.proposal.parent) || value.child_draft_id !== this.proposal.child.draft_id ||
      value.child_operation !== this.proposal.child.operation_id || value.operation_id !== this.proposal.retirement_operation ||
      !digest(value.child_request_sha256) || this.firstRecord && value.child_request_sha256 !== this.firstRecord.request_sha256 ||
      this.retirementLiteral && this.retirementLiteral !== literal) { throw new Error('Native exact retirement plan differs'); }
    const command = new RetirementCommand(); command.business_retirement.request_json = literal;
    const wire = JSON.stringify(command); budget(wire); this.retirementLiteral = literal; this.retirementWire = wire;
  }
  private requireChild(): void {
    const child = this.child, record = child?.confirmed;
    if (!child || !record || child.disposed || child.saving || child.unknown || child.conflicted || child.dirty ||
      !this.hooks.isCurrent() || !this.hooks.childCurrent(child) || !this.hooks.importsReady()) { throw new Error('Latest full child raw/writer is not confirmed under its current lease'); }
    this.validateChild(record, false);
    if (!record.current_active || record.current_generation !== record.generation || !sameValues(child.current, record.values)) { throw new Error('Current child journal changed'); }
  }
  retire(): Promise<DraftRecord> {
    this.idle(); this.requireChild(); if (!this.retirementWire || !this.parentRecord || this.retiredRecord || this.pendingKind || this.pendingReadWire || this.pendingHistoryWire) {
      return Promise.reject(new Error('Read the fixed native retirement plan before retiring its exact parent'));
    }
    return this.retirement(false);
  }
  retryRetire(): Promise<DraftRecord> {
    this.idle(); if (!this.retirementWire || this.pendingKind !== 'retirement') { return Promise.reject(new Error('No uncertain original parent retirement')); }
    return this.retirement(true);
  }
  private async retirement(wasUnknown: boolean): Promise<DraftRecord> {
    this.inFlight = true; this.hooks.changed();
    try {
      const received = await this.hooks.send(this.retirementWire); if (received?.effect === 'committed') { this.retirementEffect = true; }
      const reply = full(received), view = summary(reply);
      handoffPhase(view);
      if (reply.effect !== 'committed' || !Array.isArray(reply.drafts) || reply.drafts.length !== 1 ||
        reply.receipt_revision !== nextGeneration(this.proposal!.parent.generation)) { throw new Error('Parent retirement effect/receipt differs'); }
      const record = reply.drafts[0]; this.validateRetirementRecord(record);
      this.bindView(view); this.retiredRecord = copyRecord(record); this.retirementEffect = true; this.pendingKind = ''; this.lastError = ''; return copyRecord(record);
    } catch (error) {
      // A failed cleanup never erases a separately proven business or child.
      this.pendingKind = 'retirement'; this.lastError = errorValue(error).message; throw errorValue(error);
    } finally { this.inFlight = false; this.hooks.changed(); }
  }
  closeRetired(operation: string): Promise<EditorIntentView> {
    this.idle(); if (!this.retiredRecord || !this.proposal || this.pendingKind || this.pendingReadWire || this.pendingHistoryWire || this.closeConfirmed) {
      return Promise.reject(new Error('Intent close needs its exact confirmed parent retirement'));
    }
    this.independent([operation]);
    if ([this.proposal.plan_operation, this.proposal.child.operation_id, this.proposal.retirement_operation].includes(operation)) {
      return Promise.reject(new Error('Intent close operation must be independent'));
    }
    const close = new BusinessCloseLiteral(); close.intent = copyEditorIntentProof(this.intent); close.expected_generation = '3';
    close.operation_id = operation; close.disposition = 'handoff_retired'; close.plan_operation = this.proposal.plan_operation;
    this.freezeClose(close); return this.performClose(false);
  }
  private freezeClose(value: BusinessCloseLiteral): void {
    const literal = JSON.stringify(value); budget(literal); if (this.closeLiteral && this.closeLiteral !== literal) { throw new Error('Original business close cannot change'); }
    const command = new CloseCommand(); command.editor_intent_close.request_json = literal;
    const wire = JSON.stringify(command); budget(wire); this.closeLiteral = literal; this.closeWire = wire; this.closeMode = value.disposition;
  }
  closeExact(): Promise<EditorIntentView> {
    this.idle(); this.requireParent(true);
    if (this.closeMode !== 'saved_exact' || this.pendingKind || this.pendingReadWire || this.closeConfirmed ||
      this.parent!.dirty || !this.frozenRaw || !sameValues(this.parent!.current, this.frozenRaw)) {
      return Promise.reject(new Error('Saved-exact close input cutoff changed'));
    }
    return this.performClose(false);
  }
  close(): Promise<EditorIntentView> { return this.closeExact(); }
  retryClose(): Promise<EditorIntentView> {
    this.idle(); if (!this.closeWire || this.pendingKind !== 'close') { return Promise.reject(new Error('No uncertain original business close')); }
    return this.performClose(true);
  }
  private async performClose(wasUnknown: boolean): Promise<EditorIntentView> {
    this.inFlight = true; this.hooks.changed();
    try {
      const received = await this.hooks.send(this.closeWire); if (received?.effect === 'committed') { this.closeEffect = true; }
      const reply = full(received), view = summary(reply), revision = this.closeMode === 'saved_exact' ? '4' : '5';
      if (reply.effect !== 'committed' || reply.receipt_revision !== revision || view.phase !== 'closed' || view.current_active ||
        view.current_generation !== revision || view.close_disposition !== this.closeMode) { throw new Error('Final intent close effect/receipt differs'); }
      this.bindView(view); this.closeEffect = true; this.closeConfirmed = true; this.pendingKind = ''; this.lastError = ''; return clone(view);
    } catch (error) { this.pendingKind = 'close'; this.lastError = errorValue(error).message; throw errorValue(error); }
    finally { this.inFlight = false; this.hooks.changed(); }
  }
  static async restore(session: EditorBusinessSessionCoordinator, parent: EditorDraftCoordinator | undefined,
    hooks: EditorBusinessHandoffHooks): Promise<EditorBusinessHandoffCoordinator> {
    const model = new EditorBusinessHandoffCoordinator(session, parent, hooks);
    const reply = full(await hooks.send(editorIntentReadWire(model.intent, 'handoff'))), view = readSingle(reply, 'handoff');
    if (!sameEditorIntentProof(view.proof, model.intent) || view.card_id !== model.committed.card_id ||
      view.business_operation !== model.committed.operation || view.expected_revision !== model.committed.revision) { throw new Error('Restored handoff intent differs'); }
    const literal = view.handoff_request_json ?? '';
    if (literal) {
      handoffPhase(view);
      budget(literal); const proposal = JSON.parse(literal) as BusinessHandoffLiteral;
      exactKeys(proposal, ['schema_version', 'intent', 'parent', 'child', 'plan_operation', 'retirement_operation']);
      validateEditorIntentProof(proposal.intent); validateBusinessProof(proposal.parent);
      exactKeys(proposal.child, ['card_id', 'draft_id', 'source_kind', 'source_revision', 'expected_generation', 'operation_id', 'values', 'assets']);
      exactKeys(proposal.child.values, ['title', 'description', 'hypothesis', 'conclusion', 'todos', 'category', 'stage']);
      const write = proposal.child, card = model.committed.historical_card!;
      if (!Array.isArray(write.assets)) { throw new Error('Restored full child selection is missing'); }
      for (const selected of write.assets) { exactKeys(selected, ['origin', 'asset_id', 'aliases']); }
      identity(write.draft_id); identity(proposal.plan_operation); identity(write.operation_id); identity(proposal.retirement_operation);
      if (proposal.schema_version !== 1 || !sameEditorIntentProof(proposal.intent, model.intent) || write.card_id !== card.id ||
        write.source_kind !== 0 || write.source_revision !== card.revision || write.expected_generation !== '0' ||
        write.draft_id === proposal.parent.draft_id || new Set<string>([proposal.plan_operation, write.operation_id,
          proposal.retirement_operation, proposal.parent.save_operation, model.intent.prepare_operation, model.committed.operation]).size !== 6 ||
        write.assets.some((selected: AssetSelection): boolean => selected.origin !== 4)) { throw new Error('Restored fixed child plan differs'); }
      model.proposal = clone(proposal); model.frozenRaw = raw(write); validateRaw(model.frozenRaw);
      const restoredScope = new DraftScope(); restoredScope.card_id = card.id; restoredScope.draft_id = write.draft_id;
      restoredScope.source_kind = 0; restoredScope.source_revision = card.revision; restoredScope.source = card.source;
      model.childScope = restoredScope;
      model.handoffLiteral = literal; const command = new HandoffCommand(); command.business_handoff.request_json = literal;
      model.handoffWire = JSON.stringify(command); budget(model.handoffWire); model.handoffHash = await model.hash(bytes(literal));
      model.pendingKind = 'handoff'; await model.read('retirement');
    }
    const closeView = await model.read('close');
    if (closeView.close_request_json) {
      const close = JSON.parse(closeView.close_request_json) as BusinessCloseLiteral;
      exactKeys(close, ['schema_version', 'intent', 'expected_generation', 'operation_id', 'disposition', 'parent', 'plan_operation']);
      validateEditorIntentProof(close.intent); identity(close.operation_id);
      if (close.schema_version !== 1 || !sameEditorIntentProof(close.intent, model.intent) || !['saved_exact', 'handoff_retired'].includes(close.disposition) ||
        close.expected_generation !== (close.disposition === 'saved_exact' ? '2' : '3') ||
        (close.disposition === 'saved_exact' ? !close.parent || close.plan_operation !== '' :
          close.parent !== null || !model.proposal || close.plan_operation !== model.proposal.plan_operation)) { throw new Error('Restored exact close differs'); }
      if (close.parent) {
        exactKeys(close.parent, ['proof', 'discard_operation']); validateBusinessProof(close.parent.proof); identity(close.parent.discard_operation);
        if (!sameBusinessProof(close.parent.proof, proof(session.publication))) { throw new Error('Restored close publication differs'); }
      }
      model.closeLiteral = closeView.close_request_json; const command = new CloseCommand(); command.editor_intent_close.request_json = model.closeLiteral;
      model.closeWire = JSON.stringify(command); budget(model.closeWire); model.closeMode = close.disposition;
      model.closeConfirmed = closeView.phase === 'closed'; model.closeEffect = model.closeConfirmed;
      model.pendingKind = model.closeConfirmed ? '' : 'close';
    }
    return model;
  }
}
