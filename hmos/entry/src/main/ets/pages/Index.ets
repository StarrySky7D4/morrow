import { ColorWheel } from './ColorWheel';
import { MarkdownLine, markdownLines } from '../model/Markdown';
import { uiText } from '../model/UiStrings';
import { preferences } from '@kit.ArkData';
import { AppearanceSnapshot, MaterialChoice, alpha, mix, contrast, visualStyles, visualStyleLabels, styleRadiusScale, resolveMaterial, normalizeMaterials, canFollow } from '../model/Appearance';
import { SurfaceRelief } from './SurfaceRelief';
import { AmbientBackdrop, IdeaOrb, GlassRim, PaperBackdrop } from './StudioArtwork';
import { common } from '@kit.AbilityKit';
import { NodeContent, LazyVWaterFlowLayout, LazyVWaterFlowLayoutAttribute, LengthMetrics } from '@kit.ArkUI';
import { util } from '@kit.ArkTS';
import native from 'libmorrow.so';
import { CardView, Command, Reply, TaskView, workbench } from '../model/Workbench';
import { CardDataSource, WorkspaceCard } from '../model/CardDataSource';
import { QueryConditions, QueryState, QueryCoordinator } from '../model/QueryCoordinator';
import { Values as DraftValues, TextValue as DraftTextValue, DraftScope, DraftRecord,
  EditorDraftCoordinator, DraftSaveFailure, copyValues, sameValues, nextGeneration } from '../model/EditorDraft';

@Component
struct WorkspaceCardSlot {
  @ObjectLink value: WorkspaceCard;
  @BuilderParam content: (card: CardView) => void;
  build() {
    // Keep the outer page/id node through sorting. Recreate only a revised
    // card's presentation so event handlers also receive the current CAS data.
    ForEach([this.value.card], (card: CardView) => {
      this.content(card)
    }, (card: CardView) => card.revision)
  }
}

@Entry
@Component
struct Index {
  @State @Watch('workspaceContentChanged') cards: CardView[] = [];
  @State @Watch('workspaceConditionsChanged') section: string = '概览';
  @State @Watch('workspaceConditionsChanged') search: string = '';
  @State queryIds: string[] = [];
  @State queryPhase: string = 'idle';
  @State queryRefreshing: boolean = false;
  @State selected: string = '';
  @State title: string = '';
  @State description: string = '';
  @State hypothesis: string = '';
  @State conclusion: string = '';
  @State category: string = '灵感';
  @State taskText: string = '';
  @State taskEditId: string = '';
  @State taskRenameText: string = '';
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
  @State @Watch('workspaceConditionsChanged') filter: string = '全部';
  @State @Watch('workspaceConditionsChanged') sort: string = '默认顺序';
  @State capture: string = '';
  @State showAppearance: boolean = true;
  @State sidebarExpanded: boolean = true;
  @State pending: string = '';
  @State draftStatus: string = '';
  @State draftRecords: DraftRecord[] = [];
  @State draftsOpen: boolean = false;
  @State draftWorking: boolean = false;
  @State draftConflict: boolean = false;
  @State draftRetiring: boolean = false;
  @State draftRetirementUnknown: boolean = false;
  private draftRetirement: string = '';
  private submittedRaw: DraftValues | undefined = undefined;
  private editorDraft: EditorDraftCoordinator | undefined = undefined;
  private editorValues: DraftValues = new DraftValues();
  private draftSource: string = '';
  private draftRestoreInput: boolean = false;
  private draftSelectionPending: Set<string> = new Set();
  private draftFocusedFields: Set<string> = new Set();
  private draftCaptureBlocked: Set<string> = new Set();
  @State draftCaptureIncomplete: boolean = false;
  private titleController: TextInputController = new TextInputController();
  private descriptionController: TextAreaController = new TextAreaController();
  private hypothesisController: TextAreaController = new TextAreaController();
  private conclusionController: TextAreaController = new TextAreaController();
  private taskController: TextInputController = new TextInputController();
  @State mode: string = '磨砂';
  @State visualStyle: string = 'flat';
  @State styleDepth: number = 1;
  @State stylePickerOpen: boolean = false;
  @State surfaceAreas: Record<string, number[]> = {};
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
  private workspaceScroller: Scroller = new Scroller();
  private workspaceCards: CardDataSource = new CardDataSource();
  private updateWorkspaceCards(): void { this.workspaceCards.replace(this.section, this.visibleCards()); }
  private contentGeneration: number = 0;
  private queryNeedsRestore: boolean = false;
  private queries: QueryCoordinator = new QueryCoordinator(
    async (conditions: QueryConditions): Promise<string[]> => {
      const cmd = new Command(); cmd.action = 'query';
      cmd.section = conditions.section; cmd.filter = conditions.filter;
      cmd.text = conditions.text; cmd.sort = conditions.sort;
      const reply = await workbench.send(JSON.stringify(cmd));
      if (!reply.ok) { throw new Error(reply.error); }
      return reply.ids;
    }, (state: QueryState): void => {
      this.queryIds = state.ids; this.queryPhase = state.phase; this.queryRefreshing = state.refreshing;
      this.updateWorkspaceCards();
      if (state.phase === 'ready' && this.queryNeedsRestore) {
        this.queryNeedsRestore = false; this.restoreWorkspace();
      }
    });
  private workspaceContentChanged(): void {
    this.contentGeneration++; this.updateWorkspaceCards(); this.workspaceConditionsChanged();
  }
  private workspaceConditionsChanged(): void {
    if (!this.ready) { return; }
    if (this.section === '回收站') {
      this.queries.invalidate(); this.updateWorkspaceCards(); return;
    }
    const conditions = new QueryConditions(); conditions.generation = this.contentGeneration;
    conditions.section = this.section === '灵感' ? '灵感收件箱' : this.section === '进行中' ? '小项目' :
      this.section === '实验' ? '实验室' : this.section === '收藏' ? '已收藏' : '概览';
    conditions.filter = this.filter === '图片' ? '图像' : this.filter === '影音' ? '音视频' : this.filter;
    conditions.text = this.search;
    conditions.sort = this.sort === '标题' ? '标题排序' : this.sort === '收藏优先' ? '收藏优先' : '最近添加';
    this.queries.select(conditions);
  }
  private workspaceOffsets: Map<string, number> = new Map();
  private workspaceRestoreTimer: number = -1;
  private restoringWorkspace: boolean = false;
  private rememberWorkspace(): void {
    if (this.restoringWorkspace || this.settingsOpen || this.queryNeedsRestore ||
      (this.queryPhase === 'loading' && !this.queryRefreshing)) { return; }
    const y = Math.max(0, this.workspaceScroller.currentOffset().yOffset);
    this.workspaceOffsets.delete(this.section);
    this.workspaceOffsets.set(this.section, y);
    // Match Flutter WorkspaceViewport: retain at most five page positions.
    if (this.workspaceOffsets.size > 5) {
      this.workspaceOffsets.delete(Array.from(this.workspaceOffsets.keys())[0]);
    }
  }
  private restoreWorkspace(): void {
    if (this.workspaceRestoreTimer !== -1) { clearTimeout(this.workspaceRestoreTimer); }
    this.restoringWorkspace = true;
    const page = this.section;
    this.workspaceRestoreTimer = setTimeout(() => {
      this.workspaceRestoreTimer = -1;
      if (this.section === page && !this.settingsOpen) {
        this.workspaceScroller.scrollTo({ xOffset: 0, yOffset: this.workspaceOffsets.get(page) ?? 0 });
      }
      this.restoringWorkspace = false;
    }, 16);
  }
  private settingsRouteChanged(): void { this.settingsScroller.scrollTo({ xOffset: 0, yOffset: 0 }); }
  private preview: NodeContent = new NodeContent();
  private mounted: boolean = false;
  private pageAlive: boolean = false;

  aboutToAppear(): void {
    this.pageAlive = true;
    this.getUIContext().getFont().registerFont({ familyName: 'MorrowIcons', familySrc: $rawfile('materialicons.otf') });
    this.restoreAppearance();
    this.start();
  }
  onBackPress(): boolean {
    if (this.draftsOpen) { this.draftsOpen = false; return true; }
    if (this.editorOpen) { this.closeEditor(); return true; }
    if (this.settingsOpen) { this.backSettings(); return true; }
    return false;
  }
  aboutToDisappear(): void {
    this.pageAlive = false;
    // No mutation is replayed on destruction. Only earlier acknowledged raw
    // journal generations are recoverable after a forced process termination.
    this.editorDraft?.dispose();
    this.queries.dispose();
    if (this.workspaceRestoreTimer !== -1) { clearTimeout(this.workspaceRestoreTimer); }
    this.mounted = false; native.releasePreview(this.preview);
  }
  private current(): CardView | undefined { return this.cards.find((c: CardView) => c.id === this.selected); }
  private visibleCards(): CardView[] {
    if (this.section === '回收站') {
      // Deleted rows have their own restore workflow; normal query_v2 excludes
      // them by contract. Keep this explicitly separate from live queries.
      const items = this.cards.filter((c: CardView): boolean => c.deleted &&
        `${c.title}\n${c.description}\n${c.hypothesis}\n${c.conclusion}`.toLowerCase().includes(this.search.toLowerCase()));
      if (this.sort === '标题') { items.sort((a: CardView, b: CardView): number => a.title < b.title ? -1 : a.title > b.title ? 1 : 0); }
      if (this.sort === '收藏优先') { items.sort((a: CardView, b: CardView): number => Number(b.favorite) - Number(a.favorite)); }
      return items;
    }
    const byId = new Map<string, CardView>(); this.cards.forEach((c: CardView) => byId.set(c.id, c));
    const items: CardView[] = [];
    this.queryIds.forEach((id: string): void => { const card = byId.get(id); if (card && !card.deleted) { items.push(card); } });
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
      this.cards = result.cards; this.ready = true; this.message = ''; this.workspaceConditionsChanged();
      await this.loadDrafts();
    } catch (_) { this.message = '打开失败，请重试'; }
    finally { this.busy = false; }
  }
  private async loadDrafts(): Promise<void> {
    const cmd = new Command(); cmd.action = 'draft_list';
    const result = await workbench.send(JSON.stringify(cmd));
    if (!result.ok) { throw new Error(result.error); }
    this.draftRecords = result.drafts;
  }
  private draftChanged(): void {
    const draft = this.editorDraft;
    if (!draft) { return; }
    this.draftStatus = draft.unknown ? '草稿回复未确认，原请求已保留' : draft.conflicted ? '草稿世代已变化，请核对' :
      draft.error.length > 0 ? '草稿保存失败：' + draft.error : draft.saving ? '正在保留草稿…' :
      draft.dirty ? '草稿等待保存' : draft.confirmed ? '草稿已保留' : '';
    const confirmed = draft.confirmed;
    if (confirmed && confirmed.active && confirmed.current_active && confirmed.generation === confirmed.current_generation) {
      const others = this.draftRecords.filter((r: DraftRecord) => r.scope.draft_id !== confirmed.scope.draft_id);
      this.draftRecords = others.concat([confirmed]);
    }
  }
  private draftField(name: string): DraftTextValue {
    return name === 'title' ? this.editorValues.title : name === 'description' ? this.editorValues.description :
      name === 'hypothesis' ? this.editorValues.hypothesis : name === 'conclusion' ? this.editorValues.conclusion : this.editorValues.todos;
  }
  private draftTextChanged(name: string, value: string, preview?: PreviewText): void {
    if (!this.pageAlive || !this.editorOpen || !this.editorDraft || this.editorDraft.disposed || this.draftRestoreInput || this.draftRetiring || this.draftRetirementUnknown) { return; }
    const field = this.draftField(name);
    const offset = preview?.offset ?? -1, candidate = preview?.value ?? '';
    if (candidate.length > 0 && (!Number.isInteger(offset) || offset < 0 || offset > value.length)) {
      this.draftCaptureBlocked.add(name); this.draftCaptureIncomplete = true;
      this.message = '输入法预编辑位置无法确认，请完成输入后保留草稿。'; return;
    }
    this.draftCaptureBlocked.delete(name); this.draftCaptureIncomplete = this.draftCaptureBlocked.size > 0;
    field.text = candidate.length > 0 ? value.slice(0, offset) + candidate + value.slice(offset) : value;
    field.composing_start = candidate.length > 0 ? offset : -1;
    field.composing_end = candidate.length > 0 ? offset + candidate.length : -1;
    // Selection is independent of an edit's range. Retain valid controller
    // offsets; a later selection callback captures the actual current range.
    if (field.selection_base > field.text.length) { field.selection_base = -1; }
    if (field.selection_extent > field.text.length) { field.selection_extent = -1; }
    this.editorValues.category = this.category;
    this.editorDraft?.update(copyValues(this.editorValues));
  }
  private draftSelectionChanged(name: string, start: number, end: number): void {
    if (!this.pageAlive || !this.editorOpen || this.editorDraft?.disposed || !this.draftFocusedFields.has(name) || this.draftRestoreInput || this.draftRetiring || this.draftRetirementUnknown || this.draftSelectionPending.has(name) || !this.editorDraft) { return; }
    const field = this.draftField(name);
    if (this.draftCaptureBlocked.has(name) || start < -1 || end < -1 || start > field.text.length || end > field.text.length) { return; }
    field.selection_base = start; field.selection_extent = end;
    this.editorDraft.update(copyValues(this.editorValues));
  }
  private restoreSelection(name: string): void {
    if (!this.draftSelectionPending.has(name)) { return; }
    const value = this.draftField(name), start = value.selection_base, end = value.selection_extent;
    const draft = this.editorDraft;
    if (start < 0 || end < 0) { this.draftSelectionPending.delete(name); return; }
    // Do not start a new IME composition or steal focus. The caller already
    // focused this field; native controllers cannot restore the old IME session.
    setTimeout(() => {
      if (!this.pageAlive || !this.editorOpen || !draft || draft.disposed || this.draftRetiring || this.draftRetirementUnknown || this.editorDraft !== draft) { return; }
      const low = Math.min(start, end), high = Math.max(start, end);
      if (name === 'description' || name === 'hypothesis' || name === 'conclusion') {
        const controller = name === 'description' ? this.descriptionController : name === 'hypothesis' ? this.hypothesisController : this.conclusionController;
        if (low === high) { controller.caretPosition(low); }
        else { controller.setTextSelection(low, high, { menuPolicy: MenuPolicy.HIDE }); }
      } else {
        const controller = name === 'title' ? this.titleController : this.taskController;
        if (low === high) { controller.caretPosition(low); }
        else { controller.setTextSelection(low, high, { menuPolicy: MenuPolicy.HIDE }); }
      }
      setTimeout(() => { if (this.pageAlive && !draft.disposed && this.editorDraft === draft) { this.draftSelectionPending.delete(name); } }, 16);
    }, 16);
  }
  private focusDraftField(name: string): void {
    if (!this.pageAlive || !this.editorOpen || !this.editorDraft || this.editorDraft.disposed || this.draftRetiring || this.draftRetirementUnknown) { return; }
    // A native control emits its initial selection while being constructed.
    // Only a field that the user actually focuses can create a selection-only
    // generation; viewing an unchanged card must not consume a journal slot.
    this.draftFocusedFields.clear(); this.draftFocusedFields.add(name); this.restoreSelection(name);
  }
  private blurDraftField(name: string): void { this.draftFocusedFields.delete(name); }
  private attachDraft(card?: CardView, restored?: DraftRecord, preserveFocus: boolean = false): void {
    this.editorDraft?.dispose(); this.draftConflict = false; this.draftStatus = '';
    this.draftSelectionPending = restored ? new Set(['title', 'description', 'hypothesis', 'conclusion', 'todos']) : new Set();
    if (!preserveFocus) { this.draftFocusedFields.clear(); }
    this.draftRestoreInput = !!restored; this.draftCaptureBlocked.clear(); this.draftCaptureIncomplete = false;
    const scope = restored?.scope ?? new DraftScope();
    if (!restored) {
      scope.card_id = card?.id ?? util.generateRandomUUID(true); scope.draft_id = util.generateRandomUUID(true);
      scope.source_kind = card ? 0 : 1; scope.source_revision = card?.revision ?? '0'; scope.source = card?.source ?? '';
    }
    this.draftSource = scope.source;
    const values = restored ? copyValues(restored.values) : new DraftValues();
    if (!restored) {
      values.title.text = this.title; values.description.text = this.description;
      values.hypothesis.text = this.hypothesis; values.conclusion.text = this.conclusion;
      values.todos.text = this.taskText; values.category = this.category;
      values.stage = card?.stage ?? (this.category === '进行中' ? '计划中' : this.category === '实验' ? '待验证' : '待整理');
    }
    this.editorValues = values;
    this.editorDraft = new EditorDraftCoordinator(scope, values,
      async (serialized: string): Promise<DraftRecord> => {
        const result = await workbench.send(serialized);
        if (!result.ok) { throw new DraftSaveFailure(result.effect === 'not_committed', result.error); }
        if (result.drafts.length !== 1) { throw new Error('草稿回执缺失'); }
        return result.drafts[0];
      }, () => this.draftChanged(), () => util.generateRandomUUID(true), restored);
    // A restored frozen baseline remains separate from the current business
    // record. No implicit rebase or overwrite is permitted.
    this.draftConflict = scope.source_kind === 0 ? card?.revision !== scope.source_revision : !!card;
    this.draftChanged();
  }
  private restoreDraft(record: DraftRecord): void {
    if (this.busy || this.pending.length > 0 || this.editorOpen) { return; }
    this.draftRestoreInput = true;
    this.selected = record.scope.source_kind === 0 ? record.scope.card_id : '';
    this.title = record.values.title.text; this.description = record.values.description.text;
    this.hypothesis = record.values.hypothesis.text; this.conclusion = record.values.conclusion.text;
    this.taskText = record.values.todos.text; this.category = record.values.category;
    this.taskEditId = ''; this.taskRenameText = ''; this.previewBody = false; this.editorOpen = true; this.draftsOpen = false;
    this.dirty = true; this.message = '';
    this.attachDraft(this.cards.find((c: CardView) => c.id === record.scope.card_id), record);
    const draft = this.editorDraft;
    setTimeout(() => { if (this.editorDraft === draft) { this.draftRestoreInput = false; } }, 16);
    if (this.draftConflict) { this.message = '原卡片已变化，草稿文字已保留。请查看当前卡片并核对；此草稿不会覆盖新的修订。'; }
  }
  private async flushDraft(): Promise<boolean> {
    if (this.draftCaptureIncomplete) { this.message = '当前输入尚未完整捕获，请完成输入后重试，编辑器保持打开。'; return false; }
    const draft = this.editorDraft;
    if (!draft) { return true; }
    this.draftWorking = true;
    try {
      const record = await draft.flush();
      if (this.editorDraft !== draft || this.draftCaptureIncomplete || draft.dirty || draft.unknown ||
        (record && !sameValues(draft.current, record.values))) {
        this.message = '当前输入在保留期间发生变化，请再次保留，编辑器保持打开。'; return false;
      }
      return true;
    }
    catch (error) { this.message = '最新输入尚未保留：' + String(error); return false; }
    finally { this.draftWorking = false; this.draftChanged(); }
  }
  private async keepDraftAndClose(): Promise<void> {
    if (!(await this.flushDraft())) { return; }
    this.dirty = false; this.editorOpen = false; this.message = this.editorDraft?.confirmed ? '草稿已保留，可从草稿入口继续。' : '';
    this.editorDraft?.dispose(); this.editorDraft = undefined;
    this.taskEditId = ''; this.taskRenameText = ''; this.taskText = ''; await this.loadDrafts();
  }
  private async retireDraft(expected?: DraftValues, reconcile: boolean = false): Promise<boolean> {
    const draft = this.editorDraft, record = draft?.confirmed;
    if (!draft || !record) { this.editorDraft?.dispose(); this.editorDraft = undefined; return true; }
    if (!reconcile && this.draftRetirementUnknown) { return false; }
    if (draft.saving || draft.unknown || draft.dirty || !sameValues(draft.current, record.values) ||
      (expected && !sameValues(draft.current, expected))) {
      this.message = '卡片已保存，后续输入的草稿仍保留。'; return false;
    }
    if (this.draftRetirement.length === 0) {
      const cmd = new Command(); cmd.action = 'draft_discard'; cmd.id = record.scope.card_id;
      cmd.draft_id = record.scope.draft_id; cmd.generation = record.generation; cmd.operation = util.generateRandomUUID(true);
      this.draftRetirement = JSON.stringify(cmd);
    }
    this.draftRetiring = true;
    try {
      const result = await workbench.send(this.draftRetirement);
      if (!result.ok) {
        if (result.effect === 'not_committed' && !this.draftRetirementUnknown) { this.draftRetirement = ''; }
        else { this.draftRetirementUnknown = true; }
        this.message = '草稿清理未确认：' + result.error; return false;
      }
      if (result.drafts.length !== 1 || result.drafts[0].active || result.drafts[0].current_active ||
        result.drafts[0].scope.card_id !== record.scope.card_id || result.drafts[0].scope.draft_id !== record.scope.draft_id ||
        result.drafts[0].generation !== nextGeneration(record.generation) || result.drafts[0].current_generation !== result.drafts[0].generation) {
        throw new Error('草稿清理回执不匹配');
      }
      draft.dispose(); this.editorDraft = undefined; this.draftStatus = ''; this.draftConflict = false;
      this.draftRetirement = ''; this.draftRetirementUnknown = false;
      this.draftRecords = this.draftRecords.filter((r: DraftRecord) => r.scope.draft_id !== record.scope.draft_id);
      try { await this.loadDrafts(); } catch (_) { this.message = '草稿已清理，列表刷新失败，请重新打开工作区核对。'; }
      return true;
    } catch (_) {
      this.draftRetirementUnknown = true; this.message = '草稿清理回复未确认，原操作已保留，请显式核对。'; return false;
    } finally { this.draftRetiring = false; }
  }
  private discardDraft(): void {
    if (this.busy || this.pending.length > 0 || this.draftWorking || this.editorDraft?.unknown) { return; }
    this.getUIContext().showAlertDialog({ title: '放弃这份草稿？', message: '草稿中的文字将被丢弃，已保存卡片保持原样。',
      primaryButton: { value: '继续编辑', action: () => {} }, secondaryButton: { value: '放弃草稿', action: () => this.discardDraftConfirmed() } });
  }
  private async discardDraftConfirmed(): Promise<void> {
    this.draftRetiring = true;
    if (!(await this.flushDraft())) { this.draftRetiring = false; return; }
    this.draftWorking = true;
    try {
      if (await this.retireDraft()) { this.dirty = false; this.taskText = ''; this.editorOpen = false; }
    } catch (_) { this.message = '草稿放弃未确认，记录仍保留，请重新读取后核对。'; }
    finally { this.draftWorking = false; this.draftRetiring = false; }
  }
  private async retryDraft(): Promise<void> {
    if (!this.editorDraft || this.draftWorking) { return; }
    this.draftWorking = true;
    try { await this.editorDraft.retry(); }
    catch (error) { this.message = '草稿核对失败：' + String(error); }
    finally { this.draftWorking = false; this.draftChanged(); }
  }
  private async retryDraftRetirement(): Promise<void> {
    if (!this.draftRetirementUnknown || this.draftWorking || this.busy) { return; }
    this.draftWorking = true;
    try { if (await this.retireDraft(undefined, true)) { this.dirty = false; this.editorOpen = false; } }
    finally { this.draftWorking = false; }
  }
  private editorInputChanged(name: string, value: string, preview?: PreviewText): void {
    if (!this.pageAlive || !this.editorOpen || !this.editorDraft || this.editorDraft.disposed || this.draftRetiring || this.draftRetirementUnknown || this.draftRestoreInput) { return; }
    const previous = name === 'title' ? this.title : name === 'description' ? this.description :
      name === 'hypothesis' ? this.hypothesis : name === 'conclusion' ? this.conclusion : this.taskText;
    if (name === 'title') { this.title = value; }
    else if (name === 'description') { this.description = value; }
    else if (name === 'hypothesis') { this.hypothesis = value; }
    else if (name === 'conclusion') { this.conclusion = value; }
    else { this.taskText = value; }
    if (name !== 'todos' && (previous !== value || (preview?.value.length ?? 0) > 0)) { this.dirty = true; }
    this.draftTextChanged(name, value, preview);
  }
  private select(c: CardView): void {
    if (this.editorOpen || this.busy || this.draftWorking || this.draftRetiring || this.draftRetirementUnknown || this.pending.length > 0) { return; }
    if (this.dirty) { this.message = '请先保存当前编辑，输入已保留。'; return; }
    const recovered = this.draftRecords.find((r: DraftRecord) => r.scope.card_id === c.id);
    if (recovered) { this.restoreDraft(recovered); return; }
    this.taskEditId = ''; this.taskText = ''; this.taskRenameText = '';
    this.previewBody = false; this.selected = c.id; this.title = c.title; this.description = c.description;
    this.hypothesis = c.hypothesis; this.conclusion = c.conclusion; this.category = c.category; this.editorOpen = true; this.message = ''; 
    this.attachDraft(c);
    this.refreshPreview();
  }
  private newCard(): boolean {
    if (this.editorOpen || this.busy || this.draftWorking || this.draftRetiring || this.draftRetirementUnknown || this.pending.length > 0) { return false; }
    if (this.dirty) { this.message = '请先保存当前编辑，输入已保留。'; return false; }
    this.taskEditId = ''; this.taskText = ''; this.taskRenameText = '';
    this.selected = ''; this.title = ''; this.description = ''; this.hypothesis = ''; this.conclusion = '';
    this.category = ['灵感', '进行中', '实验'].includes(this.section) ? this.section : '灵感'; this.previewBody = false; this.editorOpen = true; this.message = ''; this.refreshPreview();
    this.attachDraft();
    return true;
  }
  private command(action: string): Command {
    const c = this.current(); const cmd = new Command();
    cmd.action = action; cmd.id = c?.id ?? this.editorDraft?.scope.card_id ?? util.generateRandomUUID(true);
    cmd.source = (action === 'edit' && this.editorDraft) ? this.draftSource : c?.source ?? '';
    cmd.operation = util.generateRandomUUID(true); cmd.now_ms = Date.now().toString(); return cmd;
  }
  private async submit(cmd: Command, raw?: DraftValues): Promise<void> {
    if (this.busy || this.pending.length > 0 || this.draftWorking || this.draftRetirementUnknown) { return; }
    if (this.taskEditId.length > 0 && cmd.action !== 'task_rename') { this.message = '请先保存或取消待办重命名'; return; }
    const frozen = raw ?? (this.editorOpen ? this.editorDraft?.current : undefined);
    if (this.editorOpen && !(await this.flushDraft())) { return; }
    this.submittedRaw = frozen;
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
          this.pending = ''; this.submittedRaw = undefined; this.message = `未提交：${result.error}。请修正后再保存，输入已保留。`;
        } else { this.message = `保存未确认：${result.error}。原请求与当前输入均已保留。`; }
        return;
      }
      this.cards = result.cards; this.pending = '';
      const unchangedRaw = !this.draftCaptureIncomplete &&
        (!this.submittedRaw || !this.editorDraft || sameValues(this.editorDraft.current, this.submittedRaw));
      if (cmd.action === 'create' || cmd.action === 'edit') {
        this.selected = cmd.id;
        const values = this.editorDraft?.current ?? this.editorValues;
        this.dirty = values.title.text !== cmd.title || values.description.text !== cmd.description ||
          values.hypothesis.text !== cmd.hypothesis || values.conclusion.text !== cmd.conclusion;
      }
      const todoRaw = this.editorDraft?.current.todos ?? this.editorValues.todos;
      if (cmd.action === 'task_add' && this.taskText === cmd.text && todoRaw.text === cmd.text &&
        todoRaw.composing_start === -1 && todoRaw.composing_end === -1 && unchangedRaw && !this.draftCaptureBlocked.has('todos')) {
        this.taskText = ''; this.draftTextChanged('todos', '');
      }
      if (cmd.action === 'task_rename' && this.selected === cmd.id && this.taskEditId === cmd.task_id &&
        this.taskRenameText.trim() === cmd.text) { this.taskEditId = ''; this.taskRenameText = ''; }
      this.message = result.receipt_revision.length > 0 ? `已保存 · 操作修订 ${result.receipt_revision}` : '已刷新';
      const completionValues = this.editorDraft?.current;
      // Ordinary body/favorite/stage commands do not consume pending todo text.
      // Keep its raw journal until an explicit task submission or handoff.
      const consumedInput = unchangedRaw && (completionValues?.todos.text.length ?? 0) === 0;
      if ((cmd.action === 'create' || cmd.action === 'edit') && !this.dirty && consumedInput) {
        // A business receipt does not certify later raw input. Retire only the
        // exact latest confirmed draft; otherwise keep the independent draft.
        if (await this.flushDraft() && await this.retireDraft(completionValues)) { this.editorOpen = false; }
      } else if (this.editorOpen && !this.dirty && consumedInput && this.taskEditId.length === 0) {
        // Each confirmed business revision starts a distinct source-bound raw
        // draft. Retire the old slot only after its latest input is durable.
        if (await this.flushDraft() && await this.retireDraft(completionValues)) { this.attachDraft(this.current(), undefined, true); }
      }
      const draft = this.editorDraft;
      if (draft) {
        const record = this.cards.find((c: CardView) => c.id === draft.scope.card_id);
        this.draftConflict = draft.scope.source_kind === 0 ? record?.revision !== draft.scope.source_revision : !!record;
        if (this.draftConflict) { this.message = '卡片已保存，较新的草稿输入仍保留。原卡片已变化，请核对后处理。'; }
      }
      this.submittedRaw = undefined;
      this.refreshPreview();
    } catch (_) { this.message = '回复未确认。请用原请求核对，不会创建新的操作。'; }
    finally { this.busy = false; }
  }
  private async save(): Promise<void> {
    if (this.draftWorking || this.draftConflict) { return; }
    if (this.taskEditId.length > 0) { this.message = '请先保存或取消待办重命名'; return; }
    if (this.title.trim().length === 0) { this.message = '请输入卡片标题'; return; }
    if (!(await this.flushDraft())) { return; }
    const values = this.editorDraft?.current ?? this.editorValues;
    const cmd = this.command(this.selected.length > 0 ? 'edit' : 'create');
    cmd.title = values.title.text; cmd.description = values.description.text; cmd.hypothesis = values.hypothesis.text;
    cmd.conclusion = values.conclusion.text; cmd.category = values.category;
    cmd.stage = this.category === '进行中' ? '计划中' : this.category === '实验' ? '待验证' : '待整理';
    this.submit(cmd, values);
  }
  private mutate(action: string, task?: TaskView): void {
    if (this.taskEditId.length > 0 && action !== 'task_rename') { this.message = '请先保存或取消待办重命名'; return; }
    if (this.dirty) { this.message = '请先保存正文编辑'; return; }
    const c = this.current(); if (!c) { return; }
    if (action === 'restore' && (Date.now() < Number(c.deleted_at) || Date.now() - Number(c.deleted_at) >= 8000)) {
      this.message = '已超过共享核心的 8 秒撤销删除期限，卡片保持删除状态。'; return;
    }
    const cmd = this.command(action);
    if (action === 'favorite') { cmd.flag = !c.favorite; }
    if (task) { cmd.task_id = task.id; cmd.flag = task.completion !== 1; }
    if (action === 'task_rename') {
      if (!task || task.id !== this.taskEditId || this.taskRenameText.trim().length === 0) { return; }
      cmd.text = this.taskRenameText.trim();
    }
    if (action === 'task_add') {
      if (this.taskText.trim().length === 0) { return; }
      const raw = this.editorDraft?.current.todos ?? this.editorValues.todos;
      if (this.draftCaptureIncomplete || raw.text !== this.taskText || raw.composing_start !== -1 || raw.composing_end !== -1) {
        this.message = '请先完成待办文字输入，再添加待办；草稿输入已保留。'; return;
      }
      cmd.task_id = util.generateRandomUUID(true); cmd.text = this.taskText;
    }
    this.submit(cmd);
  }
  private canEditTasks(): boolean {
    return this.ready && !this.busy && !this.draftWorking && !this.draftConflict && !this.draftCaptureIncomplete && !this.editorDraft?.unknown && this.pending.length === 0 &&
      !!this.current() && !this.current()?.deleted && !this.dirty && this.taskEditId.length === 0;
  }
  private renameTask(task: TaskView): void {
    if (!this.canEditTasks()) { return; }
    this.taskEditId = task.id; this.taskRenameText = task.text;
  }
  private moveTask(task: TaskView, delta: number): void {
    if (!this.canEditTasks()) { return; }
    const order = (this.current()?.tasks ?? []).map((t: TaskView): string => t.id);
    const index = order.indexOf(task.id); const next = index + delta;
    if (index < 0 || next < 0 || next >= order.length) { return; }
    order.splice(index, 1); order.splice(next, 0, task.id);
    const cmd = this.command('task_reorder'); cmd.order = order; this.submit(cmd);
  }
  private confirmTaskChange(task?: TaskView): void {
    if (!this.canEditTasks()) { return; }
    const cmd = this.command(task ? 'task_remove' : 'task_complete_all');
    cmd.task_id = task?.id ?? ''; cmd.stage = this.current()?.stage ?? '';
    this.getUIContext().showAlertDialog({ title: task ? '移除待办' : '全部勾选并设置阶段',
      message: task ? `删除待办“${task.text}”？` : `将所有待办标为完成，并将阶段设为“${cmd.stage}”？`,
      primaryButton: { value: this.t('取消'), action: () => {} },
      secondaryButton: { value: this.t('确认'), action: () => {
        // Confirmation belongs to this exact card revision, never a later view.
        if (this.canEditTasks() && this.selected === cmd.id && this.current()?.source === cmd.source) { this.submit(cmd); }
      } } });
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
      const materials = normalizeMaterials(v.materials ?? []);
      const style = v.visualStyle ?? 'flat'; const depth = v.styleDepth ?? 1;
      if (!visualStyles.includes(style) || !Number.isFinite(depth) || depth < 0 || depth > 2) { throw new Error('Invalid style'); }
      this.theme = ['白色', '自定义', '深色'].includes(v.theme) ? v.theme : '白色';
      this.mode = ['磨砂', '超透', '液体玻璃'].includes(v.mode) ? v.mode : '磨砂';
      this.glassOpacity = Math.max(20, Math.min(100, v.opacity)); this.radius = Math.max(0, Math.min(32, v.radius));
      this.themeColor = v.accent; this.toneGray = v.grayscale; this.lightness = v.lightness;
      this.canvasBackground = v.background; this.solid = v.solid; this.canvasLiquid = v.canvasLiquid;
      this.canvasBlur = v.canvasBlur; this.canvasOpacity = v.canvasOpacity;
      this.fontFamily = v.font; this.locale = v.locale; this.daily = v.daily; this.materials = materials;
      this.visualStyle = style; this.styleDepth = depth;
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
    v.visualStyle = this.visualStyle; v.styleDepth = this.styleDepth;
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
  private openSettings(page: string): void {
    this.rememberWorkspace();
    this.settingsOpen = true; this.settingsPage = page;
    if (page === '字体设置') { this.fontDraft = this.fontFamily; }
  }
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
  private material(id: string): MaterialChoice | undefined { return resolveMaterial(this.materials, id); }
  private depthFor(id: string = ''): number { const depth = this.material(id)?.depth ?? -1; return depth < 0 ? this.styleDepth : depth; }
  private surfaceShadow(id: string = '', style: string = this.visualStyle, depth: number = this.depthFor(id)): ShadowOptions {
    // Flutter FooterOverlay is plain content unless its material is customized.
    if (id === 'footer' && !this.material(id)) { return { radius: 0, color: Color.Transparent }; }
    const d = depth; const opacity = Math.min(1, d);
    switch (style) {
      case 'neumorphism': return { radius: 5, offsetX: 1.5 * d, offsetY: 1.5 * d, color: alpha(this.dark ? '#000000' : '#8A8299', (this.dark ? .24 : .12) * opacity) };
      case 'paper': return { radius: 1, offsetX: 0, offsetY: 2 * d, color: alpha(this.ink(), (this.dark ? .28 : .18) * opacity) };
      case 'clay': return { radius: 4, offsetX: d, offsetY: 2 * d, color: alpha('#000000', (this.dark ? .28 : .13) * opacity) };
      case 'fluent': return { radius: 4, offsetX: 0, offsetY: 1.5 * d, color: alpha('#000000', (this.dark ? .22 : .07) * opacity) };
      case 'brutalist': return { radius: 0, offsetX: 2.5 * d, offsetY: 2.5 * d, color: alpha(this.ink(), (this.dark ? .48 : .65) * opacity) };
      case 'industrial': return { radius: 1, offsetX: 0, offsetY: 2 * d, color: alpha(this.ink(), (this.dark ? .28 : .18) * opacity) };
      default: return { radius: 18, offsetX: 0, offsetY: 8, color: '#09716386' };
    }
  }
  private modeFor(id: string = ''): string { const m = this.material(id); return m && m.mode !== '跟随主题' ? m.mode : this.mode; }
  private materialBlur(id: string = ''): number { return this.material(id)?.blur ?? (this.modeFor(id) === '磨砂' ? 22 : this.modeFor(id) === '超透' ? 1 : 5); }
  private gradient(id: string = '', readable: boolean = false): LinearGradientOptions {
    return this.materialGradient(this.material(id), this.modeFor(id), readable);
  }
  private materialGradient(m: MaterialChoice | undefined, mode: string, readable: boolean = false): LinearGradientOptions {
    const tint = m?.color || this.surface();
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
    try { this.materials = normalizeMaterials(this.previewMaterials()); }
    catch (_) { this.message = '材质无效或形成循环跟随，未应用。'; return; }
    this.persistAppearance(); this.settingsPage = '组件与卡片';
  }
  private setMaterialValue(field: string, value: number): void {
    const v = JSON.parse(JSON.stringify(this.materialDraft)) as MaterialChoice;
    if (field === 'blur') { v.blur = value; } else if (field === 'opacity') { v.opacity = value; } else if (field === 'depth') { v.depth = value; } else { v.radius = value; }
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
      if (id !== 'footer' || this.material(id)) {
      SurfaceRelief({ style: this.visualStyle, depth: this.depthFor(id), dark: this.dark, radius: this.round(base, id) })
      if (this.modeFor(id) === '液体玻璃') { GlassRim({ dark: this.dark, radius: this.round(base, id) }).hitTestBehavior(HitTestMode.None) }
      }
    }.width(this.surfaceAreas[id + ':' + base]?.[0] ?? 0).height(this.surfaceAreas[id + ':' + base]?.[1] ?? 0).clip(true).hitTestBehavior(HitTestMode.None)
  }
  private rememberSurface(id: string, base: number, area: Area): void {
    const key = id + ':' + base; const w = Number(area.width); const h = Number(area.height);
    const prior = this.surfaceAreas[key];
    if (prior && Math.abs(prior[0] - w) < .1 && Math.abs(prior[1] - h) < .1) { return; }
    // Overlay percentages resolve against the padded content box. Use the
    // actual panel bounds so relief and liquid rims trace the outside edge.
    const next: Record<string, number[]> = {};
    Object.keys(this.surfaceAreas).forEach((k: string) => { next[k] = this.surfaceAreas[k]; });
    next[key] = [w, h]; this.surfaceAreas = next;
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
  private line(): string {
    if (this.visualStyle === 'brutalist') { return alpha(this.ink(), .65); }
    if (this.visualStyle === 'paper' || this.visualStyle === 'industrial') { return alpha(this.ink(), this.dark ? .28 : .18); }
    if (this.visualStyle === 'neumorphism' || this.visualStyle === 'clay') { return alpha('#FFFFFF', this.dark ? .13 : .58); }
    return this.dark ? '#26F0EDF8' : '#14302D43';
  }
  private styleEdgeWidth(): number { return this.visualStyle === 'brutalist' ? 1.8 : 1; }
  private round(value: number, id: string = ''): number {
    const local = this.material(id); return value * (local ? local.radius : this.radius * styleRadiusScale(this.visualStyle)) / 20;
  }
  private label(section: string): string {
    return this.t(section === '灵感' ? '灵感收件箱' : section === '进行中' ? '小项目' : section === '实验' ? '实验室' : section === '收藏' ? '已收藏' : section);
  }
  private icon(section: string): number {
    return section === '概览' ? 0xf7c4 : section === '灵感' ? 0xf126 : section === '进行中' ? 0xf091 : section === '实验' ? 0xf33d : section === '收藏' ? 0xf5cf : 0xf696;
  }
  private navigate(section: string): void {
    this.rememberWorkspace();
    if (section !== this.section) { this.queryNeedsRestore = section !== '回收站'; }
    this.section = section; this.filter = '全部';
    this.restoreWorkspace();
  }
  private closeEditor(): void {
    if (this.busy || this.pending.length > 0 || this.draftWorking || this.draftRetirementUnknown) { this.message = '操作尚未确认，当前输入已保留。'; return; }
    if (this.taskEditId.length > 0) { this.message = '请先保存或取消待办重命名'; return; }
    this.keepDraftAndClose();
  }
  private favoriteCard(c: CardView): void {
    const cmd = new Command(); cmd.action = 'favorite'; cmd.id = c.id; cmd.source = c.source;
    cmd.operation = util.generateRandomUUID(true); cmd.now_ms = Date.now().toString(); cmd.flag = !c.favorite;
    this.submit(cmd);
  }
  private captureIdea(): void {
    if (this.newCard() && this.capture.length > 0) {
      this.editorInputChanged('title', this.capture); this.capture = '';
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
      .backgroundColor(this.glass()).backdropBlur(this.materialBlur('navigation')).linearGradient(this.gradient('navigation')).overlay(this.Rim('navigation', 26)).onAreaChange((_, area: Area) => this.rememberSurface('navigation',  26, area)).shadow(this.surfaceShadow('navigation')).borderRadius(this.round(26, 'navigation')).border({ width: this.styleEdgeWidth(), color: this.line() })
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
        Button({ type: ButtonType.Normal }) {
          this.Glyph(() => this.sidebarExpanded ? 0xe15e : 0xe15f, 18, () => this.muted())
        }.width(24).height(38).padding(0).backgroundColor(Color.Transparent)
          .accessibilityText(this.sidebarExpanded ? '收起侧栏' : '展开侧栏')
          .onClick(() => { this.sidebarExpanded = !this.sidebarExpanded; })
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
        .backdropBlur(this.materialBlur('search')).linearGradient(this.gradient('search')).overlay(this.Rim('search', 12)).onAreaChange((_, area: Area) => this.rememberSurface('search',  12, area)).shadow(this.surfaceShadow('search')).border({ width: this.styleEdgeWidth(), color: this.line() })
      Button({ type: ButtonType.Normal }) { this.Glyph(() => 0xf0258,  19, () => this.muted()) }
        .width(40).height(38).backgroundColor(Color.Transparent).accessibilityText('空间外观')
        .onClick(() => { if (this.viewportWidth >= 1050) { this.showAppearance = !this.showAppearance; } else { this.openSettings('设置'); } })
      Button('草稿' + (this.draftRecords.length > 0 ? ' ' + this.draftRecords.length : ''), { type: ButtonType.Normal })
        .fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(11).fontColor(this.accent()).backgroundColor(Color.Transparent)
        .height(38).padding({ left: 8, right: 8 }).accessibilityText('打开草稿')
        .enabled(this.ready && !this.busy && this.pending.length === 0).onClick(() => { this.draftsOpen = true; })
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
      .backgroundColor(this.glass()).backdropBlur(this.materialBlur('hero')).linearGradient(this.gradient('hero')).overlay(this.Rim('hero', 23)).onAreaChange((_, area: Area) => this.rememberSurface('hero',  23, area)).shadow(this.surfaceShadow('hero')).border({ width: this.styleEdgeWidth(), color: this.line() })
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
    }.alignItems(HorizontalAlign.Start).padding(22).width('100%').backgroundColor(this.glass()).backdropBlur(this.materialBlur('summary')).linearGradient(this.gradient('summary')).overlay(this.Rim('summary', 20)).onAreaChange((_, area: Area) => this.rememberSurface('summary',  20, area)).shadow(this.surfaceShadow('summary'))
      .borderRadius(this.round(20, 'summary')).border({ width: this.styleEdgeWidth(), color: this.line() })
  }
  @Builder
  CollectionHeader() {
    Flex({ wrap: FlexWrap.Wrap, justifyContent: FlexAlign.Start, alignItems: ItemAlign.Center }) {
      Row({ space: 8 }) {
        Text(this.section === '概览' ? this.t('最近的念头') : this.label(this.section)).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(15).fontWeight(FontWeight.Medium).fontColor(this.ink())
        Text(this.queryPhase === 'loading' && !this.queryRefreshing ? '…' : this.visibleCards().length.toString().padStart(2, '0')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(10).fontColor(this.muted())
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
      .backgroundColor(this.glass()).backdropBlur(this.materialBlur('card:' + c.id + '')).linearGradient(this.gradient('card:' + c.id + '')).overlay(this.Rim('card:' + c.id + '', 19)).onAreaChange((_, area: Area) => this.rememberSurface('card:' + c.id + '',  19, area)).shadow(this.surfaceShadow('card:' + c.id + '')).borderRadius(this.round(19, 'card:' + c.id + '')).border({ width: this.styleEdgeWidth(), color: this.line() })
      .onClick(() => this.select(c))
  }
  @Builder
  WorkspaceHeader() {
    Column() {
      this.Greeting()
      Column() { if (this.section === '概览') { this.Hero() } else { this.PageIntro() } }.width('100%').margin({ top: 24, bottom: 28 })
      this.CollectionHeader()
    }.width('100%').padding({ bottom: 16 })
  }
  @Builder
  WorkspaceFooter() {
    Column() {
      if (this.queryPhase === 'loading' && !this.queryRefreshing) {
        Text(this.t('正在查找…')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(13).fontColor(this.muted())
          .width('100%').height(160).textAlign(TextAlign.Center)
      } else if (this.queryPhase === 'failed') {
        Column({ space: 12 }) {
          Text(this.t('查询未完成')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(13).fontColor(this.muted())
          Text(this.t('重试')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(11).fontColor(this.accent()).onClick(() => this.queries.retry())
        }.width('100%').height(160).justifyContent(FlexAlign.Center)
      } else if (this.visibleCards().length === 0) {
        Column({ space: 12 }) {
          this.Glyph(() => 0xf013c,  24, () => this.muted()); Text(this.t('没有找到匹配的想法')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(13).fontColor(this.muted())
          Text(this.t('查看全部')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(11).fontColor(this.accent()).onClick(() => { this.search = ''; this.navigate('概览'); })
        }.width('100%').height(160).justifyContent(FlexAlign.Center).backgroundColor(this.glass()).borderRadius(this.round(20))
      }
      Row({ space: 12 }) {
        this.Glyph(() => 0xf00d,  17, () => this.muted())
        TextInput({ text: this.capture, placeholder: this.t('脑海中闪过了什么？') }).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(11).placeholderFont({ size: 11 }).fontColor(this.ink()).placeholderColor(this.muted())
          .padding(0).height(46).backgroundColor(Color.Transparent).layoutWeight(1).onChange((s: string) => { this.capture = s; }).onSubmit(() => this.captureIdea())
        Button({ type: ButtonType.Normal }) { this.Glyph(() => 0xf57e,  17, () => this.accent()) }
          .width(30).height(30).borderRadius(this.round(10)).backgroundColor('#147662BA').accessibilityText('记录想法').onClick(() => this.captureIdea())
      }.padding({ left: 16, right: 12 }).width('100%').backgroundColor(this.glass()).backdropBlur(this.materialBlur('quick')).linearGradient(this.gradient('quick')).overlay(this.Rim('quick', 15)).onAreaChange((_, area: Area) => this.rememberSurface('quick',  15, area)).shadow(this.surfaceShadow('quick'))
        .borderRadius(this.round(15, 'quick')).border({ width: this.styleEdgeWidth(), color: this.line() }).margin({ top: 22, bottom: 94 })
    }.width('100%')
  }
  @Builder
  WorkspaceCardContent(card: CardView) {
    if (this.section === '概览') { this.IdeaCard(card) }
    else { this.SpecializedCard(card) }
  }
  @Builder
  Workspace() {
    Stack({ alignContent: Alignment.Bottom }) {
      Scroll(this.workspaceScroller) {
        // Keep one viewport. Native lazy layout measures visible cards and
        // nearby prefetch; the header/footer cannot sit in a parent Column.
        LazyVWaterFlowLayout() {
          LazyForEach(this.workspaceCards, (item: WorkspaceCard) => {
            Column() {
              WorkspaceCardSlot({ value: item, content: this.WorkspaceCardContent.bind(this) })
            }.width('100%').id('workspace-card:' + item.card.id)
          }, (item: WorkspaceCard) => item.key)
        }.columnsTemplate((this.section === '概览' && this.contentWidth >= 460) || (this.section === '进行中' && this.contentWidth >= 660) ? '1fr 1fr' : '1fr')
          .columnsGap(LengthMetrics.vp(14)).rowsGap(LengthMetrics.vp(14))
          .header(this.WorkspaceHeader).footer(this.WorkspaceFooter).width('100%')
      }.scrollBar(BarState.Off).width('100%').height('100%').align(Alignment.TopStart)
        .onAppear(() => this.restoreWorkspace()).onDidScroll(() => this.rememberWorkspace())
      Row({ space: 9 }) {
        Circle().width(6).height(6).fill(this.accent())
        Text(this.t('没有紧迫的事。给好奇心一点时间。')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(11).fontColor(this.muted()).layoutWeight(1)
        this.Glyph(() => 0xf06f9,  17, () => this.muted())
      }.width('100%').height(48).padding({ left: 16, right: 16 }).borderRadius(this.round(14, 'footer'))
        .backgroundColor(this.material('footer') ? alpha(this.material('footer')?.color || this.surface(), (this.material('footer')?.opacity ?? 0) / 100) : Color.Transparent).backdropBlur(this.material('footer')?.blur ?? 0).overlay(this.Rim('footer', 14)).onAreaChange((_, area: Area) => this.rememberSurface('footer', 14, area)).shadow(this.surfaceShadow('footer')).hitTestBehavior(HitTestMode.None)
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
              .fontColor(this.accent()).backgroundColor(Color.Transparent).border({ width: this.styleEdgeWidth(), color: this.line(), radius: this.round(10) })
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
      .border({ width: this.styleEdgeWidth(), color: this.line() }).overlay(this.Rim('card:' + c.id)).onAreaChange((_, area: Area) => this.rememberSurface('card:' + c.id, 20, area)).shadow(this.surfaceShadow('card:' + c.id)).onClick(() => this.select(c))
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
  StylePicker() {
    Column({ space: 12 }) {
      Row({ space: 12 }) {
        this.Glyph(() => 0xf0258, 19, () => this.accent())
        Column({ space: 4 }) {
          Text(this.t('界面风格')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(12).fontWeight(FontWeight.Medium).fontColor(this.ink())
          Text(this.t(visualStyleLabels[visualStyles.indexOf(this.visualStyle)])).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(10).fontColor(this.muted())
        }.alignItems(HorizontalAlign.Start).layoutWeight(1)
        Text(this.stylePickerOpen ? '⌃' : '⌄').fontSize(20).fontColor(this.muted())
      }.padding(13).width('100%').border({ width: this.styleEdgeWidth(), color: this.line(), radius: this.round(13) })
        .accessibilityText('界面风格 · ' + visualStyleLabels[visualStyles.indexOf(this.visualStyle)])
        .onClick(() => { this.stylePickerOpen = !this.stylePickerOpen; })
      if (this.stylePickerOpen) {
        ForEach(visualStyles, (style: string, i: number) => {
          Row({ space: 12 }) {
            Stack() {
              Text('Aa').fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(18).fontColor(this.ink())
              SurfaceRelief({ style: style, depth: this.styleDepth, dark: this.dark, radius: 13 * styleRadiusScale(style) })
            }.width(58).height(50).backgroundColor(alpha(this.surface(), .6)).borderRadius(13 * styleRadiusScale(style))
              .border({ width: style === 'brutalist' ? 1.8 : 1, color: style === 'brutalist' ? this.ink() : this.line() })
              .shadow(this.surfaceShadow('', style)).hitTestBehavior(HitTestMode.None)
            Column({ space: 4 }) {
              Text(this.t(visualStyleLabels[i])).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(12).fontColor(this.ink())
              Text(this.t(['轻盈边界，清晰层次', '柔和双向光影，细腻浮雕触感', '哑光纸面、细边框与克制的层次', '圆润造型与柔和隆起的色彩', '半透明层次与强调色线条', '硬朗边角、醒目边框与错位阴影', '金属质感面板、紧凑控件与利落线条'][i]))
                .fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(10).fontColor(this.muted())
              if (i > 1) { Text(this.t('实验性')).fontSize(9).fontColor(this.accent()) }
            }.alignItems(HorizontalAlign.Start).layoutWeight(1)
            if (this.visualStyle === style) { Text('✓').fontSize(17).fontColor(this.accent()) }
          }.padding(12).width('100%').backgroundColor(alpha(this.accent(), this.visualStyle === style ? .055 : .015))
            .borderRadius(this.round(13)).border({ width: 1, color: this.visualStyle === style ? alpha(this.accent(), .4) : this.line() })
            .onClick(() => { this.visualStyle = style; this.stylePickerOpen = false; this.persistAppearance(); })
        })
      }
      if (this.visualStyle !== 'flat') {
        this.SettingLabel('立体深度', () => Math.round(this.styleDepth * 100) + '%')
        this.SettingsSlider(() => this.styleDepth * 100, 200, (v: number) => { this.styleDepth = v / 100; this.persistAppearance(); })
        Text(this.t('调整浮起与下沉的强度，不改变玻璃透明度。')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(10).fontColor(this.muted()).width('100%')
        Button(this.t('恢复 100%'), { type: ButtonType.Normal }).fontSize(11).fontColor(this.accent()).backgroundColor(Color.Transparent)
          .onClick(() => { this.styleDepth = 1; this.persistAppearance(); })
      }
    }.width('100%').margin({ top: 16 })
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
      this.StylePicker()
      Column({ space: 4 }) {
        Text(this.t('界面语言')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(10).fontColor(this.muted()).width('100%')
        Select([{ value: '简体中文' }, { value: 'English' }, { value: '日本語' }, { value: '한국어' }, { value: 'Deutsch' }, { value: 'Français' }, { value: 'Español' }, { value: 'Português' }, { value: 'Русский' }])
          .value(this.languageName()).font({ size: 14 }).fontColor(this.ink()).backgroundColor(Color.Transparent).width('100%')
          .onSelect((i: number) => { this.locale = ['zh', 'en', 'ja', 'ko', 'de', 'fr', 'es', 'pt', 'ru'][i]; this.persistAppearance(); })
      }.width('100%').margin({ top: 23 }).padding(8).border({ width: this.styleEdgeWidth(), color: this.line(), radius: this.round(12) })
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
        .border({ width: this.styleEdgeWidth(), color: this.line(), radius: this.round(11) }).margin({ top: 16, bottom: 14 }).onClick(() => this.openSettings('组件与卡片'))
      Stack() {
        Circle().width(58).height(58).fill('#C9B4E0').position({ x: 20, y: 13 })
        Circle().width(76).height(76).fill('#D7E5D9').position({ right: 12, bottom: -10 })
        Column() { this.Glyph(() => this.mode === '磨砂' ? 0xf5c9 : this.mode === '超透' ? 0xf0695 : 0xe374,  19, () => this.ink()); ContentSlot(this.preview) }.justifyContent(FlexAlign.Center).width('38%').height(75).borderRadius(this.round(13)).backdropBlur(this.materialBlur()).linearGradient(this.gradient()).overlay(this.Rim('preview', 13)).onAreaChange((_, area: Area) => this.rememberSurface('preview', 13, area)).shadow(this.surfaceShadow(''))
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
          .border({ width: this.styleEdgeWidth(), color: this.line(), radius: this.round(11) }).onClick(() => { this.colorTarget = 'theme'; this.colorDraft = this.themeColor || '#7662BA'; this.openSettings('色彩罗盘'); })
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
            .backgroundColor(this.canvasBackground === bg ? alpha(this.accent(), .14) : Color.Transparent).border({ width: this.styleEdgeWidth(), color: this.line() })
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
          .border({ width: this.styleEdgeWidth(), color: this.line(), radius: this.round(10) }).enabled(false)
      }
      Text(this.canvasBackground === '默认' ? '流动的光晕，为灵感留一点色彩。' : this.canvasBackground === '纯色' ? '一张安静的纯色画布。' : this.canvasBackground === '纹理' ? '细密的纸感网点，让空间多一点触感。' : '移动端使用主题底色；透明参数作用于应用画布。').fontFamily(this.fontFamily || 'HarmonyOS Sans')
        .fontSize(9).lineHeight(15).fontColor(this.muted()).margin({ top: 10 }).width('100%')
      Text(this.appearanceStatus || '✓ 外观设置自动保存在本机').fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(9).fontColor(this.muted()).margin({ top: 13 }).width('100%')
    }.alignItems(HorizontalAlign.Start).padding(19).width('100%').borderRadius(this.round(22, 'appearance'))
      .backdropBlur(this.materialBlur('appearance')).linearGradient(this.gradient('appearance')).overlay(this.Rim('appearance', 22)).onAreaChange((_, area: Area) => this.rememberSurface('appearance',  22, area)).shadow(this.surfaceShadow('appearance')).border({ width: this.styleEdgeWidth(), color: this.line() })
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
    }.padding(19).width('100%').borderRadius(this.round(22, 'daily')).backdropBlur(this.materialBlur('daily')).linearGradient(this.gradient('daily')).overlay(this.Rim('daily', 22)).onAreaChange((_, area: Area) => this.rememberSurface('daily',  22, area)).shadow(this.surfaceShadow('daily')).border({ width: this.styleEdgeWidth(), color: this.line() })
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
    }.padding(16).width('100%').borderRadius(this.round(22, 'music')).backdropBlur(this.materialBlur('music')).linearGradient(this.gradient('music')).overlay(this.Rim('music', 22)).onAreaChange((_, area: Area) => this.rememberSurface('music',  22, area)).shadow(this.surfaceShadow('music')).border({ width: this.styleEdgeWidth(), color: this.line() })
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
    if (field === 'mode') { v.mode = value; } else if (field === 'enabled') { v.enabled = value === 'true'; }
    else if (field === 'follow') { v.follow = value; } else { v.color = value; }
    this.materialDraft = v;
  }
  private materialIds(): string[] {
    return Array.from(new Set<string>(['navigation', 'search', 'hero', 'appearance', 'daily', 'music', 'footer', 'quick', 'summary',
      ...this.cards.filter((c: CardView) => !c.deleted).map((c: CardView) => 'card:' + c.id), ...this.materials.map((m: MaterialChoice) => m.id)]));
  }
  private followIds(): string[] {
    return ['', ...Array.from(new Set<string>([...this.materialIds(), this.materialDraft.follow]))
      .filter((id: string) => id.length > 0 && canFollow(this.materials, this.materialDraft.id, id))];
  }
  private previewMaterials(): MaterialChoice[] {
    return [...this.materials.filter((m: MaterialChoice) => m.id !== this.materialDraft.id), this.materialDraft];
  }
  private previewMaterial(): MaterialChoice | undefined { return resolveMaterial(this.previewMaterials(), this.materialDraft.id); }
  private editingOwnMaterial(): boolean { return this.materialDraft.enabled && this.materialDraft.follow.length === 0; }
  private previewRadius(): number { return this.previewMaterial()?.radius ?? this.radius * styleRadiusScale(this.visualStyle); }
  private previewDepth(): number { const depth = this.previewMaterial()?.depth ?? -1; return depth < 0 ? this.styleDepth : depth; }
  private previewMode(): string { const m = this.previewMaterial(); return m && m.mode !== '跟随主题' ? m.mode : this.mode; }
  private followDescription(id: string): string {
    const m = this.materials.find((v: MaterialChoice) => v.id === id);
    if (m?.follow) { return '跟随 · ' + this.materialTitle(m.follow); }
    return this.material(id) ? this.modeFor(id) + ' · ' + Math.round(this.material(id)?.opacity ?? 76) + '%' : '跟随主题';
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
      ForEach(this.materialIds(), (id: string) => {
        Row({ space: 12 }) {
          this.Glyph(() => id.startsWith('card:') ? 0xf0023 : 0xf0258,  20, () => this.accent())
          Column({ space: 5 }) {
            Text(this.materialTitle(id)).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(13).fontColor(this.ink()).maxLines(2)
            Text(this.followDescription(id)).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(10).fontColor(this.muted())
          }.alignItems(HorizontalAlign.Start).layoutWeight(1)
          Text(this.t('›')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(23).fontColor(this.muted())
        }.padding(20).width('100%').borderRadius(16).linearGradient(this.gradient('', true)).border({ width: this.styleEdgeWidth(), color: this.line() }).onClick(() => this.beginMaterial(id))
      })
    }.width('100%')
  }
  @Builder
  MaterialEditor() {
    Column({ space: 18 }) {
      this.SettingLabel('材质来源')
      Select(this.followIds().map((id: string): SelectOption => { return { value: id ? this.materialTitle(id) : '自身材质' }; }))
        .value(this.materialDraft.follow ? this.materialTitle(this.materialDraft.follow) : '自身材质').font({ size: 13 })
        .fontColor(this.ink()).backgroundColor(Color.Transparent).onSelect((i: number) => this.changeMaterial('follow', this.followIds()[i]))
      if (this.materialDraft.follow) {
        Text('跟随目标的完整材质；目标未自定义时继承主题。循环引用不可选择。').fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(11).fontColor(this.muted())
      }
      Row() {
        Text(this.t('自定义材质')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(14).fontColor(this.ink()).layoutWeight(1)
        Toggle({ type: ToggleType.Switch, isOn: this.materialDraft.enabled }).selectedColor(this.accent()).enabled(this.materialDraft.follow.length === 0).onChange((v: boolean) => this.changeMaterial('enabled', v.toString()))
      }.width('100%')
      Stack() {
        Circle().width(110).height(110).fill('#CAB5E1').position({ left: 14, top: 3 })
        Circle().width(90).height(90).fill('#C5DCCB').position({ right: 14, bottom: 0 })
        Column({ space: 12 }) { Text(this.t('一个想法，正悄悄发芽。')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(16).fontColor(this.ink()); Text(this.t('给尚未成形的念头一点空间。')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(11).fontColor(this.muted()) }
          .justifyContent(FlexAlign.Center).width('88%').height(110).borderRadius(this.previewRadius())
          .backdropBlur(this.previewMaterial()?.blur ?? this.materialBlur())
          .linearGradient(this.materialGradient(this.previewMaterial(), this.previewMode()))
          .border({ width: this.styleEdgeWidth(), color: this.line() })
          .overlay(this.DraftRim()).onAreaChange((_, area: Area) => this.rememberSurface('material-preview', 0, area)).shadow(this.surfaceShadow('', this.visualStyle, this.previewDepth()))
      }.height(150).width('100%').clip(true)
      Column({ space: 18 }) {
      Select(['跟随主题', '磨砂', '超透', '液体玻璃'].map((s: string): SelectOption => { return { value: this.t(s) }; }))
        .value(this.t(this.materialDraft.mode)).font({ size: 13 }).fontColor(this.ink()).backgroundColor(Color.Transparent)
        .onSelect((i: number) => this.changeMaterial('mode', ['跟随主题', '磨砂', '超透', '液体玻璃'][i]))
      this.SettingLabel('模糊', () => Math.round(this.materialDraft.blur).toString())
      this.SettingsSlider(() => this.materialDraft.blur, 40, (v: number) => this.setMaterialValue('blur', v))
      this.SettingLabel('不透明度', () => Math.round(this.materialDraft.opacity) + '%')
      this.SettingsSlider(() => this.materialDraft.opacity, 100, (v: number) => this.setMaterialValue('opacity', v))
      this.SettingLabel('圆角', () => Math.round(this.materialDraft.radius).toString())
      this.SettingsSlider(() => this.materialDraft.radius, 32, (v: number) => this.setMaterialValue('radius', v))
      if (this.visualStyle !== 'flat') {
        this.SettingLabel('立体深度', () => Math.round(this.previewDepth() * 100) + '%')
        this.SettingsSlider(() => this.previewDepth() * 100, 200, (v: number) => this.setMaterialValue('depth', v / 100))
        Button(this.t('跟随主题'), { type: ButtonType.Normal }).fontSize(12).fontColor(this.accent()).backgroundColor(Color.Transparent)
          .onClick(() => this.setMaterialValue('depth', -1))
      }
      Button(this.t('自定义颜色'), { type: ButtonType.Normal }).fontSize(12).borderRadius(this.round(11)).fontColor(this.accent()).backgroundColor(Color.Transparent).onClick(() => {
        this.colorTarget = 'material'; this.colorDraft = this.materialDraft.color || this.surface(); this.settingsPage = '色彩罗盘';
      })
      }.width('100%').enabled(this.editingOwnMaterial()).opacity(this.editingOwnMaterial() ? 1 : .5)
      Text(this.t('预览中的改动仅在应用后保存。关闭自定义即可重新跟随全局主题。')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(11).lineHeight(19).fontColor(this.muted())
      Flex({ wrap: FlexWrap.Wrap, justifyContent: FlexAlign.End }) {
        Button(this.t('恢复默认'), { type: ButtonType.Normal }).fontSize(12).borderRadius(this.round(11)).backgroundColor(Color.Transparent).fontColor(this.accent()).onClick(() => { const v = new MaterialChoice(); v.id = this.materialDraft.id; this.materialDraft = v; })
        Button(this.t('取消'), { type: ButtonType.Normal }).fontSize(12).borderRadius(this.round(11)).backgroundColor(Color.Transparent).fontColor(this.accent()).onClick(() => this.backSettings())
        Button(this.t('应用'), { type: ButtonType.Normal }).fontSize(12).borderRadius(this.round(11)).backgroundColor(this.accent()).fontColor(this.dark ? '#302D43' : '#FFFFFF').onClick(() => this.applyMaterial())
      }.width('100%')
    }.alignItems(HorizontalAlign.Start).padding(24).width('100%').borderRadius(this.round(20)).linearGradient(this.gradient('', true))
  }
  @Builder
  DraftRim() {
    Stack() {
      SurfaceRelief({ style: this.visualStyle, depth: this.previewDepth(), dark: this.dark, radius: this.previewRadius() })
      if (this.previewMode() === '液体玻璃') { GlassRim({ dark: this.dark, radius: this.previewRadius() }) }
    }.width(this.surfaceAreas['material-preview:0']?.[0] ?? 0).height(this.surfaceAreas['material-preview:0']?.[1] ?? 0).clip(true).hitTestBehavior(HitTestMode.None)
  }
  @Builder
  FontSettings() {
    Column({ space: 20 }) {
      Text(this.t('为你的空间选择字体')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(18).fontColor(this.ink())
      Text('当前字体：' + (this.fontFamily || '系统默认')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(12).borderRadius(this.round(11)).fontColor(this.muted())
      TextInput({ text: this.fontDraft, placeholder: this.t('输入系统字体名称') }).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(14).fontColor(this.ink()).backgroundColor(alpha(this.surface(), .6)).maxLength(128).onChange((v: string) => { this.fontDraft = v; })
      Select(this.systemFonts().map((s: string): SelectOption => { return { value: s }; }))
        .value(this.fontDraft || this.t('选择系统字体')).font({ size: 13 }).fontColor(this.ink()).backgroundColor(Color.Transparent).onSelect((_: number, s: string) => { this.fontDraft = s; })
      Text(this.t('一个想法，正悄悄发芽。\nA little space for big ideas.\n0123456789')).fontFamily(this.fontDraft || 'HarmonyOS Sans').fontSize(22).lineHeight(35).fontColor(this.ink()).width('100%').padding(20).border({ width: this.styleEdgeWidth(), color: this.line(), radius: 16 })
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
  BodyInput() {
    TextArea({ text: this.description, placeholder: this.t('写下思路，或粘贴一段内容…'), controller: this.descriptionController })
      .fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(13).placeholderFont({ size: 13 }).fontColor(this.ink()).placeholderColor(this.muted())
      .backgroundColor(Color.Transparent).border({ width: this.styleEdgeWidth(), color: this.line() }).borderRadius(this.round(12)).height(180).maxLength(20000)
      .enabled(!this.draftRetiring && !this.draftRetirementUnknown).id('draft-description')
      .onChange((s: string, preview?: PreviewText) => this.editorInputChanged('description', s, preview))
      .onTextSelectionChange((start: number, end: number) => this.draftSelectionChanged('description', start, end))
      .onFocus(() => this.focusDraftField('description')).onBlur(() => this.blurDraftField('description'))
  }
  @Builder
  Drafts() {
    Column({ space: 16 }) {
      Row() {
        Text('保留的草稿').fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(20).fontColor(this.ink()).layoutWeight(1)
        Button('关闭', { type: ButtonType.Normal }).fontSize(12).fontColor(this.accent()).backgroundColor(Color.Transparent).onClick(() => { this.draftsOpen = false; })
      }.width('100%')
      Text('继续未完成的文字。打开草稿不会提交卡片。').fontSize(12).fontColor(this.muted()).width('100%')
      Scroll() {
        Column({ space: 12 }) {
          if (this.draftRecords.length === 0) { Text('还没有保留的草稿').fontSize(13).fontColor(this.muted()).padding(24) }
          ForEach(this.draftRecords, (record: DraftRecord) => {
            Column({ space: 8 }) {
              Text(record.values.title.text || '还没起名字的想法').fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(15).fontColor(this.ink()).maxLines(2)
              Text(record.values.description.text || '先保留，慢慢整理。').fontSize(12).fontColor(this.muted()).maxLines(3)
              Row() {
                Text(record.scope.source_kind === 1 ? '新想法' : '继续修改').fontSize(10).fontColor(this.muted()); Blank()
                Button('恢复编辑', { type: ButtonType.Normal }).fontSize(12).fontColor(this.accent()).backgroundColor(Color.Transparent).onClick(() => this.restoreDraft(record))
              }.width('100%')
            }.alignItems(HorizontalAlign.Start).width('100%').padding(16).backgroundColor('#0B7662BA').borderRadius(this.round(14))
              .id('draft-row:' + record.scope.draft_id)
          }, (record: DraftRecord) => record.scope.draft_id + ':' + record.generation)
        }.width('100%')
      }.layoutWeight(1).width('100%').scrollBar(BarState.Off).align(Alignment.TopStart)
    }.padding(24).width(Math.min(620, this.viewportWidth - 40)).height(Math.min(560, this.viewportHeight - 48))
      .backgroundColor(alpha(this.surface(), .96)).borderRadius(this.round(24)).border({ width: this.styleEdgeWidth(), color: this.line() }).clip(true)
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
          TextInput({ text: this.title, placeholder: this.t('给它起个名字'), controller: this.titleController }).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(23).placeholderFont({ size: 23 }).fontWeight(FontWeight.Medium)
            .fontColor(this.ink()).placeholderColor(this.muted()).backgroundColor(Color.Transparent).padding(16).maxLength(60).height(64)
            .border({ width: 1, color: this.accent() }).borderRadius(this.round(12))
            .enabled(!this.draftRetiring && !this.draftRetirementUnknown).id('draft-title')
            .onChange((s: string, preview?: PreviewText) => this.editorInputChanged('title', s, preview))
            .onTextSelectionChange((start: number, end: number) => this.draftSelectionChanged('title', start, end))
            .onFocus(() => this.focusDraftField('title')).onBlur(() => this.blurDraftField('title'))
          Text(`${this.title.length} / 60`).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(10).fontColor(this.muted()).width('100%').textAlign(TextAlign.End)
          Row({ space: 8 }) {
            Button({ type: ButtonType.Normal }) { Row({ space: 6 }) { this.Glyph(() => 0xe192,  16, () => this.muted()); Text(this.t('粘贴内容')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(13).fontColor(this.muted()) } }
              .height(40).padding({ left: 16, right: 16 }).borderRadius(this.round(12)).backgroundColor('#217662BA').enabled(false)
            Button({ type: ButtonType.Normal }) { Row({ space: 6 }) { this.Glyph(() => 0xe0b1,  16, () => this.muted()); Text(this.t('导入文件')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(13).fontColor(this.muted()) } }
              .height(40).padding({ left: 16, right: 16 }).borderRadius(this.round(12)).backgroundColor(Color.Transparent).border({ width: this.styleEdgeWidth(), color: this.line() }).enabled(false)
          }.justifyContent(FlexAlign.Center).width('100%')
          Text(this.t('富文本粘贴与附件导入正在适配；纯文字可使用输入框的粘贴菜单。')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(11).lineHeight(18).fontColor(this.muted()).width('100%')
          Column({ space: 10 }) {
            Row({ space: 8 }) {
              this.Glyph(() => 0xf0023,  18, () => this.accent()); Text(this.t('正文 · Markdown')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(14).fontWeight(FontWeight.Medium).fontColor(this.ink()); Blank()
              Button({ type: ButtonType.Normal }) { this.Glyph(() => 0xf4a1,  19, () => this.previewBody ? this.accent() : this.muted()) }
                .width(32).height(32).backgroundColor(Color.Transparent).accessibilityText(this.previewBody ? '隐藏预览' : '实时预览').onClick(() => { this.previewBody = !this.previewBody; })
            }.width('100%').height(32)
            Divider().color(this.line())
            if (this.previewBody && Math.min(820, this.viewportWidth - 40) - 80 >= 590) {
              Row({ space: 24 }) {
                Column() { this.BodyInput() }.layoutWeight(1)
                Column() { this.BodyPreview() }.layoutWeight(1)
              }.width('100%').alignItems(VerticalAlign.Top).id('draft-body-split')
            } else if (this.previewBody) { this.BodyPreview() }
            else { this.BodyInput() }
          }.padding(16).width('100%').backgroundColor('#077662BA').borderRadius(this.round(16)).border({ width: this.styleEdgeWidth(), color: this.line() })
          Row({ space: 12 }) {
            Text(this.t('放在哪里')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(11).fontColor(this.muted())
            Select([{ value: '灵感' }, { value: '进行中' }, { value: '实验' }]).value(this.current()?.category ?? this.category)
              .font({ size: 12 }).fontColor(this.ink()).backgroundColor('#147662BA').borderRadius(this.round(10))
              .enabled(!this.busy && !this.draftWorking && !this.draftRetiring && !this.draftRetirementUnknown && this.pending.length === 0 && !this.current()?.deleted)
              .onSelect((_: number, value: string) => {
                if (this.draftWorking || this.draftRetiring || this.draftRetirementUnknown || this.draftRestoreInput) { return; }
                if (!this.current()) {
                  this.category = value; this.dirty = true; this.editorValues.category = value;
                  this.editorValues.stage = value === '进行中' ? '计划中' : value === '实验' ? '待验证' : '待整理';
                  this.editorDraft?.update(copyValues(this.editorValues)); return;
                }
                if (this.dirty) { this.message = '请先保存正文编辑'; return; }
                const cmd = this.command('category'); cmd.category = value;
                cmd.stage = value === '进行中' ? '计划中' : value === '实验' ? '待验证' : '待整理'; this.category = value; this.submit(cmd);
              })
            Blank()
            if (this.current()) {
              Select(this.stages()).value(this.current()?.stage ?? '待整理').font({ size: 11 }).fontColor(this.muted()).backgroundColor(Color.Transparent)
                .enabled(!this.busy && !this.draftWorking && !this.draftRetiring && !this.draftRetirementUnknown && this.pending.length === 0 && !this.current()?.deleted)
                .onSelect((_: number, value: string) => {
                  if (this.draftWorking || this.draftRetiring || this.draftRetirementUnknown || this.draftRestoreInput) { return; }
                  if (this.dirty) { this.message = '请先保存正文编辑'; return; }
                  const cmd = this.command('stage'); cmd.stage = value; this.submit(cmd);
                })
            }
          }.width('100%')
          if (this.category === '实验') {
            TextArea({ text: this.hypothesis, placeholder: this.t('假设'), controller: this.hypothesisController }).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(12).placeholderFont({ size: 12 }).fontColor(this.ink()).backgroundColor(this.glass()).maxLength(5000).height(96)
              .enabled(!this.draftRetiring && !this.draftRetirementUnknown).id('draft-hypothesis')
              .onChange((s: string, preview?: PreviewText) => this.editorInputChanged('hypothesis', s, preview))
              .onTextSelectionChange((start: number, end: number) => this.draftSelectionChanged('hypothesis', start, end)).onFocus(() => this.focusDraftField('hypothesis')).onBlur(() => this.blurDraftField('hypothesis'))
            TextArea({ text: this.conclusion, placeholder: this.t('结论'), controller: this.conclusionController }).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(12).placeholderFont({ size: 12 }).fontColor(this.ink()).backgroundColor(this.glass()).maxLength(10000).height(116)
              .enabled(!this.draftRetiring && !this.draftRetirementUnknown).id('draft-conclusion')
              .onChange((s: string, preview?: PreviewText) => this.editorInputChanged('conclusion', s, preview))
              .onTextSelectionChange((start: number, end: number) => this.draftSelectionChanged('conclusion', start, end)).onFocus(() => this.focusDraftField('conclusion')).onBlur(() => this.blurDraftField('conclusion'))
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
                  .fontSize(18).fontColor(this.accent()).width(30).height(30).padding(0).backgroundColor('#147662BA').borderRadius(8)
                  .onClick(() => this.mutate('task_toggle', task)).enabled(this.canEditTasks())
                Text(task.text).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(12).fontColor(this.ink()).layoutWeight(1)
                Button('···', { type: ButtonType.Normal }).fontSize(20).fontColor(this.muted())
                  .width(36).height(36).padding(0).backgroundColor(Color.Transparent).accessibilityText('待办操作：' + task.text)
                  .enabled(this.canEditTasks()).bindMenu([
                    { value: this.t('重命名'), action: () => this.renameTask(task) },
                    { value: this.t('上移'), enabled: this.current()?.tasks[0]?.id !== task.id, action: () => this.moveTask(task, -1) },
                    { value: this.t('下移'), enabled: this.current()?.tasks[this.current()!.tasks.length - 1]?.id !== task.id, action: () => this.moveTask(task, 1) },
                    { value: this.t('移除待办'), action: () => this.confirmTaskChange(task) }
                  ])
              }.width('100%')
              if (this.taskEditId === task.id) {
                Column({ space: 8 }) {
                  TextInput({ text: this.taskRenameText, placeholder: this.t('重命名') }).fontFamily(this.fontFamily || 'HarmonyOS Sans')
                    .fontSize(12).fontColor(this.ink()).backgroundColor(this.glass()).maxLength(500)
                    .enabled(!this.busy && this.pending.length === 0).onChange((s: string) => { this.taskRenameText = s; })
                  Row({ space: 8 }) {
                    Blank()
                    Button(this.t('取消')).fontSize(12).fontColor(this.muted()).backgroundColor(Color.Transparent)
                      .enabled(!this.busy && this.pending.length === 0).onClick(() => { this.taskEditId = ''; this.taskRenameText = ''; })
                    Button(this.t('保存')).fontSize(12).backgroundColor(this.accent())
                      .enabled(!this.busy && this.pending.length === 0 && !this.dirty && this.taskRenameText.trim().length > 0)
                      .onClick(() => this.mutate('task_rename', task))
                  }.width('100%')
                }.width('100%').padding({ left: 40, bottom: 8 })
              }
            }, (task: TaskView) => task.id + ':' + task.completion.toString() + ':' + task.text)
            if (!this.current()?.deleted) {
              Row({ space: 8 }) {
                TextInput({ text: this.taskText, placeholder: this.t('添加一个小步骤…'), controller: this.taskController }).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(12).placeholderFont({ size: 12 }).fontColor(this.ink()).backgroundColor(this.glass()).maxLength(500)
                  .enabled(!this.draftRetiring && !this.draftRetirementUnknown).id('draft-todos')
                  .onChange((s: string, preview?: PreviewText) => this.editorInputChanged('todos', s, preview))
                  .onTextSelectionChange((start: number, end: number) => this.draftSelectionChanged('todos', start, end)).onFocus(() => this.focusDraftField('todos')).onBlur(() => this.blurDraftField('todos')).layoutWeight(1)
                Button(this.t('+'), { type: ButtonType.Normal }).fontSize(21).fontColor(this.accent()).width(36).height(36).padding(0).backgroundColor('#147662BA').borderRadius(10)
                  .onClick(() => this.mutate('task_add')).enabled(this.canEditTasks())
              }
              if ((this.current()?.tasks.length ?? 0) > 0) {
                Button(this.t('全部勾选并设置阶段'), { type: ButtonType.Normal }).fontSize(11).fontColor(this.accent())
                  .backgroundColor(Color.Transparent).enabled(this.canEditTasks()).onClick(() => this.confirmTaskChange())
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
      }.layoutWeight(1).width('100%').scrollBar(BarState.Off).align(Alignment.TopStart)
      if (this.message.length > 0) { Text(this.message).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(11).fontColor(this.accent()).padding({ left: 24, right: 24, bottom: 12 }).width('100%') }
      if (this.draftStatus.length > 0) {
        Row() {
          Text(this.draftStatus).fontSize(10).fontColor(this.muted()).layoutWeight(1).id('draft-status')
          if (this.editorDraft?.unknown) { Button('核对草稿', { type: ButtonType.Normal }).fontSize(11).fontColor(this.accent()).backgroundColor(Color.Transparent).enabled(!this.draftWorking).onClick(() => this.retryDraft()) }
        }.width('100%').padding({ left: 24, right: 24, bottom: 8 })
      }
      if (this.draftRetirementUnknown) {
        Button('核对草稿清理', { type: ButtonType.Normal }).fontSize(11).fontColor(this.accent()).backgroundColor(Color.Transparent)
          .enabled(!this.draftWorking && !this.busy).onClick(() => this.retryDraftRetirement())
      }
      Divider().color(this.line())
      Row({ space: 12 }) {
        Button('放弃草稿', { type: ButtonType.Normal }).fontSize(10).fontColor(this.muted()).backgroundColor(Color.Transparent)
          .enabled(!this.busy && !this.draftWorking && !this.draftRetirementUnknown && !this.editorDraft?.unknown && this.pending.length === 0).onClick(() => this.discardDraft())
        Blank()
        Button('保留草稿', { type: ButtonType.Normal }).fontSize(12).height(38).fontColor(this.accent()).backgroundColor(Color.Transparent).onClick(() => this.closeEditor())
        Button(this.busy ? '保存中…' : '保存灵感', { type: ButtonType.Normal }).fontSize(12).height(38).padding({ left: 22, right: 22 })
          .fontColor(this.dark ? '#302D43' : '#FFFFFF').backgroundColor(this.accent()).borderRadius(this.round(10))
          .enabled(this.ready && !this.busy && !this.draftWorking && !this.draftConflict && !this.draftCaptureIncomplete && !this.draftRetirementUnknown && !this.editorDraft?.unknown && this.pending.length === 0 && !this.current()?.deleted).onClick(() => this.save())
      }.padding({ left: 24, right: 24, top: 14, bottom: 14 }).width('100%')
      if (this.pending.length > 0 && !this.busy) {
        Button(this.t('使用原操作重试 / 核对')).fontSize(12).backgroundColor(this.accent()).margin({ bottom: 12 }).onClick(() => this.retry())
      }
    }.width(Math.min(820, this.viewportWidth - 40)).height(Math.max(160, Math.min(760, this.viewportHeight - 40))).backgroundColor(alpha(this.surface(), .9))
      .backdropBlur(this.materialBlur()).linearGradient(this.gradient('', true)).overlay(this.Rim('', 28)).onAreaChange((_, area: Area) => this.rememberSurface('',  28, area)).shadow(this.surfaceShadow('')).borderRadius(this.round(28)).border({ width: this.styleEdgeWidth(), color: this.line() }).clip(true)

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
          if (this.viewportWidth >= 760 && this.sidebarExpanded) { this.Navigation() }
          Column({ space: 22 }) {
            this.Header()
            Row({ space: 24 }) {
              this.Workspace()
              if (this.viewportWidth >= 1050 && this.showAppearance) {
                Scroll() { Column({ space: 20 }) { this.Appearance(); this.DailyPanel(); this.MusicPanel(); this.Quote() } }.width(252).height('100%').scrollBar(BarState.Off).align(Alignment.TopStart)
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
      if (this.draftsOpen) {
        Stack() {
          Rect().width('100%').height('100%').fill('#40302D43').onClick(() => { this.draftsOpen = false; })
          this.Drafts()
        }.width('100%').height('100%')
      } else if (this.editorOpen) {
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
          .border({ width: this.styleEdgeWidth(), color: this.line() }).position({ x: '5%', y: Math.max(0, this.viewportHeight - 124) })
      }
    }.width('100%').height('100%').backgroundColor(this.canvasBackgroundColorValue())
      .onAreaChange((_, area: Area) => { this.viewportWidth = Number(area.width); this.viewportHeight = Number(area.height); })
  }
}
