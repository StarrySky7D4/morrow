import { TextValue, copyText } from './EditorDraft';
import { EditorFieldPolicy } from './EditorFieldPolicy';

export function samePasteTarget(a: TextValue, b: TextValue): boolean {
  return a.text === b.text && a.selection_base === b.selection_base &&
    a.selection_extent === b.selection_extent && a.affinity === b.affinity && a.directional === b.directional &&
    a.composing_start === b.composing_start &&
    a.composing_end === b.composing_end;
}

function splitsSurrogate(text: string, offset: number): boolean {
  if (offset <= 0 || offset >= text.length) { return false; }
  const before = text.charCodeAt(offset - 1), after = text.charCodeAt(offset);
  return before >= 0xD800 && before <= 0xDBFF && after >= 0xDC00 && after <= 0xDFFF;
}

// Apply one complete clipboard snapshot to one frozen UTF-16 selection. Never
// trim, truncate, or silently overwrite a composing candidate or newer input.
export function insertPaste(value: TextValue, inserted: string): TextValue {
  if (value.composing_start !== -1 || value.composing_end !== -1) {
    throw new Error('请先完成输入法候选，再粘贴内容。');
  }
  if (typeof inserted !== 'string') { throw new Error('粘贴文字无法确认。'); }
  const a = value.selection_base, b = value.selection_extent;
  const unset = a === -1 && b === -1;
  if (!unset && (!Number.isInteger(a) || !Number.isInteger(b) || a < 0 || b < 0 || a > value.text.length || b > value.text.length)) {
    throw new Error('文字选区无法确认，请重新选择插入位置。');
  }
  const start = unset ? value.text.length : Math.min(a, b), end = unset ? value.text.length : Math.max(a, b);
  if (splitsSurrogate(value.text, start) || splitsSurrogate(value.text, end)) {
    throw new Error('选区位于一个字符内部，请重新选择插入位置。');
  }
  const result = copyText(value);
  result.text = value.text.slice(0, start) + inserted + value.text.slice(end);
  result.selection_base = start + inserted.length; result.selection_extent = result.selection_base;
  result.composing_start = -1; result.composing_end = -1;
  return result;
}

// Count the complete future field, including its untouched prefix and suffix.
// The synchronous splice never grants permission to persist the result.
export async function insertPasteChecked(value: TextValue, inserted: string, field: string,
  policy: EditorFieldPolicy, stillOwned: () => boolean): Promise<TextValue> {
  const frozen: TextValue = copyText(value), result: TextValue = insertPaste(frozen, inserted);
  const owned = (): boolean => stillOwned() && samePasteTarget(value, frozen);
  await policy.requireTextValue(field, result, owned);
  if (!owned()) { throw new Error('粘贴目标已变化，文字未插入。'); }
  return result;
}
