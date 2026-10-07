import { common } from '@kit.AbilityKit';
import { fileUri } from '@kit.CoreFileKit';
import { filePreview } from '@kit.PreviewKit';
import { VerifiedPreview } from './AttachmentFiles';

export class AttachmentOpenState {
  phase: string = 'idle';
  name: string = '';
  message: string = '';
  retained: boolean = false;
  busy: boolean = false;
  can_finish: boolean = false;
}

// A filename chooses only a viewer hint. It never chooses bytes, a path, an
// ability, or authority. PreviewKit still decides whether it supports the file.
function extension(name: string): string {
  const base = name.trim().split(/[\\/]/).pop() || '';
  const dot = base.lastIndexOf('.');
  return dot < 0 ? '' : base.slice(dot + 1).toLowerCase();
}

export function attachmentOpenBlocked(name: string): boolean {
  return ['exe', 'com', 'msi', 'bat', 'cmd', 'ps1', 'vbs', 'js', 'lnk', 'url', 'scr', 'reg']
    .includes(extension(name));
}

export function attachmentPreviewMime(name: string): string {
  const ext = extension(name);
  // Values in the installed API26 PreviewKit declaration's supported-type
  // table. An unrecognised suffix is left empty, as PreviewInfo specifies.
  switch (ext) {
    case 'txt': return 'text/plain';
    case 'cpp': return 'text/x-c++src';
    case 'c': return 'text/x-csrc';
    case 'h': return 'text/x-chdr';
    case 'java': return 'text/x-java';
    case 'xhtml': return 'application/xhtml+xml';
    case 'xml': return 'text/xml';
    case 'html': case 'htm': return 'text/html';
    case 'jpg': case 'jpeg': return 'image/jpeg';
    case 'png': return 'image/png';
    case 'gif': return 'image/gif';
    case 'webp': return 'image/webp';
    case 'bmp': return 'image/bmp';
    case 'svg': return 'image/svg+xml';
    case 'm4a': return 'audio/mp4a-latm';
    case 'aac': return 'audio/aac';
    case 'mp3': return 'audio/mpeg';
    case 'ogg': return 'audio/ogg';
    case 'wav': return 'audio/x-wav';
    case 'mp4': return 'video/mp4';
    case 'mkv': return 'video/x-matroska';
    case 'ts': return 'video/mp2ts';
    case 'pdf': return 'application/pdf';
    case 'doc': return 'application/msword';
    case 'docx': return 'application/vnd.openxmlformats-officedocument.wordprocessingml.document';
    case 'xls': return 'application/vnd.ms-excel';
    case 'xlsx': return 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet';
    case 'ppt': return 'application/vnd.ms-powerpoint';
    case 'pptx': return 'application/vnd.openxmlformats-officedocument.presentationml.presentation';
    case 'csv': return 'text/csv';
    case 'ofd': return 'general.ofd';
    default: return '';
  }
}

class OpenLease {
  handle: VerifiedPreview;
  token: string;
  path: string;
  uri: string;
  name: string;
  byte_length: string;
  sha256: string;
  purpose: string;
  dispatched: boolean = false;
  active: (lease: VerifiedPreview) => boolean;
  release: (token: string) => Promise<void>;
  constructor(handle: VerifiedPreview, active: (lease: VerifiedPreview) => boolean,
    release: (token: string) => Promise<void>) {
    this.handle = handle; this.token = handle.token; this.path = handle.path;
    this.uri = handle.uri; this.name = handle.name; this.byte_length = handle.byte_length;
    this.sha256 = handle.sha256; this.purpose = handle.purpose;
    this.active = active; this.release = release;
  }
  valid(): boolean {
    return this.handle.token === this.token && this.handle.path === this.path && this.handle.uri === this.uri &&
      this.handle.name === this.name && this.handle.byte_length === this.byte_length &&
      this.handle.sha256 === this.sha256 && this.handle.purpose === this.purpose && this.active(this.handle);
  }
}

class OpenSession {
  lease: OpenLease | undefined = undefined;
  state: AttachmentOpenState = new AttachmentOpenState();
  tail: Promise<void> = Promise.resolve();
  changed: (state: AttachmentOpenState) => void;
  constructor(changed: (state: AttachmentOpenState) => void) { this.changed = changed; }
}

// The preview service exposes a singleton window, not a reader-completion
// event. Keep one immutable known lease per cache root across UI adapters.
// Foreground/window-close observations NEVER delete it: PreviewKit can hand
// the URI to another app through its own open/share controls. Only the explicit
// finishReadersClosed action asserts those readers have finished.
export class AttachmentOpen {
  private context: common.UIAbilityContext;
  private active: (lease: VerifiedPreview) => boolean;
  private release: (token: string) => Promise<void>;
  private session: OpenSession;
  private static sessions: Map<string, OpenSession> = new Map<string, OpenSession>();

  constructor(context: common.UIAbilityContext, active: (lease: VerifiedPreview) => boolean,
    release: (token: string) => Promise<void>, changed: (state: AttachmentOpenState) => void) {
    this.context = context; this.active = active; this.release = release;
    const existing = AttachmentOpen.sessions.get(context.cacheDir);
    if (existing) { this.session = existing; existing.changed = changed; }
    else {
      this.session = new OpenSession(changed);
      AttachmentOpen.sessions.set(context.cacheDir, this.session);
    }
  }

  snapshot(): AttachmentOpenState {
    const current = this.session.state;
    const copy = new AttachmentOpenState();
    copy.phase = current.phase; copy.name = current.name; copy.message = current.message;
    copy.retained = this.session.lease !== undefined; copy.busy = current.busy; copy.can_finish = current.can_finish;
    return copy;
  }

  owns(lease: VerifiedPreview): boolean { return this.session.lease?.handle === lease; }

  private show(phase: string, message: string, busy: boolean = false, canFinish: boolean = false): void {
    const state = this.session.state;
    state.phase = phase; state.message = message; state.busy = busy; state.can_finish = canFinish;
    state.retained = this.session.lease !== undefined;
    try { this.session.changed(this.snapshot()); } catch (_) {}
  }

  private queued(operation: () => Promise<void>): Promise<void> {
    const result = this.session.tail.then(operation);
    this.session.tail = result.then((): void => {}, (): void => {});
    return result;
  }

  private admissible(lease: VerifiedPreview): boolean {
    const bytes = Number(lease.byte_length);
    const prefix = this.context.cacheDir + '/' + lease.token + '/';
    const leaf = lease.path.slice(prefix.length);
    if (lease.purpose !== 'open' || !/^morrow-preview-[A-Za-z0-9]{6}$/.test(lease.token) ||
      !lease.path.startsWith(prefix) || !leaf || leaf === '.' || leaf === '..' || leaf.indexOf('/') >= 0 ||
      /[\u0000-\u001f\u007f\\]/.test(lease.path) ||
      !lease.name || lease.name.length > 128 || !/^(0|[1-9][0-9]*)$/.test(lease.byte_length) ||
      !Number.isSafeInteger(bytes) || bytes > 200 * 1024 * 1024 || !/^[0-9a-f]{64}$/.test(lease.sha256)) { return false; }
    try { return this.active(lease) && lease.uri === fileUri.getUriFromPath(lease.path); }
    catch (_) { return false; }
  }

  // false means the caller still owns the rejected candidate. true means this
  // controller adopted it, even if dispatch/cleanup later needs reconciliation.
  open(lease: VerifiedPreview): Promise<boolean> {
    if (this.session.lease || this.session.state.busy || !this.admissible(lease)) { return Promise.resolve(false); }
    const entry = new OpenLease(lease, this.active, this.release);
    this.session.lease = entry; this.session.state.name = entry.name;
    this.show('checking', '正在核对系统预览…', true);
    return this.queued(async (): Promise<void> => {
      try {
        if (attachmentOpenBlocked(entry.name)) {
          await this.releaseBeforeDispatch(entry, 'unsupported', '此文件类型不直接打开，可另存后查看。'); return;
        }
        if (await filePreview.hasDisplayed(this.context)) {
          await this.releaseBeforeDispatch(entry, 'error', '系统已有预览窗口，请先关闭后再打开附件。'); return;
        }
        if (!entry.valid()) {
          await this.releaseBeforeDispatch(entry, 'error', '附件校验句柄已变化，未打开文件。'); return;
        }
        if (!await filePreview.canPreview(this.context, entry.uri)) {
          await this.releaseBeforeDispatch(entry, 'unsupported', '系统暂不支持预览此文件，可另存后查看。'); return;
        }
        if (!entry.valid()) {
          await this.releaseBeforeDispatch(entry, 'error', '附件校验句柄已变化，未打开文件。'); return;
        }
        const info: filePreview.PreviewInfo = { title: entry.name, uri: entry.uri, mimeType: attachmentPreviewMime(entry.name) };
        entry.dispatched = true;
        this.show('opening', '正在打开系统预览…', true);
        await filePreview.openPreview(this.context, info);
        await this.observe(entry);
      } catch (_) {
        if (!entry.dispatched) { await this.releaseBeforeDispatch(entry, 'error', '系统预览核对失败，未打开文件。'); }
        else { this.show('unknown', '系统预览结果尚未确认，临时文件已保留。请关闭查看后核对。'); }
      }
    }).then((): boolean => true);
  }

  private async releaseBeforeDispatch(entry: OpenLease, phase: string, message: string): Promise<void> {
    try {
      await entry.release(entry.token);
      if (this.session.lease === entry) { this.session.lease = undefined; }
      this.show(phase, message);
    } catch (_) { this.show('cleanup_failed', message + ' 临时文件清理未完成，可明确重试。', false, true); }
  }

  private async observe(entry: OpenLease): Promise<void> {
    if (this.session.lease !== entry) { return; }
    if (!entry.dispatched) {
      this.show('cleanup_failed', '文件未交给系统预览，临时文件清理尚未完成，请明确重试。', false, true); return;
    }
    try {
      const displayed = await filePreview.hasDisplayed(this.context);
      if (this.session.lease !== entry) { return; }
      if (displayed) { this.show('displayed', '系统预览已打开，临时文件将在结束查看后清理。'); }
      else {
        this.show('retained', '系统预览窗口已关闭。请先结束其他应用中的查看，再清理临时文件。', false, true);
      }
    } catch (_) { this.show('unknown', '系统预览状态无法确认，临时文件已保留。请稍后核对。'); }
  }

  reconcile(): Promise<void> {
    return this.queued(async (): Promise<void> => {
      const entry = this.session.lease;
      if (entry) { this.show('checking', '正在核对系统预览…', true); await this.observe(entry); }
    });
  }

  // Closes only the known dispatched singleton. It does not assert that other
  // apps opened from PreviewKit have finished and therefore does not delete.
  close(): Promise<void> {
    return this.queued(async (): Promise<void> => {
      const entry = this.session.lease;
      if (!entry) { return; }
      this.show('closing', '正在关闭系统预览…', true);
      try {
        if (entry.dispatched && await filePreview.hasDisplayed(this.context)) { await filePreview.closePreview(this.context); }
        await this.observe(entry);
      } catch (_) { this.show('unknown', '关闭结果无法确认，临时文件已保留。请明确核对。'); }
    });
  }

  // A direct user action in the product asserts that all readers are finished.
  // Verify the observable system window again; failed/unknown closure or file
  // removal always retains the immutable original token for an explicit retry.
  finishReadersClosed(): Promise<void> {
    return this.queued(async (): Promise<void> => {
      const entry = this.session.lease;
      if (!entry) { return; }
      this.show('checking', '正在核对结束查看…', true);
      try {
        if (entry.dispatched && await filePreview.hasDisplayed(this.context)) {
          this.show('displayed', '系统预览仍在显示，请先关闭系统预览。'); return;
        }
      } catch (_) { this.show('unknown', '系统预览状态无法确认，临时文件已保留。'); return; }
      try {
        await entry.release(entry.token);
        if (this.session.lease === entry) { this.session.lease = undefined; }
        this.show('idle', '本次预览的临时文件已清理。');
      } catch (_) { this.show('cleanup_failed', '临时文件清理未完成，已保留原句柄，请明确重试。', false, true); }
    });
  }
}
