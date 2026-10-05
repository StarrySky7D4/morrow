// Mirrors Flutter QueryCoordinator's selection policy for the development
// adapter. Read requests are serialized and coalesced; issued work is not
// cancelled, and an obsolete reply can never publish into another view.
export class QueryConditions {
  generation: number = 0;
  section: string = '概览';
  filter: string = '全部';
  text: string = '';
  sort: string = '最近添加';
}
export class QueryState {
  phase: string = 'idle';
  ids: string[] = [];
  refreshing: boolean = false;
  error: string = '';
}
export class QueryCoordinator {
  private conditions: QueryConditions | undefined = undefined;
  private state: QueryState = new QueryState();
  private serial: number = 0;
  private timer: number = -1;
  private running: boolean = false;
  private due: boolean = false;
  private disposed: boolean = false;
  private request: (conditions: QueryConditions) => Promise<string[]>;
  private changed: (state: QueryState) => void;
  private debounce: number;
  constructor(request: (conditions: QueryConditions) => Promise<string[]>,
    changed: (state: QueryState) => void, debounce: number = 200) {
    this.request = request; this.changed = changed; this.debounce = debounce;
  }

  select(conditions: QueryConditions): void {
    if (this.disposed) { return; }
    const previous = this.conditions;
    const sameView = previous !== undefined && previous.section === conditions.section &&
      previous.filter === conditions.filter && previous.text === conditions.text && previous.sort === conditions.sort;
    if (sameView && previous !== undefined && previous.generation === conditions.generation) { return; }
    const retain = sameView && (this.state.phase === 'ready' || this.state.refreshing);
    const ids = retain ? this.state.ids : [];
    const typing = previous !== undefined && previous.text !== conditions.text;
    this.reset();
    // Own a snapshot so caller changes cannot alter an issued proposal.
    this.conditions = new QueryConditions();
    this.conditions.generation = conditions.generation;
    this.conditions.section = conditions.section; this.conditions.filter = conditions.filter;
    this.conditions.text = conditions.text; this.conditions.sort = conditions.sort;
    this.state.phase = 'loading'; this.state.ids = ids; this.state.refreshing = retain;
    this.changed(this.state);
    this.schedule(typing ? this.debounce : 0);
  }
  retry(): void {
    if (this.disposed || this.state.phase !== 'failed' || this.conditions === undefined) { return; }
    this.serial++; this.state = new QueryState(); this.state.phase = 'loading';
    this.changed(this.state); this.schedule(0);
  }
  invalidate(): void {
    this.reset(); this.changed(this.state);
  }
  dispose(): void {
    this.disposed = true; this.reset();
  }
  private reset(): void {
    this.serial++;
    if (this.timer !== -1) { clearTimeout(this.timer); this.timer = -1; }
    this.conditions = undefined; this.due = false; this.state = new QueryState();
  }
  private schedule(delay: number): void {
    this.timer = setTimeout(() => { this.timer = -1; this.due = true; this.drain(); }, delay);
  }
  private async drain(): Promise<void> {
    if (this.disposed || this.running || !this.due || this.conditions === undefined) { return; }
    const conditions = this.conditions, serial = this.serial;
    this.due = false; this.running = true;
    try {
      const ids = await this.request(conditions);
      if (!this.disposed && serial === this.serial) {
        this.state = new QueryState(); this.state.phase = 'ready'; this.state.ids = ids;
        this.changed(this.state);
      }
    } catch (error) {
      if (!this.disposed && serial === this.serial) {
        this.state = new QueryState(); this.state.phase = 'failed';
        this.state.error = String(error); this.changed(this.state);
      }
    } finally {
      this.running = false;
      if (!this.disposed) { this.drain(); }
    }
  }
}
