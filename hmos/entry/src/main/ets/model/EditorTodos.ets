import { TextValue, copyText } from './EditorDraft';
import { samePasteTarget } from './EditorPaste';
import { editorTodoFilteredOld } from './EditorInputPolicy';

export interface EditorTodoFormatResult { value: TextValue; action: string; }
export interface EditorTodosHooks {
  capture: (value: TextValue, owner: string) => number | undefined;
  isCurrent: (owner: string, revision: number, value: TextValue) => boolean;
  count: (text: string, owned: () => boolean) => Promise<number>;
  format: (oldValue: TextValue, newValue: TextValue, remaining: number, owned: () => boolean) => Promise<EditorTodoFormatResult>;
  changed: () => void;
  failed: (owner: string, message: string) => void;
  id: () => string;
}
export class EditorTodoRow {
  id: string = '';
  value: TextValue = new TextValue();
  display_text: string = '';
  incarnation: number = 0;
  pending: boolean = false;
}
export class EditorTodoTicket {
  owner: string = '';
  id: string = '';
  incarnation: number = 0;
}
export class EditorTodoDrag {
  owner: string = '';
  id: string = '';
  version: number = 0;
}
export class EditorTodosView {
  owner: string = '';
  revision: number = 0;
  value: TextValue = new TextValue();
  rows: EditorTodoRow[] = [];
  count: number = -1;
  counting: boolean = false;
  focused_id: string = '';
  error: string = '';
  notice: string = '';
  capture_complete: boolean = true;
}
class TodoInputIntent {
  id: string = '';
  old_value: TextValue = new TextValue();
  new_value: TextValue = new TextValue();
}
function cloneRow(row: EditorTodoRow): EditorTodoRow {
  const result = new EditorTodoRow(); result.id = row.id; result.value = copyText(row.value);
  result.display_text = row.display_text; result.incarnation = row.incarnation; result.pending = row.pending; return result;
}
function composing(value: TextValue): boolean { return value.composing_start !== -1 || value.composing_end !== -1; }
function editingValue(value: TextValue): boolean {
  const offsets: number[] = [value.selection_base, value.selection_extent, value.composing_start, value.composing_end];
  return typeof value.text === 'string' && offsets.every((n: number): boolean => Number.isInteger(n) && n >= -1 && n <= value.text.length) &&
    (value.affinity === 0 || value.affinity === 1) && typeof value.directional === 'boolean' &&
    (value.composing_start === -1 && value.composing_end === -1 ||
      value.composing_start >= 0 && value.composing_end >= value.composing_start);
}

// Presentation row IDs never leave this model, enter a raw journal, or become
// business TaskIds. The only persisted value is the full newline TextValue.
export class EditorTodosDraft {
  private hooks: EditorTodosHooks;
  private owner: string = '';
  private revision: number = 0;
  private version: number = 0;
  private incarnation: number = 0;
  private rows: EditorTodoRow[] = [];
  private raw: TextValue = new TextValue();
  private focusId: string = '';
  private countValue: number = -1;
  private countPending: boolean = false;
  private error: string = '';
  private notice: string = '';
  private complete: boolean = true;
  private pending: TodoInputIntent | undefined = undefined;
  private countCache: Map<string, number> = new Map<string, number>();
  private issuedIds: Set<string> = new Set<string>();
  private active: boolean = false;
  constructor(hooks: EditorTodosHooks) { this.hooks = hooks; }

  bind(owner: string, revision: number, value: TextValue): void {
    if (!owner || !Number.isSafeInteger(revision) || revision < 0 || !editingValue(value)) { throw new Error('待办草稿身份或文字位置无法确认。'); }
    if (this.active && owner === this.owner && revision === this.revision && samePasteTarget(value, this.raw)) { return; }
    const sameOwner: boolean = owner === this.owner;
    const textChanged: boolean = !sameOwner || value.text !== this.raw.text;
    this.version++; this.active = true; this.owner = owner; this.revision = revision; this.raw = copyText(value);
    this.pending = undefined; this.complete = true; this.error = ''; this.notice = '';
    if (textChanged) {
      const previous: EditorTodoRow[] = sameOwner ? this.rows : [];
      const lines: string[] = value.text === '' ? [] : value.text.split('\n');
      this.rows = lines.map((text: string, index: number): EditorTodoRow => {
        const row = new EditorTodoRow(); row.id = previous[index]?.id || this.newId();
        row.incarnation = ++this.incarnation; row.value.text = text; row.display_text = text; return row;
      });
      this.focusId = ''; if (!sameOwner) { this.countCache.clear(); }
    } else {
      for (const row of this.rows) {
        row.pending = false; row.incarnation = ++this.incarnation; row.display_text = row.value.text;
        row.value.selection_base = -1; row.value.selection_extent = -1;
        row.value.composing_start = -1; row.value.composing_end = -1;
      }
    }
    this.mapExternalSelection(value);
    if (composing(value)) { this.notice = '原输入法候选已保留，请重新完成输入后保存。'; }
    this.hooks.changed(); this.refreshCount();
  }
  stop(): void {
    this.active = false; this.version++; this.pending = undefined;
    for (const row of this.rows) { row.pending = false; } this.countPending = false; this.hooks.changed();
  }
  view(): EditorTodosView {
    const result = new EditorTodosView(); result.owner = this.owner; result.revision = this.revision; result.value = copyText(this.raw);
    result.rows = this.rows.map((row: EditorTodoRow): EditorTodoRow => cloneRow(row)); result.count = this.countValue;
    result.counting = this.countPending; result.focused_id = this.focusId; result.error = this.error;
    result.notice = this.notice; result.capture_complete = this.complete; return result;
  }
  row(id: string): EditorTodoRow | undefined { const row = this.rows.find((item: EditorTodoRow): boolean => item.id === id); return row ? cloneRow(row) : undefined; }
  ticket(id: string): EditorTodoTicket {
    const row = this.rows.find((item: EditorTodoRow): boolean => item.id === id); if (!row) { throw new Error('待办行已变化。'); }
    const ticket = new EditorTodoTicket(); ticket.owner = this.owner; ticket.id = row.id; ticket.incarnation = row.incarnation; return ticket;
  }
  private admitted(ticket: EditorTodoTicket): EditorTodoRow | undefined {
    if (!this.active || ticket.owner !== this.owner || !this.hooks.isCurrent(this.owner, this.revision, copyText(this.raw))) { return undefined; }
    return this.rows.find((row: EditorTodoRow): boolean => row.id === ticket.id && row.incarnation === ticket.incarnation);
  }
  private newId(): string {
    const id: string = this.hooks.id();
    if (!id || !/^[A-Za-z0-9_-]{1,128}$/.test(id) || this.issuedIds.has(id)) { throw new Error('待办行标识无法确认。'); }
    this.issuedIds.add(id); return id;
  }
  private offset(id: string): number {
    let offset: number = 0; for (const row of this.rows) { if (row.id === id) { return offset; } offset += row.value.text.length + 1; }
    throw new Error('待办行已变化。');
  }
  private aggregate(id: string = ''): TextValue {
    const result = new TextValue(); result.text = this.rows.map((row: EditorTodoRow): string => row.value.text).join('\n');
    if (id) {
      const row = this.rows.find((item: EditorTodoRow): boolean => item.id === id); if (!row) { throw new Error('待办行已变化。'); }
      const offset: number = this.offset(id); result.affinity = row.value.affinity; result.directional = row.value.directional;
      result.selection_base = row.value.selection_base < 0 ? -1 : offset + row.value.selection_base;
      result.selection_extent = row.value.selection_extent < 0 ? -1 : offset + row.value.selection_extent;
      result.composing_start = row.value.composing_start < 0 ? -1 : offset + row.value.composing_start;
      result.composing_end = row.value.composing_end < 0 ? -1 : offset + row.value.composing_end;
    }
    return result;
  }
  private mapExternalSelection(value: TextValue): void {
    let offset: number = 0;
    for (const row of this.rows) {
      const end: number = offset + row.value.text.length;
      if (value.selection_base >= offset && value.selection_base <= end && value.selection_extent >= offset && value.selection_extent <= end) {
        row.value.selection_base = value.selection_base - offset; row.value.selection_extent = value.selection_extent - offset;
        row.value.affinity = value.affinity; row.value.directional = value.directional;
      }
      if (value.composing_start >= offset && value.composing_end <= end && value.composing_start >= 0) {
        row.value.composing_start = value.composing_start - offset; row.value.composing_end = value.composing_end - offset;
      }
      offset = end + 1;
    }
  }
  private emit(value: TextValue): boolean {
    let revision: number | undefined = undefined;
    try { revision = this.hooks.capture(copyText(value), this.owner); } catch (_) { /* Parent raw remains the authority. */ }
    if (revision === undefined || !Number.isSafeInteger(revision) || revision < this.revision) {
      const source: TextValue = copyText(this.raw), owner: string = this.owner, previous: number = this.revision;
      // A rejected parent capture must not leave a different local aggregate.
      this.rows = []; this.owner = ''; this.bind(owner, previous, source);
      this.fail('待办目标已变化，原始输入未确认。'); return false;
    }
    this.raw = copyText(value); this.revision = revision; this.version++; this.hooks.changed(); this.refreshCount(); return true;
  }
  private guard(version: number, revision: number, value: TextValue): () => boolean {
    const owner: string = this.owner;
    return (): boolean => this.active && owner === this.owner && version === this.version && revision === this.revision &&
      samePasteTarget(value, this.raw) && this.hooks.isCurrent(owner, revision, copyText(value));
  }
  private async counted(text: string, owned: () => boolean): Promise<number> {
    if (!owned()) { throw new Error('待办目标已变化。'); }
    const cached: number | undefined = this.countCache.get(text);
    if (cached !== undefined) { return cached; }
    const count: number = await this.hooks.count(text, owned);
    if (!owned() || !Number.isSafeInteger(count) || count < 0) { throw new Error('待办字数未确认，完整输入已保留。'); }
    if (this.countCache.size >= 128) { this.countCache.clear(); } this.countCache.set(text, count); return count;
  }
  private async refreshCount(): Promise<void> {
    const value: TextValue = copyText(this.raw), owned = this.guard(this.version, this.revision, value);
    this.countPending = true; this.countValue = -1; this.hooks.changed();
    try { const count: number = await this.counted(value.text, owned); if (owned()) { this.countValue = count; } }
    catch (_) { if (owned()) { this.countValue = -1; } }
    finally { if (owned()) { this.countPending = false; this.hooks.changed(); } }
  }
  private async remaining(id: string, owned: () => boolean): Promise<number> {
    let used: number = this.rows.length - 1;
    const texts: string[] = this.rows.filter((row: EditorTodoRow): boolean => row.id !== id).map((row: EditorTodoRow): string => row.value.text);
    for (const text of texts) { used += await this.counted(text, owned); }
    return Math.max(0, Math.min(1000, 1000 - used));
  }
  private fail(message: string): void { this.error = message; this.complete = false; this.hooks.failed(this.owner, message); this.hooks.changed(); }

  async input(ticket: EditorTodoTicket, text: string, previewValue: string = '', previewOffset: number = -1): Promise<void> {
    const row = this.admitted(ticket); if (!row) { return; }
    const previous = this.pending;
    const oldValue: TextValue = copyText(previous?.id === row.id ? previous.old_value : row.value), next: TextValue = copyText(row.value);
    for (const item of this.rows) { item.pending = false; }
    if (previewValue && (!Number.isInteger(previewOffset) || previewOffset < 0 || previewOffset > text.length)) {
      row.display_text = text; this.version++; this.pending = undefined; row.pending = false;
      this.fail('输入法候选位置无法确认，请完成输入后再保存；旧草稿未替换。'); return;
    }
    next.text = previewValue ? text.slice(0, previewOffset) + previewValue + text.slice(previewOffset) : text;
    next.composing_start = previewValue ? previewOffset : -1; next.composing_end = previewValue ? previewOffset + previewValue.length : -1;
    if (next.selection_base > next.text.length) { next.selection_base = -1; }
    if (next.selection_extent > next.text.length) { next.selection_extent = -1; }
    row.display_text = text; row.value = next; this.focusId = row.id; this.error = ''; this.notice = ''; this.complete = true;
    if (!this.emit(this.aggregate(row.id))) { return; }
    if (previous && previous.id !== row.id) {
      this.pending = undefined; this.fail('前一行仍在检查，完整输入已保留，请重新核对待办草稿。'); return;
    }
    if (composing(next)) {
      row.pending = false; this.pending = undefined; this.notice = '输入法候选已保留，完成输入后继续检查。'; this.hooks.changed(); return;
    }
    const intent = new TodoInputIntent(); intent.id = row.id; intent.old_value = oldValue; intent.new_value = copyText(next);
    this.pending = intent; this.complete = false; await this.formatIntent(intent);
  }
  private async formatIntent(intent: TodoInputIntent): Promise<void> {
    const row = this.rows.find((item: EditorTodoRow): boolean => item.id === intent.id); if (!row) { return; }
    const owned = this.guard(this.version, this.revision, copyText(this.raw)); row.pending = true; this.hooks.changed();
    try {
      const limit: number = await this.remaining(row.id, owned);
      const result: EditorTodoFormatResult = await this.hooks.format(copyText(intent.old_value), copyText(intent.new_value), limit, owned);
      if (!owned() || this.pending !== intent) { return; }
      if (!result || !['accepted', 'retained', 'truncated'].includes(result.action) || !editingValue(result.value) ||
        composing(result.value) || result.action === 'retained' &&
          !samePasteTarget(result.value, limit === 0 ? editorTodoFilteredOld(intent.old_value) : intent.old_value)) {
        throw new Error('待办行格式回执无法确认，完整输入已保留。');
      }
      row.value = copyText(result.value); row.display_text = result.value.text; row.pending = false; this.pending = undefined;
      this.notice = result.action === 'truncated' ? '已按剩余字数保留可见字符，请核对本行。' :
        result.action === 'retained' ? '本行已达剩余字数上限，仍保留原内容。' : '';
      this.complete = true; this.emit(this.aggregate(row.id));
    } catch (error) {
      if (owned() && this.pending === intent) { row.pending = false; this.pending = undefined; this.fail(String(error)); }
    }
  }
  async selection(ticket: EditorTodoTicket, base: number, extent: number): Promise<void> {
    const row = this.admitted(ticket); if (!row || !Number.isInteger(base) || !Number.isInteger(extent) ||
      base < -1 || extent < -1 || base > row.value.text.length || extent > row.value.text.length) { return; }
    row.value.selection_base = base; row.value.selection_extent = extent; this.focusId = row.id;
    const pending = this.pending; if (!this.emit(this.aggregate(row.id))) { return; }
    if (pending && pending.id === row.id && !composing(row.value)) {
      const next = new TodoInputIntent(); next.id = pending.id; next.old_value = copyText(pending.old_value); next.new_value = copyText(row.value);
      this.pending = next; await this.formatIntent(next);
    }
  }
  focus(ticket: EditorTodoTicket): void { const row = this.admitted(ticket); if (row) { this.focusId = row.id; this.hooks.changed(); } }
  canChangeRows(): boolean {
    return this.active && !!this.owner && this.hooks.isCurrent(this.owner, this.revision, copyText(this.raw)) &&
      !this.pending && this.complete && !this.rows.some((row: EditorTodoRow): boolean => composing(row.value));
  }
  canEdit(id: string): boolean { return !this.pending || this.pending.id === id; }
  async add(): Promise<string | undefined> {
    if (!this.owner || !this.canChangeRows() || this.rows.length >= 100) { return undefined; }
    const value: TextValue = copyText(this.raw), owned = this.guard(this.version, this.revision, value);
    const count: number = await this.counted(value.text, owned); if (!owned() || count >= 1000) { return undefined; }
    const future: string = this.rows.length ? value.text + '\n' : '';
    if (await this.counted(future, owned) > 1000 || !owned()) { return undefined; }
    const row = new EditorTodoRow(); row.id = this.newId(); row.incarnation = ++this.incarnation;
    this.rows.push(row); this.focusId = row.id; return this.emit(this.aggregate()) ? row.id : undefined;
  }
  remove(ticket: EditorTodoTicket): void {
    if (!this.admitted(ticket) || !this.canChangeRows()) { return; }
    this.rows = this.rows.filter((row: EditorTodoRow): boolean => row.id !== ticket.id);
    if (this.focusId === ticket.id) { this.focusId = ''; } this.emit(this.aggregate());
  }
  move(ticket: EditorTodoTicket, delta: number): void {
    if (!this.admitted(ticket) || !this.canChangeRows() || delta !== -1 && delta !== 1) { return; }
    const index: number = this.rows.findIndex((row: EditorTodoRow): boolean => row.id === ticket.id), target: number = index + delta;
    if (target < 0 || target >= this.rows.length) { return; }
    const row: EditorTodoRow = this.rows.splice(index, 1)[0]; this.rows.splice(target, 0, row); this.emit(this.aggregate());
  }
  beginDrag(ticket: EditorTodoTicket): EditorTodoDrag | undefined {
    if (!this.admitted(ticket) || !this.canChangeRows()) { return undefined; }
    const drag = new EditorTodoDrag(); drag.owner = this.owner; drag.id = ticket.id; drag.version = this.version; return drag;
  }
  ownsDrag(drag: EditorTodoDrag): boolean {
    return drag.owner === this.owner && drag.version === this.version && this.canChangeRows() &&
      this.rows.some((row: EditorTodoRow): boolean => row.id === drag.id);
  }
  drop(drag: EditorTodoDrag, target: EditorTodoTicket, after: boolean): void {
    if (!this.ownsDrag(drag) || !this.admitted(target) || drag.id === target.id) { return; }
    const source: number = this.rows.findIndex((row: EditorTodoRow): boolean => row.id === drag.id); if (source < 0) { return; }
    const row: EditorTodoRow = this.rows.splice(source, 1)[0];
    const destination: number = this.rows.findIndex((item: EditorTodoRow): boolean => item.id === target.id);
    this.rows.splice(destination + (after ? 1 : 0), 0, row); this.emit(this.aggregate());
  }
}
