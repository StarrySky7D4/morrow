import { ColorWheel } from './ColorWheel';
import { MarkdownDoc, MarkdownPreviewCoordinator, MarkdownAttachment, collectMarkdownImages, resolveMarkdownImage } from '../model/Markdown';
import { MarkdownPreview } from './MarkdownPreview';
import { AttachmentImagePreview } from './AttachmentImagePreview';
import { AttachmentMediaPreview, createAttachmentPlayer } from './AttachmentMediaPreview';
import { AttachmentPlayback, PlaybackSource, PlaybackView } from '../model/AttachmentPlayback';
import { MusicWorkbench } from '../model/MusicWorkbench';
import { MusicUiCallbacks, MusicUiView } from '../model/MusicUi';
import { MusicPanelContent } from './MusicPanel';
import { MusicFooterContent, LyricsDialogContent } from './MusicFooter';
import { AttachmentOpen, AttachmentOpenState, attachmentOpenBlocked } from '../model/AttachmentOpen';
import { AttachmentFullscreen, FullscreenView, FullscreenWindow, createFullscreenWindow } from '../model/AttachmentFullscreen';
import { AttachmentImportSelection, AttachmentImportBoundary, AttachmentImportIdentity, AttachmentImportReceipt,
  AttachmentImportSelectionState, attachmentImportTitle } from '../model/AttachmentImportSelection';
import { ImageReadTarget, ImageLease, VerifiedImageView, VerifiedImages } from '../model/VerifiedImages';
import { samePasteTarget } from '../model/EditorPaste';
import { EditorFieldPolicy, editorFieldLimit } from '../model/EditorFieldPolicy';
import { EditorInputPolicy } from '../model/EditorInputPolicy';
import { editorInputHash } from '../model/EditorInputHash';
import { EditorDirectInput, EditorDirectInputProposal } from '../model/EditorDirectInput';
import { EditorTodos } from './EditorTodos';
import { EditorViewLease } from './EditorViewLease';
import { EditorTodosStatus } from '../model/EditorTodos';
import { ClipboardInput, ClipboardSnapshot, ClipboardSource } from '../model/ClipboardInput';
import { ClipboardPaste, ClipboardPasteAttachment, ClipboardPastePlan, ClipboardPasteBoundary,
  ClipboardPasteIdentity, ClipboardPasteReceipt } from '../model/ClipboardPaste';
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
import { AttachmentFiles, PreparedAttachment, VerifiedPreview, AttachmentRecoveryDiagnostic, ClipboardConversion } from '../model/AttachmentFiles';
import { AssetView, ImportRecord, ImportCommand, attachmentKind, attachmentSize, attachmentIcon, attachmentDetails, selectedImport, sameImport } from '../model/Attachments';
import { QueryConditions, QueryState, QueryCoordinator } from '../model/QueryCoordinator';
import { Values as DraftValues, TextValue as DraftTextValue, DraftScope, DraftRecord,
  AssetSelection, EditorDraftCoordinator, DraftSaveFailure, copyValues, copyText, copyScope, sameValues, nextGeneration,
  validateRecord, validGeneration, sameForkLink, copyRecord } from '../model/EditorDraft';
import { EditorDraftForkCoordinator } from '../model/EditorDraftFork';
import { EditorBusinessFields, EditorBusinessSubmission } from '../model/EditorBusiness';
import { EditorBusinessSessionCoordinator, EditorBusinessSessionHooks, EditorBusinessSessionReply, EditorBusinessSessionOwnerHooks,
  EditorIntentView, EditorIntentProof } from '../model/EditorBusinessSession';
import { EditorBusinessHandoffCoordinator, EditorBusinessHandoffHooks, EditorBusinessHandoffReply } from '../model/EditorBusinessHandoff';
import { EditorBusinessRecoveryCoordinator, EditorBusinessRecoveryReady } from '../model/EditorBusinessRecovery';

class BusinessEditorBinding {
  parent: EditorDraftCoordinator | undefined;
  liveDraft: EditorDraftCoordinator;
  identity: string;
  owner: string;
  epoch: number;
  values: DraftValues;
  publication: DraftRecord;
  constructor(parent: EditorDraftCoordinator | undefined, identity: string, owner: string, epoch: number,
    values: DraftValues, publication: DraftRecord, liveDraft?: EditorDraftCoordinator) {
    if (!parent && !liveDraft) { throw new Error('Business editor needs a real current writer'); }
    this.parent = parent; this.liveDraft = liveDraft ?? parent!; this.identity = identity; this.owner = owner;
    this.epoch = epoch; this.values = copyValues(values); this.publication = publication;
  }
}

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
interface RawForkRetirementIdentity { operation_id: string; }
interface RawForkRetirementCommand { fork_retirement: RawForkRetirementIdentity; }

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
  @State taskRenameOwner: string = '';
  @State taskRenameText: string = '';
  @State fieldCountLabels: string[] = ['', '', '', '', '', ''];
  @State fieldValidationWorking: boolean = false;
  private fieldPolicy: EditorFieldPolicy = new EditorFieldPolicy((request: string): Promise<string> => native.editorField(request));
  private fieldCountEpochs: Map<string, number> = new Map();
  private editorInputEpoch: number = 0;
  private inputRevisions: Map<string, number> = new Map();
  @State inputRulesRevision: number = 0;
  @State inputRulesMessage: string = '';
  @State todoInputValue: DraftTextValue = new DraftTextValue();
  @State todoInputRevision: number = 0;
  @State todoBusinessReady: boolean = false;
  @State todoFormatPending: boolean = false;
  private inputPolicy: EditorInputPolicy = new EditorInputPolicy((request: string): Promise<string> => native.editorInput(request), editorInputHash);
  private directInput: EditorDirectInput = new EditorDirectInput({
    isCurrent: (owner: string, key: string, revision: number, value: DraftTextValue): boolean => this.directInputCurrent(owner, key, revision, value),
    format: (field: string, oldValue: DraftTextValue, newValue: DraftTextValue, owned: () => boolean) => this.inputPolicy.format(field, oldValue, newValue, owned),
    apply: (proposal: EditorDirectInputProposal, owned: () => boolean): number | undefined => this.applyDirectInput(proposal, owned),
    changed: (): void => this.directInputChanged()
  });
  private taskRenameValue: DraftTextValue = new DraftTextValue();
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
  @State businessWorking: boolean = false;
  @State businessStatus: string = '';
  @State businessIntents: EditorIntentView[] = [];
  @State businessListError: string = '';
  @State businessNext: string = '';
  private businessSession: EditorBusinessSessionCoordinator | undefined = undefined;
  private businessBinding: BusinessEditorBinding | undefined = undefined;
  private ownedTodosBaseline: EditorBusinessSessionCoordinator | undefined = undefined;
  private businessRecoveryIdentity: string = '';
  private businessRecovery: EditorBusinessRecoveryCoordinator | undefined = undefined;
  private businessRecoverySession: EditorBusinessSessionCoordinator | undefined = undefined;
  private editorOpenIdentity: string = '';
  private editorBusinessMode: string = 'edit';
  private businessHandoff: EditorBusinessHandoffCoordinator | undefined = undefined;
  private businessHandoffCloseOperation: string = '';
  private businessCloseExact: boolean = false;
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
  @State musicView: MusicUiView = new MusicUiView();
  private music: MusicWorkbench | undefined = undefined;
  private musicPage: string = util.generateRandomUUID(true);
  private musicEpoch: number = 0;
  private mediaToggleWorking: boolean = false;
  private musicCallbacks: MusicUiCallbacks = {
    action: (action: string, trackId: string, value: number, spoolId: string): void => { this.musicAction(action, trackId, value, spoolId); },
    reorder: (trackIds: string[]): void => { this.musicReorder(trackIds); }
  };
  @State fileOpenState: AttachmentOpenState = new AttachmentOpenState();
  private fileOpen: AttachmentOpen | undefined = undefined;
  private attachmentPreviewEpoch: number = 0;
  @State previewCleanupPending: boolean = false;
  @State previewCleanupWorking: boolean = false;
  @State draftConflict: boolean = false;
  @State draftRetiring: boolean = false;
  @State draftRetirementUnknown: boolean = false;
  @State draftRetiredIdentity: string = '';
  private retirementInput: Map<string, string> = new Map();
  private draftRetirement: string = '';
  private submittedRaw: DraftValues | undefined = undefined;
  private submittedInputEpoch: number = -1;
  private submittedEditor: string = '';
  private submittedDraft: EditorDraftCoordinator | undefined = undefined;
  private submittedRename: DraftTextValue | undefined = undefined;
  private submittedRenameEpoch: number = -1;
  private submittedRenameEditor: string = '';
  private editorDraft: EditorDraftCoordinator | undefined = undefined;
  @State editorViewVisible: boolean = true;
  @State editorViewOwner: string = '';
  @State editorFocusIntent: number = 0;
  private editorViewRevoked: boolean = true;
  private editorDetachResolve: (() => void) | undefined = undefined;
  private editorMountResolve: ((mounted: boolean) => void) | undefined = undefined;
  private editorBoundaryDraft: EditorDraftCoordinator | undefined = undefined;
  @State editorBoundary: string = '';
  private rawFork: EditorDraftForkCoordinator | undefined = undefined;
  private rawForkParent: EditorDraftCoordinator | undefined = undefined;
  private rawForkOwner: string = '';
  private rawForkRetirement: string = '';
  private rawForkRetirementUnknown: boolean = false;
  @State rawForkWorking: boolean = false;
  @State rawForkAction: string = '';
  @State rawForkStatus: string = '';
  private editorValues: DraftValues = new DraftValues();
  private draftSource: string = '';
  @State private draftRestoreInput: boolean = false;
  private draftSelectionPending: Set<string> = new Set();
  private draftFocusedFields: Set<string> = new Set();
  private draftCaptureBlocked: Set<string> = new Set();
  @State draftCaptureIncomplete: boolean = false;
  private titleController: TextInputController = new TextInputController();
  private descriptionController: TextAreaController = new TextAreaController();
  private hypothesisController: TextAreaController = new TextAreaController();
  private conclusionController: TextAreaController = new TextAreaController();
  private taskController: TextInputController = new TextInputController();
  private taskRenameController: TextInputController = new TextInputController();
  private editorScroller: Scroller = new Scroller();
  private todoDragY: number = 0;
  private todoDragTimer: number = -1;
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
  private clipboardInput: ClipboardInput = new ClipboardInput();
  private clipboardPaste: ClipboardPaste | undefined = undefined;
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
    if (!this.music) {
      this.music = new MusicWorkbench(context, {
        owned: (): boolean => this.pageAlive,
        foreground: (): boolean => this.foreground,
        owner: (): string => this.musicPage + ':' + this.musicEpoch.toString(),
        changed: (): void => { if (this.pageAlive && this.music) { this.musicView = this.music.view(); } },
        beforePlay: (): Promise<void> => this.beforeMusicPlay()
      });
    }
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
    if (this.musicView.lyricsOpen) { this.musicAction('lyrics_close'); return true; }
    if (this.attachmentPreviewOpen && this.mediaFullscreen) { this.exitMediaFullscreen(); return true; }
    if (this.attachmentPreviewOpen) { this.closeAttachmentPreview(); return true; }
    if (this.draftsOpen) { this.draftsOpen = false; return true; }
    if (this.editorOpen) { this.closeEditor(); return true; }
    if (this.detailId) { this.closeDetail(); return true; }
    if (this.settingsOpen) { this.backSettings(); return true; }
    return false;
  }
  aboutToDisappear(): void {
    this.directInput.stop(); this.inputPolicy.stop(); this.todoDragPosition('', 0, false);
    this.fieldPolicy.stop(); this.editorInputEpoch++;
    this.clipboardPaste?.stop();
    this.attachmentSelection?.stop();
    this.closeAttachmentPreview();
    this.musicEpoch++;
    this.music?.dispose().catch((): void => {});
    this.pageAlive = false;
    const detach = this.editorDetachResolve; this.editorDetachResolve = undefined;
    if (detach) { detach(); } // Cancels the wait; pageAlive=false rejects cleanup.
    const mount = this.editorMountResolve; this.editorMountResolve = undefined;
    if (mount) { mount(false); }
    // No mutation is replayed on destruction. Only earlier acknowledged raw
    // journal generations are recoverable after a forced process termination.
    this.editorDraft?.dispose();
    this.businessRecovery?.dispose();
    this.markdown.dispose();
    this.inlineImages.dispose();
    this.queries.dispose();
    this.cardOrder.dispose();
    if (this.workspaceRestoreTimer !== -1) { clearTimeout(this.workspaceRestoreTimer); }
    this.mounted = false; native.releasePreview(this.preview);
  }
  private foregroundChanged(): void {
    this.musicEpoch++;
    if (!this.foreground) { this.music?.background().catch((): void => {}); }
    else { this.music?.foreground(); }
    if (!this.foreground) { this.directInput.stop(); this.inputPolicy.stop(); this.todoDragPosition('', 0, false); this.fieldPolicy.stop(); this.editorInputEpoch++; this.clipboardPaste?.stop(); this.mediaPlayback.pauseForBackground(); this.exitMediaFullscreen(); }
    else { this.fileOpen?.reconcile(); if (this.editorOpen) { this.bindInputRules(); this.refreshFieldCounts(); } }
  }
  private async musicAction(action: string, trackId: string = '', value: number = 0, spoolId: string = ''): Promise<void> {
    if (!this.pageAlive || !this.foreground || !this.music) { return; }
    await this.music.action(action, trackId, value, spoolId);
    if (this.pageAlive) { this.musicView = this.music.view(); }
  }
  private async musicReorder(trackIds: string[]): Promise<void> {
    if (!this.pageAlive || !this.foreground || !this.music) { return; }
    await this.music.reorder(trackIds);
    if (this.pageAlive) { this.musicView = this.music.view(); }
  }
  private async beforeMusicPlay(): Promise<void> {
    if (!this.pageAlive || !this.foreground || this.mediaToggleWorking ||
      (this.attachmentPreviewOpen && ['audio', 'video'].includes(this.attachmentPreviewKind))) {
      throw new Error('请先结束附件媒体预览，再播放音乐。');
    }
    await this.mediaPlayback.pauseForBackground();
    if (!this.pageAlive || !this.foreground || ['playing', 'failed', 'cleanup_failed', 'closing', 'loading', 'preparing'].includes(this.mediaPlayback.view().phase)) {
      throw new Error('附件媒体尚未确认暂停，音乐未启动。');
    }
  }
  private async toggleAttachmentMedia(token: string): Promise<void> {
    if (!this.pageAlive || !this.foreground || this.mediaToggleWorking || !this.attachmentPreviewOpen ||
      this.attachmentPreviewClosing || !token || token !== this.mediaView.token) { return; }
    const epoch = this.attachmentPreviewEpoch; this.mediaToggleWorking = true;
    try {
      await this.music?.pauseForMedia();
      if (!this.pageAlive || !this.foreground || epoch !== this.attachmentPreviewEpoch ||
        this.attachmentPreviewClosing || token !== this.mediaView.token) { return; }
      await this.mediaPlayback.toggle(token);
    } catch (error) { if (this.pageAlive && epoch === this.attachmentPreviewEpoch) { this.attachmentPreviewFailure = '媒体切换未完成：' + String(error); } }
    finally { this.mediaToggleWorking = false; }
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
      await this.loadBusinessIntents();
      await this.music?.load();
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
    if (this.draftRetiredIdentity === draft.scope.draft_id) {
      this.draftStatus = '旧草稿已清理，较新的输入仅在当前编辑器中，尚未写入新草稿。'; return;
    }
    if (this.draftCaptureIncomplete && this.retirementInput?.size > 0) {
      this.draftStatus = '清理期间有新的输入，仅保留在当前编辑器，尚未写入新草稿'; return;
    }
    const values = JSON.stringify(draft.current);
    if (values !== this.attachmentEditingValues) { this.attachmentEditingValues = values; this.attachmentEditEpoch++; }
    this.refreshEditorAssets();
    this.updateInlineImages();
    this.draftStatus = this.rawFork ? this.rawForkStatus || '完整输入正在接续保留' : draft.unknown ? '草稿回复未确认，原请求已保留' : draft.conflicted ? '草稿世代已变化，请核对' :
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
  private newCardTodos(): boolean {
    return this.editorDraft?.scope.source_kind === 1 || !!this.ownedTodosBaseline &&
      this.ownedTodosBaseline.qualified && this.ownedTodosBaseline.confirmed?.card_id === this.editorDraft?.scope.card_id;
  }
  private directOwner(key: string): string { return key === 'taskRename' ? this.attachmentEditorIdentity + ':' + this.taskEditId : this.attachmentEditorIdentity; }
  private directValue(key: string): DraftTextValue { return key === 'taskRename' ? this.taskRenameValue : this.draftField(key); }
  private directInputCurrent(owner: string, key: string, revision: number, value: DraftTextValue): boolean {
    return this.pageAlive && this.foreground && this.editorOpen && !!this.editorDraft && !this.editorDraft.disposed &&
      !this.editorViewRevoked && !this.draftRetiring && !this.draftRetirementUnknown && this.draftRetiredIdentity !== this.editorDraft.scope.draft_id &&
      !this.draftCaptureBlocked.has(key) && owner === this.directOwner(key) &&
      this.inputRevisions.get(key) === revision && (key !== 'taskRename' || !!this.taskEditId) && samePasteTarget(value, this.directValue(key));
  }
  private bindInputRules(): void {
    this.directInput.stop(); this.inputPolicy.stop(); this.inputRevisions.clear();
    for (const key of ['title', 'description', 'hypothesis', 'conclusion', 'todos']) {
      if (key === 'todos' && this.newCardTodos()) { this.directInput.unbind(key); continue; }
      this.inputRevisions.set(key, 0);
      this.directInput.bind(key, key, this.directOwner(key), 0, copyText(this.directValue(key)));
    }
    this.directInput.unbind('taskRename');
    if (this.taskEditId) { this.inputRevisions.set('taskRename', 0); this.directInput.bind('taskRename', 'todos', this.directOwner('taskRename'), 0, copyText(this.taskRenameValue)); }
    this.todoInputValue = copyText(this.editorValues.todos); this.todoInputRevision++; this.todoBusinessReady = !this.newCardTodos();
    this.directInputChanged();
  }
  private captureDirectInput(key: string, preview: boolean = false): void {
    if (key === 'todos' && this.newCardTodos()) {
      this.todoInputValue = copyText(this.editorValues.todos); this.todoInputRevision++; this.todoBusinessReady = false; return;
    }
    const revision = (this.inputRevisions.get(key) ?? 0) + 1; this.inputRevisions.set(key, revision);
    this.directInput.capture(key, this.directOwner(key), revision, copyText(this.directValue(key)), preview);
  }
  // A paste/import has already passed its own full-value preflight. Adopt its
  // final selection before binding; keyboard formatting must not reissue it.
  private adoptExternalInput(name: string, input: DraftTextValue): void {
    const value = copyText(input);
    if (name === 'title') { this.editorValues.title = value; this.title = value.text; }
    else if (name === 'description') { this.editorValues.description = value; this.description = value.text; }
    else if (name === 'hypothesis') { this.editorValues.hypothesis = value; this.hypothesis = value.text; }
    else if (name === 'conclusion') { this.editorValues.conclusion = value; this.conclusion = value.text; }
    else { this.editorValues.todos = value; this.taskText = value.text; }
    this.editorInputEpoch++; if (name !== 'todos') { this.dirty = true; }
    this.draftCaptureBlocked.delete(name); this.draftCaptureIncomplete = this.draftCaptureBlocked.size > 0;
    if (name === 'todos' && this.newCardTodos()) {
      this.todoInputValue = copyText(value); this.todoInputRevision++; this.todoBusinessReady = false;
    } else {
      const revision = (this.inputRevisions.get(name) ?? 0) + 1; this.inputRevisions.set(name, revision);
      this.directInput.bind(name, name, this.directOwner(name), revision, value);
    }
    this.editorDraft!.update(copyValues(this.editorValues)); this.refreshFieldCount(name);
  }
  private retirementCapture(name: string, value: string, preview?: PreviewText, start?: number, end?: number): boolean {
    if (!this.draftRetiring && !this.draftRetirementUnknown && this.draftRetiredIdentity !== this.editorDraft?.scope.draft_id) { return false; }
    if (!this.retirementInput) { this.retirementInput = new Map(); }
    this.retirementInput.set(name + (start === undefined ? ':text' : ':selection'), JSON.stringify({ value: value, preview: preview, start: start, end: end }));
    this.editorInputEpoch++; this.draftCaptureBlocked.add(name); this.draftCaptureIncomplete = true;
    const field = this.draftField(name), candidate = preview?.value ?? '', offset = preview?.offset ?? -1;
    if (start !== undefined && end !== undefined) {
      if (Number.isInteger(start) && Number.isInteger(end) && start >= -1 && end >= -1 && start <= field.text.length && end <= field.text.length) {
        field.selection_base = start; field.selection_extent = end;
      }
    } else if (name !== 'todos' || !this.newCardTodos()) {
      if (name === 'title') { this.title = value; }
      else if (name === 'description') { this.description = value; }
      else if (name === 'hypothesis') { this.hypothesis = value; }
      else if (name === 'conclusion') { this.conclusion = value; }
      else { this.taskText = value; }
      if (!candidate.length || (Number.isInteger(offset) && offset >= 0 && offset <= value.length)) {
        field.text = candidate.length ? value.slice(0, offset) + candidate + value.slice(offset) : value;
        field.composing_start = candidate.length ? offset : -1; field.composing_end = candidate.length ? offset + candidate.length : -1;
        if (field.selection_base > field.text.length) { field.selection_base = -1; }
        if (field.selection_extent > field.text.length) { field.selection_extent = -1; }
      }
    }
    // This callback cannot acquire a write on a possibly retired identity.
    // Keep the complete SDK event locally and revoke close/save qualification.
    this.draftStatus = '清理期间有新的输入，仅保留在当前编辑器，尚未写入新草稿';
    this.message = this.draftStatus; return true;
  }
  private directInputChanged(): void {
    this.inputRulesRevision++;
    const keys = ['title', 'description', 'hypothesis', 'conclusion', 'todos', 'taskRename'];
    this.inputRulesMessage = keys.map((key: string): string => { const state = this.directInput?.view(key); return state?.error || state?.notice || ''; })
      .filter((value: string): boolean => value.length > 0).join('\n');
  }
  private applyDirectInput(proposal: EditorDirectInputProposal, owned: () => boolean): number | undefined {
    if (!owned() || !this.directInputCurrent(proposal.owner, proposal.key, proposal.revision, proposal.expected) ||
      proposal.value.composing_start !== -1 || proposal.value.composing_end !== -1) { return undefined; }
    const value = copyText(proposal.value), key = proposal.key;
    if (key === 'taskRename') { this.taskRenameValue = value; this.taskRenameText = value.text; }
    else {
      if (key === 'title') { this.editorValues.title = value; this.title = value.text; }
      else if (key === 'description') { this.editorValues.description = value; this.description = value.text; }
      else if (key === 'hypothesis') { this.editorValues.hypothesis = value; this.hypothesis = value.text; }
      else if (key === 'conclusion') { this.editorValues.conclusion = value; this.conclusion = value.text; }
      else { this.editorValues.todos = value; this.taskText = value.text; }
      if (key !== 'todos') { this.dirty = true; }
      this.editorDraft!.update(copyValues(this.editorValues));
    }
    this.editorInputEpoch++; const revision = proposal.revision + 1; this.inputRevisions.set(key, revision);
    if (key !== 'taskRename') { this.draftSelectionPending.add(key); if (this.draftFocusedFields.has(key)) { this.restoreSelection(key); } }
    else {
      const editor = this.attachmentEditorIdentity, task = this.taskEditId;
      setTimeout((): void => {
        if (this.attachmentEditorIdentity === editor && this.taskEditId === task && this.directInputCurrent(proposal.owner, key, revision, value)) {
          const start = Math.min(value.selection_base, value.selection_extent), end = Math.max(value.selection_base, value.selection_extent);
          if (start >= 0 && end >= 0) { if (start === end) { this.taskRenameController.caretPosition(start); } else { this.taskRenameController.setTextSelection(start, end, { menuPolicy: MenuPolicy.HIDE }); } }
        }
      }, 16);
    }
    this.refreshFieldCount(key); return revision;
  }
  private inputReadyFor(action: string): boolean {
    if (this.draftRetiredIdentity && this.draftRetiredIdentity === this.editorDraft?.scope.draft_id) { return false; }
    if (action === 'create' || action === 'edit') {
      return ['title', 'description', 'hypothesis', 'conclusion'].every((key: string): boolean => this.directInput.canConfirm(key)) &&
        (!this.newCardTodos() || this.todoBusinessReady);
    }
    return action === 'task_add' ? this.directInput.canConfirm('todos') : action === 'task_rename' ? this.directInput.canConfirm('taskRename') : true;
  }
  private retryInputRules(): void {
    for (const key of ['title', 'description', 'hypothesis', 'conclusion', 'todos', 'taskRename']) { this.directInput.retry(key); }
  }
  private captureTodoRows(value: DraftTextValue, owner: string): number | undefined {
    if (owner === this.attachmentEditorIdentity && this.pageAlive && this.editorOpen && this.editorDraft &&
        this.retirementCapture('todos', JSON.stringify(value))) { return undefined; }
    if (!this.pageAlive || !this.editorOpen || !this.editorDraft || this.editorDraft.disposed || !this.newCardTodos() ||
      owner !== this.attachmentEditorIdentity || this.attachmentWorking || this.pasteWorking || this.draftRestoreInput || this.draftRetiring || this.draftRetirementUnknown) { return undefined; }
    this.editorValues.todos = copyText(value); this.taskText = value.text; this.dirty = true; this.editorInputEpoch++;
    this.retirementInput?.delete('todos:text'); this.retirementInput?.delete('todos:selection');
    this.todoInputValue = copyText(value); this.todoInputRevision++; this.todoBusinessReady = false;
    this.draftCaptureBlocked.delete('todos'); this.draftCaptureIncomplete = this.draftCaptureBlocked.size > 0;
    this.editorDraft.update(copyValues(this.editorValues)); this.refreshFieldCount('todos'); return this.todoInputRevision;
  }
  private todoRowsCurrent(owner: string, revision: number, value: DraftTextValue): boolean {
    return this.pageAlive && this.foreground && this.editorOpen && !!this.editorDraft && !this.editorDraft.disposed && this.newCardTodos() &&
      owner === this.attachmentEditorIdentity && revision === this.todoInputRevision && samePasteTarget(value, this.editorValues.todos);
  }
  private todoRowsStatus(status: EditorTodosStatus): void {
    if (this.draftRetiring || this.draftRetirementUnknown || this.draftRetiredIdentity === this.editorDraft?.scope.draft_id) { return; }
    if (this.draftCaptureBlocked.has('todos') && this.retirementInput?.has('todos:text')) { return; }
    if (!this.todoRowsCurrent(status.owner, status.revision, status.value)) { return; }
    this.todoBusinessReady = status.business_ready; this.todoFormatPending = status.format_pending;
    if (status.raw_capture_complete) { this.draftCaptureBlocked.delete('todos'); } else { this.draftCaptureBlocked.add('todos'); }
    this.draftCaptureIncomplete = this.draftCaptureBlocked.size > 0;
  }
  private todoDragPosition(owner: string, windowY: number, active: boolean): void {
    if (!active || owner !== this.attachmentEditorIdentity || !this.editorOpen || !this.foreground) {
      if (this.todoDragTimer !== -1) { clearTimeout(this.todoDragTimer); this.todoDragTimer = -1; } return;
    }
    this.todoDragY = windowY;
    if (this.todoDragTimer !== -1) { return; }
    const editor = owner;
    const tick = (): void => {
      this.todoDragTimer = -1;
      if (!this.pageAlive || !this.foreground || !this.editorOpen || editor !== this.attachmentEditorIdentity) { return; }
      const bounds = this.editorScrollBounds();
      const delta = bounds.length !== 2 ? 0 : this.todoDragY < bounds[0] + 48 ? -8 : this.todoDragY > bounds[1] - 48 ? 8 : 0;
      if (delta) { this.editorScroller.scrollBy(0, delta); }
      this.todoDragTimer = setTimeout(tick, 16);
    };
    this.todoDragTimer = setTimeout(tick, 16);
  }
  private editorScrollBounds(): number[] {
    try {
      const context = this.getUIContext(), bounds = context.getComponentUtils().getRectangleById('editor-scroll');
      const top = context.px2vp(bounds.windowOffset.y), bottom = top + context.px2vp(bounds.size.height);
      return Number.isFinite(top) && Number.isFinite(bottom) && bottom > top ? [top, bottom] : [];
    } catch (_) { return []; }
  }
  private revealTodoRow(owner: string, element: string): void {
    const revision = this.todoInputRevision;
    setTimeout((): void => {
      if (!this.ownsEditorView(owner) || !this.newCardTodos() || revision !== this.todoInputRevision || !this.foreground) { return; }
      try {
        const context = this.getUIContext(), row = context.getComponentUtils().getRectangleById(element), bounds = this.editorScrollBounds();
        const top = context.px2vp(row.windowOffset.y), bottom = top + context.px2vp(row.size.height);
        if (bounds.length === 2 && Number.isFinite(top) && Number.isFinite(bottom)) {
          this.editorScroller.scrollBy(0, top < bounds[0] ? top - bounds[0] : bottom > bounds[1] ? bottom - bounds[1] : 0);
        }
      } catch (_) {} // A changed/unmounted row is never replaced by a guessed target.
    }, 16);
  }
  private fieldCountIndex(name: string): number {
    return name === 'title' ? 0 : name === 'description' ? 1 : name === 'hypothesis' ? 2 : name === 'conclusion' ? 3 : name === 'todos' ? 4 : 5;
  }
  private setFieldCount(name: string, label: string): void {
    const labels = this.fieldCountLabels.slice(); labels[this.fieldCountIndex(name)] = label; this.fieldCountLabels = labels;
  }
  private fieldCountLabel(name: string): string { return this.fieldCountLabels[this.fieldCountIndex(name)] || '计数中…'; }
  private refreshFieldCounts(): void {
    for (const name of ['title', 'description', 'hypothesis', 'conclusion', 'todos']) { this.refreshFieldCount(name); }
    if (this.taskEditId) { this.refreshFieldCount('taskRename'); }
  }
  private async refreshFieldCount(name: string): Promise<void> {
    const draft = this.editorDraft, editor = this.attachmentEditorIdentity, task = this.taskEditId;
    if (!this.pageAlive || !this.foreground || !this.editorOpen || !draft || draft.disposed) { return; }
    const rename = name === 'taskRename', field = rename ? 'todos' : name;
    const snapshot = copyText(rename ? this.taskRenameValue : this.draftField(name));
    const epoch = (this.fieldCountEpochs.get(name) ?? 0) + 1; this.fieldCountEpochs.set(name, epoch);
    const owned = (): boolean => this.pageAlive && this.foreground && this.editorOpen && this.editorDraft === draft && !draft.disposed &&
      this.attachmentEditorIdentity === editor && this.fieldCountEpochs.get(name) === epoch &&
      (!rename || this.taskEditId === task) && samePasteTarget(snapshot, rename ? this.taskRenameValue : this.draftField(name));
    this.setFieldCount(name, '计数中…');
    try {
      const result = await this.fieldPolicy.checkField(field, snapshot.text, owned);
      if (!owned()) { return; }
      this.setFieldCount(name, result.grapheme_count < 0 ? '计数未确认，完整输入已保留' :
        result.grapheme_count.toString() + ' / ' + editorFieldLimit(field).toString() + (result.ok ? '' : ' · 超出字数，完整输入已保留'));
    } catch (_) { if (owned()) { this.setFieldCount(name, '计数未确认，完整输入已保留'); } }
  }
  private taskRenameChanged(value: string, preview?: PreviewText): void {
    this.editorInputEpoch++; this.taskRenameText = value;
    const candidate = preview?.value ?? '', offset = preview?.offset ?? -1;
    const valid = candidate.length === 0 || (Number.isInteger(offset) && offset >= 0 && offset <= value.length);
    this.taskRenameValue.text = valid && candidate ? value.slice(0, offset) + candidate + value.slice(offset) : value;
    this.taskRenameValue.composing_start = candidate.length > 0 ? Math.max(0, offset) : -1;
    this.taskRenameValue.composing_end = candidate.length > 0 ? Math.max(0, offset) + candidate.length : -1;
    this.captureDirectInput('taskRename', candidate.length > 0);
    this.refreshFieldCount('taskRename');
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
    if (this.businessWorking || this.businessPending()) { return false; }
    return this.ready && this.editorOpen && !!this.editorDraft && !this.editorDraft.disposed &&
      !this.attachmentWorking && !this.pasteWorking && !this.fieldValidationWorking && !this.draftRestoreInput && !this.busy && !this.draftWorking && !this.draftRetiring && !this.draftRetirementUnknown &&
      !this.rawFork && !this.rawForkWorking && !this.editorBoundary && !this.editorViewRevoked &&
      !this.draftCaptureIncomplete && !this.editorDraft.unknown && this.pending.length === 0 && !this.current()?.deleted &&
      (this.lastDraftField === 'todos' && this.newCardTodos() ? this.todoBusinessReady : this.directInput.canConfirm(this.lastDraftField));
  }
  private async pasteContent(): Promise<void> {
    if (!this.canPaste() || !this.attachmentFiles) { return; }
    const draft = this.editorDraft!, files = this.attachmentFiles, name = this.lastDraftField;
    const snapshot = copyText(this.draftField(name)), originalValues = draft.current, editor = this.attachmentEditorIdentity, inputEpoch = this.editorInputEpoch;
    if (snapshot.composing_start !== -1 || snapshot.composing_end !== -1) {
      this.message = '请先完成输入法候选，再粘贴内容。'; return;
    }
    const spools: Map<string, PreparedAttachment> = new Map<string, PreparedAttachment>();
    let input: ClipboardSnapshot | undefined = undefined;
    // Admission permits this paste's confirmed pins, but no newer editing
    // value, source editor, composition or selection may be borrowed.
    const owner = (): boolean => {
      if (!this.pageAlive || !this.foreground || !this.editorOpen || this.editorDraft !== draft || draft.disposed || this.draftRetiring ||
        this.draftRetirementUnknown || this.draftCaptureIncomplete || draft.unknown || draft.conflicted || this.draftConflict ||
        this.attachmentEditorIdentity !== editor || this.editorInputEpoch !== inputEpoch || !samePasteTarget(snapshot, this.draftField(name))) { return false; }
      const current = draft.current; current.assets = originalValues.assets;
      return sameValues(current, originalValues);
    };
    this.pasteWorking = true;
    try {
      input = await this.clipboardInput.readAuthorized(owner);
      if (!owner() || !await this.loadImports()) { throw new Error('粘贴目标或附件记录已变化，未开始导入。'); }
      const plan: ClipboardPastePlan = await this.prepareClipboardPlan(input, files, name, owner, spools);
      if (!owner() || !this.clipboardInput.isCurrent(input)) { throw new Error('剪贴板或插入位置已变化，未开始导入。'); }
      if (!plan.text && plan.attachments.length === 0) { throw new Error(plan.warnings.join('\n') || '剪贴板中没有可读取的内容。'); }
      await this.clipboardInput.dispose(input); input = undefined;
      const coordinator = new ClipboardPaste({
        read: (): ClipboardPasteBoundary => this.clipboardBoundary(draft, editor, name, owner),
        checkText: async (value: DraftTextValue, _boundary: ClipboardPasteBoundary): Promise<void> => {
          await this.fieldPolicy.requireTextValue(name, value, owner);
        },
        confirm: async (_boundary: ClipboardPasteBoundary): Promise<ClipboardPasteBoundary> => {
          await draft.ensureConfirmed(); return this.clipboardBoundary(draft, editor, name, owner);
        },
        importOne: (item: ClipboardPasteAttachment, identity: ClipboardPasteIdentity,
          boundary: ClipboardPasteBoundary): Promise<ClipboardPasteReceipt> =>
          this.importClipboardAttachment(draft, files, editor, name, owner, spools, item, identity, boundary),
        commitText: async (value: DraftTextValue, _boundary: ClipboardPasteBoundary): Promise<ClipboardPasteBoundary> => {
          if (!owner()) { throw new Error('粘贴目标已变化，正文未插入。'); }
          this.adoptExternalInput(name, value);
          this.draftSelectionPending.add(name); await draft.flush();
          const after = this.clipboardBoundary(draft, editor, name, (): boolean => true);
          const others = draft.current; others.assets = originalValues.assets;
          if (name === 'title') { others.title = originalValues.title; }
          else if (name === 'description') { others.description = originalValues.description; }
          else if (name === 'hypothesis') { others.hypothesis = originalValues.hypothesis; }
          else if (name === 'conclusion') { others.conclusion = originalValues.conclusion; }
          else { others.todos = originalValues.todos; }
          after.blocked = !this.pageAlive || !this.foreground || !this.editorOpen || this.editorDraft !== draft || draft.disposed ||
            this.attachmentEditorIdentity !== editor || !sameValues(others, originalValues) || draft.dirty || draft.unknown ||
            draft.conflicted || !draft.confirmed || !sameValues(draft.current, draft.confirmed.values);
          return after;
        },
        operation: (): string => util.generateRandomUUID(true)
      });
      this.clipboardPaste = coordinator;
      const result = await coordinator.run(plan);
      if (this.pageAlive && this.editorDraft === draft) {
        if (result.phase === 'complete' && !draft.current.title.text.trim() && plan.attachments.length > 0) {
          const title = attachmentImportTitle(draft.current.title.text, plan.attachments[0].name);
          if (title !== undefined && !draft.dirty && !draft.unknown && draft.confirmed) {
            const values = draft.current; values.title.text = title; values.title.selection_base = title.length; values.title.selection_extent = title.length;
            this.adoptExternalInput('title', values.title); await draft.flush();
          }
        }
        if (name === 'description' && result.text_inserted) { this.previewBody = true; this.updateMarkdown(); }
        if (result.text_inserted && (name !== 'description' || Math.min(820, this.viewportWidth - 40) - 80 >= 590)) { this.restoreSelection(name); }
        const count = result.selected.toString() + '/' + result.total.toString();
        this.message = result.phase === 'complete' ?
          (result.warnings.length > 0 ? '内容已保留 · 附件 ' + count + '\n' + result.warnings.join('\n') :
            '内容已保留 · ' + this.pasteTargetName() + (result.total > 0 ? ' · 附件 ' + count : '')) :
          '粘贴已停止 · 已确认附件 ' + count + '\n' + result.error;
      }
    } catch (error) {
      if (this.pageAlive && this.editorDraft === draft) { this.message = String(error); }
    } finally {
      if (input) { try { await this.clipboardInput.dispose(input); } catch (_) {} }
      for (const spool of spools.values()) {
        // A possibly issued import keeps its immutable request and spool for
        // the existing recovery UI. Only unissued preparation is abandoned.
        if (!spool.original_request) { try { await files.release(spool); } catch (_) {} }
      }
      this.clipboardPaste = undefined; this.pasteWorking = false; this.draftChanged(); await this.loadImports();
    }
  }

  private clipboardBoundary(draft: EditorDraftCoordinator, editor: string, name: string,
    owner: () => boolean): ClipboardPasteBoundary {
    return { editor_id: editor, card_id: draft.scope.card_id, draft_id: draft.scope.draft_id,
      generation: draft.confirmed?.generation ?? '0', selected_count: draft.current.assets.length,
      import_count: this.importRecords.length, target: copyText(this.draftField(name)),
      blocked: !owner() || this.busy || this.draftWorking || this.draftRetiring || this.draftRetirementUnknown ||
        !!this.attachmentPending || this.pending.length > 0 };
  }

  private async importClipboardAttachment(draft: EditorDraftCoordinator, files: AttachmentFiles, editor: string,
    name: string, owner: () => boolean, spools: Map<string, PreparedAttachment>, item: ClipboardPasteAttachment,
    identity: ClipboardPasteIdentity, boundary: ClipboardPasteBoundary): Promise<ClipboardPasteReceipt> {
    const spool = spools.get(item.key);
    if (!spool || spool.name !== item.name || spool.byte_length !== item.byte_length || spool.sha256 !== item.sha256 || !owner()) {
      return { outcome: 'cancelled', identity, asset_id: '', next: undefined };
    }
    const command = new ImportCommand(); const request = command.import_request;
    request.card_id = identity.card_id; request.draft_id = identity.draft_id; request.expected_generation = identity.expected_generation;
    request.operation_id = identity.operation_id; request.name = item.name; request.kind = attachmentKind(item.name);
    request.byte_length = item.byte_length; request.sha256 = item.sha256;
    try {
      const current = this.clipboardBoundary(draft, editor, name, owner);
      if (current.blocked || current.generation !== boundary.generation || current.selected_count !== boundary.selected_count ||
        current.import_count !== boundary.import_count || !samePasteTarget(current.target, boundary.target)) {
        return { outcome: 'cancelled', identity, asset_id: '', next: undefined };
      }
      const outcome = await this.acceptPreparedImport(draft, spool, JSON.stringify(command), false);
      const record = this.importRecords.find((entry: ImportRecord): boolean => entry.request.operation_id === identity.operation_id);
      const assetId = outcome === 'selected' && record ? selectedImport(record, request).asset_id : '';
      return { outcome, identity, asset_id: assetId,
        next: outcome === 'selected' ? this.clipboardBoundary(draft, editor, name, owner) : undefined };
    } catch (_) { return { outcome: spool.original_request ? 'unknown' : 'failed', identity, asset_id: '', next: undefined }; }
  }

  private async prepareClipboardPlan(snapshot: ClipboardSnapshot, files: AttachmentFiles, name: string,
    owner: () => boolean, spools: Map<string, PreparedAttachment>): Promise<ClipboardPastePlan> {
    const plan: ClipboardPastePlan = { text: '', attachments: [], warnings: snapshot.warnings.slice() };
    const prepared: Map<ClipboardSource, PreparedAttachment> = new Map<ClipboardSource, PreparedAttachment>();
    const parts: string[] = [];
    const check = (): void => { if (!owner() || !this.clipboardInput.isCurrent(snapshot)) { throw new Error('剪贴板或粘贴位置已变化，尚未开始加入。'); } };
    const prepare = async (source: ClipboardSource): Promise<PreparedAttachment> => {
      check(); const prior = prepared.get(source); if (prior) { return prior; }
      const spool = await files.prepareClipboard(source); spools.set(spool.spool_id, spool); prepared.set(source, spool); check(); return spool;
    };
    const append = (spool: PreparedAttachment, references: string[] = []): ClipboardPasteAttachment => {
      let item = plan.attachments.find((entry: ClipboardPasteAttachment): boolean => entry.key === spool.spool_id);
      if (!item) {
        item = { key: spool.spool_id, name: spool.name, byte_length: spool.byte_length, sha256: spool.sha256,
          references: [], image: ['image', 'gif'].includes(attachmentKind(spool.name)) }; plan.attachments.push(item);
      }
      for (const reference of references) { if (!item.references.includes(reference)) { item.references.push(reference); } }
      return item;
    };
    const appendImage = async (spool: PreparedAttachment, reference: string): Promise<void> => {
      const prior = plan.attachments.find((item: ClipboardPasteAttachment): boolean =>
        item.image && item.sha256 === spool.sha256 && item.byte_length === spool.byte_length);
      if (prior && prior.key !== spool.spool_id) {
        if (!prior.references.includes(reference)) { prior.references.push(reference); }
        await files.release(spool); spools.delete(spool.spool_id);
      } else { append(spool, [reference]); }
    };
    for (const record of snapshot.records) {
      check();
      if (record.file) {
        const spool = await prepare(record.file);
        if (['image', 'gif'].includes(attachmentKind(spool.name))) { await appendImage(spool, 'clipboard-image-' + spool.sha256); }
        else { append(spool); }
        continue;
      }
      const primary = record.xml ?? record.html ?? (record.plain_text ? undefined : record.rtf);
      let converted: string | undefined = undefined;
      for (const source of [record.html, record.rtf, record.xml]) {
        if (!source) { continue; }
        const spool = await prepare(source); append(spool);
        try {
          const conversion: ClipboardConversion = await files.convertPreparedClipboard(spool, source.format, name); check();
          plan.warnings.push(...conversion.warnings);
          for (const image of conversion.images) {
            const duplicate = plan.attachments.find((item: ClipboardPasteAttachment): boolean =>
              item.references.includes(image.name) && item.image && item.byte_length === image.byte_length && item.sha256 === image.sha256);
            if (duplicate) { continue; }
            const embedded = await files.prepareEmbeddedClipboardImage(spool, conversion, image.local_id);
            spools.set(embedded.spool_id, embedded); check(); await appendImage(embedded, image.name);
          }
          if (source === primary) { converted = conversion.paste_text; }
        } catch (error) {
          if (source === primary) { converted = undefined; }
          check(); plan.warnings.push('已保留 ' + spool.name + ' 原件，转换未完成：' + String(error));
        }
      }
      let plain = record.plain_text;
      if (record.plain && (name === 'description' && converted === undefined || !plain)) {
        const spool = await prepare(record.plain);
        const conversion = await files.convertPreparedClipboard(spool, 'plain', name); check();
        plain = conversion.paste_text;
        if (plain !== record.plain_text) { append(spool); plan.warnings.push('表格已转换，原始 TSV 已保留。'); }
        else { await files.release(spool); spools.delete(spool.spool_id); }
      }
      const text = name === 'description' ? converted || plain || record.uri_text : record.plain_text || plain || converted || record.uri_text;
      if (text) { parts.push(text); }
      for (const source of record.binary) { append(await prepare(source)); }
      if (record.image) {
        const spool = await prepare(record.image);
        await appendImage(spool, 'clipboard-image-' + spool.sha256);
      }
    }
    plan.text = parts.join('\n\n');
    if (name === 'description' && !plan.text) {
      plan.text = plan.attachments.filter((item: ClipboardPasteAttachment): boolean => item.image)
        .map((item: ClipboardPasteAttachment): string => {
          if (item.references.length === 0) { item.references.push('clipboard-image-' + item.sha256); }
          return '![图片](attachment:' + encodeURIComponent(item.references[0]) + ')';
        }).join('\n\n');
    }
    check(); return plan;
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
    if (this.pageAlive && this.editorOpen && this.editorDraft && this.retirementCapture(name, value, preview)) { return; }
    if (!this.pageAlive || !this.editorOpen || !this.editorDraft || this.editorDraft.disposed || this.draftRestoreInput || this.attachmentWorking || this.draftRetiring || this.draftRetirementUnknown) { return; }
    this.editorInputEpoch++;
    const field = this.draftField(name);
    const offset = preview?.offset ?? -1, candidate = preview?.value ?? '';
    if (candidate.length > 0 && (!Number.isInteger(offset) || offset < 0 || offset > value.length)) {
      this.draftCaptureBlocked.add(name); this.draftCaptureIncomplete = true;
      this.fieldCountEpochs.set(name, (this.fieldCountEpochs.get(name) ?? 0) + 1);
      this.setFieldCount(name, '计数未确认，完整输入已保留');
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
    this.captureDirectInput(name, candidate.length > 0);
    this.refreshFieldCount(name);
  }
  private draftSelectionChanged(name: string, start: number, end: number): void {
    if (this.pageAlive && this.editorOpen && this.editorDraft && this.draftFocusedFields.has(name) &&
        this.retirementCapture(name, this.draftField(name).text, undefined, start, end)) { return; }
    if (!this.pageAlive || !this.editorOpen || this.editorDraft?.disposed || !this.draftFocusedFields.has(name) || this.draftRestoreInput || this.attachmentWorking || this.draftRetiring || this.draftRetirementUnknown || !this.editorDraft) { return; }
    const field = this.draftField(name);
    if (this.draftCaptureBlocked.has(name) || start < -1 || end < -1 || start > field.text.length || end > field.text.length) { return; }
    // An actual focused selection supersedes a scheduled restoration. Native
    // construction events are already excluded by the focused-field guard.
    this.draftSelectionPending.delete(name); field.selection_base = start; field.selection_extent = end;
    this.editorInputEpoch++;
    this.editorDraft.update(copyValues(this.editorValues));
    this.captureDirectInput(name, field.composing_start !== -1 || field.composing_end !== -1);
    this.refreshFieldCount(name);
  }
  private restoreSelection(name: string): void {
    if (!this.draftSelectionPending.has(name)) { return; }
    const value = copyText(this.draftField(name)), start = value.selection_base, end = value.selection_extent;
    const draft = this.editorDraft, owner = this.editorViewOwner;
    if (start < 0 || end < 0) { this.draftSelectionPending.delete(name); return; }
    // Do not start a new IME composition or steal focus. The caller already
    // focused this field; native controllers cannot restore the old IME session.
    setTimeout(() => {
      if (!this.ownsEditorView(owner) || !draft || draft.disposed || this.draftRetiring || this.draftRetirementUnknown || this.editorDraft !== draft) { return; }
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
  private ownsEditorView(owner: string): boolean {
    return !!owner && owner === this.editorViewOwner && owner === this.attachmentEditorIdentity &&
      !this.editorViewRevoked && this.pageAlive && this.editorOpen;
  }
  private editorLeaseMounted(owner: string): void {
    if (owner !== this.editorViewOwner || owner !== this.attachmentEditorIdentity || !this.editorViewVisible || !this.editorOpen) { return; }
    this.editorViewRevoked = false;
    const resolve = this.editorMountResolve; this.editorMountResolve = undefined;
    if (resolve) { resolve(true); }
  }
  private editorLeaseRevoked(owner: string): void {
    if (owner !== this.editorViewOwner || this.editorViewRevoked) { return; }
    // Revoke before resolving the waiter. No cleanup transport is admitted
    // while a view with this lease can submit another accepted SDK capture.
    this.editorViewRevoked = true;
    this.directInput.stop(); this.inputPolicy.stop(); this.fieldPolicy.stop();
    this.todoDragPosition('', 0, false);
    const resolve = this.editorDetachResolve; this.editorDetachResolve = undefined;
    if (resolve) { resolve(); }
  }
  private leaseTextChanged(owner: string, name: string, value: string, preview?: PreviewText): void {
    if (!this.ownsEditorView(owner)) { return; }
    if (preview?.value.length && (!Number.isInteger(preview.offset) || preview.offset < 0 || preview.offset > value.length)) {
      this.retirementInput.set(name + ':text', JSON.stringify({ value: value, preview: preview }));
    }
    this.editorInputChanged(name, value, preview);
  }
  private leaseSelectionChanged(owner: string, name: string, start: number, end: number): void {
    // A control's unfocused construction range is presentation state. Admit
    // the original lease and a focused field before retaining an invalid event.
    if (!this.ownsEditorView(owner) || !this.draftFocusedFields.has(name)) { return; }
    if (!Number.isInteger(start) || !Number.isInteger(end) || start < -1 || end < -1 ||
      start > this.draftField(name).text.length || end > this.draftField(name).text.length) {
      this.leaseUncaptured(owner, name, JSON.stringify({ start: start, end: end })); return;
    }
    this.draftSelectionChanged(name, start, end);
  }
  private leaseFocusChanged(owner: string, name: string, focused: boolean): void {
    if (!this.ownsEditorView(owner)) { return; }
    if (focused) { this.editorFocusIntent++; this.focusDraftField(name); } else { this.blurDraftField(name); }
  }
  private leaseTaskRenameChanged(owner: string, task: string, incarnation: string, value: string, preview?: PreviewText): void {
    if (!this.ownsEditorView(owner) || !incarnation || incarnation !== this.taskRenameOwner || task !== this.taskEditId) { return; }
    this.taskRenameChanged(value, preview);
  }
  private leaseTaskRenameFocus(owner: string, task: string, incarnation: string): void {
    if (this.ownsEditorView(owner) && incarnation && incarnation === this.taskRenameOwner && task === this.taskEditId) { this.editorFocusIntent++; }
  }
  private leaseUncaptured(owner: string, name: string, event: string): void {
    if (!this.ownsEditorView(owner)) { return; }
    this.retirementInput.set(name + ':text', event); this.editorInputEpoch++;
    this.draftCaptureBlocked.add(name); this.draftCaptureIncomplete = true;
    this.message = '当前事件尚未完整捕获，原事件已保留；草稿清理已停止。';
  }
  private async revokeEditorView(draft: EditorDraftCoordinator): Promise<boolean> {
    const owner = this.editorViewOwner;
    if (!this.pageAlive || !this.editorOpen || this.editorDraft !== draft || !this.ownsEditorView(owner) || this.editorDetachResolve) { return false; }
    this.editorBoundaryDraft = draft;
    const detached = new Promise<void>((resolve: () => void): void => { this.editorDetachResolve = resolve; });
    this.editorViewVisible = false;
    await detached;
    return this.pageAlive && this.editorOpen && this.editorDraft === draft && this.editorViewOwner === owner &&
      this.editorViewRevoked && !this.editorViewVisible;
  }
  private async remountEditorView(draft: EditorDraftCoordinator): Promise<boolean> {
    if (!this.pageAlive || !this.editorOpen || this.editorDraft !== draft || draft.disposed || !this.editorViewRevoked || this.editorMountResolve) { return false; }
    // Preserve full TextValues and the original scope; no attachDraft or
    // string-only reconstruction, and no synthetic end of IME composition.
    this.title = this.editorValues.title.text; this.description = this.editorValues.description.text;
    this.hypothesis = this.editorValues.hypothesis.text; this.conclusion = this.editorValues.conclusion.text;
    this.taskText = this.editorValues.todos.text;
    this.attachmentEditorIdentity = util.generateRandomUUID(true); this.editorViewOwner = this.attachmentEditorIdentity;
    const owner = this.editorViewOwner;
    const mounted = new Promise<boolean>((resolve: (mounted: boolean) => void): void => { this.editorMountResolve = resolve; });
    this.editorViewRevoked = true; this.editorViewVisible = true; this.editorInputEpoch++;
    this.draftFocusedFields.clear(); this.draftSelectionPending = new Set(['title', 'description', 'hypothesis', 'conclusion', 'todos']);
    this.bindInputRules(); this.refreshFieldCounts();
    return await mounted && this.pageAlive && this.editorDraft === draft && this.ownsEditorView(owner);
  }
  private rawForkCurrent(fork: EditorDraftForkCoordinator, parent: EditorDraftCoordinator, owner: string): boolean {
    return this.pageAlive && this.editorOpen && !this.editorViewRevoked && this.attachmentEditorIdentity === owner &&
      this.editorViewOwner === owner && this.rawFork === fork && this.rawForkParent === parent && !parent.disposed &&
      (this.editorDraft === parent || this.editorDraft?.scope.draft_id === fork.scope.draft_id);
  }
  private async sendRawDraft(serialized: string): Promise<DraftRecord> {
    const result = await workbench.send(serialized);
    if (!result.ok) { throw new DraftSaveFailure(result.effect === 'not_committed', result.error); }
    if (result.drafts.length !== 1) { throw new Error('草稿回执缺失'); }
    return result.drafts[0];
  }
  private async beginRawFork(): Promise<void> {
    if (this.businessWorking || this.businessPending()) { return; }
    if (this.rawForkWorking || this.rawFork || this.pending.length > 0 || this.attachmentWorking || this.pasteWorking || this.editorBoundary) { return; }
    const parent = this.editorDraft, owner = this.attachmentEditorIdentity;
    if (!parent || parent.disposed || !this.ownsEditorView(owner) || this.draftCaptureIncomplete) { this.message = '完整输入尚未确认，接续未开始。'; return; }
    const selected = new Set(parent.current.assets.map((asset: AssetSelection): string => asset.asset_id));
    if (this.attachmentPending || this.pendingSpools.length > 0 || this.importRecords.some((record: ImportRecord): boolean =>
      record.phase === 'pending' || record.phase === 'unknown' || record.phase === 'ready' && !selected.has(record.asset_id))) {
      this.rawForkStatus = '尚未加入草稿的附件导入仍归原草稿，请先核对或处理，再接续。'; this.message = this.rawForkStatus; return;
    }
    this.rawForkWorking = true; this.rawForkStatus = '正在接续保留完整输入…';
    try {
      // Settle an already issued save before freezing its exact parent proof.
      // Unknown is never reconciled by this call.
      await parent.ensureConfirmed();
      if (!this.ownsEditorView(owner) || this.editorDraft !== parent || this.draftCaptureIncomplete || parent.saving || parent.unknown || parent.conflicted) {
        throw new Error('原草稿或完整输入已变化，接续未开始');
      }
      const scope = copyScope(parent.scope); scope.draft_id = util.generateRandomUUID(true);
      this.rawForkParent = parent; this.rawForkOwner = owner;
      const fork = new EditorDraftForkCoordinator(parent, scope, parent.current, util.generateRandomUUID(true), {
        send: (serialized: string): Promise<DraftRecord> => this.sendRawDraft(serialized),
        changed: (): void => this.draftChanged(), operation: (): string => util.generateRandomUUID(true),
        isCurrent: (): boolean => this.pageAlive && this.editorOpen && !this.editorViewRevoked &&
          this.attachmentEditorIdentity === owner && this.editorViewOwner === owner && !parent.disposed &&
          (this.editorDraft === parent || this.editorDraft?.scope.draft_id === scope.draft_id)
      });
      this.rawFork = fork;
      await fork.begin(); await this.finishRawFork(fork, parent, owner);
    } catch (error) { this.rawForkFailed(String(error)); }
    finally { this.rawForkWorking = false; this.draftChanged(); }
  }
  private rawForkFailed(error: string): void {
    const fork = this.rawFork;
    this.rawForkAction = this.rawForkRetirement ? 'retirement' : fork?.unknown ? 'first' :
      this.editorDraft !== this.rawForkParent && this.editorDraft?.unknown ? 'child' : fork?.conflicted ? 'conflict' : 'continue';
    this.rawForkStatus = '接续未完成，原输入与操作仍保留：' + error; this.message = this.rawForkStatus;
  }
  private async finishRawFork(fork: EditorDraftForkCoordinator, parent: EditorDraftCoordinator, owner: string, reconcile: boolean = false): Promise<void> {
    if (!this.rawForkCurrent(fork, parent, owner) || this.draftCaptureIncomplete) { throw new Error('接续的界面归属或完整输入无法确认'); }
    let child = this.editorDraft;
    if (child === parent) {
      child = fork.openChild();
      this.rawForkRetirement = fork.retirementCommand(util.generateRandomUUID(true));
      child.update(copyValues(parent.current));
      // Switch in the same synchronous turn before the first child flush.
      // Every subsequent accepted callback writes only this live child.
      this.editorDraft = child; this.editorValues = copyValues(child.current);
      this.attachmentEditingValues = ''; this.draftChanged();
    }
    if (!child || child.scope.draft_id !== fork.scope.draft_id || child.unknown || child.conflicted) { throw new Error('子草稿需显式核对原请求'); }
    const confirmed = await child.flush();
    const epoch = this.editorInputEpoch, latest = child.current;
    if (!confirmed || !this.rawForkCurrent(fork, parent, owner) || this.editorDraft !== child || this.draftCaptureIncomplete ||
      child.dirty || child.unknown || !sameValues(latest, confirmed.values) || !sameValues(latest, this.editorValues)) {
      throw new Error('较新的完整输入尚未由子草稿确认');
    }
    if (this.editorInputEpoch !== epoch || !sameValues(latest, child.current)) { throw new Error('接续输入已变化'); }
    await this.retireRawForkParent(fork, parent, child, owner, reconcile);
  }
  private async retireRawForkParent(fork: EditorDraftForkCoordinator, parent: EditorDraftCoordinator,
    child: EditorDraftCoordinator, owner: string, reconcile: boolean): Promise<void> {
    if (!this.rawForkCurrent(fork, parent, owner) || this.editorDraft !== child || !this.rawForkRetirement ||
      this.rawForkRetirementUnknown && !reconcile) { throw new Error('父草稿清理需核对原操作'); }
    const original = this.rawForkRetirement, command = JSON.parse(original) as RawForkRetirementCommand;
    const proof = fork.parentProof, parentRecord = parent.confirmed!;
    let result: Reply;
    try { result = await workbench.send(original); }
    catch (_) { this.rawForkRetirementUnknown = true; throw new Error('父草稿清理回复未确认'); }
    if (!result.ok) {
      if (result.effect !== 'not_committed' || this.rawForkRetirementUnknown) { this.rawForkRetirementUnknown = true; }
      throw new Error(result.error);
    }
    try {
      if (result.drafts.length !== 1) { throw new Error('父草稿清理回执缺失'); }
      const record = result.drafts[0]; validateRecord(record, true);
      if (record.active || record.scope.card_id !== parentRecord.scope.card_id || record.scope.draft_id !== parentRecord.scope.draft_id ||
        record.scope.source_kind !== parentRecord.scope.source_kind || record.scope.source_revision !== parentRecord.scope.source_revision ||
        record.scope.source !== parentRecord.scope.source || record.generation !== nextGeneration(proof.parent_generation) ||
        record.operation_id !== proof.parent_save_operation || record.request_sha256 !== proof.parent_request_sha256 ||
        !sameValues(record.values, parentRecord.values) || !record.fork_retirement ||
        record.fork_retirement.child_draft_id !== child.scope.draft_id ||
        record.fork_retirement.operation_id !== command.fork_retirement.operation_id ||
        !sameForkLink(record.fork_retirement.fork_link, proof)) { throw new Error('父草稿清理回执不匹配'); }
      if (!this.rawForkCurrent(fork, parent, owner) || this.editorDraft !== child) { throw new Error('清理结果属于原界面，请从草稿入口核对'); }
      parent.dispose(); this.rawFork = undefined; this.rawForkParent = undefined; this.rawForkOwner = '';
      this.rawForkRetirement = ''; this.rawForkRetirementUnknown = false; this.rawForkAction = '';
      this.draftRecords = this.draftRecords.filter((value: DraftRecord): boolean => value.scope.draft_id !== proof.parent_draft_id);
      this.rawForkStatus = '接续草稿已建立，较新的输入以草稿确认状态为准。原卡片来源保持不变，请核对后继续处理。'; this.message = this.rawForkStatus;
      if (record.current_active || record.current_generation !== record.generation) { this.message += '父草稿的当前状态已变化，请从草稿入口核对。'; }
    } catch (error) { this.rawForkRetirementUnknown = true; throw new Error(String(error)); }
  }
  private async retryRawFork(): Promise<void> {
    if (this.businessWorking || this.businessPending()) { return; }
    const fork = this.rawFork, parent = this.rawForkParent, owner = this.rawForkOwner;
    if (!fork || !parent || this.rawForkWorking || this.busy || !this.rawForkCurrent(fork, parent, owner)) { return; }
    this.rawForkWorking = true;
    try {
      if (!fork.confirmed && !fork.unknown && !fork.conflicted) {
        fork.releaseRejectedParent(); this.rawFork = undefined; this.rawForkParent = undefined; this.rawForkOwner = '';
        this.rawForkAction = ''; this.rawForkWorking = false; await this.beginRawFork(); return;
      }
      if (fork.unknown) { await fork.retry(); }
      else if (this.editorDraft !== parent && this.editorDraft?.unknown) { await this.editorDraft.retry(); }
      await this.finishRawFork(fork, parent, owner, this.rawForkRetirementUnknown);
    } catch (error) { this.rawForkFailed(String(error)); }
    finally { this.rawForkWorking = false; this.draftChanged(); }
  }
  private attachDraft(card?: CardView, restored?: DraftRecord, preserveFocus: boolean = false,
    preparedWriter?: EditorDraftCoordinator): void {
    if (preparedWriter && (!restored || preparedWriter.disposed || !preparedWriter.writesPaused ||
      preparedWriter.scope.card_id !== restored.scope.card_id || preparedWriter.scope.draft_id !== restored.scope.draft_id ||
      preparedWriter.scope.source_kind !== restored.scope.source_kind || preparedWriter.scope.source_revision !== restored.scope.source_revision ||
      preparedWriter.scope.source !== restored.scope.source || !sameValues(preparedWriter.current, restored.values))) {
      throw new Error('Prepared recovery writer differs from the complete current native record');
    }
    this.ownedTodosBaseline = undefined;
    if (!this.businessPending()) {
      this.businessSession = undefined; this.businessBinding = undefined;
      if (!preparedWriter) { this.businessRecovery?.dispose(); this.businessRecovery = undefined; this.businessRecoverySession = undefined; }
      this.businessChanged();
    }
    this.draftRetiredIdentity = ''; this.retirementInput.clear();
    this.fieldPolicy.stop(); this.editorInputEpoch++; this.fieldCountEpochs.clear();
    this.fieldCountLabels = ['', '', '', '', '', ''];
    this.taskRenameValue = new DraftTextValue();
    this.attachmentSelection?.stop(); this.attachmentSelection = undefined;
    this.attachmentEditorIdentity = util.generateRandomUUID(true); this.attachmentEditingValues = '';
    this.editorViewOwner = this.attachmentEditorIdentity; this.editorViewRevoked = true; this.editorViewVisible = true; this.editorBoundary = '';
    this.attachmentSelectionState = { phase: 'idle', busy: false, total: 0, started: 0, selected: 0, retained: 0, error: '' };
    if (this.editorDraft && this.editorDraft !== preparedWriter) { this.editorDraft.dispose(); }
    this.draftConflict = false; this.draftStatus = '';
    this.draftSelectionPending = restored ? new Set(['title', 'description', 'hypothesis', 'conclusion', 'todos']) : new Set();
    if (!preserveFocus) { this.draftFocusedFields.clear(); this.lastDraftField = 'description'; }
    this.draftRestoreInput = !!restored; this.draftCaptureBlocked.clear(); this.draftCaptureIncomplete = false;
    const scope = restored?.scope ?? new DraftScope();
    if (!restored) {
      scope.card_id = card?.id ?? util.generateRandomUUID(true); scope.draft_id = util.generateRandomUUID(true);
      scope.source_kind = card ? 0 : 1; scope.source_revision = card?.revision ?? '0'; scope.source = card?.source ?? '';
    }
    this.draftSource = scope.source;
    this.editorBusinessMode = scope.source_kind === 1 ? 'create' :
      card?.content_kind === 'v2' || card?.content_kind === 'legacy' ? 'current_v2' : 'unknown';
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
    this.editorDraft = preparedWriter ?? new EditorDraftCoordinator(scope, values,
      async (serialized: string): Promise<DraftRecord> => {
        if (this.draftRetiredIdentity === scope.draft_id) { throw new DraftSaveFailure(true, '旧草稿已清理，当前输入尚未写入新草稿'); }
        const result = await workbench.send(serialized);
        if (!result.ok) { throw new DraftSaveFailure(result.effect === 'not_committed', result.error); }
        if (result.drafts.length !== 1) { throw new Error('草稿回执缺失'); }
        return result.drafts[0];
      }, () => this.draftChanged(), () => util.generateRandomUUID(true), restored);
    this.bindInputRules();
    // A restored frozen baseline remains separate from the current business
    // record. No implicit rebase or overwrite is permitted.
    this.draftConflict = scope.source_kind === 0 ? card?.revision !== scope.source_revision : !!card;
    this.draftChanged(); this.updateMarkdown(); this.refreshFieldCounts();
    this.importRecords = []; this.pendingSpools = []; this.attachmentDiagnostics = []; this.loadImports();
  }
  private restoreDraft(record: DraftRecord): void {
    if (this.businessWorking || this.businessPending()) { this.message = '请先核对原保存请求，当前编辑器仍保留。'; return; }
    if (this.busy || this.pending.length > 0 || this.editorOpen) { return; }
    if (record.business_link) { this.recoverLinkedBusinessDraft(record); return; }
    this.openDraftRecord(record);
  }
  private openDraftRecord(record: DraftRecord): void {
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
    if (this.rawFork && this.editorDraft === this.rawForkParent) {
      this.message = '接续原请求尚未确认，请使用接续核对入口。'; return false;
    }
    if (this.draftRetiredIdentity && this.draftRetiredIdentity === this.editorDraft?.scope.draft_id) {
      this.message = '旧草稿已清理，较新的输入仅在当前编辑器中，尚未写入新草稿。'; return false;
    }
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
    if (this.businessWorking || this.businessPending()) { return false; }
    return this.pageAlive && this.ready && this.editorOpen && !!this.editorDraft && !this.attachmentWorking &&
      !this.rawFork && !this.rawForkWorking && !this.editorBoundary && !this.editorViewRevoked &&
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
    if (title !== undefined) { this.adoptExternalInput('title', values.title); }
    else { draft.update(values); }
    await draft.flush(); this.message = '附件已保留在草稿中，保存卡片后生效。';
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
        await this.music?.pauseForMedia();
        if (!this.pageAlive || !this.foreground || epoch !== this.attachmentPreviewEpoch) { return; }
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
    if (this.businessWorking || this.businessPending()) { return; }
    const draft = this.editorDraft;
    if (!draft || this.rawFork || this.editorBoundary || !this.ownsEditorView(this.editorViewOwner)) { return; }
    this.editorBoundary = 'keep';
    try {
      if (!(await this.flushDraft())) { this.editorBoundary = ''; return; }
      draft.pauseWrites();
      if (!(await this.revokeEditorView(draft))) { return; }
      draft.resumeWrites();
      if (this.draftCaptureIncomplete || !(await this.flushDraft())) { this.editorBoundary = ''; this.remountEditorView(draft); return; }
      this.dirty = false; this.editorOpen = false; this.message = draft.confirmed ? '已确认的完整输入已保留，可从草稿入口继续。' : '';
      draft.dispose(); this.editorDraft = undefined; this.editorBoundary = '';
      this.taskEditId = ''; this.taskRenameText = ''; this.taskText = '';
      try { await this.loadDrafts(); } catch (_) { this.message = '已确认的完整输入已保留，草稿列表刷新失败，请重新打开核对。'; }
    } finally { this.draftWorking = false; }
  }
  private async closeSavedEditor(expected: DraftValues, epoch: number): Promise<boolean> {
    const draft = this.editorDraft;
    if (!draft || this.rawFork || this.editorBoundary || !this.ownsEditorView(this.editorViewOwner)) { return false; }
    if (this.draftCaptureIncomplete || this.editorInputEpoch !== epoch || !sameValues(draft.current, expected)) {
      if (!this.draftCaptureIncomplete) { await this.beginRawFork(); } return false;
    }
    this.editorBoundary = 'saved'; this.draftWorking = true;
    try {
      if (!(await this.flushDraft())) { this.editorBoundary = ''; return false; }
      draft.pauseWrites();
      if (!(await this.revokeEditorView(draft))) { return false; }
      draft.resumeWrites();
      if (this.draftCaptureIncomplete || this.editorInputEpoch !== epoch || !sameValues(draft.current, expected)) {
        this.editorBoundary = ''; const remounted = await this.remountEditorView(draft);
        this.message = '关闭期间有较新的输入，编辑器已保留；正在接续完整草稿。';
        if (remounted && !this.draftCaptureIncomplete) { await this.beginRawFork(); } return false;
      }
      if (!(await this.flushDraft())) { this.editorBoundary = ''; this.remountEditorView(draft); return false; }
      const retired = await this.retireDraft(expected);
      if (retired) { this.editorBoundary = ''; return true; }
      if (!this.draftRetirementUnknown) { this.editorBoundary = ''; this.remountEditorView(draft); }
      return false;
    } finally { this.draftWorking = false; }
  }
  private async retireDraft(expected?: DraftValues, reconcile: boolean = false): Promise<boolean> {
    const draft = this.editorDraft, record = draft?.confirmed;
    if (!this.editorViewRevoked || this.editorViewVisible || this.editorBoundaryDraft !== draft) {
      this.message = '编辑器尚未撤销，草稿清理未开始。'; return false;
    }
    if (this.draftRetiredIdentity && this.draftRetiredIdentity === draft?.scope.draft_id) { return false; }
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
    const editor = this.attachmentEditorIdentity, inputEpoch = this.editorInputEpoch, captured = copyValues(draft.current), original = this.draftRetirement;
    this.draftRetiring = true;
    try {
      const result = await workbench.send(original);
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
      if (this.editorDraft !== draft || this.attachmentEditorIdentity !== editor || this.editorInputEpoch !== inputEpoch ||
        !sameValues(draft.current, captured) || this.draftCaptureIncomplete) {
        if (this.draftRetirement === original) { this.draftRetirement = ''; this.draftRetirementUnknown = false; }
        if (this.editorDraft === draft) {
          this.draftRetiredIdentity = record.scope.draft_id; this.draftConflict = true;
          this.draftStatus = '旧草稿已清理，较新的输入仅在当前编辑器中，尚未写入新草稿。'; this.message = this.draftStatus;
        }
        return false;
      }
      draft.dispose(); this.editorDraft = undefined; this.draftStatus = ''; this.draftConflict = false;
      this.draftRetirement = ''; this.draftRetirementUnknown = false;
      this.draftRecords = this.draftRecords.filter((r: DraftRecord) => r.scope.draft_id !== record.scope.draft_id);
      try { await this.loadDrafts(); } catch (_) { this.message = '草稿已清理，列表刷新失败，请重新打开工作区核对。'; }
      return this.editorDraft === undefined && this.attachmentEditorIdentity === editor && this.editorInputEpoch === inputEpoch;
    } catch (_) {
      this.draftRetirementUnknown = true; this.message = '草稿清理回复未确认，原操作已保留，请显式核对。'; return false;
    } finally { this.draftRetiring = false; }
  }
  private discardDraft(): void {
    if (this.businessWorking || this.businessPending()) { this.message = '原保存请求尚未完成，不能放弃所属草稿。'; return; }
    if (this.attachmentWorking || this.pasteWorking) { return; }
    if (this.busy || this.pending.length > 0 || this.draftWorking || this.editorDraft?.unknown || this.rawFork || this.rawForkWorking || this.editorBoundary) { return; }
    this.getUIContext().showAlertDialog({ title: '放弃这份草稿？', message: '草稿中的文字将被丢弃，已保存卡片保持原样。',
      primaryButton: { value: '继续编辑', action: () => {} }, secondaryButton: { value: '放弃草稿', action: () => this.discardDraftConfirmed() } });
  }
  private async discardDraftConfirmed(): Promise<void> {
    if (this.businessWorking || this.businessPending()) { return; }
    const draft = this.editorDraft;
    if (this.draftCaptureIncomplete) { this.message = '完整输入尚未捕获，放弃未执行。'; return; }
    if (!draft || this.rawFork || this.editorBoundary || this.pending.length > 0) { return; }
    this.editorBoundary = 'discard';
    this.draftWorking = true;
    try {
      if (!(await this.flushDraft())) { this.editorBoundary = ''; return; }
      if (this.editorDraft !== draft || !this.ownsEditorView(this.editorViewOwner)) { return; }
      draft.pauseWrites();
      if (!(await this.revokeEditorView(draft))) { return; }
      if (this.draftCaptureIncomplete) {
        draft.resumeWrites(); this.editorBoundary = ''; this.remountEditorView(draft);
        this.message = '关闭期间的输入无法完整确认，放弃未执行；原事件仍保留，请完成输入后重试。'; return;
      }
      draft.resumeWrites();
      if (!(await this.flushDraft())) { this.editorBoundary = ''; this.remountEditorView(draft); return; }
      if (await this.retireDraft()) { this.dirty = false; this.taskText = ''; this.editorOpen = false; this.editorBoundary = ''; }
      else if (!this.draftRetirementUnknown && this.editorDraft === draft) { this.editorBoundary = ''; this.remountEditorView(draft); }
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
    try { if (await this.retireDraft(undefined, true)) { this.dirty = false; this.editorOpen = false; this.editorBoundary = ''; } }
    finally { this.draftWorking = false; }
  }
  private editorInputChanged(name: string, value: string, preview?: PreviewText): void {
    if (this.pageAlive && this.editorOpen && this.editorDraft && this.retirementCapture(name, value, preview)) { return; }
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
    this.openCardEditor(c);
  }
  private editorIdleForRead(): boolean {
    return !this.editorOpen && !this.dirty && !this.busy && !this.pending && !this.draftWorking &&
      !this.draftRetiring && !this.draftRetirementUnknown && !this.attachmentWorking && !this.pasteWorking &&
      !this.rawFork && !this.rawForkWorking && !this.editorBoundary && this.businessImportsReady();
  }
  private currentEditorCardComplete(card?: CardView): boolean {
    if (!card || !['v2', 'legacy'].includes(card.content_kind ?? '') || !validGeneration(card.revision) || card.revision === '0' ||
      typeof card.source !== 'string' || !/^(?:[0-9a-f]{2})+$/.test(card.source) ||
      [card.id, card.title, card.description, card.hypothesis, card.conclusion, card.category, card.stage, card.deleted_at]
        .some((value: string): boolean => typeof value !== 'string') ||
      typeof card.favorite !== 'boolean' || typeof card.deleted !== 'boolean' || !Array.isArray(card.tasks) || !Array.isArray(card.assets)) { return false; }
    return card.tasks.every((task: TaskView): boolean => typeof task.id === 'string' && task.id.length > 0 &&
      typeof task.text === 'string' && Number.isInteger(task.completion)) &&
      card.assets.every((asset: AssetView): boolean => typeof asset.id === 'string' && asset.id.length > 0 &&
        typeof asset.name === 'string' && typeof asset.kind === 'string' && typeof asset.media_type === 'string' &&
        typeof asset.byte_length === 'string' && typeof asset.sha256 === 'string');
  }
  private editorOpenCurrent(identity: string): boolean {
    return this.pageAlive && this.foreground && this.editorOpenIdentity === identity && this.editorIdleForRead() && !this.businessPending();
  }
  private async openCardEditor(c: CardView): Promise<void> {
    if (this.businessWorking || this.businessPending()) { this.message = '请先核对原保存请求，当前编辑器仍保留。'; return; }
    if (!this.editorIdleForRead()) { this.message = '当前输入或文件步骤尚未完成，请先保留并核对。'; return; }
    if (this.editorOpen || this.busy || this.draftWorking || this.draftRetiring || this.draftRetirementUnknown || this.pending.length > 0) { return; }
    if (this.dirty) { this.message = '请先保存当前编辑，输入已保留。'; return; }
    if (this.cards.find((v: CardView) => v.id === c.id)?.source !== c.source || c.deleted) { return; }
    const identity = util.generateRandomUUID(true); this.editorOpenIdentity = identity; this.businessWorking = true;
    let recovered: DraftRecord | undefined = undefined;
    try {
      const reply = await workbench.send(JSON.stringify(new Command()));
      if (!reply.ok || reply.error !== '' || !Array.isArray(reply.cards)) { throw new Error(reply.error || 'Current card read is incomplete'); }
      const card = reply.cards.find((value: CardView): boolean => value.id === c.id);
      if (!card || card.deleted || !this.currentEditorCardComplete(card)) {
        throw new Error('当前卡片的完整版本与任务来源尚未确认');
      }
      const command = new Command(); command.action = 'draft_list';
      const drafts = await workbench.send(JSON.stringify(command));
      if (!drafts.ok || drafts.error !== '' || !Array.isArray(drafts.drafts)) { throw new Error(drafts.error || 'Current draft list is incomplete'); }
      if (!this.editorOpenCurrent(identity)) { throw new Error('读取期间编辑归属已变化，未替换任何输入'); }
      recovered = drafts.drafts.find((record: DraftRecord): boolean => record.scope.card_id === card.id && record.active &&
        record.current_active && record.generation === record.current_generation);
      if (recovered) { validateRecord(recovered, true); }
      this.cards = reply.cards; this.draftRecords = drafts.drafts;
      if (!recovered) {
        this.detailId = ''; this.taskEditId = ''; this.taskText = ''; this.taskRenameText = '';
        this.previewBody = false; this.selected = card.id; this.title = card.title; this.description = card.description;
        this.hypothesis = card.hypothesis; this.conclusion = card.conclusion; this.category = card.category;
        this.dirty = false; this.editorOpen = true; this.message = ''; this.attachDraft(card); this.refreshPreview();
      }
    } catch (error) { this.message = '重新编辑未打开：' + String(error) + '。现有文字与任务未改变。'; }
    finally { if (this.editorOpenIdentity === identity) { this.businessWorking = false; } }
    if (recovered && this.editorOpenCurrent(identity)) { this.detailId = ''; this.restoreDraft(recovered); }
  }
  private editorMetadataReady(owner: string): boolean {
    return this.ownsEditorView(owner) && this.editorDraft?.scope.source_kind === 1 && !this.businessWorking &&
      !this.businessPending() && !this.rawFork && !this.rawForkWorking && !this.editorBoundary && !this.pasteWorking &&
      !this.draftWorking && !this.attachmentWorking && !this.draftRetiring && !this.draftRetirementUnknown && !this.draftRestoreInput;
  }
  private newCard(): boolean {
    if (this.businessWorking || this.businessPending()) { this.message = '请先核对原保存请求，当前编辑器仍保留。'; return false; }
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
    if (this.businessWorking || this.businessPending()) { this.message = '请先核对或完成原保存请求，当前输入仍保留。'; return; }
    if (cmd.action === 'create' || cmd.action === 'edit') { throw new Error('编辑器保存必须使用完整原请求日志'); }
    if (this.attachmentWorking || this.pasteWorking) { this.message = '文件或粘贴操作尚未完成，请稍候。'; return; }
    if (this.busy || this.pending.length > 0 || this.draftWorking || this.fieldValidationWorking || this.draftRetirementUnknown || this.rawFork || this.rawForkWorking || this.editorBoundary) { return; }
    if (this.taskEditId.length > 0 && cmd.action !== 'task_rename') { this.message = '请先保存或取消待办重命名'; return; }
    if (!this.inputReadyFor(cmd.action)) { this.message = '输入规则尚未确认，请完成输入或重新检查；原始草稿仍可保留。'; return; }
    const draft = this.editorDraft, editor = this.attachmentEditorIdentity, inputEpoch = this.editorInputEpoch;
    const selected = this.selected, task = this.taskEditId, rename = copyText(this.taskRenameValue);
    const original = JSON.stringify(cmd), frozen = raw ? copyValues(raw) : (this.editorOpen ? draft?.current : undefined);
    const owned = (): boolean => this.pageAlive && this.foreground && this.editorDraft === draft && this.attachmentEditorIdentity === editor &&
      this.editorInputEpoch === inputEpoch && this.selected === selected && this.taskEditId === task && JSON.stringify(cmd) === original &&
      (!frozen || (!!draft && !draft.disposed && sameValues(frozen, draft.current)));
    this.fieldValidationWorking = true;
    try {
      if (cmd.action === 'create' || cmd.action === 'edit') {
        if (!frozen || frozen.title.text !== cmd.title || frozen.description.text !== cmd.description ||
          frozen.hypothesis.text !== cmd.hypothesis || frozen.conclusion.text !== cmd.conclusion) { throw new Error('保存目标已变化，完整输入已保留。'); }
        if (cmd.action === 'create') {
          if (cmd.todos !== frozen.todos.text || frozen.todos.text.split('\n').length > 100) { throw new Error('待办原文或行数无法确认，完整草稿仍保留。'); }
          await this.fieldPolicy.requireTextValue('todos', frozen.todos, owned);
        } else if (cmd.todos !== '') { throw new Error('已有任务按各自身份保存，普通编辑不替换任务集合。'); }
        for (const name of ['title', 'description', 'hypothesis', 'conclusion']) {
          await this.fieldPolicy.requireTextValue(name, name === 'title' ? frozen.title : name === 'description' ? frozen.description :
            name === 'hypothesis' ? frozen.hypothesis : frozen.conclusion, owned);
        }
      } else if (cmd.action === 'task_add') {
        if (!frozen || frozen.todos.text !== cmd.text) { throw new Error('待办输入已变化，完整输入已保留。'); }
        await this.fieldPolicy.requireTextValue('todos', frozen.todos, owned);
      } else if (cmd.action === 'task_rename') {
        if (!task || task !== cmd.task_id || rename.text.trim() !== cmd.text) { throw new Error('重命名输入已变化，完整输入已保留。'); }
        await this.fieldPolicy.requireTextValue('todos', rename, owned);
      }
      if (!owned()) { throw new Error('保存目标已变化，完整输入已保留。'); }
      // Raw retention remains independent of business limits and preserves IME.
      if (this.editorOpen && !(await this.flushDraft())) { return; }
      if (!owned()) { throw new Error('保存目标已变化，完整输入已保留。'); }
    } catch (error) { if (this.pageAlive && this.attachmentEditorIdentity === editor && this.editorDraft === draft) { this.message = String(error); } return; }
    finally { this.fieldValidationWorking = false; }
    if ((cmd.action === 'create' || cmd.action === 'edit') && this.editorDraft?.confirmed) {
      const record = this.editorDraft.confirmed;
      cmd.draft_id = record.scope.draft_id; cmd.generation = record.generation; cmd.draft_operation = record.operation_id;
    }
    this.submittedRaw = frozen;
    this.submittedInputEpoch = inputEpoch; this.submittedEditor = editor; this.submittedDraft = draft;
    this.submittedRename = cmd.action === 'task_rename' ? copyText(rename) : undefined;
    this.submittedRenameEpoch = inputEpoch; this.submittedRenameEditor = editor;
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
          this.pending = ''; this.submittedRaw = undefined; this.submittedRename = undefined; this.message = `未提交：${result.error}。请修正后再保存，输入已保留。`;
        } else { this.message = `保存未确认：${result.error}。原请求与当前输入均已保留。`; }
        return;
      }
      this.cards = result.cards; this.pending = '';
      const unchangedRaw = !this.draftCaptureIncomplete &&
        this.submittedInputEpoch === this.editorInputEpoch && this.submittedEditor === this.attachmentEditorIdentity && this.submittedDraft === this.editorDraft &&
        (!this.submittedRaw || !this.editorDraft || sameValues(this.editorDraft.current, this.submittedRaw));
      if ((cmd.action === 'create' || cmd.action === 'edit') &&
          this.submittedEditor === this.attachmentEditorIdentity && this.submittedDraft === this.editorDraft) {
        this.selected = cmd.id;
        const values = this.editorDraft?.current ?? this.editorValues;
        this.dirty = values.title.text !== cmd.title || values.description.text !== cmd.description ||
          values.hypothesis.text !== cmd.hypothesis || values.conclusion.text !== cmd.conclusion ||
          (cmd.action === 'create' && values.todos.text !== cmd.todos);
      }
      const todoRaw = this.editorDraft?.current.todos ?? this.editorValues.todos;
      if (cmd.action === 'task_add' && this.taskText === cmd.text && todoRaw.text === cmd.text &&
        todoRaw.composing_start === -1 && todoRaw.composing_end === -1 && unchangedRaw && !this.draftCaptureBlocked.has('todos')) {
        this.taskText = ''; this.draftTextChanged('todos', '');
      }
      if (cmd.action === 'task_rename' && this.selected === cmd.id && this.taskEditId === cmd.task_id &&
        this.taskRenameText.trim() === cmd.text && this.submittedRename &&
        this.submittedRenameEditor === this.attachmentEditorIdentity && this.submittedRenameEpoch === this.editorInputEpoch &&
        this.taskRenameValue.composing_start === -1 && this.taskRenameValue.composing_end === -1 &&
        samePasteTarget(this.submittedRename, this.taskRenameValue)) { this.taskEditId = ''; this.taskRenameText = ''; }
      this.message = result.receipt_revision.length > 0 ? `已保存 · 操作修订 ${result.receipt_revision}` : '已刷新';
      const completionValues = this.editorDraft?.current;
      // Ordinary body/favorite/stage commands do not consume pending todo text.
      // Keep its raw journal until an explicit task submission or handoff.
      const consumedInput = unchangedRaw && (cmd.action === 'create' ? !!completionValues && completionValues.todos.text === cmd.todos :
        (completionValues?.todos.text.length ?? 0) === 0);
      if ((cmd.action === 'create' || cmd.action === 'edit') && !this.dirty && consumedInput) {
        // A business receipt does not certify later raw input. Retire only the
        // exact latest confirmed draft; otherwise keep the independent draft.
        if (completionValues && await this.closeSavedEditor(completionValues, this.submittedInputEpoch)) { this.editorOpen = false; }
      } else if (this.editorOpen && !this.dirty && consumedInput && this.taskEditId.length === 0) {
        // Preserve the original source bytes. A business-source continuation
        // is a separate contract, never a string-only attachDraft rebase.
        if (this.submittedDraft === this.editorDraft && this.submittedEditor === this.attachmentEditorIdentity) { await this.beginRawFork(); }
      } else if ((cmd.action === 'create' || cmd.action === 'edit') && this.editorOpen && !this.draftCaptureIncomplete &&
        this.submittedDraft === this.editorDraft && this.submittedEditor === this.attachmentEditorIdentity) {
        await this.beginRawFork();
      }
      const draft = this.editorOpen ? this.editorDraft : undefined;
      if (draft) {
        const record = this.cards.find((c: CardView) => c.id === draft.scope.card_id);
        this.draftConflict = this.draftRetiredIdentity === draft.scope.draft_id ||
          (draft.scope.source_kind === 0 ? record?.revision !== draft.scope.source_revision : !!record);
        if (this.draftConflict) { this.message = this.draftRetiredIdentity === draft.scope.draft_id ? this.draftStatus :
          '卡片已保存，较新的草稿输入仍保留。原卡片已变化，请核对后处理。'; }
      }
      this.submittedRaw = undefined; this.submittedRename = undefined;
      this.refreshPreview();
    } catch (_) { this.message = '回复未确认。请用原请求核对，不会创建新的操作。'; }
    finally { this.busy = false; }
  }
  private async save(): Promise<void> {
    if (this.attachmentWorking || this.pasteWorking || !this.businessImportsReady()) { this.message = '请先确认文件或粘贴操作，完整输入仍保留。'; return; }
    if (!this.ready || this.busy || this.businessWorking || this.businessPending() || this.pending || this.rawFork ||
      this.rawForkWorking || this.editorBoundary || this.draftWorking || this.fieldValidationWorking || this.draftConflict ||
      this.draftCaptureIncomplete || this.draftRetirementUnknown || !this.ownsEditorView(this.editorViewOwner)) { return; }
    if (this.taskEditId.length > 0) { this.message = '请先保存或取消待办重命名'; return; }
    if (this.title.trim().length === 0) { this.message = '请输入卡片标题'; return; }
    const draft = this.editorDraft;
    if (!draft || draft.unknown || draft.conflicted || draft.disposed || draft.writesPaused) { return; }
    const owner = this.editorViewOwner, identity = this.attachmentEditorIdentity, epoch = this.editorInputEpoch, values = draft.current;
    const mode = this.ownedTodosBaseline ? 'continued_todos' : draft.scope.source_kind === 1 ? 'create' : this.editorBusinessMode;
    if (mode !== 'create' && mode !== 'continued_todos' && mode !== 'current_v2' && mode !== 'edit') {
      this.message = '当前完整版本来源尚未确认，请保留草稿并重新读取卡片。'; return;
    }
    if (!this.inputReadyFor(mode === 'create' ? 'create' : 'edit')) { this.message = '请先完成文字输入或规则检查，草稿仍保留。'; return; }
    if ((mode === 'edit' || mode === 'current_v2') && values.todos.text.length > 0) { this.message = '待办文字属于原草稿接续，请恢复其保存上下文后再继续，完整文字未清除。'; return; }
    this.businessWorking = true; this.fieldValidationWorking = true;
    let binding: BusinessEditorBinding | undefined = undefined;
    try {
      // An unchanged source0 editor still needs an actual publication. Opening
      // alone is read-only; this is the user's explicit Save action.
      const publication = await draft.ensureConfirmed();
      if (!this.ownsEditorView(owner) || this.editorDraft !== draft || identity !== this.attachmentEditorIdentity ||
        epoch !== this.editorInputEpoch || this.draftCaptureIncomplete || !sameValues(values, draft.current) ||
        !sameValues(publication.values, values) || !this.businessImportsReady()) { throw new Error('保存前输入已变化，完整草稿仍保留。'); }
      if ((mode === 'create' || mode === 'continued_todos') && values.todos.text.split('\n').length > 100) {
        throw new Error('待办超过 100 行，完整草稿已保留，请调整后再保存');
      }
      draft.pauseWrites();
      binding = new BusinessEditorBinding(draft, identity, owner, epoch, values, publication);
      const business = new EditorBusinessFields(); business.action = mode === 'create' ? 'create' : 'edit';
      business.id = draft.scope.card_id; business.operation = util.generateRandomUUID(true); business.source = draft.scope.source;
      business.title = values.title.text; business.description = values.description.text; business.hypothesis = values.hypothesis.text;
      business.conclusion = values.conclusion.text; business.todos = values.todos.text; business.category = values.category; business.stage = values.stage;
      const hooks = this.businessHooks(binding), prepareOperation = util.generateRandomUUID(true);
      const session = mode === 'continued_todos' && this.ownedTodosBaseline ?
        await this.ownedTodosBaseline.continueTodos(business, publication, prepareOperation, hooks) :
        await EditorBusinessSessionCoordinator.prepare(mode, business, publication, prepareOperation, hooks);
      this.businessSession = session; this.businessBinding = binding; this.businessRecoveryIdentity = '';
      this.businessRecovery?.dispose(); this.businessRecovery = undefined; this.businessRecoverySession = undefined;
      await session.prepareIntent();
      await session.issueIntent();
      await session.loadTransport();
      await session.save();
      await this.finishBusinessInput(session, binding);
    } catch (error) {
      const session = this.businessSession;
      // A known rejected first prepare has no durable intent to reconcile.
      // Release only this proposal, keeping the complete draft and all Unknown
      // requests intact so another explicit Save can try again.
      if (binding && this.businessBinding === binding && session?.phase === 'unprepared' &&
        !session.pendingNative && !session.pendingRead && !session.committed && !session.businessUnknown) {
        this.businessSession = undefined; this.businessBinding = undefined;
      }
      this.message = '保存尚未完成：' + String(error) + '。完整输入及已派发的原请求仍保留。';
    } finally {
      if (!this.businessHandoff && !draft.disposed && draft.writesPaused) { draft.resumeWrites(); }
      this.businessWorking = false; this.fieldValidationWorking = false; this.businessChanged();
      try { await this.loadBusinessIntents(); } catch (_) { /* Existing request remains available for explicit reconciliation. */ }
    }
  }
  private businessPending(): boolean { return !!this.businessSession && this.businessSession.phase !== 'closed'; }
  private businessImportsReady(): boolean {
    const selected = new Set((this.editorDraft?.current.assets ?? []).map((asset: AssetSelection): string => asset.asset_id));
    return !this.attachmentPending && this.pendingSpools.length === 0 && !this.attachmentWorking && !this.pasteWorking &&
      !this.importRecords.some((record: ImportRecord): boolean => record.phase === 'pending' || record.phase === 'unknown' ||
        record.phase === 'ready' && !selected.has(record.asset_id));
  }
  private businessCurrent(binding: BusinessEditorBinding): boolean {
    return this.pageAlive && this.foreground && this.editorOpen && this.editorDraft === binding.liveDraft &&
      !binding.liveDraft.disposed && binding.identity === this.attachmentEditorIdentity && binding.owner === this.editorViewOwner;
  }
  private businessParentMatches(binding: BusinessEditorBinding): boolean {
    if (!binding.parent) { return false; }
    const scope = binding.parent.scope, original = binding.publication.scope;
    return scope.card_id === original.card_id && scope.draft_id === original.draft_id &&
      scope.source_kind === original.source_kind && scope.source_revision === original.source_revision && scope.source === original.source;
  }
  private businessUnchanged(binding: BusinessEditorBinding): boolean {
    return !!binding.parent && this.businessCurrent(binding) && this.businessParentMatches(binding) && !this.draftCaptureIncomplete &&
      this.editorInputEpoch === binding.epoch && binding.liveDraft === binding.parent && sameValues(binding.values, binding.parent.current);
  }
  private businessExact(binding: BusinessEditorBinding): boolean {
    return this.businessUnchanged(binding) && this.ownsEditorView(binding.owner);
  }
  private businessHooks(binding?: BusinessEditorBinding, recovery: string = ''): EditorBusinessSessionHooks {
    const parent = binding?.parent;
    return { fields: this.fieldPolicy, changed: (): void => this.businessChanged(),
      isCurrent: (): boolean => binding ? this.businessCurrent(binding) : this.pageAlive && this.businessRecoveryIdentity === recovery,
      isExact: (): boolean => !!binding && this.businessExact(binding),
      parentReady: (): boolean => !!binding && !!parent && this.businessParentMatches(binding) && parent.writesPaused && !parent.disposed &&
        !parent.saving && !parent.unknown && !parent.conflicted &&
        parent.confirmed?.generation === binding.publication.generation &&
        parent.confirmed?.request_sha256 === binding.publication.request_sha256 && sameValues(parent.current, binding.publication.values),
      send: (wire: string): Promise<EditorBusinessSessionReply> => {
        // Admission is synchronous into the one existing Workbench tail. Only
        // afterwards may parent debounce writes enqueue a newer raw generation.
        const queued = workbench.send(wire) as Promise<EditorBusinessSessionReply>;
        if ((JSON.parse(wire) as Command).action === 'editor_save' && parent &&
          !parent.disposed && parent.writesPaused) { parent.resumeWrites(); }
        return queued;
      } };
  }
  private businessChanged(): void {
    const session = this.businessSession;
    if (!session) { this.businessStatus = ''; return; }
    this.businessStatus = this.businessHandoff?.unknown ? '卡片已保存，草稿接续或关闭尚未确认；请核对原步骤，完整输入仍保留。' :
      session.qualified ? '卡片已确认保存；草稿关闭或后续输入接续以各自回执为准。' :
      session.committed ? '已收到保存事实，完整结果尚待核对；当前输入仍保留。' :
      session.unknown ? '原保存请求尚未确认，可核对或继续同一请求；较新的输入仍保留。' :
      session.phase === 'prepared' ? '原请求已保留，尚未签发保存；可继续或取消此请求。' :
      session.phase === 'closed' ? '原请求已完成关闭。' : '原请求已保留，保存尚未完成。';
  }
  private async loadBusinessIntents(more: boolean = false): Promise<void> {
    if (!this.ready || more && !this.businessNext) { return; }
    try {
      const page = await EditorBusinessSessionCoordinator.discover(more ? this.businessNext : '', 16, '', this.businessHooks());
      const retained = more ? this.businessIntents : [];
      this.businessIntents = retained.concat(page.intents); this.businessNext = page.nextAfter; this.businessListError = '';
    } catch (error) { this.businessListError = '保存请求列表未读完：' + String(error); }
  }
  private async restoreBusinessIntent(view: EditorIntentView): Promise<void> {
    if (this.businessWorking || this.busy || this.pending || this.businessPending()) { return; }
    this.businessWorking = true; const recovery = view.proof.intent_id;
    this.businessRecoveryIdentity = recovery;
    try {
      const binding = this.editorDraft && this.ownsEditorView(this.editorViewOwner) &&
        this.editorDraft.scope.card_id === view.card_id && this.editorDraft.confirmed ?
        new BusinessEditorBinding(this.editorDraft, this.attachmentEditorIdentity, this.editorViewOwner,
          this.editorInputEpoch, this.editorDraft.current, this.editorDraft.confirmed) : undefined;
      const session = await EditorBusinessSessionCoordinator.restore(view.proof, this.businessHooks(binding, recovery));
      if (this.businessRecoveryIdentity !== recovery || !this.pageAlive) { return; }
      this.businessRecovery?.dispose(); this.businessRecovery = undefined; this.businessRecoverySession = undefined;
      if (binding) { binding.publication = session.publication; binding.values = copyValues(session.publication.values); }
      this.businessBinding = binding && this.businessParentMatches(binding) ? binding : undefined; this.businessSession = session;
      this.message = '已读取原请求。请明确选择核对或继续保存，读取本身不会提交。';
      if (this.editorOpen) { this.draftsOpen = false; }
    } catch (error) { this.message = '原请求未读完整：' + String(error) + '。可从列表重新读取，未派发保存。'; }
    finally { this.businessWorking = false; this.businessChanged(); }
  }
  private async reconcileBusiness(action: string): Promise<void> {
    let session = this.businessSession; const binding = this.businessBinding;
    if (!session || this.businessWorking || session.saving || this.busy || this.pending || this.attachmentWorking || this.pasteWorking) { return; }
    this.businessWorking = true;
    try {
      if (action === 'native') {
        if (session.pendingNative === 'prepare') { await session.retryPrepare(); }
        else if (session.pendingNative === 'issue') { await session.retryIssue(); }
        else if (session.pendingNative === 'cancel') { await session.retryCancel(); }
        else if (session.pendingRead) { await session.retryRead(); }
        else if (!session.transportReady && session.phase === 'issued') { await session.loadTransport(); }
      } else if (action === 'cancel') {
        await session.cancelPrepared(util.generateRandomUUID(true));
      } else if (action === 'issue') {
        if (!binding?.parent) { throw new Error('请先恢复此请求所属的完整父草稿，才能签发原请求。'); }
        binding.parent.pauseWrites(); await session.issueIntent(); await session.loadTransport();
      } else if (action === 'inspect') {
        if (!session.transportReady && session.phase === 'issued') { await session.loadTransport(); }
        if (session.inspectionUnknown) { await session.retryInspect(); } else { await session.inspect(); }
        await this.finishBusinessInput(session, binding);
      } else if (action === 'save') {
        const proof = session.proof;
        if (session.committed) { throw new Error('已收到提交事实，请核对完整结果或完成草稿接续，不重建保存会话。'); }
        if (!proof || session.phase !== 'issued' || session.pendingNative || session.pendingRead) { throw new Error('请先核对原请求签发与完整传输。'); }
        // Explicitly restore immutable native authority, including issued S1
        // before its first send when newer S2 has already arrived. No current
        // card/raw reconstruction and no new business operation is permitted.
        if (!session.businessUnknown) {
          session = await EditorBusinessSessionCoordinator.restore(proof, this.businessHooks(binding, this.businessRecoveryIdentity));
          this.businessSession = session;
        }
        await session.retrySave(); await this.finishBusinessInput(session, binding);
      }
    } catch (error) { this.message = '原请求尚未完成：' + String(error) + '。当前输入仍保留。'; }
    finally {
      if (!this.businessHandoff && binding?.parent && !binding.parent.disposed && binding.parent.writesPaused) { binding.parent.resumeWrites(); }
      this.businessWorking = false; this.businessChanged(); await this.loadBusinessIntents();
    }
  }
  private async resumeBusinessEditor(): Promise<void> {
    const session = this.businessSession, proof = session?.proof;
    if (!session || !proof || this.businessWorking || this.editorOpen || this.busy || this.pending) { return; }
    const record = this.draftRecords.find((value: DraftRecord): boolean =>
      value.scope.draft_id === session.publication.scope.draft_id && value.active && value.current_active &&
      value.current_generation === value.generation);
    if (!record && session.phase !== 'prepared' && session.phase !== 'issued') { await this.recoverCurrentBusinessChild(); return; }
    if (!record) { this.message = '原草稿当前归属未找到，请先核对保存或接续结果。没有从卡片重建输入。'; return; }
    this.businessWorking = true;
    try {
      this.openDraftRecord(record);
      const draft = this.editorDraft;
      if (!draft) { return; }
      const binding = new BusinessEditorBinding(draft, this.attachmentEditorIdentity, this.editorViewOwner,
        this.editorInputEpoch, session.publication.values, session.publication);
      if (this.editorDraft !== draft || this.attachmentEditorIdentity !== binding.identity) { return; }
      const sessionHooks = this.businessHooks(binding);
      const ownerHooks: EditorBusinessSessionOwnerHooks = { changed: sessionHooks.changed, isCurrent: sessionHooks.isCurrent,
        isExact: sessionHooks.isExact, parentReady: sessionHooks.parentReady };
      session.rebindOwner(ownerHooks);
      this.businessBinding = binding;
      this.message = '已恢复当前完整草稿，原保存请求仍独立保留。请选择核对或继续原请求。';
    } catch (error) { this.message = '原保存请求恢复未完成：' + String(error) + '。草稿和原请求仍保留。'; }
    finally { this.businessWorking = false; this.businessChanged(); }
  }
  private businessRecoveryCurrent(identity: string): boolean {
    if (!this.pageAlive || !this.foreground || this.businessRecoveryIdentity !== identity) { return false; }
    return this.editorOpen ? !!this.businessBinding && this.businessCurrent(this.businessBinding) : this.editorIdleForRead();
  }
  private businessOwnsTodos(session: EditorBusinessSessionCoordinator): boolean {
    if (!session.qualified) { return false; }
    const original = JSON.parse(session.originalRequest) as EditorBusinessSubmission;
    return original.mode === 'create' || original.mode === 'continued_todos';
  }
  private businessRecoveryHooks(identity: string): EditorBusinessHandoffHooks {
    return { send: (wire: string): Promise<EditorBusinessHandoffReply> => workbench.send(wire) as Promise<EditorBusinessHandoffReply>,
      sendDraft: async (wire: string): Promise<DraftRecord> => {
        const reply = await workbench.send(wire);
        if (!reply.ok) { throw new DraftSaveFailure(reply.effect === 'not_committed', reply.error); }
        if (reply.drafts.length !== 1) { throw new Error('恢复子草稿的完整回执缺失'); } return reply.drafts[0];
      }, changed: (): void => { this.draftChanged(); this.businessChanged(); }, operation: (): string => util.generateRandomUUID(true),
      isCurrent: (): boolean => this.businessRecoveryCurrent(identity), isExact: (): boolean => false,
      importsReady: (): boolean => this.businessImportsReady(),
      childCurrent: (child: EditorDraftCoordinator): boolean => !!this.businessBinding &&
        this.businessRecoveryCurrent(identity) && this.businessBinding.liveDraft === child && this.editorDraft === child &&
        !this.draftCaptureIncomplete && this.ownsEditorView(this.editorViewOwner) };
  }
  private async recoverLinkedBusinessDraft(record: DraftRecord): Promise<void> {
    if (!record.business_link || this.businessWorking || this.businessPending() || !this.editorIdleForRead()) { return; }
    this.businessWorking = true; const identity = util.generateRandomUUID(true); this.businessRecoveryIdentity = identity;
    this.businessRecovery?.dispose(); this.businessRecovery = undefined; this.businessRecoverySession = undefined;
    try {
      const source = record.business_link.intent, proof = new EditorIntentProof();
      proof.intent_id = source.intent_id; proof.prepare_operation = source.prepare_operation; proof.generation = source.generation;
      proof.prepared_record_sha256 = source.prepared_record_sha256; proof.request_sha256 = source.request_sha256;
      const session = await EditorBusinessSessionCoordinator.restore(proof, this.businessHooks(undefined, identity));
      if (!this.businessRecoveryCurrent(identity)) { throw new Error('恢复归属已变化，未安装草稿'); }
      this.businessSession = session; this.businessBinding = undefined;
      await session.inspect();
      await this.prepareCurrentBusinessRecovery(session, identity, record.scope.draft_id);
    } catch (error) { this.message = '接续草稿恢复尚未完成：' + String(error) + '。原请求与完整草稿均保留。'; }
    finally { this.businessWorking = false; this.businessChanged(); }
  }
  private async recoverCurrentBusinessChild(): Promise<void> {
    const session = this.businessSession;
    if (!session || this.businessWorking || session.saving || !this.editorIdleForRead()) { return; }
    this.businessWorking = true;
    const identity = this.businessRecovery ? this.businessRecoveryIdentity : util.generateRandomUUID(true);
    this.businessRecoveryIdentity = identity;
    try {
      const owner: EditorBusinessSessionOwnerHooks = { changed: (): void => this.businessChanged(),
        isCurrent: (): boolean => this.businessRecoveryCurrent(identity), isExact: (): boolean => false, parentReady: (): boolean => false };
      session.rebindOwner(owner);
      if (!session.qualified) { if (session.inspectionUnknown) { await session.retryInspect(); } else { await session.inspect(); } }
      await this.prepareCurrentBusinessRecovery(session, identity);
    } catch (error) { this.message = '当前接续草稿尚未恢复：' + String(error) + '。已知保存事实和原步骤均保留。'; }
    finally { this.businessWorking = false; this.businessChanged(); }
  }
  private async prepareCurrentBusinessRecovery(session: EditorBusinessSessionCoordinator, identity: string, expectedDraft: string = ''): Promise<void> {
    const reply = await workbench.send(JSON.stringify(new Command()));
    if (!reply.ok || reply.error !== '' || !Array.isArray(reply.cards)) { throw new Error(reply.error || '当前卡片完整读取未完成'); }
    if (!this.businessRecoveryCurrent(identity) || this.editorOpen) { throw new Error('当前恢复输入边界已变化'); }
    this.cards = reply.cards;
    if (!this.businessRecovery) {
      this.businessRecovery = new EditorBusinessRecoveryCoordinator(session, this.businessRecoveryHooks(identity),
        (): boolean => this.businessRecoveryCurrent(identity));
      this.businessRecoverySession = session;
    }
    if (this.businessRecoverySession !== session) { throw new Error('恢复准备属于另一份原请求，未替换已知事实'); }
    const recovery = this.businessRecovery;
    const value = recovery.ready ?? (recovery.pendingRead ? await recovery.retry() : await recovery.load());
    if (expectedDraft && value.record.scope.draft_id !== expectedDraft) { throw new Error('原草稿列表身份与固定接续计划不同'); }
    this.installRecoveredBusinessChild(session, recovery, value, identity);
  }
  private installRecoveredBusinessChild(session: EditorBusinessSessionCoordinator, recovery: EditorBusinessRecoveryCoordinator,
    value: EditorBusinessRecoveryReady, identity: string): void {
    const ready = recovery.ready;
    if (!this.businessRecoveryCurrent(identity) || this.editorOpen || this.businessRecovery !== recovery ||
      this.businessSession !== session || this.businessRecoverySession !== session || session.saving || !session.qualified ||
      !ready || ready.writer !== value.writer || ready.handoff !== value.handoff || value.handoff.saving ||
      JSON.stringify(copyRecord(ready.record)) !== JSON.stringify(copyRecord(value.record)) ||
      JSON.stringify(ready.parentReference) !== JSON.stringify(value.parentReference) ||
      value.writer.disposed || !value.writer.writesPaused || !sameValues(value.writer.current, value.record.values)) {
      throw new Error('安装前当前子草稿或界面归属已变化');
    }
    const record = value.record, card = this.cards.find((current: CardView): boolean => current.id === record.scope.card_id);
    validateRecord(record, true);
    if (card && !this.currentEditorCardComplete(card)) { throw new Error('当前卡片返回不完整，原完整草稿保留'); }
    const ownsTodos = this.businessOwnsTodos(session);
    const draft = recovery.claimWriter();
    if (draft !== value.writer) { throw new Error('恢复 writer 身份已变化，未安装另一份草稿'); }
    this.detailId = ''; this.selected = record.scope.card_id;
    this.title = record.values.title.text; this.description = record.values.description.text;
    this.hypothesis = record.values.hypothesis.text; this.conclusion = record.values.conclusion.text;
    this.category = record.values.category; this.taskText = record.values.todos.text;
    this.taskEditId = ''; this.taskRenameText = ''; this.previewBody = false; this.editorOpen = true; this.draftsOpen = false;
    this.dirty = false; this.attachDraft(card, record, false, value.writer);
    const binding = new BusinessEditorBinding(undefined, this.attachmentEditorIdentity, this.editorViewOwner,
      this.editorInputEpoch, session.publication.values, session.publication, value.writer);
    this.businessBinding = binding; this.businessSession = session;
    const hooks = this.businessHooks(binding), owner: EditorBusinessSessionOwnerHooks = { changed: hooks.changed,
      isCurrent: hooks.isCurrent, isExact: hooks.isExact, parentReady: hooks.parentReady };
    session.rebindOwner(owner);
    this.ownedTodosBaseline = ownsTodos ? session : undefined;
    this.businessHandoff = value.handoff.closed ? undefined : value.handoff;
    this.businessCloseExact = false; this.businessHandoffCloseOperation = '';
    value.writer.resumeWrites(); this.bindInputRules(); this.draftChanged(); this.refreshFieldCounts();
    setTimeout(() => { if (this.editorDraft === draft) { this.draftRestoreInput = false; } }, 16);
    this.message = this.draftConflict ? '当前完整草稿已恢复，卡片来源另有变化；输入保留，请查看详情核对。' :
      this.businessHandoff ? '已恢复当前完整接续草稿。原退役或关闭步骤需明确核对，读取未提交卡片。' : '已恢复当前完整草稿，可继续编辑；原已关闭保存请求保持只读。';
  }
  private businessHandoffHooks(binding: BusinessEditorBinding, raw: DraftValues, epoch: number,
    closing: boolean = false): EditorBusinessHandoffHooks {
    const parent = binding.parent;
    return { send: (wire: string): Promise<EditorBusinessHandoffReply> => workbench.send(wire) as Promise<EditorBusinessHandoffReply>,
      sendDraft: async (wire: string): Promise<DraftRecord> => {
        const reply = await workbench.send(wire);
        if (!reply.ok) { throw new DraftSaveFailure(reply.effect === 'not_committed', reply.error); }
        if (reply.drafts.length !== 1) { throw new Error('接续草稿回执缺失'); } return reply.drafts[0];
      }, changed: (): void => { this.draftChanged(); this.businessChanged(); }, operation: (): string => util.generateRandomUUID(true),
      isCurrent: (): boolean => this.businessCurrent(binding),
      isExact: (): boolean => !!parent && this.businessCurrent(binding) && !this.draftCaptureIncomplete && this.editorInputEpoch === epoch &&
        binding.liveDraft === parent && sameValues(raw, parent.current) &&
        (closing ? this.businessUnchanged(binding) : this.ownsEditorView(binding.owner)),
      importsReady: (): boolean => this.businessImportsReady(),
      childCurrent: (child: EditorDraftCoordinator): boolean => this.businessCurrent(binding) && this.editorDraft === child &&
        binding.liveDraft === child && !this.draftCaptureIncomplete && this.ownsEditorView(binding.owner) };
  }
  private async finishBusinessInput(session: EditorBusinessSessionCoordinator, binding?: BusinessEditorBinding): Promise<void> {
    if (!session.qualified || !session.confirmed?.historical_card) {
      this.message = session.committed ? '保存已提交，完整历史结果仍需核对；当前输入保留。' : '保存结果尚未确认，原请求与输入保留。'; return;
    }
    if (!binding || !this.businessCurrent(binding)) {
      this.message = '原卡片已确认保存。结果属于原会话，请从原请求入口恢复草稿后处理。'; return;
    }
    if (this.businessHandoff) { await this.continueBusinessHandoff(session, binding); return; }
    const parent = binding.parent;
    if (!parent || !this.businessParentMatches(binding)) { throw new Error('原父草稿未安装，请从当前接续草稿入口核对'); }
    parent.pauseWrites(); await parent.settleIssued();
    if (parent.unknown || parent.conflicted || this.draftCaptureIncomplete || !this.businessImportsReady()) {
      throw new Error('请先核对草稿原操作或完整输入，已确认的业务保存不会重发');
    }
    if (session.phase !== 'issued') {
      // Restart reads the immutable native plan. It must never mint another
      // child, retirement or close operation for this already planned intent.
      const handoff = await EditorBusinessHandoffCoordinator.restore(session, parent,
        this.businessHandoffHooks(binding, parent.current, this.editorInputEpoch, true));
      this.businessHandoff = handoff; this.businessCloseExact = !handoff.originalHandoff;
      this.businessHandoffCloseOperation = '';
      this.message = '已读取原草稿接续或关闭步骤，请明确核对原步骤。当前输入仍保留。';
      return;
    }
    if (this.businessExact(binding) && session.mayConsume(parent.current)) {
      const close = EditorBusinessHandoffCoordinator.savedExact(session, parent, util.generateRandomUUID(true),
        util.generateRandomUUID(true), this.businessHandoffHooks(binding, binding.values, binding.epoch, true));
      this.editorBoundary = 'business-saved';
      if (!(await this.revokeEditorView(parent))) { this.editorBoundary = ''; return; }
      if (!this.businessUnchanged(binding)) {
        this.editorBoundary = '';
        const mounted = await this.remountEditorView(parent);
        binding.identity = this.attachmentEditorIdentity; binding.owner = this.editorViewOwner;
        if (!mounted || this.draftCaptureIncomplete) { throw new Error('关闭边界已有新输入，编辑器保留，未派发清理'); }
        // The discarded local close controller never dispatched. The own
        // qualified receipt remains and the fresh view continues complete S2.
      } else {
        this.businessHandoff = close; this.businessCloseExact = true;
        await close.close(); await this.finishBusinessClose(session, binding); return;
      }
    }
    if (!this.businessCurrent(binding) || !this.ownsEditorView(binding.owner)) { throw new Error('接续界面归属已变化'); }
    const card = session.confirmed.historical_card, scope = new DraftScope();
    scope.card_id = card.id; scope.draft_id = util.generateRandomUUID(true); scope.source_kind = 0;
    scope.source_revision = card.revision; scope.source = card.source;
    const latest = parent.current, cutoff = this.editorInputEpoch;
    const handoff = await EditorBusinessHandoffCoordinator.prepare(session, parent, scope, latest,
      util.generateRandomUUID(true), util.generateRandomUUID(true), util.generateRandomUUID(true), this.businessHandoffHooks(binding, latest, cutoff));
    this.businessHandoff = handoff; this.businessCloseExact = false; this.businessHandoffCloseOperation = util.generateRandomUUID(true);
    await handoff.begin(); await this.continueBusinessHandoff(session, binding);
  }
  private async continueBusinessHandoff(session: EditorBusinessSessionCoordinator, binding: BusinessEditorBinding): Promise<void> {
    const handoff = this.businessHandoff;
    if (!handoff || !this.businessCurrent(binding)) { return; }
    if (this.businessCloseExact) { if (handoff.closed) { await this.finishBusinessClose(session, binding); } return; }
    if (handoff.pendingNative || handoff.pendingRead || handoff.pendingHistory || handoff.conflicted) { throw new Error('接续原步骤需显式核对'); }
    const parent = binding.parent;
    if (parent && binding.liveDraft === parent) {
      const latest = parent.current, epoch = this.editorInputEpoch;
      const child = handoff.openChild(latest, (): boolean => this.businessCurrent(binding) && this.ownsEditorView(binding.owner) &&
        this.editorInputEpoch === epoch && !this.draftCaptureIncomplete && sameValues(latest, parent.current));
      // One synchronous writer/lease switch. Complete callbacks afterwards can
      // only update this child; neither the first ACK nor card strings replace S3.
      binding.liveDraft = child; this.editorDraft = child; this.editorValues = child.current; this.draftSource = child.scope.source;
      this.selected = child.scope.card_id;
      this.ownedTodosBaseline = this.businessOwnsTodos(session) ? session : undefined;
      this.editorBusinessMode = 'current_v2'; this.attachmentEditingValues = '';
      this.draftConflict = false; this.dirty = true; this.bindInputRules(); this.draftChanged(); this.refreshFieldCounts();
    }
    const child = binding.liveDraft;
    await child.ensureConfirmed();
    if (!this.businessCurrent(binding) || !this.ownsEditorView(binding.owner) || this.draftCaptureIncomplete || child.unknown || child.dirty) {
      throw new Error('接续后的最新输入尚未确认');
    }
    await handoff.loadRetirement();
    if (!handoff.retired) {
      if (!parent && this.businessRecovery?.parentHistory?.current_active === false) { await handoff.readRetirementHistory(); }
      else { await handoff.retire(); }
    }
    if (!this.businessHandoffCloseOperation) { this.businessHandoffCloseOperation = util.generateRandomUUID(true); }
    if (!handoff.closed) { await handoff.closeRetired(this.businessHandoffCloseOperation); }
    parent?.dispose(); this.businessHandoff = undefined; this.businessHandoffCloseOperation = '';
    this.message = '卡片已保存，较新的完整输入已接续为草稿，可继续编辑和保存。';
    await this.refreshAfterBusiness();
  }
  private async finishBusinessClose(session: EditorBusinessSessionCoordinator, binding: BusinessEditorBinding): Promise<void> {
    if (!this.businessHandoff?.closed) { return; }
    if (!binding.parent) { throw new Error('准确关闭仅允许其真实原父草稿'); }
    if (this.businessUnchanged(binding) && this.editorViewRevoked && !this.editorViewVisible && this.editorBoundaryDraft === binding.parent) {
      binding.parent.dispose(); this.editorDraft = undefined; this.editorOpen = false; this.editorBoundary = '';
      this.dirty = false; this.taskText = ''; this.taskEditId = ''; this.taskRenameText = '';
      this.businessHandoff = undefined; this.businessCloseExact = false; this.message = '卡片已保存，原草稿已确认关闭。';
    } else {
      this.draftRetiredIdentity = binding.parent.scope.draft_id; this.draftConflict = true;
      this.message = '原草稿关闭已确认，但界面出现了较新的输入，完整输入保留，尚未确认接续。';
    }
    await this.refreshAfterBusiness();
  }
  private async refreshAfterBusiness(): Promise<void> {
    try {
      const result = await workbench.send(JSON.stringify(new Command()));
      if (!result.ok) { throw new Error(result.error); }
      this.cards = result.cards; this.workspaceConditionsChanged(); this.refreshPreview(); await this.loadDrafts();
    } catch (_) { this.message += ' 已确认的保存保持成立，列表刷新失败，请重新读取。'; }
    await this.loadBusinessIntents(); this.businessChanged();
  }
  private async reconcileBusinessHandoff(): Promise<void> {
    const session = this.businessSession, binding = this.businessBinding, handoff = this.businessHandoff;
    if (!session || !binding || !handoff || this.businessWorking || handoff.saving || this.busy) { return; }
    this.businessWorking = true;
    try {
      if (this.businessCloseExact && !handoff.closed) {
        if (!binding.parent) { throw new Error('原准确关闭缺少真实父草稿，未派发清理'); }
        // Retrying a recovered exact close may retire its original publication.
        // Detach the owned editor only while it still contains exactly S1.
        if (!this.businessUnchanged(binding)) { throw new Error('原关闭步骤已有较新输入，请保留完整草稿并核对归属'); }
        if (!this.editorViewRevoked) {
          this.editorBoundary = 'business-saved';
          if (!(await this.revokeEditorView(binding.parent))) { this.editorBoundary = ''; return; }
        }
        if (!this.businessUnchanged(binding)) { throw new Error('关闭边界已有新输入，未派发原关闭步骤'); }
      }
      if (handoff.pendingHistory) { await handoff.retryHistory(); }
      else if (handoff.pendingRead) { await handoff.retryRead(); }
      else if (handoff.pendingNative === 'handoff') { await handoff.retry(); }
      else if (handoff.pendingNative === 'retirement') { await handoff.retryRetire(); }
      else if (handoff.pendingNative === 'close') { await handoff.retryClose(); }
      await this.continueBusinessHandoff(session, binding);
    } catch (error) { this.message = '草稿接续或关闭尚未完成：' + String(error) + '。原请求与完整输入均保留。'; }
    finally { this.businessWorking = false; this.businessChanged(); }
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
    if (this.businessWorking || this.businessPending()) { return false; }
    return this.ready && !this.rawFork && !this.rawForkWorking && !this.editorBoundary && !this.editorViewRevoked && !this.attachmentWorking && !this.pasteWorking && !this.fieldValidationWorking && !this.busy && !this.draftWorking && !this.draftConflict && !this.draftCaptureIncomplete && !this.editorDraft?.unknown && this.pending.length === 0 &&
      !!this.current() && !this.current()?.deleted && !this.dirty && this.taskEditId.length === 0;
  }
  private renameTask(task: TaskView): void {
    if (!this.canEditTasks()) { return; }
    this.taskEditId = task.id; this.taskRenameText = task.text; this.taskRenameValue = new DraftTextValue();
    this.taskRenameOwner = util.generateRandomUUID(true);
    this.taskRenameValue.text = task.text; this.editorInputEpoch++; this.refreshFieldCount('taskRename');
    const revision = (this.inputRevisions.get('taskRename') ?? 0) + 1; this.inputRevisions.set('taskRename', revision);
    this.directInput.bind('taskRename', 'todos', this.directOwner('taskRename'), revision, copyText(this.taskRenameValue));
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
    if (this.businessWorking || this.businessPending()) { this.message = '请先核对原保存请求，完整编辑器仍保留。'; return; }
    if (this.attachmentWorking || this.pasteWorking) { this.message = '文件或粘贴操作尚未完成，请稍候。'; return; }
    if (this.busy || this.pending.length > 0 || this.draftWorking || this.draftRetirementUnknown || this.rawFork || this.rawForkWorking || this.editorBoundary) { this.message = '操作尚未确认，当前输入已保留。'; return; }
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
        .enabled(this.ready && !this.busy && this.pending.length === 0).onClick(() => { this.draftsOpen = true; this.loadBusinessIntents(); })
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
      MusicFooterContent({ view: this.musicView, callbacks: this.musicCallbacks, fontFamily: this.fontFamily,
        locale: this.locale, accent: this.accent(), muted: this.muted() })
        .width('100%').margin({ top: 8 }).borderRadius(this.round(14, 'footer'))
        .backgroundColor(this.material('footer') ? alpha(this.material('footer')?.color || this.surface(), (this.material('footer')?.opacity ?? 0) / 100) : Color.Transparent)
        .backdropBlur(this.material('footer')?.blur ?? 0).overlay(this.Rim('footer', 14))
        .onAreaChange((_, area: Area) => this.rememberSurface('footer', 14, area)).shadow(this.surfaceShadow('footer'))
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
    MusicPanelContent({ view: this.musicView, callbacks: this.musicCallbacks, fontFamily: this.fontFamily,
      locale: this.locale, ink: this.ink(), muted: this.muted(), accent: this.accent(), line: this.line(), dark: this.dark, surface: this.surface(),
      style: this.visualStyle, depth: this.depthFor('music'), radiusScale: this.round(20, 'music') / 20 })
      .padding(16).width('100%').borderRadius(this.round(22, 'music')).backdropBlur(this.materialBlur('music'))
      .linearGradient(this.gradient('music')).overlay(this.Rim('music', 22))
      .onAreaChange((_, area: Area) => this.rememberSurface('music', 22, area))
      .shadow(this.surfaceShadow('music')).border({ width: this.styleEdgeWidth(), color: this.line() })
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
  BodyInput(owner: string) {
    TextArea({ text: this.description, placeholder: this.t('写下思路，或粘贴一段内容…'), controller: this.descriptionController })
      .fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(13).placeholderFont({ size: 13 }).fontColor(this.ink()).placeholderColor(this.muted())
      .backgroundColor(Color.Transparent).border({ width: this.styleEdgeWidth(), color: this.line() }).borderRadius(this.round(12)).height(180)
      .enabled(!this.attachmentWorking && !this.pasteWorking && !this.draftRetiring && !this.draftRetirementUnknown).id('draft-description')
      .onChange((s: string, preview?: PreviewText) => this.leaseTextChanged(owner, 'description', s, preview))
      .onTextSelectionChange((start: number, end: number) => this.leaseSelectionChanged(owner, 'description', start, end))
      .onFocus(() => this.leaseFocusChanged(owner, 'description', true)).onBlur(() => this.leaseFocusChanged(owner, 'description', false))
    this.FieldCounter('description')
  }
  @Builder
  FieldCounter(name: string) {
    Text(this.fieldCountLabel(name)).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(10).fontColor(this.muted()).width('100%').textAlign(TextAlign.End).id('field-count:' + name)
  }
  @Builder
  BusinessNotice() {
    if (this.businessStatus.length > 0) {
      Column({ space: 8 }) {
        Text(this.businessStatus).fontSize(11).fontColor(this.muted()).width('100%')
        if (this.businessSession || this.businessHandoff) {
          Flex({ wrap: FlexWrap.Wrap, alignItems: ItemAlign.Center }) {
            if (!this.editorOpen) {
              Button('恢复原草稿', { type: ButtonType.Normal }).fontSize(11).fontColor(this.accent()).backgroundColor(Color.Transparent)
                .enabled(!this.businessWorking).onClick(() => this.resumeBusinessEditor()).id('business-recover-editor')
              if (this.businessSession?.phase !== 'prepared' && this.businessSession?.phase !== 'issued') {
                Button(this.businessRecovery?.pendingRead ? '核对原恢复读取' : '恢复当前接续草稿', { type: ButtonType.Normal }).fontSize(11).fontColor(this.accent()).backgroundColor(Color.Transparent)
                  .enabled(!this.businessWorking).onClick(() => this.recoverCurrentBusinessChild()).id('business-recover-current-child')
              }
            }
            if (this.businessHandoff) {
              Button('核对草稿接续或关闭', { type: ButtonType.Normal }).fontSize(11).fontColor(this.accent()).backgroundColor(Color.Transparent)
                .enabled(!this.businessWorking).onClick(() => this.reconcileBusinessHandoff()).id('business-handoff-reconcile')
            } else if (this.businessSession?.pendingNative || this.businessSession?.pendingRead ||
              this.businessSession?.phase === 'issued' && !this.businessSession?.transportReady) {
              Button('核对原步骤', { type: ButtonType.Normal }).fontSize(11).fontColor(this.accent()).backgroundColor(Color.Transparent)
                .enabled(!this.businessWorking).onClick(() => this.reconcileBusiness('native')).id('business-reconcile-native')
            } else if (this.businessSession?.phase === 'prepared') {
              Button('继续原请求', { type: ButtonType.Normal }).fontSize(11).fontColor(this.accent()).backgroundColor(Color.Transparent)
                .enabled(!this.businessWorking && this.editorOpen).onClick(() => this.reconcileBusiness('issue')).id('business-issue')
              Button('取消尚未签发的请求', { type: ButtonType.Normal }).fontSize(11).fontColor(this.muted()).backgroundColor(Color.Transparent)
                .enabled(!this.businessWorking).onClick(() => this.reconcileBusiness('cancel')).id('business-cancel-prepared')
            } else {
              Button('核对保存结果', { type: ButtonType.Normal }).fontSize(11).fontColor(this.accent()).backgroundColor(Color.Transparent)
                .enabled(!this.businessWorking).onClick(() => this.reconcileBusiness('inspect')).id('business-inspect')
              if (!this.businessSession?.committed) {
                Button('继续原保存请求', { type: ButtonType.Normal }).fontSize(11).fontColor(this.accent()).backgroundColor(Color.Transparent)
                  .enabled(!this.businessWorking).onClick(() => this.reconcileBusiness('save')).id('business-retry-save')
              }
            }
          }.width('100%')
        }
      }.width('100%').padding(10).backgroundColor('#0B7662BA').borderRadius(this.round(10)).id('business-request-notice')
    }
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
          this.BusinessNotice()
          if (this.businessListError.length > 0) { Text(this.businessListError).fontSize(11).fontColor(this.muted()) }
          if (this.businessIntents.length > 0) {
            Text('原保存请求').fontSize(14).fontColor(this.ink()).width('100%')
            ForEach(this.businessIntents, (intent: EditorIntentView) => {
              Row({ space: 8 }) {
                Column({ space: 4 }) {
                  Text(this.cards.find((card: CardView): boolean => card.id === intent.card_id)?.title || '未完成的想法')
                    .fontSize(12).fontColor(this.ink()).maxLines(2)
                  Text(intent.phase === 'closed' ? '已结束，可查看原记录' : '请求已保留，点击读取不会提交')
                    .fontSize(10).fontColor(this.muted())
                }.alignItems(HorizontalAlign.Start).layoutWeight(1)
                Button('读取原请求', { type: ButtonType.Normal }).fontSize(11).fontColor(this.accent()).backgroundColor(Color.Transparent)
                  .enabled(!this.businessWorking && !this.businessPending()).onClick(() => this.restoreBusinessIntent(intent))
                  .id('business-intent-read:' + intent.proof.intent_id)
              }.padding(12).width('100%').backgroundColor('#0B7662BA').borderRadius(this.round(12))
            }, (intent: EditorIntentView): string => intent.proof.intent_id + ':' + intent.current_generation)
            if (this.businessNext.length > 0) {
              Button('读取更多原请求', { type: ButtonType.Normal }).fontSize(11).fontColor(this.accent()).backgroundColor(Color.Transparent)
                .enabled(!this.businessWorking).onClick(() => this.loadBusinessIntents(true)).id('business-intents-more')
            }
          }
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
  Editor(owner: string) {
    Column() {
      Row({ space: 12 }) {
        Row() { this.Glyph(() => 0xeea9,  21, () => this.accent()) }.width(38).height(38).justifyContent(FlexAlign.Center)
          .backgroundColor('#267662BA').borderRadius(this.round(13))
        Column({ space: 5 }) {
          Text(this.t('MORROW / A LITTLE POSSIBILITY')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(8).letterSpacing(1).fontColor(this.muted())
          Text(this.selected.length > 0 ? '让想法更清晰' : '接住一个新想法').fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(20).fontWeight(FontWeight.Medium).fontColor(this.ink())
        }.alignItems(HorizontalAlign.Start).layoutWeight(1)
        Button({ type: ButtonType.Normal }) { this.Glyph(() => 0xf647,  18, () => this.muted()) }
          .width(32).height(36).backgroundColor(Color.Transparent).accessibilityText('关闭编辑器')
          .enabled(!this.pasteWorking && !this.attachmentWorking).onClick(() => this.closeEditor())
      }.width('100%').padding({ left: 24, right: 24, top: 24 })
      Text(this.t('文字、表格、图片，先放在这里。让一个念头慢慢成形。')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(11).lineHeight(18.7).fontColor(this.muted())
        .width('100%').padding({ left: 24, right: 24 }).margin({ top: 12, bottom: 22 })
      Scroll(this.editorScroller) {
        Column({ space: 16 }) {
          TextInput({ text: this.title, placeholder: this.t('给它起个名字'), controller: this.titleController }).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(23).placeholderFont({ size: 23 }).fontWeight(FontWeight.Medium)
            .fontColor(this.ink()).placeholderColor(this.muted()).backgroundColor(Color.Transparent).padding(16).height(64)
            .border({ width: 1, color: this.accent() }).borderRadius(this.round(12))
            .enabled(!this.attachmentWorking && !this.pasteWorking && !this.draftRetiring && !this.draftRetirementUnknown).id('draft-title')
            .onChange((s: string, preview?: PreviewText) => this.leaseTextChanged(owner, 'title', s, preview))
            .onTextSelectionChange((start: number, end: number) => this.leaseSelectionChanged(owner, 'title', start, end))
            .onFocus(() => this.leaseFocusChanged(owner, 'title', true)).onBlur(() => this.leaseFocusChanged(owner, 'title', false))
          this.FieldCounter('title')
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
                .width(32).height(32).backgroundColor(Color.Transparent).accessibilityText(this.previewBody ? '隐藏预览' : '实时预览')
                .enabled(!this.pasteWorking && !this.attachmentWorking).onClick(() => { this.previewBody = !this.previewBody; })
            }.width('100%').height(32)
            Divider().color(this.line())
            if (this.previewBody && Math.min(820, this.viewportWidth - 40) - 80 >= 590) {
              Row({ space: 24 }) {
                Column() { this.BodyInput(owner) }.layoutWeight(1)
                Column() { this.BodyPreview() }.layoutWeight(1)
              }.width('100%').alignItems(VerticalAlign.Top).id('draft-body-split')
            } else if (this.previewBody) { this.BodyPreview() }
            else { this.BodyInput(owner) }
          }.padding(16).width('100%').backgroundColor('#077662BA').borderRadius(this.round(16)).border({ width: this.styleEdgeWidth(), color: this.line() })
          if (this.editorDraft?.scope.source_kind === 1) {
          Row({ space: 12 }) {
            Text(this.t('放在哪里')).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(11).fontColor(this.muted())
            Select([{ value: '灵感' }, { value: '进行中' }, { value: '实验' }]).value(this.current()?.category ?? this.category)
              .font({ size: 12 }).fontColor(this.ink()).backgroundColor('#147662BA').borderRadius(this.round(10))
              .enabled(this.editorMetadataReady(owner) && !this.busy && this.pending.length === 0)
              .onSelect((_: number, value: string) => {
                if (!this.editorMetadataReady(owner) || !['灵感', '进行中', '实验'].includes(value)) { return; }
                this.editorInputEpoch++; this.category = value; this.dirty = true; this.editorValues.category = value;
                this.editorValues.stage = value === '进行中' ? '计划中' : value === '实验' ? '待验证' : '待整理';
                this.editorDraft?.update(copyValues(this.editorValues));
              })
            Blank()
          }.width('100%')
          }
          if (this.category === '实验') {
            TextArea({ text: this.hypothesis, placeholder: this.t('假设'), controller: this.hypothesisController }).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(12).placeholderFont({ size: 12 }).fontColor(this.ink()).backgroundColor(this.glass()).height(96)
              .enabled(!this.attachmentWorking && !this.pasteWorking && !this.draftRetiring && !this.draftRetirementUnknown).id('draft-hypothesis')
              .onChange((s: string, preview?: PreviewText) => this.leaseTextChanged(owner, 'hypothesis', s, preview))
              .onTextSelectionChange((start: number, end: number) => this.leaseSelectionChanged(owner, 'hypothesis', start, end)).onFocus(() => this.leaseFocusChanged(owner, 'hypothesis', true)).onBlur(() => this.leaseFocusChanged(owner, 'hypothesis', false))
            this.FieldCounter('hypothesis')
            TextArea({ text: this.conclusion, placeholder: this.t('结论'), controller: this.conclusionController }).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(12).placeholderFont({ size: 12 }).fontColor(this.ink()).backgroundColor(this.glass()).height(116)
              .enabled(!this.attachmentWorking && !this.pasteWorking && !this.draftRetiring && !this.draftRetirementUnknown).id('draft-conclusion')
              .onChange((s: string, preview?: PreviewText) => this.leaseTextChanged(owner, 'conclusion', s, preview))
              .onTextSelectionChange((start: number, end: number) => this.leaseSelectionChanged(owner, 'conclusion', start, end)).onFocus(() => this.leaseFocusChanged(owner, 'conclusion', true)).onBlur(() => this.leaseFocusChanged(owner, 'conclusion', false))
            this.FieldCounter('conclusion')
          }
          if (this.newCardTodos()) {
            EditorTodos({ ownerKey: owner, revision: this.todoInputRevision, source: this.todoInputValue, focusIntent: this.editorFocusIntent,
              editingEnabled: !this.attachmentWorking && !this.pasteWorking && !this.draftRestoreInput && !this.draftRetiring && !this.draftRetirementUnknown,
              title: this.t('待办事项'), ink: this.ink(), muted: this.muted(), accent: this.accent(), surface: this.glass(),
              line: this.line(), radius: this.round(12), family: this.fontFamily || 'HarmonyOS Sans', locale: this.locale, controlInk: this.muted(),
              onCapture: (value: DraftTextValue, owner: string): number | undefined => this.ownsEditorView(owner) ? this.captureTodoRows(value, owner) : undefined,
              isCurrent: (owner: string, revision: number, value: DraftTextValue): boolean => this.ownsEditorView(owner) && this.todoRowsCurrent(owner, revision, value),
              count: async (text: string, owned: () => boolean): Promise<number> => {
                const count = await this.fieldPolicy.checkField('todos', text, owned);
                if (count.grapheme_count < 0) { throw new Error(count.error); } return count.grapheme_count;
              },
              format: (oldValue: DraftTextValue, newValue: DraftTextValue, remaining: number, owned: () => boolean) => this.inputPolicy.format('todos', oldValue, newValue, owned, remaining),
              onStatus: (status: EditorTodosStatus): void => { if (this.ownsEditorView(status.owner)) { this.todoRowsStatus(status); } },
              onUncaptured: (owner: string, _row: string, event: string): void => {
                this.leaseUncaptured(owner, 'todos', event);
              },
              onRowFocused: (owner: string, _row: string): void => {
                if (this.ownsEditorView(owner)) { this.editorFocusIntent++; this.lastDraftField = 'todos'; this.draftFocusedFields.clear(); this.draftFocusedFields.add('todos'); }
              },
              onRevealRow: (owner: string, _row: string, element: string): void => { if (this.ownsEditorView(owner)) { this.revealTodoRow(owner, element); } },
              onDragPosition: (owner: string, y: number, active: boolean): void => { if (this.ownsEditorView(owner)) { this.todoDragPosition(owner, y, active); } }
            })
          } else if (this.current()) {
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
                ForEach([this.taskRenameOwner], (renameOwner: string) => {
                Column({ space: 8 }) {
                  TextInput({ text: this.taskRenameText, placeholder: this.t('重命名'), controller: this.taskRenameController }).fontFamily(this.fontFamily || 'HarmonyOS Sans')
                    .fontSize(12).fontColor(this.ink()).backgroundColor(this.glass())
                    .enabled(!this.pasteWorking && !this.attachmentWorking && !this.busy && this.pending.length === 0).onChange((s: string, preview?: PreviewText) => this.leaseTaskRenameChanged(owner, task.id, renameOwner, s, preview))
                    .onFocus(() => this.leaseTaskRenameFocus(owner, task.id, renameOwner))
                  this.FieldCounter('taskRename')
                  Row({ space: 8 }) {
                    Blank()
                    Button(this.t('取消')).fontSize(12).fontColor(this.muted()).backgroundColor(Color.Transparent)
                      .enabled(!this.pasteWorking && !this.attachmentWorking && !this.busy && this.pending.length === 0).onClick(() => { this.taskEditId = ''; this.taskRenameText = ''; })
                    Button(this.t('保存')).fontSize(12).backgroundColor(this.accent())
                      .enabled(!this.pasteWorking && !this.attachmentWorking && !this.fieldValidationWorking && !this.busy && this.pending.length === 0 && !this.dirty && this.taskRenameText.trim().length > 0 && this.inputReadyFor('task_rename'))
                      .onClick(() => this.mutate('task_rename', task))
                  }.width('100%')
                }.width('100%').padding({ left: 40, bottom: 8 })
                }, (renameOwner: string): string => renameOwner)
              }
            }, (task: TaskView) => task.id + ':' + task.completion.toString() + ':' + task.text)
            if (!this.current()?.deleted) {
              Row({ space: 8 }) {
                TextInput({ text: this.taskText, placeholder: this.t('添加一个小步骤…'), controller: this.taskController }).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(12).placeholderFont({ size: 12 }).fontColor(this.ink()).backgroundColor(this.glass())
                  .enabled(!this.attachmentWorking && !this.pasteWorking && !this.draftRetiring && !this.draftRetirementUnknown).id('draft-todos')
              .onChange((s: string, preview?: PreviewText) => this.leaseTextChanged(owner, 'todos', s, preview))
                  .onTextSelectionChange((start: number, end: number) => this.leaseSelectionChanged(owner, 'todos', start, end)).onFocus(() => this.leaseFocusChanged(owner, 'todos', true)).onBlur(() => this.leaseFocusChanged(owner, 'todos', false)).layoutWeight(1)
                Button(this.t('+'), { type: ButtonType.Normal }).fontSize(21).fontColor(this.accent()).width(36).height(36).padding(0).backgroundColor('#147662BA').borderRadius(10)
                  .onClick(() => this.mutate('task_add')).enabled(this.canEditTasks())
              }
              this.FieldCounter('todos')
              if ((this.current()?.tasks.length ?? 0) > 0) {
                Button(this.t('全部勾选并设置阶段'), { type: ButtonType.Normal }).fontSize(11).fontColor(this.accent())
                  .backgroundColor(Color.Transparent).enabled(this.canEditTasks()).onClick(() => this.confirmTaskChange())
              }
            }
            Row({ space: 12 }) {
              Button(this.current()?.favorite ? '取消收藏' : '收藏', { type: ButtonType.Normal }).fontSize(11).fontColor(this.accent()).backgroundColor(Color.Transparent)
                .onClick(() => this.mutate('favorite')).enabled(!this.pasteWorking && !this.attachmentWorking && !this.busy && this.pending.length === 0 && !this.current()?.deleted)
              Blank()
              Button(this.current()?.deleted ? '撤销删除（8秒内）' : '移入回收站', { type: ButtonType.Normal }).fontSize(11).fontColor(this.muted()).backgroundColor(Color.Transparent)
                .onClick(() => this.mutate(this.current()?.deleted ? 'restore' : 'delete')).enabled(!this.pasteWorking && !this.attachmentWorking && !this.busy && this.pending.length === 0)
            }.width('100%')
          }
        }.padding({ left: 24, right: 24, bottom: 16 }).width('100%')
      }.layoutWeight(1).width('100%').scrollBar(BarState.Off).align(Alignment.TopStart).id('editor-scroll')
      if (this.inputRulesMessage.length > 0) {
        Row() {
          Text(this.inputRulesMessage).fontSize(11).fontColor(this.muted()).layoutWeight(1).id('editor-input-rule-status')
          Button('重新检查', { type: ButtonType.Normal }).fontSize(11).backgroundColor(Color.Transparent).fontColor(this.accent())
            .enabled(!this.busy && !this.draftRetiring && !this.draftWorking).onClick(() => this.retryInputRules())
        }.width('100%').padding({ left: 24, right: 24, bottom: 8 })
      }
      if (this.message.length > 0) { Text(this.message).fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(11).fontColor(this.accent()).padding({ left: 24, right: 24, bottom: 12 }).width('100%') }
      if (this.draftStatus.length > 0) {
        Row() {
          Text(this.draftStatus).fontSize(10).fontColor(this.muted()).layoutWeight(1).id('draft-status')
          if (this.editorDraft?.unknown) { Button('核对草稿', { type: ButtonType.Normal }).fontSize(11).fontColor(this.accent()).backgroundColor(Color.Transparent).enabled(!this.pasteWorking && !this.attachmentWorking && !this.draftWorking).onClick(() => this.retryDraft()) }
        }.width('100%').padding({ left: 24, right: 24, bottom: 8 })
      }
      if (this.draftRetirementUnknown) {
        Button('核对草稿清理', { type: ButtonType.Normal }).fontSize(11).fontColor(this.accent()).backgroundColor(Color.Transparent)
          .enabled(!this.pasteWorking && !this.attachmentWorking && !this.draftWorking && !this.busy).onClick(() => this.retryDraftRetirement())
      }
      if (this.rawForkStatus) {
        Row() {
          Text(this.rawForkStatus).fontSize(10).fontColor(this.muted()).layoutWeight(1)
          if (this.rawForkAction && this.rawForkAction !== 'conflict') {
            Button('核对接续', { type: ButtonType.Normal }).fontSize(11).fontColor(this.accent()).backgroundColor(Color.Transparent)
              .enabled(!this.rawForkWorking && !this.busy).onClick(() => this.rawFork ? this.retryRawFork() : this.beginRawFork())
          }
        }.width('100%').padding({ left: 24, right: 24, bottom: 8 })
      }
      this.BusinessNotice()
      Divider().color(this.line())
      Row({ space: 12 }) {
        Button('放弃草稿', { type: ButtonType.Normal }).fontSize(10).fontColor(this.muted()).backgroundColor(Color.Transparent)
          .enabled(!this.businessWorking && !this.businessPending() && !this.pasteWorking && !this.attachmentWorking && !this.busy && !this.draftWorking && !this.draftRetirementUnknown && !this.editorDraft?.unknown && this.pending.length === 0).onClick(() => this.discardDraft())
        Blank()
        Button('保留草稿', { type: ButtonType.Normal }).fontSize(12).height(38).fontColor(this.accent()).backgroundColor(Color.Transparent)
          .enabled(!this.pasteWorking && !this.attachmentWorking).onClick(() => this.closeEditor())
        Button(this.busy ? '保存中…' : '保存灵感', { type: ButtonType.Normal }).fontSize(12).height(38).padding({ left: 22, right: 22 })
          .fontColor(this.dark ? '#302D43' : '#FFFFFF').backgroundColor(this.accent()).borderRadius(this.round(10))
          .enabled(this.ready && !this.businessWorking && !this.businessPending() && !this.pasteWorking && !this.attachmentWorking && !this.fieldValidationWorking && !this.busy && !this.draftWorking && !this.draftConflict && !this.draftCaptureIncomplete && !this.draftRetirementUnknown && !this.editorDraft?.unknown && this.pending.length === 0 && !this.current()?.deleted && this.inputReadyFor(this.selected ? 'edit' : 'create')).onClick(() => this.save())
      }.padding({ left: 24, right: 24, top: 14, bottom: 14 }).width('100%')
      if (this.pending.length > 0 && !this.busy) {
        Button(this.t('使用原操作重试 / 核对')).fontSize(12).backgroundColor(this.accent()).margin({ bottom: 12 })
          .enabled(!this.pasteWorking && !this.attachmentWorking).onClick(() => this.retry())
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
          if (this.editorViewVisible) {
            ForEach([this.editorViewOwner], (owner: string) => {
              EditorViewLease({ owner: owner, content: (lease: string): void => { this.Editor(lease); },
                onMounted: (lease: string): void => this.editorLeaseMounted(lease),
                onRevoked: (lease: string): void => this.editorLeaseRevoked(lease) })
            }, (owner: string): string => owner)
          } else {
            Column({ space: 12 }) {
              Text(this.draftRetirementUnknown ? '草稿清理尚未确认，原操作已保留。' : '正在确认编辑器关闭边界…').fontColor(this.ink())
              if (this.draftRetirementUnknown) {
                Button('核对草稿清理').enabled(!this.draftWorking && !this.busy).onClick(() => this.retryDraftRetirement())
              }
              if (this.businessHandoff) {
                Text(this.businessStatus).fontSize(12).fontColor(this.muted())
                Button('核对原保存后关闭').enabled(!this.businessWorking && !this.busy)
                  .onClick(() => this.reconcileBusinessHandoff()).id('business-detached-reconcile')
              }
            }.padding(24).backgroundColor(this.surface()).borderRadius(this.round(20))
          }
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
                onToggle: (token: string): void => { this.toggleAttachmentMedia(token); },
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
      if (this.musicView.lyricsOpen) {
        Stack() {
          Rect().width('100%').height('100%').fill('#80302D43').onClick(() => this.musicAction('lyrics_close'))
          LyricsDialogContent({ view: this.musicView, callbacks: this.musicCallbacks, fontFamily: this.fontFamily,
            locale: this.locale, ink: this.ink(), muted: this.muted(), accent: this.accent() })
            .width(Math.max(180, Math.min(660, this.viewportWidth - 24)))
            .height(Math.max(160, Math.min(760, this.viewportHeight - 40)))
            .backgroundColor(this.surface()).borderRadius(this.round(18)).clip(true)
        }.width('100%').height('100%').id('music-lyrics-dialog')
      }
    }.width('100%').height('100%').backgroundColor(this.canvasBackgroundColorValue())
      .onAreaChange((_, area: Area) => { this.viewportWidth = Number(area.width); this.viewportHeight = Number(area.height); })
  }
}
