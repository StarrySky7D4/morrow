export class MarkdownRun {
  text: string = '';
  bold: boolean = false;
  italic: boolean = false;
  strike: boolean = false;
  code: boolean = false;
  href: string = '';
  image: boolean = false;
}
export class MarkdownCell { runs: MarkdownRun[] = []; }
export class MarkdownRow { header: boolean = false; cells: MarkdownCell[] = []; }
export class MarkdownBlock {
  kind: string = 'paragraph';
  runs: MarkdownRun[] = [];
  level: number = 0;
  indent: number = 0;
  quote: number = 0;
  marker: string = '';
  language: string = '';
  rows: MarkdownRow[] = [];
  alignments: string[] = [];
}
export class MarkdownDoc { blocks: MarkdownBlock[] = []; }

function copyRun(run: MarkdownRun): MarkdownRun {
  const result = new MarkdownRun(); result.text = run.text; result.bold = run.bold;
  result.italic = run.italic; result.strike = run.strike; result.code = run.code;
  result.href = run.href; result.image = run.image; return result;
}
export function copyMarkdownDoc(doc: MarkdownDoc): MarkdownDoc {
  const result = new MarkdownDoc();
  result.blocks = doc.blocks.map((block: MarkdownBlock): MarkdownBlock => {
    const value = new MarkdownBlock(); value.kind = block.kind;
    value.runs = block.runs.map(copyRun); value.level = block.level;
    value.indent = block.indent; value.quote = block.quote;
    value.marker = block.marker; value.language = block.language;
    value.alignments = block.alignments.slice();
    value.rows = block.rows.map((row: MarkdownRow): MarkdownRow => {
      const copied = new MarkdownRow(); copied.header = row.header;
      copied.cells = row.cells.map((cell: MarkdownCell): MarkdownCell => {
        const copiedCell = new MarkdownCell(); copiedCell.runs = cell.runs.map(copyRun);
        return copiedCell;
      });
      return copied;
    });
    return value;
  });
  return result;
}
function validRuns(runs: MarkdownRun[]): boolean {
  if (!Array.isArray(runs)) { return false; }
  for (const run of runs) {
    if (!run || typeof run.text !== 'string' || typeof run.href !== 'string' ||
      typeof run.bold !== 'boolean' || typeof run.italic !== 'boolean' ||
      typeof run.strike !== 'boolean' || typeof run.code !== 'boolean' ||
      typeof run.image !== 'boolean') { return false; }
  }
  return true;
}
function natural(value: number): boolean {
  return typeof value === 'number' && Number.isFinite(value) && value >= 0 && Math.floor(value) === value;
}
// The transport adapter must reject unsuccessful Replies. Check the complete
// display DTO as well, so malformed JSON cannot become a successful preview.
export function validMarkdownDoc(doc: MarkdownDoc): boolean {
  if (!doc || !Array.isArray(doc.blocks)) { return false; }
  for (const block of doc.blocks) {
    if (!block || !['paragraph', 'heading', 'code', 'rule', 'table'].includes(block.kind) ||
      !validRuns(block.runs) || !natural(block.level) || block.level > 6 ||
      !natural(block.indent) || !natural(block.quote) ||
      typeof block.marker !== 'string' || typeof block.language !== 'string' ||
      !Array.isArray(block.rows) || !Array.isArray(block.alignments)) { return false; }
    if (block.kind === 'heading' && block.level === 0) { return false; }
    for (const alignment of block.alignments) {
      if (!['none', 'left', 'center', 'right'].includes(alignment)) { return false; }
    }
    for (const row of block.rows) {
      if (!row || typeof row.header !== 'boolean' || !Array.isArray(row.cells)) { return false; }
      for (const cell of row.cells) { if (!cell || !validRuns(cell.runs)) { return false; } }
    }
  }
  return true;
}

// Read-only, single-flight preview work. A disabled or replaced editor leaves
// issued work alone; its late success and failure can never publish to the UI.
// Preview lifetime is independent of the durable editor draft coordinator.
export class MarkdownPreviewCoordinator {
  private request: (text: string) => Promise<MarkdownDoc>;
  private changed: () => void;
  private debounce: number;
  private currentScope: string = '';
  private currentText: string = '';
  private active: boolean = false;
  private isDisposed: boolean = false;
  private sequence: number = 0;
  private timer: number = -1;
  private running: boolean = false;
  private due: boolean = false;
  private currentPhase: string = 'idle';
  private currentDoc: MarkdownDoc | undefined = undefined;
  private currentError: string = '';
  constructor(request: (text: string) => Promise<MarkdownDoc>, changed: () => void,
    debounce: number = 150) {
    this.request = request; this.changed = changed; this.debounce = debounce;
  }
  get scope(): string { return this.currentScope; }
  get text(): string { return this.currentText; }
  get enabled(): boolean { return this.active; }
  get disposed(): boolean { return this.isDisposed; }
  get phase(): string { return this.currentPhase; }
  get loading(): boolean { return this.currentPhase === 'loading'; }
  get error(): string { return this.currentError; }
  get doc(): MarkdownDoc | undefined {
    return this.currentDoc ? copyMarkdownDoc(this.currentDoc) : undefined;
  }

  update(scope: string, text: string, enabled: boolean): void {
    if (this.isDisposed || (scope === this.currentScope && text === this.currentText && enabled === this.active)) { return; }
    this.cancelScheduled(); this.sequence++;
    this.currentScope = scope; this.currentText = text; this.active = enabled;
    this.currentDoc = undefined; this.currentError = '';
    this.currentPhase = enabled ? 'loading' : 'idle';
    const sequence = this.sequence;
    this.changed();
    if (enabled && !this.isDisposed && sequence === this.sequence) { this.schedule(this.debounce); }
  }
  retry(): void {
    if (this.isDisposed || !this.active || this.currentPhase !== 'failed') { return; }
    this.cancelScheduled(); this.sequence++; this.currentDoc = undefined;
    this.currentError = ''; this.currentPhase = 'loading';
    const sequence = this.sequence; this.changed();
    if (!this.isDisposed && sequence === this.sequence) { this.schedule(0); }
  }
  dispose(): void {
    if (this.isDisposed) { return; }
    this.isDisposed = true; this.sequence++; this.cancelScheduled();
    this.active = false; this.currentDoc = undefined; this.currentError = ''; this.currentPhase = 'idle';
  }
  private cancelScheduled(): void {
    if (this.timer !== -1) { clearTimeout(this.timer); this.timer = -1; }
    this.due = false;
  }
  private schedule(delay: number): void {
    this.timer = setTimeout((): void => {
      this.timer = -1; this.due = true; this.drain();
    }, delay);
  }
  private async drain(): Promise<void> {
    if (this.isDisposed || !this.active || this.running || !this.due) { return; }
    const sequence = this.sequence, scope = this.currentScope, text = this.currentText;
    this.due = false; this.running = true;
    try {
      const doc = await this.request(text);
      if (this.isDisposed || !this.active || sequence !== this.sequence ||
        scope !== this.currentScope || text !== this.currentText) { return; }
      if (!validMarkdownDoc(doc)) { throw new Error('Invalid Markdown preview response'); }
      this.currentDoc = copyMarkdownDoc(doc); this.currentError = ''; this.currentPhase = 'ready'; this.changed();
    } catch (error) {
      if (!this.isDisposed && this.active && sequence === this.sequence &&
        scope === this.currentScope && text === this.currentText) {
        const message = error instanceof Error ? error.message : String(error);
        this.currentDoc = undefined; this.currentError = message.trim().length > 0 ? message : 'Markdown preview failed';
        this.currentPhase = 'failed'; this.changed();
      }
    } finally {
      this.running = false;
      if (!this.isDisposed) { this.drain(); }
    }
  }
}
