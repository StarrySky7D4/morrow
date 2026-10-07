import { PlaybackView, playbackCanControl } from './AttachmentPlayback';

export interface MediaControlClock {
  schedule(callback: () => void, delay: number): number;
  cancel(id: number): void;
}

export class MediaSystemClock implements MediaControlClock {
  schedule(callback: () => void, delay: number): number { return setTimeout(callback, delay); }
  cancel(id: number): void { clearTimeout(id); }
}

export class MediaGestureAction {
  token: string = '';
  kind: string = '';
  value: number = 0;
  deltaMs: number = 0;
}

function seekAction(view: PlaybackView, delta: number): MediaGestureAction {
  const result = new MediaGestureAction(); result.token = view.token; result.kind = 'seek'; result.deltaMs = delta;
  result.value = Math.max(0, Math.min(view.durationMs, view.positionMs + delta)); return result;
}

function mediaReady(view: PlaybackView, fullscreen: boolean): boolean {
  return fullscreen && playbackCanControl(view) && !!view.token && Number.isFinite(view.durationMs) && view.durationMs > 0 &&
    Number.isFinite(view.positionMs) && view.positionMs >= 0 && view.positionMs <= view.durationMs;
}

function tapSide(localX: number, width: number): number {
  if (!Number.isFinite(localX) || !Number.isFinite(width) || width <= 0 || localX < 0 || localX > width) { return 0; }
  if (localX <= width / 3) { return -1; } if (localX >= width * 2 / 3) { return 1; } return 0;
}

// The Flutter defaults enable horizontal/fullscreen seek. Its vertical volume
// and brightness gestures require callbacks absent from this app's reference.
export class AttachmentMediaGestures {
  private token: string = '';
  private delta: number = 0;
  private moved: boolean = false;
  cancel(): void { this.token = ''; this.delta = 0; this.moved = false; }
  begin(view: PlaybackView, fullscreen: boolean): void {
    this.cancel(); if (mediaReady(view, fullscreen)) { this.token = view.token; }
  }
  move(offsetX: number, view: PlaybackView, fullscreen: boolean): MediaGestureAction {
    if (!this.token || this.token !== view.token || !mediaReady(view, fullscreen) || !Number.isFinite(offsetX)) {
      return new MediaGestureAction();
    }
    const proportional = offsetX * Math.floor(view.durationMs / 1000) / 1000;
    // Dart rounds halfway values away from zero; JavaScript Math.round(-0.5)
    // produces -0. Match -(initialX-currentX).round() for leftward gestures.
    const rounded = proportional < 0 ? -Math.round(-proportional) : Math.round(proportional);
    const seconds = rounded === 0 ? 0 : rounded;
    // Material controls leave the last accepted delta unchanged outside bounds.
    const relative = Math.floor(view.positionMs / 1000) + seconds;
    if (relative < 0 || relative > Math.floor(view.durationMs / 1000)) { return new MediaGestureAction(); }
    this.delta = seconds * 1000; this.moved = true; return seekAction(view, this.delta);
  }
  finish(view: PlaybackView, fullscreen: boolean): MediaGestureAction {
    const result = this.token === view.token && mediaReady(view, fullscreen) && this.moved && this.delta !== 0 ?
      seekAction(view, this.delta) : new MediaGestureAction();
    this.cancel(); return result;
  }
}

export class AttachmentControlVisibility {
  private token: string = '';
  private fullscreen: boolean = false;
  private visible: boolean = false;
  private held: boolean = false;
  private epoch: number = 0;
  private timer: number = -1;
  private clock: MediaControlClock;
  private changed: (visible: boolean) => void;
  constructor(changed: (visible: boolean) => void, clock: MediaControlClock = new MediaSystemClock()) {
    this.changed = changed; this.clock = clock;
  }
  bind(token: string, fullscreen: boolean): void {
    if (this.token === token && this.fullscreen === fullscreen) { return; }
    this.stopTimer(); this.token = token; this.fullscreen = fullscreen; this.held = false;
    this.visible = false; this.changed(false);
  }
  view(): boolean { return this.visible; }
  tap(token: string): void {
    if (!token || token !== this.token) { return; }
    if (this.visible) { this.stopTimer(); this.visible = false; this.changed(false); }
    else { this.interact(token); }
  }
  interact(token: string): void {
    if (!token || token !== this.token) { return; }
    this.stopTimer(); this.visible = true; this.changed(true);
    if (!this.held) { this.hideLater(); }
  }
  hold(token: string): void {
    if (token !== this.token) { return; } this.held = true; this.stopTimer();
  }
  release(token: string): void {
    if (token !== this.token) { return; } this.held = false; if (this.visible) { this.hideLater(); }
  }
  dispose(): void { this.stopTimer(); this.token = ''; this.visible = false; this.held = false; }
  private stopTimer(): void {
    this.epoch++; if (this.timer >= 0) { this.clock.cancel(this.timer); } this.timer = -1;
  }
  private hideLater(): void {
    this.stopTimer(); const epoch = this.epoch, token = this.token;
    this.timer = this.clock.schedule((): void => {
      if (this.epoch !== epoch || token !== this.token || this.held) { return; }
      this.timer = -1; this.visible = false; this.changed(false);
    }, 3000);
  }
}

// Material's double-tap indicator accumulates 10-second steps and submits after
// 400 ms, then fades for 200 ms. Both timers retain their original lease token.
export class AttachmentDoubleTapSeek {
  private token: string = '';
  private direction: number = 0;
  private delta: number = 0;
  private timer: number = -1;
  private epoch: number = 0;
  private clock: MediaControlClock;
  private current: () => PlaybackView;
  private fullscreen: () => boolean;
  private changed: (action: MediaGestureAction) => void;
  private submitted: (action: MediaGestureAction) => void;
  constructor(current: () => PlaybackView, fullscreen: () => boolean, changed: (action: MediaGestureAction) => void,
    submitted: (action: MediaGestureAction) => void, clock: MediaControlClock = new MediaSystemClock()) {
    this.current = current; this.fullscreen = fullscreen; this.changed = changed; this.submitted = submitted; this.clock = clock;
  }
  trigger(view: PlaybackView, fullscreen: boolean, localX: number, width: number): void {
    const direction = tapSide(localX, width);
    if (!mediaReady(view, fullscreen) || direction === 0) { return; }
    if (this.token !== view.token || this.direction !== direction) { this.cancel(); this.token = view.token; this.direction = direction; }
    this.delta += direction * 10000; this.changed(seekAction(view, this.delta)); this.submitLater();
  }
  increment(view: PlaybackView, fullscreen: boolean, localX: number, width: number): boolean {
    if (!this.token || this.token !== view.token || tapSide(localX, width) !== this.direction || !mediaReady(view, fullscreen)) { return false; }
    this.trigger(view, fullscreen, localX, width); return true;
  }
  cancel(): void {
    this.epoch++; if (this.timer >= 0) { this.clock.cancel(this.timer); } this.timer = -1;
    this.token = ''; this.direction = 0; this.delta = 0; this.changed(new MediaGestureAction());
  }
  private submitLater(): void {
    this.epoch++; if (this.timer >= 0) { this.clock.cancel(this.timer); }
    const epoch = this.epoch, token = this.token, delta = this.delta;
    this.timer = this.clock.schedule((): void => {
      if (this.epoch !== epoch || this.token !== token) { return; }
      this.timer = -1; const view = this.current();
      if (view.token !== token || !mediaReady(view, this.fullscreen())) { this.cancel(); return; }
      this.submitted(seekAction(view, delta));
      this.token = ''; this.direction = 0; this.delta = 0;
      this.timer = this.clock.schedule((): void => {
        if (this.epoch === epoch) { this.timer = -1; this.changed(new MediaGestureAction()); }
      }, 200);
    }, 400);
  }
}
