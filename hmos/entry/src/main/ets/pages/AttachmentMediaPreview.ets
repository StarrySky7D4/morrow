import { media } from '@kit.MediaKit';
import { fileIo as fs } from '@kit.CoreFileKit';
import { BusinessError } from '@kit.BasicServicesKit';
import { AttachmentPlayer, PlaybackSource, PlaybackView, playbackCanControl, playbackTime } from '../model/AttachmentPlayback';
import { AttachmentMediaGestures, AttachmentDoubleTapSeek, AttachmentControlVisibility, MediaGestureAction } from '../model/AttachmentMediaGestures';
import { uiText } from '../model/UiStrings';

// Only the parent can admit a verified lease. AVPlayer receives a read-only
// descriptor for that private file, never a provider or network URL.
class PlatformAttachmentPlayer implements AttachmentPlayer {
  private source: PlaybackSource;
  private file: fs.File | undefined = undefined;
  private player: media.AVPlayer | undefined = undefined;
  private playerReleased: boolean = false;
  private state: (value: string) => void = (): void => {};
  private position: (value: number) => void = (): void => {};
  private duration: (value: number) => void = (): void => {};
  private failure: (value: string) => void = (): void => {};
  constructor(source: PlaybackSource) { this.source = source; }
  onState(callback: (value: string) => void): void { this.state = callback; }
  onPosition(callback: (value: number) => void): void { this.position = callback; }
  onDuration(callback: (value: number) => void): void { this.duration = callback; }
  onError(callback: (value: string) => void): void { this.failure = callback; }
  async load(): Promise<void> {
    if (this.file || this.player) { throw new Error('媒体会话已打开'); }
    this.file = await fs.open(this.source.path, fs.OpenMode.READ_ONLY | fs.OpenMode.NOFOLLOW);
    const info = await fs.stat(this.file.fd);
    if (!info.isFile() || info.size !== this.source.byteLength) { throw new Error('媒体文件长度不一致'); }
    const player = await media.createAVPlayer(); this.player = player;
    player.on('stateChange', (state: string): void => {
      if (state === 'prepared') {
        if (this.source.kind === 'video') { player.videoScaleType = media.VideoScaleType.VIDEO_SCALE_TYPE_SCALED_ASPECT; }
        this.duration(player.duration); this.position(player.currentTime);
      }
      this.state(state);
    });
    player.on('timeUpdate', (value: number): void => this.position(value));
    player.on('durationUpdate', (value: number): void => this.duration(value));
    player.on('error', (_error: BusinessError): void => this.failure('媒体无法播放，可另存后打开。'));
    player.fdSrc = { fd: this.file.fd, offset: 0, length: this.source.byteLength };
  }
  setSurface(id: string): void { if (this.player) { this.player.surfaceId = id; } }
  async prepare(): Promise<void> { await this.player?.prepare(); }
  async play(): Promise<void> { await this.player?.play(); }
  async pause(): Promise<void> { await this.player?.pause(); }
  seek(value: number): void { this.player?.seek(value, media.SeekMode.SEEK_CLOSEST); }
  async release(): Promise<void> {
    // Do not close the descriptor or permit lease deletion after a rejected
    // player release. A later explicit close retries the retained resources.
    if (this.player && !this.playerReleased) {
      await this.player.release(); this.playerReleased = true;
    }
    if (this.file) { await fs.close(this.file); this.file = undefined; }
    this.player = undefined;
  }
}

export async function createAttachmentPlayer(source: PlaybackSource): Promise<AttachmentPlayer> {
  return new PlatformAttachmentPlayer(source);
}

@Component
export struct AttachmentMediaPreview {
  @Prop @Watch('viewChanged') view: PlaybackView = new PlaybackView();
  @Prop loading: boolean = false;
  @Prop failure: string = '';
  @Prop contentWidth: number = 620;
  @Prop contentHeight: number = 320;
  @Prop ink: string = '#302D43';
  @Prop muted: string = '#81798F';
  @Prop accent: string = '#7662BA';
  @Prop family: string = 'HarmonyOS Sans';
  @Prop locale: string = 'zh';
  @Prop @Watch('fullscreenChanged') fullscreen: boolean = false;
  onToggle: (token: string) => void = (_token: string): void => {};
  onSeek: (value: number, token: string) => void = (_value: number, _token: string): void => {};
  onSurface: (token: string, id: string) => void = (_token: string, _id: string): void => {};
  onSurfaceGone: (token: string, id: string) => void = (_token: string, _id: string): void => {};
  onBackground: (token: string) => void = (_token: string): void => {};
  onFullscreen: (token: string) => void = (_token: string): void => {};
  @State seeking: boolean = false;
  @State seekPosition: number = 0;
  @State controlsVisible: boolean = false;
  @State gestureFeedback: MediaGestureAction = new MediaGestureAction();
  private surfaceController: AttachmentSurfaceController = new AttachmentSurfaceController();
  private lastToken: string = '';
  private surfaceId: string = '';
  private surfaceEpoch: number = 0;
  private alive: boolean = false;
  private gestures: AttachmentMediaGestures = new AttachmentMediaGestures();
  private controlsVisibility: AttachmentControlVisibility = new AttachmentControlVisibility((visible: boolean): void => {
    if (this.alive) { this.controlsVisible = visible; }
  });
  private doubleTap: AttachmentDoubleTapSeek = new AttachmentDoubleTapSeek(
    (): PlaybackView => this.view, (): boolean => this.alive && this.fullscreen,
    (action: MediaGestureAction): void => { if (this.alive) { this.gestureFeedback = action; } },
    (action: MediaGestureAction): void => {
      if (this.alive && action.token === this.view.token && this.ready()) { this.onSeek(action.value, action.token); }
    });
  private t(value: string): string { return uiText(value, this.locale); }
  private viewChanged(): void {
    if (this.lastToken !== this.view.token) {
      this.surfaceEpoch++; this.surfaceId = ''; this.lastToken = this.view.token;
      this.bindSurface(this.view.token, this.surfaceEpoch);
      this.cancelGestures(); this.controlsVisibility.bind(this.view.token, this.fullscreen);
    }
    if (this.failed() || this.view.phase === 'closing') { this.cancelGestures(); }
    if (!this.seeking) { this.seekPosition = this.view.positionMs; }
  }
  private fullscreenChanged(): void { this.cancelGestures(); this.controlsVisibility.bind(this.view.token, this.fullscreen); }
  aboutToAppear(): void { this.alive = true; this.viewChanged(); this.controlsVisibility.bind(this.view.token, this.fullscreen); }
  aboutToDisappear(): void {
    this.alive = false; this.controlsVisibility.dispose(); this.cancelGestures();
    if (this.surfaceId) { this.onSurfaceGone(this.lastToken, this.surfaceId); this.surfaceId = ''; }
    this.surfaceEpoch++; this.onBackground(this.view.token);
  }
  private cancelGestures(): void { this.gestures.cancel(); this.doubleTap.cancel(); this.seeking = false; }
  private failed(): boolean { return this.view.phase === 'failed' || this.view.phase === 'cleanup_failed'; }
  private ready(): boolean { return playbackCanControl(this.view) && !this.loading && !this.failure; }
  private positionLabel(): string { return playbackTime(this.seeking ? this.seekPosition : this.view.positionMs); }
  private bindSurface(token: string, epoch: number): void {
    this.surfaceController = new AttachmentSurfaceController(
      (id: string): void => this.admitSurface(token, epoch, id),
      (id: string): void => {
        if (token !== this.view.token || epoch !== this.surfaceEpoch || id !== this.surfaceId) { return; }
        this.surfaceId = ''; this.onSurfaceGone(token, id); this.onBackground(token);
      });
  }
  private admitSurface(token: string, epoch: number, id: string): void {
    if (!this.alive || token !== this.view.token || epoch !== this.surfaceEpoch || !id || this.surfaceId === id) { return; }
    this.surfaceId = id; this.onSurface(token, id);
  }
  private surfaceLoaded(token: string, epoch: number, controller: XComponentController): () => void {
    return (): void => {
      if (!this.alive || token !== this.view.token || epoch !== this.surfaceEpoch) { return; }
      this.admitSurface(token, epoch, controller.getXComponentSurfaceId());
    };
  }
  private gestureX(event: GestureEvent): number {
    const fingers = event.fingerInfos && event.fingerInfos.length > 0 ? event.fingerInfos : event.fingerList;
    return fingers.length > 0 && fingers[0] ? fingers[0].localX : -1;
  }
  private feedbackLabel(): string {
    const delta = this.gestureFeedback.deltaMs;
    return (delta < 0 ? '⏪ −' : '⏩ +') + Math.abs(Math.round(delta / 1000)).toString() + this.t('秒') + '\n' +
      playbackTime(this.gestureFeedback.value) + ' / ' + playbackTime(this.view.durationMs);
  }

  @Builder
  VideoCanvas(value: PlaybackView) {
    XComponent({ id: 'attachment-video-' + value.token, type: XComponentType.SURFACE, controller: this.surfaceController })
      .width('100%').height('100%').backgroundColor(Color.Black).id('attachment-video-surface')
      .onLoad(this.surfaceLoaded(value.token, this.surfaceEpoch, this.surfaceController))
  }

  @Builder
  GestureZone(token: string) {
    Stack() {}.width(Math.max(1, this.contentWidth - 32)).height(Math.max(1, this.contentHeight - 32))
      .backgroundColor(Color.Transparent).id('attachment-media-gesture-zone')
      .gesture(GestureGroup(GestureMode.Exclusive,
        PanGesture({ fingers: 1, direction: PanDirection.Horizontal })
          .onActionStart((): void => {
            if (token !== this.view.token || !this.ready()) { return; }
            this.doubleTap.cancel(); this.controlsVisibility.hold(token); this.gestures.begin(this.view, this.fullscreen);
          })
          .onActionUpdate((event: GestureEvent): void => {
            if (token !== this.view.token || !this.ready()) { return; }
            const action = this.gestures.move(event.offsetX, this.view, this.fullscreen);
            if (action.kind) { this.gestureFeedback = action; this.seeking = true; this.seekPosition = action.value; }
          })
          .onActionEnd((): void => {
            const action = this.gestures.finish(this.view, this.fullscreen);
            this.seeking = false; this.gestureFeedback = new MediaGestureAction(); this.controlsVisibility.release(token);
            if (token === this.view.token && action.token === token && action.kind && this.ready()) { this.onSeek(action.value, token); }
          })
          .onActionCancel((): void => { this.gestures.cancel(); this.seeking = false; this.gestureFeedback = new MediaGestureAction(); this.controlsVisibility.release(token); }),
        TapGesture({ count: 2 }).onAction((event: GestureEvent): void => {
          if (token === this.view.token && this.ready()) {
            this.doubleTap.trigger(this.view, this.fullscreen, this.gestureX(event), Math.max(1, this.contentWidth - 32));
          }
        }),
        TapGesture({ count: 1 }).onAction((event: GestureEvent): void => {
          if (token !== this.view.token || !this.alive || this.failed()) { return; }
          if (!this.doubleTap.increment(this.view, this.fullscreen, this.gestureX(event), Math.max(1, this.contentWidth - 32))) {
            this.controlsVisibility.tap(token);
          }
        })
      ))
  }

  @Builder
  Controls(token: string) {
    Stack() {
      Button(this.view.phase === 'playing' ? 'Ⅱ' : '▷', { type: ButtonType.Normal })
        .fontFamily(this.family).fontSize(this.fullscreen ? 44 : 38).fontColor(Color.White)
        .backgroundColor(Color.Transparent).width(this.fullscreen ? 56 : 48).height(this.fullscreen ? 56 : 48)
        .accessibilityText(this.t(this.view.phase === 'playing' ? '暂停' : '播放')).enabled(this.ready()).id('attachment-media-toggle')
        .onClick((): void => {
          if (token === this.view.token && this.ready()) { this.controlsVisibility.interact(token); this.onToggle(token); }
        })
      Column({ space: 4 }) {
        Slider({ value: this.seeking ? this.seekPosition : this.view.positionMs, min: 0,
          max: Math.max(1, this.view.durationMs), step: 1, style: SliderStyle.OutSet })
          .width('100%').height(28).selectedColor(Color.Red).trackColor('#61FFFFFF')
          .enabled(this.ready() && this.view.durationMs > 0).id('attachment-media-seek')
          .onChange((value: number, mode: SliderChangeMode): void => {
            if (token !== this.view.token || !this.ready() || !Number.isFinite(value)) { return; }
            this.seekPosition = Math.max(0, Math.min(this.view.durationMs, value));
            if (mode === SliderChangeMode.Begin || mode === SliderChangeMode.Moving) {
              this.controlsVisibility.hold(token); this.seeking = true;
            }
            if (mode === SliderChangeMode.End || mode === SliderChangeMode.Click) {
              this.seeking = false; this.controlsVisibility.release(token); this.onSeek(this.seekPosition, token);
            }
          })
        Row({ space: 12 }) {
          Text(this.positionLabel() + ' / ' + playbackTime(this.view.durationMs)).fontFamily(this.family)
            .fontSize(12).fontColor(Color.White).id('attachment-media-time')
          Blank()
          Button(this.t(this.fullscreen ? '退出全屏' : '全屏'), { type: ButtonType.Normal })
            .fontFamily(this.family).fontSize(12).fontColor(Color.White).backgroundColor(Color.Transparent)
            .height(32).enabled(!this.loading && !this.failed() && this.view.phase !== 'closing')
            .onClick((): void => { if (token === this.view.token && this.alive) { this.onFullscreen(token); } })
            .id('attachment-media-fullscreen')
        }.width('100%').height(32)
      }.width('100%').height('100%').justifyContent(FlexAlign.End)
        .padding({ left: 16, right: 16, bottom: this.fullscreen ? 42 : 8 })
        .hitTestBehavior(HitTestMode.Transparent)
    }.width('100%').height('100%').backgroundColor('#66000000')
      .opacity(this.controlsVisible ? 1 : 0).animation({ duration: 300, curve: Curve.EaseInOut })
      .hitTestBehavior(this.controlsVisible ? HitTestMode.Transparent : HitTestMode.None)
      .id('attachment-media-controls')
  }

  build() {
    Stack() {
        if (this.failure.length > 0) {
          Text(this.failure).fontFamily(this.family).fontSize(13).fontColor(this.muted)
            .textAlign(TextAlign.Center).padding(24).id('attachment-media-read-failure')
        } else if (this.failed()) {
          Text(this.t(this.view.error || '媒体无法播放，可另存后打开。')).fontFamily(this.family).fontSize(13)
            .fontColor(this.muted).textAlign(TextAlign.Center).padding(24).id('attachment-media-playback-failure')
        } else {
          if (this.view.kind === 'video' && this.view.token && this.view.phase !== 'closing') {
            ForEach([this.view], (value: PlaybackView): void => { this.VideoCanvas(value); },
              (value: PlaybackView): string => value.token)
          } else if (this.view.kind === 'audio' && this.view.token) {
            Column({ space: 12 }) {
              Text('♫').fontSize(52).fontColor(this.accent)
              Text(this.t('音频预览')).fontSize(13).fontColor(this.muted).fontFamily(this.family)
            }.id('attachment-audio-canvas')
          }
          if (this.loading || ['idle', 'loading', 'closing'].includes(this.view.phase)) {
            LoadingProgress().width(32).height(32).color(this.accent).id('attachment-media-loading')
          }
          this.GestureZone(this.view.token)
          this.Controls(this.view.token)
          if (this.gestureFeedback.kind && this.gestureFeedback.token === this.view.token) {
            Text(this.feedbackLabel()).fontFamily(this.family).fontSize(13).fontColor(Color.White)
              .textAlign(TextAlign.Center).padding({ left: 18, right: 18, top: 12, bottom: 12 })
              .backgroundColor('#88000000').borderRadius(24).hitTestBehavior(HitTestMode.None)
              .id('attachment-media-gesture-feedback')
          }
        }
    }.width(this.contentWidth).height(this.contentHeight).backgroundColor(Color.Black).clip(true)
      .onVisibleAreaChange([0], (visible: boolean, _ratio: number): void => {
        if (!visible) { this.cancelGestures(); this.controlsVisibility.dispose(); this.onBackground(this.view.token); }
        else { this.controlsVisibility.bind(this.view.token, this.fullscreen); }
      })
  }
}

// API 26 identifies the exact destroyed surface. An old destruction callback
// cannot borrow this component's newer surface ID or newer lease controller.
class AttachmentSurfaceController extends XComponentController {
  private created: (id: string) => void;
  private destroyed: (id: string) => void;
  constructor(created: (id: string) => void = (_id: string): void => {},
    destroyed: (id: string) => void = (_id: string): void => {}) {
    super(); this.created = created; this.destroyed = destroyed;
  }
  onSurfaceCreated(id: string): void { this.created(id); }
  onSurfaceDestroyed(id: string): void { this.destroyed(id); }
}
