import { util } from '@kit.ArkTS';
import {
  DraftBusinessProof, DraftRecord, DraftScope, EditorDraftCoordinator, Values, copyRecord, copyValues, validateRecord
} from './EditorDraft';
import {
  EditorBusinessHandoffCoordinator, EditorBusinessHandoffHooks, EditorBusinessHandoffReply
} from './EditorBusinessHandoff';
import { EditorBusinessSessionCoordinator } from './EditorBusinessSession';

const LIMIT: number = 512 * 1024;
class CurrentReadCommand { action: string = 'draft_read'; id: string = ''; draft_id: string = ''; }
class RecoveryBoundaryFailure extends Error {}
class RecoveryPlanAbsentFailure extends Error {}
function copyProof(value: DraftBusinessProof): DraftBusinessProof {
  const result = new DraftBusinessProof(); result.draft_id = value.draft_id; result.generation = value.generation;
  result.save_operation = value.save_operation; result.request_sha256 = value.request_sha256; return result;
}
function budget(value: string): void {
  if (new util.TextEncoder().encodeInto(value).length > LIMIT) { throw new Error('Complete business recovery JSON bytes limit'); }
}
function snapshot(value: EditorBusinessHandoffReply): EditorBusinessHandoffReply {
  const wire = JSON.stringify(value); budget(wire); return JSON.parse(wire) as EditorBusinessHandoffReply;
}
function sameScope(a: DraftScope, b: DraftScope): boolean {
  return a.card_id === b.card_id && a.draft_id === b.draft_id && a.source_kind === b.source_kind &&
    a.source_revision === b.source_revision && a.source === b.source;
}
function fullCurrent(reply: EditorBusinessHandoffReply, scope: DraftScope): DraftRecord {
  if (reply.ok !== true || reply.error !== '' || reply.effect !== 'not_committed' || reply.receipt_revision !== '' ||
    !Array.isArray(reply.drafts) || reply.drafts.length !== 1) { throw new Error(reply.error || 'Incomplete readonly current business child'); }
  const record = reply.drafts[0];
  if (!record || !Array.isArray(record.assets) || !Array.isArray(record.consumed_imports) || typeof record.repeated !== 'boolean' ||
    record.fork_link === undefined || record.fork_retirement === undefined || record.business_link === undefined ||
    record.business_retirement === undefined || !record.scope || !sameScope(record.scope, scope)) {
    throw new Error('Incomplete current child scope or lineage fields');
  }
  validateRecord(record, true); return copyRecord(record);
}

// Metadata is detached; writer and handoff are the actual coordinators. Reading
// ready never creates another writer or transfers its ownership to a view.
export class EditorBusinessRecoveryReady {
  writer: EditorDraftCoordinator; record: DraftRecord; values: Values;
  handoff: EditorBusinessHandoffCoordinator; parentReference: DraftBusinessProof;
  constructor(writer: EditorDraftCoordinator, record: DraftRecord, handoff: EditorBusinessHandoffCoordinator,
    parentReference: DraftBusinessProof) {
    this.writer = writer; this.record = copyRecord(record); this.values = copyValues(record.values);
    this.handoff = handoff; this.parentReference = copyProof(parentReference);
  }
}

// Only prepare a current writer from readonly native evidence. The caller owns
// the page lease, installs this same writer synchronously, and then explicitly
// resumes it or continues the already fixed retirement/close steps.
export class EditorBusinessRecoveryCoordinator {
  private session: EditorBusinessSessionCoordinator; private hooks: EditorBusinessHandoffHooks;
  private guard: () => boolean;
  private ownedHandoff: EditorBusinessHandoffCoordinator | undefined = undefined;
  private parentRecord: DraftRecord | undefined = undefined;
  private currentRecordValue: DraftRecord | undefined = undefined;
  private prepared: EditorBusinessRecoveryReady | undefined = undefined;
  private currentWire: string = ''; private uncertainWire: string = ''; private uncertainStage: string = '';
  private lastWire: string = ''; private stage: string = ''; private lastError: string = '';
  private inFlight: boolean = false; private stopped: boolean = false; private claimed: boolean = false;

  constructor(session: EditorBusinessSessionCoordinator, hooks: EditorBusinessHandoffHooks, guard: () => boolean) {
    if (session.saving || !session.qualified || !session.confirmed?.historical_card || !session.proof) {
      throw new Error('Recovery needs the original qualified business session');
    }
    this.session = session; this.hooks = hooks; this.guard = guard;
  }
  get handoff(): EditorBusinessHandoffCoordinator | undefined { return this.ownedHandoff; }
  get parentHistory(): DraftRecord | undefined { return this.parentRecord ? copyRecord(this.parentRecord) : undefined; }
  // Observed readonly record; only ready grants a strictly qualified writer.
  get currentRecord(): DraftRecord | undefined { return this.currentRecordValue ? copyRecord(this.currentRecordValue) : undefined; }
  get originalCurrentRead(): string { return this.currentWire; }
  get pendingRead(): string { return this.uncertainWire; }
  get unknown(): boolean { return !!this.uncertainWire; }
  get saving(): boolean { return this.inFlight; }
  get disposed(): boolean { return this.stopped; }
  get error(): string { return this.lastError; }
  get ready(): EditorBusinessRecoveryReady | undefined {
    const value = this.prepared;
    return value && !this.stopped && !value.writer.disposed ?
      new EditorBusinessRecoveryReady(value.writer, value.record, value.handoff, value.parentReference) : undefined;
  }
  private idle(): void {
    if (this.inFlight || this.session.saving || this.ownedHandoff?.saving) { throw new Error('Business recovery read is already in flight'); }
    if (this.stopped) { throw new Error('Business recovery preparation was disposed'); }
  }
  private owner(): void {
    if (this.stopped || !this.guard() || !this.hooks.isCurrent()) { throw new RecoveryBoundaryFailure('Business recovery owner/input cutoff changed'); }
  }
  private handoffHooks(): EditorBusinessHandoffHooks {
    return {
      send: async (wire: string): Promise<EditorBusinessHandoffReply> => {
        // The returned Handoff keeps the original sender for later explicit
        // mutation. This preparation calls only its readonly restore/history.
        if (this.inFlight) { this.lastWire = wire; }
        return this.hooks.send(wire);
      },
      sendDraft: this.hooks.sendDraft, changed: this.hooks.changed, operation: this.hooks.operation,
      isCurrent: this.hooks.isCurrent, isExact: this.hooks.isExact, importsReady: this.hooks.importsReady,
      childCurrent: this.hooks.childCurrent, hash: this.hooks.hash
    };
  }
  load(): Promise<EditorBusinessRecoveryReady> {
    this.idle(); if (this.uncertainWire) { return Promise.reject(new Error('Retry the fixed readonly business recovery request explicitly')); }
    if (this.prepared) { return Promise.reject(new Error('The actual current writer is already prepared')); }
    return this.prepare(false);
  }
  retry(): Promise<EditorBusinessRecoveryReady> {
    this.idle(); if (!this.uncertainWire) { return Promise.reject(new Error('No uncertain original business recovery read')); }
    return this.prepare(true);
  }
  private freezeCurrentRead(scope: DraftScope): void {
    const command = new CurrentReadCommand(); command.id = scope.card_id; command.draft_id = scope.draft_id;
    const wire = JSON.stringify(command); budget(wire);
    if (this.currentWire && this.currentWire !== wire) { throw new Error('Original current business child read identity changed'); }
    this.currentWire = wire;
  }
  private async prepare(retry: boolean): Promise<EditorBusinessRecoveryReady> {
    this.inFlight = true; this.hooks.changed();
    try {
      if (!retry) { this.owner(); }
      if (!this.ownedHandoff) {
        this.stage = 'plan'; this.lastWire = '';
        this.ownedHandoff = await EditorBusinessHandoffCoordinator.restore(this.session, undefined, this.handoffHooks());
        this.uncertainWire = ''; this.uncertainStage = '';
        this.owner();
      }
      const handoff = this.ownedHandoff, scope = handoff.scope, parent = handoff.parentReference;
      if (!scope || !parent || !handoff.originalHandoffLiteral) { throw new RecoveryPlanAbsentFailure('The original business intent has no fixed current child plan'); }
      if (!this.parentRecord) {
        this.stage = 'parent'; this.lastWire = '';
        this.parentRecord = retry && this.uncertainStage === 'parent' && handoff.pendingHistory ?
          await handoff.retryHistory() : await handoff.loadParentHistory();
        this.uncertainWire = ''; this.uncertainStage = '';
        this.owner();
      }
      this.freezeCurrentRead(scope); this.stage = 'child'; this.lastWire = this.currentWire;
      const record = fullCurrent(snapshot(await this.hooks.send(this.currentWire)), scope);
      this.currentRecordValue = copyRecord(record);
      // The read is a known fact before checking the page boundary. Revoking a
      // view cannot erase a historical business, parent, or current child read.
      this.uncertainWire = ''; this.uncertainStage = ''; this.owner();
      let writer: EditorDraftCoordinator;
      try { writer = handoff.openCurrentChild(record, record.values, this.guard); }
      catch (error) { this.owner(); throw error as Error; }
      writer.pauseWrites();
      const value = new EditorBusinessRecoveryReady(writer, record, handoff, parent); this.prepared = value;
      try { this.owner(); } catch (error) { writer.dispose(); this.prepared = undefined; this.stopped = true; throw error as Error; }
      this.lastError = ''; return new EditorBusinessRecoveryReady(writer, record, handoff, parent);
    } catch (error) {
      this.lastError = String(error);
      if (!(error instanceof RecoveryBoundaryFailure) && !(error instanceof RecoveryPlanAbsentFailure) && this.lastWire) {
        this.uncertainWire = this.lastWire; this.uncertainStage = this.stage;
      }
      throw error as Error;
    } finally { this.inFlight = false; this.hooks.changed(); }
  }
  claimWriter(): EditorDraftCoordinator {
    this.idle();
    if (this.claimed) { throw new Error('The actual current writer was already claimed'); }
    try { this.owner(); } catch (error) {
      if (this.prepared && !this.prepared.writer.disposed) { this.prepared.writer.dispose(); }
      this.prepared = undefined; this.stopped = true; throw error as Error;
    }
    const value = this.prepared;
    if (!value || value.writer.disposed || this.claimed) { throw new Error('No unclaimed actual current business writer'); }
    this.claimed = true; return value.writer;
  }
  dispose(): void {
    this.stopped = true;
    if (!this.claimed && this.prepared && !this.prepared.writer.disposed) { this.prepared.writer.dispose(); }
    this.hooks.changed();
  }
}
