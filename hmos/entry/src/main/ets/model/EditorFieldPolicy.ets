import { TextValue, Values, copyValues } from './EditorDraft';

export interface EditorFieldCheck {
  ok: boolean;
  error: string;
  field: string;
  grapheme_count: number;
  utf16_length: number;
  utf8_length: number;
  limit: number;
  unicode_version: string;
}

export function editorFieldLimit(field: string): number {
  if (field === 'title') { return 60; }
  if (field === 'todos') { return 1000; }
  if (field === 'hypothesis') { return 5000; }
  if (field === 'conclusion') { return 10000; }
  if (field === 'description') { return 20000; }
  throw new Error('编辑字段无法确认。');
}

// UTF-16 remains the editing coordinate system. This validates encoding only;
// grapheme boundaries and counts come exclusively from the native policy.
function utf8Length(text: string): number {
  let bytes: number = 0;
  for (let index: number = 0; index < text.length; index++) {
    const code: number = text.charCodeAt(index);
    if (code >= 0xD800 && code <= 0xDBFF) {
      const next: number = text.charCodeAt(index + 1);
      if (!(next >= 0xDC00 && next <= 0xDFFF)) { throw new Error('文字含未完成的 Unicode 字符，请完成输入法候选后重试。'); }
      bytes += 4; index++;
    } else if (code >= 0xDC00 && code <= 0xDFFF) {
      throw new Error('文字含未完成的 Unicode 字符，请完成输入法候选后重试。');
    } else { bytes += code < 0x80 ? 1 : code < 0x800 ? 2 : 3; }
  }
  return bytes;
}

function isCount(value: number): boolean { return Number.isSafeInteger(value) && value >= 0; }

// A read-only native result cannot authorize a newer edit or revive an owner
// closed while the worker was running. Callers also freeze their full editor
// identity/epoch/selection in stillOwned; stop() revokes outstanding checks.
export class EditorFieldPolicy {
  private send: (request: string) => Promise<string>;
  private epoch: number = 0;
  constructor(send: (request: string) => Promise<string>) { this.send = send; }
  stop(): void { this.epoch++; }

  async checkField(field: string, text: string, stillOwned: () => boolean): Promise<EditorFieldCheck> {
    const limit: number = editorFieldLimit(field), epoch: number = this.epoch;
    if (typeof text !== 'string' || !stillOwned()) { throw new Error('编辑目标已变化，文字未确认。'); }
    const bytes: number = utf8Length(text);
    const encoded: string = await this.send(JSON.stringify({ field: field, text: text }));
    if (epoch !== this.epoch || !stillOwned()) { throw new Error('编辑目标已变化，晚到的文字计数未采用。'); }
    const result: EditorFieldCheck = JSON.parse(encoded) as EditorFieldCheck;
    if (result && !result.ok && result.field === '' && result.limit === 0 &&
      result.grapheme_count === -1 && result.utf16_length === -1 && result.utf8_length === -1 &&
      result.unicode_version === '16.0.0' && result.error === 'EditorFieldRequestBytesLimit') {
      throw new Error('文字规则未确认，完整请求超过独立字节预算，内容未保存：' + result.error);
    }
    if (!result || typeof result.ok !== 'boolean' || typeof result.error !== 'string' ||
      result.field !== field || result.limit !== limit || result.unicode_version !== '16.0.0') {
      throw new Error('文字计数回执无法确认，内容未保存。');
    }
    if (result.grapheme_count === -1 && result.utf16_length === -1 && result.utf8_length === -1 &&
      !result.ok && !!result.error && result.error !== 'EditorFieldGraphemeLimit') { return result; }
    if (!isCount(result.grapheme_count) || result.grapheme_count > text.length ||
      (text.length === 0 ? result.grapheme_count !== 0 : result.grapheme_count === 0) ||
      result.utf16_length !== text.length || result.utf8_length !== bytes ||
      (result.ok ? result.error !== '' || result.grapheme_count > limit :
        result.error !== 'EditorFieldGraphemeLimit' || result.grapheme_count <= limit)) {
      throw new Error('文字计数回执不完整，内容未保存。');
    }
    return result;
  }

  async requireField(field: string, text: string, stillOwned: () => boolean): Promise<EditorFieldCheck> {
    const epoch: number = this.epoch;
    const result: EditorFieldCheck = await this.checkField(field, text, stillOwned);
    if (epoch !== this.epoch || !stillOwned()) { throw new Error('编辑目标已变化，文字规则未采用。'); }
    if (!result.ok) {
      if (result.error === 'EditorFieldGraphemeLimit') {
        throw new Error('此字段最多输入 ' + result.limit.toString() + ' 个字符，完整内容超限，未插入或保存。');
      }
      throw new Error('文字规则未确认，内容未保存：' + result.error);
    }
    return result;
  }

  async requireTextValue(field: string, value: TextValue, stillOwned: () => boolean): Promise<EditorFieldCheck> {
    if (value.composing_start !== -1 || value.composing_end !== -1) {
      throw new Error('请先完成输入法候选，再粘贴或保存内容。');
    }
    return this.requireField(field, value.text, stillOwned);
  }

  async checkValues(values: Values, stillOwned: () => boolean): Promise<EditorFieldCheck[]> {
    const frozen: Values = copyValues(values), epoch: number = this.epoch;
    const fields: string[] = ['title', 'todos', 'hypothesis', 'conclusion', 'description'];
    const texts: TextValue[] = [frozen.title, frozen.todos, frozen.hypothesis, frozen.conclusion, frozen.description];
    const owned = (): boolean => epoch === this.epoch && stillOwned();
    const result: EditorFieldCheck[] = [];
    for (let index: number = 0; index < fields.length; index++) {
      result.push(await this.checkField(fields[index], texts[index].text, owned));
    }
    if (!owned()) { throw new Error('编辑目标已变化，完整字段检查未采用。'); }
    return result;
  }

  async requireValues(values: Values, stillOwned: () => boolean, allowComposing: boolean = false): Promise<EditorFieldCheck[]> {
    const frozen: Values = copyValues(values), epoch: number = this.epoch;
    if (!allowComposing) {
      for (const text of [frozen.title, frozen.todos, frozen.hypothesis, frozen.conclusion, frozen.description]) {
        if (text.composing_start !== -1 || text.composing_end !== -1) {
          throw new Error('请先完成输入法候选，再保存内容。');
        }
      }
    }
    const result: EditorFieldCheck[] = await this.checkValues(frozen, stillOwned);
    if (epoch !== this.epoch || !stillOwned()) { throw new Error('编辑目标已变化，完整字段检查未采用。'); }
    for (const checked of result) {
      if (!checked.ok) {
        throw new Error(checked.error === 'EditorFieldGraphemeLimit' ?
          '此字段最多输入 ' + checked.limit.toString() + ' 个字符，完整内容超限，未保存。' :
          '文字规则未确认，内容未保存：' + checked.error);
      }
    }
    return result;
  }
}
