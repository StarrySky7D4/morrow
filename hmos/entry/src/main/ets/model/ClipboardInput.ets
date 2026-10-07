import { pasteboard } from '@kit.BasicServicesKit';
import { image } from '@kit.ImageKit';
import { util } from '@kit.ArkTS';
import { attachmentKind } from './Attachments';

export type ClipboardFormat = 'plain' | 'html' | 'rtf' | 'xml' | '';
// Flutter source.length counts UTF-16 units. Bytes are an independent envelope
// for strict UTF-8 (+ optional BOM); UTF-16 BOM originals fit within it too.
export const CLIPBOARD_SOURCE_UTF16_LIMIT: number = 2 * 1024 * 1024;
export const CLIPBOARD_RTF_SOURCE_UTF16_LIMIT: number = 8 * 1024 * 1024;
export const CLIPBOARD_SOURCE_BYTE_LIMIT: number = 3 * CLIPBOARD_SOURCE_UTF16_LIMIT + 3;
export const CLIPBOARD_RTF_SOURCE_BYTE_LIMIT: number = 3 * CLIPBOARD_RTF_SOURCE_UTF16_LIMIT + 3;
export function clipboardSourceByteLimit(format: ClipboardFormat): number {
  return format === 'rtf' ? CLIPBOARD_RTF_SOURCE_BYTE_LIMIT : CLIPBOARD_SOURCE_BYTE_LIMIT;
}
const BATCH_BYTES: number = 64 * 1024 * 1024;
const TYPES_LIMIT: number = 64;
const TEXT = 'text/plain';
const HTML = 'text/html';
const URI = 'text/uri';
const PIXEL = 'pixelMap';
const RTF_TYPES: string[] = ['text/rtf', 'application/rtf'];
const XML_TYPES: string[] = ['application/vnd.ms-excel.xml', 'application/xml', 'text/xml', 'XML Spreadsheet'];
const BINARY_TYPES: string[] = [
  'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
  'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',
  'application/vnd.openxmlformats-officedocument.presentationml.presentation',
  'application/msword', 'application/vnd.ms-excel', 'application/vnd.ms-powerpoint',
  'application/pdf', 'image/gif', 'image/webp', 'image/png', 'image/jpeg', 'image/svg+xml',
  'image/tiff', 'image/bmp', 'video/mp4', 'video/quicktime', 'video/webm',
  'audio/mpeg', 'audio/wav', 'audio/ogg', 'application/zip', 'text/csv', 'application/json'
];
const EXTENSIONS: string[] = ['docx', 'xlsx', 'pptx', 'doc', 'xls', 'ppt', 'pdf', 'gif', 'webp',
  'png', 'jpg', 'svg', 'tiff', 'bmp', 'mp4', 'mov', 'webm', 'mp3', 'wav', 'ogg', 'zip', 'csv', 'json'];

// Only identity-registered handles minted from SystemPasteboard can be consumed.
// No provider URI, bytes, PixelMap or persistent retry token is exposed here.
export class ClipboardSource {
  name: string = '';
  mime: string = '';
  kind: string = 'file';
  format: ClipboardFormat = '';
}
export class ClipboardRecordInput {
  index: number = 0;
  formats: string[] = [];
  plain_text: string = '';
  uri_text: string = '';
  plain: ClipboardSource | undefined = undefined;
  html: ClipboardSource | undefined = undefined;
  rtf: ClipboardSource | undefined = undefined;
  xml: ClipboardSource | undefined = undefined;
  file: ClipboardSource | undefined = undefined;
  image: ClipboardSource | undefined = undefined;
  binary: ClipboardSource[] = [];
}
export class ClipboardSnapshot {
  change_count: number = 0;
  records: ClipboardRecordInput[] = [];
  formats: string[] = [];
  warnings: string[] = [];
}
// Private integration callback data. The opaque capability, not these fields,
// proves provenance; AttachmentFiles never accepts a caller-created value.
export class ClipboardSourceRead {
  uri: string = '';
  bytes: Uint8Array = new Uint8Array(0);
  name: string = '';
  check: () => void = (): void => {};
}
class PixelLease {
  value: image.PixelMap;
  busy: boolean = false;
  released: boolean = false;
  done: Promise<void> = Promise.resolve();
  settle: () => void = (): void => {};
  constructor(value: image.PixelMap) { this.value = value; }
}
class SnapshotEntry {
  value: ClipboardSnapshot;
  board: pasteboard.SystemPasteboard;
  owner: () => boolean;
  count: number;
  closed: boolean = false;
  sources: ClipboardSource[] = [];
  pixels: PixelLease[] = [];
  bytes: number = 0;
  constructor(value: ClipboardSnapshot, board: pasteboard.SystemPasteboard, owner: () => boolean, count: number) {
    this.value = value; this.board = board; this.owner = owner; this.count = count;
  }
  check(): void {
    if (this.closed || !this.owner() || this.board.getChangeCount() !== this.count || this.value.change_count !== this.count) {
      throw new Error('剪贴板或粘贴目标已变化，未继续读取。');
    }
  }
}
class SourceEntry {
  snapshot: SnapshotEntry;
  name: string;
  mime: string;
  kind: string;
  format: ClipboardFormat;
  bytes: Uint8Array = new Uint8Array(0);
  uri: string = '';
  pixel: PixelLease | undefined = undefined;
  used: boolean = false;
  constructor(snapshot: SnapshotEntry, value: ClipboardSource) {
    this.snapshot = snapshot; this.name = value.name; this.mime = value.mime; this.kind = value.kind; this.format = value.format;
  }
  check(value: ClipboardSource): void {
    this.snapshot.check();
    if (this.used || value.name !== this.name || value.mime !== this.mime || value.kind !== this.kind || value.format !== this.format) {
      throw new Error('剪贴板附件读取能力已使用或变化。');
    }
  }
}
const sources: Map<ClipboardSource, SourceEntry> = new Map<ClipboardSource, SourceEntry>();
const snapshots: Map<ClipboardSnapshot, SnapshotEntry> = new Map<ClipboardSnapshot, SnapshotEntry>();
let active: SnapshotEntry | undefined = undefined;

async function releasePixel(lease: PixelLease): Promise<void> {
  if (lease.busy) { await lease.done; }
  if (!lease.released) { lease.released = true; await lease.value.release(); }
}
function sourceName(index: number, extension: string): string { return 'clipboard-record-' + (index + 1) + '.' + extension; }
function validTypes(values: string[]): string[] {
  if (!Array.isArray(values) || values.length > TYPES_LIMIT) { throw new Error('剪贴板格式列表过多或无效。'); }
  const result: string[] = [];
  for (let i: number = 0; i < values.length; i++) {
    const value = values[i];
    if (typeof value !== 'string' || !value || value.length > 256 || /[\u0000-\u001f\u007f]/.test(value)) {
      throw new Error('剪贴板格式列表无效。');
    }
    if (!result.includes(value)) { result.push(value); }
  }
  return result;
}
function isFileUri(value: string): boolean {
  return value.length <= 16384 && /^file:\/\//i.test(value) && !/[\u0000-\u001f\u007f]/.test(value);
}
function textByteLength(text: string, maximum: number): number {
  let total: number = 0;
  for (let i: number = 0; i < text.length; i++) {
    const code = text.charCodeAt(i);
    if (code < 0x80) { total++; }
    else if (code < 0x800) { total += 2; }
    else if (code >= 0xd800 && code <= 0xdbff) {
      if (i + 1 >= text.length || text.charCodeAt(i + 1) < 0xdc00 || text.charCodeAt(i + 1) > 0xdfff) {
        throw new Error('剪贴板文字含不配对的 UTF-16 代理项，未编码或保存该原件。');
      }
      total += 4; i++;
    } else if (code >= 0xdc00 && code <= 0xdfff) {
      throw new Error('剪贴板文字含不配对的 UTF-16 代理项，未编码或保存该原件。');
    } else { total += 3; }
    if (total > maximum) { throw new Error('剪贴板原件超过读取预算。'); }
  }
  return total;
}

export class ClipboardInput {
  // Caller invokes this only from a successful authorized PasteButton event.
  // Owner is the frozen editor/field/selection/IME boundary, checked after every await.
  async readAuthorized(owner: () => boolean): Promise<ClipboardSnapshot> {
    if (active) { throw new Error('已有粘贴读取尚未结束，请先处理原结果。'); }
    if (!owner()) { throw new Error('粘贴目标已变化。'); }
    const board = pasteboard.getSystemPasteboard();
    const count = board.getChangeCount();
    if (!Number.isSafeInteger(count) || count < 0) { throw new Error('剪贴板快照无法确认。'); }
    const value = new ClipboardSnapshot(); value.change_count = count;
    const entry = new SnapshotEntry(value, board, owner, count);
    active = entry; snapshots.set(value, entry);
    try {
      const data = await board.getData(); entry.check();
      const recordCount = data.getRecordCount();
      if (!Number.isInteger(recordCount) || recordCount < 0 || recordCount > 20) {
        throw new Error('一次粘贴最多读取 20 条记录，未截断内容。');
      }
      const offered = validTypes(data.getProperty().mimeTypes);
      const candidates = Array.from(new Set<string>([TEXT, HTML, URI, PIXEL].concat(RTF_TYPES, XML_TYPES, BINARY_TYPES, offered)));
      for (let i: number = 0; i < recordCount; i++) {
        entry.check();
        const systemRecord = data.getRecord(i);
        const record = new ClipboardRecordInput(); record.index = i;
        const available = validTypes(systemRecord.getValidTypes(candidates));
        if (available.some((type: string): boolean => !candidates.includes(type))) { throw new Error('剪贴板返回了未请求的格式。'); }
        record.formats = candidates.filter((type: string): boolean => available.includes(type));
        value.records.push(record);
        for (const type of record.formats) { if (!value.formats.includes(type)) { value.formats.push(type); } }
        // Flutter's actual file URI branch excludes duplicate text/image flavors.
        if (record.formats.includes(URI)) {
          try {
            const uri = await systemRecord.getData(URI); entry.check();
            if (typeof uri !== 'string' || uri.length > 16384 || /[\u0000-\u001f\u007f]/.test(uri)) { throw new Error('剪贴板 URI 无效。'); }
            if (isFileUri(uri)) {
              const source = this.mint(entry, i, URI, '', 'file', 'file'); sources.get(source)!.uri = uri;
              record.file = source; continue;
            }
            record.uri_text = uri;
          } catch (error) { entry.check(); value.warnings.push('剪贴板文件引用无法读取：' + (error as Error).message); }
        }
        for (const type of record.formats) {
          if (type === URI || type === 'text/want') { continue; }
          try {
            const raw = await systemRecord.getData(type);
            // A late SDK reply can own a PixelMap even when the target has
            // already changed. Register its lease before checking the boundary.
            if (type === PIXEL && raw && typeof raw === 'object' && !(raw instanceof ArrayBuffer)) {
              const returnedPixel = raw as image.PixelMap;
              if (typeof returnedPixel.getImageInfo === 'function' && typeof returnedPixel.getPixelBytesNumber === 'function' &&
                typeof returnedPixel.release === 'function' && !entry.pixels.some((item: PixelLease): boolean => item.value === returnedPixel)) {
                entry.pixels.push(new PixelLease(returnedPixel));
              }
            }
            entry.check();
            if (type === PIXEL) {
              if (!raw || typeof raw !== 'object' || raw instanceof ArrayBuffer) { throw new Error('剪贴板图片值类型无效。'); }
              const pixel = raw as image.PixelMap;
              if (typeof pixel.getImageInfo !== 'function' || typeof pixel.getPixelBytesNumber !== 'function' || typeof pixel.release !== 'function') {
                throw new Error('剪贴板图片值类型无效。');
              }
              let lease = entry.pixels.find((item: PixelLease): boolean => item.value === pixel);
              if (!lease) { lease = new PixelLease(pixel); entry.pixels.push(lease); }
              const source = this.mint(entry, i, type, '', 'image', 'png'); sources.get(source)!.pixel = lease;
              if (!record.image) { record.image = source; }
              continue;
            }
            let format: ClipboardFormat = '';
            if (type === TEXT) { format = 'plain'; }
            else if (type === HTML) { format = 'html'; }
            else if (RTF_TYPES.includes(type)) { format = 'rtf'; }
            else if (XML_TYPES.includes(type)) { format = 'xml'; }
            let bytes: Uint8Array;
            if (typeof raw === 'string' && format) {
              // A system Unicode RTF flavor has no original document encoding.
              // Serialize it explicitly as BOM UTF-8 so literal Unicode cannot
              // be mistaken for ansicpg bytes; raw ArrayBuffer stays untouched.
              const prefixBytes: number = format === 'rtf' ? 3 : 0;
              const limit = format === 'plain' ? CLIPBOARD_SOURCE_BYTE_LIMIT : BATCH_BYTES - prefixBytes;
              if (format === 'plain' && raw.length > CLIPBOARD_SOURCE_UTF16_LIMIT ||
                raw.length > BATCH_BYTES || textByteLength(raw, limit) > limit) { throw new Error('剪贴板文字超过读取预算。'); }
              const encoded = new util.TextEncoder().encodeInto(raw);
              if (prefixBytes) {
                bytes = new Uint8Array(encoded.byteLength + prefixBytes);
                bytes[0] = 0xef; bytes[1] = 0xbb; bytes[2] = 0xbf; bytes.set(encoded, prefixBytes);
              } else { bytes = encoded; }
            } else if (raw instanceof ArrayBuffer) { bytes = new Uint8Array(raw); }
            else { throw new Error('剪贴板格式值类型无法读取。'); }
            // Rich-format originals survive conversion limits, within the same
            // attachment quota. Native checks decoded UTF-16 before conversion.
            if (format === 'plain' && bytes.byteLength > CLIPBOARD_SOURCE_BYTE_LIMIT || bytes.byteLength > BATCH_BYTES || entry.bytes + bytes.byteLength > BATCH_BYTES) {
              throw new Error('剪贴板原件超过读取预算。');
            }
            entry.bytes += bytes.byteLength;
            if (type === TEXT && typeof raw === 'string') { record.plain_text = raw; }
            const binaryIndex = BINARY_TYPES.indexOf(type);
            const extension = format === 'plain' ? 'tsv' : format || (binaryIndex >= 0 ? EXTENSIONS[binaryIndex] : 'bin');
            const kind = attachmentKind(sourceName(i, extension));
            const source = this.mint(entry, i, type, format, kind, extension);
            sources.get(source)!.bytes = bytes.slice();
            if (format === 'plain') { record.plain = source; }
            else if (format === 'html' && !record.html) { record.html = source; }
            else if (format === 'rtf' && !record.rtf) { record.rtf = source; }
            else if (format === 'xml' && !record.xml) { record.xml = source; }
            else if (!format) {
              // Original image bytes win even if PixelMap appeared earlier.
              if (kind === 'image' || kind === 'gif') {
                const prior = record.image;
                if (!prior || sources.get(prior)?.pixel) { record.image = source; }
              } else { record.binary.push(source); }
            }
          } catch (error) { entry.check(); value.warnings.push('剪贴板格式 ' + type + ' 未读取：' + (error as Error).message); }
        }
      }
      entry.check(); return value;
    } catch (error) {
      await this.dispose(value);
      if (error instanceof Error) { throw error; }
      throw new Error(String(error));
    }
  }

  private mint(entry: SnapshotEntry, index: number, mime: string, format: ClipboardFormat, kind: string, extension: string): ClipboardSource {
    const value = new ClipboardSource(); value.name = sourceName(index, extension); value.mime = mime; value.format = format;
    value.kind = attachmentKind(value.name);
    sources.set(value, new SourceEntry(entry, value)); entry.sources.push(value); return value;
  }
  isCurrent(value: ClipboardSnapshot): boolean {
    const entry = snapshots.get(value);
    if (!entry) { return false; }
    try { entry.check(); return true; } catch (_) { return false; }
  }
  async dispose(value: ClipboardSnapshot): Promise<void> {
    const entry = snapshots.get(value);
    if (!entry) { return; }
    entry.closed = true;
    for (const source of entry.sources) { sources.delete(source); }
    let failed: boolean = false;
    try {
      for (const pixel of entry.pixels) {
        try { await releasePixel(pixel); } catch (_) { failed = true; value.warnings.push('剪贴板图片资源释放失败。'); }
      }
    } finally { snapshots.delete(value); if (active === entry) { active = undefined; } }
    if (failed) { throw new Error('剪贴板图片资源释放未确认，未继续加入。'); }
  }
}

// Only AttachmentFiles uses this bridge to prepare a source into its own bounded
// namespace. Fabricated objects and replay of consumed sources fail before I/O.
export async function consumeClipboardSource<T>(value: ClipboardSource, maximum: number,
  read: (source: ClipboardSourceRead) => Promise<T>): Promise<T> {
  if (!Number.isSafeInteger(maximum) || maximum <= 0 || maximum > BATCH_BYTES) { throw new Error('剪贴板附件额度无效。'); }
  const entry = sources.get(value);
  if (!entry) { throw new Error('附件不是本次系统剪贴板的读取能力。'); }
  entry.check(value); entry.used = true;
  const result = new ClipboardSourceRead(); result.name = entry.name; result.uri = entry.uri;
  result.check = (): void => entry.snapshot.check();
  const pixel = entry.pixel;
  if (pixel?.released || pixel?.busy) { throw new Error('剪贴板图片资源已使用。'); }
  let packer: image.ImagePacker | undefined = undefined;
  let completed: boolean = false;
  if (pixel) { pixel.busy = true; pixel.done = new Promise<void>((resolve: () => void): void => { pixel.settle = resolve; }); }
  try {
    if (pixel) {
      const info = await pixel.value.getImageInfo(); entry.snapshot.check();
      const byteCount = pixel.value.getPixelBytesNumber();
      if (!info?.size || !Number.isSafeInteger(info.size.width) || !Number.isSafeInteger(info.size.height) ||
        info.size.width <= 0 || info.size.height <= 0 || !Number.isSafeInteger(byteCount) || byteCount <= 0 || byteCount > maximum ||
        info.size.width * info.size.height * 4 > maximum) { throw new Error('剪贴板图片超过可用编码预算。'); }
      packer = image.createImagePacker();
      const encoded = await packer.packToData(pixel.value, { format: 'image/png', quality: 100, bufferSize: maximum });
      entry.snapshot.check();
      if (!(encoded instanceof ArrayBuffer) || encoded.byteLength <= 0 || encoded.byteLength > maximum) { throw new Error('剪贴板图片编码超过附件预算。'); }
      result.bytes = new Uint8Array(encoded);
    } else {
      if (entry.bytes.byteLength > maximum) { throw new Error('剪贴板原件超过剩余附件预算。'); }
      result.bytes = entry.bytes;
    }
    entry.snapshot.check();
    const returned = await read(result); entry.snapshot.check(); completed = true; return returned;
  } finally {
    let cleanupFailed: boolean = false;
    if (packer) {
      try { await packer.release(); } catch (_) { cleanupFailed = true; entry.snapshot.value.warnings.push('剪贴板图片编码器资源释放失败。'); }
    }
    if (pixel) {
      pixel.busy = false; pixel.settle();
      try { await releasePixel(pixel); } catch (_) { cleanupFailed = true; entry.snapshot.value.warnings.push('剪贴板图片资源释放失败。'); }
    }
    entry.bytes = new Uint8Array(0); entry.uri = '';
    if (cleanupFailed) { throw new Error('剪贴板图片资源释放未确认，未继续加入。'); }
    if (completed) { entry.snapshot.check(); }
  }
}

export function checkClipboardSource(value: ClipboardSource): void {
  const entry = sources.get(value);
  if (!entry) { throw new Error('附件不是本次系统剪贴板的读取能力。'); }
  entry.check(value);
}
