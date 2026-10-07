import { VerifiedPreview } from '../model/AttachmentFiles';
import { AttachmentImageGestures, ImageGestureScope, ImageGestureTicket } from '../model/AttachmentImageGestures';
import { uiText } from '../model/UiStrings';

// The parent owns the verified token. This component neither reads Engine
// records nor deletes files; decode failure preserves the available export.
@Component
export struct AttachmentImagePreview {
  @Prop @Watch('previewChanged') preview: VerifiedPreview | undefined = undefined;
  @Prop @Watch('availabilityChanged') loading: boolean = false;
  @Prop @Watch('availabilityChanged') failure: string = '';
  @Prop @Watch('geometryChanged') contentWidth: number = 620;
  @Prop @Watch('geometryChanged') contentHeight: number = 320;
  @Prop ink: string = '#302D43';
  @Prop muted: string = '#81798F';
  @Prop accent: string = '#7662BA';
  @Prop family: string = 'HarmonyOS Sans';
  @Prop locale: string = 'zh';
  @State decoded: boolean = false;
  @State decodeFailed: boolean = false;
  @State zoom: number = 1;
  @State offsetX: number = 0;
  @State offsetY: number = 0;
  @State private imageSession: ImagePreviewSession = new ImagePreviewSession();
  private gestures: AttachmentImageGestures = new AttachmentImageGestures();
  private epoch: number = 0;
  private sessionSerial: number = 0;
  private alive: boolean = false;
  private canvasVisible: boolean = true;

  aboutToAppear(): void { this.alive = true; this.canvasVisible = true; this.previewChanged(); }
  aboutToDisappear(): void {
    this.alive = false; this.epoch++; this.gestures.dispose(); this.imageSession = new ImagePreviewSession(); this.syncView();
  }
  private t(value: string): string { return uiText(value, this.locale); }
  private previewChanged(): void {
    this.epoch++; this.decoded = false; this.decodeFailed = false;
    this.gestures.bind(this.preview?.token || '', this.contentWidth, this.contentHeight); this.refreshSession(); this.syncView();
  }
  private geometryChanged(): void {
    this.gestures.resize(this.preview?.token || '', this.contentWidth, this.contentHeight);
    this.refreshSession(); this.syncView();
  }
  private availabilityChanged(): void {
    if (!this.canInteract()) { this.gestures.invalidate(); }
    this.gestures.setReady(this.preview?.token || '', this.canInteract()); this.refreshSession();
  }
  private refreshSession(): void {
    const session = new ImagePreviewSession(); session.token = this.preview?.token || '';
    session.uri = this.preview?.uri || ''; session.name = this.preview?.name || '';
    session.epoch = this.epoch; session.identity = ++this.sessionSerial;
    session.scope = this.gestures.scope(); this.imageSession = session;
  }
  private syncView(): void {
    const view = this.gestures.view(); this.zoom = view.scale; this.offsetX = view.x; this.offsetY = view.y;
  }
  private ownsSession(session: ImagePreviewSession): boolean {
    return this.alive && this.imageSession === session && session.epoch === this.epoch && !!session.token &&
      session.token === this.preview?.token && session.uri === this.preview?.uri;
  }
  private canInteract(): boolean {
    return this.alive && this.canvasVisible && this.decoded && !this.decodeFailed && !this.loading &&
      this.failure.length === 0 && !!this.preview?.token && !!this.preview?.uri;
  }
  private admitted(session: ImagePreviewSession): boolean { return this.ownsSession(session) && this.canInteract(); }
  private resetFor(session: ImagePreviewSession): void {
    if (this.admitted(session) && this.gestures.reset(session.scope)) { this.syncView(); }
  }
  private zoomFor(session: ImagePreviewSession, delta: number): void {
    if (this.admitted(session) && this.gestures.zoomBy(session.scope, delta)) { this.syncView(); }
  }
  private visibilityFor(session: ImagePreviewSession, visible: boolean): void {
    if (!this.ownsSession(session) || this.canvasVisible === visible) { return; }
    this.canvasVisible = visible;
    if (!visible) { this.gestures.setReady(session.token, false); this.gestures.invalidate(); }
    else { this.gestures.setReady(session.token, this.canInteract()); this.refreshSession(); }
  }
  private completeFor(token: string, uri: string, epoch: number): (event?: ImageLoadResult) => void {
    return (event?: ImageLoadResult): void => {
      if (!this.alive || this.epoch !== epoch || this.preview?.token !== token || this.preview?.uri !== uri ||
        this.decodeFailed || !event) { return; }
      // A successful source read (status 0) is not proof of decoding (status 1).
      if (event.loadingStatus === 1 && Number.isFinite(event.width) && event.width > 0 &&
        Number.isFinite(event.height) && event.height > 0) {
        this.decoded = true; this.gestures.setReady(token, this.canInteract());
      }
    };
  }
  private failureFor(token: string, uri: string, epoch: number): () => void {
    return (): void => {
      if (this.alive && this.epoch === epoch && this.preview?.token === token && this.preview?.uri === uri) {
        this.decodeFailed = true; this.decoded = false; this.gestures.bind(token, this.contentWidth, this.contentHeight);
        this.refreshSession(); this.syncView();
      }
    };
  }
  private panStart(session: ImagePreviewSession, fingers: number, event: GestureEvent): void {
    if (!this.admitted(session) || !Number.isFinite(event.timestamp)) { return; }
    if (event.timestamp < (fingers === 1 ? session.panOneTime : session.panTwoTime)) { return; }
    const ticket = this.gestures.beginPan(session.scope, fingers, event.offsetX, event.offsetY);
    if (fingers === 1) { session.panOne = ticket; session.panOneTime = event.timestamp; }
    else { session.panTwo = ticket; session.panTwoTime = event.timestamp; }
  }
  private acceptsPanEvent(session: ImagePreviewSession, fingers: number, event: GestureEvent): boolean {
    if (!this.admitted(session) || !Number.isFinite(event.timestamp) ||
      event.timestamp < (fingers === 1 ? session.panOneTime : session.panTwoTime)) { return false; }
    if (fingers === 1) { session.panOneTime = event.timestamp; } else { session.panTwoTime = event.timestamp; }
    return true;
  }
  private panUpdate(session: ImagePreviewSession, fingers: number, event: GestureEvent): void {
    if (this.acceptsPanEvent(session, fingers, event) && this.gestures.movePan(fingers === 1 ? session.panOne : session.panTwo,
      event.offsetX, event.offsetY)) { this.syncView(); }
  }
  private panEnd(session: ImagePreviewSession, fingers: number, event: GestureEvent): void {
    if (this.acceptsPanEvent(session, fingers, event) && this.gestures.endPan(fingers === 1 ? session.panOne : session.panTwo,
      event.offsetX, event.offsetY)) { this.syncView(); }
  }
  private panCancel(session: ImagePreviewSession, fingers: number, event: GestureEvent): void {
    if (event && this.acceptsPanEvent(session, fingers, event)) {
      this.gestures.cancel(fingers === 1 ? session.panOne : session.panTwo);
    }
  }
  private pinchStart(session: ImagePreviewSession, event: GestureEvent): void {
    if (this.admitted(session) && Number.isFinite(event.timestamp) && event.timestamp >= session.pinchTime) {
      session.pinch = this.gestures.beginPinch(session.scope, event.scale, event.pinchCenterX, event.pinchCenterY);
      session.pinchTime = event.timestamp;
    }
  }
  private acceptsPinchEvent(session: ImagePreviewSession, event: GestureEvent): boolean {
    if (!this.admitted(session) || !Number.isFinite(event.timestamp) || event.timestamp < session.pinchTime) { return false; }
    session.pinchTime = event.timestamp; return true;
  }
  private pinchUpdate(session: ImagePreviewSession, event: GestureEvent): void {
    if (this.acceptsPinchEvent(session, event) && this.gestures.movePinch(session.pinch, event.scale, event.pinchCenterX,
      event.pinchCenterY)) { this.syncView(); }
  }
  private pinchEnd(session: ImagePreviewSession, event: GestureEvent): void {
    if (this.acceptsPinchEvent(session, event) && this.gestures.endPinch(session.pinch, event.scale, event.pinchCenterX,
      event.pinchCenterY)) { this.syncView(); }
  }
  private pinchCancel(session: ImagePreviewSession, event: GestureEvent): void {
    if (event && this.acceptsPinchEvent(session, event)) { this.gestures.cancel(session.pinch); }
  }

  @Builder
  ImageCanvas(session: ImagePreviewSession) {
    Stack() {
      if (!this.decodeFailed) {
        Image(session.uri).objectFit(ImageFit.Contain).width('100%').height('100%')
          .scale({ x: this.zoom, y: this.zoom }).translate({ x: this.offsetX, y: this.offsetY })
          .accessibilityText(session.name).id('verified-attachment-image')
          .onComplete(this.completeFor(session.token, session.uri, session.epoch))
          .onError(this.failureFor(session.token, session.uri, session.epoch))
          .hitTestBehavior(HitTestMode.None)
      }
      if (this.decodeFailed) {
        Text(this.t('图片无法解码，可另存后打开。')).fontFamily(this.family).fontSize(13).fontColor(this.muted)
          .textAlign(TextAlign.Center).padding(24).hitTestBehavior(HitTestMode.None).id('attachment-image-decode-failure')
      } else if (!this.decoded) {
        LoadingProgress().width(32).height(32).color(this.accent).hitTestBehavior(HitTestMode.None)
          .id('attachment-image-decode-loading')
      }
    }.width(this.contentWidth).height(this.contentHeight).clip(true).hitTestBehavior(HitTestMode.Block)
      .id('attachment-image-gesture-canvas')
      .onVisibleAreaChange([0], (visible: boolean, _ratio: number): void => { this.visibilityFor(session, visible); })
      .gesture(GestureGroup(GestureMode.Parallel,
        PinchGesture({ fingers: 2, isFingerCountLimited: true })
          .onActionStart((event: GestureEvent): void => { this.pinchStart(session, event); })
          .onActionUpdate((event: GestureEvent): void => { this.pinchUpdate(session, event); })
          .onActionEnd((event: GestureEvent): void => { this.pinchEnd(session, event); })
          .onActionCancel((event: GestureEvent): void => { this.pinchCancel(session, event); }),
        PanGesture({ fingers: 1, direction: PanDirection.All, isFingerCountLimited: true })
          .onActionStart((event: GestureEvent): void => { this.panStart(session, 1, event); })
          .onActionUpdate((event: GestureEvent): void => { this.panUpdate(session, 1, event); })
          .onActionEnd((event: GestureEvent): void => { this.panEnd(session, 1, event); })
          .onActionCancel((event: GestureEvent): void => { this.panCancel(session, 1, event); }),
        PanGesture({ fingers: 2, direction: PanDirection.All, isFingerCountLimited: true })
          .onActionStart((event: GestureEvent): void => { this.panStart(session, 2, event); })
          .onActionUpdate((event: GestureEvent): void => { this.panUpdate(session, 2, event); })
          .onActionEnd((event: GestureEvent): void => { this.panEnd(session, 2, event); })
          .onActionCancel((event: GestureEvent): void => { this.panCancel(session, 2, event); }),
        // The existing HMOS double-tap/reset affordance complements Flutter's
        // scale/pan reference; attachment_view.dart registers no double tap.
        TapGesture({ count: 2 }).onAction((): void => { this.resetFor(session); })
      ))
  }

  build() {
    Column({ space: 8 }) {
      if (this.failure.length > 0) {
        Text(this.failure).fontFamily(this.family).fontSize(13).fontColor(this.muted).textAlign(TextAlign.Center)
          .padding(24).width(this.contentWidth).height(this.contentHeight).id('attachment-image-read-failure')
      } else if (this.loading || !this.preview) {
        Stack() { LoadingProgress().width(32).height(32).color(this.accent) }
          .width(this.contentWidth).height(this.contentHeight).id('attachment-image-read-loading')
      } else {
        // Reconstruct both Image and gesture/control closures for each admitted
        // owner. Even an unchanged layout/availability watch can replace that
        // owner; retaining its ForEach key would strand callbacks on the old one.
        ForEach([this.imageSession], (session: ImagePreviewSession): void => {
          this.ImageCanvas(session)
          this.ImageControls(session)
        }, (session: ImagePreviewSession): string => session.key())
      }
    }.width(this.contentWidth)
  }

  @Builder
  ImageControls(session: ImagePreviewSession) {
      Row({ space: 16 }) {
        Button('−', { type: ButtonType.Normal }).fontSize(19).fontColor(this.accent).backgroundColor(Color.Transparent)
          .enabled(this.admitted(session) && this.zoom > 1).onClick((): void => { this.zoomFor(session, -0.25); })
          .accessibilityText('Zoom out').id('attachment-image-zoom-out')
        Button(Math.round(this.zoom * 100).toString() + '%', { type: ButtonType.Normal }).fontSize(12).fontColor(this.ink)
          .backgroundColor(Color.Transparent).enabled(this.admitted(session)).onClick((): void => { this.resetFor(session); })
          .accessibilityText('Reset zoom').id('attachment-image-zoom-reset')
        Button('+', { type: ButtonType.Normal }).fontSize(19).fontColor(this.accent).backgroundColor(Color.Transparent)
          .enabled(this.admitted(session) && this.zoom < 2.5).onClick((): void => { this.zoomFor(session, 0.25); })
          .accessibilityText('Zoom in').id('attachment-image-zoom-in')
      }.height(32)
  }
}

class ImagePreviewSession {
  token: string = '';
  uri: string = '';
  name: string = '';
  epoch: number = 0;
  identity: number = 0;
  scope: ImageGestureScope = new ImageGestureScope();
  panOne: ImageGestureTicket = new ImageGestureTicket();
  panTwo: ImageGestureTicket = new ImageGestureTicket();
  pinch: ImageGestureTicket = new ImageGestureTicket();
  panOneTime: number = -1;
  panTwoTime: number = -1;
  pinchTime: number = -1;
  key(): string { return this.token + ':' + this.epoch + ':' + this.scope.generation + ':' + this.identity; }
}

interface ImageLoadResult {
  width: number;
  height: number;
  loadingStatus: number;
}
