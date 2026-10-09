import native from 'libmorrow.so';
import { DraftRecord } from './EditorDraft';
import { MarkdownDoc } from './Markdown';
import { AssetView, ImportRequest, ImportRecord } from './Attachments';
export interface TaskView { id: string; text: string; completion: number; }
export interface CardView {
  id: string; revision: string; source: string; title: string; description: string;
  content_kind?: string;
  hypothesis: string; conclusion: string; category: string; stage: string;
  favorite: boolean; deleted: boolean; deleted_at: string; tasks: TaskView[]; assets: AssetView[];
}
export class Command {
  action: string = 'list'; path: string = ''; operation: string = ''; id: string = '';
  source: string = ''; title: string = ''; description: string = ''; hypothesis: string = '';
  conclusion: string = ''; todos: string = ''; category: string = ''; stage: string = ''; task_id: string = '';
  text: string = ''; flag: boolean = false; now_ms: string = '';
  order: string[] = [];
  section: string = '概览'; filter: string = '全部'; sort: string = '最近添加';
  draft_id: string = ''; generation: string = '';
  draft_operation: string = ''; attachment_id: string = '';
  import_request?: ImportRequest;
}
export interface Reply {
  ok: boolean; error: string; cards: CardView[]; ids: string[]; drafts: DraftRecord[]; receipt_revision: string; profile: string; effect: string;
  markdown: MarkdownDoc; paste_text: string;
  imports: ImportRecord[];
}
// Single admission owner. No implicit mutation retry; native database work runs
// off the UI thread. The caller owns each immutable serialized proposal.
class Workbench {
  private tail: Promise<void> = Promise.resolve();
  private queue(operation: () => Promise<string>): Promise<string> {
    const result: Promise<string> = this.tail.then(operation);
    this.tail = result.then((): void => {}, (): void => {}); return result;
  }
  send(serialized: string): Promise<Reply> {
    return this.sendRaw(serialized).then((raw: string): Reply => JSON.parse(raw) as Reply);
  }
  sendRaw(serialized: string): Promise<string> { return this.queue((): Promise<string> => native.request(serialized)); }
  importFd(serialized: string, fd: number): Promise<string> { return this.queue((): Promise<string> => native.importFile(serialized, fd)); }
  exportFd(serialized: string, fd: number): Promise<string> { return this.queue((): Promise<string> => native.exportFile(serialized, fd)); }
}
export const workbench: Workbench = new Workbench();
