import { AppearanceSnapshot, MaterialChoice, normalizeMaterials, visualStyles } from './Appearance';

// HMOS-only UI preferences. This is not the Rust business Store or its schema.
export const appearancePreferenceName: string = 'studio-appearance';
export const appearancePreferenceKey: string = 'appearance-v1';
export type AppearancePreferenceState = 'not-loaded' | 'loading' | 'ready' | 'read-failed' | 'write-unknown';
export interface AppearancePreferencePort {
  // All live models for the same actual preference namespace must share this
  // identity. Tests with isolated backends use distinct controlled namespaces.
  namespace?: string;
  // undefined means a positively observed absent key, never a read exception,
  // a non-string stored value or an existing empty/corrupt value.
  read: () => Promise<string | undefined>;
  put: (raw: string) => Promise<void>;
  flush: () => Promise<void>;
}
export interface AppearancePreferenceOwner {
  owned: () => boolean;
  // Include page instance and foreground epoch. A late reply cannot rebase.
  owner: () => string;
  changed: () => void;
}
export class AppearancePreferenceView {
  state: AppearancePreferenceState = 'not-loaded';
  current: AppearanceSnapshot | undefined = undefined;
  candidate: AppearanceSnapshot | undefined = undefined;
  originalRaw: string | undefined = undefined;
  observedRaw: string | undefined = undefined;
  unknownRaw: string | undefined = undefined;
  busy: boolean = false;
  dirty: boolean = false;
  error: string = '';
  revision: number = 0;
  candidateRevision: number = 0;
}

class AppearanceNamespace {
  owner: AppearancePreferences | undefined = undefined;
  models: Set<AppearancePreferences> = new Set<AppearancePreferences>();
  original: string | undefined = undefined;
  current: AppearanceSnapshot | undefined = undefined;
  unknown: string | undefined = undefined;
}
// A component's destruction does not cancel an ArkData Promise. Keep the live
// owner until that actual call terminates, and retain its fixed unknown bytes
// for the next page. This is an in-process barrier, not a business database.
const namespaces: Map<string, AppearanceNamespace> = new Map<string, AppearanceNamespace>();

const MAX_BYTES: number = 4 * 1024 * 1024;
const knownKeys: string[] = ['theme', 'mode', 'opacity', 'radius', 'accent', 'grayscale', 'lightness',
  'background', 'solid', 'canvasLiquid', 'canvasBlur', 'canvasOpacity', 'font', 'locale', 'daily',
  'materials', 'visualStyle', 'styleDepth'];
// The first HMOS snapshot did not have the three later material/style fields.
const legacyOptional: string[] = ['materials', 'visualStyle', 'styleDepth'];
const materialKeys: string[] = ['id', 'enabled', 'mode', 'blur', 'opacity', 'radius', 'color', 'follow', 'depth'];

class JsonField { key: string = ''; raw: string = ''; }
function bytes(text: string): number {
  let size = 0;
  for (let i = 0; i < text.length; i++) {
    const c = text.charCodeAt(i);
    if (c >= 0xD800 && c <= 0xDBFF) {
      const low = text.charCodeAt(++i);
      if (!(low >= 0xDC00 && low <= 0xDFFF)) { throw new Error('Invalid preference Unicode'); }
      size += 4;
    } else if (c >= 0xDC00 && c <= 0xDFFF) { throw new Error('Invalid preference Unicode'); }
    else { size += c < 0x80 ? 1 : c < 0x800 ? 2 : 3; }
  }
  return size;
}
function budget(raw: string): void {
  if (typeof raw !== 'string' || raw.length === 0 || raw.length > MAX_BYTES || bytes(raw) > MAX_BYTES) {
    throw new Error('Appearance preference budget');
  }
}
function scalar(value: string, max: number): boolean {
  return typeof value === 'string' && value.length <= max && bytes(value) <= max * 4;
}
function color(value: string): boolean { return value === '' || typeof value === 'string' && /^#[0-9a-fA-F]{6}$/.test(value); }
function finite(value: number, min: number, max: number): boolean {
  return typeof value === 'number' && Number.isFinite(value) && value >= min && value <= max;
}
function white(raw: string, index: number): number {
  while (index < raw.length && /\s/.test(raw[index])) { index++; } return index;
}
function stringEnd(raw: string, start: number): number {
  let index = start + 1;
  while (index < raw.length) {
    if (raw[index] === '\\') { index += 2; }
    else if (raw[index++] === '"') { return index; }
  }
  throw new Error('Invalid preference JSON string');
}
// JSON.parse checks grammar; spans keep all unknown values literally intact,
// including large integers, nested unknown fields and escaped property names.
// Re-serializing unknown objects with JS numbers would lose that information.
function valueEnd(raw: string, start: number): number {
  let index = start, depth = 0;
  while (index < raw.length) {
    const c = raw[index];
    if (c === '"') { index = stringEnd(raw, index); continue; }
    if (c === '[' || c === '{') { depth++; }
    else if (c === ']' || c === '}') { if (depth === 0) { break; } depth--; }
    else if (c === ',' && depth === 0) { break; }
    index++;
  }
  return index;
}
function fields(raw: string): JsonField[] {
  const parsed = JSON.parse(raw) as AppearanceSnapshot;
  if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) { throw new Error('Appearance object required'); }
  const result: JsonField[] = [], seen = new Set<string>(); let index = white(raw, 0) + 1;
  while (true) {
    index = white(raw, index); if (raw[index] === '}') { break; }
    const end = stringEnd(raw, index); const key = JSON.parse(raw.substring(index, end)) as string;
    if (seen.has(key)) { throw new Error('Duplicate appearance field'); } seen.add(key);
    index = white(raw, end) + 1; index = white(raw, index);
    const stop = valueEnd(raw, index); const field = new JsonField(); field.key = key; field.raw = raw.substring(index, stop).trim();
    result.push(field); index = white(raw, stop); if (raw[index] === ',') { index++; } else { break; }
  }
  return result;
}
function values(raw: string): string[] {
  if (!Array.isArray(JSON.parse(raw))) { throw new Error('Material array required'); }
  const result: string[] = []; let index = white(raw, 0) + 1;
  while (true) {
    index = white(raw, index); if (raw[index] === ']') { break; }
    const stop = valueEnd(raw, index); result.push(raw.substring(index, stop).trim());
    index = white(raw, stop); if (raw[index] === ',') { index++; } else { break; }
  }
  return result;
}
function fieldValue(input: JsonField[], key: string): string | undefined {
  return input.find((value: JsonField): boolean => value.key === key)?.raw;
}
function copy(value: AppearanceSnapshot): AppearanceSnapshot {
  const next = new AppearanceSnapshot();
  next.theme = value.theme; next.mode = value.mode; next.opacity = value.opacity; next.radius = value.radius;
  next.accent = value.accent; next.grayscale = value.grayscale; next.lightness = value.lightness;
  next.background = value.background; next.solid = value.solid; next.canvasLiquid = value.canvasLiquid;
  next.canvasBlur = value.canvasBlur; next.canvasOpacity = value.canvasOpacity; next.font = value.font;
  next.locale = value.locale; next.daily = value.daily.slice(); next.materials = normalizeMaterials(value.materials);
  next.visualStyle = value.visualStyle; next.styleDepth = value.styleDepth; return next;
}
function checked(value: AppearanceSnapshot): AppearanceSnapshot {
  if (!value || typeof value !== 'object' || Array.isArray(value) ||
    !['白色', '自定义', '深色'].includes(value.theme) || !['磨砂', '超透', '液体玻璃'].includes(value.mode) ||
    !finite(value.opacity, 20, 100) || !finite(value.radius, 0, 32) || !color(value.accent) ||
    !finite(value.grayscale, 0, 100) || !finite(value.lightness, 0, 100) ||
    !['默认', '纯色', '纹理', '透明'].includes(value.background) || !color(value.solid) ||
    typeof value.canvasLiquid !== 'boolean' || !finite(value.canvasBlur, 0, 40) || !finite(value.canvasOpacity, 0, 100) ||
    !scalar(value.font, 512) || !['zh', 'en', 'ja', 'ko', 'de', 'fr', 'es', 'pt', 'ru'].includes(value.locale) ||
    !Array.isArray(value.daily) || value.daily.length > 4096 ||
    !visualStyles.includes(value.visualStyle) || !finite(value.styleDepth, 0, 2)) { throw new Error('Invalid appearance preferences'); }
  const daily = new Set<string>();
  for (const item of value.daily) {
    if (!scalar(item, 4096) || item.length === 0 || daily.has(item)) { throw new Error('Invalid daily preference'); }
    daily.add(item);
  }
  // Existing material validator supplies legacy follow/depth defaults, numeric
  // bounds, unique stable ids and cycle checks. It returns owned copies.
  const next = copy(value); budget(JSON.stringify(next)); return next;
}
export function decodeAppearancePreferences(raw: string | undefined): AppearanceSnapshot {
  if (raw === undefined) { return new AppearanceSnapshot(); }
  budget(raw); const input = fields(raw); const value = JSON.parse(raw) as AppearanceSnapshot;
  for (const key of knownKeys) {
    if (!legacyOptional.includes(key) && fieldValue(input, key) === undefined) { throw new Error('Incomplete appearance preferences'); }
  }
  if (fieldValue(input, 'materials') === undefined) { value.materials = []; }
  if (fieldValue(input, 'visualStyle') === undefined) { value.visualStyle = 'flat'; }
  if (fieldValue(input, 'styleDepth') === undefined) { value.styleDepth = 1; }
  // Material unknown fields are retained later by stable id. Reject duplicate
  // known/unknown keys inside each edited object before normalizeMaterials.
  if (Array.isArray(value.materials)) {
    const materialRaw = fieldValue(input, 'materials');
    if (materialRaw !== undefined) { for (const rawMaterial of values(materialRaw)) { fields(rawMaterial); } }
  }
  return checked(value);
}
export function validateAppearanceCandidate(value: AppearanceSnapshot): AppearanceSnapshot {
  // No missing candidate field can be replaced by a default accidentally.
  const keys = Object.keys(value);
  if (keys.length !== knownKeys.length || knownKeys.some((key: string): boolean => !keys.includes(key))) {
    throw new Error('Incomplete appearance candidate');
  }
  const result = checked(value);
  for (const material of result.materials) {
    // Historic optional defaults apply only while decoding old records. A UI
    // candidate must explicitly own every field, including follow and depth.
    const original = value.materials.find((item: MaterialChoice): boolean => item.id === material.id)!;
    const keys = Object.keys(original);
    if (keys.length !== materialKeys.length || materialKeys.some((key: string): boolean => !keys.includes(key))) {
      throw new Error('Incomplete material candidate');
    }
  }
  return result;
}
function mergeFields(previous: string, next: string, allowed: string[]): string {
  const prior = fields(previous), replacement = fields(next), written = new Set<string>(), output: string[] = [];
  for (const field of prior) {
    const candidate = allowed.includes(field.key) ? fieldValue(replacement, field.key) : undefined;
    output.push(JSON.stringify(field.key) + ':' + (candidate === undefined ? field.raw : candidate)); written.add(field.key);
  }
  for (const field of replacement) {
    if (allowed.includes(field.key) && !written.has(field.key)) { output.push(JSON.stringify(field.key) + ':' + field.raw); }
  }
  return '{' + output.join(',') + '}';
}
export function encodeAppearancePreferences(value: AppearanceSnapshot, previous: string | undefined): string {
  const next = validateAppearanceCandidate(value); decodeAppearancePreferences(previous);
  const prior = previous === undefined ? '{}' : previous; const oldFields = fields(prior);
  const oldMaterials = fieldValue(oldFields, 'materials'); const retained = new Map<string, string>();
  if (oldMaterials !== undefined) {
    for (const materialRaw of values(oldMaterials)) {
      const material = JSON.parse(materialRaw) as MaterialChoice; retained.set(material.id, materialRaw);
    }
  }
  const materialJson: string[] = [];
  for (const material of next.materials) {
    materialJson.push(mergeFields(retained.get(material.id) ?? '{}', JSON.stringify(material), materialKeys));
  }
  const serialized = fields(JSON.stringify(next));
  const materialField = serialized.find((field: JsonField): boolean => field.key === 'materials')!;
  materialField.raw = '[' + materialJson.join(',') + ']';
  const replacement = '{' + serialized.map((field: JsonField): string => JSON.stringify(field.key) + ':' + field.raw).join(',') + '}';
  const result = mergeFields(prior, replacement, knownKeys); budget(result); decodeAppearancePreferences(result); return result;
}

export class AppearancePreferences {
  private port: AppearancePreferencePort;
  private hooks: AppearancePreferenceOwner;
  private state: AppearancePreferenceState = 'not-loaded';
  private currentValue: AppearanceSnapshot | undefined = undefined;
  private candidateValue: AppearanceSnapshot | undefined = undefined;
  private original: string | undefined = undefined;
  private observed: string | undefined = undefined;
  private unknown: string | undefined = undefined;
  private error: string = '';
  private revision: number = 0;
  private candidateRevision: number = 0;
  private pending: number = 0;
  private loading: boolean = false;
  private disposed: boolean = false;
  private queue: Promise<void> = Promise.resolve();
  private shared: AppearanceNamespace;
  constructor(port: AppearancePreferencePort, owner: AppearancePreferenceOwner) {
    this.port = port; this.hooks = owner;
    const namespace = port.namespace ?? appearancePreferenceName;
    if (!namespace || namespace.length > 512) { throw new Error('Invalid appearance namespace'); }
    let shared = namespaces.get(namespace);
    if (!shared) { shared = new AppearanceNamespace(); namespaces.set(namespace, shared); }
    this.shared = shared; this.shared.models.add(this); this.acceptShared();
  }
  private acceptShared(): void {
    if (this.shared.owner === this) { return; }
    if (this.shared.unknown !== undefined) {
      if (this.unknown === undefined) {
        this.original = this.shared.original;
        this.currentValue = this.shared.current ? copy(this.shared.current) : undefined;
        this.unknown = this.shared.unknown;
      }
      this.state = 'write-unknown'; this.error = 'Earlier appearance write requires explicit reconciliation';
    } else if (this.shared.owner !== undefined && this.state === 'not-loaded') {
      this.state = 'read-failed'; this.error = 'Earlier appearance operation is still live';
    }
  }
  private shareChanged(): void {
    this.shared.models.forEach((model: AppearancePreferences): void => {
      model.acceptShared();
      // The initiating operation already has a captured owner. Never rebase
      // its late completion to its current foreground token. Other pages are
      // observing a namespace barrier change, not adopting its response.
      if (model !== this) { model.notify(model.hooks.owner()); }
    });
  }
  private reserve(): boolean {
    if (this.shared.owner !== undefined) { this.acceptShared(); return false; }
    this.shared.owner = this; this.shareChanged(); return true;
  }
  private release(): void {
    if (this.shared.owner === this) { this.shared.owner = undefined; this.shareChanged(); }
  }
  view(): AppearancePreferenceView {
    const view = new AppearancePreferenceView(); view.state = this.state;
    view.current = this.currentValue ? copy(this.currentValue) : undefined;
    view.candidate = this.candidateValue ? copy(this.candidateValue) : undefined;
    view.originalRaw = this.original; view.observedRaw = this.observed; view.unknownRaw = this.unknown;
    view.busy = this.loading || this.pending > 0 || this.shared.owner !== undefined; view.error = this.error; view.revision = this.revision;
    view.candidateRevision = this.candidateRevision;
    view.dirty = this.candidateValue !== undefined && (this.currentValue === undefined ||
      JSON.stringify(this.candidateValue) !== JSON.stringify(this.currentValue)); return view;
  }
  private owned(owner: string): boolean { return !this.disposed && this.hooks.owned() && this.hooks.owner() === owner; }
  private notify(owner: string): void { this.revision++; if (this.owned(owner)) { this.hooks.changed(); } }
  private message(error: Error): string { return error instanceof Error ? error.message : String(error); }
  async restore(): Promise<boolean> {
    this.acceptShared();
    if (this.shared.owner !== undefined || this.shared.unknown !== undefined) { this.notify(this.hooks.owner()); return false; }
    if (this.state !== 'not-loaded' || this.pending > 0 || this.loading || this.disposed || !this.hooks.owned()) { return false; }
    return this.read(false);
  }
  // Explicit action only. It never puts the candidate or rolls back old bytes.
  // Following an issued unknown write, flush ACK plus exact readback confirms
  // the currently observed value; cached read alone is not a durable ACK.
  async recover(): Promise<boolean> {
    this.acceptShared();
    if (!['read-failed', 'write-unknown'].includes(this.state) || this.pending > 0 || this.loading ||
      this.disposed || !this.hooks.owned() || this.shared.owner !== undefined) { return false; }
    return this.read(true);
  }
  private async read(recovery: boolean): Promise<boolean> {
    if (!this.reserve()) { return false; }
    const owner = this.hooks.owner(), priorState = this.state; this.loading = true; this.state = 'loading'; this.notify(owner);
    try {
      const raw = await this.port.read();
      if (!this.owned(owner)) { this.state = priorState; return false; }
      this.observed = raw; const value = decodeAppearancePreferences(raw);
      if (recovery && this.unknown !== undefined) {
        await this.port.flush();
        if (!this.owned(owner)) { this.state = 'write-unknown'; return false; }
        const readback = await this.port.read();
        if (!this.owned(owner)) { this.state = 'write-unknown'; return false; }
        this.observed = readback;
        if (readback !== raw) { throw new Error('Appearance changed during recovery'); }
      }
      this.currentValue = value; this.original = raw; this.unknown = undefined; this.state = 'ready'; this.error = '';
      this.shared.original = raw; this.shared.current = copy(value); this.shared.unknown = undefined;
      return true;
    } catch (error) {
      this.state = this.unknown !== undefined ? 'write-unknown' : 'read-failed'; this.error = this.message(error); return false;
    } finally { this.loading = false; this.release(); this.notify(owner); }
  }
  // The UI can preview a complete candidate in a blocked state; it remains
  // explicitly unsaved. No defaults are ever enqueued before successful read.
  async save(value: AppearanceSnapshot): Promise<boolean> {
    if (this.disposed || !this.hooks.owned()) { return false; }
    const owner = this.hooks.owner(), candidate = validateAppearanceCandidate(value);
    this.candidateValue = candidate; const revision = ++this.candidateRevision; this.notify(owner);
    this.acceptShared();
    if (this.state !== 'ready' || this.loading || this.shared.owner !== undefined && this.shared.owner !== this) {
      this.notify(owner); return false;
    }
    this.pending++; this.notify(owner);
    const job: Promise<boolean> = this.queue.then(async (): Promise<boolean> => {
      try { return await this.write(candidate, revision, owner); }
      finally { this.pending--; this.notify(owner); }
    });
    // Capture errors in this job only. A failed/unknown operation blocks every
    // later queued write; swallowing a tail rejection cannot unblock it.
    this.queue = job.then((): void => {}, (): void => {}); return job;
  }
  private async write(candidate: AppearanceSnapshot, revision: number, owner: string): Promise<boolean> {
    this.acceptShared();
    if (!this.owned(owner) || this.state !== 'ready') { return false; }
    if (!this.reserve()) { return false; }
    let issued = false;
    try {
      const raw = await this.port.read();
      if (!this.owned(owner)) { return false; }
      this.observed = raw;
      if (raw !== this.original) { throw new Error('Appearance original changed; reread required'); }
      const serialized = encodeAppearancePreferences(candidate, this.original);
      if (this.currentValue && JSON.stringify(candidate) === JSON.stringify(this.currentValue)) { return true; }
      this.unknown = serialized; issued = true;
      this.shared.original = this.original;
      this.shared.current = this.currentValue ? copy(this.currentValue) : undefined;
      this.shared.unknown = serialized; this.shareChanged();
      await this.port.put(serialized);
      if (!this.owned(owner)) { this.state = 'write-unknown'; this.error = 'Appearance write owner changed'; return false; }
      await this.port.flush();
      if (!this.owned(owner)) { this.state = 'write-unknown'; this.error = 'Appearance flush owner changed'; return false; }
      const readback = await this.port.read();
      if (!this.owned(owner)) { this.state = 'write-unknown'; this.error = 'Appearance readback owner changed'; return false; }
      this.observed = readback;
      if (readback !== serialized) { throw new Error('Appearance write readback differs'); }
      this.original = serialized; this.currentValue = copy(candidate); this.unknown = undefined; this.error = '';
      this.shared.original = serialized; this.shared.current = copy(candidate); this.shared.unknown = undefined;
      // candidateValue belongs to the latest UI edit, even if this ACK is older.
      if (revision === this.candidateRevision) { this.candidateValue = copy(candidate); }
      return true;
    } catch (error) {
      this.state = issued ? 'write-unknown' : 'read-failed'; this.error = this.message(error); return false;
    } finally { this.release(); }
  }
  dispose(): void { this.disposed = true; this.shared.models.delete(this); }
}

export function appearancePreferenceStatus(view: AppearancePreferenceView): string {
  if (view.state === 'not-loaded') { return '外观设置尚未读取，当前改动未保存。'; }
  if (view.state === 'loading') { return '正在读取并核对外观设置。'; }
  if (view.state === 'read-failed') { return '外观设置未能读取，原数据仍保留；当前改动不会覆盖它。'; }
  if (view.state === 'write-unknown') { return '外观保存结果尚未确认，原值和候选值均保留；请重新读取并核对。'; }
  if (view.busy) { return '外观设置正在保存。'; }
  return view.dirty ? '当前外观仅供预览，尚未保存。' : '✓ 外观设置已在本机确认。';
}
