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

// Flutter IdeaMarkdown matches decoded attachment path segments against the
// supplied image/GIF inventory in order. A location is only an alias here;
// callers resolve the returned asset ID through the verified attachment reader.
// The model never opens a URI, filesystem path, or network image.
export class MarkdownAttachment {
  id: string = '';
  name: string = '';
  location: string = '';
  kind: string = 'file';
}
export class MarkdownImageResolution {
  kind: string = 'not_imported';
  attachment_id: string = '';
  name: string = '';
  href: string = '';
  error: string = '';
}

function normalizedUriPath(path: string): string {
  // Dart Uri canonicalizes malformed percent escapes to a literal percent,
  // and decodes unreserved ASCII before removing dot segments. Reserved slash
  // escapes stay encoded until pathSegments are decoded below.
  const escaped = path.replace(/\\/g, '/').replace(/%([0-9a-fA-F]{2})|%/g,
    (match: string, digits: string): string => {
      if (!digits) { return '%25'; }
      const value = String.fromCharCode(parseInt(digits, 16));
      return /^[A-Za-z0-9._~-]$/.test(value) ? value : '%' + digits.toUpperCase();
    });
  const output: string[] = [];
  let appendSlash = false;
  for (const segment of escaped.split('/')) {
    appendSlash = false;
    if (segment === '..') {
      if (output.length > 0) { output.pop(); if (output.length === 0) { output.push(''); } }
      appendSlash = true;
    } else if (segment === '.') { appendSlash = true; }
    else { output.push(segment); }
  }
  if (appendSlash) { output.push(''); }
  return output.join('/');
}

function invalidMarkdownImage(href: string, error: string): MarkdownImageResolution {
  const result = new MarkdownImageResolution(); result.kind = 'invalid';
  result.href = href; result.error = error; return result;
}

function validImageAuthority(authority: string): boolean {
  const host = authority.slice(authority.lastIndexOf('@') + 1);
  if (host.startsWith('[')) {
    const close = host.indexOf(']');
    return close > 1 && /^[0-9a-fA-F:.]+$/.test(host.slice(1, close)) &&
      (close === host.length - 1 || /^:[0-9]*$/.test(host.slice(close + 1)));
  }
  if (host.includes('[') || host.includes(']')) { return false; }
  const colon = host.indexOf(':');
  return colon < 0 || /^[0-9]*$/.test(host.slice(colon + 1));
}

export function resolveMarkdownImage(href: string, attachments: MarkdownAttachment[]): MarkdownImageResolution {
  const result = new MarkdownImageResolution(); result.href = href;
  const scheme = /^([A-Za-z][A-Za-z0-9+.-]*):/.exec(href);
  if (!scheme) { return result; }
  const kind = scheme[1].toLowerCase();
  const rest = href.slice(scheme[0].length);
  const delimiters = rest.search(/[?#]/);
  const pathAndAuthority = delimiters < 0 ? rest : rest.slice(0, delimiters);
  let path = pathAndAuthority.replace(/\\/g, '/');
  let authority = '';
  if (path.startsWith('//')) {
    const slash = path.indexOf('/', 2);
    authority = slash < 0 ? path.slice(2) : path.slice(2, slash);
    path = slash < 0 ? '' : path.slice(slash);
  }
  if (kind === 'attachment') {
    // An authority never becomes a provider URI or file path. Like Flutter,
    // only the decoded path can identify an item in the explicit inventory.
    if (!validImageAuthority(authority)) {
      return invalidMarkdownImage(href, 'invalid_uri');
    }
    const normalized = normalizedUriPath(path);
    const segmented = normalized.startsWith('/') ? normalized.slice(1) : normalized;
    try {
      result.name = segmented.split('/').map((part: string): string => decodeURIComponent(part)).join('/');
    } catch (_) { return invalidMarkdownImage(href, 'invalid_encoding'); }
    if (!result.name) { return result; }
    for (const asset of attachments) {
      if (asset && typeof asset.id === 'string' && asset.id.length > 0 &&
        typeof asset.name === 'string' && typeof asset.location === 'string' &&
        (asset.kind === 'image' || asset.kind === 'gif') &&
        (asset.name === result.name || asset.location === result.name || asset.id === result.name)) {
        result.kind = 'attachment'; result.attachment_id = asset.id; result.name = asset.name;
        return result;
      }
    }
    return result;
  }
  if (kind !== 'http' && kind !== 'https') { return result; }
  // Mirror safeContentLink: host required, credentials forbidden. Reject
  // malformed authorities before the renderer may offer explicit network load.
  if (!pathAndAuthority.replace(/\\/g, '/').startsWith('//') || !authority || authority.includes('@') ||
    /[\s\\/%?#]/.test(authority)) { return invalidMarkdownImage(href, 'unsafe_remote'); }
  const ipv6 = /^\[[0-9a-fA-F:.]+\](?::[0-9]+)?$/.test(authority);
  const hostname = /^[^:\[\]]+(?::[0-9]+)?$/.test(authority);
  if (!ipv6 && !hostname) { return invalidMarkdownImage(href, 'invalid_uri'); }
  result.kind = 'remote'; result.href = kind + ':' + rest; return result;
}

// Include table images as well as paragraph/heading images. Repeated hrefs
// share one read, while each run retains its own alt text in the document.
export function collectMarkdownImages(doc: MarkdownDoc): string[] {
  const sources: string[] = [];
  for (const block of doc.blocks) {
    if (block.kind === 'code' || block.kind === 'rule') { continue; }
    const runs: MarkdownRun[] = block.kind === 'table' ?
      block.rows.flatMap((row: MarkdownRow): MarkdownRun[] =>
        row.cells.flatMap((cell: MarkdownCell): MarkdownRun[] => cell.runs)) : block.runs;
    for (const run of runs) {
      if (run.image && !sources.includes(run.href)) { sources.push(run.href); }
    }
  }
  return sources;
}

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
