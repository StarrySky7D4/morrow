import { ColorWheel } from './ColorWheel';
import { MarkdownLine, markdownLines } from '../model/Markdown';
import { uiText } from '../model/UiStrings';
import { preferences } from '@kit.ArkData';
import { AppearanceSnapshot, MaterialChoice, alpha, mix, contrast } from '../model/Appearance';
import { AmbientBackdrop, IdeaOrb, GlassRim, PaperBackdrop } from './StudioArtwork';
import { common } from '@kit.AbilityKit';
import { NodeContent } from '@kit.ArkUI';
import { util } from '@kit.ArkTS';
import native from 'libmorrow.so';
import { CardView, Command, Reply, TaskView, workbench } from '../model/Workbench';

@Entry
@Component
struct Index {
  @State cards: CardView[] = [];
  @State section: string = '概览';
  @State search: string = '';
  @State selected: string = '';
  @State title: string = '';
  @State description: string = '';
  @State hypothesis: string = '';
  @State conclusion: string = '';
  @State category: string = '灵感';
  @State taskText: string = '';
  @State dirty: boolean = false;
  @State busy: boolean = false;
  @State ready: boolean = false;
  @State message: string = '';
  @State editorOpen: boolean = false;
  @State settingsOpen: boolean = false;
  @State dark: boolean = false;
  @State glassOpacity: number = 76;
  @State radius: number = 20;
  @State viewportWidth: number = 440;
  @State viewportHeight: number = 676;
  @State contentWidth: number = 416;
  @State filter: string = '全部';
  @State sort: string = '默认顺序';
  @State capture: string = '';
  @State showAppearance: boolean = true;
  @State pending: string = '';
  @State mode: string = '磨砂';
  @State theme: string = '白色';
  @State themeColor: string = '';
  @State toneGray: number = 0;
  @State lightness: number = 88.5;
  @State canvasBackground: string = '默认';
  @State solid: string = '';
  @State canvasLiquid: boolean = false;
  @State canvasBlur: number = 0;
  @State canvasOpacity: number = 0;
  @State fontFamily: string = '';
  @State fontDraft: string = '';
  @State locale: string = 'zh';
  @State daily: string[] = [];
  @State materials: MaterialChoice[] = [];
  @State materialDraft: MaterialChoice = new MaterialChoice();
  @State @Watch('settingsRouteChanged') settingsPage: string = '设置';
  @State colorDraft: string = '#7662BA';
  @State colorTarget: string = 'theme';
  @State showTone: boolean = false;
  @State playlistOpen: boolean = false;
  @State previewBody: boolean = false;
  @State appearanceStatus: string = '';
  private preferenceStore: preferences.Preferences | undefined = undefined;
  private appearanceWrites: Promise<void> = Promise.resolve();
  private settingsScroller: Scroller = new Scroller();
  private settingsRouteChanged(): void { this.settingsScroller.scrollTo({ xOffset: 0, yOffset: 0 }); }
  private preview: NodeContent = new NodeContent();
  private mounted: boolean = false;

  aboutToAppear(): void {
    this.getUIContext().getFont().registerFont({ familyName: 'MorrowIcons', familySrc: $rawfile('materialicons.otf') });
    this.restoreAppearance();
    this.start();
  }
  onBackPress(): boolean {
    if (this.editorOpen) { this.closeEditor(); return true; }
    if (this.settingsOpen) { this.backSettings(); return true; }
    return false;
  }
  aboutToDisappear(): void { this.mounted = false; native.releasePreview(this.preview); }
  private current(): CardView | undefined { return this.cards.find((c: CardView) => c.id === this.selected); }
  private visibleCards(): CardView[] {
    const items = this.cards.filter((c: CardView): boolean => {
      if (this.section === '回收站' ? !c.deleted : c.deleted) { return false; }
      if (this.section === '收藏' && !c.favorite) { return false; }
      if (['灵感', '进行中', '实验'].includes(this.section) && c.category !== this.section) { return false; }
      if (this.filter === '有待办' && !c.tasks.some((t: TaskView) => t.completion !== 1)) { return false; }
      if (['待整理', '已整理', '计划中', '推进中', '已完成', '待验证', '验证中', '已记录'].includes(this.filter) && c.stage !== this.filter) { return false; }
      if (['含附件', '图片', '影音', '文件'].includes(this.filter)) { return false; }
      if (this.filter === '仅收藏' && !c.favorite) { return false; }
      return (c.title + '\n' + c.description).toLowerCase().includes(this.search.toLowerCase());
    });
    if (this.sort === '标题') { items.sort((a: CardView, b: CardView) => a.title.localeCompare(b.title)); }
    if (this.sort === '收藏优先') { items.sort((a: CardView, b: CardView) => Number(b.favorite) - Number(a.favorite)); }
    return items;
  }
  private refreshPreview(): void {
    if (!this.mounted) { return; }
    native.renderPreview(this.preview, this.t(this.mode), '', this.dark);
  }
  private async start(): Promise<void> {
    this.busy = true;
    const cmd = new Command(); cmd.action = 'open';
    const context = this.getUIContext().getHostContext() as common.UIAbilityContext;
    cmd.path = context.filesDir + '/hmos-development.sqlite';
    try {
      let result: Reply = await workbench.send(JSON.stringify(cmd));
      if (!result.ok && result.error === 'AlreadyOpen') { result = await workbench.send(JSON.stringify(new Command())); }
      if (!result.ok) { this.message = result.error; return; }
      this.cards = result.cards; this.ready = true; this.message = ''; 
    } catch (_) { this.message = '打开失败，请重试'; }
    finally { this.busy = false; }
  }
  private select(c: CardView): void {
    if (this.busy || this.pending.length > 0) { return; }
    if (this.dirty) { this.message = '请先保存当前编辑，输入已保留。'; return; }
    this.previewBody = false; this.selected = c.id; this.title = c.title; this.description = c.description;
    this.hypothesis = c.hypothesis; this.conclusion = c.conclusion; this.category = c.category; this.editorOpen = true; this.message = ''; 
    this.refreshPreview();
  }
  private newCard(): void {
    if (this.busy || this.pending.length > 0) { return; }
    if (this.dirty) { this.message = '请先保存当前编辑，输入已保留。'; return; }
    this.selected = ''; this.title = ''; this.description = ''; this.hypothesis = ''; this.conclusion = '';
    this.category = ['灵感', '进行中', '实验'].includes(this.section) ? this.section : '灵感'; this.previewBody = false; this.editorOpen = true; this.message = ''; this.refreshPreview();
  }
  private command(action: string): Command {
    const c = this.current(); const cmd = new Command();
    cmd.action = action; cmd.id = c?.id ?? util.generateRandomUUID(true); cmd.source = c?.source ?? '';
    cmd.operation = util.generateRandomUUID(true); cmd.now_ms = Date.now().toString(); return cmd;
  }
  private async submit(cmd: Command): Promise<void> {
    if (this.busy || this.pending.length > 0) { return; }
    this.pending = JSON.stringify(cmd); await this.retry();
  }
  private async retry(): Promise<void> {
    if (this.busy || this.pending.length === 0) { return; }
    const original = this.pending; const cmd = JSON.parse(original) as Command;
    this.busy = true;
    try {
      const result = await workbench.send(original);
      if (!result.ok) {
        if (result.effect === 'not_committed') {
          this.pending = ''; this.message = `未提交：${result.error}。请修正后再保存，输入已保留。`;
        } else { this.message = `保存未确认：${result.error}。原请求与当前输入均已保留。`; }
        return;
      }
      this.cards = result.cards; this.pending = '';
      if (cmd.action === 'create' || cmd.action === 'edit') {
        this.selected = cmd.id;
        this.dirty = this.title !== cmd.title || this.description !== cmd.description ||
          this.hypothesis !== cmd.hypothesis || this.conclusion !== cmd.conclusion;
      }
      if (cmd.action === 'task_add' && this.taskText === cmd.text) { this.taskText = ''; }
      this.message = result.receipt_revision.length > 0 ? `已保存 · 操作修订 ${result.receipt_revision}` : '已刷新';
      if ((cmd.action === 'create' || cmd.action === 'edit') && !this.dirty) { this.editorOpen = false; }
      this.refreshPreview();
    } catch (_) { this.message = '回复未确认。请用原请求核对，不会创建新的操作。'; }
    finally { this.busy = false; }
  }
  private save(): void {
    if (this.title.trim().length === 0) { this.message = '请输入卡片标题'; return; }
    const cmd = this.command(this.selected.length > 0 ? 'edit' : 'create');
    cmd.title = this.title; cmd.description = this.description; cmd.hypothesis = this.hypothesis;
    cmd.conclusion = this.conclusion; cmd.category = this.category;
    cmd.stage = this.category === '进行中' ? '计划中' : this.category === '实验' ? '待验证' : '待整理';
    this.submit(cmd);
  }
  private mutate(action: string, task?: TaskView): void {
    if (this.dirty) { this.message = '请先保存正文编辑'; return; }
    const c = this.current(); if (!c) { return; }
    if (action === 'restore' && (Date.now() < Number(c.deleted_at) || Date.now() - Number(c.deleted_at) >= 8000)) {
      this.message = '已超过共享核心的 8 秒撤销删除期限，卡片保持删除状态。'; return;
    }
    const cmd = this.command(action);
    if (action === 'favorite') { cmd.flag = !c.favorite; }
    if (task) { cmd.task_id = task.id; cmd.flag = task.completion !== 1; }
    if (action === 'task_add') {
      if (this.taskText.trim().length === 0) { return; }
      cmd.task_id = util.generateRandomUUID(true); cmd.text = this.taskText;
    }
    this.submit(cmd);
  }
  private stages(): SelectOption[] {
    const category = this.current()?.category ?? this.category;
    const labels = category === '进行中' ? ['计划中', '推进中', '已完成'] :
      category === '实验' ? ['待验证', '验证中', '已记录'] : ['待整理', '已整理'];
    return labels.map((s: string): SelectOption => { return { value: s }; });
  }

  private restoreAppearance(): void {
    try {
      const context = this.getUIContext().getHostContext() as common.UIAbilityContext;
      this.preferenceStore = preferences.getPreferencesSync(context, { name: 'studio-appearance' });
      const raw = this.preferenceStore.getSync('appearance-v1', '') as string;
      if (raw.length === 0) { return; }
      const v = JSON.parse(raw) as AppearanceSnapshot;
      this.theme = ['白色', '自定义', '深色'].includes(v.theme) ? v.theme : '白色';
      this.mode = ['磨砂', '超透', '液体玻璃'].includes(v.mode) ? v.mode : '磨砂';
      this.glassOpacity = Math.max(20, Math.min(100, v.opacity)); this.radius = Math.max(0, Math.min(32, v.radius));
      this.themeColor = v.accent; this.toneGray = v.grayscale; this.lightness = v.lightness;
      this.canvasBackground = v.background; this.solid = v.solid; this.canvasLiquid = v.canvasLiquid;
      this.canvasBlur = v.canvasBlur; this.canvasOpacity = v.canvasOpacity;
      this.fontFamily = v.font; this.locale = v.locale; this.daily = v.daily; this.materials = v.materials;
      this.dark = this.theme === '深色' || (this.theme === '自定义' && this.lightness < 46);
    } catch (_) { this.appearanceStatus = '外观设置未能读取，已使用默认值。'; }
  }
  private persistAppearance(): void {
    this.dark = this.theme === '深色' || (this.theme === '自定义' && this.lightness < 46);
    this.refreshPreview();
    const v = new AppearanceSnapshot();
    v.theme = this.theme; v.mode = this.mode; v.opacity = this.glassOpacity; v.radius = this.radius;
    v.accent = this.themeColor; v.grayscale = this.toneGray; v.lightness = this.lightness;
    v.background = this.canvasBackground; v.solid = this.solid; v.canvasLiquid = this.canvasLiquid;
    v.canvasBlur = this.canvasBlur; v.canvasOpacity = this.canvasOpacity;
    v.font = this.fontFamily; v.locale = this.locale; v.daily = this.daily; v.materials = this.materials;
    const serialized = JSON.stringify(v);
    this.appearanceWrites = this.appearanceWrites.then(async () => {
      if (!this.preferenceStore) { return; }
      await this.preferenceStore.put('appearance-v1', serialized); await this.preferenceStore.flush();
      this.appearanceStatus = '';
    }).catch(() => { this.appearanceStatus = '外观尚未保存，请重试。'; });
  }
  private backSettings(): void {
    if (this.settingsPage.startsWith('材质 /')) { this.settingsPage = '组件与卡片'; }
    else if (this.settingsPage === '色彩罗盘' && this.colorTarget === 'material') { this.settingsPage = '材质 / ' + this.materialTitle(this.materialDraft.id); }
    else if (this.settingsPage !== '设置') { this.settingsPage = '设置'; }
    else { this.settingsOpen = false; }
  }
  private openSettings(page: string): void { this.settingsOpen = true; this.settingsPage = page; if (page === '字体设置') { this.fontDraft = this.fontFamily; } }
  private pageFilters(): string[] {
    if (this.section === '灵感') { return ['全部', '待整理', '已整理']; }
    if (this.section === '进行中') { return ['全部', '计划中', '推进中', '已完成']; }
    if (this.section === '实验') { return ['全部', '待验证', '验证中', '已记录']; }
    if (this.section === '收藏') { return ['全部', '图片', '影音', '文件', '文字']; }
    return ['全部', '有待办', '含附件', '仅收藏'];
  }
  private tone(color: string, amount: number = .08): string {
    const tint = this.themeColor.length > 0 ? mix(color, this.themeColor, amount) : color;
    if (this.theme !== '自定义') { return tint; }
    const n = parseInt(tint.substring(1), 16);
    const gray = Math.round(((n >> 16) & 255) * .2126 + ((n >> 8) & 255) * .7152 + (n & 255) * .0722).toString(16).padStart(2, '0');
    return mix(tint, '#' + gray + gray + gray, this.toneGray / 100);
  }
  private baseColor(): string {
    if (this.theme === '自定义') { const c = Math.round(this.lightness * 2.55).toString(16).padStart(2, '0'); return this.tone('#' + c + c + c, .04); }
    return this.tone(this.dark ? '#181720' : '#F9F8FC');
  }
  private canvasBackgroundColorValue(): string { return this.canvasBackground === '纯色' && this.solid.length > 0 ? this.solid : this.baseColor(); }
  private surface(): string { return this.theme === '自定义' ? mix(this.baseColor(), this.dark ? '#FFFFFF' : '#000000', this.dark ? .065 : .015) : this.tone(this.dark ? '#292634' : '#FFFFFF'); }
  private material(id: string): MaterialChoice | undefined { return this.materials.find((m: MaterialChoice) => m.id === id && m.enabled); }
  private modeFor(id: string = ''): string { const m = this.material(id); return m && m.mode !== '跟随主题' ? m.mode : this.mode; }
  private materialBlur(id: string = ''): number { return this.material(id)?.blur ?? (this.modeFor(id) === '磨砂' ? 22 : this.modeFor(id) === '超透' ? 1 : 5); }
  private gradient(id: string = '', readable: boolean = false): LinearGradientOptions {
    const mode = this.modeFor(id); const m = this.material(id); const tint = m?.color || this.surface();
    const opacity = m ? m.opacity / 100 : this.glassOpacity / 100;
    if (m || mode === '磨砂') { return { direction: GradientDirection.RightBottom, colors: [[alpha(tint, opacity), 0], [alpha(tint, opacity), 1]] }; }
    if (mode === '超透') { return { direction: GradientDirection.RightBottom, colors: [[alpha(tint, readable ? .86 : this.dark ? .12 : .1), 0], [alpha(tint, readable ? .8 : .025), 1]] }; }
    return { direction: GradientDirection.RightBottom, colors: [[alpha(tint, readable ? .88 : this.dark ? .4 : .25), 0], [alpha(tint, readable ? .82 : this.dark ? .24 : .09), .5], [alpha(tint, readable ? .88 : this.dark ? .34 : .18), 1]] };
  }
  private beginMaterial(id: string): void {
    const existing = this.materials.find((m: MaterialChoice) => m.id === id);
    const v = existing ? JSON.parse(JSON.stringify(existing)) as MaterialChoice : new MaterialChoice();
    v.id = id; this.materialDraft = v; this.settingsPage = '材质 / ' + this.materialTitle(id);
  }
  private materialTitle(id: string): string {
    if (id.startsWith('card:')) { return this.cards.find((c: CardView) => 'card:' + c.id === id)?.title ?? '卡片'; }
    const ids = ['navigation', 'search', 'hero', 'appearance', 'daily', 'music', 'footer', 'quick', 'summary'];
    const names = ['侧边导航', '搜索框', '首页引导', '空间外观', '此刻的小事', '随身听', '底部提示', '快捷记录', '分区摘要'];
    return names[ids.indexOf(id)] ?? id;
  }
  private applyMaterial(): void {
    this.materials = [...this.materials.filter((m: MaterialChoice) => m.id !== this.materialDraft.id), this.materialDraft];
    this.persistAppearance(); this.settingsPage = '组件与卡片';
  }
  private setMaterialValue(field: string, value: number): void {
    const v = JSON.parse(JSON.stringify(this.materialDraft)) as MaterialChoice;
    if (field === 'blur') { v.blur = value; } else if (field === 'opacity') { v.opacity = value; } else { v.radius = value; }
    this.materialDraft = v;
  }
  private cardAction(c: CardView, action: string, value: string = ''): void {
    const cmd = new Command(); cmd.action = action; cmd.id = c.id; cmd.source = c.source;
    cmd.operation = util.generateRandomUUID(true); cmd.now_ms = Date.now().toString();
    if (action === 'stage') { cmd.stage = value; }
    if (action === 'category') { cmd.category = '进行中'; cmd.stage = '计划中'; }
    this.submit(cmd);
  }
  @Builder
  Rim(id: string = '', base: number = 20) {
    Stack() {
      if (this.modeFor(id) === '液体玻璃') { GlassRim({ dark: this.dark, radius: this.round(base, id) }).hitTestBehavior(HitTestMode.None) }
    }.width('100%').height('100%').hitTestBehavior(HitTestMode.None)
  }

  private ink(): string { return this.tone(this.theme === '自定义' ? (this.dark ? '#FFFFFF' : '#000000') : this.dark ? '#F0EDF8' : '#302D43'); }
  private muted(): string { return this.theme === '自定义' ? mix(this.ink(), this.baseColor(), .12) : this.tone(this.dark ? '#B4AEC5' : '#777184'); }
  private accent(): string {
    if (!this.themeColor) { return this.tone(this.dark ? '#C0AFFA' : '#7662BA', 0); }
    const selected = this.tone(this.themeColor, 0); const base = this.baseColor();
    if (contrast(selected, base) >= 4.5) { return selected; }
    const target = contrast('#FFFFFF', base) > contrast('#000000', base) ? '#FFFFFF' : '#000000';
    let low = 0; let high = 1;
    for (let i = 0; i < 12; i++) { const mid = (low + high) / 2; if (contrast(mix(selected, target, mid), base) >= 4.5) { high = mid; } else { low = mid; } }
    return mix(selected, target, high);
  }
  private glass(): string { return '#00FFFFFF'; }
  private line(): string { return this.dark ? '#26F0EDF8' : '#14302D43'; }
  private round(value: number, id: string = ''): number { return value * (this.material(id)?.radius ?? this.radius) / 20; }
  private label(section: string): string {
    return this.t(section === '灵感' ? '灵感收件箱' : section === '进行中' ? '小项目' : section === '实验' ? '实验室' : section === '收藏' ? '已收藏' : section);
  }
  private icon(section: string): number {
    return section === '概览' ? 0xf7c4 : section === '灵感' ? 0xf126 : section === '进行中' ? 0xf091 : section === '实验' ? 0xf33d : section === '收藏' ? 0xf5cf : 0xf696;
  }
  private navigate(section: string): void { this.section = section; this.filter = '全部'; }
  private closeEditor(): void {
    if (this.busy || this.pending.length > 0) { this.message = '操作尚未确认，当前输入已保留。'; return; }
    if (this.dirty) {
      this.getUIContext().showAlertDialog({ title: '放弃这次修改？', message: '尚未保存的输入将被丢弃。',
        primaryButton: { value: '继续编辑', action: () => {} },
        secondaryButton: { value: '放弃修改', action: () => { this.dirty = false; this.editorOpen = false; } } });
    } else { this.editorOpen = false; }
  }
  private favoriteCard(c: CardView): void {
    const cmd = new Command(); cmd.action = 'favorite'; cmd.id = c.id; cmd.source = c.source;
    cmd.operation = util.generateRandomUUID(true); cmd.now_ms = Date.now().toString(); cmd.flag = !c.favorite;
    this.submit(cmd);
  }
  private captureIdea(): void {
    this.newCard();
    if (this.editorOpen && this.selected.length === 0 && !this.dirty && this.capture.length > 0) {
      this.title = this.capture; this.capture = ''; this.dirty = true;
    }
  }
  @Builder
  Glyph(code: () => number, size: number = 19, color: () => string = () => '#777184') {
    Text(String.fromCodePoint(code())).fontFamily('MorrowIcons').fontSize(size).fontColor(color())
      .width(size).height(size).textAlign(TextAlign.Center).accessibilityLevel('no')
  }
  @Builder
  Navigation() {
    Column() {
      Row({ space: 11 }) {
        Text(this.t('∞')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(28).fontColor('#FFFFFF').textAlign(TextAlign.Center).width(32).height(32)
          .backgroundColor(this.accent()).borderRadius(11)
        Text(this.t('Morrow')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(23).fontWeight(FontWeight.Medium).fontColor(this.ink()).letterSpacing(-1)
      }.width('100%').padding({ left: 12 })
      Text(this.t('杂事有序，奇想自由。')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(10).fontColor(this.muted()).margin({ top: 15, bottom: 36 }).width('100%').padding({ left: 12 })
      Text(this.t('我的空间')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(9).fontColor(this.muted()).width('100%').padding({ left: 12, bottom: 13 })
      ForEach(['概览', '灵感', '进行中', '实验', '收藏', '回收站'], (s: string) => {
        Row({ space: 10 }) {
          this.Glyph(() => this.icon(s),  17, () => this.section === s ? this.accent() : this.muted())
          Text(this.t(this.label(s))).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(12).fontColor(this.section === s ? this.accent() : this.muted())
          Blank()
          if (s === '灵感') { Text(this.cards.filter((c: CardView) => !c.deleted && c.category === '灵感').length.toString()).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(10).fontColor(this.muted()) }
        }.width('100%').padding({ left: 12, right: 12, top: 13, bottom: 13 }).margin({ bottom: 4 })
          .borderRadius(this.round(12)).backgroundColor(this.section === s ? '#217662BA' : Color.Transparent)
          .onClick(() => this.navigate(s)).accessibilityText(this.t(this.label(s)))
      })
      Blank()
      Text(this.t('慢一点，也是在向前。\n给念头一点生长的时间。')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(10).lineHeight(19).fontColor(this.muted())
        .padding(14).width('100%').backgroundColor('#0D7662BA').borderRadius(this.round(13))
      Row({ space: 9 }) {
        Text(this.t('D')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(11).fontColor(this.accent()).width(28).height(28).textAlign(TextAlign.Center).backgroundColor('#147662BA').borderRadius(10)
        Text(this.t('个人工作台')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(11).fontColor(this.ink())
      }.width('100%').margin({ top: 20 })
    }.padding({ left: 16, right: 16, top: 26, bottom: 16 }).height('100%').width(this.viewportWidth >= 1050 ? 208 : 176)
      .backgroundColor(this.glass()).backdropBlur(this.materialBlur('navigation')).linearGradient(this.gradient('navigation')).overlay(this.Rim('navigation', 26)).borderRadius(this.round(26, 'navigation')).border({ width: 1, color: this.line() })
  }
  @Builder
  Header() {
    Row({ space: 8 }) {
      if (this.viewportWidth < 760) {
        Button({ type: ButtonType.Normal }) { this.Glyph(() => 0xf8b6,  21, () => this.muted()) }
          .width(40).height(38).backgroundColor(Color.Transparent).accessibilityText('我的空间')
          .bindMenu([
            { value: this.t('概览'), action: () => this.navigate('概览') },
            { value: this.t('灵感收件箱'), action: () => this.navigate('灵感') },
            { value: this.t('小项目'), action: () => this.navigate('进行中') },
            { value: this.t('实验室'), action: () => this.navigate('实验') },
            { value: this.t('已收藏'), action: () => this.navigate('收藏') },
            { value: this.t('回收站'), action: () => this.navigate('回收站') }
          ])
      } else {
        this.Glyph(() => 0xf7c4,  17, () => this.muted())
        Text('工作台  /  ' + this.label(this.section)).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(11).fontColor(this.muted())
        Blank()
      }
      Row({ space: 8 }) {
        this.Glyph(() => 0xe567,  17, () => this.muted())
        TextInput({ text: this.search, placeholder: this.t('搜索你的奇思妙想…') }).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(11).placeholderFont({ size: 11 }).fontColor(this.ink())
          .placeholderColor(this.muted()).backgroundColor(Color.Transparent).padding(0).height(38).layoutWeight(1)
          .onChange((s: string) => { this.search = s; })
        if (this.search.length === 0) { Text(this.t('Ctrl K')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(9).fontColor(this.muted()) }
        else { Text(this.t('×')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(18).fontColor(this.muted()).onClick(() => { this.search = ''; }) }
      }.padding({ left: 12, right: 12 }).height(38).width(this.viewportWidth >= 760 ? 246 : undefined)
        .layoutWeight(this.viewportWidth >= 760 ? 0 : 1).borderRadius(this.round(12, 'search')).backgroundColor(this.glass())
        .backdropBlur(this.materialBlur('search')).linearGradient(this.gradient('search')).overlay(this.Rim('search', 12)).border({ width: 1, color: this.line() })
      Button({ type: ButtonType.Normal }) { this.Glyph(() => 0xf0258,  19, () => this.muted()) }
        .width(40).height(38).backgroundColor(Color.Transparent).accessibilityText('空间外观')
        .onClick(() => { if (this.viewportWidth >= 1050) { this.showAppearance = !this.showAppearance; } else { this.openSettings('设置'); } })
    }.width('100%').height(40)
  }
  @Builder
  Greeting() {
    Row({ space: 12 }) {
      Column() {
        Text(this.t('A LITTLE SPACE FOR BIG IDEAS')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(9).letterSpacing(2).fontColor(this.accent())
        Text(this.section === '概览' ? this.t('让想法，自由生长。') : this.label(this.section)).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(27).lineHeight(35)
          .fontWeight(FontWeight.Medium).letterSpacing(-.7).fontColor(this.ink()).margin({ top: 9 })
        Text(this.t('收纳日常的零碎，也留住灵光一闪。')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(12).fontColor(this.muted()).margin({ top: 8 })
      }.alignItems(HorizontalAlign.Start).layoutWeight(1)
      Button({ type: ButtonType.Normal }) {
        Row({ space: 7 }) { this.Glyph(() => 0xe047,  17, () => this.dark ? '#302D43' : '#FFFFFF'); Text(this.t('新建灵感')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(11) }
      }.height(46).padding({ left: 15, right: 15 }).borderRadius(this.round(12)).backgroundColor(this.accent())
        .fontColor(this.dark ? '#302D43' : '#FFFFFF').enabled(this.ready && !this.busy).onClick(() => this.newCard())
    }.width('100%')
  }
  @Builder
  Hero() {
    Stack({ alignContent: Alignment.TopStart }) {
      IdeaOrb().position({ right: this.contentWidth < 520 ? -145 : -20, top: -70 }).opacity(this.contentWidth < 520 ? .24 : 1)
      Column() {
        Row({ space: 7 }) {
          Circle().width(5).height(5).fill(this.accent())
          Text(this.t('让可能性发生')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(8).letterSpacing(1.6).fontColor(this.accent())
        }
        Text(this.t('还没成形，也没关系。')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(21).fontWeight(FontWeight.Medium).fontColor(this.ink()).margin({ top: 20 })
        Text(this.t('一个念头、一件小事、一个「万一呢」。\n这里是它们开始的地方。')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(11).lineHeight(20.35).fontColor(this.muted()).margin({ top: 10 })
        Row({ space: 10 }) { Text(this.t('记录此刻的想法')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(11).fontWeight(FontWeight.Medium).fontColor(this.accent()); this.Glyph(() => 0xe09b,  15, () => this.accent()) }
          .padding({ top: 5, bottom: 5 }).margin({ top: 20 }).onClick(() => this.newCard())
      }.alignItems(HorizontalAlign.Start).padding(25).width('100%')
    }.width('100%').height(214).borderRadius(this.round(23, 'hero')).clip(true)
      .backgroundColor(this.glass()).backdropBlur(this.materialBlur('hero')).linearGradient(this.gradient('hero')).overlay(this.Rim('hero', 23)).border({ width: 1, color: this.line() })
  }
  private introText(): string {
    return this.section === '灵感' ? '先接住，再整理。把值得继续的念头转成小项目。' :
      this.section === '进行中' ? '用清单推动进展。每一个完成的小步，都在靠近结果。' :
      this.section === '实验' ? '从一个假设开始，保留尝试、观察和意外发现。' :
      this.section === '收藏' ? '喜欢的文字、图像与文件，集中收在这里。' : '删除后 8 秒内，可在卡片中撤销。';
  }
  private statLabel(completed: boolean): string {
    if (this.section === '灵感') { return completed ? '已经整理' : '等待整理'; }
    if (this.section === '进行中') { return completed ? '已经完成' : '正在推进'; }
    if (this.section === '实验') { return completed ? '已有记录' : '等待验证'; }
    return '收藏记录';
  }
  private statCount(completed: boolean): number {
    return this.cards.filter((c: CardView) => {
      if (c.deleted) { return false; }
      if (this.section === '收藏') { return c.favorite; }
      if (c.category !== this.section) { return false; }
      if (this.section === '实验') { return completed ? c.conclusion.length > 0 : c.stage === '待验证'; }
      const done = c.stage === (this.section === '灵感' ? '已整理' : '已完成');
      return completed ? done : !done;
    }).length;
  }
  @Builder
  PageIntro() {
    Column({ space: 18 }) {
      Row({ space: 16 }) {
        this.Glyph(() => this.icon(this.section),  28, () => this.accent())
        Text(this.t(this.introText())).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(12).lineHeight(21.6).fontColor(this.muted()).layoutWeight(1)
      }.width('100%')
      if (this.section !== '回收站') {
        Row({ space: 24 }) {
          Text(`${this.statLabel(false)}  ${this.statCount(false)}`).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(12).fontColor(this.ink())
          if (this.section !== '收藏') { Text(`${this.statLabel(true)}  ${this.statCount(true)}`).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(12).fontColor(this.ink()) }
        }.width('100%')
      }
    }.alignItems(HorizontalAlign.Start).padding(22).width('100%').backgroundColor(this.glass()).backdropBlur(this.materialBlur('summary')).linearGradient(this.gradient('summary')).overlay(this.Rim('summary', 20))
      .borderRadius(this.round(20, 'summary')).border({ width: 1, color: this.line() })
  }
  @Builder
  CollectionHeader() {
    Flex({ wrap: FlexWrap.Wrap, justifyContent: FlexAlign.Start, alignItems: ItemAlign.Center }) {
      Row({ space: 8 }) {
        Text(this.section === '概览' ? this.t('最近的念头') : this.label(this.section)).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(15).fontWeight(FontWeight.Medium).fontColor(this.ink())
        Text(this.visibleCards().length.toString().padStart(2, '0')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(10).fontColor(this.muted())
      }.margin({ right: 12, bottom: 12 })
      Row({ space: 4 }) {
        ForEach(this.pageFilters(), (f: string) => {
          Text(this.t(f)).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(10).fontColor(this.filter === f ? this.accent() : this.muted())
            .padding({ left: 11, right: 11, top: 7, bottom: 7 }).borderRadius(this.round(8))
            .backgroundColor(this.filter === f ? '#217662BA' : Color.Transparent)
            .onClick(() => { this.filter = f; })
        })
      }.margin({ bottom: 12 })
    }.width('100%')
    Row({ space: 5 }) {
      Blank(); this.Glyph(() => 0xf01a8,  13, () => this.muted()); Text(this.t(this.sort)).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(10).fontColor(this.muted())
    }.padding(6).width('100%').bindMenu([
      { value: this.t('默认顺序'), action: () => { this.sort = '默认顺序'; } },
      { value: this.t('收藏优先'), action: () => { this.sort = '收藏优先'; } },
      { value: this.t('标题'), action: () => { this.sort = '标题'; } }
    ])
  }
  @Builder
  IdeaCard(c: CardView) {
    Column() {
      Row() {
        Row() { this.Glyph(() => this.icon(c.category),  19, () => this.accent()) }.justifyContent(FlexAlign.Center)
          .width(34).height(34).backgroundColor('#267662BA').borderRadius(this.round(11))
        Blank()
        Button({ type: ButtonType.Normal }) { this.Glyph(() => c.favorite ? 0xf5d1 : 0xf5cf,  17, () => c.favorite ? this.accent() : this.muted()) }
          .width(32).height(32).backgroundColor(Color.Transparent).accessibilityText(c.favorite ? '取消收藏' : '收藏')
          .enabled(!this.busy && this.pending.length === 0 && !c.deleted).onClick(() => this.favoriteCard(c))
      }.width('100%')
      Text(c.title).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(14).fontWeight(FontWeight.Medium).fontColor(this.ink()).maxLines(1)
        .textOverflow({ overflow: TextOverflow.Ellipsis }).margin({ top: 14 })
      Text(c.description.length > 0 ? c.description : '给这个念头，留一点生长的空间。').fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(11).lineHeight(19.8).fontColor(this.muted())
        .maxLines(2).textOverflow({ overflow: TextOverflow.Ellipsis }).margin({ top: 8 })
      Row() {
        Text(this.t(c.category)).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(9).fontColor(this.accent()).padding({ left: 7, right: 7, top: 4, bottom: 4 })
          .backgroundColor('#147662BA').borderRadius(this.round(5))
        Blank()
        Text(this.t(c.stage)).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(9).fontColor(this.muted())
      }.width('100%').margin({ top: 19 })
      if (this.section !== '概览' && c.tasks.length > 0) {
        Text(`待办  ${c.tasks.filter((t: TaskView) => t.completion === 1).length} / ${c.tasks.length}`).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(10).fontColor(this.muted()).margin({ top: 12 })
      }
    }.alignItems(HorizontalAlign.Start).padding({ left: 19, right: 13, top: 16, bottom: 17 }).width('100%')
      .backgroundColor(this.glass()).backdropBlur(this.materialBlur('card:' + c.id + '')).linearGradient(this.gradient('card:' + c.id + '')).overlay(this.Rim('card:' + c.id + '', 19)).borderRadius(this.round(19, 'card:' + c.id + '')).border({ width: 1, color: this.line() })
      .shadow({ radius: 18, color: '#09716386', offsetY: 8 }).onClick(() => this.select(c))
  }
  @Builder
  Workspace() {
    Stack({ alignContent: Alignment.Bottom }) {
      Scroll() {
        Column() {
          this.Greeting()
          Column() { if (this.section === '概览') { this.Hero() } else { this.PageIntro() } }.width('100%').margin({ top: 24, bottom: 28 })
          this.CollectionHeader()
          if (this.visibleCards().length === 0) {
            Column({ space: 12 }) {
              this.Glyph(() => 0xf013c,  24, () => this.muted()); Text(this.t('没有找到匹配的想法')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(13).fontColor(this.muted())
              Text(this.t('查看全部')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(11).fontColor(this.accent()).onClick(() => { this.search = ''; this.filter = '全部'; this.section = '概览'; })
            }.width('100%').height(160).justifyContent(FlexAlign.Center).backgroundColor(this.glass()).borderRadius(this.round(20)).margin({ top: 16 })
          } else {
            Flex({ wrap: FlexWrap.Wrap, justifyContent: FlexAlign.SpaceBetween }) {
              ForEach(this.visibleCards(), (c: CardView) => {
                Column() { if (this.section === '概览') { this.IdeaCard(c) } else { this.SpecializedCard(c) } }.width((this.section === '概览' && this.contentWidth >= 460) || (this.section === '进行中' && this.contentWidth >= 660) ? '48.8%' : '100%').margin({ bottom: 14 })
              }, (c: CardView) => c.id + ':' + c.revision)
            }.width('100%').margin({ top: 16 })
          }
          Row({ space: 12 }) {
            this.Glyph(() => 0xf00d,  17, () => this.muted())
            TextInput({ text: this.capture, placeholder: this.t('脑海中闪过了什么？') }).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(11).placeholderFont({ size: 11 }).fontColor(this.ink()).placeholderColor(this.muted())
              .padding(0).height(46).backgroundColor(Color.Transparent).layoutWeight(1).onChange((s: string) => { this.capture = s; }).onSubmit(() => this.captureIdea())
            Button({ type: ButtonType.Normal }) { this.Glyph(() => 0xf57e,  17, () => this.accent()) }
              .width(30).height(30).borderRadius(this.round(10)).backgroundColor('#147662BA').accessibilityText('记录想法').onClick(() => this.captureIdea())
          }.padding({ left: 16, right: 12 }).width('100%').backgroundColor(this.glass()).backdropBlur(this.materialBlur('quick')).linearGradient(this.gradient('quick')).overlay(this.Rim('quick', 15))
            .borderRadius(this.round(15, 'quick')).border({ width: 1, color: this.line() }).margin({ top: 8, bottom: 94 })
        }.width('100%')
      }.scrollBar(BarState.Off).width('100%').height('100%')
      Row({ space: 9 }) {
        Circle().width(6).height(6).fill(this.accent())
        Text(this.t('没有紧迫的事。给好奇心一点时间。')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(11).fontColor(this.muted()).layoutWeight(1)
        this.Glyph(() => 0xf06f9,  17, () => this.muted())
      }.width('100%').height(48).padding({ left: 16, right: 16 }).borderRadius(this.round(15))
        .backgroundColor(this.material('footer') ? alpha(this.material('footer')?.color || this.surface(), (this.material('footer')?.opacity ?? 0) / 100) : Color.Transparent).backdropBlur(this.material('footer')?.blur ?? 0).overlay(this.Rim('footer', 15)).hitTestBehavior(HitTestMode.None)
    }.layoutWeight(1).height('100%').onAreaChange((_, area: Area) => { this.contentWidth = Number(area.width); })
  }
  @Builder
  StageMenu(c: CardView) {
    Select((c.category === '进行中' ? ['计划中', '推进中', '已完成'] : ['待验证', '验证中', '已记录']).map((s: string): SelectOption => { return { value: s }; }))
      .value(c.stage).font({ size: 11 }).fontColor(this.accent()).backgroundColor(Color.Transparent)
      .enabled(!this.busy && this.pending.length === 0).onSelect((_: number, value: string) => this.cardAction(c, 'stage', value))
  }
  @Builder
  Bookmark(c: CardView) {
    Button({ type: ButtonType.Normal }) { this.Glyph(() => c.favorite ? 0xf5d1 : 0xf5cf,  18, () => this.accent()) }
      .width(34).height(34).backgroundColor(Color.Transparent).accessibilityText(c.favorite ? '取消收藏' : '收藏')
      .enabled(!this.busy && this.pending.length === 0).onClick(() => this.favoriteCard(c))
  }
  @Builder
  RecordText(c: CardView) {
    Text(c.title).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(15).fontWeight(FontWeight.Medium).fontColor(this.ink()).maxLines(2).textOverflow({ overflow: TextOverflow.Ellipsis })
  }
  @Builder
  SpecializedCard(c: CardView) {
    Column() {
      if (this.section === '收藏' || this.section === '回收站') {
        Row({ space: 14 }) {
          Row() { this.Glyph(() => 0xf0023,  25, () => this.accent()) }.width(56).height(56).justifyContent(FlexAlign.Center).backgroundColor(alpha(this.accent(), .12)).borderRadius(this.round(12))
          Column({ space: 8 }) {
            this.RecordText(c)
            Text(c.description).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(12).lineHeight(20.4).fontColor(this.muted()).maxLines(3).textOverflow({ overflow: TextOverflow.Ellipsis })
          }.alignItems(HorizontalAlign.Start).layoutWeight(1)
          if (!c.deleted) { this.Bookmark(c) }
        }.width('100%').alignItems(VerticalAlign.Top)
      } else {
        if (this.section === '实验') {
          Row() {
            Text('EXP ' + (this.visibleCards().findIndex((v: CardView) => v.id === c.id) + 1).toString().padStart(2, '0')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(11).letterSpacing(2).fontColor(this.accent())
            Blank(); this.StageMenu(c); this.Bookmark(c)
          }.width('100%').margin({ bottom: 8 })
          this.RecordText(c)
        } else {
          Row({ space: 12 }) { this.Glyph(() => this.icon(c.category),  20, () => this.accent()); Column() { this.RecordText(c) }.alignItems(HorizontalAlign.Start).layoutWeight(1); this.Bookmark(c) }.width('100%')
        }
        Text(c.description).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(12).lineHeight(20.4).fontColor(this.muted()).maxLines(3).textOverflow({ overflow: TextOverflow.Ellipsis }).margin({ top: 8 })
        if (this.section === '灵感') {
          Flex({ wrap: FlexWrap.Wrap }) {
            Button(c.stage === '已整理' ? '✓ 已整理' : '○ 标记已整理', { type: ButtonType.Normal }).fontSize(12).height(36)
              .fontColor(this.accent()).backgroundColor(Color.Transparent).enabled(!this.busy && this.pending.length === 0)
              .onClick(() => this.cardAction(c, 'stage', c.stage === '已整理' ? '待整理' : '已整理')).margin({ right: 8 })
            Button(this.t('→ 转为项目'), { type: ButtonType.Normal }).fontSize(12).height(36).padding({ left: 12, right: 12 })
              .fontColor(this.accent()).backgroundColor(Color.Transparent).border({ width: 1, color: this.line(), radius: this.round(10) })
              .enabled(!this.busy && this.pending.length === 0).onClick(() => this.cardAction(c, 'category'))
          }.width('100%').margin({ top: 12 })
        }
        if (this.section === '进行中') {
          Row() {
            this.StageMenu(c); Blank()
            Text(`${c.tasks.filter((t: TaskView) => t.completion === 1).length} 完成 · ${c.tasks.filter((t: TaskView) => t.completion === 0).length} 未完成 · ${c.tasks.filter((t: TaskView) => t.completion === 2).length} 待确认`).fontFamily(this.fontFamily || 'HarmonyOS Sans')
              .fontSize(11).fontColor(this.muted()).maxLines(2)
          }.width('100%').margin({ top: 12, bottom: 8 })
          Progress({ value: c.tasks.filter((t: TaskView) => t.completion === 1).length, total: Math.max(1, c.tasks.length), type: ProgressType.Linear })
            .color(this.accent()).backgroundColor(this.line()).style({ strokeWidth: 5 }).width('100%')
          if (c.tasks.length === 0) {
            Text(this.t('打开项目，编辑下一步')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(12).fontColor(this.accent()).padding({ top: 20, bottom: 8 }).onClick(() => this.select(c))
          }
          ForEach(c.tasks.slice(0, 3), (task: TaskView) => {
            Row({ space: 8 }) {
              Text(task.completion === 1 ? '✓' : task.completion === 2 ? '?' : '○').fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(16).fontColor(this.accent())
              Text(task.text).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(12).fontColor(this.ink()).maxLines(2).textOverflow({ overflow: TextOverflow.Ellipsis }).layoutWeight(1)
            }.width('100%').padding({ top: 7, bottom: 7 })
          }, (task: TaskView) => task.id + ':' + task.completion)
          if (c.tasks.length > 3) { Text(`还有 ${c.tasks.length - 3} 个步骤`).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(10).fontColor(this.muted()) }
        }
        if (this.section === '实验') {
          Divider().color(this.line()).margin({ top: 14, bottom: 14 })
          Text(this.t('假设 / 想试什么')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(10).fontColor(this.accent())
          Text(c.hypothesis || '打开记录，写下这次想验证的问题。').fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(12).lineHeight(20.4).fontColor(this.ink()).maxLines(3).margin({ top: 6 })
          Text(this.t('观察 / 留下发现')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(10).fontColor(this.accent()).margin({ top: 16 })
          Text(c.conclusion || '结果还未发生，过程也值得记录。').fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(12).lineHeight(20.4).fontColor(this.muted()).maxLines(4).margin({ top: 6 })
        }
      }
    }.alignItems(HorizontalAlign.Start).padding(18).width('100%').borderRadius(this.round(20, 'card:' + c.id))
      .backgroundColor(Color.Transparent).backdropBlur(this.materialBlur('card:' + c.id)).linearGradient(this.gradient('card:' + c.id))
      .border({ width: 1, color: this.line() }).overlay(this.Rim('card:' + c.id)).onClick(() => this.select(c))
  }
  @Builder
  SettingLabel(text: string, value: () => string = () => '') {
    Row() { Text(this.t(text)).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(10).fontColor(this.muted()); Blank(); Text(value()).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(11).fontColor(this.accent()) }.width('100%')
  }
  @Builder
  SettingsSlider(value: () => number, max: number, change: (v: number) => void, min: number = 0) {
    Slider({ value: value(), min: min, max: max, step: 1 }).selectedColor(this.accent()).trackColor(this.line()).blockColor(this.accent())
      .trackThickness(3).blockSize({ width: 12, height: 12 }).height(32).onChange((v: number) => change(v))
  }
  @Builder
  SettingLink(icon: number, title: string, page: string) {
    Row({ space: 12 }) { this.Glyph(() => icon,  20, () => this.ink()); Text(this.t(title)).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(13).fontColor(this.ink()).layoutWeight(1); Text(this.t('›')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(24).fontColor(this.muted()) }
      .width('100%').padding({ top: 14, bottom: 14 }).onClick(() => this.openSettings(page))
  }
  @Builder
  ModeButtons() {
    Row({ space: 8 }) {
      ForEach(['磨砂', '超透', '液体玻璃'], (mode: string) => {
        Column({ space: 5 }) {
          this.Glyph(() => mode === '磨砂' ? 0xf5c9 : mode === '超透' ? 0xf0695 : 0xe374,  15, () => this.mode === mode ? this.accent() : this.muted())
          Text(this.t(mode)).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(11).fontColor(this.mode === mode ? this.accent() : this.muted())
        }.justifyContent(FlexAlign.Center).height(62).layoutWeight(1).borderRadius(this.round(10))
          .backgroundColor(this.mode === mode ? alpha(this.accent(), .14) : alpha(this.surface(), .12))
          .border({ width: 1, color: this.mode === mode ? alpha(this.accent(), .5) : this.line() })
          .onClick(() => { this.mode = mode; this.persistAppearance(); })
      })
    }.width('100%')
  }
  @Builder
  Appearance() {
    Column() {
      Row({ space: 8 }) {
        Row() { this.Glyph(() => 0xf0258,  16, () => this.ink()) }.width(40).height(40).justifyContent(FlexAlign.Center)
        Text(this.t('空间外观')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(13).fontWeight(FontWeight.Medium).fontColor(this.ink()).layoutWeight(1)
        Text(this.t('打造你的风格')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(7).letterSpacing(1).fontColor(this.muted())
        if (!this.settingsOpen) { Text(this.t('⤢')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(18).fontColor(this.muted()).padding(8).onClick(() => this.openSettings('设置')) }
      }.width('100%')
      Column({ space: 4 }) {
        Text(this.t('界面语言')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(10).fontColor(this.muted()).width('100%')
        Select([{ value: '简体中文' }, { value: 'English' }, { value: '日本語' }, { value: '한국어' }, { value: 'Deutsch' }, { value: 'Français' }, { value: 'Español' }, { value: 'Português' }, { value: 'Русский' }])
          .value(this.languageName()).font({ size: 14 }).fontColor(this.ink()).backgroundColor(Color.Transparent).width('100%')
          .onSelect((i: number) => { this.locale = ['zh', 'en', 'ja', 'ko', 'de', 'fr', 'es', 'pt', 'ru'][i]; this.persistAppearance(); })
      }.width('100%').margin({ top: 23 }).padding(8).border({ width: 1, color: this.line(), radius: this.round(12) })
      this.SettingLink(0xf096, '字体', '字体设置')
      Column({ space: 10 }) { this.SettingLabel('玻璃质感'); this.ModeButtons() }.width('100%').margin({ top: 18 })
      if (this.mode === '磨砂') {
        Column() {
          this.SettingLabel('磨砂不透明度', () => `${Math.round(this.glassOpacity)}%`)
          this.SettingsSlider(() => this.glassOpacity, 100, (v: number) => { this.glassOpacity = v; this.persistAppearance(); }, 20)
          Row() { Text(this.t('20% · 轻盈')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(9).fontColor(this.muted()); Blank(); Text(this.t('100% · 纯粹')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(9).fontColor(this.muted()) }.width('100%')
        }.width('100%').margin({ top: 16 })
      }
      Button(this.t('组件与卡片 · 独立设置'), { type: ButtonType.Normal }).width('100%').height(38).fontSize(11).fontColor(this.accent()).backgroundColor(Color.Transparent)
        .border({ width: 1, color: this.line(), radius: this.round(11) }).margin({ top: 16, bottom: 14 }).onClick(() => this.openSettings('组件与卡片'))
      Stack() {
        Circle().width(58).height(58).fill('#C9B4E0').position({ x: 20, y: 13 })
        Circle().width(76).height(76).fill('#D7E5D9').position({ right: 12, bottom: -10 })
        Column() { this.Glyph(() => this.mode === '磨砂' ? 0xf5c9 : this.mode === '超透' ? 0xf0695 : 0xe374,  19, () => this.ink()); ContentSlot(this.preview) }.justifyContent(FlexAlign.Center).width('38%').height(75).borderRadius(this.round(13)).backdropBlur(this.materialBlur()).linearGradient(this.gradient()).overlay(this.Rim())
          .onAppear(() => { this.mounted = true; this.refreshPreview(); }).onDisAppear(() => { this.mounted = false; native.releasePreview(this.preview); })
      }.height(108).width('100%').borderRadius(this.round(14)).clip(true)
        .linearGradient({ direction: GradientDirection.RightBottom, colors: [[this.dark ? '#424260' : '#D9E5DB', 0], [this.dark ? '#736381' : '#D6C3E5', .5], [this.dark ? '#343E45' : '#EDDED6', 1]] })
      Text(this.t(this.mode === '磨砂' ? '柔化背景，让思绪安静地浮现。' : this.mode === '超透' ? '通透轻盈，让光与色彩穿过界面。' : '光沿着边缘流动，折射出轻盈层次。')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(9).fontColor(this.muted()).margin({ top: 10 }).width('100%')
      Column({ space: 12 }) {
        this.SettingLabel('主题色调')
        Row() {
          ForEach(['白色', '自定义', '深色'], (theme: string) => {
            Column({ space: 7 }) {
              Text(this.theme === theme ? '✓' : '').fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(15).fontColor(theme === '深色' ? '#FFFFFF' : '#746095').textAlign(TextAlign.Center)
                .width(40).height(40).backgroundColor(theme === '白色' ? '#FAFAFC' : theme === '自定义' ? '#CFD1DA' : '#302E3E')
                .borderRadius(20).border({ width: 1.5, color: this.theme === theme ? this.accent() : '#22888899' })
              Text(this.t(theme)).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(10).fontColor(this.theme === theme ? this.accent() : this.muted())
            }.layoutWeight(1).onClick(() => { this.theme = theme; this.persistAppearance(); })
          })
        }.width('100%')
        Button(this.t('主题色彩色罗盘'), { type: ButtonType.Normal }).width('100%').height(38).fontSize(11).fontColor(this.accent()).backgroundColor(Color.Transparent)
          .border({ width: 1, color: this.line(), radius: this.round(11) }).onClick(() => { this.colorTarget = 'theme'; this.colorDraft = this.themeColor || '#7662BA'; this.openSettings('色彩罗盘'); })
        Row() {
          Text(this.themeColor ? '全局颜色 · ' + this.themeColor : '默认主题色 · 全局控件').fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(10).fontColor(this.muted()).layoutWeight(1)
          Text(this.t('恢复默认')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(11).fontColor(this.accent()).onClick(() => { this.themeColor = ''; this.persistAppearance(); })
        }.width('100%')
      }.width('100%').margin({ top: 22 })
      if (this.theme === '自定义') {
        Text(this.showTone ? '收起自定义色调  ⌃' : '调整自定义色调  ⌄').fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(12).fontColor(this.accent()).padding({ top: 16, bottom: 12 }).width('100%').onClick(() => { this.showTone = !this.showTone; })
        if (this.showTone) {
          this.SettingLabel('主题灰度', () => `${Math.round(this.toneGray)}%`)
          this.SettingsSlider(() => this.toneGray, 100, (v: number) => { this.toneGray = v; this.persistAppearance(); })
          this.SettingLabel('自定义明暗', () => `${Math.round(this.lightness)}%`)
          this.SettingsSlider(() => this.lightness, 100, (v: number) => { this.lightness = v; this.persistAppearance(); })
        }
      }
      Divider().color(this.line()).margin({ top: 14, bottom: 14 })
      this.SettingLabel('圆角', () => `${Math.round(this.radius)} / 32`)
      this.SettingsSlider(() => this.radius, 32, (v: number) => { this.radius = v; this.persistAppearance(); })
      Text(this.t('拖至 0 即为方角')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(9).fontColor(this.muted()).width('100%')
      Divider().color(this.line()).margin({ top: 12, bottom: 12 })
      this.SettingLabel('背景画布')
      Flex({ wrap: FlexWrap.Wrap, justifyContent: FlexAlign.SpaceBetween }) {
        ForEach(['默认', '纯色', '纹理', '透明'], (bg: string) => {
          Row({ space: 6 }) {
            this.Glyph(() => bg === '默认' ? 0xf7ba : bg === '纹理' ? 0xf7bc : bg === '透明' ? 0xf156 : 0xe3fa,  15, () => this.canvasBackground === bg ? this.accent() : this.muted())
            Text(this.t(bg)).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(11).fontColor(this.canvasBackground === bg ? this.accent() : this.muted())
          }.justifyContent(FlexAlign.Center).width('48%').height(38).margin({ top: 8 }).borderRadius(this.round(10))
            .backgroundColor(this.canvasBackground === bg ? alpha(this.accent(), .14) : Color.Transparent).border({ width: 1, color: this.line() })
            .onClick(() => { this.canvasBackground = bg; this.persistAppearance(); })
        })
      }.width('100%').margin({ top: 2 })
      Row() {
        Column({ space: 4 }) { Text(this.t('液体玻璃效果')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(12).fontColor(this.ink()); Text(this.t('独立于背景类型，四种画布均可开启')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(10).fontColor(this.muted()) }.alignItems(HorizontalAlign.Start).layoutWeight(1)
        Toggle({ type: ToggleType.Switch, isOn: this.canvasLiquid }).selectedColor(this.accent()).onChange((v: boolean) => { this.canvasLiquid = v; this.persistAppearance(); })
      }.width('100%').margin({ top: 16, bottom: 12 })
      if (this.canvasBackground === '纯色') {
        Row() {
          ForEach(['#F9F8FC', '#E5DEF0', '#DDE9E2', '#F0E3D8'], (color: string) => {
            Circle().width(28).height(28).fill(color).stroke(this.solid === color ? this.accent() : this.line()).strokeWidth(1.5).onClick(() => { this.solid = color; this.persistAppearance(); })
          })
        }.justifyContent(FlexAlign.SpaceAround).width('100%')
        Text(this.t('自定义颜色')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(12).fontColor(this.accent()).padding(12).onClick(() => { this.colorTarget = 'solid'; this.colorDraft = this.solid || '#E5DEF0'; this.openSettings('色彩罗盘'); })
      }
      if (this.canvasBackground === '透明') {
        this.SettingLabel('画布模糊', () => `${Math.round(this.canvasBlur)}`)
        this.SettingsSlider(() => this.canvasBlur, 40, (v: number) => { this.canvasBlur = v; this.persistAppearance(); })
        this.SettingLabel('画布不透明度', () => `${Math.round(this.canvasOpacity)}%`)
        this.SettingsSlider(() => this.canvasOpacity, 100, (v: number) => { this.canvasOpacity = v; this.persistAppearance(); })
      }
      if (this.canvasBackground === '纹理') {
        Button(this.t('导入背景图片 / 视频'), { type: ButtonType.Normal }).width('100%').height(36).fontSize(11).fontColor(this.muted()).backgroundColor(Color.Transparent)
          .border({ width: 1, color: this.line(), radius: this.round(10) }).enabled(false)
      }
      Text(this.canvasBackground === '默认' ? '流动的光晕，为灵感留一点色彩。' : this.canvasBackground === '纯色' ? '一张安静的纯色画布。' : this.canvasBackground === '纹理' ? '细密的纸感网点，让空间多一点触感。' : '移动端使用主题底色；透明参数作用于应用画布。').fontFamily(this.fontFamily || 'HarmonyOS Sans')
        .fontSize(9).lineHeight(15).fontColor(this.muted()).margin({ top: 10 }).width('100%')
      Text(this.appearanceStatus || '✓ 外观设置自动保存在本机').fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(9).fontColor(this.muted()).margin({ top: 13 }).width('100%')
    }.alignItems(HorizontalAlign.Start).padding(19).width('100%').borderRadius(this.round(22, 'appearance'))
      .backdropBlur(this.materialBlur('appearance')).linearGradient(this.gradient('appearance')).overlay(this.Rim('appearance', 22)).border({ width: 1, color: this.line() })
  }
  @Builder
  DailyPanel() {
    Column({ space: 12 }) {
      Row({ space: 7 }) { this.Glyph(() => 0xeedd,  17, () => this.accent()); Text(this.t('此刻的小事')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(12).fontWeight(FontWeight.Medium).fontColor(this.ink()) }.width('100%')
      ForEach(['给自己倒一杯水', '把一个想法写下来', '留十分钟，随便探索'], (s: string) => {
        Row({ space: 8 }) {
          Text(this.daily.includes(s) ? '✓' : '○').fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(16).fontColor(this.accent())
          Text(this.t(s)).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(11).fontColor(this.muted()).decoration({ type: this.daily.includes(s) ? TextDecorationType.LineThrough : TextDecorationType.None })
        }.width('100%').padding({ top: 4, bottom: 4 }).onClick(() => { this.daily = this.daily.includes(s) ? this.daily.filter((v: string) => v !== s) : [...this.daily, s]; this.persistAppearance(); })
      })
      Text(this.t('慢一点，也是在向前。')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(9).fontColor(this.muted()).width('100%')
    }.padding(19).width('100%').borderRadius(this.round(22, 'daily')).backdropBlur(this.materialBlur('daily')).linearGradient(this.gradient('daily')).overlay(this.Rim('daily', 22)).border({ width: 1, color: this.line() })
  }
  @Builder
  MusicPanel() {
    Column({ space: 12 }) {
      Row({ space: 7 }) { this.Glyph(() => 0xf7bd,  17, () => this.accent()); Text(this.t('随身听')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(12).fontWeight(FontWeight.Medium).fontColor(this.ink()).layoutWeight(1); Text(this.t('+')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(22).fontColor(this.muted()).opacity(.4) }.width('100%')
      Row({ space: 10 }) {
        Row() { this.Glyph(() => 0xf8ed,  22, () => this.accent()) }.width(48).height(48).justifyContent(FlexAlign.Center).backgroundColor(alpha(this.accent(), .12)).borderRadius(this.round(12))
        Column({ space: 4 }) { Text(this.t('留一点空间给音乐')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(11).fontColor(this.ink()); Text(this.t('本地音乐与歌词正在适配')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(9).fontColor(this.muted()) }.alignItems(HorizontalAlign.Start).layoutWeight(1)
      }.width('100%')
      Row() {
        this.Glyph(() => 0xf0193,  22, () => this.muted())
        Row() { this.Glyph(() => 0xf00a0,  22, () => this.muted()) }.width(40).height(40).justifyContent(FlexAlign.Center).backgroundColor(alpha(this.accent(), .12)).borderRadius(this.round(12))
        this.Glyph(() => 0xf0192,  22, () => this.muted())
        Text(this.t('☷')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(24).fontColor(this.accent()).onClick(() => { this.playlistOpen = !this.playlistOpen; })
      }.width('100%').justifyContent(FlexAlign.SpaceAround)
      if (this.playlistOpen) { Divider().color(this.line()); Text(this.t('播放列表还是空的')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(11).fontColor(this.muted()).padding(16) }
    }.padding(16).width('100%').borderRadius(this.round(22, 'music')).backdropBlur(this.materialBlur('music')).linearGradient(this.gradient('music')).overlay(this.Rim('music', 22)).border({ width: 1, color: this.line() })
  }
  @Builder
  Quote() {
    Column({ space: 10 }) {
      Text(this.t('“')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(42).fontColor(alpha(this.accent(), .4))
      Text(this.t('所有有趣的东西，\n都始于一点点好奇。')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(12).lineHeight(22.8).fontColor(this.muted())
      Text(this.t('保持好奇，做你自己。')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(9).fontColor(this.muted())
    }.alignItems(HorizontalAlign.Start).padding({ left: 17, right: 17, top: 8, bottom: 8 }).width('100%')
  }

  private t(value: string): string { return uiText(value, this.locale); }
  private languageName(): string {
    return ['简体中文', 'English', '日本語', '한국어', 'Deutsch', 'Français', 'Español', 'Português', 'Русский'][['zh', 'en', 'ja', 'ko', 'de', 'fr', 'es', 'pt', 'ru'].indexOf(this.locale)] || '简体中文';
  }
  private systemFonts(): string[] {
    const controller = this.getUIContext().getFont();
    const families = controller.getSystemFontList().map((name: string) => controller.getFontByName(name)?.family || name);
    return Array.from(new Set<string>(['HarmonyOS Sans', ...families]));
  }
  private changeMaterial(field: string, value: string): void {
    const v = JSON.parse(JSON.stringify(this.materialDraft)) as MaterialChoice;
    if (field === 'mode') { v.mode = value; } else if (field === 'enabled') { v.enabled = value === 'true'; } else { v.color = value; }
    this.materialDraft = v;
  }
  private colorBrightness(): number {
    if (!/^#[0-9a-fA-F]{6}$/.test(this.colorDraft)) { return 100; }
    const n = parseInt(this.colorDraft.substring(1), 16); return Math.max(n >> 16 & 255, n >> 8 & 255, n & 255) / 2.55;
  }
  private adjustBrightness(value: number): void {
    if (!/^#[0-9a-fA-F]{6}$/.test(this.colorDraft)) { return; }
    const n = parseInt(this.colorDraft.substring(1), 16); const channels = [n >> 16 & 255, n >> 8 & 255, n & 255];
    const max = Math.max(...channels);
    this.colorDraft = '#' + channels.map((c: number) => Math.round((max === 0 ? 1 : c / max) * value * 2.55).toString(16).padStart(2, '0')).join('');
  }
  @Builder
  MaterialList() {
    Column({ space: 14 }) {
      Text(this.t('为组件或单张卡片设置独立材质；未开启自定义时跟随空间外观。')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(12).lineHeight(21).fontColor(this.muted()).width('100%')
      ForEach(['navigation', 'search', 'hero', 'appearance', 'daily', 'music', 'footer', 'quick', 'summary', ...this.cards.filter((c: CardView) => !c.deleted).map((c: CardView) => 'card:' + c.id)], (id: string) => {
        Row({ space: 12 }) {
          this.Glyph(() => id.startsWith('card:') ? 0xf0023 : 0xf0258,  20, () => this.accent())
          Column({ space: 5 }) {
            Text(this.materialTitle(id)).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(13).fontColor(this.ink()).maxLines(2)
            Text(this.material(id) ? this.modeFor(id) + ' · ' + Math.round(this.material(id)?.opacity ?? 76) + '%' : '跟随主题').fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(10).fontColor(this.muted())
          }.alignItems(HorizontalAlign.Start).layoutWeight(1)
          Text(this.t('›')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(23).fontColor(this.muted())
        }.padding(20).width('100%').borderRadius(16).linearGradient(this.gradient('', true)).border({ width: 1, color: this.line() }).onClick(() => this.beginMaterial(id))
      })
    }.width('100%')
  }
  @Builder
  MaterialEditor() {
    Column({ space: 18 }) {
      Row() {
        Text(this.t('自定义材质')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(14).fontColor(this.ink()).layoutWeight(1)
        Toggle({ type: ToggleType.Switch, isOn: this.materialDraft.enabled }).selectedColor(this.accent()).onChange((v: boolean) => this.changeMaterial('enabled', v.toString()))
      }.width('100%')
      Stack() {
        Circle().width(110).height(110).fill('#CAB5E1').position({ left: 14, top: 3 })
        Circle().width(90).height(90).fill('#C5DCCB').position({ right: 14, bottom: 0 })
        Column({ space: 12 }) { Text(this.t('一个想法，正悄悄发芽。')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(16).fontColor(this.ink()); Text(this.t('给尚未成形的念头一点空间。')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(11).fontColor(this.muted()) }
          .justifyContent(FlexAlign.Center).width('88%').height(110).borderRadius(this.materialDraft.enabled ? this.materialDraft.radius : this.radius)
          .backdropBlur(this.materialDraft.enabled ? this.materialDraft.blur : this.materialBlur())
          .backgroundColor(alpha(this.materialDraft.color || this.surface(), (this.materialDraft.enabled ? this.materialDraft.opacity : this.glassOpacity) / 100))
          .border({ width: 1, color: this.line() })
      }.height(150).width('100%').clip(true)
      Select(['跟随主题', '磨砂', '超透', '液体玻璃'].map((s: string): SelectOption => { return { value: this.t(s) }; }))
        .value(this.t(this.materialDraft.mode)).font({ size: 13 }).fontColor(this.ink()).backgroundColor(Color.Transparent)
        .onSelect((i: number) => this.changeMaterial('mode', ['跟随主题', '磨砂', '超透', '液体玻璃'][i]))
      this.SettingLabel('模糊', () => Math.round(this.materialDraft.blur).toString())
      this.SettingsSlider(() => this.materialDraft.blur, 40, (v: number) => this.setMaterialValue('blur', v))
      this.SettingLabel('不透明度', () => Math.round(this.materialDraft.opacity) + '%')
      this.SettingsSlider(() => this.materialDraft.opacity, 100, (v: number) => this.setMaterialValue('opacity', v))
      this.SettingLabel('圆角', () => Math.round(this.materialDraft.radius).toString())
      this.SettingsSlider(() => this.materialDraft.radius, 32, (v: number) => this.setMaterialValue('radius', v))
      Button(this.t('自定义颜色'), { type: ButtonType.Normal }).fontSize(12).borderRadius(this.round(11)).fontColor(this.accent()).backgroundColor(Color.Transparent).onClick(() => {
        this.colorTarget = 'material'; this.colorDraft = this.materialDraft.color || this.surface(); this.settingsPage = '色彩罗盘';
      })
      Text(this.t('预览中的改动仅在应用后保存。关闭自定义即可重新跟随全局主题。')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(11).lineHeight(19).fontColor(this.muted())
      Flex({ wrap: FlexWrap.Wrap, justifyContent: FlexAlign.End }) {
        Button(this.t('恢复默认'), { type: ButtonType.Normal }).fontSize(12).borderRadius(this.round(11)).backgroundColor(Color.Transparent).fontColor(this.accent()).onClick(() => { const v = new MaterialChoice(); v.id = this.materialDraft.id; this.materialDraft = v; })
        Button(this.t('取消'), { type: ButtonType.Normal }).fontSize(12).borderRadius(this.round(11)).backgroundColor(Color.Transparent).fontColor(this.accent()).onClick(() => this.backSettings())
        Button(this.t('应用'), { type: ButtonType.Normal }).fontSize(12).borderRadius(this.round(11)).backgroundColor(this.accent()).fontColor(this.dark ? '#302D43' : '#FFFFFF').onClick(() => this.applyMaterial())
      }.width('100%')
    }.alignItems(HorizontalAlign.Start).padding(24).width('100%').borderRadius(this.round(20)).linearGradient(this.gradient('', true))
  }
  @Builder
  FontSettings() {
    Column({ space: 20 }) {
      Text(this.t('为你的空间选择字体')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(18).fontColor(this.ink())
      Text('当前字体：' + (this.fontFamily || '系统默认')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(12).borderRadius(this.round(11)).fontColor(this.muted())
      TextInput({ text: this.fontDraft, placeholder: this.t('输入系统字体名称') }).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(14).fontColor(this.ink()).backgroundColor(alpha(this.surface(), .6)).maxLength(128).onChange((v: string) => { this.fontDraft = v; })
      Select(this.systemFonts().map((s: string): SelectOption => { return { value: s }; }))
        .value(this.fontDraft || this.t('选择系统字体')).font({ size: 13 }).fontColor(this.ink()).backgroundColor(Color.Transparent).onSelect((_: number, s: string) => { this.fontDraft = s; })
      Text(this.t('一个想法，正悄悄发芽。\nA little space for big ideas.\n0123456789')).fontFamily(this.fontDraft || 'HarmonyOS Sans').fontSize(22).lineHeight(35).fontColor(this.ink()).width('100%').padding(20).border({ width: 1, color: this.line(), radius: 16 })
      Text(this.t('可选择本机已安装的系统字体；字体文件导入仍在适配。')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(11).fontColor(this.muted()).lineHeight(19)
      Flex({ wrap: FlexWrap.Wrap, justifyContent: FlexAlign.End }) {
        Button(this.t('恢复默认'), { type: ButtonType.Normal }).fontSize(12).borderRadius(this.round(11)).fontColor(this.accent()).backgroundColor(Color.Transparent).onClick(() => { this.fontFamily = ''; this.fontDraft = ''; this.persistAppearance(); })
        Button(this.t('导入字体'), { type: ButtonType.Normal }).fontSize(12).borderRadius(this.round(11)).fontColor(this.muted()).backgroundColor(Color.Transparent).enabled(false)
        Button(this.t('应用字体'), { type: ButtonType.Normal }).fontSize(12).borderRadius(this.round(11)).fontColor(this.dark ? '#302D43' : '#FFFFFF').backgroundColor(this.accent()).onClick(() => {
          if (!this.systemFonts().includes(this.fontDraft)) { this.message = '请选择本机可用的字体'; return; }
          this.fontFamily = this.fontDraft; this.persistAppearance();
        })
      }.width('100%')
    }.alignItems(HorizontalAlign.Start).padding(24).width('100%').borderRadius(this.round(20)).linearGradient(this.gradient('', true))
  }
  @Builder
  ColorSettings() {
    Column({ space: 18 }) {
      Text(this.t('转动色彩，找到属于你的那一种。')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(12).borderRadius(this.round(11)).fontColor(this.muted())
      ColorWheel({ color: this.colorDraft, onPick: (color: string) => { this.colorDraft = color; } })
      this.SettingLabel('明度', () => Math.round(this.colorBrightness()) + '%')
      this.SettingsSlider(() => this.colorBrightness(), 100, (v: number) => this.adjustBrightness(v))
      Row({ space: 12 }) {
        Rect().width(54).height(54).radius(12).fill(/^#[0-9a-fA-F]{6}$/.test(this.colorDraft) ? this.colorDraft : Color.Transparent).stroke(this.line()).strokeWidth(1)
        TextInput({ text: this.colorDraft, placeholder: this.t('#7662BA') }).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(16).fontColor(this.ink()).backgroundColor(Color.Transparent).maxLength(7).layoutWeight(1).onChange((s: string) => { this.colorDraft = s; })
      }.width('100%')
      if (!/^#[0-9a-fA-F]{6}$/.test(this.colorDraft)) { Text(this.t('请输入 # 和六位十六进制颜色')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(11).fontColor(this.accent()) }
      Row({ space: 12 }) {
        Blank()
        Button(this.t('取消'), { type: ButtonType.Normal }).fontSize(12).borderRadius(this.round(11)).fontColor(this.accent()).backgroundColor(Color.Transparent).onClick(() => this.backSettings())
        Button(this.t('应用颜色'), { type: ButtonType.Normal }).fontSize(12).borderRadius(this.round(11)).fontColor(this.dark ? '#302D43' : '#FFFFFF').backgroundColor(this.accent()).enabled(/^#[0-9a-fA-F]{6}$/.test(this.colorDraft)).onClick(() => {
          if (this.colorTarget === 'material') { this.changeMaterial('color', this.colorDraft); }
          else { if (this.colorTarget === 'solid') { this.solid = this.colorDraft; } else { this.themeColor = this.colorDraft; } this.persistAppearance(); }
          this.backSettings();
        })
      }.width('100%')
    }.padding(24).width('100%').constraintSize({ maxWidth: 480 }).borderRadius(this.round(28)).linearGradient(this.gradient('', true))
  }
  @Builder
  Settings() {
    Column({ space: 20 }) {
      Row({ space: 12 }) {
        Button({ type: ButtonType.Normal }) { this.Glyph(() => 0xf572, 24, () => this.ink()) }.width(38).height(38).backgroundColor(Color.Transparent).fontColor(this.muted()).accessibilityText('返回').onClick(() => this.backSettings())
        Text(this.t(this.settingsPage)).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(18).fontColor(this.ink()).fontWeight(FontWeight.Medium)
      }.width('100%')
      Scroll(this.settingsScroller) {
        Column({ space: 20 }) {
          if (this.settingsPage === '设置') {
            this.Appearance()
            this.DailyPanel()
            this.MusicPanel()
            this.Quote()
          } else if (this.settingsPage === '组件与卡片') { this.MaterialList() }
          else if (this.settingsPage.startsWith('材质 /')) { this.MaterialEditor() }
          else if (this.settingsPage === '字体设置') { this.FontSettings() }
          else if (this.settingsPage === '色彩罗盘') { this.ColorSettings() }
        }.padding({ bottom: 24 }).width('100%')
      }.layoutWeight(1).scrollBar(BarState.Off).width('100%').align(Alignment.TopStart)
    }.padding(this.viewportWidth >= 760 ? 24 : 12).width('100%').constraintSize({ maxWidth: 920 }).height('100%')
  }

  @Builder
  BodyPreview() {
    Scroll() {
      Column({ space: 7 }) {
        if (!this.description.trim()) { Text(this.t('写一点内容，再看看它的样子。')).fontSize(13).fontColor(this.muted()) }
        ForEach(markdownLines(this.description), (line: MarkdownLine) => {
          Text(line.text || ' ').fontFamily(line.kind === 'code' ? 'monospace' : this.fontFamily || 'HarmonyOS Sans')
            .fontSize(line.kind === 'heading' ? Math.max(14, 24 - line.level * 2) : 13)
            .fontWeight(line.kind === 'heading' ? FontWeight.Medium : FontWeight.Normal).lineHeight(23)
            .fontColor(line.kind === 'quote' ? this.muted() : this.ink()).width('100%')
            .padding(line.kind === 'code' ? 8 : 0).backgroundColor(line.kind === 'code' ? alpha(this.accent(), .07) : Color.Transparent)
        })
      }.alignItems(HorizontalAlign.Start).width('100%').padding(8)
    }.height(200).width('100%').scrollBar(BarState.Off).align(Alignment.TopStart)
    Text(this.t('预览支持标题、列表、引用与代码块；附件和复杂排版仍在适配。')).fontSize(10).fontColor(this.muted())
  }
  @Builder
  Editor() {
    Column() {
      Row({ space: 12 }) {
        Row() { this.Glyph(() => 0xeea9,  21, () => this.accent()) }.width(38).height(38).justifyContent(FlexAlign.Center)
          .backgroundColor('#267662BA').borderRadius(this.round(13))
        Column({ space: 5 }) {
          Text(this.t('MORROW / A LITTLE POSSIBILITY')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(8).letterSpacing(1).fontColor(this.muted())
          Text(this.selected.length > 0 ? '让想法更清晰' : '接住一个新想法').fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(20).fontWeight(FontWeight.Medium).fontColor(this.ink())
        }.alignItems(HorizontalAlign.Start).layoutWeight(1)
        Button({ type: ButtonType.Normal }) { this.Glyph(() => 0xf647,  18, () => this.muted()) }
          .width(32).height(36).backgroundColor(Color.Transparent).accessibilityText('关闭编辑器').onClick(() => this.closeEditor())
      }.width('100%').padding({ left: 24, right: 24, top: 24 })
      Text(this.t('文字、表格、图片，先放在这里。让一个念头慢慢成形。')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(11).lineHeight(18.7).fontColor(this.muted())
        .width('100%').padding({ left: 24, right: 24 }).margin({ top: 12, bottom: 22 })
      Scroll() {
        Column({ space: 16 }) {
          TextInput({ text: this.title, placeholder: this.t('给它起个名字') }).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(23).placeholderFont({ size: 23 }).fontWeight(FontWeight.Medium)
            .fontColor(this.ink()).placeholderColor(this.muted()).backgroundColor(Color.Transparent).padding(16).maxLength(4000).height(64)
            .border({ width: 1, color: this.accent() }).borderRadius(this.round(12))
            .onChange((s: string) => { if (this.title !== s) { this.title = s; this.dirty = true; } })
          Text(`${this.title.length} / 4000`).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(10).fontColor(this.muted()).width('100%').textAlign(TextAlign.End)
          Row({ space: 8 }) {
            Button({ type: ButtonType.Normal }) { Row({ space: 6 }) { this.Glyph(() => 0xe192,  16, () => this.muted()); Text(this.t('粘贴内容')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(13).fontColor(this.muted()) } }
              .height(40).padding({ left: 16, right: 16 }).borderRadius(this.round(12)).backgroundColor('#217662BA').enabled(false)
            Button({ type: ButtonType.Normal }) { Row({ space: 6 }) { this.Glyph(() => 0xe0b1,  16, () => this.muted()); Text(this.t('导入文件')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(13).fontColor(this.muted()) } }
              .height(40).padding({ left: 16, right: 16 }).borderRadius(this.round(12)).backgroundColor(Color.Transparent).border({ width: 1, color: this.line() }).enabled(false)
          }.justifyContent(FlexAlign.Center).width('100%')
          Text(this.t('富文本粘贴与附件导入正在适配；纯文字可使用输入框的粘贴菜单。')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(11).lineHeight(18).fontColor(this.muted()).width('100%')
          Column({ space: 10 }) {
            Row({ space: 8 }) {
              this.Glyph(() => 0xf0023,  18, () => this.accent()); Text(this.t('正文 · Markdown')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(14).fontWeight(FontWeight.Medium).fontColor(this.ink()); Blank()
              Button({ type: ButtonType.Normal }) { this.Glyph(() => 0xf4a1,  19, () => this.previewBody ? this.accent() : this.muted()) }
                .width(32).height(32).backgroundColor(Color.Transparent).accessibilityText(this.previewBody ? '隐藏预览' : '实时预览').onClick(() => { this.previewBody = !this.previewBody; })
            }.width('100%').height(32)
            Divider().color(this.line())
            if (this.previewBody) { this.BodyPreview() }
            else { TextArea({ text: this.description, placeholder: this.t('写下思路，或粘贴一段内容…') }).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(13).placeholderFont({ size: 13 }).fontColor(this.ink()).placeholderColor(this.muted())
              .backgroundColor(Color.Transparent).border({ width: 1, color: this.line() }).borderRadius(this.round(12)).height(160).maxLength(16000)
              .onChange((s: string) => { if (this.description !== s) { this.description = s; this.dirty = true; } }) }
          }.padding(16).width('100%').backgroundColor('#077662BA').borderRadius(this.round(16)).border({ width: 1, color: this.line() })
          Row({ space: 12 }) {
            Text(this.t('放在哪里')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(11).fontColor(this.muted())
            Select([{ value: '灵感' }, { value: '进行中' }, { value: '实验' }]).value(this.current()?.category ?? this.category)
              .font({ size: 12 }).fontColor(this.ink()).backgroundColor('#147662BA').borderRadius(this.round(10))
              .enabled(!this.busy && this.pending.length === 0 && !this.current()?.deleted)
              .onSelect((_: number, value: string) => {
                if (!this.current()) { this.category = value; this.dirty = true; return; }
                if (this.dirty) { this.message = '请先保存正文编辑'; return; }
                const cmd = this.command('category'); cmd.category = value;
                cmd.stage = value === '进行中' ? '计划中' : value === '实验' ? '待验证' : '待整理'; this.category = value; this.submit(cmd);
              })
            Blank()
            if (this.current()) {
              Select(this.stages()).value(this.current()?.stage ?? '待整理').font({ size: 11 }).fontColor(this.muted()).backgroundColor(Color.Transparent)
                .enabled(!this.busy && this.pending.length === 0 && !this.current()?.deleted)
                .onSelect((_: number, value: string) => {
                  if (this.dirty) { this.message = '请先保存正文编辑'; return; }
                  const cmd = this.command('stage'); cmd.stage = value; this.submit(cmd);
                })
            }
          }.width('100%')
          if (this.category === '实验') {
            TextInput({ text: this.hypothesis, placeholder: this.t('假设') }).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(12).placeholderFont({ size: 12 }).fontColor(this.ink()).backgroundColor(this.glass()).maxLength(4000)
              .onChange((s: string) => { if (this.hypothesis !== s) { this.hypothesis = s; this.dirty = true; } })
            TextInput({ text: this.conclusion, placeholder: this.t('结论') }).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(12).placeholderFont({ size: 12 }).fontColor(this.ink()).backgroundColor(this.glass()).maxLength(4000)
              .onChange((s: string) => { if (this.conclusion !== s) { this.conclusion = s; this.dirty = true; } })
          }
          if (this.current()) {
            Divider().color(this.line())
            Row() {
              Text(this.t('待办事项')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(13).fontWeight(FontWeight.Medium).fontColor(this.ink()); Blank()
              Text(this.current()?.stage ?? '').fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(10).fontColor(this.muted())
            }.width('100%')
            ForEach(this.current()?.tasks ?? [], (task: TaskView) => {
              Row({ space: 10 }) {
                Button(task.completion === 1 ? '✓' : task.completion === 2 ? '?' : '○', { type: ButtonType.Normal })
                  .fontSize(18).fontColor(this.accent()).width(30).height(30).backgroundColor('#147662BA').borderRadius(8)
                  .onClick(() => this.mutate('task_toggle', task)).enabled(!this.busy && this.pending.length === 0 && !this.current()?.deleted)
                Text(task.text).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(12).fontColor(this.ink()).layoutWeight(1)
                Button({ type: ButtonType.Normal }) { this.Glyph(() => 0xf647,  16, () => this.muted()) }
                  .width(28).height(28).backgroundColor(Color.Transparent).accessibilityText('移除待办').onClick(() => this.mutate('task_remove', task))
                  .enabled(!this.busy && this.pending.length === 0 && !this.current()?.deleted)
              }.width('100%')
            }, (task: TaskView) => task.id + ':' + task.completion.toString() + ':' + task.text)
            if (!this.current()?.deleted) {
              Row({ space: 8 }) {
                TextInput({ text: this.taskText, placeholder: this.t('添加一个小步骤…') }).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(12).placeholderFont({ size: 12 }).fontColor(this.ink()).backgroundColor(this.glass()).maxLength(500)
                  .onChange((s: string) => { this.taskText = s; }).layoutWeight(1)
                Button(this.t('+'), { type: ButtonType.Normal }).fontSize(21).fontColor(this.accent()).width(36).height(36).backgroundColor('#147662BA').borderRadius(10)
                  .onClick(() => this.mutate('task_add')).enabled(!this.busy && this.pending.length === 0)
              }
            }
            Row({ space: 12 }) {
              Button(this.current()?.favorite ? '取消收藏' : '收藏', { type: ButtonType.Normal }).fontSize(11).fontColor(this.accent()).backgroundColor(Color.Transparent)
                .onClick(() => this.mutate('favorite')).enabled(!this.busy && this.pending.length === 0 && !this.current()?.deleted)
              Blank()
              Button(this.current()?.deleted ? '撤销删除（8秒内）' : '移入回收站', { type: ButtonType.Normal }).fontSize(11).fontColor(this.muted()).backgroundColor(Color.Transparent)
                .onClick(() => this.mutate(this.current()?.deleted ? 'restore' : 'delete')).enabled(!this.busy && this.pending.length === 0)
            }.width('100%')
          }
        }.padding({ left: 24, right: 24, bottom: 16 }).width('100%')
      }.layoutWeight(1).width('100%').scrollBar(BarState.Off)
      if (this.message.length > 0) { Text(this.message).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(11).fontColor(this.accent()).padding({ left: 24, right: 24, bottom: 12 }).width('100%') }
      Divider().color(this.line())
      Row({ space: 12 }) {
        if (this.dirty) { Text(this.t('未保存')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(10).fontColor(this.muted()) }
        Blank()
        Button(this.t('再想想'), { type: ButtonType.Normal }).fontSize(12).height(38).fontColor(this.accent()).backgroundColor(Color.Transparent).onClick(() => this.closeEditor())
        Button(this.busy ? '保存中…' : '保存灵感', { type: ButtonType.Normal }).fontSize(12).height(38).padding({ left: 22, right: 22 })
          .fontColor(this.dark ? '#302D43' : '#FFFFFF').backgroundColor(this.accent()).borderRadius(this.round(10))
          .enabled(this.ready && !this.busy && this.pending.length === 0 && !this.current()?.deleted).onClick(() => this.save())
      }.padding({ left: 24, right: 24, top: 14, bottom: 14 }).width('100%')
      if (this.pending.length > 0 && !this.busy) {
        Button(this.t('使用原操作重试 / 核对')).fontSize(12).backgroundColor(this.accent()).margin({ bottom: 12 }).onClick(() => this.retry())
      }
    }.width(Math.min(820, this.viewportWidth - 40)).height(Math.max(160, Math.min(760, this.viewportHeight - 40))).backgroundColor(Color.Transparent)
      .backdropBlur(this.materialBlur()).linearGradient(this.gradient('', true)).overlay(this.Rim('', 28)).borderRadius(this.round(28)).border({ width: 1, color: this.line() }).clip(true)
      .shadow({ radius: 32, color: '#28716386', offsetY: 12 })
  }
  build() {
    Stack() {
      if (this.canvasBackground === '默认') { if (this.dark) { AmbientBackdrop({ dark: true }) } else { AmbientBackdrop({ dark: false }) } }
      if (this.canvasBackground === '纹理') { if (this.dark) { PaperBackdrop({ dark: true }) } else { PaperBackdrop({ dark: false }) } }
      if (this.canvasBackground === '透明') { Rect().width('100%').height('100%').fill(alpha(this.surface(), this.canvasOpacity / 100)).backdropBlur(this.canvasBlur).hitTestBehavior(HitTestMode.None) }
      if (this.canvasLiquid) { GlassRim({ dark: this.dark, radius: this.round(20) }).hitTestBehavior(HitTestMode.None) }
      if (this.settingsOpen) { this.Settings() }
      else {
        Row({ space: this.viewportWidth >= 1050 ? 26 : 16 }) {
          if (this.viewportWidth >= 760) { this.Navigation() }
          Column({ space: 22 }) {
            this.Header()
            Row({ space: 24 }) {
              this.Workspace()
              if (this.viewportWidth >= 1050 && this.showAppearance) {
                Scroll() { Column({ space: 20 }) { this.Appearance(); this.DailyPanel(); this.MusicPanel(); this.Quote() } }.width(252).height('100%').scrollBar(BarState.Off)
              }
            }.layoutWeight(1).width('100%').alignItems(VerticalAlign.Top)
          }.layoutWeight(1).height('100%')
        }.padding(this.viewportWidth >= 1050 ? 24 : 12).width('100%').height('100%')
      }
      if (!this.ready) {
        Column({ space: 20 }) {
          Text(this.t('Morrow')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(28).fontWeight(FontWeight.Medium).fontColor(this.ink())
          Text(this.busy ? '正在打开工作区…' : this.message).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(13).fontColor(this.muted())
          if (!this.busy) { Button(this.t('重新打开')).backgroundColor(this.accent()).onClick(() => this.start()) }
        }.padding(32).width('90%').backgroundColor(this.glass()).borderRadius(this.round(23))
      }
      if (this.editorOpen) {
        Stack() {
          Rect().width('100%').height('100%').fill('#40302D43').onClick(() => this.closeEditor())
          this.Editor()
        }.width('100%').height('100%')
      } else if (this.message.length > 0) {
        Row({ space: 8 }) {
          Text(this.message).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(11).fontColor(this.ink()).layoutWeight(1)
          if (this.pending.length > 0 && !this.busy) { Button(this.t('核对')).fontSize(11).backgroundColor(this.accent()).onClick(() => this.retry()) }
          else { Text(this.t('×')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(18).fontColor(this.muted()).onClick(() => { this.message = ''; }) }
        }.padding(14).width('90%').backgroundColor(this.dark ? '#F5292634' : '#F5FFFFFF').borderRadius(this.round(12))
          .border({ width: 1, color: this.line() }).position({ x: '5%', y: Math.max(0, this.viewportHeight - 124) })
      }
    }.width('100%').height('100%').backgroundColor(this.canvasBackgroundColorValue())
      .onAreaChange((_, area: Area) => { this.viewportWidth = Number(area.width); this.viewportHeight = Number(area.height); })
  }
}

