import { window } from '@kit.ArkUI';
import { common } from '@kit.AbilityKit';

export class FullscreenSnapshot {
  windowId: number = 0;
  layoutFullscreen: boolean = false;
  immersive: boolean = false;
  orientation: number = 0;
  statusBar: boolean = true;
  navigationBar: boolean = true;
}

export class FullscreenView {
  phase: string = 'idle';
  busy: boolean = false;
  // Keep the exit control available while a window restoration is unresolved.
  active: boolean = false;
  retained: boolean = false;
  error: string = '';
}

export interface FullscreenWindow {
  capture(): FullscreenSnapshot;
  setLayout(value: boolean): Promise<void>;
  setImmersive(value: boolean): Promise<void>;
  setBars(status: boolean, navigation: boolean): Promise<void>;
  setOrientation(value: number): Promise<void>;
}

class PlatformFullscreenWindow implements FullscreenWindow {
  private main: window.Window;
  constructor(main: window.Window) { this.main = main; }
  capture(): FullscreenSnapshot {
    const properties = this.main.getWindowProperties(), result = new FullscreenSnapshot();
    result.windowId = properties.id; result.layoutFullscreen = properties.isLayoutFullScreen;
    result.immersive = this.main.getImmersiveModeEnabledState(); result.orientation = this.main.getPreferredOrientation();
    // WindowKit has no getter for bar-enable flags. EntryAbility establishes
    // this app-owned baseline once, before it marks this exact window ready.
    result.statusBar = true; result.navigationBar = true; return result;
  }
  async setLayout(value: boolean): Promise<void> { await this.main.setWindowLayoutFullScreen(value); }
  async setImmersive(value: boolean): Promise<void> { this.main.setImmersiveModeEnabledState(value); }
  async setBars(status: boolean, navigation: boolean): Promise<void> {
    const names: Array<'status' | 'navigation'> = [];
    if (status) { names.push('status'); } if (navigation) { names.push('navigation'); }
    await this.main.setWindowSystemBarEnable(names);
  }
  async setOrientation(value: number): Promise<void> { await this.main.setPreferredOrientation(value as window.Orientation); }
}

export async function createFullscreenWindow(context: common.UIAbilityContext): Promise<FullscreenWindow> {
  if (AppStorage.get<boolean>('hmos-fullscreen-policy-ready') !== true) { throw new Error('窗口尚未准备好，请稍后重试。'); }
  const id = AppStorage.get<number>('hmos-fullscreen-window-id');
  const main = await window.getLastWindow(context);
  if (main.getWindowProperties().id !== id || AppStorage.get<boolean>('hmos-fullscreen-policy-ready') !== true) {
    throw new Error('应用窗口已变化，请重新打开预览。');
  }
  return new PlatformFullscreenWindow(main);
}

export class AttachmentFullscreen {
  private factory: () => Promise<FullscreenWindow>;
  private landscape: number;
  private changed: () => void;
  private state: FullscreenView = new FullscreenView();
  private queue: Promise<void> = Promise.resolve();
  private operation: number = 0;
  private desired: boolean = false;
  private port: FullscreenWindow | undefined = undefined;
  private original: FullscreenSnapshot | undefined = undefined;
  constructor(factory: () => Promise<FullscreenWindow>, landscape: number, changed: () => void) {
    this.factory = factory; this.landscape = landscape; this.changed = changed;
  }
  view(): FullscreenView {
    const value = new FullscreenView(); value.phase = this.state.phase; value.busy = this.state.busy;
    value.active = this.state.active; value.retained = this.state.retained; value.error = this.state.error; return value;
  }
  snapshot(): FullscreenSnapshot | undefined {
    return this.original ? this.copySnapshot(this.original) : undefined;
  }
  private copySnapshot(value: FullscreenSnapshot): FullscreenSnapshot {
    const result = new FullscreenSnapshot(); result.windowId = value.windowId; result.layoutFullscreen = value.layoutFullscreen;
    result.immersive = value.immersive; result.orientation = value.orientation;
    result.statusBar = value.statusBar; result.navigationBar = value.navigationBar; return result;
  }
  private publish(phase: string, busy: boolean, error: string = ''): void {
    this.state.phase = phase; this.state.busy = busy; this.state.retained = this.original !== undefined;
    this.state.active = this.original !== undefined; this.state.error = error; this.changed();
  }

  set(enabled: boolean): Promise<void> {
    this.desired = enabled; const operation = ++this.operation;
    const next = this.queue.then(async (): Promise<void> => {
      if (enabled) { await this.enter(operation); } else { await this.restore(); }
    });
    this.queue = next.catch((): void => {});
    return next;
  }
  toggle(): Promise<void> {
    // A retained failed restoration is an exit obligation, not permission to
    // start another fullscreen session or recapture the changed window.
    return this.set(this.state.phase === 'restore_failed' ? false : !this.desired);
  }
  exit(): Promise<void> { return this.set(false); }

  private async enter(operation: number): Promise<void> {
    if (operation !== this.operation || !this.desired) { return; }
    if (this.original) {
      if (this.state.phase === 'fullscreen') { return; }
      throw new Error('全屏布局尚未恢复，请先重试退出。');
    }
    this.publish('entering', true);
    try {
      const port = await this.factory();
      if (operation !== this.operation || !this.desired) { this.publish('idle', false); return; }
      const snapshot = port.capture();
      if (!Number.isSafeInteger(snapshot.windowId) || snapshot.windowId <= 0 ||
        !Number.isSafeInteger(snapshot.orientation) || snapshot.orientation < 0 ||
        typeof snapshot.layoutFullscreen !== 'boolean' || typeof snapshot.immersive !== 'boolean' ||
        typeof snapshot.statusBar !== 'boolean' || typeof snapshot.navigationBar !== 'boolean') { throw new Error('窗口状态无效'); }
      this.port = port; this.original = this.copySnapshot(snapshot); this.publish('entering', true);
      // Retain the baseline before each first mutation: a rejected platform
      // promise may still have changed the window and requires restoration.
      await port.setLayout(true);
      if (await this.cancelled(operation)) { return; }
      await port.setImmersive(true);
      if (await this.cancelled(operation)) { return; }
      await port.setBars(false, false);
      if (await this.cancelled(operation)) { return; }
      await port.setOrientation(this.landscape);
      if (await this.cancelled(operation)) { return; }
      this.publish('fullscreen', false);
    } catch (_) {
      if (operation === this.operation) { this.desired = false; }
      if (this.state.phase === 'restore_failed') { throw new Error('全屏布局尚未恢复，请重试退出。'); }
      if (this.original) {
        await this.restore();
        this.publish('enter_failed', false, '无法进入全屏，已恢复原布局。');
      } else { this.publish('enter_failed', false, '无法进入全屏，请重试。'); }
      throw new Error('无法进入全屏，请重试。');
    }
  }
  private async cancelled(operation: number): Promise<boolean> {
    if (operation === this.operation && this.desired) { return false; }
    await this.restore(); return true;
  }

  private async restore(): Promise<void> {
    const original = this.original, port = this.port;
    if (!original || !port) { this.publish('idle', false); return; }
    this.publish('restoring', true);
    const failures: string[] = [];
    // Attempt every independent restoration even if one API rejects. Keep the
    // original snapshot and same window for the next explicit retry.
    try { await port.setBars(original.statusBar, original.navigationBar); } catch (_) { failures.push('system bars'); }
    try { await port.setOrientation(original.orientation); } catch (_) { failures.push('orientation'); }
    try { await port.setLayout(original.layoutFullscreen); } catch (_) { failures.push('layout'); }
    // These two settings share semantics today, but reading both preserves a
    // non-fullscreen window whose requested immersive state was already true.
    try { await port.setImmersive(original.immersive); } catch (_) { failures.push('immersive'); }
    if (failures.length > 0) {
      this.publish('restore_failed', false, '全屏布局尚未恢复，请重试退出。');
      throw new Error('Window restoration failed: ' + failures.join(', '));
    }
    this.original = undefined; this.port = undefined; this.publish('idle', false);
  }
}
