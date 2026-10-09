// Durable music metadata and operation coordination. Native owns the library,
// complete blobs and import receipts; platform files/player leases stay outside.
import { MusicPlaybackIdentity } from './MusicPlayback';
export const MUSIC_TRACK_LIMIT: number = 512;
export const MUSIC_AUDIO_LIMIT: number = 150 * 1024 * 1024;
export const MUSIC_LYRICS_LIMIT: number = 49152;
const MUSIC_REPLY_LIMIT: number = 512 * 1024;
const MUSIC_PAGE_LIMIT: number = 16;
export class MusicImportRequest {
  schema_version: number = 1;
  track_id: string = '';
  operation_id: string = '';
  expected_revision: string = '0';
  name: string = '';
  byte_length: string = '0';
  sha256: string = '';
}
export interface NativeMusicTrack {
  track_id: string; library_revision: string; import_operation: string; name: string;
  byte_length: string; sha256: string; phase: string; source_uri: string; title: string; artist: string;
  duration_ms: string; lyric_source: string; lyrics_byte_length: string; bytes_retained: boolean;
  request: MusicImportRequest; request_json: string; request_sha256: string;
}
export interface NativeMusicLyricLine { time_ms: string; text: string; }
export interface NativeMusicLyrics {
  track_id: string; text: string; source: string; lines: NativeMusicLyricLine[]; active_index: number; untimed: boolean;
}
export interface NativeMusicPlayback {
  track_id: string; index: string; playing: boolean; blocked: boolean; position_ms: string;
  duration_ms: string; transport_effect: string;
}
export interface MusicPlaybackPolicyRequest {
  library_revision: string; index: string; playing: boolean; blocked: boolean; position_ms: string;
  duration_ms: string; music_action: string; value: string; flag: boolean;
}
interface MusicPlaybackPolicyWire { action: string; music: MusicPlaybackPolicyAction; }
interface MusicPlaybackPolicyAction extends MusicPlaybackPolicyRequest { action: string; }
export interface NativeMusicSummary {
  schema_version: number; kind: string; library_revision: string; order: string[];
  selected_track_id: string; show_lyrics: boolean; online_lyrics: boolean; tracks: NativeMusicTrack[];
  next_after: string; repeated: boolean; operation_id: string; operation_revision: string;
  lyrics?: NativeMusicLyrics;
  playback?: NativeMusicPlayback;
  match_index?: number;
}
interface NativeMusicReply { ok: boolean; error: string; effect: string; receipt_revision: string; music?: NativeMusicSummary; }
interface MusicReadCommand { action: string; music: MusicReadRequest; }
interface MusicReadRequest { action: string; after?: string; limit?: number; library_revision?: string; track_id?: string; position_ms?: string; }
interface MusicImportCommand { action: string; music: MusicImportAction; }
interface MusicImportAction { action: string; request: MusicImportRequest; }
interface MusicMutationCommand { action: string; music: MusicMutationRequest; }
interface MusicMutationRequest {
  action: string; library_revision: string; operation_id: string; track_id?: string;
  order?: string[]; flag?: boolean; lyrics?: string; lyric_source?: string;
}
export class MusicLibraryTrackIdentity extends MusicPlaybackIdentity {
  trackId: string = '';
  libraryRevision: string = '0';
  importOperation: string = '';
  byteLength: number = 0;
  sha256: string = '';
}
export class MusicImportPlan {
  request: MusicImportRequest = new MusicImportRequest();
  original_begin: string = '';
  original_import: string = '';
  original_inspect: string = '';
  original_reconcile: string = '';
  phase: string = 'unissued';
  unknown: boolean = false;
  qualified_ready: boolean = false;
  record: NativeMusicTrack | undefined = undefined;
}
export class MusicLibraryView {
  library_revision: string = '0';
  order: string[] = [];
  tracks: NativeMusicTrack[] = [];
  records: NativeMusicTrack[] = [];
  selected_track_id: string = '';
  show_lyrics: boolean = false;
  loaded: boolean = false;
  busy: boolean = false;
  unknown: boolean = false;
  error: string = '';
  original_read: string = '';
  original_mutation: string = '';
  import_plan: MusicImportPlan | undefined = undefined;
  lyrics: NativeMusicLyrics | undefined = undefined;
}
export interface MusicLibraryHooks {
  send: (serialized: string) => Promise<string>;
  hash: (serialized: string) => Promise<string>;
  importFile: (originalNativeLiteral: string, request: MusicImportRequest) => Promise<string>;
  changed: () => void;
  owned: () => boolean;
  owner?: () => string;
}
class MusicPageRead {
  original: string = '';
  header: NativeMusicSummary | undefined = undefined;
  tracks: NativeMusicTrack[] = [];
  after: string = '';
}
class MusicDetailRead {
  original: string = '';
  kind: string = '';
  track_id: string = '';
  revision: string = '';
  position_ms: string = '0';
}
class MusicPolicyRead {
  original: string = '';
  request: MusicPlaybackPolicyRequest;
  result: NativeMusicPlayback | undefined = undefined;
  constructor(request: MusicPlaybackPolicyRequest, original: string) { this.request = request; this.original = original; }
}
class MusicMutation {
  original: string = '';
  request: MusicMutationRequest;
  effect: string = 'unknown';
  constructor(request: MusicMutationRequest, original: string) { this.request = request; this.original = original; }
}
function musicBytes(value: string): number {
  if (typeof value !== 'string') { throw new Error('音乐文字字段无法确认。'); }
  let length: number = 0;
  for (let i: number = 0; i < value.length; i++) {
    const code: number = value.charCodeAt(i);
    if (code >= 0xD800 && code <= 0xDBFF) {
      const low: number = value.charCodeAt(++i);
      if (!(low >= 0xDC00 && low <= 0xDFFF)) { throw new Error('音乐文字含不完整字符。'); }
      length += 4;
    } else if (code >= 0xDC00 && code <= 0xDFFF) { throw new Error('音乐文字含不完整字符。'); }
    else { length += code < 0x80 ? 1 : code < 0x800 ? 2 : 3; }
  }
  return length;
}
function musicText(value: string, limit: number): boolean { return typeof value === 'string' && musicBytes(value) <= limit; }
function musicId(value: string, limit: number = 64): boolean {
  return typeof value === 'string' && value.length > 0 && value.length <= limit && /^[A-Za-z0-9_.-]+$/.test(value) && !value.startsWith('morrow-host-');
}
function musicDecimal(value: string): boolean {
  return typeof value === 'string' && /^(0|[1-9][0-9]{0,19})$/.test(value) &&
    (value.length < 20 || value <= '18446744073709551615');
}
function musicBound(value: string, limit: number): boolean { return musicDecimal(value) && Number(value) <= limit; }
function musicKeys(value: Object, required: string[], optional: string[] = []): boolean {
  if (!value || typeof value !== 'object' || Array.isArray(value)) { return false; }
  const keys: string[] = Object.keys(value);
  return required.every((key: string): boolean => keys.includes(key)) && keys.every((key: string): boolean => required.includes(key) || optional.includes(key));
}
function musicCopyRequest(value: MusicImportRequest): MusicImportRequest {
  const result = new MusicImportRequest(); result.track_id = value.track_id; result.operation_id = value.operation_id;
  result.expected_revision = value.expected_revision; result.name = value.name; result.byte_length = value.byte_length; result.sha256 = value.sha256; return result;
}
export function validMusicImportRequest(value: MusicImportRequest): boolean {
  return musicKeys(value, ['schema_version', 'track_id', 'operation_id', 'expected_revision', 'name', 'byte_length', 'sha256']) &&
    value.schema_version === 1 && musicId(value.track_id) && musicId(value.operation_id, 128) && musicDecimal(value.expected_revision) &&
    musicText(value.name, 512) && !!value.name && !/[\u0000-\u001f\u007f/\\]/.test(value.name) &&
    /\.(mp3|wav|flac|m4a|aac|ogg|opus|wma|ape|aif|aiff|alac|wv|dsf|dff|mp2|ac3|dts|au)$/i.test(value.name) &&
    musicBound(value.byte_length, MUSIC_AUDIO_LIMIT) && value.byte_length !== '0' && /^[0-9a-f]{64}$/.test(value.sha256);
}
function sameMusicRequest(a: MusicImportRequest, b: MusicImportRequest): boolean {
  return a.schema_version === b.schema_version && a.track_id === b.track_id && a.operation_id === b.operation_id &&
    a.expected_revision === b.expected_revision && a.name === b.name && a.byte_length === b.byte_length && a.sha256 === b.sha256;
}
function musicImportLiteral(value: string): MusicImportRequest {
  if (!musicText(value, 8192)) { throw new Error('音乐原导入请求超出预算，未替换。'); }
  const command: MusicImportCommand = JSON.parse(value) as MusicImportCommand;
  if (!musicKeys(command, ['action', 'music']) || command.action !== 'music_import' ||
    !musicKeys(command.music, ['action', 'request']) || command.music.action !== 'import_file' || !validMusicImportRequest(command.music.request)) {
    throw new Error('音乐原导入请求身份无法确认。');
  }
  return musicCopyRequest(command.music.request);
}
function validMusicTrack(value: NativeMusicTrack, revision: string): boolean {
  return musicKeys(value, ['track_id', 'library_revision', 'import_operation', 'name', 'byte_length', 'sha256', 'phase', 'source_uri',
    'title', 'artist', 'duration_ms', 'lyric_source', 'lyrics_byte_length', 'bytes_retained', 'request', 'request_json', 'request_sha256']) &&
    musicId(value.track_id) && value.library_revision === revision && musicId(value.import_operation, 128) &&
    validMusicImportRequest(value.request) && value.track_id === value.request.track_id && value.import_operation === value.request.operation_id &&
    value.name === value.request.name && value.byte_length === value.request.byte_length && value.sha256 === value.request.sha256 &&
    ['pending', 'ready', 'retired'].includes(value.phase) && typeof value.bytes_retained === 'boolean' && (value.phase === 'pending' || value.bytes_retained) &&
    value.source_uri === 'file:///morrow-music/' + value.track_id && musicText(value.title, 16384) && musicText(value.artist, 16384) &&
    musicBound(value.duration_ms, Number.MAX_SAFE_INTEGER) && musicText(value.lyric_source, 4096) && musicBound(value.lyrics_byte_length, MUSIC_LYRICS_LIMIT) &&
    /^[0-9a-f]{64}$/.test(value.request_sha256) && sameMusicRequest(musicImportLiteral(value.request_json), value.request);
}
function copyMusicTrack(value: NativeMusicTrack): NativeMusicTrack { return JSON.parse(JSON.stringify(value)) as NativeMusicTrack; }
function sameMusicTrack(a: NativeMusicTrack, b: NativeMusicTrack): boolean {
  return a.track_id === b.track_id && a.library_revision === b.library_revision && a.import_operation === b.import_operation &&
    a.name === b.name && a.byte_length === b.byte_length && a.sha256 === b.sha256 && a.phase === b.phase && a.source_uri === b.source_uri &&
    a.title === b.title && a.artist === b.artist && a.duration_ms === b.duration_ms && a.lyric_source === b.lyric_source &&
    a.lyrics_byte_length === b.lyrics_byte_length && a.bytes_retained === b.bytes_retained && sameMusicRequest(a.request, b.request) &&
    a.request_json === b.request_json && a.request_sha256 === b.request_sha256;
}
function musicNextRevision(value: string): string {
  if (!musicDecimal(value) || value === '18446744073709551615') { return ''; }
  let result: string = '', carry: number = 1;
  for (let i: number = value.length - 1; i >= 0; i--) {
    const digit: number = Number(value[i]) + carry; result = String(digit % 10) + result; carry = digit >= 10 ? 1 : 0;
  }
  return carry ? '1' + result : result;
}
function copyMusicLyrics(value: NativeMusicLyrics): NativeMusicLyrics { return JSON.parse(JSON.stringify(value)) as NativeMusicLyrics; }
function validMusicLyrics(value: NativeMusicLyrics): boolean {
  if (!musicKeys(value, ['track_id', 'text', 'source', 'lines', 'active_index', 'untimed']) || !musicId(value.track_id) ||
    !musicText(value.text, MUSIC_LYRICS_LIMIT) || !musicText(value.source, 4096) || !Array.isArray(value.lines) || value.lines.length > 1024 ||
    !Number.isInteger(value.active_index) || value.active_index < -1 || value.active_index >= value.lines.length || typeof value.untimed !== 'boolean' ||
    value.untimed !== (value.lines.length === 0)) { return false; }
  let previous: number = -1;
  for (const line of value.lines) {
    if (!musicKeys(line, ['time_ms', 'text']) || !musicBound(line.time_ms, 1099511627776) || !musicText(line.text, MUSIC_LYRICS_LIMIT) || Number(line.time_ms) < previous) { return false; }
    previous = Number(line.time_ms);
  }
  return true;
}
export function parseNativeMusicReply(serialized: string): NativeMusicSummary {
  if (!musicText(serialized, MUSIC_REPLY_LIMIT)) { throw new Error('音乐完整回执超出预算，原状态仍保留。'); }
  const reply: NativeMusicReply = JSON.parse(serialized) as NativeMusicReply;
  if (!reply || typeof reply.ok !== 'boolean' || typeof reply.error !== 'string' || !['committed', 'not_committed', 'unknown'].includes(reply.effect) ||
    typeof reply.receipt_revision !== 'string' || reply.receipt_revision !== '' && !musicDecimal(reply.receipt_revision)) { throw new Error('音乐事务回执无法确认。'); }
  if (!reply.ok || reply.error || reply.effect === 'unknown') { throw new Error('音乐操作未确认：' + (reply.error || reply.effect)); }
  const value = reply.music;
  if (!value || !musicKeys(value, ['schema_version', 'kind', 'library_revision', 'order', 'selected_track_id', 'show_lyrics', 'online_lyrics',
    'tracks', 'next_after', 'repeated', 'operation_id', 'operation_revision'], ['lyrics', 'playback', 'match_index']) || value.schema_version !== 1 ||
    !['library', 'track', 'lyrics', 'playback', 'match', 'import_policy'].includes(value.kind) || !musicDecimal(value.library_revision) || !Array.isArray(value.order) || value.order.length > MUSIC_TRACK_LIMIT ||
    !value.order.every((id: string): boolean => musicId(id)) || new Set(value.order).size !== value.order.length ||
    typeof value.selected_track_id !== 'string' || (value.order.length === 0 ? value.selected_track_id !== '' : !value.order.includes(value.selected_track_id)) ||
    typeof value.show_lyrics !== 'boolean' || value.online_lyrics !== false || !Array.isArray(value.tracks) || value.tracks.length > MUSIC_PAGE_LIMIT ||
    !value.tracks.every((track: NativeMusicTrack): boolean => validMusicTrack(track, value.library_revision)) ||
    new Set(value.tracks.map((track: NativeMusicTrack): string => track.track_id)).size !== value.tracks.length ||
    typeof value.next_after !== 'string' || value.next_after !== '' && !musicId(value.next_after) || typeof value.repeated !== 'boolean' ||
    typeof value.operation_id !== 'string' || value.operation_id !== '' && (!/^[A-Za-z0-9_.-]{1,128}$/.test(value.operation_id)) ||
    typeof value.operation_revision !== 'string' || value.operation_revision !== '' && !musicDecimal(value.operation_revision) ||
    (value.operation_id === '') !== (value.operation_revision === '') ||
    (value.kind === 'lyrics' ? !value.lyrics || !validMusicLyrics(value.lyrics) : value.lyrics !== undefined) ||
    (value.kind === 'playback' ? !value.playback || !validMusicPlayback(value.playback) : value.playback !== undefined) ||
    (value.kind === 'match' ? !Number.isInteger(value.match_index) || value.match_index! < -1 || value.match_index! >= 30 : value.match_index !== undefined)) {
    throw new Error('音乐完整列表或曲目身份无法确认。');
  }
  if (reply.effect === 'committed' && (!value.operation_id || value.operation_revision === '' || value.operation_revision === '0' || reply.receipt_revision !== value.operation_revision) ||
    value.operation_id && (value.operation_revision === '0' || value.operation_revision.length > value.library_revision.length ||
      value.operation_revision.length === value.library_revision.length && value.operation_revision > value.library_revision) ||
    value.library_revision === '0' && (value.order.length || value.tracks.length || value.next_after || value.operation_id || value.show_lyrics)) {
    throw new Error('音乐真实事务版本不匹配。');
  }
  return JSON.parse(JSON.stringify(value)) as NativeMusicSummary;
}
function validMusicPlayback(value: NativeMusicPlayback): boolean {
  return musicKeys(value, ['track_id', 'index', 'playing', 'blocked', 'position_ms', 'duration_ms', 'transport_effect']) &&
    (value.track_id === '' || musicId(value.track_id)) && musicBound(value.index, 511) && typeof value.playing === 'boolean' && typeof value.blocked === 'boolean' &&
    musicBound(value.position_ms, 1099511627776) && musicBound(value.duration_ms, 1099511627776) &&
    ['none', 'open', 'play', 'pause', 'seek', 'stop'].includes(value.transport_effect) && !(value.blocked && value.playing);
}
function sameMusicHeader(a: NativeMusicSummary, b: NativeMusicSummary): boolean {
  return a.library_revision === b.library_revision && JSON.stringify(a.order) === JSON.stringify(b.order) &&
    a.selected_track_id === b.selected_track_id && a.show_lyrics === b.show_lyrics && a.online_lyrics === b.online_lyrics;
}
function copyMusicPlan(plan: MusicImportPlan): MusicImportPlan {
  const result = new MusicImportPlan(); result.request = musicCopyRequest(plan.request); result.original_begin = plan.original_begin;
  result.original_import = plan.original_import; result.original_inspect = plan.original_inspect; result.original_reconcile = plan.original_reconcile;
  result.phase = plan.phase; result.unknown = plan.unknown; result.qualified_ready = plan.qualified_ready;
  result.record = plan.record ? copyMusicTrack(plan.record) : undefined; return result;
}
export class MusicLibrary {
  private hooks: MusicLibraryHooks;
  private snapshot: NativeMusicSummary | undefined = undefined;
  private tracks: NativeMusicTrack[] = [];
  private page: MusicPageRead | undefined = undefined;
  private detail: MusicDetailRead | undefined = undefined;
  private policy: MusicPolicyRead | undefined = undefined;
  private mutation: MusicMutation | undefined = undefined;
  private plan: MusicImportPlan | undefined = undefined;
  private lyrics: NativeMusicLyrics | undefined = undefined;
  private working: boolean = false;
  private disposed: boolean = false;
  private version: number = 0;
  private error: string = '';
  private unknown: boolean = false;
  private operationOwner: string = '';
  constructor(hooks: MusicLibraryHooks) { this.hooks = hooks; }
  view(): MusicLibraryView {
    const result = new MusicLibraryView();
    if (this.snapshot) {
      result.library_revision = this.snapshot.library_revision; result.order = this.snapshot.order.slice();
      result.selected_track_id = this.snapshot.selected_track_id; result.show_lyrics = this.snapshot.show_lyrics;
      result.records = this.tracks.map((track: NativeMusicTrack): NativeMusicTrack => copyMusicTrack(track));
      result.tracks = this.snapshot.order.map((id: string): NativeMusicTrack => copyMusicTrack(this.tracks.find((track: NativeMusicTrack): boolean => track.track_id === id)!)); result.loaded = true;
    }
    result.busy = this.working; result.error = this.error; result.unknown = this.unknown;
    result.original_read = this.policy?.original || this.detail?.original || this.page?.original || '';
    result.original_mutation = this.mutation?.original || '';
    result.import_plan = this.plan ? copyMusicPlan(this.plan) : undefined; result.lyrics = this.lyrics ? copyMusicLyrics(this.lyrics) : undefined; return result;
  }
  private owned(): boolean { return !this.disposed && this.hooks.owned(); }
  private notify(): void { this.hooks.changed(); }
  private claim(): number {
    if (!this.owned() || this.working) { throw new Error('音乐页面或操作身份已变化。'); }
    this.operationOwner = this.hooks.owner ? this.hooks.owner() : '';
    this.working = true; this.error = ''; this.notify(); return this.version;
  }
  private guard(version: number): boolean {
    return version === this.version && this.owned() && (!this.hooks.owner || this.operationOwner === this.hooks.owner());
  }
  private fail(error: Error | string): void { this.error = String(error); this.unknown = true; this.notify(); }
  private finish(): void { this.working = false; this.notify(); }
  private async checked(serialized: string, readonly: boolean = false): Promise<NativeMusicSummary> {
    const result = parseNativeMusicReply(serialized);
    const reply: NativeMusicReply = JSON.parse(serialized) as NativeMusicReply;
    if (readonly && (reply.effect !== 'not_committed' || reply.receipt_revision !== '')) { throw new Error('音乐只读结果不得冒称持久写入。'); }
    for (const track of result.tracks) {
      const hash: string = await this.hooks.hash(track.request_json);
      if (hash !== track.request_sha256) { throw new Error('音乐原导入字节身份无法确认。'); }
    }
    return result;
  }
  private readWire(after: string): string {
    const command: MusicReadCommand = { action: 'music', music: { action: 'read', after: after, limit: MUSIC_PAGE_LIMIT } }; return JSON.stringify(command);
  }
  async load(): Promise<void> {
    if (!this.owned() || this.working || this.page || this.detail || this.policy || this.mutation || this.plan?.unknown) { throw new Error('音乐原操作尚待核对，不能用新列表替换。'); }
    this.page = new MusicPageRead(); this.page.original = this.readWire(''); this.detail = undefined;
    await this.readPages();
  }
  async retryRead(): Promise<void> {
    if (this.policy) { await this.retryPlaybackPolicy(); return; }
    if (this.detail) { await this.readDetail(); return; }
    if (this.page) { await this.readPages(); }
  }
  async restartRead(): Promise<void> {
    if (!this.owned() || this.working || this.mutation || this.plan?.unknown) { throw new Error('音乐原写入尚未核对，不能刷新替换。'); }
    this.version++; this.page = undefined; this.detail = undefined; this.policy = undefined;
    await this.load();
  }
  private async readPages(): Promise<void> {
    if (!this.page) { return; }
    const page: MusicPageRead = this.page, version: number = this.claim();
    try {
      while (true) {
        const value: NativeMusicSummary = await this.checked(await this.hooks.send(page.original), true);
        if (!this.guard(version) || this.page !== page) { throw new Error('音乐列表晚到，原页面状态未替换。'); }
        if (value.kind !== 'library' || value.operation_id || value.operation_revision !== '' || value.repeated || value.lyrics) { throw new Error('音乐分页回执不是只读列表。'); }
        if (page.header && !sameMusicHeader(page.header, value)) { throw new Error('音乐分页版本已变化，请重新读取完整列表。'); }
        const header: NativeMusicSummary = page.header || value;
        let previous: string = page.after;
        for (const track of value.tracks) {
          if (track.track_id <= previous) { throw new Error('音乐记录分页重复或顺序无法确认。'); }
          previous = track.track_id;
        }
        const next: string = value.next_after;
        if (page.tracks.length + value.tracks.length > MUSIC_TRACK_LIMIT || next && (value.tracks.length !== MUSIC_PAGE_LIMIT || next !== previous) ||
          page.after && value.tracks.length === 0) { throw new Error('音乐分页游标或记录预算无法确认。'); }
        const candidate: NativeMusicTrack[] = page.tracks.concat(value.tracks);
        if (!next) {
          const ready: string[] = candidate.filter((track: NativeMusicTrack): boolean => track.phase === 'ready').map((track: NativeMusicTrack): string => track.track_id);
          if (ready.length !== header.order.length || ready.some((id: string): boolean => !header.order.includes(id))) { throw new Error('音乐完整分页缺少播放列表的实际就绪记录。'); }
          this.snapshot = header; this.tracks = candidate.map((track: NativeMusicTrack): NativeMusicTrack => copyMusicTrack(track));
          this.page = undefined; this.lyrics = undefined; this.unknown = false; this.error = ''; this.notify(); return;
        }
        page.header = header; page.tracks = candidate;
        page.after = next; page.original = this.readWire(next);
      }
    } catch (error) { this.fail(String(error)); }
    finally { this.finish(); }
  }
  identity(trackId: string): MusicLibraryTrackIdentity | undefined {
    if (!this.snapshot || this.page || this.policy || this.mutation || this.plan?.unknown || !this.owned() || !this.snapshot.order.includes(trackId)) { return undefined; }
    const track = this.tracks.find((item: NativeMusicTrack): boolean => item.track_id === trackId);
    if (!track || track.phase !== 'ready' || !track.bytes_retained) { return undefined; }
    const identity = new MusicLibraryTrackIdentity(); identity.trackId = track.track_id; identity.libraryRevision = this.snapshot.library_revision;
    identity.importOperation = track.import_operation; identity.byteLength = Number(track.byte_length); identity.sha256 = track.sha256; return identity;
  }
  adjacent(current: MusicLibraryTrackIdentity, direction: number): MusicLibraryTrackIdentity | undefined {
    if (direction !== -1 && direction !== 1 || !this.snapshot || this.working || this.detail || this.policy || this.mutation || this.plan?.unknown) { return undefined; }
    const actual = this.identity(current.trackId);
    if (!actual || actual.importOperation !== current.importOperation || actual.byteLength !== current.byteLength || actual.sha256 !== current.sha256) { return undefined; }
    const order: string[] = this.snapshot.order, index: number = order.indexOf(current.trackId);
    return this.identity(order[(index + direction + order.length) % order.length]);
  }
  async readTrack(trackId: string): Promise<void> { await this.startDetail('track_read', trackId, '0'); }
  async readLyrics(trackId: string, positionMs: number = 0): Promise<void> {
    if (!Number.isSafeInteger(positionMs) || positionMs < 0) { throw new Error('歌词位置无法确认。'); }
    await this.startDetail('lyrics_read', trackId, String(positionMs));
  }
  private async startDetail(kind: string, trackId: string, position: string): Promise<void> {
    const record = this.tracks.find((track: NativeMusicTrack): boolean => track.track_id === trackId);
    if (!this.snapshot || !record || !this.owned() || this.working || this.detail || this.page || this.policy || this.mutation || this.plan?.unknown ||
      kind === 'lyrics_read' && !this.identity(trackId)) { throw new Error('音乐曲目读取身份未确认。'); }
    const detail = new MusicDetailRead(); detail.kind = kind; detail.track_id = trackId; detail.revision = this.snapshot.library_revision;
    detail.position_ms = position;
    const command: MusicReadCommand = { action: 'music', music: { action: kind, library_revision: detail.revision, track_id: trackId } };
    if (kind === 'lyrics_read') { command.music.position_ms = position; }
    detail.original = JSON.stringify(command); this.detail = detail; await this.readDetail();
  }
  private async readDetail(): Promise<void> {
    if (!this.detail) { return; }
    const detail: MusicDetailRead = this.detail, version: number = this.claim();
    try {
      const value = await this.checked(await this.hooks.send(detail.original), true);
      if (!this.guard(version) || this.detail !== detail || this.snapshot?.library_revision !== detail.revision || value.library_revision !== detail.revision ||
        !this.snapshot || !sameMusicHeader(this.snapshot, value) || value.next_after || value.operation_id || value.operation_revision !== '' || value.repeated) { throw new Error('音乐曲目只读回执身份已变化。'); }
      if (detail.kind === 'track_read') {
        const old = this.tracks.find((track: NativeMusicTrack): boolean => track.track_id === detail.track_id);
        if (value.kind !== 'track' || value.tracks.length !== 1 || value.tracks[0].track_id !== detail.track_id || !old ||
          !sameMusicTrack(value.tracks[0], old)) { throw new Error('音乐完整曲目读取不匹配。'); }
      } else {
        const track = this.tracks.find((item: NativeMusicTrack): boolean => item.track_id === detail.track_id);
        if (value.kind !== 'lyrics' || !value.lyrics || value.lyrics.track_id !== detail.track_id || !track || value.tracks.length !== 1 || !sameMusicTrack(value.tracks[0], track) ||
          musicBytes(value.lyrics.text) !== Number(track.lyrics_byte_length) || value.lyrics.source !== track.lyric_source) { throw new Error('音乐歌词完整原文不匹配。'); }
        let active: number = -1;
        value.lyrics.lines.forEach((line: NativeMusicLyricLine, index: number): void => { if (Number(line.time_ms) <= Number(detail.position_ms)) { active = index; } });
        if (value.lyrics.active_index !== active) { throw new Error('音乐歌词位置回执不匹配。'); }
        this.lyrics = copyMusicLyrics(value.lyrics);
      }
      this.detail = undefined; this.unknown = false; this.error = ''; this.notify();
    } catch (error) { this.fail(String(error)); }
    finally { this.finish(); }
  }
  async playbackPolicy(request: MusicPlaybackPolicyRequest): Promise<NativeMusicPlayback | undefined> {
    if (!this.snapshot || !this.owned() || this.working || this.page || this.detail || this.policy || this.mutation || this.plan?.unknown ||
      !musicKeys(request, ['library_revision', 'index', 'playing', 'blocked', 'position_ms', 'duration_ms', 'music_action', 'value', 'flag']) ||
      request.library_revision !== this.snapshot.library_revision || !musicBound(request.index, Math.max(0, this.snapshot.order.length - 1)) ||
      typeof request.playing !== 'boolean' || typeof request.blocked !== 'boolean' || typeof request.flag !== 'boolean' ||
      !musicBound(request.position_ms, 1099511627776) || !musicBound(request.duration_ms, 1099511627776) ||
      !['restore', 'block', 'toggle', 'seek', 'select', 'next', 'previous', 'remove'].includes(request.music_action) ||
      typeof request.value !== 'string' || !/^(0|-?[1-9][0-9]{0,18})$/.test(request.value) || !Number.isSafeInteger(Number(request.value))) {
      throw new Error('音乐纯播放请求或实际列表身份未确认。');
    }
    const music: MusicPlaybackPolicyAction = { action: 'policy', library_revision: request.library_revision, index: request.index,
      playing: request.playing, blocked: request.blocked, position_ms: request.position_ms, duration_ms: request.duration_ms,
      music_action: request.music_action, value: request.value, flag: request.flag };
    const command: MusicPlaybackPolicyWire = { action: 'music', music: music };
    const captured: MusicPlaybackPolicyRequest = JSON.parse(JSON.stringify(request)) as MusicPlaybackPolicyRequest;
    this.policy = new MusicPolicyRead(captured, JSON.stringify(command)); return this.retryPlaybackPolicy();
  }
  async retryPlaybackPolicy(): Promise<NativeMusicPlayback | undefined> {
    if (!this.policy) { return undefined; }
    const policy: MusicPolicyRead = this.policy, version: number = this.claim();
    try {
      const value: NativeMusicSummary = await this.checked(await this.hooks.send(policy.original), true);
      if (!this.guard(version) || this.policy !== policy || !this.snapshot || !sameMusicHeader(this.snapshot, value) ||
        value.kind !== 'playback' || !value.playback || value.tracks.length || value.next_after || value.operation_id || value.operation_revision || value.repeated ||
        value.playback.track_id !== '' && !this.snapshot.order.includes(value.playback.track_id) ||
        !this.playbackProposalMatches(policy.request, value.playback)) { throw new Error('音乐纯播放策略回执身份已变化。'); }
      policy.result = JSON.parse(JSON.stringify(value.playback)) as NativeMusicPlayback;
      this.policy = undefined; this.unknown = false; this.error = ''; this.notify(); return policy.result;
    } catch (error) { this.fail(String(error)); return undefined; }
    finally { this.finish(); }
  }
  private playbackProposalMatches(request: MusicPlaybackPolicyRequest, result: NativeMusicPlayback): boolean {
    // This checks the actual services::Playback response contract; it never
    // manufactures a decision or turns a transport proposal into player success.
    if (!this.snapshot) { return false; }
    const order: string[] = this.snapshot.order.slice();
    let index: number = Number(request.index), playing: boolean = request.playing, blocked: boolean = request.blocked;
    let position: string = request.position_ms, duration: string = request.duration_ms, effect: string = 'none';
    const value: number = Number(request.value), action: string = request.music_action;
    if (action === 'restore') { playing = false; position = '0'; duration = '0'; effect = 'stop'; }
    else if (action === 'block') { blocked = request.flag; if (blocked) { playing = false; effect = 'pause'; } }
    else if (action === 'toggle' && !blocked && order.length) { playing = !playing; effect = playing ? 'play' : 'pause'; }
    else if (action === 'seek') { if (value < 0) { return false; } position = String(Math.min(value, Number(duration))); effect = 'seek'; }
    else if (['select', 'next', 'previous'].includes(action) && order.length) {
      const target: number = action === 'next' ? index + 1 : action === 'previous' ? index - 1 : value;
      index = (target % order.length + order.length) % order.length; playing = request.flag && !blocked; position = '0'; duration = '0'; effect = 'open';
    } else if (action === 'remove') {
      if (value < 0 || value >= order.length) { return false; }
      const wasCurrent: boolean = value === index; order.splice(value, 1);
      if (value < index || index >= order.length) { index = Math.max(0, Math.min(index - 1, order.length - 1)); }
      if (wasCurrent) { position = '0'; duration = '0'; effect = order.length ? 'open' : 'stop'; }
    }
    if (blocked || !order.length) { playing = false; }
    return result.track_id === (order[index] || '') && result.index === String(index) && result.playing === playing && result.blocked === blocked &&
      result.position_ms === position && result.duration_ms === duration && result.transport_effect === effect;
  }
  async beginImport(request: MusicImportRequest): Promise<void> {
    if (!this.owned() || !validMusicImportRequest(request) || this.plan || this.mutation || this.working || this.page || this.detail || this.policy ||
      !this.snapshot || request.expected_revision !== this.snapshot.library_revision || this.tracks.some((track: NativeMusicTrack): boolean => track.track_id === request.track_id)) {
      throw new Error('音乐导入计划或曲库版本无法确认。');
    }
    const plan = new MusicImportPlan(); plan.request = musicCopyRequest(request);
    const command: MusicImportCommand = { action: 'music', music: { action: 'import_begin', request: musicCopyRequest(request) } };
    plan.original_begin = JSON.stringify(command); this.importChecks(plan); this.plan = plan;
    await this.importAction('begin');
  }
  restoreImportLiteral(serialized: string): void {
    if (!this.owned() || this.working || this.page || this.detail || this.policy || this.plan || this.mutation) { throw new Error('音乐原导入上下文不可替换。'); }
    const plan = new MusicImportPlan(); plan.request = musicImportLiteral(serialized); plan.original_import = serialized;
    plan.phase = 'unknown'; plan.unknown = true; this.importChecks(plan); this.plan = plan; this.unknown = true; this.notify();
  }
  private importChecks(plan: MusicImportPlan): void {
    const inspect: MusicImportCommand = { action: 'music', music: { action: 'import_inspect', request: musicCopyRequest(plan.request) } };
    const reconcile: MusicImportCommand = { action: 'music', music: { action: 'reconcile', request: musicCopyRequest(plan.request) } };
    plan.original_inspect = JSON.stringify(inspect); plan.original_reconcile = JSON.stringify(reconcile);
  }
  async retryImportBegin(): Promise<void> { if (this.plan?.original_begin && this.plan.phase === 'unissued') { await this.importAction('begin'); } }
  async commitImport(): Promise<void> {
    if (!this.plan || this.plan.unknown || this.plan.phase !== 'pending' || !this.plan.original_import) { throw new Error('音乐原导入尚未核对为可继续的待导入计划。'); }
    await this.importAction('file');
  }
  async inspectImport(): Promise<void> { await this.importAction('import_inspect'); }
  async reconcileImport(): Promise<void> { await this.importAction('reconcile'); }
  private async importAction(action: string): Promise<void> {
    if (!this.plan) { return; }
    const plan: MusicImportPlan = this.plan, version: number = this.claim();
    let serialized: string = plan.original_begin;
    if (action === 'file') { serialized = plan.original_import; }
    else if (action !== 'begin') { serialized = action === 'import_inspect' ? plan.original_inspect : plan.original_reconcile; }
    try {
      const raw: string = action === 'file' ? await this.hooks.importFile(serialized, musicCopyRequest(plan.request)) : await this.hooks.send(serialized);
      const value = await this.checked(raw);
      if (!this.guard(version) || this.plan !== plan || value.kind !== 'track' || value.tracks.length !== 1 || value.next_after) { throw new Error('音乐原导入回执身份已变化。'); }
      const track: NativeMusicTrack = value.tracks[0];
      if (!sameMusicRequest(track.request, plan.request) || plan.original_import && track.request_json !== plan.original_import) { throw new Error('音乐原导入字节或请求不匹配。'); }
      const readyOperation: string = 'morrow-host-music-ready-' + await this.hooks.hash('morrow.hmos.music.v1\u0000ready\u0000' + plan.request.track_id + '\u0000' + plan.request.operation_id);
      const expectedOperation: string = action === 'file' || action === 'reconcile' && track.phase === 'ready' ? readyOperation : plan.request.operation_id;
      if (!this.guard(version) || value.operation_id !== expectedOperation || !value.operation_revision || value.operation_revision === '0') { throw new Error('音乐原导入阶段收据不匹配。'); }
      if (action === 'begin' && track.phase !== 'pending' && track.phase !== 'ready') { throw new Error('音乐原导入已退役，不能重新激活。'); }
      if (action === 'file' && track.phase !== 'ready') { throw new Error('音乐完整音频尚未确认就绪。'); }
      plan.original_import = track.request_json; plan.record = copyMusicTrack(track); plan.phase = track.phase;
      plan.qualified_ready = track.phase === 'ready' && track.bytes_retained; plan.unknown = false; this.unknown = false; this.error = ''; this.notify();
    } catch (error) { plan.unknown = true; this.fail(String(error)); }
    finally { this.finish(); }
  }
  finishImport(): void {
    if (!this.plan || this.working || this.plan.unknown || !this.plan.qualified_ready) { throw new Error('音乐完整导入尚未确认，原计划仍保留。'); }
    this.plan = undefined; this.notify();
  }
  private async change(request: MusicMutationRequest): Promise<void> {
    if (!this.snapshot || !this.owned() || this.working || this.page || this.detail || this.policy || this.mutation || this.plan || !musicId(request.operation_id, 128)) { throw new Error('音乐列表仍有未完成操作。'); }
    const command: MusicMutationCommand = { action: 'music', music: request };
    this.mutation = new MusicMutation(request, JSON.stringify(command)); await this.mutate();
  }
  async select(trackId: string, operationId: string): Promise<MusicLibraryTrackIdentity> {
    if (!this.identity(trackId)) { throw new Error('音乐曲目未确认可选。'); }
    await this.change({ action: 'select', library_revision: this.view().library_revision, operation_id: operationId, track_id: trackId });
    const identity = this.identity(trackId);
    if (!identity || this.view().selected_track_id !== trackId || this.mutation || this.unknown) { throw new Error('音乐实际选曲尚未确认，未授予新播放来源。'); }
    return identity;
  }
  async remove(trackId: string, operationId: string): Promise<void> {
    if (!this.identity(trackId)) { throw new Error('音乐曲目未确认可删除。'); }
    await this.change({ action: 'remove', library_revision: this.view().library_revision, operation_id: operationId, track_id: trackId });
  }
  async reorder(order: string[], operationId: string): Promise<void> {
    const current: string[] = this.view().order;
    if (order.length !== current.length || new Set(order).size !== order.length || order.some((id: string): boolean => !current.includes(id))) { throw new Error('音乐排序必须保留完整稳定曲目集合。'); }
    await this.change({ action: 'reorder', library_revision: this.view().library_revision, operation_id: operationId, order: order.slice() });
  }
  async setLyrics(trackId: string, text: string, source: string, operationId: string): Promise<void> {
    if (!this.identity(trackId) || !musicText(text, MUSIC_LYRICS_LIMIT) || !text.trim() || !musicText(source, 4096)) { throw new Error('音乐完整歌词或目标未确认。'); }
    await this.change({ action: 'set_lyrics', library_revision: this.view().library_revision, operation_id: operationId, track_id: trackId, lyrics: text, lyric_source: source });
  }
  async setShowLyrics(flag: boolean, operationId: string): Promise<void> {
    if (typeof flag !== 'boolean') { throw new Error('音乐歌词开关未确认。'); }
    await this.change({ action: 'show_lyrics', library_revision: this.view().library_revision, operation_id: operationId, flag: flag });
  }
  async retryMutation(): Promise<void> { if (this.mutation) { await this.mutate(); } }
  private async mutate(): Promise<void> {
    if (!this.mutation) { return; }
    const mutation: MusicMutation = this.mutation, version: number = this.claim();
    try {
      // A previous known no-write result cannot qualify a new attempt whose
      // reply is lost. The fixed bytes and CAS remain unchanged on retry.
      mutation.effect = 'unknown';
      const raw: string = await this.hooks.send(mutation.original), reply: NativeMusicReply = JSON.parse(raw) as NativeMusicReply;
      if (reply && typeof reply.ok === 'boolean' && typeof reply.error === 'string' && !reply.ok && reply.effect === 'not_committed' && reply.receipt_revision === '') {
        mutation.effect = 'not_committed'; throw new Error('音乐固定写入未提交：' + reply.error);
      }
      const value: NativeMusicSummary = await this.checked(raw);
      if (!this.guard(version) || this.mutation !== mutation || reply.effect !== 'committed' || value.operation_id !== mutation.request.operation_id || !value.operation_revision || value.operation_revision === '0' ||
        !['library', 'track', 'lyrics'].includes(value.kind) || value.next_after || !this.mutationMatches(mutation.request, value)) { throw new Error('音乐固定写入回执未确认。'); }
      mutation.effect = 'committed'; this.mutation = undefined; this.unknown = false; this.error = ''; this.notify();
    } catch (error) {
      this.error = String(error); this.unknown = mutation.effect !== 'not_committed'; this.notify();
    }
    finally { this.finish(); }
    if (!this.mutation && this.owned()) { await this.load(); }
  }
  private mutationMatches(request: MusicMutationRequest, value: NativeMusicSummary): boolean {
    if (!this.snapshot || value.operation_revision !== musicNextRevision(request.library_revision)) { return false; }
    if (request.track_id) {
      const old = this.tracks.find((track: NativeMusicTrack): boolean => track.track_id === request.track_id);
      const track = value.tracks[0];
      if (!old || value.kind !== 'track' || value.tracks.length !== 1 || !sameMusicRequest(old.request, track.request) ||
        old.request_json !== track.request_json || old.request_sha256 !== track.request_sha256) { return false; }
    } else if (value.kind !== 'library' || value.tracks.length) { return false; }
    if (value.repeated) { return true; }
    if (value.library_revision !== value.operation_revision) { return false; }
    const order: string[] = request.action === 'reorder' ? request.order!.slice() : this.snapshot.order.slice();
    let selected: string = this.snapshot.selected_track_id, show: boolean = this.snapshot.show_lyrics;
    if (request.action === 'select') { selected = request.track_id!; }
    if (request.action === 'show_lyrics') { show = request.flag!; }
    if (request.action === 'remove') {
      const removed: number = order.indexOf(request.track_id!); order.splice(removed, 1);
      if (selected === request.track_id) { selected = order[Math.min(removed, order.length - 1)] || ''; }
      if (value.tracks[0].phase !== 'retired') { return false; }
    } else if (request.track_id && value.tracks[0].phase !== 'ready') { return false; }
    if (request.action === 'set_lyrics' && (value.tracks[0].lyrics_byte_length !== String(musicBytes(request.lyrics!)) || value.tracks[0].lyric_source !== request.lyric_source)) { return false; }
    return JSON.stringify(value.order) === JSON.stringify(order) && value.selected_track_id === selected && value.show_lyrics === show;
  }
  discardKnownMutation(): void {
    if (!this.owned() || this.working || !this.mutation || this.mutation.effect !== 'not_committed') { throw new Error('音乐原写入结果未知，不能丢弃。'); }
    this.mutation = undefined; this.unknown = false; this.error = ''; this.notify();
  }
  dispose(): void { this.disposed = true; this.version++; this.notify(); }
}
