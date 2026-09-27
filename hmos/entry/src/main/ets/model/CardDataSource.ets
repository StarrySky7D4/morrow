import { CardView } from './Workbench';

@Observed
export class WorkspaceCard {
  key: string;
  card: CardView;
  constructor(page: string, card: CardView) { this.key = page + ':' + card.id; this.card = card; }
}

// UI virtualization only. The Rust query limit and authorization are unchanged.
export class CardDataSource implements IDataSource {
  private items: WorkspaceCard[] = [];
  private listeners: DataChangeListener[] = [];
  totalCount(): number { return this.items.length; }
  getData(index: number): WorkspaceCard { return this.items[index]; }
  registerDataChangeListener(listener: DataChangeListener): void {
    if (!this.listeners.includes(listener)) { this.listeners.push(listener); }
  }
  unregisterDataChangeListener(listener: DataChangeListener): void {
    this.listeners = this.listeners.filter((entry: DataChangeListener) => entry !== listener);
  }
  replace(page: string, cards: CardView[]): void {
    const oldRevisions: Map<string, string> = new Map();
    const oldKeys = this.items.map((item: WorkspaceCard) => item.key);
    const retained: Map<string, WorkspaceCard> = new Map();
    this.items.forEach((item: WorkspaceCard) => oldRevisions.set(item.key, item.card.revision));
    this.items.forEach((item: WorkspaceCard) => retained.set(item.key, item));
    const next = cards.map((card: CardView): WorkspaceCard => {
      const item = retained.get(page + ':' + card.id);
      if (!item) { return new WorkspaceCard(page, card); }
      if (item.card.revision !== card.revision) { item.card = card; }
      return item;
    });
    const reordered = next.length !== this.items.length || next.some((item: WorkspaceCard, index: number) => item.key !== this.items[index].key);
    this.items = next;
    if (reordered) { this.listeners.forEach((listener: DataChangeListener) => listener.onDataReloaded()); }
    // Reload retains keyed nodes but the native layout also caches sizes by
    // position. Invalidate moved indexes as well as changed content, otherwise
    // mixed-height cards can overlap after sorting/filtering.
    next.forEach((item: WorkspaceCard, index: number) => {
      if (oldKeys[index] !== item.key || (oldRevisions.has(item.key) && oldRevisions.get(item.key) !== item.card.revision)) {
        this.listeners.forEach((listener: DataChangeListener) => listener.onDataChange(index));
      }
    });
  }
}
