export class MaterialChoice {
  id: string = '';
  enabled: boolean = false;
  mode: string = '跟随主题';
  blur: number = 22;
  opacity: number = 76;
  radius: number = 20;
  color: string = '';
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
