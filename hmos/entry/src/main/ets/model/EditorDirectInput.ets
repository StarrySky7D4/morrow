import { TextValue, copyText } from './EditorDraft';
import { samePasteTarget } from './EditorPaste';
import { EditorInputFormat } from './EditorInputPolicy';
import { editorFieldLimit } from './EditorFieldPolicy';

export class EditorDirectInputProposal {
  key: string = '';
  field: string = '';
  owner: string = '';
  revision: number = 0;
  expected: TextValue = new TextValue();
  value: TextValue = new TextValue();
  action: string = '';
  format_applied: boolean = false;
  grapheme_count: number = -1;
  limit: number = 0;
}
export interface EditorDirectInputHooks {
  isCurrent: (owner: string, key: string, revision: number, value: TextValue) => boolean;
  format: (field: string, oldValue: TextValue, newValue: TextValue, owned: () => boolean) => Promise<EditorInputFormat>;
  // Root checks owned() immediately before updating the complete raw value and
  // its control. Return the new field revision only after that synchronous
  // capture. Controller selection restoration has its own later owner guard.
  apply: (proposal: EditorDirectInputProposal, owned: () => boolean) => number | undefined;
  changed: () => void;
}
export class EditorDirectInputView {
  key: string = '';
  field: string = '';
  owner: string = '';
  revision: number = 0;
  value: TextValue = new TextValue();
  phase: string = 'stopped';
  pending: boolean = false;
  preview_active: boolean = false;
  error: string = '';
  notice: string = '';
  action: string = '';
  // -1 when only a different, unapplied proposal has been counted. Root keeps
  // its independent full-raw field counter; proposal counts never label raw.
  grapheme_count: number = -1;
}
class DirectInputSlot {
  key: string = '';
  field: string = '';
  owner: string = '';
  revision: number = 0;
  version: number = 0;
  value: TextValue = new TextValue();
  preview: boolean = false;
  old: TextValue | undefined = undefined;
  phase: string = 'ready';
  error: string = '';
  notice: string = '';
  action: string = '';
  count: number = -1;
}
function composing(value: TextValue): boolean { return value.composing_start !== -1 || value.composing_end !== -1; }
function fullValue(value: TextValue): boolean {
  if (!value || typeof value.text !== 'string' || typeof value.directional !== 'boolean' || (value.affinity !== 0 && value.affinity !== 1)) { return false; }
  for (const offset of [value.selection_base, value.selection_extent, value.composing_start, value.composing_end]) {
    if (!Number.isInteger(offset) || offset < -1 || offset > value.text.length) { return false; }
  }
  return !(value.composing_start >= 0 && value.composing_end >= 0 && value.composing_start > value.composing_end);
}
function sameComposition(a: TextValue, b: TextValue): boolean {
  return a.composing_start === b.composing_start && a.composing_end === b.composing_end;
}

// Root captures complete raw first, then calls capture(). Only the injected
// native policy formats text: this coordinator does not segment, trim, save or
// replace active IME candidates. A formatter-ready field is not a save grant.
export class EditorDirectInput {
  private hooks: EditorDirectInputHooks;
  private slots: Map<string, DirectInputSlot> = new Map<string, DirectInputSlot>();
  private active: boolean = true;
  private epoch: number = 0;
  constructor(hooks: EditorDirectInputHooks) { this.hooks = hooks; }

  // Bind on attachment/restoration or a separately validated external edit.
  // Rebinding a pending input is an explicit handoff, not worker acceptance.
  bind(key: string, field: string, owner: string, revision: number, value: TextValue, preview: boolean = false): void {
    editorFieldLimit(field);
    if (!key || !owner || !Number.isSafeInteger(revision) || revision < 0 || !fullValue(value)) {
      throw new Error('完整输入或编辑目标无法确认，原输入仍保留。');
    }
    const previous = this.slots.get(key);
    if (this.active && previous && previous.phase !== 'stopped' && previous.owner === owner && previous.field === field && previous.revision === revision &&
      previous.preview === preview && samePasteTarget(previous.value, value)) { return; }
    if (previous) { previous.version++; }
    const slot = new DirectInputSlot(); slot.key = key; slot.field = field; slot.owner = owner; slot.revision = revision;
    slot.value = copyText(value); slot.preview = preview;
    if (preview || composing(value)) { slot.phase = 'deferred'; slot.notice = '完整输入法候选已保留，完成输入后继续检查。'; }
    this.active = true; this.slots.set(key, slot); this.hooks.changed();
  }
  unbind(key: string): void {
    const slot = this.slots.get(key); if (slot) { slot.version++; this.slots.delete(key); this.hooks.changed(); }
  }
  stop(): void {
    this.active = false; this.epoch++;
    for (const slot of this.slots.values()) { slot.version++; slot.old = undefined; slot.phase = 'stopped'; }
    this.hooks.changed();
  }
  view(key: string): EditorDirectInputView | undefined {
    const slot = this.slots.get(key); if (!slot) { return undefined; }
    const view = new EditorDirectInputView(); view.key = slot.key; view.field = slot.field; view.owner = slot.owner;
    view.revision = slot.revision; view.value = copyText(slot.value); view.phase = slot.phase;
    view.pending = ['queued', 'pending', 'applying'].includes(slot.phase); view.preview_active = slot.preview || composing(slot.value);
    view.error = slot.error; view.notice = slot.notice; view.action = slot.action; view.grapheme_count = slot.count; return view;
  }
  private current(slot: DirectInputSlot): boolean {
    if (!this.active || this.slots.get(slot.key) !== slot) { return false; }
    try { return this.hooks.isCurrent(slot.owner, slot.key, slot.revision, copyText(slot.value)); } catch (_) { return false; }
  }
  canConfirm(key: string = ''): boolean {
    const slots: DirectInputSlot[] = key ? (this.slots.has(key) ? [this.slots.get(key)!] : []) : Array.from(this.slots.values());
    return slots.length > 0 && slots.every((slot: DirectInputSlot): boolean => this.current(slot) && slot.phase === 'ready' &&
      !slot.preview && !composing(slot.value) && fullValue(slot.value));
  }
  // revision belongs to this field, never a page-wide worker completion clock.
  // Selection callbacks while an edit is pending keep its original old value;
  // otherwise an onChange followed by onSelection could bypass truncation by
  // being submitted as a same-text selection-only update.
  capture(key: string, owner: string, revision: number, value: TextValue, preview: boolean = false): boolean {
    const slot = this.slots.get(key);
    if (!this.active || !slot || slot.owner !== owner || !Number.isSafeInteger(revision) || revision < slot.revision) { return false; }
    try { if (!this.hooks.isCurrent(owner, key, revision, copyText(value))) { return false; } } catch (_) { return false; }
    if (revision === slot.revision) { return slot.preview === preview && samePasteTarget(slot.value, value); }
    const previous = copyText(slot.value);
    const selectionOnly = previous.text === value.text && sameComposition(previous, value) && slot.preview === preview;
    slot.old = copyText(selectionOnly && slot.old ? slot.old : previous);
    slot.value = copyText(value); slot.revision = revision; slot.preview = preview; slot.version++;
    slot.error = ''; slot.notice = ''; slot.action = ''; slot.count = -1;
    if (!fullValue(value)) { this.failed(slot, '完整输入位置无法确认，原输入仍保留。'); return true; }
    this.queue(slot); return true;
  }
  retry(key: string): boolean {
    const slot = this.slots.get(key);
    if (!slot || slot.phase !== 'failed' || !slot.old || !this.current(slot)) { return false; }
    slot.version++; slot.error = ''; this.queue(slot); return true;
  }
  private failed(slot: DirectInputSlot, message: string): void {
    slot.phase = 'failed'; slot.error = message; slot.notice = '完整原输入已保留，确认前请重新检查。'; this.hooks.changed();
  }
  private queue(slot: DirectInputSlot): void {
    const version = slot.version, epoch = this.epoch;
    slot.phase = 'queued'; this.hooks.changed();
    // Coalesce only within the current event turn; subsequent selection events
    // revoke the previous owned() and reissue a pure read against the latest raw.
    Promise.resolve().then((): Promise<void> => this.run(slot, version, epoch));
  }
  private async run(slot: DirectInputSlot, version: number, epoch: number): Promise<void> {
    const owned = (): boolean => epoch === this.epoch && version === slot.version && this.current(slot);
    if (!owned() || !slot.old) { return; }
    const old = copyText(slot.old), next = copyText(slot.value), revision = slot.revision;
    slot.phase = 'pending'; this.hooks.changed();
    try {
      const result: EditorInputFormat = await this.hooks.format(slot.field, old, next, owned);
      if (!owned()) { return; }
      if (!result || !fullValue(result.value) || !['accepted', 'retained', 'truncated'].includes(result.action) ||
        typeof result.format_applied !== 'boolean' || result.limit !== editorFieldLimit(slot.field) ||
        !Number.isSafeInteger(result.grapheme_count) || result.grapheme_count < 0) {
        throw new Error('完整输入规则回执无法确认，原输入仍保留。');
      }
      slot.action = result.action; slot.count = samePasteTarget(result.value, next) ? result.grapheme_count : -1;
      if (slot.preview || composing(next)) {
        slot.phase = 'deferred'; slot.notice = '已检查完整候选，完成输入后再采用输入规则。'; this.hooks.changed(); return;
      }
      if (composing(result.value)) {
        throw new Error('控件无法恢复原输入法候选，当前完整输入仍保留，请重新完成输入。');
      }
      if (samePasteTarget(result.value, next)) {
        slot.old = undefined; slot.phase = 'ready'; slot.notice = ''; this.hooks.changed(); return;
      }
      const proposal = new EditorDirectInputProposal(); proposal.key = slot.key; proposal.field = slot.field;
      proposal.owner = slot.owner; proposal.revision = revision; proposal.expected = copyText(next); proposal.value = copyText(result.value);
      proposal.action = result.action; proposal.format_applied = result.format_applied;
      proposal.grapheme_count = result.grapheme_count; proposal.limit = result.limit;
      // Do not let a hook's mutations change the independently frozen proposal.
      const adopted = copyText(result.value);
      slot.phase = 'applying'; this.hooks.changed(); if (!owned()) { return; }
      const adoptedRevision = this.hooks.apply(proposal, owned);
      if (epoch !== this.epoch || version !== slot.version || this.slots.get(slot.key) !== slot || !this.active) { return; }
      if (adoptedRevision === undefined || !Number.isSafeInteger(adoptedRevision) || adoptedRevision <= revision ||
        !this.hooks.isCurrent(slot.owner, slot.key, adoptedRevision, copyText(adopted))) {
        throw new Error('输入规则尚未被完整采用，原输入及处理建议仍保留。');
      }
      slot.value = adopted; slot.revision = adoptedRevision; slot.version++; slot.old = undefined; slot.phase = 'ready';
      slot.count = result.grapheme_count;
      slot.notice = result.action === 'truncated' ? '已按字数保留可见字符，请核对输入。' :
        result.action === 'retained' ? '已达字数上限，保留原内容。' : ''; this.hooks.changed();
    } catch (error) {
      // apply may have changed Root's raw before throwing. Never replace it here;
      // the failed local receipt blocks confirmation until an explicit rebind.
      if (epoch === this.epoch && version === slot.version && this.slots.get(slot.key) === slot && this.active) {
        this.failed(slot, String(error));
      }
    }
  }
}
