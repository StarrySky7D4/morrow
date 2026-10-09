import { common } from '@kit.AbilityKit';
import { fileIo as fs, picker } from '@kit.CoreFileKit';
import { BusinessError } from '@kit.BasicServicesKit';
import { util } from '@kit.ArkTS';
import native from 'libmorrow.so';
import { workbench } from './Workbench';
import { MusicPlaybackIdentity, MusicPlaybackSource, MusicPlaybackDescriptor, MusicPlaybackFiles } from './MusicPlayback';

export const MUSIC_FILE_LIMIT: number = 150 * 1024 * 1024;
const MUSIC_SPOOL_BUDGET: number = 512 * 1024 * 1024;
const MUSIC_PREVIEW_BUDGET: number = 300 * 1024 * 1024;
const REQUEST_LIMIT: number = 512 * 1024;
const RESERVE: number = REQUEST_LIMIT + 4096;
export class PreparedMusicFile {
  spoolId: string = '';
  name: string = '';
  byteLength: string = '0';
  sha256: string = '';
  requestJson: string = '';
}
export class SelectedMusicLyrics { name: string = ''; text: string = ''; byteLength: string = '0'; sha256: string = ''; }
interface StreamReply { ok: boolean; error: string; byte_length: string; sha256: string; }
interface MusicFileRequest {
  schema_version: number; track_id: string; operation_id: string; expected_revision: string;
  name: string; byte_length: string; sha256: string;
}
interface MusicFileEnvelope { action: string; music: MusicFileAction; }
interface MusicFileAction { action: string; request: MusicFileRequest; }
interface MusicExportEnvelope { action: string; music: MusicExportAction; }
interface MusicExportAction {
  action: string; library_revision: string; track_id: string; import_operation: string;
  byte_length: string; sha256: string;
}
class MusicSpool {
  value: PreparedMusicFile;
  directory: string;
  signature: string;
  // Set only after the exact owned directory passed removal checks. This is
  // process-local cleanup ownership, never recovered from disk as replay data.
  releaseStarted: boolean = false;
  constructor(value: PreparedMusicFile, directory: string) {
    this.value = value; this.directory = directory; this.signature = JSON.stringify(value);
  }
}
class MusicLease {
  source: MusicPlaybackSource;
  directory: string;
  path: string;
  signature: string;
  descriptor: MusicPlaybackDescriptor | undefined = undefined;
  file: fs.File | undefined = undefined;
  constructor(source: MusicPlaybackSource, directory: string) {
    this.source = source; this.directory = directory; this.path = directory + '/data'; this.signature = JSON.stringify(source);
  }
}
function positive(value: string, maximum: number): boolean {
  return typeof value === 'string' && /^[1-9][0-9]*$/.test(value) && Number.isSafeInteger(Number(value)) && Number(value) <= maximum;
}
function decimal(value: string): boolean {
  return typeof value === 'string' && /^(0|[1-9][0-9]*)$/.test(value) && value.length <= 20 &&
    (value.length < 20 || value <= '18446744073709551615');
}
function musicName(name: string): string {
  if (typeof name !== 'string') { throw new Error('音乐文件名称无法确认。'); }
  const result = name.split(/[\\/]/).pop() || '';
  if (!result || result === '.' || result === '..' || new util.TextEncoder().encodeInto(result).byteLength > 512 || /[\u0000-\u001f\u007f]/.test(result)) {
    throw new Error('音乐文件名称无法确认。');
  }
  return result;
}
function fileReply(raw: string, maximum: number): StreamReply {
  const value = JSON.parse(raw) as StreamReply;
  if (!value || value.ok !== true || value.error !== '' || !positive(value.byte_length, maximum) ||
    typeof value.sha256 !== 'string' || !/^[0-9a-f]{64}$/.test(value.sha256)) { throw new Error('音乐文件完整字节核验未完成。'); }
  return value;
}
function importLiteral(raw: string, value: PreparedMusicFile): void {
  if (!raw || raw.includes('\u0000') || new util.TextEncoder().encodeInto(raw).byteLength > REQUEST_LIMIT) { throw new Error('音乐导入请求无法确认。'); }
  const envelope = JSON.parse(raw) as MusicFileEnvelope, action = envelope?.music, request = action?.request;
  if (!envelope || Object.keys(envelope).sort().join(',') !== 'action,music' || envelope.action !== 'music_import' ||
    !action || Object.keys(action).sort().join(',') !== 'action,request' || action.action !== 'import_file' || !request ||
    Object.keys(request).sort().join(',') !== 'byte_length,expected_revision,name,operation_id,schema_version,sha256,track_id' ||
    request.schema_version !== 1 || !decimal(request.expected_revision) ||
    typeof request.track_id !== 'string' || !/^[A-Za-z0-9_.-]{1,64}$/.test(request.track_id) || request.track_id.startsWith('morrow-host-') ||
    typeof request.operation_id !== 'string' || !/^[A-Za-z0-9_.-]{1,128}$/.test(request.operation_id) || request.operation_id.startsWith('morrow-host-') ||
    request.name !== value.name || request.byte_length !== value.byteLength || request.sha256 !== value.sha256) {
    throw new Error('音乐原请求与完整文件不一致，已保留。');
  }
}
function checkedIdentity(value: MusicPlaybackIdentity): void {
  if (!value || !/^[A-Za-z0-9_.-]{1,64}$/.test(value.trackId) || value.trackId.startsWith('morrow-host-') ||
    !/^[A-Za-z0-9_.-]{1,128}$/.test(value.importOperation) || value.importOperation.startsWith('morrow-host-') ||
    !decimal(value.libraryRevision) || !Number.isSafeInteger(value.byteLength) || value.byteLength < 1 || value.byteLength > MUSIC_FILE_LIMIT ||
    !/^[0-9a-f]{64}$/.test(value.sha256)) { throw new Error('音乐曲目来源无法确认。'); }
}

// Persistent source lives in the independent native music library. These
// spools and playback files only carry captured bytes and exact fixed requests.
// Provider URIs never become persistent retry sources or player addresses.
export class MusicFiles implements MusicPlaybackFiles {
  private root: string;
  private context: common.UIAbilityContext;
  private selection: Set<string> = new Set<string>();
  private entries: Map<string, MusicSpool> = new Map<string, MusicSpool>();
  private leases: Map<string, MusicLease> = new Map<string, MusicLease>();
  private issues: string[] = [];
  private static active: Map<string, MusicLease> = new Map<string, MusicLease>();
  private static pendingIo: Map<string, fs.File[]> = new Map<string, fs.File[]>();
  private static tail: Promise<void> = Promise.resolve();
  constructor(context: common.UIAbilityContext) { this.context = context; this.root = context.filesDir + '/hmos-music-spool'; }
  private queued<T>(work: () => Promise<T>): Promise<T> {
    const result = MusicFiles.tail.then(work); MusicFiles.tail = result.then((): void => {}, (): void => {}); return result;
  }
  private async present(path: string): Promise<fs.Stat | undefined> {
    try { return await fs.lstat(path); }
    catch (error) { if ((error as BusinessError).code === 13900002) { return undefined; } throw new Error('音乐缓存无法读取，已保留。'); }
  }
  private async ensureRoot(): Promise<void> {
    if (!await this.present(this.root)) { await fs.mkdir(this.root); }
    const stat = await fs.lstat(this.root); if (!stat.isDirectory() || stat.isSymbolicLink()) { throw new Error('音乐缓存目录不可用。'); }
  }
  private async writeSmall(path: string, value: string, maximum: number): Promise<void> {
    const bytes = new util.TextEncoder().encodeInto(value); if (bytes.byteLength > maximum) { throw new Error('音乐缓存记录超过预算。'); }
    const current = await this.present(path);
    if (current && (!current.isFile() || current.isSymbolicLink())) { throw new Error('音乐缓存记录已变化，已保留。'); }
    const directory = path.slice(0, path.lastIndexOf('/'));
    const file = await fs.open(path, fs.OpenMode.CREATE | fs.OpenMode.WRITE_ONLY | fs.OpenMode.TRUNC | fs.OpenMode.NOFOLLOW);
    try {
      let offset: number = 0;
      while (offset < bytes.byteLength) {
        const count = await fs.write(file.fd, bytes.buffer.slice(offset));
        if (!Number.isSafeInteger(count) || count <= 0 || count > bytes.byteLength - offset) { throw new Error('音乐缓存未能完整保存。'); }
        offset += count;
      }
      await fs.fsync(file.fd);
    } finally { await this.closeOwned(directory, file); }
  }
  private checked(value: PreparedMusicFile): MusicSpool {
    const entry = this.entries.get(value.spoolId);
    if (!entry || entry.value !== value || entry.signature !== JSON.stringify(value)) { throw new Error('音乐导入缓存身份已变化。'); }
    return entry;
  }
  private async closeOwned(directory: string, file: fs.File): Promise<void> {
    try { await fs.close(file); }
    catch (error) {
      const retained = MusicFiles.pendingIo.get(directory) || [];
      if (!retained.includes(file)) { retained.push(file); MusicFiles.pendingIo.set(directory, retained); }
      this.issues.push('音乐文件句柄关闭未确认，请显式重试清理。'); throw error as Error;
    }
  }
  private async removeOwn(directory: string, spool: boolean, beforeRemoval?: () => void): Promise<void> {
    if (MusicFiles.pendingIo.has(directory)) { throw new Error('音乐文件句柄尚未确认关闭，已保留缓存。'); }
    const root = spool ? this.root : this.context.cacheDir;
    const name = directory.slice(root.length + 1);
    if (!directory.startsWith(root + '/') || !(spool ? /^music-[A-Za-z0-9]{6}$/ : /^morrow-music-[A-Za-z0-9]{6}$/).test(name)) {
      throw new Error('音乐缓存位置无法确认。');
    }
    const folder = await this.present(directory); if (!folder) { return; }
    if (!folder.isDirectory() || folder.isSymbolicLink()) { throw new Error('音乐缓存目录已变化，已保留。'); }
    const names = await fs.listFile(directory), allowed = spool ? ['data', 'metadata.json', 'request.json'] : ['data'];
    for (const child of names) {
      const stat = await fs.lstat(directory + '/' + child);
      if (!allowed.includes(child) || !stat.isFile() || stat.isSymbolicLink()) { throw new Error('音乐缓存含未知内容，已保留。'); }
    }
    if (beforeRemoval) { beforeRemoval(); }
    for (const child of names) { await fs.unlink(directory + '/' + child); }
    if ((await fs.listFile(directory)).length !== 0) { throw new Error('音乐缓存清理未确认。'); }
    await fs.rmdir(directory);
  }
  private async remaining(): Promise<number> {
    await this.ensureRoot(); const names = await fs.listFile(this.root);
    if (names.length >= 20) { throw new Error('已有20份音乐导入缓存，请先核对。'); }
    let used: number = 0;
    for (const name of names) {
      if (!/^music-[A-Za-z0-9]{6}$/.test(name)) { throw new Error('音乐缓存含未知记录，请先核对。'); }
      const directory = this.root + '/' + name, folder = await fs.lstat(directory);
      if (!folder.isDirectory() || folder.isSymbolicLink()) { throw new Error('音乐缓存含未知记录，请先核对。'); }
      let sidecars: number = 0;
      for (const child of await fs.listFile(directory)) {
        const stat = await fs.lstat(directory + '/' + child);
        if (!['data', 'metadata.json', 'request.json'].includes(child) || !stat.isFile() || stat.isSymbolicLink() || !Number.isSafeInteger(stat.size) || stat.size < 0) {
          throw new Error('音乐缓存含未知内容，请先核对。');
        }
        if (child === 'data') { used += stat.size; } else { sidecars += stat.size; }
      }
      used += Math.max(RESERVE, sidecars);
    }
    const available = MUSIC_SPOOL_BUDGET - used - RESERVE;
    if (available <= 0) { throw new Error('音乐缓存达到512 MiB，请先核对已有导入。'); }
    return Math.min(MUSIC_FILE_LIMIT, available);
  }
  selectUris(maximum: number = 20): Promise<string[]> {
    return this.queued(async (): Promise<string[]> => {
      if (!Number.isInteger(maximum) || maximum < 1 || maximum > 20 || this.selection.size) { throw new Error('音乐选择额度无法确认。'); }
      await this.remaining(); const options = new picker.DocumentSelectOptions(); options.maxSelectNumber = maximum; options.selectMode = picker.DocumentSelectMode.FILE;
      const uris = await new picker.DocumentViewPicker(this.context).select(options);
      if (!Array.isArray(uris) || uris.length > maximum || new Set<string>(uris).size !== uris.length ||
        uris.some((uri: string): boolean => typeof uri !== 'string' || !uri || uri.length > 16384 || /[\u0000-\u001f\u007f]/.test(uri))) {
        throw new Error('系统返回的音乐选择无法确认。');
      }
      this.selection = new Set<string>(uris); return uris;
    });
  }
  discardSelection(): void { this.selection.clear(); }
  pickLyrics(): Promise<SelectedMusicLyrics | undefined> {
    return this.queued(async (): Promise<SelectedMusicLyrics | undefined> => {
      if (this.selection.size) { throw new Error('请先完成当前音乐选择。'); }
      const options = new picker.DocumentSelectOptions(); options.maxSelectNumber = 1; options.selectMode = picker.DocumentSelectMode.FILE;
      const uris = await new picker.DocumentViewPicker(this.context).select(options);
      if (!Array.isArray(uris) || uris.length > 1 || uris.some((uri: string): boolean =>
        typeof uri !== 'string' || !uri || uri.length > 16384 || /[\u0000-\u001f\u007f]/.test(uri))) { throw new Error('歌词文件选择无法确认。'); }
      if (!uris.length) { return undefined; }
      const directory = await fs.mkdtemp(this.context.cacheDir + '/morrow-music-XXXXXX');
      let input: fs.File | undefined = undefined, output: fs.File | undefined = undefined;
      try {
        input = await fs.open(uris[0], fs.OpenMode.READ_ONLY); const name = musicName(input.name);
        if (!/\.(lrc|txt)$/i.test(name)) { throw new Error('请选择 LRC 或 TXT 歌词。'); }
        output = await fs.open(directory + '/data', fs.OpenMode.CREATE | fs.OpenMode.READ_WRITE);
        const reply = fileReply(await native.prepareMusicFile(input.fd, output.fd, 1024 * 1024), 1024 * 1024);
        const stat = await fs.stat(output.fd); if (!stat.isFile() || stat.size !== Number(reply.byte_length)) { throw new Error('歌词文件长度无法确认。'); }
        const bytes = new ArrayBuffer(Number(reply.byte_length)); let offset: number = 0;
        while (offset < bytes.byteLength) {
          const chunk = new ArrayBuffer(bytes.byteLength - offset);
          const count = await fs.read(output.fd, chunk, { offset: offset, length: chunk.byteLength });
          if (!Number.isSafeInteger(count) || count <= 0 || count > chunk.byteLength) { throw new Error('歌词文件未完整读回。'); }
          new Uint8Array(bytes).set(new Uint8Array(chunk, 0, count), offset); offset += count;
        }
        const value = new SelectedMusicLyrics(); value.name = name; value.byteLength = reply.byte_length; value.sha256 = reply.sha256;
        value.text = util.TextDecoder.create('utf-8', { fatal: true }).decodeToString(new Uint8Array(bytes));
        if (!value.text.trim() || value.text.includes('\u0000')) { throw new Error('歌词为空或含无效字符。'); }
        return value;
      } finally {
        try { if (input) { await this.closeOwned(directory, input); } } finally {
          try { if (output) { await this.closeOwned(directory, output); } } finally {
            try { await this.removeOwn(directory, false); } catch (_) { this.issues.push('歌词临时文件清理未确认，已保留。'); }
          }
        }
      }
    });
  }
  prepareUri(uri: string): Promise<PreparedMusicFile> {
    return this.queued(async (): Promise<PreparedMusicFile> => {
      if (!this.selection.delete(uri)) { throw new Error('此文件不是尚未读取的音乐选择。'); }
      const limit = await this.remaining(), directory = await fs.mkdtemp(this.root + '/music-XXXXXX');
      let source: fs.File | undefined = undefined, destination: fs.File | undefined = undefined, admitted: boolean = false;
      try {
        source = await fs.open(uri, fs.OpenMode.READ_ONLY); const name = musicName(source.name);
        if (!/\.(mp3|wav|flac|ogg|m4a|aac|opus|wma|ape|aif|aiff|alac|wv|dsf|dff|mp2|ac3|dts|au)$/i.test(name)) { throw new Error('请选择标准音频文件。'); }
        destination = await fs.open(directory + '/data', fs.OpenMode.CREATE | fs.OpenMode.READ_WRITE);
        const reply = fileReply(await native.prepareMusicFile(source.fd, destination.fd, limit), limit);
        const stat = await fs.stat(destination.fd); if (!stat.isFile() || stat.size !== Number(reply.byte_length)) { throw new Error('音乐文件长度不一致。'); }
        const value = new PreparedMusicFile(); value.spoolId = directory.slice(this.root.length + 1); value.name = name; value.byteLength = reply.byte_length; value.sha256 = reply.sha256;
        await this.writeSmall(directory + '/metadata.json', JSON.stringify(value), 4096); this.entries.set(value.spoolId, new MusicSpool(value, directory)); admitted = true; return value;
      } finally {
        try { if (source) { await this.closeOwned(directory, source); } } finally {
          try { if (destination) { await this.closeOwned(directory, destination); } } finally {
            if (!admitted) { try { await this.removeOwn(directory, true); } catch (_) { this.issues.push('中断的音乐准备缓存清理失败，已保留。'); } }
          }
        }
      }
    });
  }
  recover(): Promise<PreparedMusicFile[]> {
    return this.queued(async (): Promise<PreparedMusicFile[]> => {
      await this.ensureRoot(); this.issues = []; const result: PreparedMusicFile[] = [];
      for (const id of await fs.listFile(this.root)) {
        const original = this.entries.get(id);
        if (original?.releaseStarted) {
          // Some of data/metadata/request may already be gone. Preserve only
          // the original registered object for explicit release retry; never
          // present this as a complete audio source or admit another FD import.
          this.issues.push('原音乐导入缓存已开始清理，请继续原清理；不可重新导入。');
          continue;
        }
        try {
          if (!/^music-[A-Za-z0-9]{6}$/.test(id)) { throw new Error('未知音乐缓存名称'); }
          const directory = this.root + '/' + id, folder = await fs.lstat(directory);
          if (!folder.isDirectory() || folder.isSymbolicLink()) { throw new Error('音乐缓存目录已变化'); }
          const children = await fs.listFile(directory); if (children.some((n: string): boolean => !['data', 'metadata.json', 'request.json'].includes(n))) { throw new Error('音乐缓存内容已变化'); }
          const metadata = await fs.lstat(directory + '/metadata.json'), data = await fs.lstat(directory + '/data');
          if (!metadata.isFile() || metadata.isSymbolicLink() || metadata.size > 4096 || metadata.size < 1 || !data.isFile() || data.isSymbolicLink()) { throw new Error('音乐缓存数据无法确认'); }
          const value = JSON.parse(await fs.readText(directory + '/metadata.json', { length: 4096 })) as PreparedMusicFile;
          if (Object.keys(value).sort().join(',') !== 'byteLength,name,requestJson,sha256,spoolId' || value.spoolId !== id ||
            musicName(value.name) !== value.name || !positive(value.byteLength, MUSIC_FILE_LIMIT) || !/^[0-9a-f]{64}$/.test(value.sha256) || data.size !== Number(value.byteLength) || value.requestJson !== '') { throw new Error('音乐缓存元数据无法确认'); }
          const request = await this.present(directory + '/request.json');
          if (request) {
            if (!request.isFile() || request.isSymbolicLink() || request.size < 1 || request.size > REQUEST_LIMIT) { throw new Error('音乐原请求无法读取'); }
            value.requestJson = await fs.readText(directory + '/request.json', { length: REQUEST_LIMIT }); importLiteral(value.requestJson, value);
          }
          const old = this.entries.get(id);
          if (old && old.signature !== JSON.stringify(value)) { throw new Error('音乐缓存身份已变化'); }
          if (old) { result.push(old.value); } else { this.entries.set(id, new MusicSpool(value, directory)); result.push(value); }
        } catch (_) { this.entries.delete(id); this.issues.push('有音乐导入缓存需要核对，原件和原请求已保留。'); }
      }
      return result;
    });
  }
  diagnostics(): string[] { return this.issues.slice(); }
  importPrepared(value: PreparedMusicFile, serialized: string, send: (wire: string, fd: number) => Promise<string>): Promise<string> {
    return this.queued(async (): Promise<string> => {
      const entry = this.checked(value); importLiteral(serialized, value);
      if (entry.releaseStarted) { throw new Error('原音乐缓存已开始清理，只能继续原清理，不能重新导入。'); }
      if (MusicFiles.pendingIo.has(entry.directory)) { throw new Error('音乐文件句柄尚未确认关闭，已保留原请求。'); }
      if (value.requestJson && value.requestJson !== serialized) { throw new Error('音乐原导入未确认，不能更换请求。'); }
      if (!value.requestJson) {
        const previous = await this.present(entry.directory + '/request.json');
        if (previous) {
          if (!previous.isFile() || previous.isSymbolicLink() || previous.size < 1 || previous.size > REQUEST_LIMIT) { throw new Error('音乐原请求已变化，已保留。'); }
          const original = await fs.readText(entry.directory + '/request.json', { length: REQUEST_LIMIT });
          importLiteral(original, value);
          if (original !== serialized) { throw new Error('音乐原导入未确认，不能更换请求。'); }
        }
        await this.writeSmall(entry.directory + '/request.json', serialized, REQUEST_LIMIT);
        value.requestJson = serialized; entry.signature = JSON.stringify(value);
      }
      const file = await fs.open(entry.directory + '/data', fs.OpenMode.READ_ONLY | fs.OpenMode.NOFOLLOW);
      try { return await send(value.requestJson, file.fd); } finally { await this.closeOwned(entry.directory, file); }
    });
  }
  // The library coordinator must first qualify Ready for this exact request;
  // no timeout, read failure or Unknown automatically reaches this operation.
  release(value: PreparedMusicFile): Promise<void> {
    return this.queued(async (): Promise<void> => {
      const entry = this.checked(value);
      await this.removeOwn(entry.directory, true, (): void => { entry.releaseStarted = true; });
      this.entries.delete(value.spoolId);
    });
  }
  retryPendingIo(): Promise<void> {
    return this.queued(async (): Promise<void> => {
      for (const directory of Array.from(MusicFiles.pendingIo.keys())) {
        const pending = MusicFiles.pendingIo.get(directory)!;
        while (pending.length) { await fs.close(pending[0]); pending.shift(); }
        MusicFiles.pendingIo.delete(directory);
      }
    });
  }
  private async previewUsage(): Promise<number> {
    let used: number = 0;
    for (const name of await fs.listFile(this.context.cacheDir)) {
      if (!/^morrow-music-[A-Za-z0-9]{6}$/.test(name)) { continue; }
      const directory = this.context.cacheDir + '/' + name, folder = await fs.lstat(directory);
      if (!folder.isDirectory() || folder.isSymbolicLink()) { throw new Error('旧音乐播放缓存需要核对。'); }
      for (const child of await fs.listFile(directory)) {
        const stat = await fs.lstat(directory + '/' + child);
        if (child !== 'data' || !stat.isFile() || stat.isSymbolicLink() || !Number.isSafeInteger(stat.size) || stat.size < 0 || stat.size > MUSIC_FILE_LIMIT) {
          throw new Error('旧音乐播放缓存含未知内容，已保留。');
        }
        used += stat.size;
      }
    }
    return used;
  }
  cleanupInactivePlaybackCache(): Promise<void> {
    return this.queued(async (): Promise<void> => {
      for (const name of await fs.listFile(this.context.cacheDir)) {
        if (!/^morrow-music-[A-Za-z0-9]{6}$/.test(name)) { continue; }
        const directory = this.context.cacheDir + '/' + name;
        if (!MusicFiles.active.has(directory) && !MusicFiles.pendingIo.has(directory)) { await this.removeOwn(directory, false); }
      }
    });
  }
  acquire(identity: MusicPlaybackIdentity): Promise<MusicPlaybackSource> {
    return this.queued(async (): Promise<MusicPlaybackSource> => {
      checkedIdentity(identity); const original = JSON.stringify(identity);
      const used = await this.previewUsage();
      if (MusicFiles.active.size >= 2 || used + identity.byteLength > MUSIC_PREVIEW_BUDGET) { throw new Error('旧音乐播放资源尚未释放，请先关闭重试。'); }
      const directory = await fs.mkdtemp(this.context.cacheDir + '/morrow-music-XXXXXX');
      let file: fs.File | undefined = undefined, admitted: boolean = false, closeAttempted: boolean = false;
      try {
        file = await fs.open(directory + '/data', fs.OpenMode.CREATE | fs.OpenMode.READ_WRITE);
        const envelope: MusicExportEnvelope = { action: 'music_export', music: { action: 'export', library_revision: identity.libraryRevision,
          track_id: identity.trackId, import_operation: identity.importOperation, byte_length: identity.byteLength.toString(), sha256: identity.sha256 } };
        const wire = JSON.stringify(envelope);
        const reply = fileReply(await workbench.exportFd(wire, file.fd), MUSIC_FILE_LIMIT);
        const stat = await fs.stat(file.fd);
        if (original !== JSON.stringify(identity) || reply.byte_length !== identity.byteLength.toString() || reply.sha256 !== identity.sha256 || !stat.isFile() || stat.size !== identity.byteLength) { throw new Error('音乐完整来源已变化，请重新读取曲目。'); }
        // Close and acknowledge the export writer before admitting a source.
        // Otherwise a rejected close would lose the caller's only lease handle.
        closeAttempted = true; await this.closeOwned(directory, file); file = undefined;
        const source = new MusicPlaybackSource(); source.token = directory.slice(this.context.cacheDir.length + 1);
        source.trackId = identity.trackId; source.libraryRevision = identity.libraryRevision; source.importOperation = identity.importOperation; source.byteLength = identity.byteLength; source.sha256 = identity.sha256;
        const lease = new MusicLease(source, directory); this.leases.set(source.token, lease); MusicFiles.active.set(directory, lease); admitted = true; return source;
      } finally {
        try { if (file && !closeAttempted) { await this.closeOwned(directory, file); } } finally {
          if (!admitted) { try { await this.removeOwn(directory, false); } catch (_) { this.issues.push('音乐预览准备清理失败，已保留。'); } }
        }
      }
    });
  }
  ownsSource(source: MusicPlaybackSource): boolean {
    const lease = this.leases.get(source.token); return !!lease && lease.source === source && lease.signature === JSON.stringify(source) && MusicFiles.active.get(lease.directory) === lease;
  }
  open(source: MusicPlaybackSource): Promise<MusicPlaybackDescriptor> {
    return this.queued(async (): Promise<MusicPlaybackDescriptor> => {
      if (!this.ownsSource(source)) { throw new Error('音乐播放来源已变化。'); } const lease = this.leases.get(source.token)!;
      if (lease.file || lease.descriptor || MusicFiles.pendingIo.has(lease.directory)) { throw new Error('音乐播放文件已打开或关闭未确认。'); }
      const file = await fs.open(lease.path, fs.OpenMode.READ_ONLY | fs.OpenMode.NOFOLLOW);
      try {
        const stat = await fs.stat(file.fd); if (!stat.isFile() || stat.size !== source.byteLength || !this.ownsSource(source)) { throw new Error('音乐播放文件长度或来源已变化。'); }
        const value = new MusicPlaybackDescriptor(); value.token = source.token; value.fd = file.fd; value.byteLength = source.byteLength; value.sha256 = source.sha256;
        lease.file = file; lease.descriptor = value; return value;
      } catch (error) { await this.closeOwned(lease.directory, file); throw error as Error; }
    });
  }
  ownsDescriptor(source: MusicPlaybackSource, descriptor: MusicPlaybackDescriptor): boolean {
    const lease = this.leases.get(source.token); return this.ownsSource(source) && !!lease && lease.descriptor === descriptor && !!lease.file &&
      descriptor.token === source.token && descriptor.fd === lease.file.fd && descriptor.byteLength === source.byteLength && descriptor.sha256 === source.sha256;
  }
  close(descriptor: MusicPlaybackDescriptor): Promise<void> {
    return this.queued(async (): Promise<void> => {
      const lease = this.leases.get(descriptor.token); if (!lease || !this.ownsDescriptor(lease.source, descriptor) || !lease.file) { throw new Error('音乐播放文件句柄无法确认。'); }
      await fs.close(lease.file); lease.file = undefined; lease.descriptor = undefined;
    });
  }
  releaseSource(source: MusicPlaybackSource): Promise<void> {
    return this.queued(async (): Promise<void> => {
      if (!this.ownsSource(source)) { throw new Error('音乐播放资源身份无法确认。'); } const lease = this.leases.get(source.token)!;
      if (lease.file || lease.descriptor) { throw new Error('播放器尚未关闭音乐文件。'); }
      await this.removeOwn(lease.directory, false); this.leases.delete(source.token); MusicFiles.active.delete(lease.directory);
    });
  }
}
