export class ImageGestureScope {
  token: string = '';
  generation: number = 0;
}

export class ImageGestureTicket extends ImageGestureScope {
  id: number = 0;
  kind: string = '';
}

export class ImageTransformView {
  scale: number = 1;
  x: number = 0;
  y: number = 0;
}

class ImageGestureTrack {
  ticket: ImageGestureTicket = new ImageGestureTicket();
  offsetX: number = 0;
  offsetY: number = 0;
  scale: number = 1;
  eventScale: number = 1;
  sceneX: number = 0;
  sceneY: number = 0;
}

// attachment_view.dart uses a constrained InteractiveViewer inside a tight
// SizedBox. Its zero-margin boundary is the Image layout box, including contain
// letterboxing. _matrixScale consequently raises the nominal .8 minimum to 1.
export class AttachmentImageGestures {
  private token: string = '';
  private generation: number = 0;
  private serial: number = 0;
  private width: number = 0;
  private height: number = 0;
  private ready: boolean = false;
  private current: ImageTransformView = new ImageTransformView();
  private panOne: ImageGestureTrack = new ImageGestureTrack();
  private panTwo: ImageGestureTrack = new ImageGestureTrack();
  private pinch: ImageGestureTrack = new ImageGestureTrack();

  bind(token: string, width: number, height: number): ImageGestureScope {
    this.token = token; this.width = width; this.height = height; this.ready = false;
    this.invalidate(); this.current = new ImageTransformView(); return this.scope();
  }

  resize(token: string, width: number, height: number): ImageGestureScope {
    if (token !== this.token) { return new ImageGestureScope(); }
    if (width !== this.width || height !== this.height) {
      this.width = width; this.height = height; this.invalidate(); this.current = new ImageTransformView();
    }
    return this.scope();
  }

  scope(): ImageGestureScope {
    const result = new ImageGestureScope(); result.token = this.token; result.generation = this.generation; return result;
  }

  view(): ImageTransformView {
    const result = new ImageTransformView(); result.scale = this.current.scale;
    result.x = this.current.x; result.y = this.current.y; return result;
  }

  setReady(token: string, ready: boolean): void {
    if (token !== this.token) { return; }
    if (!ready && this.ready) { this.invalidate(); }
    this.ready = ready;
  }

  invalidate(): void {
    this.generation++; this.panOne = new ImageGestureTrack(); this.panTwo = new ImageGestureTrack();
    this.pinch = new ImageGestureTrack();
  }

  dispose(): void { this.ready = false; this.token = ''; this.invalidate(); this.current = new ImageTransformView(); }

  private admits(scope: ImageGestureScope): boolean {
    return this.ready && !!this.token && scope.token === this.token && scope.generation === this.generation &&
      Number.isFinite(this.width) && this.width > 0 && Number.isFinite(this.height) && this.height > 0;
  }

  private owns(ticket: ImageGestureTicket, track: ImageGestureTrack): boolean {
    return ticket.id > 0 && ticket.id === track.ticket.id && ticket.kind === track.ticket.kind && this.admits(ticket);
  }

  private ticket(scope: ImageGestureScope, kind: string): ImageGestureTicket {
    const result = new ImageGestureTicket(); result.token = scope.token; result.generation = scope.generation;
    result.id = ++this.serial; result.kind = kind; return result;
  }

  private bounded(value: number, extent: number): number {
    const limit = extent * (this.current.scale - 1) / 2; return Math.max(-limit, Math.min(limit, value));
  }

  private move(x: number, y: number): void {
    this.current.x = this.bounded(x, this.width); this.current.y = this.bounded(y, this.height);
  }

  private scale(value: number): void { this.current.scale = Math.max(1, Math.min(2.5, value)); }

  beginPan(scope: ImageGestureScope, fingers: number, x: number, y: number): ImageGestureTicket {
    if (!this.admits(scope) || !Number.isFinite(x) || !Number.isFinite(y) ||
      (fingers !== 1 && fingers !== 2) || this.pinch.ticket.id > 0 || (fingers === 1 && this.panTwo.ticket.id > 0)) {
      return new ImageGestureTicket();
    }
    const track = new ImageGestureTrack(); track.ticket = this.ticket(scope, fingers === 1 ? 'pan-one' : 'pan-two');
    track.offsetX = x; track.offsetY = y;
    if (fingers === 1) { this.panOne = track; }
    else { this.panOne = new ImageGestureTrack(); this.panTwo = track; }
    return track.ticket;
  }

  movePan(ticket: ImageGestureTicket, x: number, y: number): boolean {
    const track = ticket.kind === 'pan-one' ? this.panOne : this.panTwo;
    if (!this.owns(ticket, track) || this.pinch.ticket.id > 0 || !Number.isFinite(x) || !Number.isFinite(y)) { return false; }
    this.move(this.current.x + x - track.offsetX, this.current.y + y - track.offsetY);
    // Rebase even when a boundary clips the movement. Reversing direction then
    // moves immediately, as InteractiveViewer updates its reference scene point.
    track.offsetX = x; track.offsetY = y; return true;
  }

  endPan(ticket: ImageGestureTicket, x: number, y: number): boolean {
    const changed = this.movePan(ticket, x, y); this.cancel(ticket); return changed;
  }

  beginPinch(scope: ImageGestureScope, scale: number, x: number, y: number): ImageGestureTicket {
    if (!this.admits(scope) || !Number.isFinite(scale) || scale <= 0 || !Number.isFinite(x) || !Number.isFinite(y)) {
      return new ImageGestureTicket();
    }
    const track = new ImageGestureTrack(); track.ticket = this.ticket(scope, 'pinch');
    track.scale = this.current.scale; track.eventScale = scale;
    track.sceneX = (x - this.width / 2 - this.current.x) / this.current.scale;
    track.sceneY = (y - this.height / 2 - this.current.y) / this.current.scale;
    this.panOne = new ImageGestureTrack(); this.panTwo = new ImageGestureTrack(); this.pinch = track; return track.ticket;
  }

  movePinch(ticket: ImageGestureTicket, scale: number, x: number, y: number): boolean {
    const track = this.pinch;
    if (!this.owns(ticket, track) || !Number.isFinite(scale) || scale <= 0 || !Number.isFinite(x) || !Number.isFinite(y)) {
      return false;
    }
    const desiredScale = track.scale * (scale / track.eventScale);
    if (!Number.isFinite(desiredScale)) { return false; }
    this.scale(desiredScale);
    const focalX = x - this.width / 2, focalY = y - this.height / 2;
    const desiredX = focalX - track.sceneX * this.current.scale;
    const desiredY = focalY - track.sceneY * this.current.scale;
    this.move(desiredX, desiredY);
    // Keep the moving two-finger focal point on its scene point, rebasing the
    // reference only where the zero-margin layout boundary stopped translation.
    if (Math.abs(desiredX - this.current.x) > 0.000000001) { track.sceneX = (focalX - this.current.x) / this.current.scale; }
    if (Math.abs(desiredY - this.current.y) > 0.000000001) { track.sceneY = (focalY - this.current.y) / this.current.scale; }
    return true;
  }

  endPinch(ticket: ImageGestureTicket, scale: number, x: number, y: number): boolean {
    const changed = this.movePinch(ticket, scale, x, y); this.cancel(ticket); return changed;
  }

  cancel(ticket: ImageGestureTicket): void {
    if (this.owns(ticket, this.panOne)) { this.panOne = new ImageGestureTrack(); }
    if (this.owns(ticket, this.panTwo)) { this.panTwo = new ImageGestureTrack(); }
    if (this.owns(ticket, this.pinch)) { this.pinch = new ImageGestureTrack(); }
  }

  reset(scope: ImageGestureScope): boolean {
    if (!this.admits(scope)) { return false; }
    this.panOne = new ImageGestureTrack(); this.panTwo = new ImageGestureTrack(); this.pinch = new ImageGestureTrack();
    this.current = new ImageTransformView(); return true;
  }

  zoomBy(scope: ImageGestureScope, delta: number): boolean {
    if (!this.admits(scope) || !Number.isFinite(delta)) { return false; }
    this.panOne = new ImageGestureTrack(); this.panTwo = new ImageGestureTrack(); this.pinch = new ImageGestureTrack();
    this.scale(this.current.scale + delta); this.move(this.current.x, this.current.y); return true;
  }
}
