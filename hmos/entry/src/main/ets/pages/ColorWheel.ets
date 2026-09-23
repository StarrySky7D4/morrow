export function hsvColor(h: number, s: number, v: number): string {
  const c = v * s; const x = c * (1 - Math.abs((h / 60) % 2 - 1)); const m = v - c;
  const rgb = h < 60 ? [c, x, 0] : h < 120 ? [x, c, 0] : h < 180 ? [0, c, x] : h < 240 ? [0, x, c] : h < 300 ? [x, 0, c] : [c, 0, x];
  return '#' + rgb.map((n: number) => Math.round((n + m) * 255).toString(16).padStart(2, '0')).join('');
}
@Component
export struct ColorWheel {
  @Prop @Watch('paint') color: string = '#7662BA';
  onPick: (color: string) => void = () => {};
  private ctx: CanvasRenderingContext2D = new CanvasRenderingContext2D(new RenderingContextSettings(true));
  private paint(): void {
    const c = this.ctx; c.clearRect(0, 0, 232, 232);
    for (let a = 0; a < 360; a += 2) {
      const gradient = c.createRadialGradient(116, 116, 0, 116, 116, 114);
      gradient.addColorStop(0, '#FFFFFF'); gradient.addColorStop(1, hsvColor(a, 1, 1)); c.fillStyle = gradient;
      c.beginPath(); c.moveTo(116, 116); c.arc(116, 116, 114, a * Math.PI / 180, (a + 2.5) * Math.PI / 180); c.closePath(); c.fill();
    }
    if (!/^#[0-9a-fA-F]{6}$/.test(this.color)) { return; }
    const n = parseInt(this.color.substring(1), 16); const r = (n >> 16 & 255) / 255; const g = (n >> 8 & 255) / 255; const b = (n & 255) / 255;
    const max = Math.max(r, g, b); const d = max - Math.min(r, g, b);
    let h = d === 0 ? 0 : max === r ? ((g - b) / d + 6) % 6 : max === g ? (b - r) / d + 2 : (r - g) / d + 4;
    h *= Math.PI / 3; const radius = max === 0 ? 0 : d / max * 114;
    c.beginPath(); c.arc(116 + Math.cos(h) * radius, 116 + Math.sin(h) * radius, 6, 0, Math.PI * 2);
    c.strokeStyle = '#302D43'; c.lineWidth = 3; c.stroke(); c.strokeStyle = '#FFFFFF'; c.lineWidth = 1.5; c.stroke();
  }
  build() {
    Canvas(this.ctx).width(232).height(232).onReady(() => this.paint()).onTouch((e: TouchEvent) => {
      if ((e.type !== TouchType.Down && e.type !== TouchType.Move) || e.touches.length === 0) { return; }
      const x = e.touches[0].x - 116; const y = e.touches[0].y - 116;
      const n = /^#[0-9a-fA-F]{6}$/.test(this.color) ? parseInt(this.color.substring(1), 16) : 0xFFFFFF;
      const v = Math.max(n >> 16 & 255, n >> 8 & 255, n & 255) / 255;
      this.onPick(hsvColor((Math.atan2(y, x) * 180 / Math.PI + 360) % 360, Math.min(1, Math.sqrt(x * x + y * y) / 114), v || 1));
    }).accessibilityText('色彩罗盘，可通过下方十六进制输入精确选色')
  }
}
