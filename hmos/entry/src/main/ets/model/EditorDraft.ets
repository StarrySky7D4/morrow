// A draft journal is separate from a business edit. An acknowledgement only
// confirms the frozen snapshot; it never replaces the user's newer input.
export class TextValue {
  text: string = '';
  selection_base: number = -1;
  selection_extent: number = -1;
  affinity: number = 0;
  directional: boolean = false;
  composing_start: number = -1;
  composing_end: number = -1;
}

export class Values {
  title: TextValue = new TextValue();
  description: TextValue = new TextValue();
  hypothesis: TextValue = new TextValue();
  conclusion: TextValue = new TextValue();
  todos: TextValue = new TextValue();
  category: string = '灵感';
  stage: string = '待整理';
  // UI snapshot field. The original protobuf WriteRequest keeps this inventory
  // beside values, so freeze() moves it to draft.assets on the JSON wire.
  assets: AssetSelection[] = [];
}

export class AssetSelection {
  origin: number = 2;
  asset_id: string = '';
  aliases: string[] = [];
}

export class StoredAsset {
  selection: AssetSelection = new AssetSelection();
  pin_id: string = '';
  display_name: string = '';
  media_type: string = '';
  byte_length: string = '0';
  sha256: string = '';
}

export class DraftScope {
  card_id: string = '';
  draft_id: string = '';
  source_kind: number = 1;
  source_revision: string = '0';
  // UI-only source card bytes. These are never authority in a draft request.
  source: string = '';
}

export class DraftRecord {
  scope: DraftScope = new DraftScope();
  values: Values = new Values();
  operation_id: string = '';
  generation: string = '0';
  current_generation: string = '0';
  active: boolean = false;
  current_active: boolean = false;
  repeated: boolean = false;
  assets: StoredAsset[] = [];
  consumed_imports: string[] = [];
  // Native canonical WriteRequest digest. Required for development fork proof;
  // older normal-only synthetic receipts may omit it.
  request_sha256: string = '';
  fork_link: DraftForkLink | null = null;
  fork_retirement: DraftForkRetirement | null = null;
}

// Independent development lineage; never a protected capture/handoff ticket.
export class DraftForkLink {
  schema_version: number = 1;
  parent_draft_id: string = '';
  parent_generation: string = '0';
  parent_save_operation: string = '';
  parent_request_sha256: string = '';
  child_operation: string = '';
}

export class DraftForkRetirement {
  schema_version: number = 1;
  child_draft_id: string = '';
  operation_id: string = '';
  fork_link: DraftForkLink = new DraftForkLink();
}

export class DraftSaveFailure extends Error {
  knownRejected: boolean;
  constructor(knownRejected: boolean, message: string = 'Draft save failed') {
    super(message); this.knownRejected = knownRejected;
  }
}

export function copyText(value: TextValue): TextValue {
  const result = new TextValue(); result.text = value.text;
  result.selection_base = value.selection_base; result.selection_extent = value.selection_extent;
  result.affinity = value.affinity; result.directional = value.directional;
  result.composing_start = value.composing_start; result.composing_end = value.composing_end;
  return result;
}

export function copyValues(values: Values): Values {
  const result = new Values(); result.title = copyText(values.title);
  result.description = copyText(values.description); result.hypothesis = copyText(values.hypothesis);
  result.conclusion = copyText(values.conclusion); result.todos = copyText(values.todos);
  result.category = values.category; result.stage = values.stage;
  result.assets = (values.assets ?? []).map((asset: AssetSelection): AssetSelection => copyAsset(asset)); return result;
}

export function copyAsset(asset: AssetSelection): AssetSelection {
  const result = new AssetSelection(); result.origin = asset.origin; result.asset_id = asset.asset_id;
  result.aliases = asset.aliases.slice(); return result;
}

export function copyStoredAsset(asset: StoredAsset): StoredAsset {
  const result = new StoredAsset(); result.selection = copyAsset(asset.selection); result.pin_id = asset.pin_id;
  result.display_name = asset.display_name; result.media_type = asset.media_type;
  result.byte_length = asset.byte_length; result.sha256 = asset.sha256; return result;
}

export function copyScope(scope: DraftScope): DraftScope {
  const result = new DraftScope(); result.card_id = scope.card_id; result.draft_id = scope.draft_id;
  result.source_kind = scope.source_kind; result.source_revision = scope.source_revision;
  result.source = scope.source; return result;
}

export function copyRecord(record: DraftRecord): DraftRecord {
  const result = new DraftRecord(); result.scope = copyScope(record.scope);
  result.values = copyValues(record.values); result.operation_id = record.operation_id;
  result.generation = record.generation; result.current_generation = record.current_generation;
  result.active = record.active; result.current_active = record.current_active;
  result.repeated = record.repeated;
  result.assets = (record.assets ?? []).map((asset: StoredAsset): StoredAsset => copyStoredAsset(asset));
  result.consumed_imports = (record.consumed_imports ?? []).slice();
  result.request_sha256 = record.request_sha256 ?? '';
  result.fork_link = record.fork_link ? copyForkLink(record.fork_link) : null;
  result.fork_retirement = record.fork_retirement ? copyForkRetirement(record.fork_retirement) : null;
  return result;
}

export function copyForkLink(link: DraftForkLink): DraftForkLink {
  const result = new DraftForkLink(); result.schema_version = link.schema_version;
  result.parent_draft_id = link.parent_draft_id; result.parent_generation = link.parent_generation;
  result.parent_save_operation = link.parent_save_operation; result.parent_request_sha256 = link.parent_request_sha256;
  result.child_operation = link.child_operation; return result;
}

export function copyForkRetirement(marker: DraftForkRetirement): DraftForkRetirement {
  const result = new DraftForkRetirement(); result.schema_version = marker.schema_version;
  result.child_draft_id = marker.child_draft_id; result.operation_id = marker.operation_id;
  result.fork_link = copyForkLink(marker.fork_link); return result;
}

export function sameForkLink(a: DraftForkLink, b: DraftForkLink): boolean {
  return a.schema_version === b.schema_version && a.parent_draft_id === b.parent_draft_id &&
    a.parent_generation === b.parent_generation && a.parent_save_operation === b.parent_save_operation &&
    a.parent_request_sha256 === b.parent_request_sha256 && a.child_operation === b.child_operation;
}

function sameText(a: TextValue, b: TextValue): boolean {
  return a.text === b.text && a.selection_base === b.selection_base && a.selection_extent === b.selection_extent &&
    a.affinity === b.affinity && a.directional === b.directional && a.composing_start === b.composing_start &&
    a.composing_end === b.composing_end;
}

export function sameValues(a: Values, b: Values): boolean {
  return sameText(a.title, b.title) && sameText(a.description, b.description) &&
    sameText(a.hypothesis, b.hypothesis) && sameText(a.conclusion, b.conclusion) && sameText(a.todos, b.todos) &&
    a.category === b.category && a.stage === b.stage && sameAssets(a.assets ?? [], b.assets ?? []);
}

function sameAsset(a: AssetSelection, b: AssetSelection): boolean {
  return a.origin === b.origin && a.asset_id === b.asset_id && a.aliases.length === b.aliases.length &&
    a.aliases.every((alias: string, index: number): boolean => alias === b.aliases[index]);
}

export function sameAssets(a: AssetSelection[], b: AssetSelection[]): boolean {
  return a.length === b.length && a.every((asset: AssetSelection, index: number): boolean => sameAsset(asset, b[index]));
}

function sameScope(a: DraftScope, b: DraftScope): boolean {
  return a.card_id === b.card_id && a.draft_id === b.draft_id && a.source_kind === b.source_kind &&
    a.source_revision === b.source_revision;
}

export function validGeneration(value: string): boolean {
  return typeof value === 'string' && /^(0|[1-9][0-9]*)$/.test(value) && (value.length < 20 ||
    value.length === 20 && value <= '18446744073709551615');
}

// Avoid Number/BigInt conversion: revisions can exceed Number's safe range.
export function nextGeneration(value: string): string {
  if (!validGeneration(value) || value === '18446744073709551615') {
    throw new Error('Invalid or exhausted draft generation');
  }
  let carry = true, result = '';
  for (let index = value.length - 1; index >= 0; index--) {
    const digit = value.charCodeAt(index) - 48;
    result = (carry ? digit === 9 ? '0' : (digit + 1).toString() : value[index]) + result;
    carry = carry && digit === 9;
  }
  return carry ? '1' + result : result;
}

function validOffset(offset: number, units: number): boolean {
  return Number.isInteger(offset) && (offset === -1 || offset >= 0 && offset <= units);
}

export function validateValues(values: Values, allowForkAssets: boolean = false): void {
  for (const value of [values.title, values.description, values.hypothesis, values.conclusion, values.todos]) {
    if (typeof value.text !== 'string' || !validOffset(value.selection_base, value.text.length) ||
      !validOffset(value.selection_extent, value.text.length) || !validOffset(value.composing_start, value.text.length) ||
      !validOffset(value.composing_end, value.text.length) ||
      value.composing_start >= 0 && value.composing_end >= 0 && value.composing_start > value.composing_end ||
      value.affinity !== 0 && value.affinity !== 1 || typeof value.directional !== 'boolean') {
      throw new Error('Invalid draft UTF-16 editing value');
    }
  }
  if (typeof values.category !== 'string' || typeof values.stage !== 'string') {
    throw new Error('Invalid draft category or stage');
  }
  validateAssets(values.assets ?? [], allowForkAssets);
}

function validateAssets(assets: AssetSelection[], allowForkAssets: boolean = false): void {
  if (!Array.isArray(assets) || assets.length > 20) { throw new Error('Invalid draft selected asset count'); }
  const ids = new Set<string>();
  for (const asset of assets) {
    if (!asset || ![0, 2, 3].includes(asset.origin) && !(allowForkAssets && asset.origin === 4) || typeof asset.asset_id !== 'string' || !asset.asset_id ||
      ids.has(asset.asset_id) || !Array.isArray(asset.aliases) || asset.aliases.length > 8 ||
      asset.aliases.some((alias: string): boolean => typeof alias !== 'string' || !alias || alias.length > 16384)) {
      throw new Error('Invalid draft selected asset identity');
    }
    ids.add(asset.asset_id);
  }
}

function validateRecordAssets(record: DraftRecord): void {
  const selected = record.values.assets ?? [], stored = record.assets ?? [], consumed = record.consumed_imports ?? [];
  if (!Array.isArray(stored) || stored.length !== selected.length || !Array.isArray(consumed) ||
    consumed.length > stored.length || new Set<string>(consumed).size !== consumed.length ||
    consumed.some((operation: string): boolean => typeof operation !== 'string' || !operation)) {
    throw new Error('Draft receipt asset inventory differs');
  }
  stored.forEach((asset: StoredAsset, index: number): void => {
    if (!asset || !asset.selection || !sameAsset(asset.selection, selected[index]) || asset.pin_id !== 'draft-asset-' + index ||
      typeof asset.display_name !== 'string' || typeof asset.media_type !== 'string' ||
      !validGeneration(asset.byte_length) || typeof asset.sha256 !== 'string' || !/^[0-9a-f]{64}$/.test(asset.sha256)) {
      throw new Error('Draft receipt stored asset differs');
    }
  });
}

export function validateForkProof(link: DraftForkLink): void {
  if (!link || link.schema_version !== 1 || typeof link.parent_draft_id !== 'string' || !link.parent_draft_id ||
    !validGeneration(link.parent_generation) || link.parent_generation === '0' || link.parent_generation === '18446744073709551615' ||
    typeof link.parent_save_operation !== 'string' || !link.parent_save_operation ||
    typeof link.child_operation !== 'string' || !link.child_operation || link.child_operation === link.parent_save_operation ||
    typeof link.parent_request_sha256 !== 'string' || !/^[0-9a-f]{64}$/.test(link.parent_request_sha256)) {
    throw new Error('Invalid development draft fork proof');
  }
}

// Validate native proof shape and its binding to the full values/inventory.
// Native checks the digest against authoritative canonical protobuf history;
// ETS never treats a digest by itself as storage or capture authority.
export function validateRecord(record: DraftRecord, requireDigest: boolean = false): void {
  const link = record.fork_link;
  const marker = record.fork_retirement;
  const digest = record.request_sha256 ?? '';
  if (link !== undefined && link !== null && typeof link !== 'object' ||
    marker !== undefined && marker !== null && typeof marker !== 'object') {
    throw new Error('Invalid development draft fork relation');
  }
  if (typeof record.active !== 'boolean' || typeof record.current_active !== 'boolean') {
    throw new Error('Invalid draft receipt active state');
  }
  if (record.request_sha256 === null || typeof digest !== 'string' || (requireDigest || !!link || !!marker || digest.length > 0) && !/^[0-9a-f]{64}$/.test(digest)) {
    throw new Error('Invalid draft canonical request digest');
  }
  if (link) {
    validateForkProof(link);
    if (link.parent_draft_id === record.scope.draft_id || !validGeneration(record.generation) || record.generation === '0' ||
      record.generation === '1' && record.operation_id !== link.child_operation ||
      record.scope.source_kind === 1 && record.scope.source !== '' ||
      record.scope.source_kind === 0 && !/^(?:[0-9a-f]{2})+$/.test(record.scope.source)) {
      throw new Error('Development fork receipt binding differs');
    }
  }
  if (marker) {
    validateForkProof(marker.fork_link);
    if (marker.schema_version !== 1 || typeof marker.child_draft_id !== 'string' || !marker.child_draft_id ||
      marker.child_draft_id === record.scope.draft_id || marker.fork_link.parent_draft_id !== record.scope.draft_id ||
      typeof marker.operation_id !== 'string' || !marker.operation_id || marker.operation_id === marker.fork_link.child_operation ||
      marker.operation_id === marker.fork_link.parent_save_operation || record.active ||
      record.generation !== nextGeneration(marker.fork_link.parent_generation) ||
      record.operation_id !== marker.fork_link.parent_save_operation || digest !== marker.fork_link.parent_request_sha256) {
      throw new Error('Development fork retirement receipt differs');
    }
  }
  validateValues(record.values, !!link);
  validateRecordAssets(record);
  if (record.values.assets.some((asset: AssetSelection): boolean => asset.origin === 4) &&
    (!link || record.active && record.generation !== '1' || record.operation_id !== link.child_operation)) {
    throw new Error('Parent pins require the exact first development fork receipt');
  }
}

class DraftTextValuesWire {
  title: TextValue = new TextValue();
  description: TextValue = new TextValue();
  hypothesis: TextValue = new TextValue();
  conclusion: TextValue = new TextValue();
  todos: TextValue = new TextValue();
  category: string = '';
  stage: string = '';
}

function wireValues(values: Values): DraftTextValuesWire {
  const result = new DraftTextValuesWire(); result.title = copyText(values.title);
  result.description = copyText(values.description); result.hypothesis = copyText(values.hypothesis);
  result.conclusion = copyText(values.conclusion); result.todos = copyText(values.todos);
  result.category = values.category; result.stage = values.stage; return result;
}

class DraftWriteWire {
  card_id: string = '';
  draft_id: string = '';
  source_kind: number = 1;
  source_revision: string = '0';
  values: DraftTextValuesWire = new DraftTextValuesWire();
  assets: AssetSelection[] = [];
  expected_generation: string = '0';
  operation_id: string = '';
}

class DraftCommandWire {
  action: string = 'draft_save';
  draft: DraftWriteWire = new DraftWriteWire();
}

export class EditorDraftCoordinator {
  private ownedScope: DraftScope;
  private ownedCurrent: Values;
  private baseline: Values;
  private ownedConfirmed: DraftRecord | undefined = undefined;
  private journalGeneration: string = '0';
  private serialized: string | undefined = undefined;
  private frozen: Values | undefined = undefined;
  private operation: string = '';
  private expected: string = '0';
  private inFlight: Promise<DraftRecord> | undefined = undefined;
  private flushing: Promise<DraftRecord | undefined> | undefined = undefined;
  private timer: number = -1;
  private stopped: boolean = false;
  private paused: boolean = false;
  private forkOriginsPromoted: boolean = false;
  private outcomeUnknown: boolean = false;
  private journalConflict: boolean = false;
  private lastError: string = '';
  private send: (serialized: string) => Promise<DraftRecord>;
  private changed: () => void;
  private operationFactory: () => string;
  private debounce: number;

  constructor(scope: DraftScope, initial: Values, send: (serialized: string) => Promise<DraftRecord>,
    changed: () => void, operationFactory: () => string, restored: DraftRecord | undefined = undefined,
    debounce: number = 500) {
    if (scope.card_id.length === 0 || scope.draft_id.length === 0 || !validGeneration(scope.source_revision) ||
      (scope.source_kind !== 0 && scope.source_kind !== 1) ||
      (scope.source_kind === 0 ? scope.source_revision === '0' : scope.source_revision !== '0') ||
      !Number.isFinite(debounce) || debounce < 0) { throw new Error('Invalid draft scope'); }
    if (restored !== undefined) { validateRecord(restored); }
    validateValues(initial, !!restored?.fork_link);
    this.ownedScope = copyScope(scope); this.ownedCurrent = copyValues(initial); this.baseline = copyValues(initial);
    this.send = send; this.changed = changed; this.operationFactory = operationFactory; this.debounce = debounce;
    if (restored !== undefined) {
      validateRecord(restored);
      if (!sameScope(restored.scope, this.ownedScope) || !validGeneration(restored.generation) ||
        restored.generation === '0' || !validGeneration(restored.current_generation)) {
        throw new Error('Invalid restored draft record');
      }
      if (restored.fork_link && restored.scope.source !== this.ownedScope.source) { throw new Error('Restored development fork source differs'); }
      if (initial.assets.some((asset: AssetSelection): boolean => asset.origin === 4 &&
        !restored.assets.some((pin: StoredAsset): boolean => pin.selection.asset_id === asset.asset_id))) {
        throw new Error('Restored development parent pin is not confirmed');
      }
      this.ownedConfirmed = copyRecord(restored); this.baseline = copyValues(restored.values);
      this.journalGeneration = restored.generation;
      this.journalConflict = !restored.active || !restored.current_active ||
        restored.current_generation !== restored.generation;
      if (this.journalConflict) { this.lastError = 'Draft journal changed'; }
      // Restoring is a read-only action, including a restored new-card draft.
    } else if (scope.source_kind === 1) { this.schedule(); }
  }

  get scope(): DraftScope { return copyScope(this.ownedScope); }
  get current(): Values { return copyValues(this.ownedCurrent); }
  get confirmed(): DraftRecord | undefined {
    return this.ownedConfirmed === undefined ? undefined : copyRecord(this.ownedConfirmed);
  }
  get pending(): string | undefined { return this.serialized; }
  get dirty(): boolean {
    return this.ownedScope.source_kind === 1 && this.ownedConfirmed === undefined ||
      !sameValues(this.ownedCurrent, this.baseline);
  }
  get saving(): boolean { return this.inFlight !== undefined; }
  get unknown(): boolean { return this.outcomeUnknown; }
  get conflicted(): boolean { return this.journalConflict; }
  get error(): string { return this.lastError; }
  get disposed(): boolean { return this.stopped; }
  get writesPaused(): boolean { return this.paused; }

  // Pausing never cancels issued work or replaces complete current raw input.
  pauseWrites(): void { this.requireOpen(); this.paused = true; this.cancelTimer(); this.notify(); }
  resumeWrites(): void { this.requireOpen(); this.paused = false; this.schedule(); this.notify(); }

  update(values: Values): void {
    this.requireOpen(); validateValues(values, !!this.ownedConfirmed?.fork_link);
    const latest = copyValues(values);
    if (this.ownedConfirmed?.fork_link && (this.forkOriginsPromoted || !sameValues(latest, this.ownedCurrent))) {
      this.promoteForkAssets(latest); this.forkOriginsPromoted = true;
    }
    if (sameValues(latest, this.ownedCurrent)) { return; }
    this.ownedCurrent = latest;
    if (!this.outcomeUnknown && !this.journalConflict && !this.saving) { this.lastError = ''; }
    this.schedule(); this.notify();
  }

  // Only an explicit retry may reconcile an uncertain original request. It
  // stops at that receipt; newer input requires its own flush or observation.
  async retry(): Promise<DraftRecord | undefined> {
    this.requireWritable(); this.cancelTimer();
    if (this.journalConflict) { throw new Error('Draft journal changed'); }
    if (this.inFlight !== undefined) { return copyRecord(await this.inFlight); }
    if (!this.outcomeUnknown || this.serialized === undefined || this.frozen === undefined) {
      throw new Error('No uncertain draft request to retry');
    }
    return copyRecord(await this.start(true));
  }

  // Callers may close/submit only after this resolves with the latest values.
  // Issued work is never cancelled when the caller or view stops waiting.
  flush(): Promise<DraftRecord | undefined> {
    if (this.flushing !== undefined) { return this.flushing; }
    this.flushing = this.flushCurrent().finally(() => { this.flushing = undefined; });
    return this.flushing;
  }

  // Explicitly establish an owner for a successful file selection. Calling this
  // is the user's import/save action; merely opening an unchanged card is read-only.
  async ensureConfirmed(): Promise<DraftRecord> {
    this.requireWritable();
    while (true) {
      const existing = await this.flush();
      this.requireOpen();
      if (existing !== undefined && !this.dirty) { return existing; }
      if (this.journalConflict || this.outcomeUnknown) { throw new Error(this.lastError || 'Draft journal is not confirmed'); }
      if (existing === undefined) { await this.start(false); }
    }
  }

  private async flushCurrent(): Promise<DraftRecord | undefined> {
    this.requireWritable(); this.cancelTimer();
    while (true) {
      if (this.inFlight !== undefined) { await this.inFlight; }
      this.requireWritable(); this.cancelTimer();
      if (this.journalConflict) { throw new Error('Draft journal changed'); }
      if (this.outcomeUnknown) { throw new Error(this.lastError || 'Draft save outcome is unknown'); }
      if (!this.dirty) { return this.confirmed; }
      await this.start(false);
    }
  }

  dispose(): void { this.stopped = true; this.cancelTimer(); }

  private requireOpen(): void {
    if (this.stopped) { throw new Error('Draft coordinator is disposed'); }
  }
  private requireWritable(): void {
    this.requireOpen(); if (this.paused) { throw new Error('Draft writes are paused'); }
  }
  private notify(): void { if (!this.stopped) { this.changed(); } }
  private cancelTimer(): void {
    if (this.timer !== -1) { clearTimeout(this.timer); this.timer = -1; }
  }
  private schedule(): void {
    this.cancelTimer();
    if (this.stopped || this.paused || this.outcomeUnknown || this.journalConflict || this.saving || !this.dirty) { return; }
    this.timer = setTimeout(() => {
      this.timer = -1;
      if (this.stopped || this.paused || this.outcomeUnknown || this.journalConflict || this.saving || !this.dirty) { return; }
      // start() stores and publishes the failure before rejecting. Automatic
      // saves expose it through state; explicit flush/retry preserve rejection.
      this.start(false).catch(() => {});
    }, this.debounce);
  }

  private freeze(): void {
    if (this.ownedConfirmed?.fork_link) {
      this.promoteForkAssets(this.ownedCurrent); this.forkOriginsPromoted = true;
    }
    this.frozen = copyValues(this.ownedCurrent); this.operation = this.operationFactory();
    if (this.operation.length === 0) { throw new Error('Empty draft operation'); }
    this.expected = this.journalGeneration;
    nextGeneration(this.expected);
    const command = new DraftCommandWire(); command.draft.card_id = this.ownedScope.card_id;
    command.draft.draft_id = this.ownedScope.draft_id; command.draft.source_kind = this.ownedScope.source_kind;
    command.draft.source_revision = this.ownedScope.source_revision;
    for (const asset of this.frozen.assets) {
      if (asset.origin === 0 && this.ownedScope.source_kind !== 0 || asset.origin === 3 && this.expected === '0') {
        throw new Error('Draft asset has no confirmed source');
      }
    }
    command.draft.values = wireValues(this.frozen);
    command.draft.assets = this.frozen.assets.map((asset: AssetSelection): AssetSelection => copyAsset(asset));
    command.draft.expected_generation = this.expected;
    command.draft.operation_id = this.operation; this.serialized = JSON.stringify(command);
  }

  private start(retry: boolean): Promise<DraftRecord> {
    this.requireWritable();
    if (this.inFlight !== undefined) { return this.inFlight; }
    this.cancelTimer();
    if (!retry) {
      try { this.freeze(); } catch (error) {
        this.serialized = undefined; this.frozen = undefined;
        this.lastError = String(error); this.outcomeUnknown = false; this.notify();
        return Promise.reject(error);
      }
    }
    this.inFlight = this.perform(retry); this.notify(); return this.inFlight;
  }

  private async perform(retry: boolean): Promise<DraftRecord> {
    const wasUnknown = this.outcomeUnknown, frozen = copyValues(this.frozen as Values);
    let scheduleNext = false;
    try {
      const record = await this.send(this.serialized as string);
      this.requireOpen(); validateRecord(record);
      const priorLink = this.ownedConfirmed?.fork_link;
      if (!!priorLink !== !!record.fork_link || priorLink && record.fork_link && !sameForkLink(priorLink, record.fork_link) ||
        priorLink && record.scope.source !== this.ownedScope.source || record.fork_retirement) {
        throw new Error('Draft receipt changed development lineage');
      }
      for (const asset of record.assets ?? []) {
        const prior = this.ownedConfirmed?.assets.find((pin: StoredAsset): boolean => pin.selection.asset_id === asset.selection.asset_id);
        if (prior && (prior.display_name !== asset.display_name || prior.media_type !== asset.media_type ||
          prior.byte_length !== asset.byte_length || prior.sha256 !== asset.sha256)) {
          throw new Error('Draft receipt changed a confirmed asset');
        }
      }
      if (!sameScope(record.scope, this.ownedScope) || record.operation_id !== this.operation ||
        !sameValues(record.values, frozen) || !record.active || !validGeneration(record.generation) ||
        record.generation !== nextGeneration(this.expected) || !validGeneration(record.current_generation)) {
        throw new Error('Draft receipt differs from original request');
      }
      if (!record.current_active || record.current_generation !== record.generation) {
        this.journalConflict = true; throw new Error('Draft journal advanced during save');
      }
      this.ownedConfirmed = copyRecord(record); this.baseline = frozen;
      this.journalGeneration = record.generation; this.serialized = undefined; this.frozen = undefined;
      this.outcomeUnknown = false; this.lastError = ''; scheduleNext = !retry; this.notify();
      return copyRecord(record);
    } catch (error) {
      if (!this.stopped) {
        const rejected = !wasUnknown && error instanceof DraftSaveFailure && error.knownRejected;
        this.lastError = String(error); this.outcomeUnknown = !rejected;
        if (rejected) {
          this.serialized = undefined; this.frozen = undefined;
          // A different S2 observed during S1 may debounce, never the rejected S1.
          scheduleNext = !retry && !sameValues(this.ownedCurrent, frozen);
        }
        this.notify();
      }
      if (error instanceof Error) { throw error; }
      throw new Error(String(error));
    } finally {
      this.inFlight = undefined;
      if (scheduleNext) { this.schedule(); }
      this.notify();
    }
  }

  private promoteForkAssets(values: Values): void {
    for (const selected of values.assets) {
      const confirmed = this.ownedConfirmed?.assets.some((pin: StoredAsset): boolean => pin.selection.asset_id === selected.asset_id);
      if (selected.origin === 4 && !confirmed) {
        throw new Error('Development parent pin is not confirmed');
      }
      if (this.ownedConfirmed?.fork_link && confirmed) {
        selected.origin = 3;
      }
    }
  }
}
