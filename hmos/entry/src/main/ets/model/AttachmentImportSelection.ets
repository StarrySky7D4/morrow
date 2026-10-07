// This coordinator owns only an ephemeral picker selection. The existing
// import request/spool journal and EditorDraftCoordinator own each item.
export const ATTACHMENT_SELECTION_LIMIT: number = 20;

export interface AttachmentImportBoundary {
  editor_id: string;
  card_id: string;
  draft_id: string;
  generation: string;
  edit_epoch: number;
  selected_count: number;
  import_count: number;
  title: string;
  blocked: boolean;
}

export interface AttachmentImportIdentity {
  card_id: string;
  draft_id: string;
  expected_generation: string;
  operation_id: string;
}

// Only the existing per-item adapter may assert selected/retained. It must
// validate the exact import receipt and, for selected, the current pin receipt.
export interface AttachmentImportReceipt {
  outcome: string;
  identity: AttachmentImportIdentity;
  name: string;
  next: AttachmentImportBoundary | undefined;
  error: string;
}

export interface AttachmentImportSelectionState {
  phase: string;
  busy: boolean;
  total: number;
  started: number;
  selected: number;
  retained: number;
  error: string;
}

export interface AttachmentImportSelectionHooks {
  read: () => AttachmentImportBoundary;
  select: (maximum: number) => Promise<string[]>;
  // Establish the existing draft owner only after a non-empty valid selection.
  // It may advance generation, but must not adopt newer user editing values.
  confirm: (boundary: AttachmentImportBoundary) => Promise<AttachmentImportBoundary>;
  // No retry or cleanup is performed by this coordinator. A returned failure
  // or a rejected await stops the remainder; existing journals stay available.
  importOne: (uri: string, identity: AttachmentImportIdentity,
    boundary: AttachmentImportBoundary) => Promise<AttachmentImportReceipt>;
  operation: () => string;
  changed: (state: AttachmentImportSelectionState) => void;
}

function validIdentity(value: string): boolean {
  return typeof value === 'string' && value.length > 0 && value.length <= 256 && value.indexOf('\u0000') < 0;
}

function validGeneration(value: string): boolean {
  return typeof value === 'string' && /^(0|[1-9][0-9]*)$/.test(value) &&
    (value.length < 20 || value.length === 20 && value <= '18446744073709551615');
}

function laterGeneration(a: string, b: string): boolean {
  return a.length > b.length || a.length === b.length && a > b;
}

function copyBoundary(value: AttachmentImportBoundary): AttachmentImportBoundary {
  return { editor_id: value.editor_id, card_id: value.card_id, draft_id: value.draft_id,
    generation: value.generation, edit_epoch: value.edit_epoch, selected_count: value.selected_count,
    import_count: value.import_count, title: value.title, blocked: value.blocked };
}

function validBoundary(value: AttachmentImportBoundary): boolean {
  return !!value && validIdentity(value.editor_id) && validIdentity(value.card_id) && validIdentity(value.draft_id) &&
    validGeneration(value.generation) && Number.isSafeInteger(value.edit_epoch) && value.edit_epoch >= 0 &&
    Number.isInteger(value.selected_count) && value.selected_count >= 0 && value.selected_count <= ATTACHMENT_SELECTION_LIMIT &&
    Number.isInteger(value.import_count) && value.import_count >= 0 && value.import_count <= ATTACHMENT_SELECTION_LIMIT &&
    typeof value.title === 'string' && typeof value.blocked === 'boolean';
}

function sameOwner(a: AttachmentImportBoundary, b: AttachmentImportBoundary): boolean {
  return a.editor_id === b.editor_id && a.card_id === b.card_id && a.draft_id === b.draft_id;
}

function sameBoundary(a: AttachmentImportBoundary, b: AttachmentImportBoundary): boolean {
  return sameOwner(a, b) && a.generation === b.generation && a.edit_epoch === b.edit_epoch &&
    a.selected_count === b.selected_count && a.import_count === b.import_count && a.title === b.title && a.blocked === b.blocked;
}

function copyIdentity(value: AttachmentImportIdentity): AttachmentImportIdentity {
  return { card_id: value.card_id, draft_id: value.draft_id,
    expected_generation: value.expected_generation, operation_id: value.operation_id };
}

function sameIdentity(a: AttachmentImportIdentity, b: AttachmentImportIdentity): boolean {
  return !!a && a.card_id === b.card_id && a.draft_id === b.draft_id &&
    a.expected_generation === b.expected_generation && a.operation_id === b.operation_id;
}

export function attachmentSelectionCapacity(value: AttachmentImportBoundary): number {
  if (!validBoundary(value) || value.blocked) { return 0; }
  return Math.min(ATTACHMENT_SELECTION_LIMIT - value.selected_count,
    ATTACHMENT_SELECTION_LIMIT - value.import_count);
}

// Apply this synchronously to the current Values only after a confirmed import,
// immediately before the existing pin update. It is a proposal, never a receipt
// replacement. A non-empty new user title wins. Avoid half a UTF-16 surrogate.
export function attachmentImportTitle(currentTitle: string, importedName: string): string | undefined {
  if (typeof currentTitle !== 'string' || typeof importedName !== 'string' || currentTitle.trim().length !== 0) {
    return undefined;
  }
  let value: string = importedName.slice(0, 60);
  const last: number = value.charCodeAt(value.length - 1);
  if (last >= 0xd800 && last <= 0xdbff) { value = value.slice(0, -1); }
  return value.trim().length > 0 ? value : undefined;
}

export class AttachmentImportSelection {
  private hooks: AttachmentImportSelectionHooks;
  private state: AttachmentImportSelectionState = { phase: 'idle', busy: false,
    total: 0, started: 0, selected: 0, retained: 0, error: '' };
  private epoch: number = 0;
  private inFlight: Promise<AttachmentImportSelectionState> | undefined = undefined;
  private operations: Set<string> = new Set<string>();

  constructor(hooks: AttachmentImportSelectionHooks) { this.hooks = hooks; }

  snapshot(): AttachmentImportSelectionState {
    return { phase: this.state.phase, busy: this.state.busy, total: this.state.total,
      started: this.state.started, selected: this.state.selected, retained: this.state.retained, error: this.state.error };
  }

  // Stopping only prevents further preparation/admission. An issued picker or
  // import await stays live and its existing owner handles the eventual result.
  stop(): void {
    this.epoch++;
    if (this.inFlight !== undefined) { this.state.phase = 'stopping'; this.notify(); }
  }

  run(): Promise<AttachmentImportSelectionState> {
    if (this.inFlight !== undefined) { return this.inFlight; }
    const epoch: number = ++this.epoch;
    this.state = { phase: 'selecting', busy: true, total: 0, started: 0, selected: 0, retained: 0, error: '' };
    // Store the handle before calling observers, which may synchronously stop
    // or request run again. Neither action may create a second picker.
    this.inFlight = Promise.resolve().then((): Promise<AttachmentImportSelectionState> => this.perform(epoch))
      .finally(() => { this.inFlight = undefined; });
    this.notify();
    return this.inFlight;
  }

  private notify(): void { try { this.hooks.changed(this.snapshot()); } catch (_) {} }

  private finish(phase: string, error: string = ''): AttachmentImportSelectionState {
    this.state.phase = phase; this.state.busy = false; this.state.error = error; this.notify(); return this.snapshot();
  }

  private read(): AttachmentImportBoundary {
    const value: AttachmentImportBoundary = this.hooks.read();
    if (!validBoundary(value)) { throw new Error('附件编辑状态无法确认。'); }
    return copyBoundary(value);
  }

  private current(epoch: number, expected: AttachmentImportBoundary): boolean {
    const current: AttachmentImportBoundary = this.read();
    return this.epoch === epoch && !current.blocked && sameBoundary(current, expected);
  }

  private async perform(epoch: number): Promise<AttachmentImportSelectionState> {
    let issued: boolean = false;
    let confirming: boolean = false;
    try {
      let boundary: AttachmentImportBoundary = this.read();
      const maximum: number = attachmentSelectionCapacity(boundary);
      if (maximum === 0) { return this.finish('stopped', '当前草稿没有可用附件位置。'); }
      const result: string[] = await this.hooks.select(maximum);
      if (!this.current(epoch, boundary)) { return this.finish('stopped', '编辑状态已变化，后续附件未开始导入。'); }
      if (!Array.isArray(result) || result.length > maximum) { return this.finish('failed', '系统返回的附件数量超过本次选择上限。'); }
      if (result.length === 0) { return this.finish('cancelled'); }
      const uris: string[] = Array.from(result);
      if (uris.some((uri: string): boolean => typeof uri !== 'string' || uri.length === 0 || uri.length > 16384 ||
        /[\u0000-\u001f\u007f]/.test(uri)) || new Set<string>(uris).size !== uris.length) {
        return this.finish('failed', '系统返回的附件选择无法确认。');
      }
      confirming = true;
      const confirmed: AttachmentImportBoundary = await this.hooks.confirm(copyBoundary(boundary));
      confirming = false;
      if (!validBoundary(confirmed) || confirmed.blocked || !sameOwner(boundary, confirmed) || confirmed.generation === '0' ||
        confirmed.generation !== boundary.generation && !laterGeneration(confirmed.generation, boundary.generation) ||
        confirmed.edit_epoch !== boundary.edit_epoch || confirmed.selected_count !== boundary.selected_count ||
        confirmed.import_count !== boundary.import_count || confirmed.title !== boundary.title || !this.current(epoch, confirmed)) {
        return this.finish('stopped', '草稿确认期间编辑状态已变化，附件未开始导入。');
      }
      boundary = copyBoundary(confirmed);
      this.state.total = uris.length; this.state.phase = 'importing'; this.notify();
      for (const uri of uris) {
        if (!this.current(epoch, boundary) || attachmentSelectionCapacity(boundary) === 0) {
          return this.finish('stopped', '编辑状态已变化，后续附件未开始导入。');
        }
        const operation: string = this.hooks.operation();
        if (!validIdentity(operation) || this.operations.has(operation)) {
          return this.finish('failed', '本次附件操作身份无法确认。');
        }
        this.operations.add(operation);
        const identity: AttachmentImportIdentity = { card_id: boundary.card_id, draft_id: boundary.draft_id,
          expected_generation: boundary.generation, operation_id: operation };
        // Notify may synchronously close/change the editor; check again before
        // passing even the first provider URI to the per-item adapter.
        this.notify();
        if (!this.current(epoch, boundary)) { return this.finish('stopped', '编辑状态已变化，后续附件未开始导入。'); }
        issued = true;
        this.state.started++;
        const pending: Promise<AttachmentImportReceipt> = this.hooks.importOne(uri, copyIdentity(identity), copyBoundary(boundary));
        this.notify();
        const receipt: AttachmentImportReceipt = await pending;
        issued = false;
        if (!receipt || !sameIdentity(receipt.identity, identity) || typeof receipt.name !== 'string' ||
          typeof receipt.error !== 'string' || !['selected', 'retained', 'failed', 'unknown', 'cancelled'].includes(receipt.outcome)) {
          return this.finish('unknown', '附件回执身份无法确认，原导入记录已保留。');
        }
        if (receipt.outcome === 'selected') { this.state.selected++; }
        else if (receipt.outcome === 'retained') { this.state.retained++; }
        if (this.epoch !== epoch) { return this.finish('stopped', '已停止选择，已开始的附件保留原处理结果。'); }
        if (receipt.outcome !== 'selected') {
          const phase: string = receipt.outcome === 'unknown' ? 'unknown' : receipt.outcome === 'failed' ? 'failed' :
            receipt.outcome === 'cancelled' ? 'cancelled' : 'stopped';
          return this.finish(phase, receipt.error);
        }
        const next: AttachmentImportBoundary | undefined = receipt.next;
        const proposedTitle: string | undefined = attachmentImportTitle(boundary.title, receipt.name);
        const allowedTitle: string = proposedTitle === undefined ? boundary.title : proposedTitle;
        if (!next || !validBoundary(next) || next.blocked || !sameOwner(boundary, next) ||
          !laterGeneration(next.generation, boundary.generation) || next.edit_epoch !== boundary.edit_epoch + 1 ||
          next.selected_count !== boundary.selected_count + 1 || next.import_count !== boundary.import_count + 1 ||
          next.title !== allowedTitle ||
          !this.current(epoch, next)) {
          return this.finish('stopped', '附件已保留，编辑状态已变化，后续附件未开始导入。');
        }
        boundary = copyBoundary(next); this.notify();
      }
      return this.finish('complete');
    } catch (_) {
      // A rejected import await has no trustworthy terminal receipt. In
      // particular it must not be retried under a new operation identity.
      return this.finish(issued || confirming ? 'unknown' : 'failed', issued ?
        '附件导入尚未确认，原请求已保留；后续附件未开始导入。' : confirming ?
          '草稿确认尚未完成，原草稿请求已保留；附件未开始导入。' : '附件选择或准备失败，后续附件未开始导入。');
    }
  }
}
