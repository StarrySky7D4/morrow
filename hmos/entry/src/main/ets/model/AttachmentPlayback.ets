// A playback session owns its player and descriptor until release succeeds.
// The parent owns the verified file lease and must await dispose before deleting it.
export class PlaybackSource {
  token: string = '';
  path: string = '';
  uri: string = '';
  kind: string = 'audio';
  byteLength: number = 0;
}

export class PlaybackView {
  token: string = '';
  kind: string = 'audio';
  phase: string = 'idle';
  error: string = '';
  positionMs: number = 0;
  durationMs: number = 0;
  busy: boolean = false;
}

export interface AttachmentPlayer {
  onState(callback: (state: string) => void): void;
  onPosition(callback: (value: number) => void): void;
  onDuration(callback: (value: number) => void): void;
  onError(callback: (message: string) => void): void;
  load(): Promise<void>;
  setSurface(id: string): void;
  prepare(): Promise<void>;
  play(): Promise<void>;
  pause(): Promise<void>;
  seek(value: number): void;
  release(): Promise<void>;
}

class PlaybackSession {
  source: PlaybackSource;
  view: PlaybackView = new PlaybackView();
  player: AttachmentPlayer | undefined = undefined;
  creation: Promise<void> = Promise.resolve();
  work: Promise<void> = Promise.resolve();
  cleanup: Promise<void> | undefined = undefined;
  closing: boolean = false;
  initialized: boolean = false;
  preparing: boolean = false;
  surface: string = '';
  background: boolean = false;
  constructor(source: PlaybackSource) {
    this.source = source; this.view.token = source.token; this.view.kind = source.kind; this.view.phase = 'loading';
  }
}

export function playbackTime(value: number): string {
  const seconds = Math.floor(Math.max(0, Number.isFinite(value) ? value : 0) / 1000);
  const minutes = Math.floor(seconds / 60);
  return minutes.toString() + ':' + (seconds % 60).toString().padStart(2, '0');
}

export function playbackCanControl(view: PlaybackView): boolean {
  return ['prepared', 'playing', 'paused', 'completed'].includes(view.phase) && !view.busy;
}

export class AttachmentPlayback {
  private current: PlaybackSession | undefined = undefined;
  private operation: number = 0;
  private factory: (source: PlaybackSource) => Promise<AttachmentPlayer>;
  private changed: () => void;
  constructor(factory: (source: PlaybackSource) => Promise<AttachmentPlayer>, changed: () => void) {
    this.factory = factory; this.changed = changed;
  }

  view(): PlaybackView {
    const result = new PlaybackView();
    const value = this.current?.view;
    if (value) {
      result.token = value.token; result.kind = value.kind; result.phase = value.phase; result.error = value.error;
      result.positionMs = value.positionMs; result.durationMs = value.durationMs; result.busy = value.busy;
    }
    return result;
  }

  async open(source: PlaybackSource): Promise<void> {
    const operation = ++this.operation;
    await this.closeCurrent();
    if (operation !== this.operation) { return; }
    if (!source.token || !source.path || !source.uri || !['audio', 'video'].includes(source.kind) ||
      !Number.isSafeInteger(source.byteLength) || source.byteLength <= 0) { throw new Error('媒体读取回复无效'); }
    const frozen = new PlaybackSource(); frozen.token = source.token; frozen.path = source.path;
    frozen.uri = source.uri; frozen.kind = source.kind; frozen.byteLength = source.byteLength;
    const session = new PlaybackSession(frozen); this.current = session;
    session.creation = this.create(session);
    this.changed();
    await session.creation;
  }

  private active(session: PlaybackSession): boolean { return this.current === session && !session.closing; }
  private notify(session: PlaybackSession): void { if (this.current === session) { this.changed(); } }
  private fail(session: PlaybackSession, error: Error | string): void {
    if (!this.active(session)) { return; }
    session.view.phase = 'failed'; session.view.busy = false;
    session.view.error = typeof error === 'string' ? error : error.message; this.notify(session);
  }

  private async create(session: PlaybackSession): Promise<void> {
    try {
      const player = await this.factory(session.source); session.player = player;
      if (!this.active(session)) { return; }
      player.onState((state: string): void => this.stateChanged(session, state));
      player.onPosition((value: number): void => {
        if (!this.active(session) || session.view.phase === 'failed' || !Number.isFinite(value) || value < 0) { return; }
        session.view.positionMs = session.view.durationMs > 0 ? Math.min(value, session.view.durationMs) : value;
        this.notify(session);
      });
      player.onDuration((value: number): void => {
        if (!this.active(session) || session.view.phase === 'failed' || !Number.isFinite(value) || value < 0) { return; }
        session.view.durationMs = value;
        session.view.positionMs = Math.min(session.view.positionMs, value); this.notify(session);
      });
      player.onError((message: string): void => this.fail(session, message || '媒体无法播放，可另存后打开。'));
      this.enqueue(session, async (): Promise<void> => { await player.load(); });
      await session.work;
    } catch (error) {
      this.fail(session, error instanceof Error ? error : String(error));
    }
  }

  private stateChanged(session: PlaybackSession, state: string): void {
    if (!this.active(session) || session.view.phase === 'failed') { return; }
    if (state === 'initialized') {
      session.initialized = true; this.prepareIfReady(session); return;
    }
    if (['prepared', 'playing', 'paused', 'completed'].includes(state)) {
      session.view.phase = state; session.view.error = '';
      if (state === 'completed') { session.view.positionMs = session.view.durationMs; }
      this.notify(session);
      // A queued play may complete after the ability was hidden. Pause it as
      // soon as the platform acknowledges playback, without reopening media.
      if (state === 'playing' && session.background) {
        this.enqueue(session, async (): Promise<void> => {
          if (session.view.phase === 'playing') { await session.player?.pause(); }
        });
      }
    } else if (state === 'error') { this.fail(session, '媒体无法播放，可另存后打开。'); }
  }

  setSurface(token: string, id: string): void {
    const session = this.current;
    if (!session || !this.active(session) || session.source.token !== token || session.source.kind !== 'video' || !id) { return; }
    session.surface = id;
    if (session.preparing) {
      this.enqueue(session, async (): Promise<void> => { session.player?.setSurface(id); });
    } else { this.prepareIfReady(session); }
  }

  private prepareIfReady(session: PlaybackSession): void {
    if (!this.active(session) || !session.initialized || session.preparing ||
      (session.source.kind === 'video' && !session.surface)) { return; }
    session.preparing = true;
    this.enqueue(session, async (): Promise<void> => {
      if (session.source.kind === 'video') { session.player?.setSurface(session.surface); }
      await session.player?.prepare();
    });
  }

  private enqueue(session: PlaybackSession, action: () => Promise<void>): Promise<void> {
    session.view.busy = true; this.notify(session);
    const next = session.work.then(async (): Promise<void> => {
      if (this.active(session) && session.view.phase !== 'failed') { await action(); }
    }).catch((error: Error): void => this.fail(session, error));
    session.work = next;
    next.then((): void => {
      if (session.work === next && this.active(session)) { session.view.busy = false; this.notify(session); }
    });
    return next;
  }

  async toggle(): Promise<void> {
    const session = this.current;
    if (!session || !this.active(session) || !playbackCanControl(session.view)) { return; }
    session.background = false;
    const pause = session.view.phase === 'playing';
    await this.enqueue(session, async (): Promise<void> => {
      if (pause) { await session.player?.pause(); } else { await session.player?.play(); }
    });
  }

  async seek(value: number): Promise<void> {
    const session = this.current;
    if (!session || !this.active(session) || !playbackCanControl(session.view) ||
      !Number.isFinite(value) || session.view.durationMs <= 0) { return; }
    const target = Math.round(Math.max(0, Math.min(session.view.durationMs, value)));
    await this.enqueue(session, async (): Promise<void> => { session.player?.seek(target); });
  }

  async pauseForBackground(token: string = ''): Promise<void> {
    const session = this.current;
    if (!session || !this.active(session) || (token && session.source.token !== token)) { return; }
    session.background = true;
    // Serialize behind an accepted play even before its state callback arrives.
    await this.enqueue(session, async (): Promise<void> => {
      if (session.view.phase === 'playing') { await session.player?.pause(); }
    });
  }

  async dispose(): Promise<void> {
    this.operation++;
    await this.closeCurrent();
  }

  private async closeCurrent(): Promise<void> {
    const session = this.current;
    if (!session) { return; }
    if (session.cleanup) { return session.cleanup; }
    session.closing = true; session.view.phase = 'closing'; session.view.busy = true; this.notify(session);
    const cleanup = this.cleanupSession(session); session.cleanup = cleanup;
    try { await cleanup; } finally { session.cleanup = undefined; }
  }

  private async cleanupSession(session: PlaybackSession): Promise<void> {
    try {
      await session.creation; await session.work;
      await session.player?.release();
      session.player = undefined;
      if (this.current === session) { this.current = undefined; this.changed(); }
    } catch (error) {
      session.view.phase = 'cleanup_failed'; session.view.busy = false;
      session.view.error = '媒体资源未能释放，文件已保留，请重试关闭。'; this.notify(session);
      throw new Error('媒体资源释放失败，请重试关闭。');
    }
  }
}
