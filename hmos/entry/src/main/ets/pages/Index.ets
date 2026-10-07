import { ColorWheel } from './ColorWheel';
import { MarkdownDoc, MarkdownPreviewCoordinator, MarkdownAttachment, collectMarkdownImages, resolveMarkdownImage } from '../model/Markdown';
import { MarkdownPreview } from './MarkdownPreview';
import { AttachmentImagePreview } from './AttachmentImagePreview';
import { AttachmentMediaPreview, createAttachmentPlayer } from './AttachmentMediaPreview';
import { AttachmentPlayback, PlaybackSource, PlaybackView } from '../model/AttachmentPlayback';
import { AttachmentOpen, AttachmentOpenState, attachmentOpenBlocked } from '../model/AttachmentOpen';
import { AttachmentFullscreen, FullscreenView, FullscreenWindow, createFullscreenWindow } from '../model/AttachmentFullscreen';
import { AttachmentImportSelection, AttachmentImportBoundary, AttachmentImportIdentity, AttachmentImportReceipt,
  AttachmentImportSelectionState, attachmentImportTitle } from '../model/AttachmentImportSelection';
import { ImageReadTarget, ImageLease, VerifiedImageView, VerifiedImages } from '../model/VerifiedImages';
import { insertPaste, samePasteTarget } from '../model/EditorPaste';
import { pasteboard } from '@kit.BasicServicesKit';
import { uiText } from '../model/UiStrings';
import { preferences } from '@kit.ArkData';
import { AppearanceSnapshot, MaterialChoice, alpha, mix, contrast, visualStyles, visualStyleLabels, styleRadiusScale, resolveMaterial, normalizeMaterials, canFollow } from '../model/Appearance';
import { SurfaceRelief } from './SurfaceRelief';
import { AmbientBackdrop, IdeaOrb, GlassRim, PaperBackdrop } from './StudioArtwork';
import { common } from '@kit.AbilityKit';
import { NodeContent, LazyVWaterFlowLayout, LazyVWaterFlowLayoutAttribute, LengthMetrics, window } from '@kit.ArkUI';
import { util } from '@kit.ArkTS';
import native from 'libmorrow.so';
import { CardView, Command, Reply, TaskView, workbench } from '../model/Workbench';
import { CardDataSource, WorkspaceCard } from '../model/CardDataSource';
import { CardOrderPreferences, cardOrderPage } from '../model/CardOrder';
import { AttachmentFiles, PreparedAttachment, VerifiedPreview, AttachmentRecoveryDiagnostic } from '../model/AttachmentFiles';
import { AssetView, ImportRecord, ImportCommand, attachmentKind, attachmentSize, attachmentIcon, attachmentDetails, selectedImport, sameImport } from '../model/Attachments';
import { QueryConditions, QueryState, QueryCoordinator } from '../model/QueryCoordinator';
import { Values as DraftValues, TextValue as DraftTextValue, DraftScope, DraftRecord,
  AssetSelection, EditorDraftCoordinator, DraftSaveFailure, copyValues, copyText, sameValues, nextGeneration } from '../model/EditorDraft';

class CardDrag {
  card: CardView;
  token: string;
  revision: number;
  nonce: string;
  constructor(card: CardView, token: string, revision: number, nonce: string) {
    this.card = card; this.token = token; this.revision = revision; this.nonce = nonce;
  }
}
interface CardDragExtra { extraInfo?: string; }

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
  @State @Watch('updateMarkdown') detailId: string = '';
  @State orderRevision: number = 0;
  @State orderWorking: boolean = false;
  @State orderError: string = '';
  @State dragSource: string = '';
  @State dragTarget: string = '';
  private heldCard: CardDrag | undefined = undefined;
  private cardBounds: Map<string, number[]> = new Map();
  @State title: string = '';
  @State @Watch('updateMarkdown') description: string = '';
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
  @State @Watch('updateMarkdown') editorOpen: boolean = false;
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
  @State attachmentWorking: boolean = false;
  @State attachmentSelectionState: AttachmentImportSelectionState = { phase: 'idle', busy: false, total: 0, started: 0, selected: 0, retained: 0, error: '' };
  private attachmentSelection: AttachmentImportSelection | undefined = undefined;
  private attachmentEditEpoch: number = 0;
  private attachmentEditorIdentity: string = '';
  private attachmentEditingValues: string = '';
  @State editorAssets: AssetView[] = [];
  @State importRecords: ImportRecord[] = [];
  @State pendingSpools: PreparedAttachment[] = [];
  @State attachmentDiagnostics: AttachmentRecoveryDiagnostic[] = [];
  private attachmentFiles: AttachmentFiles | undefined = undefined;
  @State attachmentPending: string = '';
  @State attachmentPreview: VerifiedPreview | undefined = undefined;
  @State attachmentPreviewOpen: boolean = false;
  @State attachmentPreviewLoading: boolean = false;
  @State attachmentPreviewFailure: string = '';
  @State attachmentPreviewKind: string = 'image';
  @State attachmentPreviewTitle: string = '';
  @State attachmentPreviewClosing: boolean = false;
  @State mediaFullscreen: boolean = false;
  @State fullscreenState: FullscreenView = new FullscreenView();
  private fullscreen: AttachmentFullscreen = new AttachmentFullscreen(
    (): Promise<FullscreenWindow> => createFullscreenWindow(this.getUIContext().getHostContext() as common.UIAbilityContext),
    window.Orientation.LANDSCAPE, (): void => {
      if (this.pageAlive) { this.fullscreenState = this.fullscreen.view(); this.mediaFullscreen = this.fullscreenState.active; }
    });
  @State mediaView: PlaybackView = new PlaybackView();
  private mediaPlayback: AttachmentPlayback = new AttachmentPlayback(createAttachmentPlayer,
    (): void => {
      if (this.pageAlive) {
        const prior = this.mediaView.phase; this.mediaView = this.mediaPlayback.view();
        if (this.mediaView.phase === 'failed' && prior !== 'failed') { this.exitMediaFullscreen(); }
      }
    });
  @StorageLink('hmos-foreground') @Watch('foregroundChanged') foreground: boolean = true;
  @State fileOpenState: AttachmentOpenState = new AttachmentOpenState();
  private fileOpen: AttachmentOpen | undefined = undefined;
  private attachmentPreviewEpoch: number = 0;
  @State previewCleanupPending: boolean = false;
  @State previewCleanupWorking: boolean = false;
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
  @State @Watch('updateMarkdown') previewBody: boolean = false;
  @State markdownDoc: MarkdownDoc = new MarkdownDoc();
  @State markdownLoading: boolean = false;
  @State markdownError: string = '';
  @State markdownAssets: MarkdownAttachment[] = [];
  @State markdownImages: VerifiedImageView[] = [];
  private inlineImages: VerifiedImages = new VerifiedImages(
    async (target: ImageReadTarget): Promise<ImageLease> => {
      if (!this.pageAlive || !this.attachmentFiles) { throw new Error('图片读取已关闭'); }
      return await this.attachmentFiles.previewFile(target.request, target.name,
        (serialized: string, fd: number): Promise<string> => workbench.exportFd(serialized, fd), 'inline', target.byte_length);
    }, async (token: string): Promise<void> => { await this.releaseImageToken(token); },
    (): void => { this.markdownImages = this.inlineImages.views(); });
  @State pasteWorking: boolean = false;
  @State lastDraftField: string = 'description';
  private markdown: MarkdownPreviewCoordinator = new MarkdownPreviewCoordinator(
    async (text: string): Promise<MarkdownDoc> => {
      const command = new Command(); command.action = 'markdown'; command.text = text;
      const reply = await workbench.send(JSON.stringify(command));
      if (!reply.ok) { throw new Error(reply.error); }
      return reply.markdown;
    }, (): void => {
      this.markdownDoc = this.markdown.doc ?? new MarkdownDoc(); this.markdownLoading = this.markdown.loading; this.markdownError = this.markdown.error;
      this.updateInlineImages();
    });
  @State appearanceStatus: string = '';
  private preferenceStore: preferences.Preferences | undefined = undefined;
  private orderStore: preferences.Preferences | undefined = undefined;
  private cardOrder: CardOrderPreferences = new CardOrderPreferences(
    async (): Promise<string | undefined> => {
      if (!this.orderStore) { throw new Error('卡片顺序设置不可用'); }
      const raw = this.orderStore.getSync('card-order-v1', '') as string; return raw || undefined;
    }, async (raw: string): Promise<void> => {
      if (!this.orderStore) { throw new Error('卡片顺序设置不可用'); }
      const prior = this.orderStore.getSync('card-order-v1', '') as string;
      try { await this.orderStore.put('card-order-v1', raw); await this.orderStore.flush(); }
      catch (error) {
        // Preferences.put also changes its in-memory cache. Roll that cache
        // back so an unrelated later flush cannot persist a failed candidate.
        this.orderStore.putSync('card-order-v1', prior); throw new Error('卡片顺序写入失败');
      }
    }, (): void => {
      this.orderRevision = this.cardOrder.revision; this.orderWorking = this.cardOrder.busy || this.cardOrder.loading;
      this.orderError = this.cardOrder.error; this.updateWorkspaceCards();
    });
  private appearanceWrites: Promise<void> = Promise.resolve();
  private settingsScroller: Scroller = new Scroller();
  private workspaceScroller: Scroller = new Scroller();
  private detailScroller: Scroller = new Scroller();
  private detailOffset: number = 0;
  private workspaceCards: CardDataSource = new CardDataSource();
  private updateWorkspaceCards(): void {
    this.workspaceCards.replace(this.section, this.visibleCards());
    const page = cardOrderPage(this.section);
    if (page) { this.cardOrder.setView(page, this.pageCards().map((c: CardView): string => c.id),
      this.visibleCards().map((c: CardView): string => c.id), this.cardViewToken()); }
    else { this.cardOrder.setView('', [], [], ''); }
  }
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
    this.contentGeneration++; this.updateWorkspaceCards(); this.workspaceConditionsChanged(); this.updateMarkdown();
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
    const context = this.getUIContext().getHostContext() as common.UIAbilityContext;
    if (!this.attachmentFiles) { this.attachmentFiles = new AttachmentFiles(context); }
    const files = this.attachmentFiles;
    this.fileOpen = new AttachmentOpen(context, (lease: VerifiedPreview): boolean => files.isPreviewActive(lease),
      async (token: string): Promise<void> => {
        try { await files.releasePreview(token); }
        finally { if (this.pageAlive) { this.previewCleanupPending = files.previewCleanupPending(); } }
      }, (state: AttachmentOpenState): void => { if (this.pageAlive) { this.fileOpenState = state; } });
    this.fileOpenState = this.fileOpen.snapshot();
    this.previewCleanupPending = this.attachmentFiles.previewCleanupPending();
    this.getUIContext().getFont().registerFont({ familyName: 'MorrowIcons', familySrc: $rawfile('materialicons.otf') });
    this.restoreAppearance();
    this.restoreCardOrder();
    this.start();
  }
  onBackPress(): boolean {
    if (this.attachmentPreviewOpen && this.mediaFullscreen) { this.exitMediaFullscreen(); return true; }
    if (this.attachmentPreviewOpen) { this.closeAttachmentPreview(); return true; }
    if (this.draftsOpen) { this.draftsOpen = false; return true; }
    if (this.editorOpen) { this.closeEditor(); return true; }
    if (this.detailId) { this.closeDetail(); return true; }
    if (this.settingsOpen) { this.backSettings(); return true; }
    return false;
  }
  aboutToDisappear(): void {
    this.attachmentSelection?.stop();
    this.closeAttachmentPreview();
    this.pageAlive = false;
    // No mutation is replayed on destruction. Only earlier acknowledged raw
    // journal generations are recoverable after a forced process termination.
    this.editorDraft?.dispose();
    this.markdown.dispose();
    this.inlineImages.dispose();
    this.queries.dispose();
    this.cardOrder.dispose();
    if (this.workspaceRestoreTimer !== -1) { clearTimeout(this.workspaceRestoreTimer); }
    this.mounted = false; native.releasePreview(this.preview);
  }
  private foregroundChanged(): void {
    if (!this.foreground) { this.mediaPlayback.pauseForBackground(); this.exitMediaFullscreen(); }
    else { this.fileOpen?.reconcile(); }
  }
  private async switchMediaFullscreen(token: string): Promise<void> {
    if (!this.pageAlive || !this.attachmentPreviewOpen || this.attachmentPreviewClosing || !token || token !== this.mediaView.token) { return; }
    try { await this.fullscreen.toggle(); }
    catch (_) { if (this.pageAlive) { this.message = this.fullscreen.view().error || '全屏切换未完成，请重试。'; } }
  }
  private async exitMediaFullscreen(): Promise<void> {
    try { await this.fullscreen.exit(); }
    catch (_) { if (this.pageAlive) { this.message = this.fullscreen.view().error || '窗口恢复未完成，请重试退出。'; } }
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
    const pageIds = new Set<string>(this.pageCards().map((c: CardView): string => c.id));
    this.queryIds.forEach((id: string): void => { const card = byId.get(id); if (card && pageIds.has(id)) { items.push(card); } });
    const page = cardOrderPage(this.section);
    if (!page || !this.cardOrder.manual(page)) { return items; }
    const ordered = this.cardOrder.order(page, items.map((c: CardView): string => c.id));
    return ordered.map((id: string): CardView => byId.get(id)!);
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
    const values = JSON.stringify(draft.current);
    if (values !== this.attachmentEditingValues) { this.attachmentEditingValues = values; this.attachmentEditEpoch++; }
    this.refreshEditorAssets();
    this.updateInlineImages();
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
  private updateMarkdown(): void {
    const detail = this.cards.find((c: CardView): boolean => c.id === this.detailId);
    if (detail && !this.editorOpen) {
      this.markdown.update('detail:' + detail.id + ':' + detail.revision, detail.description, this.pageAlive); this.updateInlineImages(); return;
    }
    this.markdown.update(this.editorDraft?.scope.draft_id ?? '', this.description, this.pageAlive && this.editorOpen && this.previewBody);
    this.updateInlineImages();
  }
  private updateInlineImages(): void {
    const card = !this.editorOpen ? this.cards.find((value: CardView): boolean => value.id === this.detailId) : undefined;
    const enabled = this.pageAlive && (!!card || this.editorOpen && this.previewBody);
    const assets: AssetView[] = enabled ? card?.assets ?? this.editorAssets : [];
    this.markdownAssets = assets.map((asset: AssetView): MarkdownAttachment => {
      const value = new MarkdownAttachment(); value.id = asset.id; value.name = asset.name; value.kind = asset.kind; return value;
    });
    const targets: ImageReadTarget[] = [];
    if (enabled) {
      for (const href of collectMarkdownImages(this.markdownDoc)) {
        const resolved = resolveMarkdownImage(href, this.markdownAssets);
        const asset = assets.find((value: AssetView): boolean => value.id === resolved.attachment_id);
        if (resolved.kind !== 'attachment' || !asset) { continue; }
        try {
          const target = new ImageReadTarget(); target.id = asset.id; target.name = asset.name; target.byte_length = asset.byte_length;
          target.request = this.attachmentReadRequest(asset, card); targets.push(target);
        } catch (_) {} // The renderer shows unavailable metadata rather than opening a source path.
      }
    }
    this.inlineImages.update(targets);
  }
  private openInlineImage(id: string): void {
    const card = !this.editorOpen ? this.cards.find((value: CardView): boolean => value.id === this.detailId) : undefined;
    const asset = (card?.assets ?? this.editorAssets).find((value: AssetView): boolean => value.id === id);
    if (asset) { this.previewAttachment(asset, card); }
  }
  private pasteTargetName(): string {
    return this.lastDraftField === 'title' ? '标题' : this.lastDraftField === 'hypothesis' ? '假设' :
      this.lastDraftField === 'conclusion' ? '结论' : this.lastDraftField === 'todos' ? '待办输入' : '正文';
  }
  private canPaste(): boolean {
    return this.ready && this.editorOpen && !!this.editorDraft && !this.editorDraft.disposed &&
      !this.attachmentWorking && !this.pasteWorking && !this.draftRestoreInput && !this.busy && !this.draftWorking && !this.draftRetiring && !this.draftRetirementUnknown &&
      !this.draftCaptureIncomplete && !this.editorDraft.unknown && this.pending.length === 0 && !this.current()?.deleted;
  }
  private async pasteContent(): Promise<void> {
    if (!this.canPaste()) { return; }
    const draft = this.editorDraft!, name = this.lastDraftField, snapshot = copyText(this.draftField(name));
    if (snapshot.composing_start !== -1 || snapshot.composing_end !== -1) {
      this.message = '请先完成输入法候选，再粘贴内容。'; return;
    }
    const limit = name === 'title' ? 60 : name === 'todos' ? 500 : name === 'hypothesis' ? 5000 : name === 'conclusion' ? 10000 : 20000;
    this.pasteWorking = true;
    try {
      const clipboard = pasteboard.getSystemPasteboard(), before = clipboard.getChangeCount();
      const data = await clipboard.getData();
      const count = data.getRecordCount();
      if (count > 20) { throw new Error('剪贴板条目过多，请分次复制，内容未插入。'); }
      const chunks: string[] = []; let rich = false, unsupported = false;
      for (let index = 0; index < count; index++) {
        const record = data.getRecord(index);
        const types = record.getValidTypes([pasteboard.MIMETYPE_TEXT_PLAIN, pasteboard.MIMETYPE_TEXT_HTML]);
        rich = rich || types.includes(pasteboard.MIMETYPE_TEXT_HTML);
        if (types.includes(pasteboard.MIMETYPE_TEXT_PLAIN)) {
          const text = await record.getData(pasteboard.MIMETYPE_TEXT_PLAIN);
          if (typeof text !== 'string') { throw new Error('剪贴板文字格式无法读取，内容未插入。'); }
          chunks.push(text);
        } else { unsupported = true; }
      }
      if (clipboard.getChangeCount() !== before) { throw new Error('读取期间剪贴板发生了变化，请重新粘贴。'); }
      const text = chunks.join('\n\n');
      if (text.length === 0) { throw new Error('剪贴板中没有可粘贴的文字。文件与图片导入仍在适配。'); }
      if (text.length > 20000) { throw new Error('剪贴板文字过长，请分次复制，内容未插入。'); }
      const command = new Command(); command.action = 'paste_plain'; command.text = text; command.section = name;
      const reply = await workbench.send(JSON.stringify(command));
      if (!reply.ok) { throw new Error(reply.error); }
      if (typeof reply.paste_text !== 'string') { throw new Error('粘贴内容回复无效，内容未插入。'); }
      if (!this.pageAlive || !this.editorOpen || this.editorDraft !== draft || draft.disposed || this.draftRetiring ||
        this.draftRetirementUnknown || this.draftCaptureIncomplete || !samePasteTarget(snapshot, this.draftField(name))) {
        throw new Error('插入位置或文字已变化，请重新粘贴，内容未插入。');
      }
      const inserted = insertPaste(snapshot, reply.paste_text, limit);
      this.editorInputChanged(name, inserted.text);
      const field = this.draftField(name); field.selection_base = inserted.selection_base; field.selection_extent = inserted.selection_extent;
      draft.update(copyValues(this.editorValues)); this.draftSelectionPending.add(name);
      if (name === 'description') {
        // A wide editor retains its native input alongside the preview. Defer
        // a narrow editor's caret until that field is visible/focused again.
        this.previewBody = true; this.updateMarkdown();
        if (Math.min(820, this.viewportWidth - 40) - 80 >= 590) { this.restoreSelection(name); }
      }
      else { this.restoreSelection(name); }
      this.message = rich || unsupported ? '已粘贴可读文字；富文本样式、图片与文件尚未导入。' : '内容已读入 · ' + this.pasteTargetName();
    } catch (error) {
      if (this.pageAlive && this.editorDraft === draft) { this.message = String(error); }
    } finally { this.pasteWorking = false; }
  }
  private async openMarkdownLink(href: string): Promise<void> {
    // Only the validated native projection supplies these URLs. Attachment
    // handles stay inside the library and are never opened as external paths.
    const value = href.toLowerCase();
    if (!this.pageAlive || (!this.editorOpen && !this.detailId) || (!value.startsWith('https://') && !value.startsWith('http://') && !value.startsWith('mailto:'))) { return; }
    try {
      const context = this.getUIContext().getHostContext() as common.UIAbilityContext;
      await context.openLink(href, { appLinkingOnly: false, hideFailureTipDialog: true });
    } catch (_) { if (this.pageAlive) { this.message = '链接无法打开，请检查可用的浏览器或邮件应用。'; } }
  }
  private draftTextChanged(name: string, value: string, preview?: PreviewText): void {
    if (!this.pageAlive || !this.editorOpen || !this.editorDraft || this.editorDraft.disposed || this.draftRestoreInput || this.attachmentWorking || this.draftRetiring || this.draftRetirementUnknown) { return; }
    const field = this.draftField(name);
    const offset = preview?.offset ?? -1, candidate = preview?.value ?? '';
    if (candidate.length > 0 && (!Number.isInteger(offset) || offset < 0 || offset > value.length)) {
      this.draftCaptureBlocked.add(name); this.draftCaptureIncomplete = true;
      this.message = '输入法预编辑位置无法确认，请完成输入后保留草稿。'; return;
    }
    this.draftCaptureBlocked.delete(name); this.draftCaptureIncomplete = this.draftCaptureBlocked.size > 0;
    this.draftSelectionPending.delete(name);
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
    if (!this.pageAlive || !this.editorOpen || this.editorDraft?.disposed || !this.draftFocusedFields.has(name) || this.draftRestoreInput || this.attachmentWorking || this.draftRetiring || this.draftRetirementUnknown || !this.editorDraft) { return; }
    const field = this.draftField(name);
    if (this.draftCaptureBlocked.has(name) || start < -1 || end < -1 || start > field.text.length || end > field.text.length) { return; }
    // An actual focused selection supersedes a scheduled restoration. Native
    // construction events are already excluded by the focused-field guard.
    this.draftSelectionPending.delete(name); field.selection_base = start; field.selection_extent = end;
    this.editorDraft.update(copyValues(this.editorValues));
  }
  private restoreSelection(name: string): void {
    if (!this.draftSelectionPending.has(name)) { return; }
    const value = copyText(this.draftField(name)), start = value.selection_base, end = value.selection_extent;
    const draft = this.editorDraft;
    if (start < 0 || end < 0) { this.draftSelectionPending.delete(name); return; }
    // Do not start a new IME composition or steal focus. The caller already
    // focused this field; native controllers cannot restore the old IME session.
    setTimeout(() => {
      if (!this.pageAlive || !this.editorOpen || !draft || draft.disposed || this.draftRetiring || this.draftRetirementUnknown || this.editorDraft !== draft) { return; }
      if (!this.draftSelectionPending.has(name)) { return; }
      if (!samePasteTarget(value, this.draftField(name))) { this.draftSelectionPending.delete(name); return; }
      this.draftSelectionPending.delete(name);
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
    }, 16);
  }
  private focusDraftField(name: string): void {
    if (!this.pageAlive || !this.editorOpen || !this.editorDraft || this.editorDraft.disposed || this.draftRetiring || this.draftRetirementUnknown) { return; }
    // A native control emits its initial selection while being constructed.
    // Only a field that the user actually focuses can create a selection-only
    // generation; viewing an unchanged card must not consume a journal slot.
    this.lastDraftField = name;
    this.draftFocusedFields.clear(); this.draftFocusedFields.add(name); this.restoreSelection(name);
  }
  private blurDraftField(name: string): void { this.draftFocusedFields.delete(name); }
  private attachDraft(card?: CardView, restored?: DraftRecord, preserveFocus: boolean = false): void {
    this.attachmentSelection?.stop(); this.attachmentSelection = undefined;
    this.attachmentEditorIdentity = util.generateRandomUUID(true); this.attachmentEditingValues = '';
    this.attachmentSelectionState = { phase: 'idle', busy: false, total: 0, started: 0, selected: 0, retained: 0, error: '' };
    this.editorDraft?.dispose(); this.draftConflict = false; this.draftStatus = '';
    this.draftSelectionPending = restored ? new Set(['title', 'description', 'hypothesis', 'conclusion', 'todos']) : new Set();
    if (!preserveFocus) { this.draftFocusedFields.clear(); this.lastDraftField = 'description'; }
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
      values.assets = (card?.assets ?? []).map((asset: AssetView): AssetSelection => {
        const selected = new AssetSelection(); selected.asset_id = asset.id; selected.origin = 0; return selected;
      });
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
    this.draftChanged(); this.updateMarkdown();
    this.importRecords = []; this.pendingSpools = []; this.attachmentDiagnostics = []; this.loadImports();
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
  private refreshEditorAssets(): void {
    const draft = this.editorDraft;
    const sources: AssetView[] = (this.current()?.assets ?? []).slice();
    for (const pin of draft?.confirmed?.assets ?? []) {
      sources.push({ id: pin.selection.asset_id, name: pin.display_name, kind: attachmentKind(pin.display_name), byte_length: pin.byte_length, sha256: pin.sha256, media_type: pin.media_type });
    }
    for (const item of this.importRecords) {
      sources.push({ id: item.asset_id, name: item.request.name, kind: item.request.kind, byte_length: item.request.byte_length, sha256: item.request.sha256, media_type: '' });
    }
    this.editorAssets = (draft?.current.assets ?? []).map((selected: AssetSelection): AssetView =>
      sources.filter((asset: AssetView): boolean => asset.id === selected.asset_id).pop() ??
      { id: selected.asset_id, name: '附件信息待核对', kind: 'file', byte_length: '0', sha256: '', media_type: '' });
  }
  private canImportAttachment(): boolean {
    return this.pageAlive && this.ready && this.editorOpen && !!this.editorDraft && !this.attachmentWorking &&
      !this.pasteWorking && !this.busy && !this.draftWorking && !this.draftRetiring && !this.draftRetirementUnknown && !this.attachmentPending &&
      !this.draftConflict && !this.draftCaptureIncomplete && !this.editorDraft?.unknown && this.pending.length === 0;
  }
  private async loadImports(): Promise<boolean> {
    const draft = this.editorDraft, files = this.attachmentFiles;
    if (!draft || !files) { return false; }
    try {
      const cmd = new Command(); cmd.action = 'import_list'; cmd.id = draft.scope.card_id; cmd.draft_id = draft.scope.draft_id;
      const result = await workbench.send(JSON.stringify(cmd));
      if (!result.ok) { throw new Error(result.error); }
      const spools = await files.recover();
      if (this.editorDraft !== draft || !this.editorOpen || !this.pageAlive) { return false; }
      this.importRecords = result.imports;
      this.attachmentDiagnostics = files.recoveryDiagnostics();
      this.pendingSpools = spools.filter((spool: PreparedAttachment): boolean => {
        if (!spool.original_request) { return true; } // No native request was admitted; show cancel-preparation explicitly.
        try { const frozen = JSON.parse(spool.original_request) as ImportCommand;
          return frozen.action === 'import_file' && frozen.import_request.card_id === draft.scope.card_id && frozen.import_request.draft_id === draft.scope.draft_id;
        } catch (_) { return false; }
      });
      this.refreshEditorAssets();
      return true;
    } catch (error) { if (this.editorDraft === draft) { this.message = '附件记录读取失败：' + String(error); } return false; }
  }
  private importBoundary(draft: EditorDraftCoordinator, editor: string): AttachmentImportBoundary {
    return { editor_id: this.attachmentEditorIdentity, card_id: draft.scope.card_id, draft_id: draft.scope.draft_id,
      generation: draft.confirmed?.generation ?? '0', edit_epoch: this.attachmentEditEpoch,
      selected_count: draft.current.assets.length, import_count: this.importRecords.length, title: draft.current.title.text,
      blocked: !this.pageAlive || !this.editorOpen || this.editorDraft !== draft || this.attachmentEditorIdentity !== editor ||
        draft.disposed || draft.unknown || draft.conflicted || this.draftConflict || this.draftCaptureIncomplete ||
        this.busy || this.pasteWorking || this.draftRetiring || this.draftRetirementUnknown || !!this.attachmentPending || !!this.pending ||
        this.title !== draft.current.title.text };
  }
  private async importAttachment(): Promise<void> {
    if (!this.canImportAttachment() || !this.attachmentFiles || !this.editorDraft) { return; }
    const draft = this.editorDraft, files = this.attachmentFiles, editor = this.attachmentEditorIdentity;
    const current = draft.current; this.attachmentWorking = true;
    try {
      // Settle already scheduled input before a slow picker. A cancelled
      // unchanged source editor still never creates a new raw journal.
      if (draft.dirty || draft.saving) { await draft.ensureConfirmed(); }
      if (this.importBoundary(draft, editor).blocked || !sameValues(current, draft.current) || !await this.loadImports()) { return; }
      const selection = new AttachmentImportSelection({
        read: (): AttachmentImportBoundary => this.importBoundary(draft, editor),
        select: (maximum: number): Promise<string[]> => files.selectUris(maximum),
        confirm: async (_boundary: AttachmentImportBoundary): Promise<AttachmentImportBoundary> => {
          await draft.ensureConfirmed(); return this.importBoundary(draft, editor);
        },
        importOne: (uri: string, identity: AttachmentImportIdentity, boundary: AttachmentImportBoundary): Promise<AttachmentImportReceipt> =>
          this.importSelectedUri(draft, files, editor, uri, identity, boundary),
        operation: (): string => util.generateRandomUUID(true),
        changed: (state: AttachmentImportSelectionState): void => {
          if (this.pageAlive && this.editorDraft === draft && this.attachmentEditorIdentity === editor) { this.attachmentSelectionState = state; }
        }
      });
      this.attachmentSelection = selection;
      const result = await selection.run();
      if (this.pageAlive && this.editorDraft === draft && result.error) { this.message = result.error; }
    } catch (error) {
      if (this.pageAlive && this.editorDraft === draft) { this.message = '附件导入尚未确认：' + String(error); }
    } finally { files.discardSelection(); this.attachmentWorking = false; this.draftChanged(); await this.loadImports(); }
  }
  private async importSelectedUri(draft: EditorDraftCoordinator, files: AttachmentFiles, editor: string, uri: string,
    identity: AttachmentImportIdentity, boundary: AttachmentImportBoundary): Promise<AttachmentImportReceipt> {
    let spool: PreparedAttachment | undefined = undefined;
    try {
      spool = await files.prepareUri(uri);
      const now = this.importBoundary(draft, editor);
      if (now.blocked || draft.dirty || now.generation !== identity.expected_generation || now.edit_epoch !== boundary.edit_epoch ||
        now.selected_count !== boundary.selected_count || now.import_count !== boundary.import_count || now.title !== boundary.title) {
        return { outcome: 'cancelled', identity, name: spool.name, next: undefined, error: '草稿发生变化，后续附件未开始导入。' };
      }
      const cmd = new ImportCommand(); const request = cmd.import_request;
      request.card_id = identity.card_id; request.draft_id = identity.draft_id;
      request.expected_generation = identity.expected_generation; request.operation_id = identity.operation_id;
      request.name = spool.name; request.kind = attachmentKind(spool.name); request.byte_length = spool.byte_length; request.sha256 = spool.sha256;
      const outcome = await this.acceptPreparedImport(draft, spool, JSON.stringify(cmd), true);
      return { outcome, identity, name: spool.name, next: outcome === 'selected' ? this.importBoundary(draft, editor) : undefined,
        error: outcome === 'selected' ? '' : '附件已保留，后续附件未开始导入。' };
    } catch (_) {
      return { outcome: spool?.original_request ? 'unknown' : 'failed', identity, name: spool?.name ?? '', next: undefined,
        error: spool?.original_request ? '附件导入尚未确认，原请求已保留；后续附件未开始导入。' : '附件准备失败，后续附件未开始导入。' };
    } finally {
      if (spool && !spool.original_request) { try { await files.release(spool); } catch (_) {} }
    }
  }
  private async acceptPreparedImport(draft: EditorDraftCoordinator, spool: PreparedAttachment, original: string, fillTitle: boolean = false): Promise<string> {
    const files = this.attachmentFiles;
    if (!files) { return 'retained'; }
    const request = (JSON.parse(original) as ImportCommand).import_request;
    const result = JSON.parse(await files.importPrepared(spool, original,
      (serialized: string, fd: number): Promise<string> => workbench.importFd(serialized, fd))) as Reply;
    if (!result.ok || result.imports.length !== 1) { throw new Error(result.error || '附件回执缺失'); }
    const record = result.imports[0];
    if (!sameImport(record.request, request)) { throw new Error('原附件回执身份不匹配。'); }
    if (this.editorDraft !== draft || !this.editorOpen || !this.pageAlive) { return 'retained'; }
    if (record.phase === 'retired') { await files.release(spool); this.message = '原导入已结束。'; return 'cancelled'; }
    selectedImport(record, request);
    this.importRecords = this.importRecords.filter((r: ImportRecord): boolean => r.request.operation_id !== request.operation_id).concat([record]);
    await files.release(spool); // Ready is retained in Rust; URI/spool are no longer recovery evidence.
    if (this.editorDraft !== draft || !this.editorOpen || !this.pageAlive || draft.disposed || draft.conflicted || this.draftConflict) { return 'retained'; }
    if (draft.confirmed?.generation !== request.expected_generation || draft.dirty || draft.unknown) {
      this.message = '附件已导入，草稿已变化；可在待处理附件中明确加入。'; return 'retained';
    }
    return await this.addImportedAsset(record, fillTitle, draft) ? 'selected' : 'retained';
  }
  private async retryAttachment(spool: PreparedAttachment): Promise<void> {
    if (!this.canImportAttachment() || !this.editorDraft || !spool.original_request) { return; }
    const draft = this.editorDraft; this.attachmentWorking = true;
    try { await this.acceptPreparedImport(draft, spool, spool.original_request); }
    catch (error) { this.message = '原附件请求核对失败：' + String(error); }
    finally { this.attachmentWorking = false; this.draftChanged(); await this.loadImports(); }
  }
  private async addImportedAsset(record: ImportRecord, fillTitle: boolean = false, expectedDraft?: EditorDraftCoordinator): Promise<boolean> {
    const draft = expectedDraft ?? this.editorDraft;
    if (!draft || draft !== this.editorDraft || !this.pageAlive || !this.editorOpen || draft.disposed || draft.conflicted ||
      record.request.card_id !== draft.scope.card_id || record.request.draft_id !== draft.scope.draft_id ||
      draft.unknown || draft.dirty || !draft.confirmed) { this.message = '请先保留当前草稿，再加入附件。'; return false; }
    const selected = selectedImport(record, record.request);
    const values = draft.current;
    if (values.assets.some((asset: AssetSelection): boolean => asset.asset_id === selected.asset_id)) { return true; }
    const title = fillTitle ? attachmentImportTitle(values.title.text, record.request.name) : undefined;
    if (title !== undefined) {
      const text = new DraftTextValue(); text.text = title; text.selection_base = title.length; text.selection_extent = title.length;
      values.title = text; this.title = title;
    }
    values.assets.push(selected); this.editorValues = copyValues(values); this.dirty = true;
    draft.update(values); await draft.flush(); this.message = '附件已保留在草稿中，保存卡片后生效。';
    return this.editorDraft === draft && !draft.dirty && !draft.unknown && !!draft.confirmed && sameValues(draft.current, draft.confirmed.values);
  }
  private async selectPendingImport(record: ImportRecord): Promise<void> {
    if (!this.canImportAttachment()) { return; }
    this.attachmentWorking = true;
    try { await this.addImportedAsset(record); }
    catch (error) { this.message = '附件加入尚未确认：' + String(error); }
    finally { this.attachmentWorking = false; this.draftChanged(); await this.loadImports(); }
  }
  private removeEditorAsset(asset: AssetView): void {
    if (!this.canImportAttachment() || !this.editorDraft) { return; }
    const values = this.editorDraft.current;
    values.assets = values.assets.filter((selection: AssetSelection): boolean => selection.asset_id !== asset.id);
    this.editorValues = copyValues(values); this.dirty = true; this.editorDraft.update(values);
  }
  private abandonImport(record: ImportRecord): void {
    if (!this.canImportAttachment() || !this.editorDraft?.confirmed) { return; }
    const draft = this.editorDraft;
    this.getUIContext().showAlertDialog({ title: '放弃这次附件导入？', message: record.request.name,
      primaryButton: { value: '继续保留', action: () => {} }, secondaryButton: { value: '放弃导入', action: () => {
        if (this.editorDraft !== draft || !this.canImportAttachment() || !draft.confirmed) { return; }
        const cmd = new Command(); cmd.action = 'import_abandon'; cmd.id = draft.scope.card_id; cmd.draft_id = draft.scope.draft_id;
        cmd.generation = draft.confirmed.generation; cmd.operation = util.generateRandomUUID(true); cmd.import_request = record.request;
        this.attachmentPending = JSON.stringify(cmd); this.retryAttachmentCleanup();
      } } });
  }
  private async cancelPrepared(spool: PreparedAttachment): Promise<void> {
    if (!this.attachmentFiles || spool.original_request || this.attachmentWorking) { return; }
    this.attachmentWorking = true;
    try { await this.attachmentFiles.release(spool); this.message = '尚未开始的附件准备已取消。'; }
    catch (error) { this.message = '附件缓存清理失败：' + String(error); }
    finally { this.attachmentWorking = false; await this.loadImports(); }
  }
  private async reconcileImports(): Promise<void> {
    const draft = this.editorDraft;
    if (!draft || this.attachmentWorking) { return; }
    const cmd = new Command(); cmd.action = 'import_reconcile'; cmd.id = draft.scope.card_id; cmd.draft_id = draft.scope.draft_id;
    this.attachmentPending = JSON.stringify(cmd); await this.retryAttachmentCleanup();
  }
  private async retryAttachmentCleanup(): Promise<void> {
    if (!this.attachmentPending || this.attachmentWorking) { return; }
    const original = this.attachmentPending; this.attachmentWorking = true;
    try {
      const result = await workbench.send(original);
      if (!result.ok) {
        if (result.effect === 'not_committed') { this.attachmentPending = ''; }
        throw new Error(result.error);
      }
      const command = JSON.parse(original) as Command;
      if (command.action === 'import_abandon') {
        command.action = 'import_reconcile'; this.attachmentPending = JSON.stringify(command);
        const cleanup = await workbench.send(this.attachmentPending);
        if (!cleanup.ok) { throw new Error(cleanup.error); }
      }
      this.attachmentPending = ''; this.message = '原附件导入已结束。';
    } catch (error) { this.message = '附件清理尚未确认：' + String(error); }
    finally { this.attachmentWorking = false; await this.loadImports(); }
  }
  private async exportAttachment(c: CardView, asset: AssetView): Promise<void> {
    if (this.attachmentWorking || this.busy || this.pending || !this.attachmentFiles || this.cards.find((card: CardView): boolean => card.id === c.id)?.source !== c.source) { return; }
    const cmd = new Command(); cmd.action = 'attachment_export'; cmd.id = c.id; cmd.source = c.source;
    cmd.generation = c.revision; cmd.attachment_id = asset.id;
    this.attachmentWorking = true;
    try { this.message = await this.attachmentFiles.exportToPicker(JSON.stringify(cmd), asset.name,
      (serialized: string, fd: number): Promise<string> => workbench.exportFd(serialized, fd)) ? '附件已导出。' : '已取消导出。'; }
    catch (error) { this.message = '附件导出未完成：' + String(error); }
    finally { this.attachmentWorking = false; }
  }
  private attachmentReadRequest(asset: AssetView, c?: CardView): string {
    const cmd = new Command(); const draft = this.editorDraft;
    if (c) {
      if (this.cards.find((card: CardView): boolean => card.id === c.id)?.source !== c.source ||
        !c.assets.some((value: AssetView): boolean => value.id === asset.id && value.sha256 === asset.sha256 && value.byte_length === asset.byte_length)) {
        throw new Error('附件来源已变化，请重新打开卡片。');
      }
      cmd.action = 'attachment_export'; cmd.id = c.id; cmd.source = c.source; cmd.generation = c.revision;
    } else {
      if (!this.editorOpen || !draft || draft.disposed || !draft.current.assets.some((selection: AssetSelection): boolean => selection.asset_id === asset.id)) {
        throw new Error('附件已从当前草稿移除。');
      }
      const pin = draft.confirmed?.assets.find((value): boolean => value.selection.asset_id === asset.id);
      if (pin && pin.sha256 === asset.sha256 && pin.byte_length === asset.byte_length) {
        cmd.action = 'draft_asset_export'; cmd.id = draft.scope.card_id; cmd.draft_id = draft.scope.draft_id; cmd.generation = draft.confirmed!.generation;
      } else {
        const current = this.current();
        if (!current || current.source !== this.draftSource || !current.assets.some((value: AssetView): boolean => value.id === asset.id && value.sha256 === asset.sha256 && value.byte_length === asset.byte_length)) {
          throw new Error('附件信息尚未确认，请先保留草稿后重试。');
        }
        cmd.action = 'attachment_export'; cmd.id = current.id; cmd.source = current.source; cmd.generation = current.revision;
      }
    }
    cmd.attachment_id = asset.id; return JSON.stringify(cmd);
  }
  private async exportEditorAttachment(asset: AssetView): Promise<void> {
    if (!this.canImportAttachment() || !this.attachmentFiles) { return; }
    this.attachmentWorking = true;
    try {
      const original = this.attachmentReadRequest(asset);
      this.message = await this.attachmentFiles.exportToPicker(original, asset.name,
        (serialized: string, fd: number): Promise<string> => workbench.exportFd(serialized, fd)) ? '附件已导出。' : '已取消导出。';
    } catch (error) { this.message = '附件导出未完成：' + String(error); }
    finally { this.attachmentWorking = false; }
  }
  private async previewAttachment(asset: AssetView, c?: CardView): Promise<void> {
    if (this.attachmentWorking || this.busy || this.attachmentPreviewOpen || !this.attachmentFiles || !['image', 'gif', 'audio', 'video'].includes(asset.kind)) { return; }
    const draft = this.editorDraft, files = this.attachmentFiles, epoch = ++this.attachmentPreviewEpoch;
    let candidate: VerifiedPreview | undefined = undefined;
    this.attachmentWorking = true; this.attachmentPreviewOpen = true; this.attachmentPreviewLoading = true; this.attachmentPreviewFailure = '';
    this.attachmentPreviewKind = asset.kind; this.attachmentPreviewTitle = asset.name; this.mediaFullscreen = false;
    try {
      const original = this.attachmentReadRequest(asset, c);
      const verified = await files.previewFile(original, asset.name,
        (serialized: string, fd: number): Promise<string> => workbench.exportFd(serialized, fd), 'dialog', asset.byte_length);
      candidate = verified;
      if (!this.pageAlive || epoch !== this.attachmentPreviewEpoch || (c ? this.detailId !== c.id : this.editorDraft !== draft) ||
        this.attachmentReadRequest(asset, c) !== original) { return; }
      this.attachmentPreview = verified; candidate = undefined;
      if (asset.kind === 'audio' || asset.kind === 'video') {
        const source = new PlaybackSource(); source.token = verified.token; source.path = verified.path;
        source.uri = verified.uri; source.kind = asset.kind; source.byteLength = Number(verified.byte_length);
        await this.mediaPlayback.open(source);
      }
    } catch (error) { if (this.pageAlive && epoch === this.attachmentPreviewEpoch) { this.attachmentPreviewFailure = '附件预览失败：' + String(error); } }
    finally { if (candidate) { try { await this.releaseImageToken(candidate.token); } catch (_) {} } this.attachmentWorking = false; if (epoch === this.attachmentPreviewEpoch) { this.attachmentPreviewLoading = false; } }
  }
  private async closeAttachmentPreview(): Promise<void> {
    if (this.attachmentPreviewClosing) { return; }
    this.attachmentPreviewEpoch++; this.attachmentPreviewLoading = false; this.attachmentPreviewClosing = true;
    try {
      let windowRestored: boolean = true;
      try { await this.fullscreen.exit(); } catch (_) { windowRestored = false; }
      // A rejected native release retains the descriptor and its verified
      // file; keeping this modal open gives the same owner an explicit retry.
      await this.mediaPlayback.dispose();
      const preview = this.attachmentPreview; this.attachmentPreview = undefined;
      this.attachmentPreviewOpen = !windowRestored;
      this.attachmentPreviewFailure = windowRestored ? '' : '播放器已关闭，窗口还未恢复。请重试关闭。';
      if (preview && this.attachmentFiles) {
        try { await this.releaseImageToken(preview.token); }
        catch (_) { if (this.pageAlive) { this.message = '预览已关闭，临时文件清理尚未完成。'; } }
      }
    } catch (_) {
      if (this.pageAlive) { this.attachmentPreviewFailure = '媒体尚未完成关闭，文件已保留。请重试关闭。'; }
    } finally { this.attachmentPreviewClosing = false; }
  }
  private async openSystemAttachment(asset: AssetView, c?: CardView): Promise<void> {
    const files = this.attachmentFiles, opener = this.fileOpen;
    if (!files || !opener || this.attachmentWorking || this.busy || this.attachmentPreviewOpen || this.fileOpenState.retained || this.fileOpenState.busy) { return; }
    if (attachmentOpenBlocked(asset.name)) { this.message = '此文件类型不直接打开，请导出后查看。'; return; }
    const draft = this.editorDraft;
    let candidate: VerifiedPreview | undefined = undefined;
    this.attachmentWorking = true;
    try {
      const original = this.attachmentReadRequest(asset, c);
      const verified = await files.previewFile(original, asset.name,
        (serialized: string, fd: number): Promise<string> => workbench.exportFd(serialized, fd), 'open', asset.byte_length);
      candidate = verified;
      if (!this.pageAlive || (c ? this.detailId !== c.id : this.editorDraft !== draft) || this.attachmentReadRequest(asset, c) !== original) { return; }
      if (await opener.open(verified)) { candidate = undefined; }
      else { this.message = '已有文件正在查看，请先结束本次查看。'; }
    } catch (error) { if (this.pageAlive) { this.message = '文件打开未完成：' + String(error); } }
    finally {
      // Once adopted, an unknown system dispatch also owns the lease. Never
      // delete its bytes just because a UI route disappeared or await failed.
      if (candidate && !opener.owns(candidate)) { try { await files.releasePreview(candidate.token); } catch (_) {} }
      this.attachmentWorking = false; this.previewCleanupPending = files.previewCleanupPending();
    }
  }
  @Builder
  FileOpenNotice() {
    if (this.fileOpenState.phase !== 'idle' || this.fileOpenState.retained) {
      Column({ space: 5 }) {
        Text(this.fileOpenState.name).fontSize(12).fontColor(this.ink()).width('100%').maxLines(2)
        Text(this.fileOpenState.message).fontSize(11).fontColor(this.muted()).width('100%')
        if (this.fileOpenState.retained) {
          Flex({ wrap: FlexWrap.Wrap }) {
            Button('关闭系统预览', { type: ButtonType.Normal }).fontSize(11).fontColor(this.accent()).backgroundColor(Color.Transparent)
              .enabled(!this.fileOpenState.busy).onClick(() => this.fileOpen?.close()).id('attachment-system-close')
            Button('核对', { type: ButtonType.Normal }).fontSize(11).fontColor(this.accent()).backgroundColor(Color.Transparent)
              .enabled(!this.fileOpenState.busy).onClick(() => this.fileOpen?.reconcile()).id('attachment-system-reconcile')
            if (this.fileOpenState.can_finish) {
              Button('已结束查看，清理临时文件', { type: ButtonType.Normal }).fontSize(11).fontColor(this.accent()).backgroundColor(Color.Transparent)
                .enabled(!this.fileOpenState.busy).onClick(() => this.fileOpen?.finishReadersClosed()).id('attachment-system-finish')
            }
          }.width('100%')
        }
      }.width('100%').padding(10).backgroundColor(alpha(this.accent(), .05)).borderRadius(this.round(10)).id('attachment-system-state')
    }
  }
  private async releaseImageToken(token: string): Promise<void> {
    const files = this.attachmentFiles;
    if (!files) { return; }
    try { await files.releasePreview(token); }
    finally { this.previewCleanupPending = files.previewCleanupPending(); }
  }
  private async retryImageCleanup(): Promise<void> {
    if (!this.attachmentFiles || this.previewCleanupWorking) { return; }
    this.previewCleanupWorking = true;
    try { await this.attachmentFiles.retryPreviewCleanup(); }
    catch (_) { this.message = '图片缓存尚未清理，请稍后重试。'; }
    finally { this.previewCleanupPending = this.attachmentFiles.previewCleanupPending(); this.previewCleanupWorking = false; }
  }
  @Builder
  PreviewCleanupNotice() {
    if (this.previewCleanupPending) {
      Row({ space: 8 }) {
        Text('预览临时文件未能清理').fontSize(11).fontColor(this.muted()).layoutWeight(1)
        Button('重试清理', { type: ButtonType.Normal }).fontSize(11).fontColor(this.accent()).backgroundColor(Color.Transparent)
          .enabled(!this.previewCleanupWorking).onClick(() => this.retryImageCleanup()).id('preview-cleanup-retry')
      }.width('100%')
    }
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
    if (this.attachmentWorking || this.pasteWorking) { return; }
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
    if (!this.pageAlive || !this.editorOpen || !this.editorDraft || this.editorDraft.disposed || this.attachmentWorking || this.draftRetiring || this.draftRetirementUnknown || this.draftRestoreInput) { return; }
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
    if (this.cards.find((v: CardView) => v.id === c.id)?.source !== c.source || c.deleted) { return; }
    this.detailId = '';
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
    if (this.attachmentWorking || this.pasteWorking) { this.message = '文件或粘贴操作尚未完成，请稍候。'; return; }
    if (this.busy || this.pending.length > 0 || this.draftWorking || this.draftRetirementUnknown) { return; }
    if (this.taskEditId.length > 0 && cmd.action !== 'task_rename') { this.message = '请先保存或取消待办重命名'; return; }
    const frozen = raw ?? (this.editorOpen ? this.editorDraft?.current : undefined);
    if (this.editorOpen && !(await this.flushDraft())) { return; }
    if ((cmd.action === 'create' || cmd.action === 'edit') && this.editorDraft?.confirmed) {
      const record = this.editorDraft.confirmed;
      cmd.draft_id = record.scope.draft_id; cmd.generation = record.generation; cmd.draft_operation = record.operation_id;
    }
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
      const draft = this.editorOpen ? this.editorDraft : undefined;
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
    if (this.attachmentWorking || this.pasteWorking) { this.message = '文件或粘贴操作尚未完成，请稍候。'; return; }
    if (this.draftWorking || this.draftConflict) { return; }
    if (this.taskEditId.length > 0) { this.message = '请先保存或取消待办重命名'; return; }
    if (this.title.trim().length === 0) { this.message = '请输入卡片标题'; return; }
    if (!(await this.flushDraft())) { return; }
    const values = this.editorDraft?.current ?? this.editorValues;
    const cmd = this.command(this.selected.length > 0 ? 'edit' : 'create');
    cmd.title = values.title.text; cmd.description = values.description.text; cmd.hypothesis = values.hypothesis.text;
    cmd.conclusion = values.conclusion.text; cmd.category = values.category;
    cmd.stage = values.stage;
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
    return this.ready && !this.attachmentWorking && !this.pasteWorking && !this.busy && !this.draftWorking && !this.draftConflict && !this.draftCaptureIncomplete && !this.editorDraft?.unknown && this.pending.length === 0 &&
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
    if (!this.canChangeCard(c)) { return; }
    const cmd = new Command(); cmd.action = action; cmd.id = c.id; cmd.source = c.source;
    cmd.operation = util.generateRandomUUID(true); cmd.now_ms = Date.now().toString();
    if (action === 'stage') { cmd.stage = value; }
    if (action === 'category') {
      cmd.category = value || '进行中';
      cmd.stage = cmd.category === '进行中' ? '计划中' : cmd.category === '实验' ? '待验证' : '待整理';
    }
    this.submit(cmd);
  }
  private canChangeCard(c: CardView): boolean {
    return this.pageAlive && this.ready && !this.editorOpen && !this.busy && !this.draftWorking && !this.draftRetiring &&
      !this.draftRetirementUnknown && !this.attachmentWorking && !this.pasteWorking && !this.dirty && !c.deleted && this.pending.length === 0 &&
      this.cards.find((v: CardView): boolean => v.id === c.id)?.source === c.source;
  }
  private openDetail(c: CardView): void {
    if (!this.pageAlive || this.editorOpen || this.busy || this.pending || this.dirty ||
      this.cards.find((v: CardView): boolean => v.id === c.id)?.source !== c.source) { return; }
    if (this.detailId !== c.id) { this.detailOffset = 0; }
    this.selected = c.id; this.detailId = c.id; this.message = ''; this.updateMarkdown();
  }
  private closeDetail(): void {
    if (this.busy) { return; }
    this.detailId = ''; this.selected = ''; this.updateMarkdown();
  }
  private deleteCard(c: CardView): void {
    if (!this.canChangeCard(c)) { return; }
    this.getUIContext().showAlertDialog({ title: '移入回收站', message: `删除“${c.title}”？删除后 8 秒内可撤销。`,
      primaryButton: { value: this.t('取消'), action: () => {} }, secondaryButton: { value: '移入回收站', action: () => {
        if (this.canChangeCard(c)) { this.cardAction(c, 'delete'); }
      } } });
  }
  private async copyCard(c: CardView): Promise<void> {
    try {
      await pasteboard.getSystemPasteboard().setData(pasteboard.createData(pasteboard.MIMETYPE_TEXT_PLAIN, c.title + '\n\n' + c.description));
      if (this.pageAlive) { this.message = '已复制卡片文字'; }
    } catch (_) { if (this.pageAlive) { this.message = '卡片文字未能复制，请重试。'; } }
  }
  private detailTask(c: CardView, task: TaskView): void {
    if (!this.canChangeCard(c) || !c.tasks.some((t: TaskView): boolean => t.id === task.id && t.completion === task.completion)) { return; }
    const cmd = new Command(); cmd.action = 'task_toggle'; cmd.id = c.id; cmd.source = c.source; cmd.task_id = task.id;
    cmd.flag = task.completion !== 1; cmd.operation = util.generateRandomUUID(true); cmd.now_ms = Date.now().toString(); this.submit(cmd);
  }
  private pageCards(): CardView[] {
    return this.cards.filter((c: CardView): boolean => !c.deleted && (this.section === '概览' ||
      (this.section === '收藏' ? c.favorite : c.category === this.section)));
  }
  private cardViewToken(): string {
    return JSON.stringify([this.section, this.filter, this.search, this.sort, this.contentGeneration.toString(),
      this.orderRevision.toString(), this.queryPhase, this.queryIds.join(','), this.queryRefreshing.toString()]);
  }
  private canOrderCards(): boolean {
    return this.pageAlive && this.ready && !!cardOrderPage(this.section) && !this.busy && !this.pending && !this.editorOpen && !this.detailId &&
      !this.draftsOpen && !this.settingsOpen && !this.cardOrder.busy && !this.cardOrder.loading && this.queryPhase === 'ready' && !this.queryRefreshing;
  }
  private sortLabel(): string {
    const page = cardOrderPage(this.section);
    return this.orderRevision >= 0 && page && this.cardOrder.manual(page) ? '自定义顺序' : this.sort;
  }
  private canSelectSort(): boolean {
    return this.section === '回收站' ? this.pageAlive && this.ready && !this.busy && !this.pending && !this.editorOpen &&
      !this.detailId && !this.cardOrder.busy && !this.cardOrder.loading : this.canOrderCards();
  }
  private restoreCardOrder(): void {
    try {
      const context = this.getUIContext().getHostContext() as common.UIAbilityContext;
      this.orderStore = preferences.getPreferencesSync(context, { name: 'studio-card-order-development' });
      this.cardOrder.restore();
    } catch (_) { this.orderError = '卡片顺序设置未能读取'; }
  }
  private async selectCardSort(value: string): Promise<void> {
    if (!this.canSelectSort()) { return; }
    const section = this.section; const page = cardOrderPage(section); const prior = this.sort;
    this.sort = value === '自定义顺序' ? '默认顺序' : value;
    if (!page) { return; }
    try {
      const saved = await this.cardOrder.select(page, value === '自定义顺序');
      if (!saved && this.section === section) { this.sort = prior; }
    } catch (_) {
      if (this.section === section) { this.sort = prior; }
      if (this.pageAlive) { this.message = '卡片顺序未能保存：' + this.cardOrder.error; }
    }
  }
  private canNudge(c: CardView, delta: number): boolean {
    const index = this.visibleCards().findIndex((v: CardView): boolean => v.id === c.id);
    return this.canOrderCards() && this.canChangeCard(c) && index >= 0 && index + delta >= 0 && index + delta < this.visibleCards().length;
  }
  private async moveCard(c: CardView, target: string, after: boolean, token: string, revision: number): Promise<void> {
    if (!this.canOrderCards() || !this.canChangeCard(c) || token !== this.cardViewToken() || revision !== this.cardOrder.revision) { return; }
    const page = cardOrderPage(this.section); if (!page) { return; }
    const section = this.section;
    try {
      const saved = await this.cardOrder.move(page, c.id, target, after,
        this.pageCards().map((v: CardView): string => v.id), this.visibleCards().map((v: CardView): string => v.id), revision, token);
      if (saved && this.pageAlive && this.section === section) { this.sort = '默认顺序'; }
      if (!saved && this.cardOrder.error && this.pageAlive) { this.message = '卡片顺序未能保存：' + this.cardOrder.error; }
    } catch (_) { if (this.pageAlive) { this.message = '卡片顺序未能保存：' + this.cardOrder.error; } }
  }
  private nudgeCard(c: CardView, delta: number): void {
    if (!this.canNudge(c, delta)) { return; }
    const visible = this.visibleCards(); const index = visible.findIndex((v: CardView): boolean => v.id === c.id);
    this.moveCard(c, visible[index + delta].id, delta > 0, this.cardViewToken(), this.cardOrder.revision);
  }
  private beginCardDrag(c: CardView): DragItemInfo {
    this.endCardDrag();
    if (!this.canOrderCards() || !this.canChangeCard(c)) { return {}; }
    this.heldCard = new CardDrag(c, this.cardViewToken(), this.cardOrder.revision, util.generateRandomUUID(true));
    this.dragSource = c.id;
    return { extraInfo: this.heldCard.nonce, builder: (): void => { this.CardDragPreview(c); } };
  }
  private acceptsCardDrag(extra: string | undefined, c: CardView): boolean {
    const held = this.heldCard;
    if (!held || !extra || !this.canOrderCards() || !this.canChangeCard(held.card) || !this.canChangeCard(c) ||
      held.token !== this.cardViewToken() || held.revision !== this.cardOrder.revision || held.card.id === c.id) { return false; }
    try { return (JSON.parse(extra) as CardDragExtra).extraInfo === held.nonce; } catch (_) { return false; }
  }
  private hoverCard(event: DragEvent, extra: string | undefined, c: CardView): void {
    const allowed = this.acceptsCardDrag(extra, c);
    event.setResult(allowed ? DragResult.DROP_ENABLED : DragResult.DROP_DISABLED);
    this.dragTarget = allowed ? c.id : '';
  }
  private dropCard(event: DragEvent, extra: string | undefined, c: CardView): void {
    const held = this.heldCard; const bounds = this.cardBounds.get(this.section + ':' + c.id);
    if (!held || !bounds || !this.acceptsCardDrag(extra, c)) { event.setResult(DragResult.DRAG_FAILED); this.endCardDrag(); return; }
    const after = event.getWindowY() >= bounds[0] + bounds[1] / 2;
    event.setResult(DragResult.DRAG_SUCCESSFUL); this.endCardDrag();
    this.moveCard(held.card, c.id, after, held.token, held.revision);
  }
  private endCardDrag(): void { this.heldCard = undefined; this.dragSource = ''; this.dragTarget = ''; }
  private restoreDeleted(c: CardView): void {
    if (!c.deleted || this.busy || this.pending || this.editorOpen || this.cards.find((v: CardView) => v.id === c.id)?.source !== c.source) { return; }
    if (Date.now() < Number(c.deleted_at) || Date.now() - Number(c.deleted_at) >= 8000) {
      this.message = '已超过共享核心的 8 秒撤销删除期限，卡片保持删除状态。'; return;
    }
    const cmd = new Command(); cmd.action = 'restore'; cmd.id = c.id; cmd.source = c.source;
    cmd.operation = util.generateRandomUUID(true); cmd.now_ms = Date.now().toString(); this.submit(cmd);
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
    if (this.attachmentWorking || this.pasteWorking) { this.message = '文件或粘贴操作尚未完成，请稍候。'; return; }
    if (this.busy || this.pending.length > 0 || this.draftWorking || this.draftRetirementUnknown) { this.message = '操作尚未确认，当前输入已保留。'; return; }
    if (this.taskEditId.length > 0) { this.message = '请先保存或取消待办重命名'; return; }
    this.keepDraftAndClose();
  }
  private favoriteCard(c: CardView): void {
    if (!this.canChangeCard(c)) { return; }
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
      if (this.orderWorking) { Text('正在保存顺序…').fontSize(10).fontColor(this.muted()) }
      Blank(); this.Glyph(() => 0xf01a8,  13, () => this.muted()); Text(this.t(this.sortLabel())).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(10).fontColor(this.muted())
    }.padding(6).width('100%').id('card-sort').bindMenu([
      { value: this.t('默认顺序'), enabled: this.canSelectSort(), action: () => { this.selectCardSort('默认顺序'); } },
      { value: this.t('收藏优先'), enabled: this.canSelectSort(), action: () => { this.selectCardSort('收藏优先'); } },
      { value: this.t('标题'), enabled: this.canSelectSort(), action: () => { this.selectCardSort('标题'); } },
      { value: '自定义顺序', enabled: this.canOrderCards(), action: () => { this.selectCardSort('自定义顺序'); } }
    ])
    if (this.orderError) { Text('卡片顺序未能保存 · ' + this.orderError).fontSize(11).fontColor(this.muted()).width('100%') }
    if (this.sortLabel() === '自定义顺序') { Text('长按拖动柄调整顺序，或在卡片菜单中前移、后移。').fontSize(10).fontColor(this.muted()).width('100%') }
  }
  @Builder
  CardDragPreview(c: CardView) {
    Text(c.title).fontSize(14).fontColor(this.ink()).maxLines(2).padding(16).width(220)
      .backgroundColor(this.dark ? '#FF292634' : '#FFFFFFFF').borderRadius(12)
  }
  @Builder
  CardMenu(c: CardView) {
    Menu() {
      MenuItem({ content: '查看详情' }).onClick(() => this.openDetail(c))
      if (!c.deleted) {
        MenuItem({ content: '前移卡片' }).enabled(this.canNudge(c, -1)).onClick(() => this.nudgeCard(c, -1))
        MenuItem({ content: '后移卡片' }).enabled(this.canNudge(c, 1)).onClick(() => this.nudgeCard(c, 1))
        MenuItem({ content: this.t('编辑') }).enabled(this.canChangeCard(c)).onClick(() => this.select(c))
      }
      MenuItem({ content: this.t('复制') }).onClick(() => this.copyCard(c))
      if (c.category === '灵感' && !c.deleted) {
        MenuItem({ content: '转为项目' }).enabled(this.canChangeCard(c)).onClick(() => this.cardAction(c, 'category'))
      }
      if (!c.deleted) {
        MenuItem({ content: c.favorite ? '取消收藏' : '收藏' }).enabled(this.canChangeCard(c)).onClick(() => this.favoriteCard(c))
        MenuItem({ content: '移入回收站' }).enabled(this.canChangeCard(c)).onClick(() => this.deleteCard(c))
        MenuItem({ content: '组件设置' }).onClick(() => {
          if (this.editorOpen || this.busy || this.pending) { return; }
          this.closeDetail(); this.openSettings('组件与卡片'); this.beginMaterial('card:' + c.id);
        })
      } else {
        MenuItem({ content: '撤销删除（8秒内）' }).enabled(!this.busy && !this.pending).onClick(() => this.restoreDeleted(c))
      }
    }
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
      .onClick(() => this.openDetail(c)).bindContextMenu(this.CardMenu(c), ResponseType.LongPress)
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
              Row({ space: 6 }) {
                if (!item.card.deleted) {
                  Button('⠿', { type: ButtonType.Normal }).fontSize(21).fontColor(this.muted()).width(40).height(32).padding(0).backgroundColor(Color.Transparent)
                    .id('card-drag:' + item.card.id).accessibilityText('长按拖动卡片：' + item.card.title)
                    .enabled(this.canOrderCards()).draggable(this.canOrderCards())
                    .onDragStart((): DragItemInfo => this.beginCardDrag(item.card)).onDragEnd(() => this.endCardDrag())
                }
                Blank()
                Button('···', { type: ButtonType.Normal }).fontSize(21).fontColor(this.muted()).width(40).height(32).padding(0).backgroundColor(Color.Transparent)
                  .id('card-menu:' + item.card.id).accessibilityText('卡片操作：' + item.card.title).bindMenu(this.CardMenu(item.card))
              }.width('100%')
            }.width('100%').id('workspace-card:' + item.card.id)
              .backgroundColor(this.dragTarget === item.card.id ? alpha(this.accent(), .12) : Color.Transparent)
              .opacity(this.dragSource === item.card.id ? .55 : 1)
              .onAreaChange((_: Area, area: Area) => { this.cardBounds.set(this.section + ':' + item.card.id, [Number(area.globalPosition.y), Number(area.height)]); })
              .onDragEnter((event: DragEvent, extra?: string) => this.hoverCard(event, extra, item.card))
              .onDragMove((event: DragEvent, extra?: string) => this.hoverCard(event, extra, item.card))
              .onDragLeave(() => { this.dragTarget = ''; })
              .onDrop((event: DragEvent, extra?: string) => this.dropCard(event, extra, item.card))
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
      .border({ width: this.styleEdgeWidth(), color: this.line() }).overlay(this.Rim('card:' + c.id)).onAreaChange((_, area: Area) => this.rememberSurface('card:' + c.id, 20, area)).shadow(this.surfaceShadow('card:' + c.id)).onClick(() => this.openDetail(c)).bindContextMenu(this.CardMenu(c), ResponseType.LongPress)
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
  Detail(c: CardView) {
    Column({ space: 16 }) {
      Row({ space: 12 }) {
        this.Glyph(() => this.icon(c.category), 24, () => this.accent())
        Column({ space: 5 }) {
          Text(c.title).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(22).fontWeight(FontWeight.Medium).fontColor(this.ink())
            .maxLines(3).width('100%').textOverflow({ overflow: TextOverflow.Ellipsis }).id('detail-title')
          Text('想法详情').fontSize(11).fontColor(this.muted())
        }.alignItems(HorizontalAlign.Start).layoutWeight(1)
        Button('×', { type: ButtonType.Normal }).fontSize(22).fontColor(this.muted()).width(36).height(36).padding(0).backgroundColor(Color.Transparent)
          .accessibilityText('关闭详情').id('detail-close').enabled(!this.busy).onClick(() => this.closeDetail())
      }.width('100%')
      Scroll(this.detailScroller) {
        Column({ space: 18 }) {
          Text(this.t(c.category)).fontSize(12).fontColor(this.accent()).width('100%')
          this.PreviewCleanupNotice()
          this.FileOpenNotice()
          if (this.markdownLoading) { Text('正在生成预览…').fontSize(12).fontColor(this.muted()) }
          if (this.markdownError) {
            Text('预览暂不可用 · ' + this.markdownError).fontSize(12).fontColor(this.muted())
            Button('重新生成预览', { type: ButtonType.Normal }).fontSize(12).fontColor(this.accent()).backgroundColor(Color.Transparent).onClick(() => this.markdown.retry())
          }
          if (!c.description.trim()) { Text('给这个念头，留一点生长的空间。').fontSize(13).fontColor(this.muted()) }
          MarkdownPreview({ document: this.markdownDoc, ink: this.ink(), muted: this.muted(), accent: this.accent(),
            line: this.line(), family: this.fontFamily || 'HarmonyOS Sans', locale: this.locale,
            attachments: this.markdownAssets, images: this.markdownImages,
            openImage: (id: string): void => this.openInlineImage(id),
            retryImage: (id: string): void => this.inlineImages.retry(id),
            imageError: (id: string, path: string): void => this.inlineImages.decodeFailed(id, path),
            openLink: (href: string): void => { this.openMarkdownLink(href); } })
          if (c.category === '实验') {
            Text(this.t('假设 / 想试什么')).fontSize(12).fontColor(this.accent()).width('100%')
            Text(c.hypothesis || '还没有写下假设。').fontSize(13).lineHeight(22).fontColor(this.ink()).copyOption(CopyOptions.LocalDevice).width('100%')
            Text(this.t('观察 / 留下发现')).fontSize(12).fontColor(this.accent()).width('100%')
            Text(c.conclusion || '还没有留下观察记录。').fontSize(13).lineHeight(22).fontColor(this.ink()).copyOption(CopyOptions.LocalDevice).width('100%')
          }
          if (!c.deleted) {
            Row({ space: 12 }) {
              Select(['灵感', '进行中', '实验'].map((s: string): SelectOption => { return { value: this.t(s) }; }))
                .value(this.t(c.category)).font({ size: 12 }).fontColor(this.accent()).backgroundColor(Color.Transparent).id('detail-category')
                .enabled(this.canChangeCard(c)).onSelect((index: number) => {
                  const value = ['灵感', '进行中', '实验'][index]; if (value && value !== c.category) { this.cardAction(c, 'category', value); }
                })
              Blank()
              Select((c.category === '灵感' ? ['待整理', '已整理'] : c.category === '进行中' ? ['计划中', '推进中', '已完成'] : ['待验证', '验证中', '已记录'])
                .map((s: string): SelectOption => { return { value: s }; }))
                .value(c.stage).font({ size: 12 }).fontColor(this.muted()).backgroundColor(Color.Transparent).id('detail-stage')
                .enabled(this.canChangeCard(c)).onSelect((_: number, value: string) => { if (value !== c.stage) { this.cardAction(c, 'stage', value); } })
            }.width('100%')
          }
          if (c.tasks.length > 0) {
            Text(`待办事项 · ${c.tasks.filter((task: TaskView): boolean => task.completion === 1).length} / ${c.tasks.length}`).fontSize(12).fontColor(this.accent()).width('100%')
            ForEach(c.tasks, (task: TaskView) => {
              Row({ space: 10 }) {
                Button(task.completion === 1 ? '✓' : task.completion === 2 ? '?' : '○', { type: ButtonType.Normal })
                  .fontSize(18).fontColor(this.accent()).width(32).height(32).padding(0).backgroundColor(alpha(this.accent(), .08))
                  .id('detail-task:' + task.id).enabled(this.canChangeCard(c)).onClick(() => this.detailTask(c, task))
                Text(task.text).fontSize(13).fontColor(this.ink()).layoutWeight(1).copyOption(CopyOptions.LocalDevice)
              }.width('100%')
            }, (task: TaskView) => task.id + ':' + task.completion)
          }
          if (c.assets.length > 0) {
            Text('附件 · ' + c.assets.length).fontSize(12).fontColor(this.accent()).width('100%')
            ForEach(c.assets, (asset: AssetView) => {
              Row({ space: 8 }) {
                this.Glyph(() => attachmentIcon(asset), 18, () => this.accent())
                Column({ space: 4 }) {
                  Text(asset.name).fontSize(12).fontColor(this.ink()).maxLines(2)
                  Text(attachmentDetails(asset)).fontSize(10).fontColor(this.muted())
                }.alignItems(HorizontalAlign.Start).layoutWeight(1)
                if (['image', 'gif', 'audio', 'video'].includes(asset.kind)) {
                  Button('预览', { type: ButtonType.Normal }).fontSize(11).fontColor(this.accent()).backgroundColor(Color.Transparent).enabled(!this.attachmentWorking).onClick(() => this.previewAttachment(asset, c)).id('attachment-preview:' + asset.id)
                } else {
                  Button('打开', { type: ButtonType.Normal }).fontSize(11).fontColor(this.accent()).backgroundColor(Color.Transparent)
                    .enabled(!this.attachmentWorking && !this.fileOpenState.retained && !this.fileOpenState.busy)
                    .onClick(() => this.openSystemAttachment(asset, c)).id('attachment-open:' + asset.id)
                }
                Button('导出', { type: ButtonType.Normal }).fontSize(11).fontColor(this.accent()).backgroundColor(Color.Transparent).enabled(!this.attachmentWorking).onClick(() => this.exportAttachment(c, asset)).id('attachment-export:' + asset.id)
              }.width('100%').padding(10).backgroundColor('#0B7662BA').borderRadius(10).id('saved-asset:' + asset.id)
            }, (asset: AssetView) => asset.id)
          }
          if (c.deleted) { Text('这张卡片已移入回收站，删除后 8 秒内可以撤销。').fontSize(12).fontColor(this.muted()).width('100%') }
          if (this.message) { Text(this.message).fontSize(12).lineHeight(20).fontColor(this.muted()).width('100%').id('detail-message') }
          if (this.pending && !this.busy) { Button(this.t('核对')).fontSize(12).backgroundColor(this.accent()).onClick(() => this.retry()) }
        }.alignItems(HorizontalAlign.Start).width('100%')
      }.height('auto').constraintSize({ maxHeight: Math.max(150, this.viewportHeight - 270) }).width('100%').scrollBar(BarState.Off).align(Alignment.TopStart).id('detail-scroll')
        .onDidScroll(() => { this.detailOffset = this.detailScroller.currentOffset().yOffset; })
        .onAppear(() => {
          const offset = this.detailOffset;
          setTimeout(() => { if (this.pageAlive && this.detailId === c.id) { this.detailScroller.scrollTo({ xOffset: 0, yOffset: offset }); } }, 16);
        })
      Flex({ wrap: FlexWrap.Wrap, justifyContent: FlexAlign.End }) {
        Button(this.t('复制'), { type: ButtonType.Normal }).fontSize(12).fontColor(this.accent()).backgroundColor(Color.Transparent).height(40).onClick(() => this.copyCard(c)).id('detail-copy')
        if (c.deleted) {
          Button('撤销删除（8秒内）', { type: ButtonType.Normal }).fontSize(12).fontColor(this.accent()).backgroundColor(Color.Transparent).height(40)
            .enabled(!this.busy && !this.pending).onClick(() => this.restoreDeleted(c))
        } else {
          Button(c.favorite ? '取消收藏' : '收藏', { type: ButtonType.Normal }).fontSize(12).fontColor(this.accent()).backgroundColor(Color.Transparent).height(40).enabled(this.canChangeCard(c)).onClick(() => this.favoriteCard(c))
          Button('删除', { type: ButtonType.Normal }).fontSize(12).fontColor(this.muted()).backgroundColor(Color.Transparent).height(40).enabled(this.canChangeCard(c)).onClick(() => this.deleteCard(c))
          Button(this.t('编辑'), { type: ButtonType.Normal }).fontSize(12).fontColor(this.accent()).backgroundColor(Color.Transparent).height(40).enabled(this.canChangeCard(c)).id('detail-edit').onClick(() => this.select(c))
        }
        Button('完成', { type: ButtonType.Normal }).fontSize(12).fontColor(this.ink()).backgroundColor(alpha(this.accent(), .12)).height(40).enabled(!this.busy).onClick(() => this.closeDetail())
      }.width('100%')
    }.padding(24).width(Math.min(470, this.viewportWidth - 24)).constraintSize({ maxHeight: this.viewportHeight - 40 })
      .backgroundColor(this.dark ? '#FA292634' : '#FAFFFFFF').backdropBlur(20).borderRadius(this.round(23)).border({ width: this.styleEdgeWidth(), color: this.line() }).id('card-detail')
  }
  @Builder
  BodyPreview() {
    Scroll() {
      Column({ space: 7 }) {
        if (!this.description.trim()) { Text(this.t('写一点内容，再看看它的样子。')).fontSize(13).fontColor(this.muted()) }
        if (this.markdownLoading) { Text('正在生成预览…').fontSize(12).fontColor(this.muted()) }
        if (this.markdownError) {
          Text('预览暂不可用 · ' + this.markdownError).fontSize(12).fontColor(this.muted())
          Button('重新生成预览', { type: ButtonType.Normal }).fontSize(12).backgroundColor(Color.Transparent).fontColor(this.accent()).onClick(() => this.markdown.retry())
        }
        MarkdownPreview({ document: this.markdownDoc, ink: this.ink(), muted: this.muted(), accent: this.accent(),
          line: this.line(), family: this.fontFamily || 'HarmonyOS Sans', locale: this.locale,
          attachments: this.markdownAssets, images: this.markdownImages,
          openImage: (id: string): void => this.openInlineImage(id),
          retryImage: (id: string): void => this.inlineImages.retry(id),
          imageError: (id: string, path: string): void => this.inlineImages.decodeFailed(id, path),
          openLink: (href: string): void => { this.openMarkdownLink(href); } })
      }.alignItems(HorizontalAlign.Start).width('100%').padding(8)
    }.height(250).width('100%').scrollBar(BarState.Off).align(Alignment.TopStart).id('markdown-preview')
  }
  @Builder
  BodyInput() {
    TextArea({ text: this.description, placeholder: this.t('写下思路，或粘贴一段内容…'), controller: this.descriptionController })
      .fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(13).placeholderFont({ size: 13 }).fontColor(this.ink()).placeholderColor(this.muted())
      .backgroundColor(Color.Transparent).border({ width: this.styleEdgeWidth(), color: this.line() }).borderRadius(this.round(12)).height(180).maxLength(20000)
      .enabled(!this.attachmentWorking && !this.pasteWorking && !this.draftRetiring && !this.draftRetirementUnknown).id('draft-description')
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
  EditorAttachments() {
    Column({ space: 9 }) {
      this.PreviewCleanupNotice()
      this.FileOpenNotice()
      if (this.attachmentSelectionState.busy || this.attachmentSelectionState.selected > 0 || this.attachmentSelectionState.error) {
        Row({ space: 8 }) {
          Text(this.attachmentSelectionState.busy ? '正在导入附件…' : this.attachmentSelectionState.error || '附件已保留在草稿中。')
            .fontSize(11).fontColor(this.muted()).layoutWeight(1)
          Text(`${this.attachmentSelectionState.selected}/${this.attachmentSelectionState.total}`).fontSize(11).fontColor(this.accent())
          if (this.attachmentSelectionState.busy) {
            Button('停止后续导入', { type: ButtonType.Normal }).fontSize(11).fontColor(this.accent()).backgroundColor(Color.Transparent)
              .onClick(() => this.attachmentSelection?.stop()).id('attachment-import-stop')
          }
        }.width('100%').padding(8).id('attachment-import-progress')
      }
      if (this.editorAssets.length > 0) {
        Text('草稿附件').fontSize(13).fontWeight(FontWeight.Medium).fontColor(this.ink()).width('100%')
        ForEach(this.editorAssets, (asset: AssetView) => {
          Row({ space: 8 }) {
            this.Glyph(() => attachmentIcon(asset), 18, () => this.accent())
            Column({ space: 4 }) {
              Text(asset.name).fontSize(12).fontColor(this.ink()).maxLines(2)
              Text(attachmentDetails(asset)).fontSize(10).fontColor(this.muted())
            }.alignItems(HorizontalAlign.Start).layoutWeight(1)
            if (['image', 'gif', 'audio', 'video'].includes(asset.kind)) {
              Button('预览', { type: ButtonType.Normal }).fontSize(11).backgroundColor(Color.Transparent).fontColor(this.accent())
                .enabled(!this.attachmentWorking && !this.busy).onClick(() => this.previewAttachment(asset)).id('draft-asset-preview:' + asset.id)
            } else {
              Button('打开', { type: ButtonType.Normal }).fontSize(11).fontColor(this.accent()).backgroundColor(Color.Transparent)
                .enabled(this.canImportAttachment() && !this.fileOpenState.retained && !this.fileOpenState.busy)
                .onClick(() => this.openSystemAttachment(asset)).id('draft-asset-open:' + asset.id)
            }
            Button('导出', { type: ButtonType.Normal }).fontSize(11).backgroundColor(Color.Transparent).fontColor(this.accent())
              .enabled(this.canImportAttachment()).onClick(() => this.exportEditorAttachment(asset)).id('draft-asset-export:' + asset.id)
            Button('移除', { type: ButtonType.Normal }).fontSize(11).backgroundColor(Color.Transparent).fontColor(this.muted())
              .enabled(this.canImportAttachment()).onClick(() => this.removeEditorAsset(asset)).id('draft-asset-remove:' + asset.id)
          }.width('100%').padding(10).backgroundColor('#0B7662BA').borderRadius(10).id('draft-asset:' + asset.id)
        }, (asset: AssetView) => asset.id)
      }
      ForEach(this.importRecords.filter((item: ImportRecord): boolean => !this.editorAssets.some((asset: AssetView): boolean => asset.id === item.asset_id)), (item: ImportRecord) => {
        Column({ space: 7 }) {
          Text(item.request.name + ' · ' + attachmentSize(item.request.byte_length)).fontSize(12).fontColor(this.ink()).width('100%')
          Text(item.phase === 'retired' ? '导入已结束，清理待确认' : item.phase === 'ready' ? '已导入，尚未加入草稿' : item.bytes_retained ? '字节已保留，导入结果待核对' : '导入待完成').fontSize(10).fontColor(this.muted()).width('100%')
          Row({ space: 10 }) {
            if (item.phase === 'ready' && item.current_active) {
              Button('加入草稿', { type: ButtonType.Normal }).fontSize(11).backgroundColor(Color.Transparent).fontColor(this.accent()).enabled(this.canImportAttachment()).onClick(() => this.selectPendingImport(item))
            }
            if (item.phase === 'retired') {
              Button('核对清理', { type: ButtonType.Normal }).fontSize(11).backgroundColor(Color.Transparent).fontColor(this.accent()).enabled(this.canImportAttachment()).onClick(() => this.reconcileImports())
            } else {
              Button('放弃导入', { type: ButtonType.Normal }).fontSize(11).backgroundColor(Color.Transparent).fontColor(this.muted()).enabled(this.canImportAttachment()).onClick(() => this.abandonImport(item))
            }
          }.width('100%')
        }.padding(10).backgroundColor('#0B7662BA').borderRadius(10).width('100%').id('pending-import:' + item.request.operation_id)
      }, (item: ImportRecord) => item.request.operation_id + ':' + item.phase)
      ForEach(this.pendingSpools, (spool: PreparedAttachment) => {
        Row({ space: 10 }) {
          Text(spool.name + (spool.original_request ? ' · 原导入待核对' : ' · 尚未开始导入')).fontSize(11).fontColor(this.muted()).layoutWeight(1)
          if (spool.original_request) {
            Button('核对导入', { type: ButtonType.Normal }).fontSize(11).backgroundColor(Color.Transparent).fontColor(this.accent()).enabled(this.canImportAttachment()).onClick(() => this.retryAttachment(spool))
          } else {
            Button('取消准备', { type: ButtonType.Normal }).fontSize(11).backgroundColor(Color.Transparent).fontColor(this.muted()).enabled(!this.attachmentWorking).onClick(() => this.cancelPrepared(spool))
          }
        }.width('100%')
      }, (spool: PreparedAttachment) => spool.spool_id)
      ForEach(this.attachmentDiagnostics, (issue: AttachmentRecoveryDiagnostic) => {
        Row({ space: 10 }) {
          Text(issue.message).fontSize(11).fontColor(this.muted()).layoutWeight(1)
          if (issue.requires_review) {
            Button('重新检查', { type: ButtonType.Normal }).fontSize(11).backgroundColor(Color.Transparent).fontColor(this.accent())
              .enabled(!this.attachmentWorking).onClick(() => this.loadImports())
          }
        }.width('100%').id('attachment-recovery:' + issue.spool_id)
      }, (issue: AttachmentRecoveryDiagnostic) => issue.spool_id + ':' + issue.code)
      if (this.attachmentPending) {
        Button('核对附件清理', { type: ButtonType.Normal }).fontSize(11).fontColor(this.accent()).backgroundColor(Color.Transparent).enabled(!this.attachmentWorking).onClick(() => this.retryAttachmentCleanup())
      }
    }.width('100%')
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
            .enabled(!this.attachmentWorking && !this.pasteWorking && !this.draftRetiring && !this.draftRetirementUnknown).id('draft-title')
            .onChange((s: string, preview?: PreviewText) => this.editorInputChanged('title', s, preview))
            .onTextSelectionChange((start: number, end: number) => this.draftSelectionChanged('title', start, end))
            .onFocus(() => this.focusDraftField('title')).onBlur(() => this.blurDraftField('title'))
          Text(`${this.title.length} / 60`).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(10).fontColor(this.muted()).width('100%').textAlign(TextAlign.End)
          Row({ space: 8 }) {
            PasteButton({ icon: PasteIconStyle.LINES, text: PasteDescription.PASTE, buttonType: ButtonType.Normal })
              .height(40).fontSize(13).fontColor('#302D43').iconColor('#302D43').backgroundColor('#E9E4F8')
              .padding({ left: 16, right: 16 }).borderRadius(12).enabled(this.canPaste()).id('editor-paste')
              .onClick((_: ClickEvent, result: PasteButtonOnClickResult) => {
                if (result === PasteButtonOnClickResult.SUCCESS) { this.pasteContent(); }
                else { this.message = '剪贴板读取未获授权，请再次点击粘贴按钮。'; }
              })
            Button({ type: ButtonType.Normal }) { Row({ space: 6 }) { this.Glyph(() => 0xe0b1,  16, () => this.muted()); Text(this.t('导入文件')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(13).fontColor(this.muted()) } }
              .height(40).padding({ left: 16, right: 16 }).borderRadius(this.round(12)).backgroundColor(Color.Transparent).border({ width: this.styleEdgeWidth(), color: this.line() })
              .enabled(this.canImportAttachment()).id('editor-import').onClick(() => this.importAttachment())
          }.justifyContent(FlexAlign.Center).width('100%')
          Text(this.attachmentWorking ? '正在处理附件…' : this.pasteWorking ? '正在读取粘贴内容…' : '粘贴至' + this.pasteTargetName() + ' · 附件从文件选择器导入。')
            .fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(11).lineHeight(18).fontColor(this.muted()).width('100%')
          this.EditorAttachments()
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
                if (this.pasteWorking || this.draftWorking || this.attachmentWorking || this.draftRetiring || this.draftRetirementUnknown || this.draftRestoreInput) { return; }
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
                  if (this.pasteWorking || this.draftWorking || this.attachmentWorking || this.draftRetiring || this.draftRetirementUnknown || this.draftRestoreInput) { return; }
                  if (this.dirty) { this.message = '请先保存正文编辑'; return; }
                  const cmd = this.command('stage'); cmd.stage = value; this.submit(cmd);
                })
            }
          }.width('100%')
          if (this.category === '实验') {
            TextArea({ text: this.hypothesis, placeholder: this.t('假设'), controller: this.hypothesisController }).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(12).placeholderFont({ size: 12 }).fontColor(this.ink()).backgroundColor(this.glass()).maxLength(5000).height(96)
              .enabled(!this.attachmentWorking && !this.pasteWorking && !this.draftRetiring && !this.draftRetirementUnknown).id('draft-hypothesis')
              .onChange((s: string, preview?: PreviewText) => this.editorInputChanged('hypothesis', s, preview))
              .onTextSelectionChange((start: number, end: number) => this.draftSelectionChanged('hypothesis', start, end)).onFocus(() => this.focusDraftField('hypothesis')).onBlur(() => this.blurDraftField('hypothesis'))
            TextArea({ text: this.conclusion, placeholder: this.t('结论'), controller: this.conclusionController }).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(12).placeholderFont({ size: 12 }).fontColor(this.ink()).backgroundColor(this.glass()).maxLength(10000).height(116)
              .enabled(!this.attachmentWorking && !this.pasteWorking && !this.draftRetiring && !this.draftRetirementUnknown).id('draft-conclusion')
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
                  .enabled(!this.attachmentWorking && !this.pasteWorking && !this.draftRetiring && !this.draftRetirementUnknown).id('draft-todos')
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
            if (!this.editorOpen && !this.detailId) { this.FileOpenNotice() }
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
      } else if (this.detailId) {
        Stack() {
          Rect().width('100%').height('100%').fill('#40302D43').onClick(() => this.closeDetail())
          ForEach(this.cards.filter((c: CardView): boolean => c.id === this.detailId), (c: CardView) => {
            this.Detail(c)
          }, (c: CardView) => c.id + ':' + c.revision)
        }.width('100%').height('100%')
      } else if (this.message.length > 0) {
        Row({ space: 8 }) {
          Text(this.message).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(11).fontColor(this.ink()).layoutWeight(1)
          if (this.pending.length > 0 && !this.busy) { Button(this.t('核对')).fontSize(11).backgroundColor(this.accent()).onClick(() => this.retry()) }
          else { Text(this.t('×')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(18).fontColor(this.muted()).onClick(() => { this.message = ''; }) }
        }.padding(14).width('90%').backgroundColor(this.dark ? '#F5292634' : '#F5FFFFFF').borderRadius(this.round(12))
          .border({ width: this.styleEdgeWidth(), color: this.line() }).position({ x: '5%', y: Math.max(0, this.viewportHeight - 124) })
      }
      if (this.attachmentPreviewOpen) {
        Stack() {
          Rect().width('100%').height('100%').fill('#B0302D43').onClick(() => this.closeAttachmentPreview())
          Column({ space: 12 }) {
            Text(this.attachmentPreview?.name || this.attachmentPreviewTitle || '附件预览').fontSize(15).fontColor(this.ink()).width('100%').maxLines(2)
            if (['audio', 'video'].includes(this.attachmentPreviewKind)) {
              AttachmentMediaPreview({ view: this.mediaView, loading: this.attachmentPreviewLoading, failure: this.attachmentPreviewFailure,
                contentWidth: (this.mediaFullscreen ? this.viewportWidth : Math.min(656, this.viewportWidth - 24)) - 36,
                contentHeight: this.mediaFullscreen ? Math.max(160, this.viewportHeight - 130) : Math.max(160, Math.min(320, this.viewportHeight - 180)),
                ink: this.ink(), muted: this.muted(), accent: this.accent(), family: this.fontFamily || 'HarmonyOS Sans', locale: this.locale,
                fullscreen: this.mediaFullscreen,
                onToggle: (token: string): void => { if (!this.attachmentPreviewClosing && token === this.mediaView.token) { this.mediaPlayback.toggle(token); } },
                onSeek: (value: number, token: string): void => { if (!this.attachmentPreviewClosing && token === this.mediaView.token) { this.mediaPlayback.seek(value, token); } },
                onSurface: (token: string, id: string): void => this.mediaPlayback.setSurface(token, id),
                onSurfaceGone: (token: string, id: string): void => this.mediaPlayback.clearSurface(token, id),
                onBackground: (token: string): void => { if (token) { this.mediaPlayback.pauseForBackground(token); } },
                onFullscreen: (token: string): void => { this.switchMediaFullscreen(token); } })
            } else {
              AttachmentImagePreview({ preview: this.attachmentPreview, loading: this.attachmentPreviewLoading, failure: this.attachmentPreviewFailure,
                contentWidth: Math.min(660, this.viewportWidth - 24) - 36, contentHeight: Math.max(160, this.viewportHeight - 270),
                ink: this.ink(), muted: this.muted(), accent: this.accent(), family: this.fontFamily || 'HarmonyOS Sans', locale: this.locale })
            }
            if (this.fullscreenState.error) { Text(this.fullscreenState.error).fontSize(11).fontColor(this.muted()).width('100%').id('attachment-fullscreen-failure') }
            Button(this.attachmentPreviewClosing ? '正在关闭…' : this.mediaView.phase === 'cleanup_failed' || this.fullscreenState.phase === 'restore_failed' ? '重试关闭' : '关闭预览', { type: ButtonType.Normal })
              .fontSize(12).fontColor(this.accent()).backgroundColor(Color.Transparent).enabled(!this.attachmentPreviewClosing)
              .onClick(() => this.closeAttachmentPreview()).id('attachment-preview-close')
          }.padding(18).width(this.mediaFullscreen ? this.viewportWidth : Math.min(660, this.viewportWidth - 24)).backgroundColor(this.surface()).borderRadius(this.mediaFullscreen ? 0 : this.round(18))
        }.width('100%').height('100%').id('attachment-preview-dialog')
      }
    }.width('100%').height('100%').backgroundColor(this.canvasBackgroundColorValue())
      .onAreaChange((_, area: Area) => { this.viewportWidth = Number(area.width); this.viewportHeight = Number(area.height); })
  }
}
