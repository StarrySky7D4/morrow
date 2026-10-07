import { common } from '@kit.AbilityKit';
import { fileIo as fs, fileUri, picker } from '@kit.CoreFileKit';
import { util } from '@kit.ArkTS';
import { BusinessError } from '@kit.BasicServicesKit';
import native from 'libmorrow.so';
import { ImportCommand } from './Attachments';

export const ATTACHMENT_IMPORT_BUDGET: number = 64 * 1024 * 1024;
export const ATTACHMENT_EXPORT_LIMIT: number = 200 * 1024 * 1024;
export const ATTACHMENT_INLINE_PREVIEW_BUDGET: number = 64 * 1024 * 1024;
export const ATTACHMENT_INLINE_PREVIEW_LIMIT: number = 8;
const MAX_SPOOLS: number = 20;
const MAX_REQUEST_BYTES: number = 512 * 1024;
// Reserve room for a future unknown-operation journal as well as metadata, so
// payload plus every retained sidecar stays below the private 64 MiB quota.
const SPOOL_SIDECAR_RESERVE: number = MAX_REQUEST_BYTES + 4096;

export interface FileStreamReply {
  ok: boolean;
  error: string;
  byte_length: string;
  sha256: string;
}

// This is an opaque handle and metadata, never a provider URI or a file path.
export class PreparedAttachment {
  spool_id: string = '';
  name: string = '';
  byte_length: string = '0';
  sha256: string = '';
  original_request: string = '';
}

export class AttachmentRecoveryDiagnostic {
  spool_id: string = '';
  code: string = '';
  message: string = '有附件缓存需要核对，已保留。';
  request_state: string = 'unknown';
  requires_review: boolean = true;
  preserved: boolean = true;
}

// This path is a UI-only resource returned after native whole-file validation.
// It must never be serialized into an Engine request or a persistent record.
export class VerifiedPreview {
  token: string = '';
  path: string = '';
  uri: string = '';
  name: string = '';
  byte_length: string = '0';
  sha256: string = '';
  purpose: string = 'dialog';
}

// Flutter InteractiveViewer's default image range. Geometry is expressed in
// vp and kept independent of source pixels and private file ownership.
export function imagePreviewScale(value: number): number {
  return Number.isFinite(value) ? Math.max(0.8, Math.min(2.5, value)) : 1;
}
export function imagePreviewOffset(value: number, renderedExtent: number, viewportExtent: number, scale: number): number {
  if (!Number.isFinite(value) || !Number.isFinite(renderedExtent) || !Number.isFinite(viewportExtent) ||
    renderedExtent <= 0 || viewportExtent <= 0) { return 0; }
  const limit = Math.max(0, (renderedExtent * imagePreviewScale(scale) - viewportExtent) / 2);
  return Math.max(-limit, Math.min(limit, value));
}

class PreviewEntry {
  value: VerifiedPreview;
  token: string;
  directory: string;
  cache_root: string;
  path: string;
  uri: string;
  name: string;
  byte_length: string;
  sha256: string;
  purpose: string;
  cleanup_failed: boolean = false;
  constructor(value: VerifiedPreview, directory: string, cacheRoot: string) {
    this.value = value; this.token = value.token; this.directory = directory; this.cache_root = cacheRoot; this.path = value.path; this.uri = value.uri;
    this.name = value.name; this.byte_length = value.byte_length; this.sha256 = value.sha256; this.purpose = value.purpose;
  }
  matches(value: VerifiedPreview): boolean {
    return this.value === value && value.token === this.token &&
      value.path === this.path && value.uri === this.uri && value.name === this.name && value.byte_length === this.byte_length &&
      value.sha256 === this.sha256 && value.purpose === this.purpose;
  }
}

class SpoolEntry {
  value: PreparedAttachment;
  directory: string;
  path: string;
  name: string;
  byte_length: string;
  sha256: string;
  request: string;
  constructor(value: PreparedAttachment, directory: string) {
    this.value = value; this.directory = directory; this.path = directory + '/data';
    this.name = value.name; this.byte_length = value.byte_length; this.sha256 = value.sha256;
    this.request = value.original_request;
  }
}

function safeName(value: string): string {
  let name = value.replace(/[\u0000-\u001f\u007f\\/:*?"<>|]/g, '_').trim().slice(0, 128);
  const last = name.charCodeAt(name.length - 1);
  if (last >= 0xd800 && last <= 0xdbff) { name = name.slice(0, -1); }
  return name && name !== '.' && name !== '..' ? name : '附件';
}

function streamReply(serialized: string, limit: number): FileStreamReply {
  const reply = JSON.parse(serialized) as FileStreamReply;
  if (!reply || reply.ok !== true) { throw new Error('文件未能完成读取或验证。'); }
  const length = Number(reply.byte_length);
  if (typeof reply.byte_length !== 'string' || !/^(0|[1-9][0-9]*)$/.test(reply.byte_length) ||
    !Number.isSafeInteger(length) || length > limit || typeof reply.sha256 !== 'string' ||
    !/^[0-9a-f]{64}$/.test(reply.sha256)) { throw new Error('文件验证结果不完整。'); }
  return reply;
}

function close(file: fs.File | undefined): void {
  if (file) { try { fs.closeSync(file); } catch (_) {} }
}

function validImportRequest(serialized: string, value: PreparedAttachment): void {
  const command = JSON.parse(serialized) as ImportCommand;
  const request = command?.import_request;
  if (command?.action !== 'import_file' || !request || Array.isArray(request) ||
    [request.card_id, request.draft_id, request.operation_id].some((id: string): boolean =>
      typeof id !== 'string' || !id || id.length > 256 || id.indexOf('\u0000') >= 0) ||
    typeof request.expected_generation !== 'string' || !/^(0|[1-9][0-9]*)$/.test(request.expected_generation) ||
    request.expected_generation.length > 20 || request.expected_generation.length === 20 &&
      request.expected_generation > '18446744073709551615' ||
    !['file', 'image', 'gif', 'video', 'audio'].includes(request.kind) ||
    request.name !== value.name || request.byte_length !== value.byte_length || request.sha256 !== value.sha256) {
    throw new Error('原附件请求无法验证，已保留。');
  }
}

// Attachment bytes stay in bounded native streams. Only small metadata and
// the immutable request journal are encoded in ArkTS memory.
export class AttachmentFiles {
  private context: common.UIAbilityContext;
  private root: string;
  private entries: Map<string, SpoolEntry> = new Map<string, SpoolEntry>();
  private previews: Map<string, PreviewEntry> = new Map<string, PreviewEntry>();
  // Admission is shared across adapters in this UI process. A new instance
  // cannot bypass the inline byte/file budget or the single-dialog limit.
  private static activePreviews: Map<string, PreviewEntry> = new Map<string, PreviewEntry>();
  private issues: AttachmentRecoveryDiagnostic[] = [];
  // Instances in the same UI process share filesystem admission too.
  private static tail: Promise<void> = Promise.resolve();

  constructor(context: common.UIAbilityContext) {
    this.context = context;
    this.root = context.filesDir + '/hmos-attachment-spool';
  }

  private queued<T>(operation: () => Promise<T>): Promise<T> {
    const result: Promise<T> = AttachmentFiles.tail.then(operation);
    AttachmentFiles.tail = result.then((): void => {}, (): void => {});
    return result;
  }

  private async ensureRoot(): Promise<void> {
    if (!await this.statIfPresent(this.root)) { await fs.mkdir(this.root); }
    const stat = await fs.lstat(this.root);
    if (!stat.isDirectory() || stat.isSymbolicLink()) { throw new Error('附件缓存目录不可用。'); }
  }

  private async statIfPresent(path: string): Promise<fs.Stat | undefined> {
    try { return await fs.lstat(path); }
    catch (error) {
      // access(false) is insufficient: it can hide a dangling symlink or an
      // access failure. Only the documented ENOENT is proof of absence.
      if ((error as BusinessError).code === 13900002) { return undefined; }
      throw new Error('附件缓存读取失败，已保留。');
    }
  }

  private diagnose(id: string, code: string, requestState: string, removed: boolean = false): void {
    const issue = new AttachmentRecoveryDiagnostic();
    issue.spool_id = id; issue.code = code; issue.request_state = requestState;
    if (removed) {
      issue.message = '已清理未开始导入的中断准备。'; issue.requires_review = false; issue.preserved = false;
    }
    this.issues.push(issue);
  }

  recoveryDiagnostics(): AttachmentRecoveryDiagnostic[] {
    return this.issues.map((issue: AttachmentRecoveryDiagnostic): AttachmentRecoveryDiagnostic => {
      const copy = new AttachmentRecoveryDiagnostic();
      copy.spool_id = issue.spool_id; copy.code = issue.code; copy.message = issue.message;
      copy.request_state = issue.request_state; copy.requires_review = issue.requires_review; copy.preserved = issue.preserved;
      return copy;
    });
  }

  private checked(value: PreparedAttachment): SpoolEntry {
    const entry = this.entries.get(value.spool_id);
    if (!entry || entry.value !== value || entry.name !== value.name || entry.byte_length !== value.byte_length ||
      entry.sha256 !== value.sha256 || entry.request !== value.original_request) {
      throw new Error('附件缓存已变化，请重新读取。');
    }
    return entry;
  }

  private async remaining(): Promise<number> {
    await this.scanSpools();
    const names = await fs.listFile(this.root);
    if (names.length >= MAX_SPOOLS) { throw new Error('已有 20 份待处理附件，请先处理已有附件。'); }
    let used: number = 0;
    for (const name of names) {
      if (!/^import-[A-Za-z0-9]{6}$/.test(name)) { throw new Error('附件缓存需要核对，未开始新的导入。'); }
      const directory = this.root + '/' + name;
      const folder = await fs.lstat(directory);
      if (!folder.isDirectory() || folder.isSymbolicLink()) { throw new Error('附件缓存目录不可用。'); }
      const children = await fs.listFile(directory);
      if (children.some((child: string): boolean => !['data', 'metadata.json', 'request.json'].includes(child))) {
        throw new Error('附件缓存需要核对，未开始新的导入。');
      }
      let sidecars: number = 0;
      for (const child of ['data', 'metadata.json', 'request.json']) {
        const stat = await this.statIfPresent(directory + '/' + child);
        if (stat) {
          if (!stat.isFile() || stat.isSymbolicLink() || !Number.isSafeInteger(stat.size) || stat.size < 0) {
            throw new Error('附件缓存需要核对，未开始新的导入。');
          }
          if (child === 'data') { used += stat.size; } else { sidecars += stat.size; }
        }
      }
      // Corrupt journals remain retained. Count their actual size too, even
      // when it exceeds the amount reserved for a valid future journal.
      used += Math.max(SPOOL_SIDECAR_RESERVE, sidecars);
    }
    const remaining = ATTACHMENT_IMPORT_BUDGET - used - SPOOL_SIDECAR_RESERVE;
    if (remaining <= 0) { throw new Error('附件导入缓存已达到 64 MiB，请先处理已有附件。'); }
    return remaining;
  }

  private async writeSmall(path: string, text: string, limit: number): Promise<void> {
    const bytes = new util.TextEncoder().encodeInto(text);
    if (bytes.byteLength > limit) { throw new Error('附件请求过大，未开始导入。'); }
    let file: fs.File | undefined = undefined;
    try {
      file = await fs.open(path, fs.OpenMode.CREATE | fs.OpenMode.WRITE_ONLY | fs.OpenMode.TRUNC);
      let offset: number = 0;
      while (offset < bytes.byteLength) {
        const written = await fs.write(file.fd, bytes.buffer.slice(offset));
        if (!Number.isSafeInteger(written) || written <= 0 || written > bytes.byteLength - offset) {
          throw new Error('附件请求未能保存，未开始导入。');
        }
        offset += written;
      }
      await fs.fsync(file.fd);
    } finally { close(file); }
  }

  private async removeOwn(directory: string, unadmitted: boolean = false): Promise<void> {
    // Every directory passed here was created by this instance, or recovered
    // from the strict private spool namespace. No URI-derived path is accepted.
    const importId = directory.slice(this.root.length + 1);
    const exportId = directory.slice(this.context.cacheDir.length + 1);
    if (!(directory.startsWith(this.root + '/') && /^import-[A-Za-z0-9]{6}$/.test(importId)) &&
      !(directory.startsWith(this.context.cacheDir + '/') && /^morrow-(export|preview)-[A-Za-z0-9]{6}$/.test(exportId))) {
      throw new Error('附件缓存位置无效。');
    }
    const folder = await this.statIfPresent(directory);
    if (!folder) { return; }
    if (!folder.isDirectory() || folder.isSymbolicLink()) { throw new Error('附件缓存需要核对，已保留。'); }
    const names = await fs.listFile(directory);
    if (names.some((name: string): boolean => !['data', 'metadata.json', 'request.json'].includes(name))) {
      throw new Error('附件缓存需要核对，已保留。');
    }
    // SDK rmdir is recursive. Preflight every child before deleting anything
    // so it cannot follow an unexpected directory or erase unknown contents.
    for (const name of names) {
      const stat = await this.statIfPresent(directory + '/' + name);
      if (stat && (!stat.isFile() || stat.isSymbolicLink())) { throw new Error('附件缓存需要核对，已保留。'); }
    }
    if (unadmitted && await this.statIfPresent(directory + '/request.json')) {
      throw new Error('原附件请求已存在，缓存已保留。');
    }
    for (const name of names) {
      const path = directory + '/' + name;
      if (unadmitted && await this.statIfPresent(directory + '/request.json')) {
        throw new Error('原附件请求已存在，缓存已保留。');
      }
      if (await this.statIfPresent(path)) { await fs.unlink(path); }
    }
    if (unadmitted && await this.statIfPresent(directory + '/request.json')) {
      throw new Error('原附件请求已存在，缓存已保留。');
    }
    if ((await fs.listFile(directory)).length !== 0) { throw new Error('附件缓存需要核对，已保留。'); }
    await fs.rmdir(directory);
  }

  pick(maxBytes: number = ATTACHMENT_IMPORT_BUDGET): Promise<PreparedAttachment | undefined> {
    return this.queued(async (): Promise<PreparedAttachment | undefined> => {
      if (!Number.isSafeInteger(maxBytes) || maxBytes <= 0 || maxBytes > ATTACHMENT_IMPORT_BUDGET) {
        throw new Error('附件导入额度无效。');
      }
      const remaining = await this.remaining();
      const options = new picker.DocumentSelectOptions();
      options.maxSelectNumber = 1; options.selectMode = picker.DocumentSelectMode.FILE;
      const uris = await new picker.DocumentViewPicker(this.context).select(options);
      if (uris.length === 0) { return undefined; }
      if (uris.length !== 1 || !uris[0]) { throw new Error('请选择一个附件。'); }
      const directory = await fs.mkdtemp(this.root + '/import-XXXXXX');
      let source: fs.File | undefined = undefined;
      let spool: fs.File | undefined = undefined;
      let prepared: boolean = false;
      try {
        // Open the exact picker URI with the actual grant; never resolve it to
        // an Android-style path, persist a raw FD, or assume durable URI rights.
        source = await fs.open(uris[0], fs.OpenMode.READ_ONLY);
        spool = await fs.open(directory + '/data', fs.OpenMode.CREATE | fs.OpenMode.READ_WRITE);
        const reply = streamReply(await native.prepareFile(source.fd, spool.fd, Math.min(maxBytes, remaining)),
          Math.min(maxBytes, remaining));
        const stat = await fs.stat(spool.fd);
        if (!stat.isFile() || stat.size !== Number(reply.byte_length)) { throw new Error('附件长度验证失败。'); }
        const value = new PreparedAttachment();
        value.spool_id = directory.slice(this.root.length + 1);
        value.name = safeName(source.name); value.byte_length = reply.byte_length; value.sha256 = reply.sha256;
        const entry = new SpoolEntry(value, directory);
        await this.writeSmall(directory + '/metadata.json', JSON.stringify(value), 4096);
        this.entries.set(value.spool_id, entry); prepared = true; return value;
      } finally {
        close(source); close(spool);
        // Prepare has no Store/session effect, so a failed partial copy can be
        // removed even if the native preparation result was unavailable.
        if (!prepared) { try { await this.removeOwn(directory); } catch (_) {} }
      }
    });
  }

  recover(): Promise<PreparedAttachment[]> {
    return this.queued((): Promise<PreparedAttachment[]> => this.scanSpools());
  }

  private async scanSpools(): Promise<PreparedAttachment[]> {
      await this.ensureRoot();
      this.issues = [];
      const result: PreparedAttachment[] = [];
      for (const id of await fs.listFile(this.root)) {
        if (!/^import-[A-Za-z0-9]{6}$/.test(id)) { this.diagnose(id, 'UNRECOGNIZED_NAMESPACE', 'unknown'); continue; }
        const directory = this.root + '/' + id;
        const cached = this.entries.get(id);
        let requestState: string = 'unknown';
        let safeChildren: boolean = false;
        let issueCode: string = 'UNREADABLE_SPOOL';
        try {
          const folder = await fs.lstat(directory);
          if (!folder.isDirectory() || folder.isSymbolicLink()) { issueCode = 'UNSAFE_DIRECTORY'; throw new Error(); }
          const requestStat = await this.statIfPresent(directory + '/request.json');
          requestState = requestStat ? 'present' : 'absent';
          const names = await fs.listFile(directory);
          if (names.some((name: string): boolean => !['data', 'metadata.json', 'request.json'].includes(name))) {
            issueCode = 'UNEXPECTED_CONTENTS'; throw new Error();
          }
          const metadata = await this.statIfPresent(directory + '/metadata.json');
          const data = await this.statIfPresent(directory + '/data');
          if (data && (!data.isFile() || data.isSymbolicLink()) ||
            metadata && (!metadata.isFile() || metadata.isSymbolicLink())) {
            issueCode = 'UNSAFE_CONTENTS'; throw new Error();
          }
          safeChildren = true;
          if (requestStat && (!requestStat.isFile() || requestStat.isSymbolicLink() || requestStat.size <= 0 ||
            requestStat.size > MAX_REQUEST_BYTES)) { issueCode = 'REQUEST_INVALID'; throw new Error(); }
          issueCode = 'METADATA_INVALID';
          if (!metadata || metadata.size <= 0 || metadata.size > 4096 || !data || !Number.isSafeInteger(data.size) ||
            data.size < 0 || data.size > ATTACHMENT_IMPORT_BUDGET) { throw new Error(); }
          const value = JSON.parse(await fs.readText(directory + '/metadata.json', { length: 4096 })) as PreparedAttachment;
          const recovered: FileStreamReply = { ok: true, error: '', byte_length: value.byte_length, sha256: value.sha256 };
          streamReply(JSON.stringify(recovered), ATTACHMENT_IMPORT_BUDGET);
          if (value.spool_id !== id || typeof value.name !== 'string' || safeName(value.name) !== value.name ||
            data.size !== Number(value.byte_length)) { throw new Error(); }
          value.original_request = '';
          if (requestStat) {
            issueCode = 'REQUEST_INVALID';
            value.original_request = await fs.readText(directory + '/request.json', { length: MAX_REQUEST_BYTES });
            validImportRequest(value.original_request, value);
          }
          if (cached) {
            issueCode = 'CACHE_IDENTITY_CHANGED';
            if (cached.name !== value.name || cached.byte_length !== value.byte_length || cached.sha256 !== value.sha256 ||
              cached.request !== value.original_request) { throw new Error(); }
            result.push(cached.value);
          } else {
            this.entries.set(id, new SpoolEntry(value, directory)); result.push(value);
          }
        } catch (_) {
          this.entries.delete(id);
          if (requestState === 'absent' && safeChildren && !cached?.request) {
            try {
              await this.removeOwn(directory, true);
              this.diagnose(id, 'PREPARE_INTERRUPTED_CLEANED', 'absent', true); continue;
            } catch (_) { issueCode = 'PREPARE_CLEANUP_FAILED'; }
          }
          this.diagnose(id, issueCode, requestState);
        }
      }
      return result;
  }

  importPrepared(value: PreparedAttachment, serialized: string,
    send: (request: string, fd: number) => Promise<string>): Promise<string> {
    return this.queued(async (): Promise<string> => {
      const entry = this.checked(value);
      if (!serialized || serialized.indexOf('\u0000') >= 0) { throw new Error('附件请求无效。'); }
      validImportRequest(serialized, value);
      if (entry.request && entry.request !== serialized) { throw new Error('请先核对原附件请求，不能改写尚未确认的导入。'); }
      if (!entry.request) {
        // Journal the exact proposal before entering the Workbench admission
        // queue. Neither provider nor unknown native failures trigger a retry.
        await this.writeSmall(entry.directory + '/request.json', serialized, MAX_REQUEST_BYTES);
        entry.request = serialized; value.original_request = serialized;
      }
      let file: fs.File | undefined = undefined;
      try {
        file = await fs.open(entry.path, fs.OpenMode.READ_ONLY);
        // Keep the caller FD alive while Workbench awaits its single tail.
        return await send(entry.request, file.fd);
      } finally { close(file); }
    });
  }

  release(value: PreparedAttachment): Promise<void> {
    return this.queued(async (): Promise<void> => {
      const entry = this.checked(value);
      await this.removeOwn(entry.directory); this.entries.delete(value.spool_id);
    });
  }

  isPreviewActive(value: VerifiedPreview): boolean {
    const entry = this.previews.get(value.token);
    return entry !== undefined && entry.matches(value) && AttachmentFiles.activePreviews.get(entry.directory) === entry;
  }

  private previewLimit(purpose: string, expectedByteLength: string): number {
    if (purpose !== 'dialog' && purpose !== 'inline') { throw new Error('附件预览类型无效。'); }
    let count: number = 0;
    let used: number = 0;
    AttachmentFiles.activePreviews.forEach((entry: PreviewEntry): void => {
      if (entry.cache_root === this.context.cacheDir && entry.purpose === purpose) {
        count++; used += Number(entry.byte_length);
      }
    });
    if (purpose === 'dialog' && count > 0) { throw new Error('请先关闭当前附件预览。'); }
    if (purpose === 'inline' && count >= ATTACHMENT_INLINE_PREVIEW_LIMIT) { throw new Error('当前内容最多同时显示 8 张附件图片。'); }
    const limit = purpose === 'inline' ? ATTACHMENT_INLINE_PREVIEW_BUDGET - used : ATTACHMENT_EXPORT_LIMIT;
    // The current export ABI has no caller byte-limit argument. Inline callers
    // must pre-admit the exact Core/pin metadata before the native read, and the
    // completed stream is checked against that length as well as its whole hash.
    if (purpose === 'inline' || expectedByteLength !== '') {
      const bytes = Number(expectedByteLength);
      if (!/^(0|[1-9][0-9]*)$/.test(expectedByteLength) || !Number.isSafeInteger(bytes) || bytes > limit) {
        throw new Error('附件图片超过当前 64 MiB 预览额度，或长度无法确认。');
      }
    }
    return limit;
  }

  previewFile(serialized: string, name: string,
    send: (request: string, fd: number) => Promise<string>, purpose: string = 'dialog',
    expectedByteLength: string = ''): Promise<VerifiedPreview> {
    return this.queued(async (): Promise<VerifiedPreview> => {
      const limit = this.previewLimit(purpose, expectedByteLength);
      const directory = await fs.mkdtemp(this.context.cacheDir + '/morrow-preview-XXXXXX');
      let temporary: fs.File | undefined = undefined;
      let verified: boolean = false;
      try {
        temporary = await fs.open(directory + '/data', fs.OpenMode.CREATE | fs.OpenMode.READ_WRITE);
        const reply = streamReply(await send(serialized, temporary.fd), limit);
        if (expectedByteLength !== '' && reply.byte_length !== expectedByteLength) {
          throw new Error('附件长度已变化，未打开预览。');
        }
        const stat = await fs.stat(temporary.fd);
        if (!stat.isFile() || stat.size !== Number(reply.byte_length)) { throw new Error('附件长度验证失败，未打开预览。'); }
        await fs.fsync(temporary.fd);
        const value = new VerifiedPreview();
        value.token = directory.slice(this.context.cacheDir.length + 1);
        value.path = directory + '/data'; value.uri = fileUri.getUriFromPath(value.path); value.name = safeName(name);
        value.byte_length = reply.byte_length; value.sha256 = reply.sha256;
        value.purpose = purpose;
        // File extension is not used as authority; the Index receives the
        // selected attachment's MIME/name projection and chooses its viewer.
        const entry = new PreviewEntry(value, directory, this.context.cacheDir);
        this.previews.set(value.token, entry); AttachmentFiles.activePreviews.set(directory, entry);
        verified = true; return value;
      } finally {
        close(temporary);
        if (!verified) { try { await this.removeOwn(directory); } catch (_) {} }
      }
    });
  }

  releasePreview(token: string): Promise<void> {
    return this.queued(async (): Promise<void> => {
      const entry = this.previews.get(token);
      if (!entry) { return; }
      // A different helper may have completed an explicit failed-cleanup
      // retry. Discard this stale local reference without touching disk again.
      if (AttachmentFiles.activePreviews.get(entry.directory) !== entry) { this.previews.delete(token); return; }
      await this.cleanupPreview(entry);
    });
  }

  private async cleanupPreview(entry: PreviewEntry): Promise<void> {
    try {
      await this.removeOwn(entry.directory);
      AttachmentFiles.activePreviews.delete(entry.directory);
      if (this.previews.get(entry.token) === entry) { this.previews.delete(entry.token); }
    } catch (_) {
      // UI owners can disappear before removal completes. Keep the immutable
      // registered directory and its quota until an explicit retry succeeds.
      entry.cleanup_failed = true;
      throw new Error('预览缓存清理尚未完成，请明确重试清理。');
    }
  }

  previewCleanupPending(): boolean {
    let pending: boolean = false;
    AttachmentFiles.activePreviews.forEach((entry: PreviewEntry): void => {
      if (entry.cache_root === this.context.cacheDir && entry.cleanup_failed) { pending = true; }
    });
    return pending;
  }

  retryPreviewCleanup(): Promise<void> {
    return this.queued(async (): Promise<void> => {
      const pending: PreviewEntry[] = [];
      AttachmentFiles.activePreviews.forEach((entry: PreviewEntry): void => {
        if (entry.cache_root === this.context.cacheDir && entry.cleanup_failed) { pending.push(entry); }
      });
      let failed: boolean = false;
      for (const entry of pending) {
        if (AttachmentFiles.activePreviews.get(entry.directory) !== entry) { continue; }
        try { await this.cleanupPreview(entry); } catch (_) { failed = true; }
      }
      if (failed) { throw new Error('部分预览缓存仍未清理，请保留提示并再次核对。'); }
    });
  }

  exportToPicker(serialized: string, name: string,
    send: (request: string, fd: number) => Promise<string>): Promise<boolean> {
    return this.queued(async (): Promise<boolean> => {
      const directory = await fs.mkdtemp(this.context.cacheDir + '/morrow-export-XXXXXX');
      let temporary: fs.File | undefined = undefined;
      let source: fs.File | undefined = undefined;
      let destination: fs.File | undefined = undefined;
      try {
        temporary = await fs.open(directory + '/data', fs.OpenMode.CREATE | fs.OpenMode.READ_WRITE);
        const reply = streamReply(await send(serialized, temporary.fd), ATTACHMENT_EXPORT_LIMIT);
        const stat = await fs.stat(temporary.fd);
        if (!stat.isFile() || stat.size !== Number(reply.byte_length)) { throw new Error('附件长度验证失败，未打开保存入口。'); }
        close(temporary); temporary = undefined;
        // Rust first verifies the selected blob/hash into a private temporary
        // FD. No unverified bytes are written to a user-selected destination.
        const options = new picker.DocumentSaveOptions(); options.newFileNames = [safeName(name)];
        const uris = await new picker.DocumentViewPicker(this.context).save(options);
        if (uris.length === 0) { return false; }
        if (uris.length !== 1 || !uris[0]) { throw new Error('附件保存位置无效。'); }
        source = await fs.open(directory + '/data', fs.OpenMode.READ_ONLY);
        // Picker may return a newly allocated or user-confirmed existing URI.
        // CREATE/TRUNC establishes a zero-length destination, including a
        // shorter replacement of a previously longer file. Unsupported flags,
        // sync or provider stat are a failure, never an inferred success.
        try {
          destination = await fs.open(uris[0], fs.OpenMode.WRITE_ONLY | fs.OpenMode.CREATE | fs.OpenMode.TRUNC);
          await fs.copyFile(source.fd, destination.fd);
          await fs.fsync(destination.fd);
          const destinationStat = await fs.stat(destination.fd);
          if (!destinationStat.isFile() || destinationStat.size !== Number(reply.byte_length)) {
            throw new Error('保存目标长度不符。');
          }
        } catch (_) { throw new Error('附件保存尚未确认，请检查目标文件；未自动重试。'); }
        return true;
      } finally {
        close(temporary); close(source); close(destination);
        try { await this.removeOwn(directory); } catch (_) {}
      }
    });
  }
}
