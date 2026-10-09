import { alpha, mix } from '../model/Appearance';
import { NativeMusicTrack } from '../model/MusicLibrary';
import { MusicRecoveryChoice, MusicSpoolView, MusicUiCallbacks, MusicUiView } from '../model/MusicUi';
import { uiText } from '../model/UiStrings';

class MusicDragTicket {
  trackId: string = '';
  importOperation: string = '';
  sha256: string = '';
  revision: string = '';
  order: string[] = [];
}
class MusicRowArea {
  revision: string = '';
  top: number = 0;
  height: number = 0;
}

// Flutter's fill=false NeumorphicSurface at depth=-.8. Only the inset edge
// belongs to this content; Index owns the palette, radius and outer material.
@Component
struct MusicInsetRelief {
  @Prop @Watch('draw') style: string = 'flat';
  @Prop @Watch('draw') depth: number = 1;
  @Prop @Watch('draw') dark: boolean = false;
  @Prop @Watch('draw') surface: string = '#FFFFFF';
  @Prop @Watch('draw') radius: number = 14;
  private ctx: CanvasRenderingContext2D = new CanvasRenderingContext2D(new RenderingContextSettings(true));
  private resizeTimer: number = -1;
  private shape(inset: number = 0, dx: number = 0, dy: number = 0): Path2D {
    const c = this.ctx, x: number = inset + dx, y: number = inset + dy;
    const w: number = c.width - 2 * inset, h: number = c.height - 2 * inset;
    const radius: number = Number.isFinite(this.radius) ? this.radius : 0;
    const r: number = Math.max(0, Math.min(radius - inset, w / 2, h / 2));
    const path: Path2D = new Path2D();
    path.moveTo(x + r, y); path.lineTo(x + w - r, y);
    path.arcTo(x + w, y, x + w, y + r, r); path.lineTo(x + w, y + h - r);
    path.arcTo(x + w, y + h, x + w - r, y + h, r); path.lineTo(x + r, y + h);
    path.arcTo(x, y + h, x, y + h - r, r); path.lineTo(x, y + r);
    path.arcTo(x, y, x + r, y, r); path.closePath();
    return path;
  }
  private color(value: string, opacity: number): string {
    const rgb: number = parseInt(value.substring(1), 16);
    return `rgba(${(rgb >> 16) & 255},${(rgb >> 8) & 255},${rgb & 255},${opacity})`;
  }
  private stroke(inset: number, color: string, width: number): void {
    const c = this.ctx; c.strokeStyle = color; c.lineWidth = width; c.stroke(this.shape(inset));
  }
  private edge(inset: number, top: string, bottom: string, width: number): void {
    const c = this.ctx;
    const gradient: CanvasGradient = c.createLinearGradient(inset, inset, c.width - inset, c.height - inset);
    gradient.addColorStop(0, top); gradient.addColorStop(.5, 'rgba(0,0,0,0)'); gradient.addColorStop(1, bottom);
    c.strokeStyle = gradient; c.lineWidth = width; c.stroke(this.shape(inset));
  }
  private inset(shade: string, light: string, shift: number, blur: number): void {
    // A blurred complement of each shifted opening produces an inner shadow.
    // The unshifted rounded clip keeps the wall inside the exact child bounds.
    const c = this.ctx, margin: number = blur * 5 + shift + 2;
    c.save(); c.clip(this.shape()); c.filter = `blur(${blur}vp)`;
    for (let index: number = 0; index < 2; index++) {
      const offset: number = index === 0 ? shift : -shift;
      const wall: Path2D = new Path2D();
      wall.rect(-margin, -margin, c.width + 2 * margin, c.height + 2 * margin);
      wall.addPath(this.shape(0, offset, offset));
      c.fillStyle = index === 0 ? shade : light; c.fill(wall, 'evenodd');
    }
    c.restore();
  }
  private draw(): void {
    const c = this.ctx; c.clearRect(0, 0, c.width, c.height);
    if (this.style === 'flat' || !Number.isFinite(c.width) || !Number.isFinite(c.height) || c.width < 8 || c.height < 8 ||
      !Number.isFinite(this.depth) || this.depth <= 0) { return; }
    const strength: number = Math.min(2, this.depth * .8), fade: number = Math.min(1, strength);
    const darkRgb: string = this.dark ? '#000000' : '#37323D';
    const darkEdge: string = this.color(darkRgb, (this.dark ? .72 : .52) * fade);
    const lightEdge: string = this.color('#FFFFFF', (this.dark ? .34 : .82) * fade);
    const shadow: string = this.color('#000000', (this.dark ? .48 : .18) * fade);
    switch (this.style) {
      case 'neumorphism': {
        const surface: string = /^#[0-9a-fA-F]{6}$/.test(this.surface) ? this.surface : (this.dark ? '#292634' : '#FFFFFF');
        const shade: string = this.color(mix(surface, '#000000', this.dark ? .85 : .75), (this.dark ? .52 : .24) * fade);
        const light: string = this.color(mix(surface, '#FFFFFF', this.dark ? .28 : .8), (this.dark ? .20 : .82) * fade);
        this.inset(shade, light, 1.25 * strength, 2); break;
      }
      case 'paper':
        this.stroke(0, this.color(darkRgb, (this.dark ? .4 : .28) * fade), 1);
        this.edge(1, lightEdge, darkEdge, 1); break;
      case 'clay':
        this.inset(shadow, lightEdge, 1.5 * strength, 4);
        this.stroke(1, this.color('#FFFFFF', .3 * fade), 1); break;
      case 'fluent':
        this.stroke(0, this.color(darkRgb, .35 * fade), 1);
        this.edge(1, shadow, lightEdge, .8); break;
      case 'brutalist': {
        const ink: string = this.dark ? '#FFFFFF' : '#211E26';
        this.stroke(0, this.color(ink, fade), 1.8);
        this.edge(1.4, this.color(ink, .45 * fade), 'rgba(0,0,0,0)', 1); break;
      }
      case 'industrial':
        this.stroke(0, darkEdge, 2 * strength);
        this.edge(1.5, shadow, lightEdge, 1.5);
        this.stroke(3, this.color('#FFFFFF', .2 * fade), .7); break;
    }
  }
  private resized(): void {
    if (this.resizeTimer !== -1) { clearTimeout(this.resizeTimer); }
    this.resizeTimer = setTimeout((): void => { this.resizeTimer = -1; this.draw(); }, 16);
  }
  aboutToDisappear(): void {
    if (this.resizeTimer !== -1) { clearTimeout(this.resizeTimer); this.resizeTimer = -1; }
  }
  build() {
    Canvas(this.ctx).width('100%').height('100%').onReady((): void => this.draw())
      .onAreaChange((): void => this.resized()).hitTestBehavior(HitTestMode.None)
  }
}

@Component
export struct MusicPanelContent {
  @Prop @Watch('viewChanged') view: MusicUiView = new MusicUiView();
  callbacks: MusicUiCallbacks = { action: (): void => {}, reorder: (): void => {} };
  @Prop fontFamily: string = '';
  @Prop locale: string = 'zh';
  @Prop ink: string = '#302D43';
  @Prop surface: string = '#FFFFFF';
  @Prop muted: string = '#787181';
  @Prop accent: string = '#AE85AB';
  @Prop line: string = '#24302D43';
  @Prop dark: boolean = false;
  @Prop style: string = 'flat';
  @Prop depth: number = 1;
  @Prop radiusScale: number = 1;
  @State private expanded: boolean = false;
  @State private playlistVisited: boolean = false;
  @State private dragTarget: string = '';
  @State private dragAfter: boolean = false;
  private playlistScroller: Scroller = new Scroller();
  private alive: boolean = false;
  private drag: MusicDragTicket | undefined = undefined;
  private rowAreas: Map<string, MusicRowArea> = new Map<string, MusicRowArea>();
  private measuredOrder: string = '';
  private playlistTop: number = 0;
  private playlistHeight: number = 0;
  private dragY: number = -1;
  private dragTimer: number = -1;

  aboutToAppear(): void { this.alive = true; }
  aboutToDisappear(): void { this.alive = false; this.finishDrag(false); }
  private t(value: string): string { return uiText(value, this.locale); }
  private family(): string { return this.fontFamily || 'HarmonyOS Sans'; }
  private round(base: number): number { return Math.max(0, base * this.radiusScale); }
  private current(): NativeMusicTrack | undefined {
    return this.view.library.tracks.find((track: NativeMusicTrack): boolean => track.track_id === this.view.library.selected_track_id);
  }
  private playing(): boolean {
    const current = this.current();
    return !!current && this.view.playback.trackId === current.track_id && this.view.playback.phase === 'playing';
  }
  private canChange(): boolean {
    return this.alive && this.view.canWrite && this.view.controlsEnabled && !this.view.working && !this.view.library.busy &&
      !this.view.library.unknown && !this.view.library.original_mutation && !this.view.library.import_plan && !this.view.playback.busy;
  }
  private canControl(): boolean {
    return this.alive && this.view.controlsEnabled && !!this.current() && !this.view.working &&
      ((this.view.playback.phase === 'idle' && !this.view.playback.trackId) ||
        (this.view.playback.trackId === this.view.library.selected_track_id && ['prepared', 'playing', 'paused', 'completed'].includes(this.view.playback.phase))) &&
      !this.view.playback.busy && !this.view.playback.background && !this.view.library.busy && !this.view.library.unknown;
  }
  private canSeek(): boolean {
    return this.canControl() && this.view.playback.trackId === this.view.library.selected_track_id &&
      ['prepared', 'playing', 'paused', 'completed'].includes(this.view.playback.phase) && this.duration() > 0;
  }
  private canPause(): boolean {
    // A library read/write can become Unknown while the owned player continues.
    // Stopping that exact player must not depend on another library receipt.
    return this.alive && this.view.playback.phase === 'playing' && !!this.view.playback.token &&
      !this.view.playback.busy && !this.view.playback.background;
  }
  private clock(value: number): string {
    const seconds: number = Number.isFinite(value) ? Math.max(0, Math.floor(value / 1000)) : 0;
    return Math.floor(seconds / 60).toString() + ':' + (seconds % 60).toString().padStart(2, '0');
  }
  private positionMs(): number {
    const value: number = this.view.playback.positionMs;
    return Number.isFinite(value) ? Math.max(0, Math.min(value, this.duration())) : 0;
  }
  private duration(): number {
    const value: number = this.view.playback.durationMs;
    return Number.isFinite(value) ? Math.max(0, value) : 0;
  }
  private status(): string {
    const current = this.current();
    if (this.view.working || this.view.library.busy || this.view.playback.busy) { return this.t('正在加载…'); }
    if (!current) { return this.t('导入喜欢的本地音乐'); }
    const position: number = this.view.library.order.indexOf(current.track_id) + 1;
    return position.toString() + ' / ' + this.view.library.order.length.toString() + ' · ' + this.t(this.playing() ? '正在播放' : '已暂停');
  }
  private hasCurrentLyrics(): boolean {
    const current = this.current(), lyrics = this.view.library.lyrics;
    return !!current && !!lyrics && lyrics.track_id === current.track_id && !!lyrics.text;
  }
  private allowed(action: string, trackId: string = '', value: number = 0, spoolId: string = ''): boolean {
    if (!this.alive) { return false; }
    if (action === 'add') { return this.canChange() && this.view.library.records.length < 512; }
    if (['select', 'remove', 'move'].includes(action)) {
      const index: number = this.view.library.order.indexOf(trackId);
      return this.canChange() && index >= 0 && !!this.view.library.tracks.find((track: NativeMusicTrack): boolean => track.track_id === trackId) &&
        (action !== 'move' || ((value === -1 || value === 1) && index + value >= 0 && index + value < this.view.library.order.length));
    }
    if (action === 'toggle') { return this.canPause() || this.canControl(); }
    if (['previous', 'next'].includes(action)) { return this.canControl(); }
    if (action === 'seek') { return this.canSeek() && Number.isFinite(value); }
    if (['lyrics_toggle', 'lyrics_import'].includes(action)) { return this.canChange() && !!this.current(); }
    if (action === 'lyrics_view') {
      return !this.view.working && !this.view.library.busy &&
        (trackId ? trackId === this.view.unsavedLyricsTrackId || this.view.library.records.some((track: NativeMusicTrack): boolean => track.track_id === trackId) : !!this.current());
    }
    if (action === 'read_retry') { return this.view.canRetryRead; }
    if (action === 'mutation_retry') { return this.view.canRetryMutation; }
    if (action === 'mutation_discard_known') { return this.view.canDiscardKnownMutation; }
    if (action === 'begin_retry') { return this.view.canRetryBegin; }
    if (action === 'import_inspect') { return this.view.canInspectImport; }
    if (action === 'import_continue') { return this.view.canContinueImport; }
    if (action === 'import_reconcile') { return this.view.canReconcileImport; }
    if (action === 'import_finish') { return this.view.canFinishImport; }
    if (action === 'spool_start' || action === 'spool_resume') {
      const spool = this.view.spools.find((entry: MusicSpoolView): boolean => entry.spoolId === spoolId);
      return this.canChange() && !!spool && (action === 'spool_start' ? spool.canStart :
        (spool.originalSaved ? trackId === '' : spool.choices.some((choice: MusicRecoveryChoice): boolean => choice.trackId === trackId)));
    }
    if (action === 'retry_close') { return !this.view.working && !this.view.playback.busy && this.view.playback.phase === 'cleanup_failed'; }
    if (action === 'retry_io' || action === 'cleanup_cache') { return !this.view.working && !this.view.library.busy && this.view.diagnostics.length > 0; }
    return false;
  }
  private issue(action: string, trackId: string = '', value: number = 0, spoolId: string = ''): void {
    if (!this.allowed(action, trackId, value, spoolId)) { return; }
    this.callbacks.action(action, trackId, action === 'seek' ? Math.round(Math.max(0, Math.min(value, this.duration()))) : value, spoolId);
  }
  private togglePlaylist(): void {
    this.expanded = !this.expanded; this.playlistVisited = true;
    if (!this.expanded) { this.finishDrag(false); }
  }
  private ticket(track: NativeMusicTrack): MusicDragTicket {
    const result = new MusicDragTicket(); result.trackId = track.track_id; result.importOperation = track.import_operation;
    result.sha256 = track.sha256; result.revision = this.view.library.library_revision; result.order = this.view.library.order.slice(); return result;
  }
  private owns(ticket: MusicDragTicket): boolean {
    const track = this.view.library.tracks.find((entry: NativeMusicTrack): boolean => entry.track_id === ticket.trackId);
    return this.canChange() && this.expanded && ticket.revision === this.view.library.library_revision && !!track &&
      ticket.importOperation === track.import_operation && ticket.sha256 === track.sha256 &&
      JSON.stringify(ticket.order) === JSON.stringify(this.view.library.order);
  }
  private viewChanged(): void {
    if (this.drag && !this.owns(this.drag)) { this.finishDrag(false); }
    const order: string = JSON.stringify(this.view.library.order);
    // Lyrics/visibility CAS can change a revision without moving any fixed-height
    // row. Preserve those measured positions, but discard geometry after reorder.
    if (this.measuredOrder && this.measuredOrder === order) {
      this.rowAreas.forEach((area: MusicRowArea): void => { area.revision = this.view.library.library_revision; });
    } else { this.rowAreas.clear(); }
    this.measuredOrder = order;
  }
  private beginDrag(ticket: MusicDragTicket): void {
    this.finishDrag(false); if (!this.owns(ticket)) { return; }
    this.drag = ticket; this.dragTimer = setInterval((): void => this.scrollDrag(), 16);
  }
  private eventY(event: GestureEvent): number {
    if (event.fingerList.length !== 1) { return -1; }
    const y: number = event.fingerList[0].globalY; return Number.isFinite(y) ? y : -1;
  }
  private dragMoved(event: GestureEvent): void {
    if (!this.drag || !this.owns(this.drag)) { this.finishDrag(false); return; }
    this.dragY = this.eventY(event); this.findTarget();
  }
  private findTarget(): void {
    this.dragTarget = ''; this.dragAfter = false;
    if (!this.drag || this.dragY < this.playlistTop || this.dragY > this.playlistTop + this.playlistHeight) { return; }
    for (const id of this.view.library.order) {
      const area = this.rowAreas.get(id);
      if (id !== this.drag.trackId && area && area.revision === this.drag.revision && this.dragY >= area.top && this.dragY <= area.top + area.height) {
        this.dragTarget = id; this.dragAfter = this.dragY >= area.top + area.height / 2; return;
      }
    }
  }
  private scrollDrag(): void {
    if (!this.drag || !this.owns(this.drag)) { this.finishDrag(false); return; }
    if (this.dragY < this.playlistTop - 32 || this.dragY > this.playlistTop + this.playlistHeight + 32 || this.playlistHeight <= 0) { return; }
    const delta: number = this.dragY < this.playlistTop + Math.min(64, this.playlistHeight / 3) ? -12 :
      this.dragY > this.playlistTop + this.playlistHeight - Math.min(64, this.playlistHeight / 3) ? 12 : 0;
    if (delta !== 0) { try { this.playlistScroller.scrollBy(0, delta); this.findTarget(); } catch (_) { this.finishDrag(false); } }
  }
  private finishDrag(commit: boolean): void {
    const ticket = this.drag, target: string = this.dragTarget, after: boolean = this.dragAfter;
    const accepted: boolean = commit && !!ticket && this.owns(ticket) && !!target && this.view.library.order.includes(target);
    this.drag = undefined; this.dragTarget = ''; this.dragAfter = false; this.dragY = -1;
    if (this.dragTimer !== -1) { clearInterval(this.dragTimer); this.dragTimer = -1; }
    if (!accepted || !ticket) { return; }
    const order: string[] = this.view.library.order.filter((id: string): boolean => id !== ticket.trackId);
    order.splice(order.indexOf(target) + (after ? 1 : 0), 0, ticket.trackId);
    if (JSON.stringify(order) !== JSON.stringify(this.view.library.order)) { this.callbacks.reorder(order); }
  }
  private rowArea(trackId: string, area: Area): void {
    this.measuredOrder = JSON.stringify(this.view.library.order);
    if (typeof area.globalPosition.y !== 'number' || typeof area.height !== 'number' || !Number.isFinite(area.globalPosition.y) || !Number.isFinite(area.height) || area.height <= 0) {
      this.rowAreas.delete(trackId); return;
    }
    const value = new MusicRowArea(); value.revision = this.view.library.library_revision; value.top = area.globalPosition.y; value.height = area.height; this.rowAreas.set(trackId, value);
  }
  private listArea(area: Area): void {
    this.playlistTop = typeof area.globalPosition.y === 'number' && Number.isFinite(area.globalPosition.y) ? area.globalPosition.y : 0;
    this.playlistHeight = typeof area.height === 'number' && Number.isFinite(area.height) ? Math.max(0, area.height) : 0;
  }

  @Builder
  private glyph(code: number, size: number, color: string) {
    Text(String.fromCodePoint(code)).fontFamily('MorrowIcons').fontSize(size).fontColor(color)
  }
  @Builder
  private iconAction(code: number, description: string, action: string, size: number = 22, trackId: string = '', value: number = 0) {
    Button({ type: ButtonType.Normal }) { this.glyph(code, size, this.muted) }
      .id('music-' + action + (trackId ? '-' + trackId : '')).width(40).height(40).padding(0).backgroundColor(Color.Transparent)
      .borderRadius(this.round(12)).enabled(this.allowed(action, trackId, value)).accessibilityText(this.t(description))
      .onClick((): void => this.issue(action, trackId, value))
  }
  @Builder
  private recoveryAction(label: string, action: string, trackId: string = '', spoolId: string = '') {
    Button(this.t(label), { type: ButtonType.Normal }).fontFamily(this.family()).fontSize(10).fontColor(this.accent)
      .padding({ left: 8, right: 8, top: 8, bottom: 8 }).backgroundColor(Color.Transparent).borderRadius(this.round(10))
      .enabled(this.allowed(action, trackId, 0, spoolId)).onClick((): void => this.issue(action, trackId, 0, spoolId))
  }
  @Builder
  private dragHandle(track: NativeMusicTrack, captured: MusicDragTicket) {
    Row() { this.glyph(0xf6e3, 19, this.muted) }.width(36).height(40).justifyContent(FlexAlign.Center)
      .opacity(this.canChange() ? 1 : .3).accessibilityText(this.t('拖拽排序'))
      .gesture(GestureGroup(GestureMode.Sequence,
        LongPressGesture({ fingers: 1, repeat: false, duration: 380 }).onAction((): void => this.beginDrag(captured)),
        PanGesture({ fingers: 1, direction: PanDirection.All }).onActionUpdate((event: GestureEvent): void => this.dragMoved(event))
          .onActionEnd((event: GestureEvent): void => { this.dragMoved(event); this.finishDrag(true); })
      ).onCancel((): void => this.finishDrag(false)))
  }
  @Builder
  private trackRow(track: NativeMusicTrack, index: number) {
    Stack() {
      Row({ space: 4 }) {
        this.dragHandle(track, this.ticket(track))
        Text((index + 1).toString().padStart(2, '0')).fontFamily(this.family()).fontSize(9).fontColor(this.muted).width(18)
        Text(track.title || track.name).fontFamily(this.family()).fontSize(10).fontColor(track.track_id === this.view.library.selected_track_id ? this.accent : this.ink)
          .maxLines(1).textOverflow({ overflow: TextOverflow.Ellipsis }).layoutWeight(1).padding({ top: 10, bottom: 10 })
          .onClick((): void => this.issue('select', track.track_id))
          .bindMenu([
            { value: this.t('上移'), enabled: this.allowed('move', track.track_id, -1), action: (): void => this.issue('move', track.track_id, -1) },
            { value: this.t('下移'), enabled: this.allowed('move', track.track_id, 1), action: (): void => this.issue('move', track.track_id, 1) },
            { value: this.t('移出播放列表'), enabled: this.allowed('remove', track.track_id), action: (): void => this.issue('remove', track.track_id) }
          ])
        this.iconAction(0xf647, '移出播放列表', 'remove', 14, track.track_id)
      }.width('100%').padding({ left: 8, right: 2 }).borderRadius(this.round(10)).backgroundColor(Color.Transparent)
      if (track.track_id === this.view.library.selected_track_id) {
        MusicInsetRelief({ style: this.style, depth: this.depth, dark: this.dark, surface: this.surface, radius: this.round(10) })
      }
      if (this.dragTarget === track.track_id) {
        Divider().color(this.accent).strokeWidth(2).position({ left: 8, right: 8, top: this.dragAfter ? undefined : 0, bottom: this.dragAfter ? 0 : undefined })
          .hitTestBehavior(HitTestMode.None)
      }
    }.width('100%').height(40).onAreaChange((_old: Area, next: Area): void => this.rowArea(track.track_id, next))
  }
  @Builder
  private playlist() {
    Column({ space: 4 }) {
      Divider().color(this.line).margin({ top: 12, bottom: 12 })
      if (!this.view.library.tracks.length) { Text(this.t('播放列表还是空的')).fontFamily(this.family()).fontSize(10).fontColor(this.muted).padding({ bottom: 8 }) }
      List({ scroller: this.playlistScroller }) {
        ForEach(this.view.library.tracks, (track: NativeMusicTrack, index: number): void => {
          ListItem() { this.trackRow(track, index) }
        }, (track: NativeMusicTrack): string => track.track_id + ':' + track.import_operation)
      }.width('100%').height(Math.min(240, this.view.library.tracks.length * 40)).onAreaChange((_old: Area, next: Area): void => this.listArea(next))
        .onScroll((): void => this.findTarget()).scrollBar(BarState.Auto).enableScrollInteraction(!this.drag)
      if (this.current()) {
        Row({ space: 4 }) {
          this.recoveryAction('查看歌词', 'lyrics_view')
          this.recoveryAction('导入歌词', 'lyrics_import')
        }.width('100%')
        Text(this.t(this.hasCurrentLyrics() ? (this.view.library.lyrics?.source || '歌词文件') : '尚未读取到歌词'))
          .fontFamily(this.family()).fontSize(10).fontColor(this.muted).width('100%')
      }
    }.width('100%').alignItems(HorizontalAlign.Start)
      .height(this.expanded ? undefined : 0).opacity(this.expanded ? 1 : 0).clip(true).hitTestBehavior(this.expanded ? HitTestMode.Default : HitTestMode.None)
  }
  @Builder
  private recovery() {
    Column({ space: 4 }) {
      if (this.view.canRetryRead) { this.recoveryAction('重新核对曲库', 'read_retry') }
      if (this.view.canRetryMutation) { this.recoveryAction('核对原写入', 'mutation_retry') }
      if (this.view.canDiscardKnownMutation) { this.recoveryAction('取消未写入的更改', 'mutation_discard_known') }
      if (this.view.canRetryBegin) { this.recoveryAction('核对原导入登记', 'begin_retry') }
      if (this.view.canInspectImport) { this.recoveryAction('核对原导入', 'import_inspect') }
      if (this.view.canContinueImport) { this.recoveryAction('继续原导入', 'import_continue') }
      if (this.view.canReconcileImport) { this.recoveryAction('核对已保留的音乐', 'import_reconcile') }
      if (this.view.canFinishImport) { this.recoveryAction('完成原导入', 'import_finish') }
      ForEach(this.view.spools, (spool: MusicSpoolView): void => {
        Column({ space: 4 }) {
          Text(spool.name).fontFamily(this.family()).fontSize(10).fontColor(this.ink).width('100%')
          Text(this.t('保留了完整文件，等待确认导入。')).fontFamily(this.family()).fontSize(9).fontColor(this.muted).width('100%')
          if (spool.originalSaved) { this.recoveryAction('核对原导入', 'spool_resume', '', spool.spoolId) }
          ForEach(spool.choices, (choice: MusicRecoveryChoice): void => {
            this.recoveryAction(this.t('继续原导入') + ' · ' + (choice.title || choice.trackId) + ' · ' + choice.phase, 'spool_resume', choice.trackId, spool.spoolId)
          }, (choice: MusicRecoveryChoice): string => choice.trackId)
          if (spool.canStart) { this.recoveryAction('导入保留的文件', 'spool_start', '', spool.spoolId) }
        }.width('100%').padding({ top: 4, bottom: 4 }).alignItems(HorizontalAlign.Start)
      }, (spool: MusicSpoolView): string => spool.spoolId)
      if (this.view.diagnostics.length > 0) {
        ForEach(this.view.diagnostics, (message: string): void => { Text(message).fontFamily(this.family()).fontSize(9).fontColor(this.muted).width('100%') })
        Row() { this.recoveryAction('重试关闭原文件', 'retry_io'); this.recoveryAction('清理旧播放缓存', 'cleanup_cache') }.width('100%')
      }
      if (this.allowed('retry_close')) { this.recoveryAction('重试关闭播放器', 'retry_close') }
    }.width('100%').alignItems(HorizontalAlign.Start)
  }

  build() {
    Column() {
      Row({ space: 7 }) {
        this.glyph(0xf7bd, 17, this.accent)
        Text(this.t('随身听')).fontFamily(this.family()).fontSize(12).fontWeight(600).fontColor(this.ink).layoutWeight(1)
        this.iconAction(0xf537, '导入音乐', 'add', 18)
      }.width('100%')
      Stack() {
        Row({ space: 10 }) {
          Row() { this.glyph(0xf554, 30, this.accent) }.width(48).height(48).justifyContent(FlexAlign.Center).borderRadius(this.round(12))
            .linearGradient({ colors: [[alpha(this.accent, .4), 0], [alpha(this.ink, .025), 1]], angle: 135 })
          Column({ space: 4 }) {
            Text(this.current()?.title || this.t('留一点空间给音乐')).fontFamily(this.family()).fontSize(11).lineHeight(15.4).fontColor(this.ink)
              .maxLines(2).textOverflow({ overflow: TextOverflow.Ellipsis }).width('100%')
            Text(this.status()).fontFamily(this.family()).fontSize(9).fontColor(this.muted).width('100%')
          }.layoutWeight(1).alignItems(HorizontalAlign.Start)
        }.width('100%').padding(8).borderRadius(this.round(14)).backgroundColor(Color.Transparent)
        MusicInsetRelief({ style: this.style, depth: this.depth, dark: this.dark, surface: this.surface, radius: this.round(14) })
      }.width('100%').height(64).margin({ top: 8 })
      Row() {
        this.iconAction(0xf0193, '上一首', 'previous')
        Button({ type: ButtonType.Normal }) { this.glyph(this.view.playback.phase === 'playing' ? 0xf0056 : 0xf00a0, 24, this.accent) }
          .id('music-toggle').width(40).height(40).padding(0).backgroundColor(alpha(this.accent, .12)).borderRadius(this.round(12))
          .enabled(this.allowed('toggle')).accessibilityText(this.t(this.view.playback.phase === 'playing' ? '暂停音乐' : '播放音乐')).onClick((): void => this.issue('toggle'))
        this.iconAction(0xf0192, '下一首', 'next')
        Button({ type: ButtonType.Normal }) { this.glyph(0xf00d0, 21, this.expanded ? this.accent : this.muted) }
          .id('music-playlist-toggle').width(40).height(40).padding(0).backgroundColor(Color.Transparent).borderRadius(this.round(12))
          .accessibilityText(this.t(this.expanded ? '收起播放列表' : '展开播放列表')).onClick((): void => this.togglePlaylist())
      }.width('100%').justifyContent(FlexAlign.SpaceBetween).margin({ top: 12 })
      if (this.current()) {
        Slider({ value: this.positionMs(), min: 0, max: Math.max(1, this.duration()), style: SliderStyle.OutSet })
          .id('music-seek').width('100%').trackThickness(this.style === 'neumorphism' ? 6 : 2).blockSize({ width: 8, height: 8 })
          .selectedColor(this.accent).trackColor(alpha(this.muted, .2)).blockColor(this.accent)
          .enabled(this.canSeek()).onChange((value: number, mode: SliderChangeMode): void => {
            if (mode === SliderChangeMode.End || mode === SliderChangeMode.Click) { this.issue('seek', '', value); }
          })
        Row() {
          Text(this.clock(this.positionMs())).fontFamily(this.family()).fontSize(9).fontColor(this.muted)
          Blank()
          Text(this.clock(this.duration())).fontFamily(this.family()).fontSize(9).fontColor(this.muted)
        }.width('100%')
        Row() {
          Text(this.t('底部显示歌词')).fontFamily(this.family()).fontSize(10).fontColor(this.muted).layoutWeight(1)
          Toggle({ type: ToggleType.Switch, isOn: this.view.library.show_lyrics }).selectedColor(this.accent).width(36).height(20)
            .id('music-lyrics-toggle').enabled(this.canChange()).onChange((checked: boolean): void => this.issue('lyrics_toggle', '', checked ? 1 : 0))
        }.width('100%').margin({ top: 7 })
        Text(this.t(this.view.library.show_lyrics && !this.hasCurrentLyrics() ? '从播放列表菜单导入 LRC 歌词' : '播放列表与歌词自动保存'))
          .fontFamily(this.family()).fontSize(9).fontColor(this.muted).width('100%').margin({ top: 4 })
      }
      if (this.view.error || this.view.playback.error || this.view.library.error) {
        Text(this.view.error || this.view.playback.error || this.view.library.error).fontFamily(this.family()).fontSize(10).fontColor(this.accent).width('100%').margin({ top: 8 })
      }
      if (this.view.notice) { Text(this.view.notice).fontFamily(this.family()).fontSize(9).fontColor(this.muted).width('100%').margin({ top: 4 }) }
      if (this.view.unsavedLyricsTrackId) {
        Text((this.view.unsavedLyricsTitle || this.view.unsavedLyricsTrackId) + ' · ' + this.t('已读取完整歌词，尚未保存。请查看歌词保留的全文。'))
          .fontFamily(this.family()).fontSize(10).fontColor(this.accent).width('100%').margin({ top: 4 })
        this.recoveryAction('查看未保存的原歌词', 'lyrics_view', this.view.unsavedLyricsTrackId)
      }
      if (this.view.working || this.view.library.busy || this.view.playback.busy) { Progress({ value: 0, type: ProgressType.Linear }).height(2).width('100%').color(this.accent).margin({ top: 6 }) }
      if (this.playlistVisited) { this.playlist() }
      this.recovery()
    }.width('100%').alignItems(HorizontalAlign.Start)
  }
}
