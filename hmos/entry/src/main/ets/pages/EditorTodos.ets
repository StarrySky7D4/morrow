import { TextValue, copyText } from '../model/EditorDraft';
import { samePasteTarget } from '../model/EditorPaste';
import { EditorTodosDraft, EditorTodoRow, EditorTodoTicket, EditorTodoDrag, EditorTodosView, EditorTodosStatus, EditorTodoFormatResult } from '../model/EditorTodos';
import { uiText } from '../model/UiStrings';

class TodoRowArea {
  incarnation: number = 0;
  top: number = 0;
  height: number = 0;
}
interface TodoUncapturedInput { kind: string; text: string; preview?: PreviewText; }
interface TodoUncapturedSelection { kind: string; base: number; extent: number; }
interface TodoInitialDisplayEcho {
  controller: TextAreaController;
  ticket: EditorTodoTicket;
  value: TextValue;
  display: string;
  control: string;
  pending: boolean;
}
function todoInputEvent(text: string, preview?: PreviewText): string {
  const event: TodoUncapturedInput = { kind: 'input', text, preview }; return JSON.stringify(event);
}
function todoSelectionEvent(base: number, extent: number): string {
  const event: TodoUncapturedSelection = { kind: 'selection', base, extent }; return JSON.stringify(event);
}

// This component edits only the raw newline field. Its parent owns the journal,
// editorInputPolicy workers and business save. Row IDs are local UI identities.
@Component
export struct EditorTodos {
  @Prop @Watch('sourceChanged') ownerKey: string = '';
  @Prop @Watch('sourceChanged') revision: number = 0;
  @Prop @Watch('sourceChanged') source: TextValue = new TextValue();
  @Prop @Watch('availabilityChanged') editingEnabled: boolean = true;
  @Prop @Watch('focusIntentChanged') focusIntent: number = 0;
  @Prop title: string = '待办';
  @Prop ink: string = '#302D43';
  @Prop muted: string = '#81798F';
  // Flutter's ordinary IconButtons use onSurfaceVariant, while the handle uses
  // palette.muted. Parent can supply the actual theme control color explicitly.
  @Prop controlInk: string = '';
  @Prop accent: string = '#7662BA';
  @Prop surface: string = '#FFFFFF';
  @Prop line: string = '#DDD8E8';
  @Prop radius: number = 18;
  @Prop family: string = 'HarmonyOS Sans';
  @Prop locale: string = 'zh';
  onCapture: (value: TextValue, owner: string) => number | undefined = (): undefined => undefined;
  isCurrent: (owner: string, revision: number, value: TextValue) => boolean = (): boolean => false;
  count: (text: string, owned: () => boolean) => Promise<number> = async (): Promise<number> => { throw new Error('待办计数未连接。'); };
  format: (oldValue: TextValue, newValue: TextValue, remaining: number, owned: () => boolean) => Promise<EditorTodoFormatResult> =
    async (): Promise<EditorTodoFormatResult> => { throw new Error('待办行检查未连接。'); };
  onStatus: (status: EditorTodosStatus) => void = (): void => {};
  onUncaptured: (owner: string, row: string, event: string) => void = (): void => {};
  onRowFocused: (owner: string, row: string) => void = (): void => {};
  onRevealRow: (owner: string, row: string, element: string) => void = (): void => {};
  onDragPosition: (owner: string, windowY: number, active: boolean) => void = (): void => {};
  @State private state: EditorTodosView = new EditorTodosView();
  @State private renderRevision: number = 0;
  private alive: boolean = false;
  private serial: number = 0;
  private drag: EditorTodoDrag | undefined = undefined;
  private areas: Map<string, TodoRowArea> = new Map<string, TodoRowArea>();
  private focusTicket: EditorTodoTicket | undefined = undefined;
  private focusTimer: number = -1;
  private focusAttempt: number = 0;
  private focusRevealed: boolean = false;
  private focusIntentEpoch: number = 0;
  private focusRequestIntent: number = 0;
  private model: EditorTodosDraft = new EditorTodosDraft({
    capture: (value: TextValue, owner: string): number | undefined => this.alive && this.editingEnabled && owner === this.ownerKey ? this.onCapture(value, owner) : undefined,
    isCurrent: (owner: string, revision: number, value: TextValue): boolean => this.alive && this.editingEnabled && owner === this.ownerKey && this.isCurrent(owner, revision, value),
    count: (text: string, owned: () => boolean): Promise<number> => this.count(text, owned),
    format: (oldValue: TextValue, newValue: TextValue, remaining: number, owned: () => boolean): Promise<EditorTodoFormatResult> => this.format(oldValue, newValue, remaining, owned),
    changed: (): void => this.syncView(),
    failed: (_owner: string, _message: string): void => {},
    id: (): string => 'todo_' + (++this.serial)
  });

  aboutToAppear(): void { this.alive = true; this.sourceChanged(); }
  aboutToDisappear(): void { this.focusIntentChanged(); this.finishDrag(); this.alive = false; this.areas.clear(); this.model.stop(); }
  private t(value: string): string { return uiText(value, this.locale); }
  private sourceChanged(): void {
    if (!this.alive || !this.ownerKey) { return; }
    const value: TextValue = copyText(this.source);
    // Prop watchers may deliver revision and source separately. Only the full
    // parent-owned snapshot can replace rows or cancel an in-flight formatter.
    if (!this.isCurrent(this.ownerKey, this.revision, value)) { return; }
    if (this.state.owner !== this.ownerKey) { this.focusIntentChanged(); this.finishDrag(); this.areas.clear(); }
    try { this.model.bind(this.ownerKey, this.revision, value); if (!this.editingEnabled) { this.model.stop(); } }
    catch (error) {
      // Do not borrow a previous owner's status when this source cannot bind.
      const status = new EditorTodosStatus(); status.owner = this.ownerKey; status.revision = this.revision;
      status.value = copyText(this.source); status.raw_capture_complete = false; status.business_ready = false;
      status.capture_error = String(error); this.onStatus(status);
    }
  }
  private availabilityChanged(): void { if (this.editingEnabled) { this.sourceChanged(); } else { this.focusIntentChanged(); this.finishDrag(); this.model.stop(); } }
  private focusIntentChanged(): void { this.focusIntentEpoch++; this.cancelFocus(); }
  private syncView(): void {
    if (!this.alive) { return; }
    this.state = this.model.view(); this.renderRevision++;
    if (this.state.owner === this.ownerKey) {
      this.onStatus(this.model.status());
    }
  }
  private ticket(row: EditorTodoRow): EditorTodoTicket {
    const result = new EditorTodoTicket(); result.owner = this.state.owner; result.id = row.id; result.incarnation = row.incarnation; return result;
  }
  private ownsIdentity(ticket: EditorTodoTicket): boolean {
    const row = this.model.row(ticket.id);
    return this.alive && ticket.owner === this.ownerKey && ticket.owner === this.state.owner && !!row && row.incarnation === ticket.incarnation;
  }
  private owns(ticket: EditorTodoTicket): boolean {
    return this.editingEnabled && this.ownsIdentity(ticket) && this.isCurrent(this.state.owner, this.state.revision, copyText(this.state.value));
  }
  private uncapturedFor(ticket: EditorTodoTicket, event: string): void {
    if (this.ownsIdentity(ticket)) { this.onUncaptured(ticket.owner, ticket.id, event); }
  }
  private element(row: string, owner: string = this.state.owner): string { return 'editor-todo-' + encodeURIComponent(owner) + '-' + row; }
  private async inputFor(ticket: EditorTodoTicket, text: string, preview?: PreviewText): Promise<void> {
    if (!this.editingEnabled) { this.uncapturedFor(ticket, todoInputEvent(text, preview)); return; }
    if (!this.owns(ticket)) { return; }
    await this.model.input(ticket, text, preview?.value || '', preview?.offset ?? -1);
  }
  private async selectionFor(ticket: EditorTodoTicket, base: number, extent: number): Promise<void> {
    if (!this.editingEnabled) { this.uncapturedFor(ticket, todoSelectionEvent(base, extent)); return; }
    if (this.owns(ticket)) { await this.model.selection(ticket, base, extent); }
  }
  private async retryFor(ticket: EditorTodoTicket): Promise<void> {
    if (this.owns(ticket) && this.model.canRetry(ticket)) { await this.model.retryFormat(ticket); }
  }
  private focusFor(ticket: EditorTodoTicket): void {
    if (!this.owns(ticket)) { return; }
    this.focusIntentChanged();
    this.model.focus(ticket); this.onRowFocused(ticket.owner, ticket.id);
  }
  private async addRow(): Promise<void> {
    const owner: string = this.state.owner;
    this.focusIntentChanged();
    const epoch: number = this.focusIntentEpoch, intent: number = this.focusIntent;
    try {
      const id: string | undefined = await this.model.add();
      if (!id || !this.alive || owner !== this.ownerKey || epoch !== this.focusIntentEpoch || intent !== this.focusIntent) { return; }
      const ticket = this.model.ticket(id);
      // A completed model Add is not proof that ArkUI has mounted its field.
      // Wait for that row's measured area, reveal it, then focus in a later frame.
      this.cancelFocus(); this.focusTicket = ticket; this.focusRequestIntent = intent; this.focusAttempt = 0; this.queueFocus();
    } catch (error) {
      // A failed nonmutating Add count is not a missing raw capture or a
      // failed formatter for the current text. Parent still validates save.
      if (this.alive && owner === this.ownerKey) { this.state.error = String(error); }
    }
  }
  private cancelFocus(): void {
    if (this.focusTimer !== -1) { clearTimeout(this.focusTimer); }
    this.focusTimer = -1; this.focusTicket = undefined; this.focusAttempt = 0; this.focusRevealed = false;
  }
  private queueFocus(): void {
    if (!this.focusTicket || this.focusTimer !== -1) { return; }
    this.focusTimer = setTimeout((): void => { this.focusTimer = -1; this.focusAddedRow(); }, 32);
  }
  private focusAddedRow(): void {
    const ticket = this.focusTicket;
    if (!ticket || !this.owns(ticket) || this.focusRequestIntent !== this.focusIntent) { this.cancelFocus(); return; }
    if (++this.focusAttempt > 12) { this.cancelFocus(); return; }
    const area = this.areas.get(ticket.id);
    if (!area || area.incarnation !== ticket.incarnation) { this.queueFocus(); return; }
    const element = this.element(ticket.id, ticket.owner);
    if (!this.focusRevealed || this.focusAttempt % 3 === 0) {
      this.focusRevealed = true; this.onRevealRow(ticket.owner, ticket.id, element); this.queueFocus(); return;
    }
    try {
      this.getUIContext().getFocusController().requestFocus(element); this.cancelFocus();
    } catch (_) {
      // API26 throws 150003 for a not-yet-mounted, clipped or disabled field.
      // It is a presentation delay: do not lose the row or terminate the app.
      this.queueFocus();
    }
  }
  private removeFor(ticket: EditorTodoTicket): void { if (this.owns(ticket)) { this.model.remove(ticket); } }
  private moveFor(ticket: EditorTodoTicket, delta: number): void { if (this.owns(ticket)) { this.model.move(ticket, delta); } }
  private startDrag(ticket: EditorTodoTicket): DragItemInfo {
    this.drag = this.owns(ticket) ? this.model.beginDrag(ticket) : undefined;
    // No external payload is admitted on drop; the private owner ticket is the authority.
    return { extraInfo: '' };
  }
  private finishDrag(): void {
    const drag = this.drag; this.drag = undefined;
    if (drag) { this.onDragPosition(drag.owner, 0, false); }
  }
  private dragMoved(event: DragEvent): void {
    const drag = this.drag, y: number = event.getWindowY();
    if (!drag) { return; }
    if (!this.alive || !this.editingEnabled || drag.owner !== this.ownerKey || !this.model.ownsDrag(drag)) { this.finishDrag(); return; }
    if (Number.isFinite(y)) { this.onDragPosition(drag.owner, y, true); }
  }
  private areaFor(ticket: EditorTodoTicket, area: Area): void {
    if (!this.owns(ticket)) { return; }
    if (typeof area.globalPosition.y !== 'number' || typeof area.height !== 'number' ||
      !Number.isFinite(area.globalPosition.y) || !Number.isFinite(area.height) || area.height <= 0) { this.areas.delete(ticket.id); return; }
    const result = new TodoRowArea(); result.incarnation = ticket.incarnation; result.top = area.globalPosition.y; result.height = area.height;
    this.areas.set(ticket.id, result);
  }
  private dropFor(ticket: EditorTodoTicket, event: DragEvent): void {
    const drag = this.drag; this.finishDrag();
    const area = this.areas.get(ticket.id), y: number = event.getWindowY();
    if (!drag || !this.owns(ticket) || !area || area.incarnation !== ticket.incarnation || !Number.isFinite(y)) { return; }
    this.model.drop(drag, ticket, y >= area.top + area.height / 2);
  }
  private actionPath(action: string): string {
    if (action === 'up') { return 'M4.6 11.3Q4 12 4.7 12.7Q5.4 13.4 6.1 12.7L11 7.8V20H13V7.8L17.9 12.7Q18.6 13.4 19.3 12.7Q20 12 19.4 11.3L12 4Z'; }
    if (action === 'down') { return 'M4.6 12.7Q4 12 4.7 11.3Q5.4 10.6 6.1 11.3L11 16.2V4H13V16.2L17.9 11.3Q18.6 10.6 19.3 11.3Q20 12 19.4 12.7L12 20Z'; }
    return 'M5.3 5.3Q6 4.6 6.7 5.3L12 10.6L17.3 5.3Q18 4.6 18.7 5.3Q19.4 6 18.7 6.7L13.4 12L18.7 17.3Q19.4 18 18.7 18.7Q18 19.4 17.3 18.7L12 13.4L6.7 18.7Q6 19.4 5.3 18.7Q4.6 18 5.3 17.3L10.6 12L5.3 6.7Q4.6 6 5.3 5.3Z';
  }

  @Builder
  private action(icon: string, description: string, allowed: boolean, run: () => void) {
    Button({ type: ButtonType.Normal }) {
      Shape() { Path().commands(this.actionPath(icon)).fill(this.controlInk || this.muted) }
        .viewPort({ x: 0, y: 0, width: 24, height: 24 }).width(17).height(17)
    }.backgroundColor(Color.Transparent).padding(8).borderRadius(12).height(40).width(40)
      .enabled(this.editingEnabled && allowed).accessibilityText(this.t(description)).onClick(run)
  }
  @Builder
  private dragHandle(ticket: EditorTodoTicket) {
    Button({ type: ButtonType.Normal }) {
      Shape() {
        Path().commands('M8 3A2 2 0 1 1 8 7A2 2 0 1 1 8 3ZM16 3A2 2 0 1 1 16 7A2 2 0 1 1 16 3ZM8 10A2 2 0 1 1 8 14A2 2 0 1 1 8 10ZM16 10A2 2 0 1 1 16 14A2 2 0 1 1 16 10ZM8 17A2 2 0 1 1 8 21A2 2 0 1 1 8 17ZM16 17A2 2 0 1 1 16 21A2 2 0 1 1 16 17Z').fill(this.muted)
      }.viewPort({ x: 0, y: 0, width: 24, height: 24 }).width(19).height(19)
        .opacity(this.editingEnabled && this.model.canChangeRows() ? 1 : 0.3)
    }.backgroundColor(Color.Transparent).padding(0).borderRadius(12).height(40).width(36)
      .accessibilityText(this.t('拖拽排序'))
      .draggable(this.editingEnabled && this.model.canChangeRows()).onDragStart((): DragItemInfo => this.startDrag(ticket))
      .onDragMove((event: DragEvent): void => this.dragMoved(event)).onDragEnd((): void => this.finishDrag())
  }
  @Builder
  private rowEditor(row: EditorTodoRow, index: number, ticket: EditorTodoTicket) {
    Column() {
      Row() {
        Text(this.t('提示') + ' ' + (index + 1).toString().padStart(2, '0')).fontSize(11).fontColor(this.accent).fontWeight(600).layoutWeight(1)
        this.dragHandle(ticket)
        this.action('up', '上移', this.model.canChangeRows() && index > 0, (): void => this.moveFor(ticket, -1))
        this.action('down', '下移', this.model.canChangeRows() && index < this.state.rows.length - 1, (): void => this.moveFor(ticket, 1))
        this.action('delete', '删除', this.model.canChangeRows(), (): void => this.removeFor(ticket))
      }.width('100%')
      EditorTodoRowInput({ row, owner: ticket.owner, elementId: this.element(row.id), renderRevision: this.renderRevision,
        editingEnabled: this.editingEnabled && this.model.canEdit(row.id), ink: this.ink, muted: this.muted, accent: this.accent, family: this.family, locale: this.locale,
        onInput: (captured: EditorTodoTicket, text: string, preview?: PreviewText): Promise<void> => this.inputFor(captured, text, preview),
        onSelection: (captured: EditorTodoTicket, base: number, extent: number): Promise<void> => this.selectionFor(captured, base, extent),
        onUncaptured: (captured: EditorTodoTicket, event: string): void => this.uncapturedFor(captured, event),
        onRowFocus: (captured: EditorTodoTicket): void => this.focusFor(captured)
      })
      if (row.pending) { Text(this.t('正在检查…')).fontSize(11).fontColor(this.muted).width('100%') }
      if (this.state.retry_row_id === row.id) {
        Button(this.t('重新检查'), { type: ButtonType.Normal }).fontSize(12).fontColor(this.accent)
          .backgroundColor(this.surface).borderRadius(12)
          .enabled(this.editingEnabled && this.model.canRetry(ticket)).onClick((): void => { this.retryFor(ticket); })
      }
    }.width('100%').padding({ left: 16, right: 12, top: 6, bottom: 14 }).alignItems(HorizontalAlign.Start)
      .backgroundColor(this.surface).borderRadius(this.radius).border({ width: 1, color: this.line })
      .onDrop((event: DragEvent): void => this.dropFor(ticket, event))
      .onAreaChange((_old: Area, next: Area): void => this.areaFor(ticket, next))
  }
  build() {
    Column({ space: 12 }) {
      Row({ space: 12 }) {
        Text('“').fontSize(28).fontColor(this.accent).padding(10).borderRadius(14).backgroundColor(this.surface)
        Text(this.title).fontSize(18).fontWeight(600).fontColor(this.ink).layoutWeight(1)
        Text(this.state.rows.length + ' / 100').fontSize(12).fontColor(this.muted)
      }.width('100%')
      Text(this.t('一条一张卡片，可添加、删除或拖拽排序。随当前卡片一起保存。')).fontSize(12).fontColor(this.muted).lineHeight(19).width('100%')
      Row({ space: 10 }) {
        Button(this.t('＋ 添加一条'), { type: ButtonType.Normal }).fontSize(14).fontColor(this.accent).backgroundColor(this.surface).borderRadius(12)
          .enabled(this.editingEnabled && this.model.canChangeRows() && this.state.rows.length < 100 && this.state.count >= 0 && this.state.count < 1000)
          .onClick((): void => { this.addRow(); })
        Text((this.state.counting ? this.t('计数中…') : this.state.count >= 0 ? this.state.count + ' / 1000' : this.t('字数未确认')))
          .fontSize(12).fontColor(this.muted)
      }.width('100%')
      if (!this.state.rows.length) { Text(this.t('还没有文案。添加一条，或直接保存以恢复默认。')).fontSize(13).fontColor(this.muted).padding(16).width('100%') }
      ForEach(this.state.rows, (row: EditorTodoRow, index: number): void => { this.rowEditor(row, index, this.ticket(row)); },
        (row: EditorTodoRow): string => this.state.owner + ':' + row.id)
      if (this.state.notice) { Text(this.t(this.state.notice)).fontSize(12).fontColor(this.muted).width('100%') }
      if (this.state.error) { Text(this.t(this.state.error)).fontSize(12).fontColor(this.accent).width('100%') }
      Text(this.t('空白条目会忽略 · 保存后生效')).fontSize(11).fontColor(this.muted).width('100%')
    }.width('100%').alignItems(HorizontalAlign.Start)
  }
}

@Component
struct EditorTodoRowInput {
  @Prop @Watch('rowChanged') row: EditorTodoRow = new EditorTodoRow();
  @Prop owner: string = '';
  @Prop elementId: string = '';
  @Prop @Watch('rowChanged') renderRevision: number = 0;
  @Prop editingEnabled: boolean = true;
  @Prop ink: string = '#302D43';
  @Prop muted: string = '#81798F';
  @Prop accent: string = '#7662BA';
  @Prop family: string = 'HarmonyOS Sans';
  @Prop locale: string = 'zh';
  onInput: (ticket: EditorTodoTicket, text: string, preview?: PreviewText) => Promise<void> = async (): Promise<void> => {};
  onSelection: (ticket: EditorTodoTicket, base: number, extent: number) => Promise<void> = async (): Promise<void> => {};
  onUncaptured: (ticket: EditorTodoTicket, event: string) => void = (): void => {};
  onRowFocus: (ticket: EditorTodoTicket) => void = (): void => {};
  @State private text: string = '';
  @State private session: EditorTodoTicket = new EditorTodoTicket();
  private controller: TextAreaController = new TextAreaController();
  private alive: boolean = false;
  @State private focused: boolean = false;
  private serial: number = 0;
  private expectedBase: number = -1;
  private expectedExtent: number = -1;
  private mountedOnce: boolean = false;
  private everFocused: boolean = false;
  private initialEcho: TodoInitialDisplayEcho | undefined = undefined;

  aboutToAppear(): void {
    const firstMount: boolean = !this.mountedOnce; this.mountedOnce = true;
    this.alive = true; this.rowChanged();
    if (firstMount) {
      const ticket = new EditorTodoTicket(); ticket.owner = this.session.owner; ticket.id = this.session.id; ticket.incarnation = this.session.incarnation;
      const echo: TodoInitialDisplayEcho = { controller: this.controller, ticket, value: copyText(this.row.value), display: this.row.display_text, control: this.text, pending: this.row.pending };
      this.initialEcho = echo;
    }
  }
  aboutToDisappear(): void { this.initialEcho = undefined; this.alive = false; this.focused = false; this.serial++; this.controller.stopEditing(); }
  private ownsIdentity(ticket: EditorTodoTicket): boolean {
    return this.alive && ticket.owner === this.owner && ticket.id === this.row.id && ticket.incarnation === this.row.incarnation;
  }
  private owns(ticket: EditorTodoTicket): boolean { return this.editingEnabled && this.ownsIdentity(ticket); }
  private rowChanged(): void {
    this.initialEcho = undefined;
    if (!this.alive) { return; }
    const fresh = new EditorTodoTicket(); fresh.owner = this.owner; fresh.id = this.row.id; fresh.incarnation = this.row.incarnation;
    this.session = fresh; this.text = this.row.display_text;
    this.expectedBase = -1; this.expectedExtent = -1;
    const serial: number = ++this.serial, value = copyText(this.row.value);
    if (this.row.pending || value.composing_start !== -1 || value.composing_end !== -1 || value.selection_base < 0 || value.selection_extent < 0) { return; }
    setTimeout((): void => {
      if (this.owns(fresh) && serial === this.serial && this.focused) {
        this.expectedBase = value.selection_base; this.expectedExtent = value.selection_extent;
        this.controller.setTextSelection(value.selection_base, value.selection_extent);
      }
    }, 0);
  }
  private changeFor(ticket: EditorTodoTicket, text: string, preview?: PreviewText): void {
    if (!this.ownsIdentity(ticket)) { return; }
    // Only the first admitted callback of a new control can acknowledge its
    // already-set initial display. A refresh, focus or prior input revokes this
    // capability; equal text by itself never clears a missing capture/Unknown.
    const echo = this.initialEcho; this.initialEcho = undefined;
    if (echo && echo.controller === this.controller && echo.ticket.owner === ticket.owner && echo.ticket.id === ticket.id &&
      echo.ticket.incarnation === ticket.incarnation && !this.everFocused && !this.focused && !echo.pending && !this.row.pending &&
      echo.value.composing_start === -1 && echo.value.composing_end === -1 && samePasteTarget(echo.value, this.row.value) &&
      echo.value.text === echo.display && echo.display === this.row.display_text && echo.control === this.text && text === echo.display && text === echo.control &&
      (preview === undefined || (!!preview && preview.value === '' && preview.offset === -1))) { return; }
    if (!this.editingEnabled) {
      this.onUncaptured(ticket, todoInputEvent(text, preview)); return;
    }
    this.text = text; this.expectedBase = -1; this.expectedExtent = -1;
    this.onInput(ticket, text, preview);
  }
  private selectionFor(ticket: EditorTodoTicket, base: number, extent: number): void {
    // Unfocused construction callbacks describe the native control, not a new
    // raw selection. A disabled focused row can still deliver a real event.
    if (!this.ownsIdentity(ticket) || !this.focused) { return; }
    if (this.expectedBase >= 0 && this.expectedExtent >= 0 && base === this.expectedBase && extent === this.expectedExtent) {
      this.expectedBase = -1; this.expectedExtent = -1; return;
    }
    this.expectedBase = -1; this.expectedExtent = -1;
    if (!this.editingEnabled) { this.onUncaptured(ticket, todoSelectionEvent(base, extent)); return; }
    this.onSelection(ticket, base, extent);
  }
  private focusFor(ticket: EditorTodoTicket, focused: boolean): void {
    if (!this.ownsIdentity(ticket)) { return; }
    this.initialEcho = undefined;
    if (focused) { this.everFocused = true; }
    if (!focused) { this.focused = false; this.expectedBase = -1; this.expectedExtent = -1; return; }
    if (!this.editingEnabled) { return; }
    this.focused = true; this.onRowFocus(ticket);
  }
  @Builder
  private field(ticket: EditorTodoTicket) {
    TextArea({ text: this.text, placeholder: uiText('写下一句话…', this.locale), controller: this.controller })
      .id(this.elementId).enabled(this.editingEnabled).fontFamily(this.family).fontSize(14).fontColor(this.ink).placeholderColor(this.muted)
      .backgroundColor(Color.Transparent).borderRadius(0).borderWidth({ bottom: this.focused ? 1 : 0 }).borderColor(this.accent)
      .padding({ top: 8, bottom: 8 }).width('100%').lineHeight('23.1fp')
      .minLines(1).maxLines(3, { overflowMode: MaxLinesMode.SCROLL }).textOverflow(TextOverflow.None)
      .enablePreviewText(true).onChange((text: string, preview?: PreviewText): void => this.changeFor(ticket, text, preview))
      .onTextSelectionChange((base: number, extent: number): void => this.selectionFor(ticket, base, extent))
      .onFocus((): void => this.focusFor(ticket, true)).onBlur((): void => this.focusFor(ticket, false))
  }
  build() { this.field(this.session) }
}
