import { common } from '@kit.AbilityKit';
import { util } from '@kit.ArkTS';
import { MusicFiles, PreparedMusicFile } from './MusicFiles';
import { MusicLibrary, MusicImportRequest, MusicLibraryTrackIdentity, NativeMusicTrack,
  NativeMusicLyrics, NativeMusicPlayback, MusicPlaybackPolicyRequest } from './MusicLibrary';
import { MusicPlayback, MusicPlaybackIdentity, MusicPlaybackSource, MusicPlayer, musicPlaybackSameTrack } from './MusicPlayback';
import { MusicUiView, MusicSpoolView, MusicRecoveryChoice } from './MusicUi';
import { createMusicPlayer } from '../pages/PlatformMusicPlayer';
import { workbench } from './Workbench';
import { editorInputHash } from './EditorInputHash';

export interface MusicWorkbenchHooks {
  owned: () => boolean;
  foreground: () => boolean;
  owner: () => string;
  changed: () => void;
  beforePlay: () => Promise<void>;
}

class MusicCachedLyrics {
  identity: MusicLibraryTrackIdentity;
  lyrics: NativeMusicLyrics;
  byteLength: string;
  constructor(identity: MusicLibraryTrackIdentity, lyrics: NativeMusicLyrics, byteLength: string) {
    this.identity = identity; this.lyrics = lyrics; this.byteLength = byteLength;
  }
}

// This joins the actual durable library, selected document grants and player
// leases. A Rust transport proposal is never used as an AVPlayer acknowledgement.
export class MusicWorkbench {
  private hooks: MusicWorkbenchHooks;
  private files: MusicFiles;
  private library: MusicLibrary;
  private playback: MusicPlayback;
  private admitted: Map<string, MusicPlaybackIdentity> = new Map<string, MusicPlaybackIdentity>();
  private spools: PreparedMusicFile[] = [];
  private activeSpool: PreparedMusicFile | undefined = undefined;
  private released: Set<string> = new Set<string>();
  private working: boolean = false;
  private disposed: boolean = false;
  private error: string = '';
  private notice: string = '';
  private lyricsOpen: boolean = false;
  private lyricsTrack: string = '';
  private lyrics: Map<string, MusicCachedLyrics> = new Map<string, MusicCachedLyrics>();
  private unsavedLyrics: string = '';
  private unsavedLyricsTrack: string = '';
  private unsavedLyricsTitle: string = '';

  constructor(context: common.UIAbilityContext, hooks: MusicWorkbenchHooks) {
    this.hooks = hooks; this.files = new MusicFiles(context);
    this.library = new MusicLibrary({
      send: (wire: string): Promise<string> => workbench.sendRaw(wire),
      hash: (wire: string): Promise<string> => editorInputHash(wire),
      importFile: (wire: string, request: MusicImportRequest): Promise<string> => this.importFile(wire, request),
      owned: (): boolean => this.available(), owner: (): string => this.hooks.owner(),
      changed: (): void => this.notify()
    });
    this.playback = new MusicPlayback({
      acquire: (identity: MusicPlaybackIdentity): Promise<MusicPlaybackSource> => this.files.acquire(identity),
      ownsSource: (source: MusicPlaybackSource): boolean => this.files.ownsSource(source),
      createPlayer: (source: MusicPlaybackSource): Promise<MusicPlayer> => createMusicPlayer(source, this.files),
      releaseSource: (source: MusicPlaybackSource): Promise<void> => this.files.releaseSource(source),
      admitSelection: (identity: MusicPlaybackIdentity): Promise<MusicPlaybackIdentity> => this.admitSelection(identity),
      adjacent: (identity: MusicPlaybackIdentity, direction: number): MusicPlaybackIdentity | undefined => this.adjacent(identity, direction),
      isCurrentOwner: (): boolean => !this.disposed && this.hooks.owned(),
      changed: (): void => this.notify()
    });
  }
  private available(): boolean { return !this.disposed && this.hooks.owned() && this.hooks.foreground(); }
  private notify(): void { if (!this.disposed && this.hooks.owned()) { this.hooks.changed(); } }
  private guard(owner: string): void {
    if (!this.available() || owner !== this.hooks.owner()) { throw new Error('音乐页面已变化，原操作和缓存仍保留。'); }
  }
  private operation(): string { return 'music-' + util.generateRandomUUID(true); }
  private preparedMatches(file: PreparedMusicFile, request: MusicImportRequest): boolean {
    return file.name === request.name && file.byteLength === request.byte_length && file.sha256 === request.sha256;
  }
  private adjacent(identity: MusicPlaybackIdentity, direction: number): MusicPlaybackIdentity | undefined {
    const actual = this.library.identity(identity.trackId);
    return actual && musicPlaybackSameTrack(actual, identity) ? this.library.adjacent(actual, direction) : undefined;
  }
  private async importFile(wire: string, request: MusicImportRequest): Promise<string> {
    const file = this.activeSpool;
    if (!file || this.released.has(file.spoolId) || !this.preparedMatches(file, request)) { throw new Error('原音乐导入没有匹配的完整缓存。'); }
    return this.files.importPrepared(file, wire, (original: string, fd: number): Promise<string> => workbench.importFd(original, fd));
  }
  private async admitSelection(identity: MusicPlaybackIdentity): Promise<MusicPlaybackIdentity> {
    const owner = this.hooks.owner(); this.guard(owner);
    let actual = this.library.identity(identity.trackId);
    if (!actual || !musicPlaybackSameTrack(actual, identity)) { throw new Error('曲目来源已变化，请重新读取。'); }
    if (this.library.view().selected_track_id !== identity.trackId) {
      actual = await this.library.select(identity.trackId, this.operation()); this.guard(owner);
    }
    if (!actual || this.library.view().selected_track_id !== identity.trackId) { throw new Error('原选曲尚未确认。'); }
    await this.hooks.beforePlay(); this.guard(owner);
    await this.readLyrics(identity.trackId, owner); this.guard(owner);
    const confirmed = this.library.identity(identity.trackId);
    if (!confirmed || !musicPlaybackSameTrack(actual, confirmed)) { throw new Error('选曲后的完整来源尚未确认。'); }
    this.admitted.set(confirmed.trackId, confirmed);
    return confirmed;
  }
  private recordMatches(file: PreparedMusicFile, record: NativeMusicTrack): boolean {
    return this.preparedMatches(file, record.request) && record.name === file.name && record.byte_length === file.byteLength && record.sha256 === file.sha256;
  }
  private cachedLyrics(record: NativeMusicTrack | undefined): NativeMusicLyrics | undefined {
    if (!record) { return undefined; }
    const cached = this.lyrics.get(record.track_id);
    if (!cached || record.import_operation !== cached.identity.importOperation || record.sha256 !== cached.identity.sha256 ||
      record.byte_length !== String(cached.identity.byteLength) || record.lyric_source !== cached.lyrics.source ||
      record.lyrics_byte_length !== cached.byteLength) { return undefined; }
    return cached.lyrics;
  }
  private spoolView(file: PreparedMusicFile): MusicSpoolView {
    const result = new MusicSpoolView(); result.spoolId = file.spoolId; result.name = file.name; result.byteLength = file.byteLength;
    result.originalSaved = !!file.requestJson;
    const state = this.library.view();
    if (!file.requestJson && state.loaded && !state.unknown && !state.original_read) {
      for (const record of state.records) {
        if (record.phase === 'pending' && this.recordMatches(file, record)) {
          const choice = new MusicRecoveryChoice(); choice.trackId = record.track_id; choice.title = record.title || record.name; choice.phase = record.phase;
          result.choices.push(choice);
        }
      }
      result.canStart = result.choices.length === 0 && !state.import_plan && !state.original_mutation;
    }
    return result;
  }
  view(): MusicUiView {
    const result = new MusicUiView(); result.library = this.library.view(); result.playback = this.playback.view();
    result.spools = this.spools.filter((file: PreparedMusicFile): boolean => !this.released.has(file.spoolId)).map((file: PreparedMusicFile): MusicSpoolView => this.spoolView(file));
    const stable = result.library.loaded && !result.library.busy && !result.library.unknown &&
      !result.library.original_read && !result.library.original_mutation && !result.library.import_plan;
    result.working = this.working || result.library.busy || result.playback.busy;
    result.canWrite = this.available() && stable && !result.working;
    result.controlsEnabled = this.available() && !result.playback.background && !this.working && !result.playback.busy &&
      (result.playback.phase === 'playing' || result.canWrite && ['idle', 'prepared', 'paused', 'completed', 'failed'].includes(result.playback.phase));
    result.error = this.error || result.library.error || result.playback.error; result.notice = this.notice;
    result.canRetryRead = this.available() && !result.working && !!result.library.original_read;
    result.canRetryMutation = this.available() && !result.working && !!result.library.original_mutation;
    result.canDiscardKnownMutation = result.canRetryMutation && !result.library.unknown;
    const plan = result.library.import_plan;
    const canImport = this.available() && !result.working && !!plan;
    result.canInspectImport = canImport;
    result.canContinueImport = canImport && !plan!.unknown && plan!.phase === 'pending' && !!this.activeSpool;
    result.canReconcileImport = canImport && !plan!.qualified_ready;
    result.canFinishImport = canImport && !plan!.unknown && plan!.qualified_ready;
    result.canRetryBegin = canImport && plan!.phase === 'unissued' && !!plan!.original_begin;
    result.diagnostics = this.files.diagnostics(); result.lyricsOpen = this.lyricsOpen;
    const current = result.playback.trackId || result.library.selected_track_id;
    const record = result.library.tracks.find((track: NativeMusicTrack): boolean => track.track_id === current);
    const panelRecord = result.library.records.find((track: NativeMusicTrack): boolean => track.track_id === (this.lyricsTrack || current));
    const panel = this.cachedLyrics(panelRecord), currentLyrics = this.cachedLyrics(record);
    result.lyricsText = panel ? panel.text : ''; result.lyricsSource = panel ? panel.source : '';
    result.lyricsTitle = panelRecord ? panelRecord.title || panelRecord.name : ''; result.lyricsUntimed = panel ? panel.untimed : false;
    result.unsavedLyrics = (this.lyricsTrack || current) === this.unsavedLyricsTrack ? this.unsavedLyrics : '';
    result.unsavedLyricsTrackId = this.unsavedLyricsTrack; result.unsavedLyricsTitle = this.unsavedLyricsTitle;
    if (record && result.library.show_lyrics && result.playback.phase === 'playing' && !result.playback.background) {
      const title = record.title || record.name;
      result.footerText = '♪ ' + title;
      if (currentLyrics && currentLyrics.lines.length) {
        for (const line of currentLyrics.lines) {
          if (Number(line.time_ms) <= result.playback.positionMs) { result.footerText = line.text || '♪'; } else { break; }
        }
      } else if (currentLyrics && currentLyrics.text.trim()) {
        result.footerText = currentLyrics.text.trim().split('\n')[0] + ' · 无时间轴';
      } else { result.footerText += record.lyrics_byte_length === '0' ? ' · 暂无歌词' : ' · 歌词待读取'; }
    }
    return result;
  }
  private async run(action: (owner: string) => Promise<void>): Promise<void> {
    if (!this.available() || this.working) { return; }
    const owner = this.hooks.owner(); this.working = true; this.error = ''; this.notice = ''; this.notify();
    try { await action(owner); }
    catch (error) { if (this.available() && owner === this.hooks.owner()) { this.error = String(error); } }
    finally { this.working = false; this.notify(); }
  }
  private async recover(owner: string): Promise<void> {
    const spools = await this.files.recover(); this.guard(owner); this.spools = spools;
    if (this.activeSpool) {
      const found = spools.find((file: PreparedMusicFile): boolean => file.spoolId === this.activeSpool!.spoolId);
      if (found) { this.activeSpool = found; }
    }
    this.notify();
  }
  load(): Promise<void> {
    return this.run(async (owner: string): Promise<void> => {
      await this.library.load(); this.guard(owner); await this.reconcilePlayer(owner); await this.recover(owner);
      // Restoring the selected ID is only a read. Opening or autoplaying audio
      // would turn application startup into an unrequested transport action.
    });
  }
  private async reconcilePlayer(owner: string): Promise<void> {
    const state = this.library.view(), player = this.playback.view();
    if (!state.loaded || state.busy || state.unknown || state.original_read || state.original_mutation || state.import_plan || !player.trackId) { return; }
    const actual = this.library.identity(player.trackId), original = this.admitted.get(player.trackId);
    if (!actual || !original || !musicPlaybackSameTrack(actual, original)) {
      await this.playback.close(); this.guard(owner); this.admitted.delete(player.trackId);
    }
    // A read or fixed write retry may retire the formerly playing track. It
    // only closes that old source; replacement transport needs a user action.
  }
  private requireWrite(): void {
    const state = this.library.view();
    if (!state.loaded || state.busy || state.unknown || state.original_read || state.original_mutation || state.import_plan) { throw new Error('请先核对音乐原操作。'); }
  }
  private async policy(action: string, value: number = 0, flag: boolean = true): Promise<NativeMusicPlayback> {
    this.requireWrite(); const state = this.library.view(), player = this.playback.view();
    const current = player.trackId || state.selected_track_id;
    const request: MusicPlaybackPolicyRequest = { library_revision: state.library_revision,
      index: String(Math.max(0, state.order.indexOf(current))), playing: player.phase === 'playing', blocked: player.background,
      position_ms: String(Math.max(0, Math.round(player.positionMs))), duration_ms: String(Math.max(0, Math.round(player.durationMs))),
      music_action: action, value: String(Math.round(value)), flag: flag };
    const result = await this.library.playbackPolicy(request);
    if (!result) { throw new Error('音乐播放策略未确认，播放器状态未替换。'); }
    return result;
  }
  private async readLyrics(trackId: string, owner: string): Promise<void> {
    const identity = this.library.identity(trackId); if (!identity) { return; }
    await this.library.readLyrics(trackId); this.guard(owner);
    this.captureLyrics(trackId, identity);
  }
  private captureLyrics(trackId: string, identity: MusicLibraryTrackIdentity): void {
    const lyrics = this.library.view().lyrics, actual = this.library.identity(trackId);
    if (lyrics && actual && lyrics.track_id === trackId && musicPlaybackSameTrack(actual, identity)) {
      const track = this.library.view().tracks.find((item: NativeMusicTrack): boolean => item.track_id === trackId);
      if (track) { this.lyrics.set(trackId, new MusicCachedLyrics(actual, lyrics, track.lyrics_byte_length)); }
      if (trackId === this.unsavedLyricsTrack && lyrics.text === this.unsavedLyrics) {
        this.unsavedLyrics = ''; this.unsavedLyricsTrack = ''; this.unsavedLyricsTitle = '';
      }
    }
  }
  private async finishImport(owner: string): Promise<void> {
    const plan = this.library.view().import_plan, file = this.activeSpool;
    if (!plan || plan.unknown || !plan.qualified_ready || !plan.record || plan.record.phase !== 'ready' || !plan.record.bytes_retained ||
      !file || !this.preparedMatches(file, plan.request) || !this.recordMatches(file, plan.record) || plan.record.request_json !== plan.original_import ||
      file.requestJson && file.requestJson !== plan.original_import) { throw new Error('原音乐完整导入尚未核对，缓存已保留。'); }
    this.guard(owner);
    if (!this.released.has(file.spoolId)) {
      try { await this.files.release(file); this.released.add(file.spoolId); }
      catch (error) {
        // A partly deleted Ready spool is no longer a complete import source.
        // Preserve its exact registered handle and plan for release retry while
        // the public recovery list only shows fully recovered sources.
        try { await this.recover(owner); } catch (_) {}
        throw error as Error;
      }
    }
    this.guard(owner); this.library.finishImport(); this.activeSpool = undefined;
    this.spools = this.spools.filter((item: PreparedMusicFile): boolean => item.spoolId !== file.spoolId);
    await this.library.load(); this.guard(owner); this.notice = '已保存完整音乐文件。'; this.notify();
  }
  private async startImport(file: PreparedMusicFile, owner: string): Promise<void> {
    this.requireWrite(); this.activeSpool = file;
    const request = new MusicImportRequest(); request.track_id = 'track-' + util.generateRandomUUID(true);
    request.operation_id = this.operation(); request.expected_revision = this.library.view().library_revision;
    request.name = file.name; request.byte_length = file.byteLength; request.sha256 = file.sha256;
    await this.library.beginImport(request); this.guard(owner);
    if (this.library.view().import_plan?.unknown) { return; }
    await this.library.commitImport(); this.guard(owner);
    if (this.library.view().import_plan?.qualified_ready && !this.library.view().import_plan?.unknown) { await this.finishImport(owner); }
  }
  private async add(owner: string): Promise<void> {
    this.requireWrite();
    try {
      const uris = await this.files.selectUris(20); this.guard(owner);
      for (const uri of uris) {
        const file = await this.files.prepareUri(uri); this.spools.push(file); this.guard(owner);
        await this.startImport(file, owner); this.guard(owner);
        if (this.library.view().import_plan || this.library.view().unknown) { break; }
      }
    } finally { this.files.discardSelection(); }
  }
  private async resume(spoolId: string, trackId: string, owner: string): Promise<void> {
    const state = this.library.view();
    if (!state.loaded || state.unknown || state.original_read || state.import_plan || state.original_mutation) { throw new Error('请先核对曲库原操作。'); }
    const file = this.spools.find((item: PreparedMusicFile): boolean => item.spoolId === spoolId);
    if (!file) { throw new Error('此音乐缓存尚未完整恢复。'); }
    let literal = file.requestJson;
    if (!literal) {
      const record = state.records.find((item: NativeMusicTrack): boolean => item.track_id === trackId && item.phase === 'pending' && this.recordMatches(file, item));
      if (!record) { throw new Error('缓存没有匹配的实际待导入原请求，请明确选择。'); }
      literal = record.request_json;
    }
    this.activeSpool = file; this.library.restoreImportLiteral(literal);
    await this.library.inspectImport(); this.guard(owner);
    // Inspection and continuation are separate user actions. A matching request
    // alone neither replays FD input nor deletes a prepared source.
  }
  private async remove(trackId: string, owner: string): Promise<void> {
    this.requireWrite(); const player = this.playback.view(), current = player.trackId === trackId;
    const wasPlaying = player.phase === 'playing';
    await this.library.remove(trackId, this.operation()); this.guard(owner);
    if (this.library.view().original_mutation || this.library.view().unknown) { return; }
    if (current) {
      await this.playback.close(); this.guard(owner);
      const selected = this.library.identity(this.library.view().selected_track_id);
      if (selected) { await this.playback.select(selected, wasPlaying); this.guard(owner); }
    }
    this.lyrics.delete(trackId);
  }
  reorder(ids: string[]): Promise<void> {
    return this.run(async (owner: string): Promise<void> => { this.requireWrite(); await this.library.reorder(ids, this.operation()); this.guard(owner); });
  }
  action(action: string, trackId: string = '', value: number = 0, spoolId: string = ''): Promise<void> {
    return this.run(async (owner: string): Promise<void> => {
      if (action === 'add') { await this.add(owner); }
      else if (action === 'select' || action === 'next' || action === 'previous') {
        const order = this.library.view().order;
        if (action === 'select' && !order.includes(trackId)) { throw new Error('所选曲目不在实际播放列表。'); }
        const proposal = await this.policy(action, action === 'select' ? order.indexOf(trackId) : 0, true); this.guard(owner);
        const identity = this.library.identity(proposal.track_id);
        if (!identity) { throw new Error('实际播放曲目未确认。'); }
        await this.playback.select(identity, proposal.playing); this.guard(owner);
      } else if (action === 'toggle') {
        if (this.playback.view().phase === 'playing') {
          await this.playback.pause(this.playback.view().token); this.guard(owner); return;
        }
        const proposal = await this.policy('toggle'); this.guard(owner);
        if (proposal.transport_effect === 'play') {
          await this.hooks.beforePlay(); this.guard(owner);
          const identity = this.library.identity(proposal.track_id);
          if (!identity) { throw new Error('实际播放曲目未确认。'); }
          await this.playback.select(identity, true);
        } else if (proposal.transport_effect === 'pause') { await this.playback.pause(this.playback.view().token); }
      } else if (action === 'seek') {
        if (!Number.isFinite(value) || value < 0) { throw new Error('播放位置无效。'); }
        const proposal = await this.policy('seek', value); this.guard(owner);
        await this.playback.seek(Number(proposal.position_ms), this.playback.view().token);
      } else if (action === 'remove') { await this.remove(trackId, owner); }
      else if (action === 'move') {
        this.requireWrite(); const order = this.library.view().order, index = order.indexOf(trackId), target = index + value;
        if (!Number.isInteger(value) || Math.abs(value) !== 1 || index < 0 || target < 0 || target >= order.length) { throw new Error('音乐移动位置无效。'); }
        const other = order[target]; order[target] = trackId; order[index] = other;
        await this.library.reorder(order, this.operation());
      } else if (action === 'lyrics_toggle') {
        if (value !== 0 && value !== 1) { throw new Error('歌词开关目标无效。'); }
        this.requireWrite();
        if (this.library.view().show_lyrics !== (value === 1)) { await this.library.setShowLyrics(value === 1, this.operation()); }
      } else if (action === 'lyrics_import') {
        this.requireWrite(); const identity = this.library.identity(trackId || this.library.view().selected_track_id);
        if (!identity) { throw new Error('请选择歌词所属曲目。'); }
        const chosen = await this.files.pickLyrics(); this.guard(owner); if (!chosen) { return; }
        const actual = this.library.identity(identity.trackId);
        if (!actual || !musicPlaybackSameTrack(actual, identity)) { throw new Error('歌词所属曲目已变化，未保存。'); }
        this.unsavedLyrics = chosen.text; this.unsavedLyricsTrack = identity.trackId; this.lyricsTrack = identity.trackId;
        const track = this.library.view().tracks.find((item: NativeMusicTrack): boolean => item.track_id === identity.trackId);
        this.unsavedLyricsTitle = track ? track.title || track.name : identity.trackId; this.notify();
        await this.library.setLyrics(identity.trackId, chosen.text, chosen.name, this.operation()); this.guard(owner);
        if (!this.library.view().unknown && !this.library.view().original_mutation) { await this.readLyrics(identity.trackId, owner); }
      } else if (action === 'lyrics_view') {
        const target = trackId || this.playback.view().trackId || this.library.view().selected_track_id;
        if (!this.lyrics.has(target) && this.unsavedLyricsTrack !== target) { this.requireWrite(); await this.readLyrics(target, owner); }
        this.lyricsTrack = target;
        this.guard(owner); this.lyricsOpen = true;
      } else if (action === 'lyrics_close') { this.lyricsOpen = false; this.lyricsTrack = ''; }
      else if (action === 'read_retry') {
        await this.library.retryRead(); this.guard(owner);
        await this.reconcilePlayer(owner); this.guard(owner);
        const lyrics = this.library.view().lyrics, identity = lyrics ? this.library.identity(lyrics.track_id) : undefined;
        if (lyrics && identity) { this.captureLyrics(lyrics.track_id, identity); }
        await this.recover(owner);
      }
      else if (action === 'mutation_retry') {
        await this.library.retryMutation(); this.guard(owner);
        await this.reconcilePlayer(owner); this.guard(owner);
        if (this.unsavedLyricsTrack && !this.library.view().unknown && !this.library.view().original_mutation) { await this.readLyrics(this.unsavedLyricsTrack, owner); }
      } else if (action === 'mutation_discard_known') { this.library.discardKnownMutation(); }
      else if (action === 'begin_retry') { await this.library.retryImportBegin(); }
      else if (action === 'import_inspect') { await this.library.inspectImport(); }
      else if (action === 'import_continue') {
        await this.library.commitImport(); this.guard(owner);
        if (this.library.view().import_plan?.qualified_ready && !this.library.view().import_plan?.unknown) { await this.finishImport(owner); }
      } else if (action === 'import_reconcile') { await this.library.reconcileImport(); }
      else if (action === 'import_finish') { await this.finishImport(owner); }
      else if (action === 'spool_resume') { await this.resume(spoolId, trackId, owner); }
      else if (action === 'spool_start') {
        this.requireWrite(); const file = this.spools.find((item: PreparedMusicFile): boolean => item.spoolId === spoolId);
        if (!file || file.requestJson || !this.spoolView(file).canStart) { throw new Error('此音乐缓存已有原请求，不能生成新导入。'); }
        await this.startImport(file, owner);
      } else if (action === 'retry_io') {
        await this.files.retryPendingIo(); this.guard(owner);
        if (this.playback.view().phase === 'cleanup_failed') { await this.playback.retryClose(); this.guard(owner); }
        await this.recover(owner);
      } else if (action === 'cleanup_cache') { await this.files.cleanupInactivePlaybackCache(); }
      else if (action === 'retry_close') { await this.playback.retryClose(); }
      else { throw new Error('未知音乐操作，未执行。'); }
      this.guard(owner); this.notify();
    });
  }
  background(): Promise<void> { return this.playback.pauseForBackground(); }
  async pauseForMedia(): Promise<void> {
    const player = this.playback.view();
    if (['failed', 'cleanup_failed', 'closing'].includes(player.phase)) { throw new Error('请先确认原音乐播放器已关闭。'); }
    if (player.phase === 'loading' || player.busy) { throw new Error('音乐播放器仍在处理请求，请稍后重试。'); }
    await this.playback.pause(player.token);
    if (!['idle', 'prepared', 'paused', 'completed'].includes(this.playback.view().phase)) { throw new Error('音乐播放器尚未确认暂停，未打开其他媒体。'); }
  }
  foreground(): void { if (!this.disposed && this.hooks.owned()) { this.playback.foreground(); this.notify(); } }
  async dispose(): Promise<void> {
    this.disposed = true; this.library.dispose(); this.files.discardSelection(); await this.playback.dispose();
  }
}
