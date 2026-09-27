export class MaterialChoice {
  id: string = '';
  enabled: boolean = false;
  mode: string = '跟随主题';
  blur: number = 22;
  opacity: number = 76;
  radius: number = 20;
  color: string = '';
  follow: string = '';
  depth: number = -1;
}
export class AppearanceSnapshot {
  theme: string = '白色';
  mode: string = '磨砂';
  opacity: number = 76;
  radius: number = 20;
  accent: string = '';
  grayscale: number = 0;
  lightness: number = 88.5;
  background: string = '默认';
  solid: string = '';
  canvasLiquid: boolean = false;
  canvasBlur: number = 0;
  canvasOpacity: number = 0;
  font: string = '';
  locale: string = 'zh';
  daily: string[] = [];
  materials: MaterialChoice[] = [];
  visualStyle: string = 'flat';
  styleDepth: number = 1;
}
export const visualStyles: string[] = ['flat', 'neumorphism', 'paper', 'clay', 'fluent', 'brutalist', 'industrial'];
export const visualStyleLabels: string[] = ['扁平 · 默认', 'Neumorphism · 新拟态', '纸感', '黏土', 'Fluent 流畅', '粗野主义', '工业风'];
export function styleRadiusScale(style: string): number {
  return [1, 1, .24, 1.3, .55, .18, .4][visualStyles.indexOf(style)] ?? 1;
}
export function resolveMaterial(materials: MaterialChoice[], id: string): MaterialChoice | undefined {
  const entries = new Map<string, MaterialChoice>(); for (const m of materials) { entries.set(m.id, m); }
  const visited = new Set<string>(); let current = id;
  while (current.length > 0) {
    if (visited.has(current)) { throw new Error('Component material cycle'); }
    visited.add(current);
    const material = entries.get(current);
    if (!material) { return undefined; }
    if (!material.follow) { return material.enabled ? material : undefined; }
    current = material.follow;
  }
  return undefined;
}
export function canFollow(materials: MaterialChoice[], id: string, target: string): boolean {
  const visited = new Set<string>([id]); let current = target;
  while (current.length > 0) {
    if (visited.has(current)) { return false; }
    visited.add(current); current = materials.find((m: MaterialChoice) => m.id === current)?.follow ?? '';
  }
  return true;
}
// Accept dev.3/4 fields with explicit defaults. Reject corrupt numbers and graphs
// before any preferences are applied; disabled links also participate in cycles.
export function normalizeMaterials(input: MaterialChoice[]): MaterialChoice[] {
  if (!Array.isArray(input) || input.length > 4096) { throw new Error('Material budget'); }
  const ids = new Set<string>(); const out: MaterialChoice[] = [];
  for (const old of input) {
    const v = new MaterialChoice();
    v.id = old.id; v.enabled = old.enabled; v.mode = old.mode; v.blur = old.blur;
    v.opacity = old.opacity; v.radius = old.radius; v.color = old.color;
    v.follow = old.follow ?? ''; v.depth = old.depth ?? -1;
    if (typeof v.id !== 'string' || !v.id || v.id.length > 256 || ids.has(v.id) ||
      typeof v.enabled !== 'boolean' || !['跟随主题', '磨砂', '超透', '液体玻璃'].includes(v.mode) ||
      !Number.isFinite(v.blur) || v.blur < 0 || v.blur > 40 ||
      !Number.isFinite(v.opacity) || v.opacity < 0 || v.opacity > 100 ||
      !Number.isFinite(v.radius) || v.radius < 0 || v.radius > 32 ||
      !Number.isFinite(v.depth) || (v.depth !== -1 && (v.depth < 0 || v.depth > 2)) ||
      typeof v.follow !== 'string' || v.follow.length > 256 ||
      (v.color !== '' && !/^#[0-9a-fA-F]{6}$/.test(v.color))) { throw new Error('Invalid material'); }
    ids.add(v.id); out.push(v);
  }
  const links = new Map<string, string>(); for (const m of out) { links.set(m.id, m.follow); }
  const checked = new Set<string>();
  for (const material of out) {
    const visiting = new Set<string>(); let current = material.id;
    while (current && !checked.has(current)) {
      if (visiting.has(current)) { throw new Error('Component material cycle'); }
      visiting.add(current); current = links.get(current) ?? '';
    }
    visiting.forEach((id: string) => { checked.add(id); });
  }
  return out;
}
export function mix(a: string, b: string, t: number): string {
  const aa = parseInt(a.substring(1), 16); const bb = parseInt(b.substring(1), 16);
  let result = '#';
  for (let shift = 16; shift >= 0; shift -= 8) {
    result += Math.round(((aa >> shift) & 255) * (1 - t) + ((bb >> shift) & 255) * t).toString(16).padStart(2, '0');
  }
  return result;
}
export function alpha(color: string, value: number): string {
  return '#' + Math.round(Math.max(0, Math.min(1, value)) * 255).toString(16).padStart(2, '0') + color.substring(1);
}
export function luminance(color: string): number {
  const n = parseInt(color.substring(1), 16); const channels: number[] = [];
  for (let shift = 16; shift >= 0; shift -= 8) {
    const c = ((n >> shift) & 255) / 255; channels.push(c <= .04045 ? c / 12.92 : Math.pow((c + .055) / 1.055, 2.4));
  }
  return channels[0] * .2126 + channels[1] * .7152 + channels[2] * .0722;
}
export function contrast(a: string, b: string): number {
  const x = luminance(a); const y = luminance(b); return (Math.max(x, y) + .05) / (Math.min(x, y) + .05);
}
