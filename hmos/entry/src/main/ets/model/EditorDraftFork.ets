import {
  AssetSelection, DraftForkLink, DraftRecord, DraftSaveFailure, DraftScope, EditorDraftCoordinator,
  TextValue, Values, copyAsset, copyForkLink, copyRecord, copyScope, copyValues,
  sameForkLink, sameValues, validateRecord, validateValues, validGeneration
} from './EditorDraft';

// Private development raw-draft continuation. This does not rebase a business
// card, reconcile legacy todos, or establish a protected capture/handoff.
export interface DraftForkHooks {
  send: (serialized: string) => Promise<DraftRecord>;
  changed: () => void;
  operation: () => string;
  isCurrent: () => boolean;
}

class ForkTextWire {
  title: TextValue = new TextValue();
  description: TextValue = new TextValue();
  hypothesis: TextValue = new TextValue();
  conclusion: TextValue = new TextValue();
  todos: TextValue = new TextValue();
  category: string = '';
  stage: string = '';
}

class ForkChildWire {
  card_id: string = '';
  draft_id: string = '';
  source_kind: number = 1;
  source_revision: string = '0';
  expected_generation: string = '0';
  operation_id: string = '';
  values: ForkTextWire = new ForkTextWire();
  assets: AssetSelection[] = [];
}

class ForkWire {
  child: ForkChildWire = new ForkChildWire();
  parent_draft_id: string = '';
  parent_generation: string = '0';
  parent_save_operation: string = '';
  parent_request_sha256: string = '';
}

class ForkCommand {
  action: string = 'draft_fork';
  fork: ForkWire = new ForkWire();
}

class RetirementWire {
  card_id: string = '';
  child_draft_id: string = '';
  child_operation: string = '';
  parent_draft_id: string = '';
  parent_generation: string = '0';
  parent_save_operation: string = '';
  parent_request_sha256: string = '';
  operation_id: string = '';
}

class RetirementCommand {
  action: string = 'draft_fork_retire';
  fork_retirement: RetirementWire = new RetirementWire();
}

function sameFullScope(a: DraftScope, b: DraftScope): boolean {
  return a.card_id === b.card_id && a.draft_id === b.draft_id && a.source_kind === b.source_kind &&
    a.source_revision === b.source_revision && a.source === b.source;
}

export class EditorDraftForkCoordinator {
  private parent: EditorDraftCoordinator;
  private parentRecord: DraftRecord;
  private childScope: DraftScope;
  private frozenRaw: Values;
  private proof: DraftForkLink;
  private hooks: DraftForkHooks;
  private serialized: string;
  private inFlight: Promise<DraftRecord> | undefined = undefined;
  private firstRecord: DraftRecord | undefined = undefined;
  private child: EditorDraftCoordinator | undefined = undefined;
  private retirement: string = '';
  private retirementOperation: string = '';
  private phase: string = 'frozen';
  private lastError: string = '';
  private debounce: number;

  constructor(parent: EditorDraftCoordinator, childScope: DraftScope, firstRaw: Values, firstOperation: string,
    hooks: DraftForkHooks, debounce: number = 500) {
    const record = parent.confirmed;
    if (parent.disposed || parent.saving || parent.unknown || parent.conflicted || !record || !hooks.isCurrent() ||
      !record.active || !record.current_active || record.generation !== record.current_generation ||
      !validGeneration(record.generation) || record.generation === '0' || record.generation === '18446744073709551615') {
      throw new Error('Development fork requires an exact active confirmed parent');
    }
    validateRecord(record, true); validateValues(firstRaw, !!record.fork_link);
    const scope = parent.scope;
    if (!sameFullScope(record.scope, scope) || !sameValues(firstRaw, parent.current) ||
      !childScope.draft_id || childScope.draft_id === scope.draft_id || !firstOperation || firstOperation === record.operation_id ||
      childScope.card_id !== scope.card_id || childScope.source_kind !== scope.source_kind ||
      childScope.source_revision !== scope.source_revision || childScope.source !== scope.source ||
      scope.source_kind === 1 && scope.source !== '' ||
      scope.source_kind === 0 && !/^(?:[0-9a-f]{2})+$/.test(scope.source) || !Number.isFinite(debounce) || debounce < 0) {
      throw new Error('Development fork scope, operation or complete raw input differs');
    }
    this.parent = parent; this.parentRecord = copyRecord(record); this.childScope = copyScope(childScope);
    this.frozenRaw = copyValues(firstRaw); this.hooks = hooks; this.debounce = debounce;
    let parentIndex = -1;
    for (const selected of this.frozenRaw.assets) {
      const index = record.assets.findIndex((asset): boolean => asset.selection.asset_id === selected.asset_id);
      const pin = index >= 0 ? record.assets[index] : undefined;
      if (!pin || index <= parentIndex || selected.aliases.length !== pin.selection.aliases.length ||
        selected.aliases.some((alias: string, aliasIndex: number): boolean => alias !== pin.selection.aliases[aliasIndex])) {
        throw new Error('Development fork selection, aliases or order has no exact confirmed parent pin');
      }
      parentIndex = index;
      selected.origin = 4;
    }
    this.proof = new DraftForkLink(); this.proof.parent_draft_id = scope.draft_id;
    this.proof.parent_generation = record.generation; this.proof.parent_save_operation = record.operation_id;
    this.proof.parent_request_sha256 = record.request_sha256; this.proof.child_operation = firstOperation;
    const command = new ForkCommand(); command.fork.parent_draft_id = this.proof.parent_draft_id;
    command.fork.parent_generation = this.proof.parent_generation; command.fork.parent_save_operation = this.proof.parent_save_operation;
    command.fork.parent_request_sha256 = this.proof.parent_request_sha256;
    const child = command.fork.child; child.card_id = childScope.card_id; child.draft_id = childScope.draft_id;
    child.source_kind = childScope.source_kind; child.source_revision = childScope.source_revision; child.operation_id = firstOperation;
    child.values.title = this.frozenRaw.title; child.values.description = this.frozenRaw.description;
    child.values.hypothesis = this.frozenRaw.hypothesis; child.values.conclusion = this.frozenRaw.conclusion;
    child.values.todos = this.frozenRaw.todos; child.values.category = this.frozenRaw.category; child.values.stage = this.frozenRaw.stage;
    child.assets = this.frozenRaw.assets.map((asset: AssetSelection): AssetSelection => copyAsset(asset));
    this.serialized = JSON.stringify(command); parent.pauseWrites();
  }

  get pending(): string | undefined { return this.phase === 'confirmed' ? undefined : this.serialized; }
  get original(): string { return this.serialized; }
  get unknown(): boolean { return this.phase === 'unknown'; }
  get conflicted(): boolean { return this.phase === 'conflicted'; }
  get saving(): boolean { return this.inFlight !== undefined; }
  get error(): string { return this.lastError; }
  get confirmed(): DraftRecord | undefined { return this.firstRecord ? copyRecord(this.firstRecord) : undefined; }
  get firstRaw(): Values { return copyValues(this.frozenRaw); }
  get parentProof(): DraftForkLink { return copyForkLink(this.proof); }
  get scope(): DraftScope { return copyScope(this.childScope); }

  begin(): Promise<DraftRecord> {
    if (this.inFlight) { return this.inFlight; }
    if (this.phase !== 'frozen') { return Promise.reject(new Error('Development fork requires its original explicit reconciliation')); }
    return this.issue(false);
  }

  retry(): Promise<DraftRecord> {
    if (this.inFlight) { return this.inFlight; }
    if (this.phase !== 'unknown') { return Promise.reject(new Error('No uncertain development fork request to retry')); }
    return this.issue(true);
  }

  // A valid first receipt may be retained after the live owner changed. It must
  // never install a coordinator into that different owner or revive old input.
  openChild(): EditorDraftCoordinator {
    this.requireParent();
    if (this.phase !== 'confirmed' || !this.firstRecord) { throw new Error('Development fork first receipt is not current'); }
    if (!this.child) {
      this.child = new EditorDraftCoordinator(this.childScope, this.firstRecord.values, this.hooks.send,
        this.hooks.changed, this.hooks.operation, this.firstRecord, this.debounce);
    }
    return this.child;
  }

  // This only freezes retirement identity. The caller must confirm its latest
  // full raw in the child, switch its writer, and guard the live input epoch.
  retirementCommand(operation: string): string {
    this.requireParent();
    if (this.phase !== 'confirmed' || !this.child || !operation || operation === this.proof.child_operation ||
      operation === this.proof.parent_save_operation) { throw new Error('Development fork retirement is not admitted'); }
    if (this.retirement) {
      if (operation !== this.retirementOperation) { throw new Error('Development fork retirement operation cannot change'); }
      return this.retirement;
    }
    const command = new RetirementCommand(), wire = command.fork_retirement;
    wire.card_id = this.childScope.card_id; wire.child_draft_id = this.childScope.draft_id;
    wire.child_operation = this.proof.child_operation; wire.parent_draft_id = this.proof.parent_draft_id;
    wire.parent_generation = this.proof.parent_generation; wire.parent_save_operation = this.proof.parent_save_operation;
    wire.parent_request_sha256 = this.proof.parent_request_sha256; wire.operation_id = operation;
    this.retirementOperation = operation; this.retirement = JSON.stringify(command); return this.retirement;
  }

  releaseRejectedParent(): void {
    if (this.inFlight || this.phase !== 'frozen' && this.phase !== 'rejected') {
      throw new Error('Issued uncertain or committed fork cannot release its parent');
    }
    this.requireParent(); this.phase = 'released'; this.parent.resumeWrites(); this.hooks.changed();
  }

  private requireParent(): void {
    const record = this.parent.confirmed;
    if (!this.hooks.isCurrent() || this.parent.disposed || this.parent.saving || this.parent.unknown || this.parent.conflicted ||
      !this.parent.writesPaused || !record || !record.active || !record.current_active ||
      record.current_generation !== this.proof.parent_generation || record.generation !== this.proof.parent_generation ||
      record.operation_id !== this.proof.parent_save_operation || record.request_sha256 !== this.proof.parent_request_sha256 ||
      !sameFullScope(record.scope, this.parentRecord.scope)) {
      throw new Error('Development fork parent or live owner changed');
    }
  }

  private issue(wasUnknown: boolean): Promise<DraftRecord> {
    try { this.requireParent(); } catch (error) { return Promise.reject(error); }
    this.phase = 'pending'; this.lastError = '';
    this.inFlight = this.perform(wasUnknown).finally(() => { this.inFlight = undefined; this.hooks.changed(); });
    this.hooks.changed(); return this.inFlight;
  }

  private async perform(wasUnknown: boolean): Promise<DraftRecord> {
    try {
      const record = await this.hooks.send(this.serialized);
      validateRecord(record, true);
      if (!sameFullScope(record.scope, this.childScope) || record.operation_id !== this.proof.child_operation ||
        record.generation !== '1' || !record.active || !record.fork_link || !sameForkLink(record.fork_link, this.proof) ||
        record.fork_retirement || !sameValues(record.values, this.frozenRaw) ||
        !validGeneration(record.current_generation) || record.current_generation === '0' || typeof record.current_active !== 'boolean') {
        throw new Error('Development fork receipt differs from the frozen first request');
      }
      for (const pin of record.assets) {
        const prior = this.parentRecord.assets.find((asset): boolean => asset.selection.asset_id === pin.selection.asset_id);
        if (!prior || pin.display_name !== prior.display_name || pin.media_type !== prior.media_type ||
          pin.byte_length !== prior.byte_length || pin.sha256 !== prior.sha256) {
          throw new Error('Development fork receipt changed a confirmed parent pin');
        }
      }
      this.firstRecord = copyRecord(record);
      if (!record.current_active || record.current_generation !== '1') {
        this.phase = 'conflicted'; this.lastError = 'Development fork committed, but its current journal changed';
        throw new Error(this.lastError);
      }
      this.phase = 'confirmed'; this.lastError = ''; this.hooks.changed(); return copyRecord(record);
    } catch (error) {
      if (this.phase !== 'conflicted') {
        this.phase = !wasUnknown && error instanceof DraftSaveFailure && error.knownRejected ? 'rejected' : 'unknown';
        this.lastError = String(error);
      }
      this.hooks.changed(); if (error instanceof Error) { throw error; } throw new Error(String(error));
    }
  }
}
