import native from 'libmorrow.so';
export interface TaskView { id: string; text: string; completion: number; }
export interface CardView {
  id: string; revision: string; source: string; title: string; description: string;
  hypothesis: string; conclusion: string; category: string; stage: string;
  favorite: boolean; deleted: boolean; deleted_at: string; tasks: TaskView[];
}
export class Command {
  action: string = 'list'; path: string = ''; operation: string = ''; id: string = '';
  source: string = ''; title: string = ''; description: string = ''; hypothesis: string = '';
  conclusion: string = ''; category: string = ''; stage: string = ''; task_id: string = '';
  text: string = ''; flag: boolean = false; now_ms: string = '';
}
export interface Reply {
  ok: boolean; error: string; cards: CardView[]; receipt_revision: string; profile: string; effect: string;
}
// Single admission owner. No implicit mutation retry; native database work runs
// off the UI thread. The caller owns each immutable serialized proposal.
class Workbench {
  private tail: Promise<void> = Promise.resolve();
  send(serialized: string): Promise<Reply> {
    const result: Promise<Reply> = this.tail.then(async (): Promise<Reply> => {
      return JSON.parse(await native.request(serialized)) as Reply;
    });
    this.tail = result.then((): void => {}, (): void => {});
    return result;
  }
}
export const workbench: Workbench = new Workbench();
