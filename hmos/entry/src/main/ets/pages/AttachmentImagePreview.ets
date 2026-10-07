import { VerifiedPreview, imagePreviewScale, imagePreviewOffset } from '../model/AttachmentFiles';
import { uiText } from '../model/UiStrings';

// The parent owns the verified token. This component neither reads Engine
// records nor deletes files; decode failure preserves the available export.
@Component
export struct AttachmentImagePreview {
  @Prop @Watch('previewChanged') preview: VerifiedPreview | undefined = undefined;
  @Prop loading: boolean = false;
  @Prop failure: string = '';
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
  private sourceWidth: number = 0;
  private sourceHeight: number = 0;
  private gestureScale: number = 1;
  private gestureX: number = 0;
  private gestureY: number = 0;
  private gestureFocalX: number = 0;
  private gestureFocalY: number = 0;
  private epoch: number = 0;
  private alive: boolean = false;

  aboutToAppear(): void { this.alive = true; }
  aboutToDisappear(): void { this.alive = false; this.epoch++; }
  private t(value: string): string { return uiText(value, this.locale); }
  private previewChanged(): void {
    this.epoch++; this.decoded = false; this.decodeFailed = false;
    this.sourceWidth = 0; this.sourceHeight = 0; this.resetView();
  }
  private geometryChanged(): void { this.resetView(); }
  private resetView(): void { this.zoom = 1; this.offsetX = 0; this.offsetY = 0; }
  private fitExtent(horizontal: boolean): number {
    if (this.sourceWidth <= 0 || this.sourceHeight <= 0) { return horizontal ? this.contentWidth : this.contentHeight; }
    const fit = Math.min(this.contentWidth / this.sourceWidth, this.contentHeight / this.sourceHeight);
    return (horizontal ? this.sourceWidth : this.sourceHeight) * fit;
  }
  private moveTo(x: number, y: number): void {
    this.offsetX = imagePreviewOffset(x, this.fitExtent(true), this.contentWidth, this.zoom);
    this.offsetY = imagePreviewOffset(y, this.fitExtent(false), this.contentHeight, this.zoom);
  }
  private setZoom(value: number): void {
    this.zoom = imagePreviewScale(value); this.moveTo(this.offsetX, this.offsetY);
  }
  private completeFor(token: string, epoch: number): (event?: ImageLoadResult) => void {
    return (event?: ImageLoadResult): void => {
      if (!this.alive || this.epoch !== epoch || this.preview?.token !== token || this.decodeFailed || !event) { return; }
      // A successful source read (status 0) is not proof of decoding (status 1).
      if (event.loadingStatus === 1 && Number.isFinite(event.width) && event.width > 0 &&
        Number.isFinite(event.height) && event.height > 0) {
        this.sourceWidth = event.width; this.sourceHeight = event.height; this.decoded = true;
      }
    };
  }
  private failureFor(token: string, epoch: number): () => void {
    return (): void => {
      if (this.alive && this.epoch === epoch && this.preview?.token === token) {
        this.decodeFailed = true; this.decoded = false; this.resetView();
      }
    };
  }
  private canInteract(): boolean { return this.decoded && !this.decodeFailed && !this.loading && this.failure.length === 0; }

  @Builder
  ImageCanvas(value: VerifiedPreview) {
    Stack() {
      if (!this.decodeFailed) {
        Image(value.uri).objectFit(ImageFit.Contain).width('100%').height('100%')
          .scale({ x: this.zoom, y: this.zoom }).translate({ x: this.offsetX, y: this.offsetY })
          .accessibilityText(value.name).id('verified-attachment-image')
          .onComplete(this.completeFor(value.token, this.epoch))
          .onError(this.failureFor(value.token, this.epoch))
      }
      if (this.decodeFailed) {
        Text(this.t('图片无法解码，可另存后打开。')).fontFamily(this.family).fontSize(13).fontColor(this.muted)
          .textAlign(TextAlign.Center).padding(24).id('attachment-image-decode-failure')
      } else if (!this.decoded) {
        LoadingProgress().width(32).height(32).color(this.accent).id('attachment-image-decode-loading')
      }
    }.width(this.contentWidth).height(this.contentHeight).clip(true)
      .gesture(GestureGroup(GestureMode.Parallel,
        PinchGesture({ fingers: 2 })
          .onActionStart((event: GestureEvent): void => {
            this.gestureScale = this.zoom; this.gestureX = this.offsetX; this.gestureY = this.offsetY;
            this.gestureFocalX = event.pinchCenterX - this.contentWidth / 2;
            this.gestureFocalY = event.pinchCenterY - this.contentHeight / 2;
          })
          .onActionUpdate((event: GestureEvent): void => {
            if (!this.canInteract()) { return; }
            this.zoom = imagePreviewScale(this.gestureScale * event.scale);
            const ratio = this.zoom / this.gestureScale;
            this.moveTo(this.gestureX * ratio + this.gestureFocalX * (1 - ratio),
              this.gestureY * ratio + this.gestureFocalY * (1 - ratio));
          }),
        PanGesture({ fingers: 1, direction: PanDirection.All })
          .onActionStart((): void => { this.gestureX = this.offsetX; this.gestureY = this.offsetY; })
          .onActionUpdate((event: GestureEvent): void => {
            if (this.canInteract()) { this.moveTo(this.gestureX + event.offsetX, this.gestureY + event.offsetY); }
          }),
        TapGesture({ count: 2 }).onAction((): void => { if (this.canInteract()) { this.resetView(); } })
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
        // Reconstruct Image for each verified token. ArkUI can otherwise keep a
        // previous decoded image when a later source fails to load.
        ForEach([this.preview], (value: VerifiedPreview): void => { this.ImageCanvas(value); },
          (value: VerifiedPreview): string => value.token)
      }
      Row({ space: 16 }) {
        Button('−', { type: ButtonType.Normal }).fontSize(19).fontColor(this.accent).backgroundColor(Color.Transparent)
          .enabled(this.canInteract() && this.zoom > 0.8).onClick((): void => { this.setZoom(this.zoom - 0.25); })
          .accessibilityText('Zoom out').id('attachment-image-zoom-out')
        Button(Math.round(this.zoom * 100).toString() + '%', { type: ButtonType.Normal }).fontSize(12).fontColor(this.ink)
          .backgroundColor(Color.Transparent).enabled(this.canInteract()).onClick((): void => { this.resetView(); })
          .accessibilityText('Reset zoom').id('attachment-image-zoom-reset')
        Button('+', { type: ButtonType.Normal }).fontSize(19).fontColor(this.accent).backgroundColor(Color.Transparent)
          .enabled(this.canInteract() && this.zoom < 2.5).onClick((): void => { this.setZoom(this.zoom + 0.25); })
          .accessibilityText('Zoom in').id('attachment-image-zoom-in')
      }.height(32)
    }.width(this.contentWidth)
  }
}

interface ImageLoadResult {
  width: number;
  height: number;
  loadingStatus: number;
}
