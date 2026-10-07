import { TextValue, copyText } from './EditorDraft';
import { insertPaste, samePasteTarget } from './EditorPaste';

export interface ClipboardPasteAttachment {
  key: string;
  name: string;
  byte_length: string;
  sha256: string;
  references: string[];
  image: boolean;
}

export interface ClipboardPastePlan {
  text: string;
  attachments: ClipboardPasteAttachment[];
  warnings: string[];
}

export interface ClipboardPasteBoundary {
  editor_id: string;
  card_id: string;
  draft_id: string;
  generation: string;
  selected_count: number;
  import_count: number;
  target: TextValue;
  blocked: boolean;
}

export interface ClipboardPasteIdentity {
  card_id: string;
  draft_id: string;
  expected_generation: string;
  operation_id: string;
}

export interface ClipboardPasteReceipt {
  outcome: string;
  identity: ClipboardPasteIdentity;
  asset_id: string;
  next: ClipboardPasteBoundary | undefined;
}

export interface ClipboardPasteResult {
  phase: string;
  total: number;
  selected: number;
  text_inserted: boolean;
  warnings: string[];
  error: string;
}

export interface ClipboardPasteHooks {
  read: () => ClipboardPasteBoundary;
  confirm: (boundary: ClipboardPasteBoundary) => Promise<ClipboardPasteBoundary>;
  importOne: (attachment: ClipboardPasteAttachment, identity: ClipboardPasteIdentity,
    boundary: ClipboardPasteBoundary) => Promise<ClipboardPasteReceipt>;
  commitText: (value: TextValue, boundary: ClipboardPasteBoundary) => Promise<ClipboardPasteBoundary>;
  operation: () => string;
}

function sameOwner(a: ClipboardPasteBoundary, b: ClipboardPasteBoundary): boolean {
  return !a.blocked && !b.blocked && a.editor_id === b.editor_id && a.card_id === b.card_id &&
    a.draft_id === b.draft_id && samePasteTarget(a.target, b.target);
}

function copyBoundary(value: ClipboardPasteBoundary): ClipboardPasteBoundary {
  return { editor_id: value.editor_id, card_id: value.card_id, draft_id: value.draft_id,
    generation: value.generation, selected_count: value.selected_count, import_count: value.import_count,
    target: copyText(value.target), blocked: value.blocked };
}

function validBoundary(value: ClipboardPasteBoundary): boolean {
  return !!value.editor_id && !!value.card_id && !!value.draft_id && /^(0|[1-9][0-9]*)$/.test(value.generation) &&
    Number.isInteger(value.selected_count) && value.selected_count >= 0 && value.selected_count <= 20 &&
    Number.isInteger(value.import_count) && value.import_count >= 0 && value.import_count <= 20;
}

function sameIdentity(a: ClipboardPasteIdentity, b: ClipboardPasteIdentity): boolean {
  return a.card_id === b.card_id && a.draft_id === b.draft_id &&
    a.expected_generation === b.expected_generation && a.operation_id === b.operation_id;
}

function referenceText(plan: ClipboardPastePlan, ids: Map<string, string>): string {
  let text: string = plan.text;
  for (const item of plan.attachments) {
    if (item.references.length === 0) { continue; }
    const id: string | undefined = ids.get(item.key);
    if (!id || !/^asset-draft-import-[a-f0-9]{64}$/.test(id)) {
      throw new Error('图片附件尚未得到准确保存回执，正文未插入。');
    }
    for (const reference of item.references) {
      text = text.split('attachment:' + encodeURIComponent(reference)).join('attachment:' + id);
    }
  }
  return text;
}

// Owns a single prepared plan. Native import journals and confirmed draft pins
// remain the recovery owners; this coordinator never retries their requests.
export class ClipboardPaste {
  private hooks: ClipboardPasteHooks;
  private epoch: number = 0;
  private inFlight: Promise<ClipboardPasteResult> | undefined = undefined;
  constructor(hooks: ClipboardPasteHooks) { this.hooks = hooks; }
  stop(): void { this.epoch++; }

  run(plan: ClipboardPastePlan, limit: number): Promise<ClipboardPasteResult> {
    if (this.inFlight !== undefined) { return this.inFlight; }
    const epoch: number = ++this.epoch;
    // Copy public plan metadata before any await; the actual capabilities stay
    // in the caller's private key -> PreparedAttachment inventory.
    const frozen: ClipboardPastePlan = { text: plan.text, warnings: plan.warnings.slice(),
      attachments: plan.attachments.map((item: ClipboardPasteAttachment): ClipboardPasteAttachment =>
        ({ key: item.key, name: item.name, byte_length: item.byte_length, sha256: item.sha256,
          references: item.references.slice(), image: item.image })) };
    this.inFlight = Promise.resolve().then((): Promise<ClipboardPasteResult> => this.perform(frozen, limit, epoch))
      .finally(() => { this.inFlight = undefined; });
    return this.inFlight;
  }

  private async perform(plan: ClipboardPastePlan, limit: number, epoch: number): Promise<ClipboardPasteResult> {
    const result: ClipboardPasteResult = { phase: 'preparing', total: plan.attachments.length,
      selected: 0, text_inserted: false, warnings: plan.warnings.slice(), error: '' };
    const ids: Map<string, string> = new Map<string, string>();
    const operations: Set<string> = new Set<string>();
    try {
      let boundary: ClipboardPasteBoundary = copyBoundary(this.hooks.read());
      if (!validBoundary(boundary) || boundary.blocked || epoch !== this.epoch) {
        throw new Error('粘贴目标已变化，尚未开始导入。');
      }
      if (plan.attachments.length > Math.min(20 - boundary.selected_count, 20 - boundary.import_count)) {
        throw new Error('附件剩余位置不足，文字和附件均未开始加入。');
      }
      const keys: Set<string> = new Set<string>();
      const references: Set<string> = new Set<string>();
      for (const item of plan.attachments) {
        if (!item.key || keys.has(item.key) || !item.name || !/^[1-9][0-9]*$/.test(item.byte_length) ||
          !/^[a-f0-9]{64}$/.test(item.sha256) || !Array.isArray(item.references) || item.references.length > 40 ||
          item.references.some((reference: string): boolean => !reference || reference.length > 512 || references.has(reference)) ||
          new Set<string>(item.references).size !== item.references.length) {
          throw new Error('粘贴附件计划无效，尚未开始导入。');
        }
        keys.add(item.key); for (const reference of item.references) { references.add(reference); }
        ids.set(item.key, 'asset-draft-import-' + '0'.repeat(64));
      }
      // Imported IDs have a fixed validated size. Preflight the complete final
      // insertion before the first native request, without persisting a token.
      insertPaste(boundary.target, referenceText(plan, ids), limit); ids.clear();
      const original: ClipboardPasteBoundary = boundary;
      boundary = copyBoundary(await this.hooks.confirm(copyBoundary(boundary)));
      if (!validBoundary(boundary) || !sameOwner(original, boundary) || epoch !== this.epoch ||
        boundary.selected_count !== original.selected_count || boundary.import_count !== original.import_count) {
        throw new Error('草稿确认期间插入位置发生变化，尚未开始导入。');
      }
      for (const item of plan.attachments) {
        const live: ClipboardPasteBoundary = copyBoundary(this.hooks.read());
        if (epoch !== this.epoch || !sameOwner(boundary, live) || live.generation !== boundary.generation ||
          live.selected_count !== boundary.selected_count || live.import_count !== boundary.import_count) {
          throw new Error('粘贴目标已变化，后续附件未开始导入。');
        }
        const operation: string = this.hooks.operation();
        if (!operation || operations.has(operation)) { throw new Error('附件操作身份无效。'); }
        operations.add(operation);
        const identity: ClipboardPasteIdentity = { card_id: boundary.card_id, draft_id: boundary.draft_id,
          expected_generation: boundary.generation, operation_id: operation };
        result.phase = 'importing';
        const receipt: ClipboardPasteReceipt = await this.hooks.importOne(item, identity, copyBoundary(boundary));
        if (!sameIdentity(receipt.identity, identity) || receipt.outcome !== 'selected' || !receipt.next ||
          !/^asset-draft-import-[a-f0-9]{64}$/.test(receipt.asset_id) || !validBoundary(receipt.next) ||
          receipt.next.editor_id !== boundary.editor_id || receipt.next.card_id !== boundary.card_id ||
          receipt.next.draft_id !== boundary.draft_id || receipt.next.selected_count !== boundary.selected_count + 1 ||
          receipt.next.import_count !== boundary.import_count + 1 || receipt.next.generation === boundary.generation) {
          throw new Error('附件结果尚未完整确认；原请求与已保存附件保留，正文未插入。');
        }
        ids.set(item.key, receipt.asset_id); result.selected++;
        if (epoch !== this.epoch || !sameOwner(boundary, receipt.next)) {
          throw new Error('已确认附件保留；粘贴目标已变化，后续附件和正文未开始加入。');
        }
        boundary = copyBoundary(receipt.next);
      }
      const live: ClipboardPasteBoundary = copyBoundary(this.hooks.read());
      if (epoch !== this.epoch || !sameOwner(boundary, live) || live.generation !== boundary.generation ||
        live.selected_count !== boundary.selected_count || live.import_count !== boundary.import_count) {
        throw new Error('插入位置发生变化，已保存附件保留，正文未插入。');
      }
      const text: string = referenceText(plan, ids);
      if (text.length > 0) {
        const inserted: TextValue = insertPaste(boundary.target, text, limit);
        result.phase = 'saving';
        const after: ClipboardPasteBoundary = await this.hooks.commitText(inserted, copyBoundary(boundary));
        if (epoch !== this.epoch || !validBoundary(after) || after.blocked || after.editor_id !== boundary.editor_id ||
          after.card_id !== boundary.card_id || after.draft_id !== boundary.draft_id ||
          after.selected_count !== boundary.selected_count || after.import_count !== boundary.import_count ||
          after.generation === boundary.generation ||
          !samePasteTarget(inserted, after.target)) {
          throw new Error('粘贴文字保存尚未确认，请核对草稿；不会重新粘贴。');
        }
        result.text_inserted = true;
      }
      result.phase = 'complete'; return result;
    } catch (error) {
      result.phase = 'stopped'; result.error = String(error); return result;
    }
  }
}
