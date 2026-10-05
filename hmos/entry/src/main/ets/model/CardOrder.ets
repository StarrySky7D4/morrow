// Port of Flutter card_order_preferences.dart / pages/card_order.dart and
// hold_reorder.dart::moveRelative. This writes UI preferences, never the Store.
const stablePages: string[] = ['workbench.page.overview', 'workbench.page.inbox',
  'workbench.page.projects', 'workbench.page.laboratory', 'workbench.page.favorites'];
export const cardOrderPages: string[] = stablePages.slice();
export function cardOrderPage(section: string): string {
  const index = ['概览', '灵感', '进行中', '实验', '收藏'].indexOf(section);
  return index >= 0 ? stablePages[index] : stablePages.includes(section) ? section : '';
}
export class CardOrderSnapshot {
  version: number = 1;
  orders: Record<string, string[]> = {};
  manual: string[] = [];
}
export class CardOrderView {
  page: string = '';
  all: string[] = [];
  visible: string[] = [];
  token: string = '';
}
const MAX_CARDS: number = 256;
function validPage(page: string): boolean { return stablePages.includes(page); }
function validIds(ids: string[]): boolean {
  if (!Array.isArray(ids) || ids.length > MAX_CARDS) { return false; }
  const seen = new Set<string>();
  for (const id of ids) {
    if (typeof id !== 'string' || id.length === 0 || seen.has(id)) { return false; }
    seen.add(id);
  }
  return true;
}
function sameIds(a: string[], b: string[]): boolean {
  return a.length === b.length && a.every((id: string, index: number): boolean => id === b[index]);
}
function validView(all: string[], visible: string[]): boolean {
  if (!validIds(all) || !validIds(visible)) { return false; }
  const ids = new Set<string>(all);
  return visible.every((id: string): boolean => ids.has(id));
}
function copySnapshot(value: CardOrderSnapshot): CardOrderSnapshot {
  const result = new CardOrderSnapshot(); result.version = value.version;
  for (const page of Object.keys(value.orders)) { result.orders[page] = value.orders[page].slice(); }
  result.manual = value.manual.slice(); return result;
}
function validSnapshot(value: CardOrderSnapshot): boolean {
  if (!value || value.version !== 1 || !value.orders || typeof value.orders !== 'object' ||
    Array.isArray(value.orders) || !Array.isArray(value.manual) || value.manual.length > 5) { return false; }
  const pages = Object.keys(value.orders), manual = new Set<string>();
  if (pages.length > 5) { return false; }
  for (const page of pages) { if (!validPage(page) || !validIds(value.orders[page])) { return false; } }
  for (const page of value.manual) {
    if (!validPage(page) || manual.has(page)) { return false; }
    manual.add(page);
  }
  return true;
}
export function moveRelative(values: string[], source: string, target: string, after: boolean): string[] {
  const result = values.slice();
  if (source === target || !result.includes(source) || !result.includes(target)) { return result; }
  result.splice(result.indexOf(source), 1);
  result.splice(result.indexOf(target) + (after ? 1 : 0), 0, source); return result;
}

export class CardOrderPreferences {
  private read: () => Promise<string | undefined>;
  private write: (serialized: string) => Promise<void>;
  private changed: () => void;
  private state: CardOrderSnapshot = new CardOrderSnapshot();
  private activeView: CardOrderView | undefined = undefined;
  private currentRevision: number = 0;
  private isLoading: boolean = false;
  private isBusy: boolean = false;
  private isDisposed: boolean = false;
  private currentError: string = '';
  constructor(read: () => Promise<string | undefined>, write: (serialized: string) => Promise<void>,
    changed: () => void) { this.read = read; this.write = write; this.changed = changed; }
  get revision(): number { return this.currentRevision; }
  get loading(): boolean { return this.isLoading; }
  get busy(): boolean { return this.isBusy; }
  get disposed(): boolean { return this.isDisposed; }
  get error(): string { return this.currentError; }
  get snapshot(): CardOrderSnapshot { return copySnapshot(this.state); }
  get view(): CardOrderView | undefined {
    if (!this.activeView) { return undefined; }
    const result = new CardOrderView(); result.page = this.activeView.page;
    result.all = this.activeView.all.slice(); result.visible = this.activeView.visible.slice();
    result.token = this.activeView.token; return result;
  }
  manual(page: string): boolean { return this.state.manual.includes(page); }
  order(page: string, ids: string[]): string[] {
    // Like Dart ids.toSet(), keep each incoming identity only once and append
    // new identities after surviving saved entries. Reading does not prune.
    const remaining = new Set<string>(ids), result: string[] = [];
    const saved = validPage(page) ? this.state.orders[page] ?? [] : [];
    for (const id of saved) {
      if (remaining.delete(id)) { result.push(id); }
    }
    remaining.forEach((id: string): void => { result.push(id); }); return result;
  }
  // Root supplies the whole page/filter/query/sort/content/query-state token.
  // Owning both identity arrays prevents a drag candidate changing in place.
  setView(page: string, all: string[], visible: string[], token: string): void {
    if (this.isDisposed) { return; }
    if (page.length === 0) { this.activeView = undefined; return; }
    if (!validPage(page) || !validView(all, visible)) { throw new Error('Card order view is invalid'); }
    const view = new CardOrderView(); view.page = page; view.all = all.slice(); view.visible = visible.slice();
    view.token = token; this.activeView = view;
  }
  async restore(): Promise<void> {
    if (this.isDisposed || this.isLoading || this.isBusy) { return; }
    this.isLoading = true; this.notify();
    try {
      const raw = await this.read();
      if (this.isDisposed) { return; }
      if (raw !== undefined) {
        const candidate = JSON.parse(raw) as CardOrderSnapshot;
        if (!validSnapshot(candidate)) { throw new Error('Card order preferences are invalid'); }
        this.state = copySnapshot(candidate);
      }
      this.currentError = '';
    } catch (error) { if (!this.isDisposed) { this.currentError = this.message(error); } }
    finally { this.isLoading = false; this.notify(); }
  }
  async select(page: string, enabled: boolean): Promise<boolean> {
    if (this.isDisposed || this.isBusy || this.isLoading || !validPage(page)) { return false; }
    if (this.manual(page) === enabled) { return true; }
    const candidate = copySnapshot(this.state);
    candidate.manual = candidate.manual.filter((id: string): boolean => id !== page);
    if (enabled) { candidate.manual.push(page); }
    return this.save(candidate);
  }
  async move(page: string, source: string, target: string, after: boolean, all: string[], visible: string[],
    expectedRevision?: number, expectedViewToken?: string): Promise<boolean> {
    if (this.isDisposed || this.isBusy || this.isLoading || !validPage(page) || !validView(all, visible) ||
      source === target || !visible.includes(source) || !visible.includes(target) ||
      (expectedRevision !== undefined && expectedRevision !== this.currentRevision)) { return false; }
    if (this.activeView && (page !== this.activeView.page || !sameIds(all, this.activeView.all) ||
      !sameIds(visible, this.activeView.visible) ||
      (expectedViewToken !== undefined && expectedViewToken !== this.activeView.token))) { return false; }
    if (expectedViewToken !== undefined && (!this.activeView || expectedViewToken !== this.activeView.token)) { return false; }
    const full = this.order(page, all), changed = moveRelative(visible, source, target, after);
    const shown = new Set<string>(visible); let index = 0;
    // Replace only visible slots. Hidden cards retain their exact positions,
    // even when the displayed query order differs from the full stored order.
    const merged = full.map((id: string): string => shown.has(id) ? changed[index++] : id);
    const candidate = copySnapshot(this.state); candidate.orders[page] = merged;
    if (!candidate.manual.includes(page)) { candidate.manual.push(page); }
    return this.save(candidate);
  }
  dispose(): void { this.isDisposed = true; this.activeView = undefined; }
  private notify(): void {
    if (this.isDisposed) { return; }
    this.currentRevision++; this.changed();
  }
  private message(error: Error): string {
    const value = error instanceof Error ? error.message : String(error);
    return value.trim().length > 0 ? value : 'Card order could not be saved';
  }
  private async save(candidate: CardOrderSnapshot): Promise<boolean> {
    if (!validSnapshot(candidate)) { throw new Error('Card order preferences are invalid'); }
    const previous = this.state, owned = copySnapshot(candidate), serialized = JSON.stringify(owned);
    this.state = owned; this.isBusy = true; this.notify();
    try {
      await this.write(serialized); this.currentError = ''; return !this.isDisposed;
    } catch (error) {
      this.state = previous; this.currentError = this.message(error);
      if (error instanceof Error) { throw error; }
      throw new Error(this.currentError);
    } finally { this.isBusy = false; this.notify(); }
  }
}
