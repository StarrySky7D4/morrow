import { abilityDelegatorRegistry, TestRunner, Driver, PointerMatrix, ON, Component, Rect } from '@kit.TestKit';
import { fileIo as fs } from '@kit.CoreFileKit';
import { ImagePointerPlan, imagePointerPlan, imageZoomPercent, sameImageBounds } from './ImagePointerPlan';

const PRODUCT_BUNDLE: string = 'dev.morrow.hmos';

class ProbeObservation {
  versionCode: number = 0;
  draftTitle: string = '';
  imageName: string = '';
  imageDescription: string = '';
  zoomText: string = '';
  zoomPercent: number = 0;
  zoomEnabled: boolean = false;
  imageReadiness: string = 'UNKNOWN';
  readinessMarkers: string[] = [];
  imageBoundsSemantics: string = 'UITEST_COMPONENT_RECT_NOT_RENDERED_PIXEL_TRANSFORM';
  windowFocused: boolean = false;
  canvas: Rect = { left: 0, top: 0, right: 0, bottom: 0 };
  image: Rect = { left: 0, top: 0, right: 0, bottom: 0 };
  capturedAt: number = 0;
}

class ProbeReport {
  status: string = 'NOT_RUN';
  reason: string = '';
  operation: string = '';
  runLabel: string = '';
  productBundle: string = PRODUCT_BUNDLE;
  expectedVersionCode: number = 0;
  expectedDraftTitle: string = '';
  expectedImageName: string = '';
  expectedToken: string = '';
  tokenStatus: string = 'NOT_EXPOSED_BY_UI';
  expectedHapSha: string = '';
  hapShaStatus: string = 'HOST_DECLARED_NOT_READ_BACK';
  before: ProbeObservation | undefined = undefined;
  intent: ProbeObservation | undefined = undefined;
  after: ProbeObservation | undefined = undefined;
  plan: ImagePointerPlan | undefined = undefined;
  injectionAcknowledged: boolean = false;
  visualEffect: string = 'NOT_QUALIFIED';
  reportDirectory: string = '';
}

// This runner does not launch an ability or prepare a fixture. The root operator
// opens its own retained image first, then explicitly starts one named probe.
export default class ImageGestureRunner implements TestRunner {
  private stopped: boolean = false;
  private started: boolean = false;
  private driver: Driver | undefined = undefined;
  private report: ProbeReport = new ProbeReport();
  private expectedCanvas: Rect = { left: 0, top: 0, right: 0, bottom: 0 };

  onPrepare(): void {}
  onStop(): void { this.stopped = true; }

  private parameter(parameters: Record<string, string>, name: string): string {
    const value = parameters[name];
    if (!value || value.length > 512 || /[\r\n\0]/.test(value)) { throw new Error('Required exact parameter: ' + name); }
    return value;
  }

  private write(name: string, value: Object): void {
    const file = fs.openSync(this.report.reportDirectory + '/' + name,
      fs.OpenMode.WRITE_ONLY | fs.OpenMode.CREATE | fs.OpenMode.TRUNC);
    try { fs.writeSync(file.fd, JSON.stringify(value, null, 2)); fs.fsyncSync(file.fd); }
    finally { fs.closeSync(file); }
  }

  private checkStopped(): void { if (this.stopped) { throw new Error('Runner stopped; no next input is allowed'); } }

  private async unique(id: string): Promise<Component> {
    const values = await this.driver!.findComponents(ON.id(id).inWindow(PRODUCT_BUNDLE));
    if (values.length !== 1) { throw new Error('Current product component is not unique: ' + id); }
    return values[0];
  }

  private async checkProductWindow(): Promise<void> {
    this.checkStopped();
    const productWindow = await this.driver!.findWindow({ bundleName: PRODUCT_BUNDLE });
    if (await productWindow.getBundleName() !== PRODUCT_BUNDLE || !await productWindow.isFocused() || !await productWindow.isActive()) {
      throw new Error('The product window is not the active focused window; do not inject into another app or picker');
    }
    this.checkStopped();
  }

  private async observe(): Promise<ProbeObservation> {
    this.checkStopped();
    const delegator = abilityDelegatorRegistry.getAbilityDelegator();
    const version = await delegator.executeShellCommand('bm dump -n dev.morrow.hmos', 5);
    if (version.exitCode !== 0) { throw new Error('Cannot read the installed product version'); }
    const codes = version.stdResult.match(/"versionCode"\s*:\s*\d+/g);
    if (!codes || codes.length === 0 || codes.some((value: string): boolean =>
      Number(value.substring(value.indexOf(':') + 1).trim()) !== this.report.expectedVersionCode)) {
      throw new Error('Installed product version does not match the requested probe');
    }
    await this.checkProductWindow();
    const modal = await this.unique('attachment-preview-dialog');
    const names = await this.driver!.findComponents(ON.text(this.report.expectedImageName)
      .withinComponent(modal).inWindow(PRODUCT_BUNDLE));
    if (names.length === 0) { throw new Error('Exact image name is absent from the current preview dialog'); }
    const draft = await this.unique('draft-title');
    const title = await draft.getOriginalText();
    if (title !== this.report.expectedDraftTitle) { throw new Error('The current draft title belongs to another fixture'); }
    const canvas = await this.unique('attachment-image-gesture-canvas');
    const bounds = await canvas.getBounds();
    if (!sameImageBounds(bounds, this.expectedCanvas)) { throw new Error('Fresh canvas bounds differ from the supplied observation'); }
    const image = await this.unique('verified-attachment-image');
    const zoom = await this.unique('attachment-image-zoom-reset');
    const observation = new ProbeObservation(); observation.versionCode = this.report.expectedVersionCode;
    observation.windowFocused = true;
    observation.draftTitle = title; observation.imageName = this.report.expectedImageName;
    observation.imageDescription = await image.getDescription(); observation.image = await image.getBounds();
    if (observation.imageDescription !== this.report.expectedImageName) { throw new Error('Image component description differs from the exact preview fixture'); }
    observation.canvas = bounds; observation.zoomText = await zoom.getOriginalText();
    observation.zoomPercent = imageZoomPercent(observation.zoomText); observation.zoomEnabled = await zoom.isEnabled();
    for (const id of ['attachment-image-read-loading', 'attachment-image-read-failure', 'attachment-image-decode-loading', 'attachment-image-decode-failure']) {
      if ((await this.driver!.findComponents(ON.id(id).inWindow(PRODUCT_BUNDLE))).length > 0) { observation.readinessMarkers.push(id); }
    }
    observation.imageReadiness = observation.zoomEnabled && observation.readinessMarkers.length === 0 ? 'UI_ADMISSION_READY_REQUIRES_PIXEL_REVIEW' : 'UI_NOT_READY';
    if (this.report.operation !== 'observe' && observation.imageReadiness === 'UI_NOT_READY') {
      throw new Error('Decoded image admission is not ready; gestures will not be injected');
    }
    await this.checkProductWindow(); observation.capturedAt = Date.now();
    return observation;
  }

  private async capture(label: string): Promise<void> {
    this.checkStopped();
    const png = await this.driver!.screenCap(this.report.reportDirectory + '/' + label + '.png');
    const tree = await this.driver!.dumpLayout(this.report.reportDirectory + '/' + label + '.tree.json');
    if (!png || !tree) { throw new Error('Fresh screenshot/layout was not acknowledged: ' + label); }
  }

  private checkStart(operation: string, zoom: number): void {
    if ((operation === 'pinch-out' || operation === 'pinch-moving-focal') && zoom !== 100) {
      throw new Error('Zoom-outward probe requires an explicitly prepared 100% image; no reset is automatic');
    }
    if ((operation === 'pinch-in' || operation.indexOf('pan-') >= 0 || operation === 'double-tap-reset') && zoom <= 100) {
      throw new Error('This probe requires an already zoomed image; no pinch or reset is automatic');
    }
  }

  async onRun(): Promise<void> {
    const delegator = abilityDelegatorRegistry.getAbilityDelegator();
    if (this.started) {
      const repeat = new ProbeReport(); repeat.status = 'STOPPED'; repeat.reason = 'A TestRunner instance cannot replay its execution';
      await delegator.print(JSON.stringify(repeat)); await delegator.finishTest(JSON.stringify(repeat), 1); return;
    }
    this.started = true; let exitCode = 1;
    try {
      const parameters = abilityDelegatorRegistry.getArguments().parameters;
      this.report.operation = this.parameter(parameters, 'operation');
      this.report.runLabel = this.parameter(parameters, 'runLabel');
      if (!/^[A-Za-z0-9_-]{1,80}$/.test(this.report.runLabel)) { throw new Error('runLabel must be a fresh simple filename'); }
      this.report.expectedVersionCode = Number(this.parameter(parameters, 'expectVersionCode'));
      if (!Number.isSafeInteger(this.report.expectedVersionCode) || this.report.expectedVersionCode <= 0) { throw new Error('Invalid expected version'); }
      this.report.expectedDraftTitle = this.parameter(parameters, 'expectDraftTitle');
      this.report.expectedImageName = this.parameter(parameters, 'expectImageName');
      this.report.expectedToken = parameters['expectToken'] || '';
      this.report.expectedHapSha = parameters['expectHapSha'] || '';
      this.expectedCanvas = {
        left: Number(this.parameter(parameters, 'left')), top: Number(this.parameter(parameters, 'top')),
        right: Number(this.parameter(parameters, 'right')), bottom: Number(this.parameter(parameters, 'bottom'))
      };
      const base = delegator.getAppContext().filesDir + '/image-gestures';
      fs.mkdirSync(base, true);
      const runDirectory = base + '/' + this.report.runLabel;
      // Existing label throws before any UI input. Unknown runs are reconciled,
      // never replayed by rerunning the same test label.
      fs.mkdirSync(runDirectory);
      this.report.reportDirectory = runDirectory; this.driver = Driver.create();
      this.report.before = await this.observe(); await this.capture('before');
      this.write('before.observation.json', this.report.before);
      this.report.intent = await this.observe(); this.checkStart(this.report.operation, this.report.intent.zoomPercent);
      if (this.report.operation === 'observe') {
        this.report.status = 'READONLY_OBSERVED'; this.report.visualEffect = 'NOT_RUN'; exitCode = 0;
      } else if (this.report.operation === 'double-tap-reset') {
        await this.checkProductWindow();
        this.report.status = 'INPUT_INTENT'; this.write('input-intent.json', this.report); this.checkStopped();
        await this.driver.doubleClick(Math.round((this.report.intent.canvas.left + this.report.intent.canvas.right) / 2),
          Math.round((this.report.intent.canvas.top + this.report.intent.canvas.bottom) / 2));
        this.report.injectionAcknowledged = true;
      } else {
        this.report.plan = imagePointerPlan(this.report.operation, this.report.intent.canvas);
        const matrix = PointerMatrix.create(this.report.plan.fingers, this.report.plan.steps);
        for (let finger = 0; finger < this.report.plan.fingers; finger++) {
          for (let step = 0; step < this.report.plan.steps; step++) { matrix.setPoint(finger, step, this.report.plan.points[finger][step]); }
        }
        await this.checkProductWindow();
        this.report.status = 'INPUT_INTENT'; this.write('input-intent.json', this.report); this.checkStopped();
        this.report.injectionAcknowledged = await this.driver.injectMultiPointerAction(matrix, this.report.plan.speed);
        if (!this.report.injectionAcknowledged) { throw new Error('Multi-pointer provider did not acknowledge the gesture; do not replay'); }
      }
      if (this.report.operation !== 'observe') {
        await this.driver.delayMs(700); this.report.after = await this.observe(); await this.capture('after');
        this.write('after.observation.json', this.report.after);
        const before = this.report.intent.zoomPercent, after = this.report.after.zoomPercent;
        if ((this.report.operation === 'pinch-out' || this.report.operation === 'pinch-moving-focal') && after <= before) {
          throw new Error('Pointer action was acknowledged, but visible zoom percentage did not increase');
        }
        if (this.report.operation === 'pinch-in' && after >= before) { throw new Error('Visible zoom percentage did not decrease'); }
        if (this.report.operation === 'double-tap-reset' && after !== 100) { throw new Error('Double tap did not visibly reset the percentage'); }
        if (this.report.operation.indexOf('pan-') >= 0 && before !== after) { throw new Error('Pure pan unexpectedly changed the zoom percentage'); }
        this.report.status = 'ACKNOWLEDGED_REQUIRES_VISUAL_REVIEW';
        this.report.visualEffect = 'SCREENSHOTS_CAPTURED_NOT_AUTOMATICALLY_QUALIFIED'; exitCode = 0;
      }
    } catch (error) {
      this.report.reason = (error as Error).message || JSON.stringify(error);
      this.report.status = this.report.status === 'INPUT_INTENT' ? 'UNKNOWN_INPUT_EFFECT' : 'STOPPED';
    }
    if (this.report.reportDirectory) { this.write('result.json', this.report); }
    await delegator.print(JSON.stringify(this.report));
    await delegator.finishTest(JSON.stringify(this.report), exitCode);
  }
}
