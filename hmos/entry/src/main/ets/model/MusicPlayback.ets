// MusicFiles alone admits a verified private lease. No path, provider URI or
// native JSON reply can be substituted for that owned source.
export class MusicPlaybackIdentity {
  trackId: string = '';
  libraryRevision: string = '';
  importOperation: string = '';
  byteLength: number = 0;
  sha256: string = '';
}

export class MusicPlaybackSource extends MusicPlaybackIdentity {
  token: string = '';
}

export class MusicPlaybackDescriptor {
  token: string = '';
  fd: number = -1;
  byteLength: number = 0;
  sha256: string = '';
}

export interface MusicPlaybackFiles {
  ownsSource(source: MusicPlaybackSource): boolean;
  open(source: MusicPlaybackSource): Promise<MusicPlaybackDescriptor>;
  ownsDescriptor(source: MusicPlaybackSource, descriptor: MusicPlaybackDescriptor): boolean;
  close(descriptor: MusicPlaybackDescriptor): Promise<void>;
}

export interface MusicPlayer {
  onState(callback: (state: string) => void): void;
  onPosition(callback: (value: number) => void): void;
  onDuration(callback: (value: number) => void): void;
  onError(callback: (message: string) => void): void;
  load(): Promise<void>;
  prepare(): Promise<void>;
  play(): Promise<void>;
  pause(): Promise<void>;
  seek(value: number): Promise<void>;
  release(): Promise<void>;
}

export interface MusicPlaybackHooks {
  admitSelection(identity: MusicPlaybackIdentity): Promise<MusicPlaybackIdentity>;
  acquire(identity: MusicPlaybackIdentity): Promise<MusicPlaybackSource>;
  ownsSource(source: MusicPlaybackSource): boolean;
  createPlayer(source: MusicPlaybackSource): Promise<MusicPlayer>;
  releaseSource(source: MusicPlaybackSource): Promise<void>;
  isCurrentOwner(): boolean;
  adjacent(identity: MusicPlaybackIdentity, direction: number): MusicPlaybackIdentity | undefined;
  changed(): void;
}

export class MusicPlaybackView {
  token: string = '';
  trackId: string = '';
  libraryRevision: string = '';
  phase: string = 'idle';
  error: string = '';
  positionMs: number = 0;
  durationMs: number = 0;
  busy: boolean = false;
  background: boolean = false;
}

export function musicPlaybackIdentityValid(value: MusicPlaybackIdentity): boolean {
  return !!value && typeof value.trackId === 'string' && /^[A-Za-z0-9_.-]{1,64}$/.test(value.trackId) &&
    !value.trackId.startsWith('morrow-host-') && typeof value.importOperation === 'string' &&
    /^[A-Za-z0-9_.-]{1,128}$/.test(value.importOperation) && !value.importOperation.startsWith('morrow-host-') &&
    typeof value.libraryRevision === 'string' && /^[1-9][0-9]{0,19}$/.test(value.libraryRevision) &&
    (value.libraryRevision.length < 20 || value.libraryRevision <= '18446744073709551615') &&
    Number.isSafeInteger(value.byteLength) && value.byteLength > 0 && value.byteLength <= 150 * 1024 * 1024 &&
    typeof value.sha256 === 'string' && /^[0-9a-f]{64}$/.test(value.sha256);
}

export function musicPlaybackIdentityCopy(value: MusicPlaybackIdentity): MusicPlaybackIdentity {
  const result = new MusicPlaybackIdentity();
  result.trackId = value.trackId; result.libraryRevision = value.libraryRevision;
  result.importOperation = value.importOperation; result.byteLength = value.byteLength; result.sha256 = value.sha256;
  return result;
}

export function musicPlaybackSourceValid(value: MusicPlaybackSource): boolean {
  return musicPlaybackIdentityValid(value) && typeof value.token === 'string' && value.token.length > 0 &&
    value.token.length <= 256 && !/[\u0000-\u0020\u007f]/.test(value.token);
}

// Revision is the acquire CAS observation. It is not the stable audio identity:
// sorting, lyrics and visibility can advance it while an owned lease plays.
export function musicPlaybackSameTrack(left: MusicPlaybackIdentity, right: MusicPlaybackIdentity): boolean {
  return left.trackId === right.trackId && left.importOperation === right.importOperation &&
    left.byteLength === right.byteLength && left.sha256 === right.sha256;
}

export function musicPlaybackSameSource(left: MusicPlaybackSource, right: MusicPlaybackSource): boolean {
  return left.token === right.token && left.libraryRevision === right.libraryRevision && musicPlaybackSameTrack(left, right);
}

export function musicPlaybackSourceCopy(value: MusicPlaybackSource): MusicPlaybackSource {
  const result = new MusicPlaybackSource(); result.token = value.token;
  result.trackId = value.trackId; result.libraryRevision = value.libraryRevision;
  result.importOperation = value.importOperation; result.byteLength = value.byteLength; result.sha256 = value.sha256;
  return result;
}

export function musicPlaybackCanControl(view: MusicPlaybackView): boolean {
  return !view.background && !view.busy && ['prepared', 'playing', 'paused', 'completed'].includes(view.phase);
}

class MusicSession {
  source: MusicPlaybackSource;
  admitted: MusicPlaybackSource;
  generation: number;
  view: MusicPlaybackView = new MusicPlaybackView();
  player: MusicPlayer | undefined = undefined;
  preparing: boolean = false;
  playQueued: boolean = false;
  pauseQueued: boolean = false;
  completedSeen: boolean = false;
  acknowledgedState: string = 'idle';
  wantsPlay: boolean = false;
  closing: boolean = false;
  playerReleased: boolean = false;
  constructor(source: MusicPlaybackSource, generation: number, wantsPlay: boolean) {
    this.source = source; this.admitted = musicPlaybackSourceCopy(source); this.generation = generation;
    this.wantsPlay = wantsPlay; this.view.token = source.token; this.view.trackId = source.trackId;
    this.view.libraryRevision = source.libraryRevision; this.view.phase = 'loading';
  }
}

export class MusicPlayback {
  private hooks: MusicPlaybackHooks;
  private current: MusicSession | undefined = undefined;
  private notice: MusicPlaybackView = new MusicPlaybackView();
  private selection: number = 0;
  private work: Promise<void> = Promise.resolve();
  private pending: number = 0;
  private closeFlight: Promise<void> | undefined = undefined;
  private hidden: boolean = false;
  private disposed: boolean = false;
  constructor(hooks: MusicPlaybackHooks) { this.hooks = hooks; }

  view(): MusicPlaybackView {
    const value = this.current ? this.current.view : this.notice;
    const result = new MusicPlaybackView(); result.token = value.token; result.trackId = value.trackId;
    result.libraryRevision = value.libraryRevision; result.phase = value.phase; result.error = value.error;
    result.positionMs = value.positionMs; result.durationMs = value.durationMs;
    result.busy = this.pending > 0; result.background = this.hidden; return result;
  }

  private owned(session: MusicSession): boolean {
    return musicPlaybackSameSource(session.source, session.admitted) && this.hooks.ownsSource(session.source);
  }
  private active(session: MusicSession): boolean {
    return !this.disposed && this.hooks.isCurrentOwner() && this.current === session &&
      session.generation === this.selection && !session.closing && this.owned(session);
  }
  private notify(): void { if (this.hooks.isCurrentOwner()) { this.hooks.changed(); } }
  private fail(session: MusicSession, error: Error): void {
    if (this.current !== session || session.closing || session.view.phase === 'failed') { return; }
    session.wantsPlay = false; session.view.phase = 'failed'; session.view.error = error.message; this.notify();
    // A rejected play/seek can arrive after a real playing acknowledgement.
    // Stop acknowledged playback without claiming the failed command succeeded.
    if (session.acknowledgedState === 'playing') { this.queuePause(session); }
  }
  private enqueue(action: () => Promise<void>): Promise<void> {
    this.pending++; this.notify();
    const next = this.work.then(action);
    this.work = next.catch((_error: Error): void => {});
    next.then((): void => { this.pending--; this.notify(); }, (_error: Error): void => { this.pending--; this.notify(); });
    return next;
  }
  private validOwner(): boolean { return !this.disposed && this.hooks.isCurrentOwner(); }

  async select(identity: MusicPlaybackIdentity, play: boolean = false): Promise<void> {
    if (!this.validOwner()) { return; }
    if (!musicPlaybackIdentityValid(identity)) { throw new Error('歌曲来源身份无效'); }
    if (this.current?.view.phase === 'cleanup_failed') { throw new Error('请先重试关闭原歌曲'); }
    const target = musicPlaybackIdentityCopy(identity);
    const existing = this.current;
    const keepPlayer = !!existing && this.active(existing) && musicPlaybackSameTrack(existing.admitted, target) &&
      ['prepared', 'playing', 'paused', 'completed'].includes(existing.view.phase);
    const generation = ++this.selection;
    if (existing && keepPlayer) { existing.generation = generation; }
    else if (existing) { existing.closing = true; existing.wantsPlay = false; existing.view.phase = 'closing'; }
    await this.enqueue(async (): Promise<void> => {
      try {
        if (this.current?.view.phase === 'cleanup_failed') { throw new Error('请先重试关闭原歌曲'); }
        if (this.current && !keepPlayer) { await this.closeSession(this.current); }
        if (!this.validOwner() || generation !== this.selection) { return; }
        const admitted = await this.hooks.admitSelection(musicPlaybackIdentityCopy(target));
        if (!musicPlaybackIdentityValid(admitted) || !musicPlaybackSameTrack(admitted, target)) {
          throw new Error('歌曲选择未返回原曲目的已确认身份');
        }
        if (!this.validOwner() || generation !== this.selection) { return; }
        if (keepPlayer) {
          if (!existing || this.current !== existing || !this.active(existing)) { throw new Error('原歌曲租约已失效，请先关闭'); }
          if (play && !this.hidden) { existing.wantsPlay = true; this.queuePlay(existing); }
          return;
        }
        const selected = musicPlaybackIdentityCopy(admitted);
        const source = await this.hooks.acquire(musicPlaybackIdentityCopy(selected));
        const session = new MusicSession(source, generation, play && !this.hidden); this.current = session; this.notify();
        if (!musicPlaybackSourceValid(source) || !musicPlaybackSameTrack(source, selected) ||
          source.libraryRevision !== selected.libraryRevision || !this.owned(session)) {
          throw new Error('歌曲读取未返回原请求的已验证租约');
        }
        if (!this.active(session)) { await this.closeSession(session); return; }
        const player = await this.hooks.createPlayer(source); session.player = player;
        if (!this.active(session)) { await this.closeSession(session); return; }
        player.onState((state: string): void => this.stateChanged(session, state));
        player.onPosition((value: number): void => {
          if (!this.active(session) || session.view.phase === 'failed' || !Number.isFinite(value) || value < 0) { return; }
          session.view.positionMs = session.view.durationMs > 0 ? Math.min(value, session.view.durationMs) : value; this.notify();
        });
        player.onDuration((value: number): void => {
          if (!this.active(session) || session.view.phase === 'failed' || !Number.isFinite(value) || value < 0) { return; }
          session.view.durationMs = value; session.view.positionMs = Math.min(value, session.view.positionMs); this.notify();
        });
        player.onError((message: string): void => {
          if (this.active(session)) { this.fail(session, new Error(message || '歌曲无法播放，请检查文件或更换音频格式。')); }
        });
        await player.load();
        if (!this.active(session)) { await this.closeSession(session); }
      } catch (error) {
        const failure = error instanceof Error ? error : new Error('歌曲播放请求未能完成');
        if (this.current) { this.fail(this.current, failure); }
        else { this.notice.phase = 'failed'; this.notice.error = failure.message; this.notify(); }
        throw failure;
      }
    });
  }

  private stateChanged(session: MusicSession, state: string): void {
    if (!this.active(session)) { return; }
    session.acknowledgedState = state;
    if (session.view.phase === 'failed') {
      if (state === 'playing') { this.queuePause(session); }
      return;
    }
    if (state === 'initialized') {
      if (session.preparing) { return; }
      session.preparing = true;
      this.command(session, async (player: MusicPlayer): Promise<void> => { await player.prepare(); }); return;
    }
    if (['prepared', 'playing', 'paused', 'completed'].includes(state)) {
      session.view.phase = state; session.view.error = '';
      if (state === 'playing') { session.completedSeen = false; }
      if (state === 'completed') { session.view.positionMs = session.view.durationMs; }
      this.notify();
      if (state === 'prepared' && session.wantsPlay && !this.hidden) { this.queuePlay(session); }
      if (state === 'playing' && (this.hidden || !session.wantsPlay)) { this.queuePause(session); }
      if (state === 'completed' && !session.completedSeen) {
        session.completedSeen = true;
        if (session.wantsPlay && !this.hidden) {
          this.next().catch((_error: Error): void => {});
        }
      }
    } else if (state === 'error') { this.fail(session, new Error('歌曲无法播放，请检查文件或更换音频格式。')); }
  }

  private command(session: MusicSession, action: (player: MusicPlayer) => Promise<void>): Promise<void> {
    return this.enqueue(async (): Promise<void> => {
      if (!this.active(session) || session.view.phase === 'failed' || !session.player) { return; }
      try { await action(session.player); }
      catch (error) { this.fail(session, error instanceof Error ? error : new Error('歌曲播放请求未能完成')); }
    });
  }
  private queuePlay(session: MusicSession): Promise<void> {
    if (session.playQueued) { return this.work; }
    session.playQueued = true;
    return this.command(session, async (player: MusicPlayer): Promise<void> => {
      try {
        if (session.wantsPlay && !this.hidden && ['prepared', 'paused', 'completed'].includes(session.view.phase)) { await player.play(); }
      } finally { session.playQueued = false; }
    }).then((): void => { session.playQueued = false; });
  }
  private queuePause(session: MusicSession): Promise<void> {
    if (session.pauseQueued) { return this.work; }
    session.pauseQueued = true;
    return this.enqueue(async (): Promise<void> => {
      try {
        if (this.active(session) && session.player && session.acknowledgedState === 'playing' &&
          (this.hidden || !session.wantsPlay || session.view.phase === 'failed')) { await session.player.pause(); }
      } catch (error) { this.fail(session, error instanceof Error ? error : new Error('歌曲暂停未能完成')); }
      finally { session.pauseQueued = false; }
    }).then((): void => { session.pauseQueued = false; });
  }
  private control(token: string): MusicSession | undefined {
    const session = this.current;
    if (!session || !this.active(session) || session.view.phase === 'failed' || (token && token !== session.admitted.token)) { return undefined; }
    return session;
  }
  async play(token: string = ''): Promise<void> {
    const session = this.control(token); if (!session || this.hidden) { return; }
    session.wantsPlay = true; await this.queuePlay(session);
  }
  async pause(token: string = ''): Promise<void> {
    const session = this.control(token); if (!session) { return; }
    session.wantsPlay = false; await this.queuePause(session);
  }
  async toggle(token: string = ''): Promise<void> {
    const session = this.control(token); if (!session || this.hidden) { return; }
    if (session.wantsPlay || session.view.phase === 'playing') { await this.pause(token); } else { await this.play(token); }
  }
  async seek(value: number, token: string = ''): Promise<void> {
    const session = this.control(token);
    if (!session || this.hidden || !Number.isFinite(value) || session.view.durationMs <= 0 ||
      !['prepared', 'playing', 'paused', 'completed'].includes(session.view.phase)) { return; }
    const target = Math.round(Math.max(0, Math.min(session.view.durationMs, value)));
    await this.command(session, async (player: MusicPlayer): Promise<void> => { await player.seek(target); });
  }
  private async move(direction: number): Promise<void> {
    const session = this.control(''); if (!session || this.hidden) { return; }
    const target = this.hooks.adjacent(musicPlaybackIdentityCopy(session.admitted), direction);
    if (target) { await this.select(target, true); }
    else if (session.view.phase === 'completed') { session.wantsPlay = false; }
  }
  async next(): Promise<void> { await this.move(1); }
  async previous(): Promise<void> { await this.move(-1); }
  async pauseForBackground(): Promise<void> {
    this.hidden = true; const session = this.current; this.notify();
    if (session && this.active(session)) { session.wantsPlay = false; await this.queuePause(session); }
  }
  foreground(): void { this.hidden = false; this.notify(); }

  private async closeSession(session: MusicSession): Promise<void> {
    session.closing = true; session.wantsPlay = false; session.view.phase = 'closing'; this.notify();
    try {
      if (session.player && !session.playerReleased) { await session.player.release(); session.playerReleased = true; }
      await this.hooks.releaseSource(session.source);
      if (this.current === session) { this.current = undefined; this.notice = new MusicPlaybackView(); this.notify(); }
    } catch (error) {
      session.view.phase = 'cleanup_failed'; session.view.error = '歌曲资源未能释放，原文件已保留，请重试关闭。'; this.notify();
      throw error instanceof Error ? error : new Error('歌曲资源释放失败');
    }
  }
  private requestClose(retry: boolean): Promise<void> {
    if (this.closeFlight) { return this.closeFlight; }
    const session = this.current;
    if (session?.view.phase === 'cleanup_failed' && !retry) { return Promise.reject(new Error('请显式重试关闭原歌曲')); }
    ++this.selection;
    if (session) { session.closing = true; session.wantsPlay = false; session.view.phase = 'closing'; this.notify(); }
    const closing = this.enqueue(async (): Promise<void> => {
      if (this.current?.view.phase === 'cleanup_failed' && !retry) { throw new Error('请显式重试关闭原歌曲'); }
      if (this.current) { await this.closeSession(this.current); }
    });
    this.closeFlight = closing;
    closing.then((): void => { this.closeFlight = undefined; }, (_error: Error): void => { this.closeFlight = undefined; });
    return closing;
  }
  close(): Promise<void> { return this.requestClose(false); }
  retryClose(): Promise<void> { return this.requestClose(true); }
  dispose(): Promise<void> { this.disposed = true; return this.requestClose(false); }
}
