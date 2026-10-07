import { TextValue, copyText } from '../model/EditorDraft';
import { EditorTodosDraft, EditorTodoRow, EditorTodoTicket, EditorTodoDrag, EditorTodosView, EditorTodoFormatResult } from '../model/EditorTodos';
import { uiText } from '../model/UiStrings';

class TodoRowArea {
  incarnation: number = 0;
  top: number = 0;
  height: number = 0;
}

// This component edits only the raw newline field. Its parent owns the journal,
// editorInputPolicy workers and business save. Row IDs are local UI identities.
@Component
export struct EditorTodos {
  @Prop @Watch('sourceChanged') ownerKey: string = '';
  @Prop @Watch('sourceChanged') revision: number = 0;
  @Prop @Watch('sourceChanged') source: TextValue = new TextValue();
  @Prop @Watch('availabilityChanged') editingEnabled: boolean = true;
  @Prop title: string = '待办';
  @Prop ink: string = '#302D43';
  @Prop muted: string = '#81798F';
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
  onStatus: (owner: string, complete: boolean, pending: boolean, error: string) => void = (): void => {};
  onRowFocused: (owner: string, row: string) => void = (): void => {};
  onRevealRow: (owner: string, row: string, element: string) => void = (): void => {};
  onDragPosition: (owner: string, windowY: number, active: boolean) => void = (): void => {};
  @State private state: EditorTodosView = new EditorTodosView();
  @State private renderRevision: number = 0;
  private alive: boolean = false;
  private serial: number = 0;
  private drag: EditorTodoDrag | undefined = undefined;
  private areas: Map<string, TodoRowArea> = new Map<string, TodoRowArea>();
  private model: EditorTodosDraft = new EditorTodosDraft({
    capture: (value: TextValue, owner: string): number | undefined => this.alive && this.editingEnabled && owner === this.ownerKey ? this.onCapture(value, owner) : undefined,
    isCurrent: (owner: string, revision: number, value: TextValue): boolean => this.alive && this.editingEnabled && owner === this.ownerKey && this.isCurrent(owner, revision, value),
    count: (text: string, owned: () => boolean): Promise<number> => this.count(text, owned),
    format: (oldValue: TextValue, newValue: TextValue, remaining: number, owned: () => boolean): Promise<EditorTodoFormatResult> => this.format(oldValue, newValue, remaining, owned),
    changed: (): void => this.syncView(),
    failed: (owner: string, message: string): void => { if (this.alive && owner === this.ownerKey) { this.onStatus(owner, false, false, message); } },
    id: (): string => 'todo_' + (++this.serial)
  });

  aboutToAppear(): void { this.alive = true; this.sourceChanged(); }
  aboutToDisappear(): void { this.finishDrag(); this.alive = false; this.areas.clear(); this.model.stop(); }
  private t(value: string): string { return uiText(value, this.locale); }
  private sourceChanged(): void {
    if (!this.alive || !this.ownerKey) { return; }
    if (this.state.owner !== this.ownerKey) { this.finishDrag(); this.areas.clear(); }
    try { this.model.bind(this.ownerKey, this.revision, copyText(this.source)); if (!this.editingEnabled) { this.model.stop(); } }
    catch (error) { this.onStatus(this.ownerKey, false, false, String(error)); }
  }
  private availabilityChanged(): void { if (this.editingEnabled) { this.sourceChanged(); } else { this.finishDrag(); this.model.stop(); } }
  private syncView(): void {
    if (!this.alive) { return; }
    this.state = this.model.view(); this.renderRevision++;
    if (this.state.owner === this.ownerKey) {
      this.onStatus(this.ownerKey, this.state.capture_complete, this.state.rows.some((row: EditorTodoRow): boolean => row.pending), this.state.error);
    }
  }
  private ticket(row: EditorTodoRow): EditorTodoTicket {
    const result = new EditorTodoTicket(); result.owner = this.state.owner; result.id = row.id; result.incarnation = row.incarnation; return result;
  }
  private owns(ticket: EditorTodoTicket): boolean {
    const row = this.model.row(ticket.id);
    return this.alive && this.editingEnabled && ticket.owner === this.ownerKey && ticket.owner === this.state.owner &&
      row?.incarnation === ticket.incarnation && this.isCurrent(this.state.owner, this.state.revision, copyText(this.state.value));
  }
  private element(row: string, owner: string = this.state.owner): string { return 'editor-todo-' + encodeURIComponent(owner) + '-' + row; }
  private async inputFor(ticket: EditorTodoTicket, text: string, preview?: PreviewText): Promise<void> {
    if (!this.owns(ticket)) { return; }
    await this.model.input(ticket, text, preview?.value || '', preview?.offset ?? -1);
  }
  private async selectionFor(ticket: EditorTodoTicket, base: number, extent: number): Promise<void> {
    if (this.owns(ticket)) { await this.model.selection(ticket, base, extent); }
  }
  private focusFor(ticket: EditorTodoTicket): void {
    if (!this.owns(ticket)) { return; }
    this.model.focus(ticket); this.onRowFocused(ticket.owner, ticket.id);
  }
  private async addRow(): Promise<void> {
    const owner: string = this.state.owner;
    try {
      const id: string | undefined = await this.model.add();
      if (!id || !this.alive || owner !== this.ownerKey) { return; }
      const ticket = this.model.ticket(id);
      this.onRevealRow(owner, id, this.element(id, owner));
      setTimeout((): void => {
        if (this.owns(ticket)) { this.getUIContext().getFocusController().requestFocus(this.element(id, owner)); }
      }, 0);
    } catch (error) { if (this.alive && owner === this.ownerKey) { this.onStatus(owner, false, false, String(error)); } }
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
    if (!this.owns(ticket) || typeof area.globalPosition.y !== 'number' || typeof area.height !== 'number' ||
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

  @Builder
  private action(label: string, description: string, allowed: boolean, run: () => void) {
    Button(label, { type: ButtonType.Normal }).fontSize(14).fontColor(this.accent).backgroundColor(Color.Transparent)
      .borderRadius(9).height(32).width(32).enabled(this.editingEnabled && allowed).accessibilityText(this.t(description)).onClick(run)
  }
  @Builder
  private dragHandle(ticket: EditorTodoTicket) {
    Button('⋮⋮', { type: ButtonType.Normal }).fontSize(14).fontColor(this.accent).backgroundColor(Color.Transparent)
      .borderRadius(9).height(32).width(32).enabled(this.editingEnabled && this.model.canChangeRows()).accessibilityText(this.t('拖拽排序'))
      .draggable(this.editingEnabled && this.model.canChangeRows()).onDragStart((): DragItemInfo => this.startDrag(ticket))
      .onDragMove((event: DragEvent): void => this.dragMoved(event)).onDragEnd((): void => this.finishDrag())
  }
  @Builder
  private rowEditor(row: EditorTodoRow, index: number, ticket: EditorTodoTicket) {
    Column({ space: 5 }) {
      Row({ space: 3 }) {
        Text(this.t('提示') + ' ' + (index + 1).toString().padStart(2, '0')).fontSize(11).fontColor(this.accent).fontWeight(FontWeight.Medium).layoutWeight(1)
        this.dragHandle(ticket)
        this.action('↑', '上移', this.model.canChangeRows() && index > 0, (): void => this.moveFor(ticket, -1))
        this.action('↓', '下移', this.model.canChangeRows() && index < this.state.rows.length - 1, (): void => this.moveFor(ticket, 1))
        this.action('×', '删除', this.model.canChangeRows(), (): void => this.removeFor(ticket))
      }.width('100%')
      EditorTodoRowInput({ row, owner: ticket.owner, elementId: this.element(row.id), renderRevision: this.renderRevision,
        editingEnabled: this.editingEnabled && this.model.canEdit(row.id), ink: this.ink, muted: this.muted, accent: this.accent, family: this.family, locale: this.locale,
        onInput: (captured: EditorTodoTicket, text: string, preview?: PreviewText): Promise<void> => this.inputFor(captured, text, preview),
        onSelection: (captured: EditorTodoTicket, base: number, extent: number): Promise<void> => this.selectionFor(captured, base, extent),
        onRowFocus: (captured: EditorTodoTicket): void => this.focusFor(captured)
      })
      if (row.pending) { Text(this.t('正在检查…')).fontSize(11).fontColor(this.muted).width('100%') }
    }.width('100%').padding({ left: 16, right: 12, top: 6, bottom: 14 }).alignItems(HorizontalAlign.Start)
      .backgroundColor(this.surface).borderRadius(this.radius).border({ width: 1, color: this.line })
      .onDrop((event: DragEvent): void => this.dropFor(ticket, event))
      .onAreaChange((_old: Area, next: Area): void => this.areaFor(ticket, next))
  }
  build() {
    Column({ space: 12 }) {
      Row({ space: 12 }) {
        Text('“').fontSize(28).fontColor(this.accent).padding(10).borderRadius(14).backgroundColor(this.surface)
        Text(this.title).fontSize(18).fontWeight(FontWeight.Medium).fontColor(this.ink).layoutWeight(1)
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
  onRowFocus: (ticket: EditorTodoTicket) => void = (): void => {};
  @State private text: string = '';
  @State private session: EditorTodoTicket = new EditorTodoTicket();
  private controller: TextAreaController = new TextAreaController();
  private alive: boolean = false;
  private focused: boolean = false;
  private serial: number = 0;
  private expectedBase: number = -1;
  private expectedExtent: number = -1;

  aboutToAppear(): void { this.alive = true; this.rowChanged(); }
  aboutToDisappear(): void { this.alive = false; this.focused = false; this.serial++; this.controller.stopEditing(); }
  private owns(ticket: EditorTodoTicket): boolean {
    return this.alive && this.editingEnabled && ticket.owner === this.owner && ticket.id === this.row.id && ticket.incarnation === this.row.incarnation;
  }
  private rowChanged(): void {
    if (!this.alive) { return; }
    const fresh = new EditorTodoTicket(); fresh.owner = this.owner; fresh.id = this.row.id; fresh.incarnation = this.row.incarnation;
    this.session = fresh; this.text = this.row.display_text;
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
    if (!this.owns(ticket)) { return; }
    this.text = text; this.expectedBase = -1; this.expectedExtent = -1;
    this.onInput(ticket, text, preview);
  }
  private selectionFor(ticket: EditorTodoTicket, base: number, extent: number): void {
    if (!this.owns(ticket) || !this.focused) { return; }
    if (base === this.expectedBase && extent === this.expectedExtent) { this.expectedBase = -1; this.expectedExtent = -1; return; }
    this.expectedBase = -1; this.expectedExtent = -1; this.onSelection(ticket, base, extent);
  }
  private focusFor(ticket: EditorTodoTicket, focused: boolean): void {
    if (!this.owns(ticket)) { return; }
    this.focused = focused; if (focused) { this.onRowFocus(ticket); }
  }
  @Builder
  private field(ticket: EditorTodoTicket) {
    TextArea({ text: this.text, placeholder: uiText('写下一句话…', this.locale), controller: this.controller })
      .id(this.elementId).enabled(this.editingEnabled).fontFamily(this.family).fontSize(14).fontColor(this.ink).placeholderColor(this.muted)
      .backgroundColor(Color.Transparent).borderRadius(0).padding({ top: 8, bottom: 8 }).width('100%').height(76)
      .enablePreviewText(true).onChange((text: string, preview?: PreviewText): void => this.changeFor(ticket, text, preview))
      .onTextSelectionChange((base: number, extent: number): void => this.selectionFor(ticket, base, extent))
      .onFocus((): void => this.focusFor(ticket, true)).onBlur((): void => this.focusFor(ticket, false))
  }
  build() { this.field(this.session) }
}
