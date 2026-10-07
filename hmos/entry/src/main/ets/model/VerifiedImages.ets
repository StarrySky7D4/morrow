// Read-only image leases belong to one exact source/pin request. Replacement,
// removal and disposal release their own token, including late completions.
export class ImageReadTarget {
  id: string = '';
  name: string = '';
  request: string = '';
  byte_length: string = '0';
}
export class ImageLease {
  token: string = '';
  path: string = '';
  uri: string = '';
}
export class VerifiedImageView {
  id: string = '';
  path: string = '';
  uri: string = '';
  phase: string = 'loading';
  error: string = '';
}
class ImageReadEntry {
  target: ImageReadTarget;
  view: VerifiedImageView = new VerifiedImageView();
  lease: ImageLease | undefined = undefined;
  constructor(target: ImageReadTarget) {
    this.target = target; this.view.id = target.id;
  }
}
export class VerifiedImages {
  private entries: ImageReadEntry[] = [];
  private running: boolean = false;
  private disposed: boolean = false;
  private read: (target: ImageReadTarget) => Promise<ImageLease>;
  private release: (token: string) => Promise<void>;
  private changed: () => void;
  constructor(read: (target: ImageReadTarget) => Promise<ImageLease>, release: (token: string) => Promise<void>, changed: () => void) {
    this.read = read; this.release = release; this.changed = changed;
  }
  views(): VerifiedImageView[] {
    return this.entries.map((entry: ImageReadEntry): VerifiedImageView => {
      const view = new VerifiedImageView(); view.id = entry.view.id; view.path = entry.view.path;
      view.uri = entry.view.uri;
      view.phase = entry.view.phase; view.error = entry.view.error; return view;
    });
  }
  update(targets: ImageReadTarget[]): void {
    if (this.disposed) { return; }
    const next: ImageReadEntry[] = [];
    for (const target of targets) {
      if (next.some((entry: ImageReadEntry): boolean => entry.target.id === target.id)) { continue; }
      const prior = this.entries.find((entry: ImageReadEntry): boolean => entry.target.id === target.id &&
        entry.target.request === target.request && entry.target.byte_length === target.byte_length && entry.target.name === target.name);
      const frozen = new ImageReadTarget(); frozen.id = target.id; frozen.name = target.name;
      frozen.request = target.request; frozen.byte_length = target.byte_length;
      next.push(prior ?? new ImageReadEntry(frozen));
    }
    for (const entry of this.entries) { if (!next.includes(entry)) { this.releaseEntry(entry); } }
    this.entries = next; this.changed(); this.drain();
  }
  retry(id: string): void {
    const entry = this.entries.find((value: ImageReadEntry): boolean => value.target.id === id);
    if (this.disposed || !entry || entry.view.phase !== 'failed') { return; }
    this.releaseEntry(entry); entry.view.phase = 'loading'; entry.view.error = ''; this.changed(); this.drain();
  }
  decodeFailed(id: string, path: string): void {
    const entry = this.entries.find((value: ImageReadEntry): boolean => value.target.id === id);
    if (this.disposed || !entry || entry.view.phase !== 'ready' || entry.view.path !== path) { return; }
    this.releaseEntry(entry); entry.view.phase = 'failed'; entry.view.error = '图片无法解码'; this.changed();
  }
  dispose(): void {
    if (this.disposed) { return; }
    this.disposed = true; this.entries.forEach((entry: ImageReadEntry): void => this.releaseEntry(entry)); this.entries = [];
  }
  private releaseEntry(entry: ImageReadEntry): void {
    const lease = entry.lease; entry.lease = undefined; entry.view.path = ''; entry.view.uri = '';
    if (lease) { this.release(lease.token).catch((): void => {}); }
  }
  private async drain(): Promise<void> {
    if (this.running || this.disposed) { return; }
    const entry = this.entries.find((value: ImageReadEntry): boolean => value.view.phase === 'loading');
    if (!entry) { return; }
    this.running = true; entry.view.phase = 'reading'; this.changed();
    try {
      const lease = await this.read(entry.target);
      if (this.disposed || !this.entries.includes(entry)) { await this.release(lease.token); return; }
      if (!lease.token || !lease.path || !lease.uri) { if (lease.token) { await this.release(lease.token); } throw new Error('图片读取回复无效'); }
      entry.lease = lease; entry.view.path = lease.path; entry.view.uri = lease.uri; entry.view.phase = 'ready'; entry.view.error = ''; this.changed();
    } catch (error) {
      if (!this.disposed && this.entries.includes(entry)) {
        entry.view.phase = 'failed'; entry.view.error = error instanceof Error ? error.message : String(error); this.changed();
      }
    } finally { this.running = false; this.drain(); }
  }
}
