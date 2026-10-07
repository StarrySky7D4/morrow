import { media } from '@kit.MediaKit';
import { fileIo as fs } from '@kit.CoreFileKit';
import { BusinessError } from '@kit.BasicServicesKit';
import { AttachmentPlayer, PlaybackSource, PlaybackView, playbackCanControl, playbackTime } from '../model/AttachmentPlayback';
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
  @Prop fullscreen: boolean = false;
  onToggle: () => void = (): void => {};
  onSeek: (value: number) => void = (_value: number): void => {};
  onSurface: (token: string, id: string) => void = (_token: string, _id: string): void => {};
  onBackground: (token: string) => void = (_token: string): void => {};
  onFullscreen: () => void = (): void => {};
  @State seeking: boolean = false;
  @State seekPosition: number = 0;
  private surfaceController: XComponentController = new XComponentController();
  private lastToken: string = '';
  private t(value: string): string { return uiText(value, this.locale); }
  private viewChanged(): void {
    if (this.lastToken !== this.view.token) { this.lastToken = this.view.token; this.seeking = false; }
    if (!this.seeking) { this.seekPosition = this.view.positionMs; }
  }
  aboutToDisappear(): void { this.onBackground(this.view.token); }
  private failed(): boolean { return this.view.phase === 'failed' || this.view.phase === 'cleanup_failed'; }
  private ready(): boolean { return playbackCanControl(this.view) && !this.loading && !this.failure; }
  private canvasHeight(): number { return Math.max(100, this.contentHeight - 84); }
  private positionLabel(): string { return playbackTime(this.seeking ? this.seekPosition : this.view.positionMs); }

  @Builder
  VideoCanvas(value: PlaybackView) {
    XComponent({ id: 'attachment-video-' + value.token, type: XComponentType.SURFACE, controller: this.surfaceController })
      .width('100%').height('100%').backgroundColor(Color.Black).id('attachment-video-surface')
      .onLoad((): void => { this.onSurface(value.token, this.surfaceController.getXComponentSurfaceId()); })
      .onDestroy((): void => { if (this.view.token === value.token) { this.onBackground(value.token); } })
  }

  build() {
    Column({ space: 8 }) {
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
        }
      }.width(this.contentWidth).height(this.canvasHeight()).clip(true)
      Slider({ value: this.seeking ? this.seekPosition : this.view.positionMs, min: 0,
        max: Math.max(1, this.view.durationMs), step: 1, style: SliderStyle.OutSet })
        .width('100%').height(24).selectedColor(this.accent).trackColor('#DFDAE8')
        .enabled(this.ready() && this.view.durationMs > 0).id('attachment-media-seek')
        .onChange((value: number, mode: SliderChangeMode): void => {
          if (!this.ready() || !Number.isFinite(value)) { return; }
          this.seekPosition = Math.max(0, Math.min(this.view.durationMs, value));
          if (mode === SliderChangeMode.Begin || mode === SliderChangeMode.Moving) { this.seeking = true; }
          if (mode === SliderChangeMode.End || mode === SliderChangeMode.Click) {
            this.seeking = false; this.onSeek(this.seekPosition);
          }
        })
      Row({ space: 12 }) {
        Button(this.t(this.view.phase === 'playing' ? '暂停' : '播放'), { type: ButtonType.Normal })
          .fontFamily(this.family).fontSize(13).fontColor(this.accent).backgroundColor(Color.Transparent)
          .height(32).enabled(this.ready()).onClick((): void => this.onToggle()).id('attachment-media-toggle')
        Text(this.positionLabel() + ' / ' + playbackTime(this.view.durationMs)).fontFamily(this.family)
          .fontSize(12).fontColor(this.ink).id('attachment-media-time')
        Blank()
        Button(this.t(this.fullscreen ? '退出全屏' : '全屏'), { type: ButtonType.Normal })
          .fontFamily(this.family).fontSize(12).fontColor(this.accent).backgroundColor(Color.Transparent)
          .height(32).enabled(!this.loading && !this.failed() && this.view.phase !== 'closing')
          .onClick((): void => this.onFullscreen()).id('attachment-media-fullscreen')
      }.width('100%').height(32)
    }.width(this.contentWidth).height(this.contentHeight)
      .onVisibleAreaChange([0], (visible: boolean, _ratio: number): void => { if (!visible) { this.onBackground(this.view.token); } })
  }
}
