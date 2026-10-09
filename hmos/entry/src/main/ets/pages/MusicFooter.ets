import { MusicUiCallbacks, MusicUiView } from '../model/MusicUi';
import { uiText } from '../model/UiStrings';

@Component
export struct MusicFooterContent {
  @Prop view: MusicUiView = new MusicUiView();
  callbacks: MusicUiCallbacks = { action: (): void => {}, reorder: (): void => {} };
  @Prop fontFamily: string = '';
  @Prop locale: string = 'zh';
  @Prop accent: string = '#AE85AB';
  @Prop muted: string = '#787181';
  @Prop tipText: string = '没有紧迫的事。给好奇心一点时间。';
  private playing(): boolean {
    return this.view.playback.phase === 'playing' && !!this.view.playback.trackId &&
      this.view.playback.trackId === this.view.library.selected_track_id &&
      this.view.library.tracks.some((track): boolean => track.track_id === this.view.playback.trackId);
  }
  private footerText(): string {
    return this.playing() && this.view.library.show_lyrics ? this.view.footerText : uiText(this.tipText, this.locale);
  }
  private canToggle(): boolean {
    return this.playing() && this.view.controlsEnabled && this.view.canWrite && !this.view.working && !this.view.library.busy &&
      !this.view.library.unknown && !this.view.library.original_mutation && !this.view.library.import_plan && !this.view.playback.busy && !this.view.playback.background;
  }
  private toggleLyrics(): void {
    if (this.canToggle()) { this.callbacks.action('lyrics_toggle', '', this.view.library.show_lyrics ? 0 : 1, ''); }
  }
  build() {
    Row({ space: 8 }) {
      if (this.playing()) { Text(String.fromCodePoint(0xf8ed)).fontFamily('MorrowIcons').fontSize(15).fontColor(this.accent) }
      else { Circle().width(6).height(6).fill(this.accent) }
      Text(this.footerText()).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(11).lineHeight(18.7).fontColor(this.muted)
        .maxLines(3).textOverflow({ overflow: TextOverflow.Ellipsis }).layoutWeight(1)
      Button({ type: ButtonType.Normal }) { Text(String.fromCodePoint(0xf06f9)).fontFamily('MorrowIcons').fontSize(17).fontColor(this.view.library.show_lyrics ? this.accent : this.muted) }
        .id('footer-lyrics-toggle').width(32).height(32).padding(0).backgroundColor(Color.Transparent).enabled(this.canToggle())
        .accessibilityText(uiText(this.view.library.show_lyrics ? '底部提示与歌词' : '底部显示歌词', this.locale)).onClick((): void => this.toggleLyrics())
    }.width('100%').padding({ left: 14, right: 14, top: 4, bottom: 4 })
  }
}

@Component
export struct LyricsDialogContent {
  @Prop view: MusicUiView = new MusicUiView();
  callbacks: MusicUiCallbacks = { action: (): void => {}, reorder: (): void => {} };
  @Prop fontFamily: string = '';
  @Prop locale: string = 'zh';
  @Prop ink: string = '#302D43';
  @Prop muted: string = '#787181';
  @Prop accent: string = '#AE85AB';
  private lyricsText(): string {
    return this.view.unsavedLyrics || this.view.lyricsText || uiText('尚未读取到歌词', this.locale);
  }
  private lyricsTitle(): string {
    return (this.view.unsavedLyrics ? this.view.unsavedLyricsTitle : this.view.lyricsTitle) || uiText('查看歌词', this.locale);
  }
  private lyricsSource(): string {
    return this.view.unsavedLyrics ? uiText('已读取完整歌词，尚未保存。', this.locale) : this.view.lyricsSource || uiText('尚未读取到歌词', this.locale);
  }
  private close(): void { this.callbacks.action('lyrics_close', '', 0, ''); }
  build() {
    Column({ space: 8 }) {
      Row() {
        Text(this.lyricsTitle()).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(18).fontWeight(FontWeight.Medium).fontColor(this.ink).layoutWeight(1)
        Button(uiText('关闭', this.locale), { type: ButtonType.Normal }).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(12).fontColor(this.accent)
          .backgroundColor(Color.Transparent).onClick((): void => this.close())
      }.width('100%')
      Text(this.lyricsSource()).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(11).fontColor(this.muted).width('100%')
      if (this.view.unsavedLyrics) { Text(uiText('已读取完整歌词，尚未保存。', this.locale)).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(11).fontColor(this.accent).width('100%') }
      if (!this.view.unsavedLyrics && this.view.lyricsUntimed) { Text(uiText('纯文本歌词', this.locale)).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(11).fontColor(this.muted).width('100%') }
      Scroll() {
        Text(this.lyricsText()).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(14).lineHeight(25.2).fontColor(this.ink)
          .copyOption(CopyOptions.InApp).width('100%')
      }.width('100%').layoutWeight(1).align(Alignment.TopStart).scrollBar(BarState.Auto)
    }.width('100%').height('100%').padding(24).alignItems(HorizontalAlign.Start)
  }
}
