import { TextValue, copyText } from './EditorDraft';
import { samePasteTarget } from './EditorPaste';
import { editorFieldLimit } from './EditorFieldPolicy';

export interface EditorInputFormat {
  value: TextValue;
  action: string;
  format_applied: boolean;
  grapheme_count: number;
  limit: number;
}
interface InputRequest { field: string; mode: string; old_value: TextValue; new_value: TextValue; limit?: number; }
interface InputReply {
  ok: boolean; error: string; field: string; mode: string; limit: number;
  action: string; format_applied: boolean; old_value: TextValue | null; new_value: TextValue | null; value: TextValue | null;
  old_grapheme_count: number; old_utf16_length: number; old_utf8_length: number;
  new_grapheme_count: number; new_utf16_length: number; new_utf8_length: number;
  grapheme_count: number; utf16_length: number; utf8_length: number;
  unicode_version: string; request_sha256: string;
}
function bytes(text: string): number {
  let length: number = 0;
  for (let i: number = 0; i < text.length; i++) {
    const unit = text.charCodeAt(i);
    if (unit >= 0xD800 && unit <= 0xDBFF) {
      const next = text.charCodeAt(++i);
      if (!(next >= 0xDC00 && next <= 0xDFFF)) { throw new Error('输入包含未完成的字符，原输入仍保留。'); }
      length += 4;
    } else if (unit >= 0xDC00 && unit <= 0xDFFF) { throw new Error('输入包含未完成的字符，原输入仍保留。'); }
    else { length += unit < 0x80 ? 1 : unit < 0x800 ? 2 : 3; }
  }
  return length;
}
function validValue(value: TextValue): boolean {
  if (!value || typeof value.text !== 'string' || typeof value.directional !== 'boolean' || (value.affinity !== 0 && value.affinity !== 1)) { return false; }
  for (const offset of [value.selection_base, value.selection_extent, value.composing_start, value.composing_end]) {
    if (!Number.isInteger(offset) || offset < -1 || offset > value.text.length) { return false; }
  }
  return !(value.composing_start >= 0 && value.composing_end >= 0 && value.composing_start > value.composing_end);
}
function completeCount(value: TextValue, count: number, units: number, utf8: number): boolean {
  return validValue(value) && Number.isSafeInteger(count) && count >= 0 && count <= value.text.length &&
    (value.text.length === 0 ? count === 0 : count > 0) && units === value.text.length && utf8 === bytes(value.text);
}

// Receipt validation only: a zero-budget row can retain the old value before
// Flutter's CR/LF filter finalizes its selection and composing metadata.
export function editorTodoFilteredOld(value: TextValue): TextValue {
  const filtered = copyText(value);
  filtered.text = value.text.replace(/[\r\n]/g, ' ');
  if (value.selection_base < 0 || value.selection_extent < 0) {
    filtered.selection_base = -1; filtered.selection_extent = -1;
    filtered.affinity = 1; filtered.directional = false;
  }
  if (value.composing_start < 0 || value.composing_end < 0 || value.composing_start === value.composing_end) {
    filtered.composing_start = -1; filtered.composing_end = -1;
  }
  return filtered;
}

// A pure formatter proposes an editing value; it never authorizes a business
// save or writes a draft. Root retains the raw input before invoking it.
export class EditorInputPolicy {
  private send: (request: string) => Promise<string>;
  private hash: (request: string) => Promise<string>;
  private epoch: number = 0;
  constructor(send: (request: string) => Promise<string>, hash: (request: string) => Promise<string>) { this.send = send; this.hash = hash; }
  stop(): void { this.epoch++; }
  async format(field: string, oldValue: TextValue, newValue: TextValue, stillOwned: () => boolean,
    remaining?: number): Promise<EditorInputFormat> {
    const old = copyText(oldValue), next = copyText(newValue), epoch = this.epoch;
    const mode = remaining === undefined ? 'field' : 'todo_row';
    const limit = remaining === undefined ? editorFieldLimit(field) : remaining;
    if (!validValue(old) || !validValue(next) || (mode === 'todo_row' && (field !== 'todos' || !Number.isInteger(limit) || limit < 0 || limit > 1000))) {
      throw new Error('完整输入或待办剩余字数无法确认，原输入仍保留。');
    }
    bytes(old.text); bytes(next.text);
    const owned = (): boolean => epoch === this.epoch && stillOwned() && samePasteTarget(old, oldValue) && samePasteTarget(next, newValue);
    if (!owned()) { throw new Error('编辑目标已变化，输入规则未采用。'); }
    const request: InputRequest = { field: field, mode: mode, old_value: old, new_value: next };
    if (remaining !== undefined) { request.limit = limit; }
    const encoded = JSON.stringify(request);
    if (bytes(encoded) > 512 * 1024) { throw new Error('完整输入超过处理容量，原输入仍保留。'); }
    const sha = (await this.hash(encoded)).toLowerCase();
    if (!owned() || !/^[a-f0-9]{64}$/.test(sha)) { throw new Error('完整输入身份无法确认，原输入仍保留。'); }
    const raw = await this.send(encoded);
    if (!owned()) { throw new Error('编辑目标已变化，晚到的输入规则未采用。'); }
    if (bytes(raw) > 512 * 1024) { throw new Error('完整输入回执超过处理容量，原输入仍保留。'); }
    const reply: InputReply = JSON.parse(raw) as InputReply;
    if (!reply || reply.field !== field || reply.mode !== mode || reply.limit !== limit || reply.unicode_version !== '16.0.0' ||
      reply.request_sha256 !== sha || typeof reply.ok !== 'boolean' || typeof reply.error !== 'string' || typeof reply.format_applied !== 'boolean') {
      throw new Error('完整输入回执身份无法确认，原输入仍保留。');
    }
    if (!reply.ok) { throw new Error('输入规则未确认，原输入仍保留：' + reply.error); }
    const value = reply.value, echoedOld = reply.old_value, echoedNew = reply.new_value;
    if (reply.error || !value || !echoedOld || !echoedNew || !validValue(value) ||
      !samePasteTarget(old, echoedOld) || !samePasteTarget(next, echoedNew) ||
      !completeCount(old, reply.old_grapheme_count, reply.old_utf16_length, reply.old_utf8_length) ||
      !completeCount(next, reply.new_grapheme_count, reply.new_utf16_length, reply.new_utf8_length) ||
      !completeCount(value, reply.grapheme_count, reply.utf16_length, reply.utf8_length) ||
      !['accepted', 'retained', 'truncated'].includes(reply.action)) { throw new Error('完整输入回执不完整，原输入仍保留。'); }
    if ((!reply.format_applied && (reply.action !== 'accepted' || !samePasteTarget(value, next))) ||
      (reply.action === 'retained' && (!reply.format_applied || !samePasteTarget(value,
        mode === 'todo_row' && limit === 0 ? editorTodoFilteredOld(old) : old))) ||
      (reply.action === 'accepted' && mode === 'field' && !samePasteTarget(value, next)) ||
      (reply.action === 'accepted' && mode === 'field' && reply.format_applied && reply.grapheme_count > limit) ||
      (reply.action === 'accepted' && mode === 'todo_row' && !samePasteTarget(value,
        reply.format_applied ? editorTodoFilteredOld(next) : next)) ||
      (reply.action === 'accepted' && mode === 'todo_row' && reply.format_applied &&
        (limit > 0 ? reply.grapheme_count > limit : reply.new_grapheme_count > reply.old_grapheme_count)) ||
      (reply.action === 'truncated' && (!reply.format_applied || reply.grapheme_count !== limit ||
        !(mode === 'todo_row' ? next.text.replace(/[\r\n]/g, ' ') : next.text).startsWith(value.text)))) {
      throw new Error('完整输入处理结果无法确认，原输入仍保留。');
    }
    if (!owned()) { throw new Error('编辑目标已变化，输入处理结果未采用。'); }
    return { value: copyText(value), action: reply.action, format_applied: reply.format_applied,
      grapheme_count: reply.grapheme_count, limit: reply.limit };
  }
}
