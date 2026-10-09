import { mix } from '../model/Appearance';

// Shared fill=false inset paint from Flutter NeumorphicSurface and
// ExperimentalSurfacePainter. The caller supplies its palette and signed
// widget depth: search -1 with styled Glass, music -.8 with global Appearance.
@Component
export struct RecessedGlassRelief {
  @Prop @Watch('draw') style: string = 'flat';
  @Prop @Watch('draw') depth: number = 1;
  @Prop @Watch('draw') depthMultiplier: number = -1;
  @Prop @Watch('draw') dark: boolean = false;
  @Prop @Watch('draw') surface: string = '#FFFFFF';
  @Prop @Watch('draw') radius: number = 14;
  private ctx: CanvasRenderingContext2D = new CanvasRenderingContext2D(new RenderingContextSettings(true));
  private resizeTimer: number = -1;
  private shape(inset: number = 0, dx: number = 0, dy: number = 0): Path2D {
    const c = this.ctx, x: number = inset + dx, y: number = inset + dy;
    const w: number = c.width - 2 * inset, h: number = c.height - 2 * inset;
    const radius: number = Number.isFinite(this.radius) ? this.radius : 0;
    const r: number = Math.max(0, Math.min(radius - inset, w / 2, h / 2));
    const path: Path2D = new Path2D();
    path.moveTo(x + r, y); path.lineTo(x + w - r, y);
    path.arcTo(x + w, y, x + w, y + r, r); path.lineTo(x + w, y + h - r);
    path.arcTo(x + w, y + h, x + w - r, y + h, r); path.lineTo(x + r, y + h);
    path.arcTo(x, y + h, x, y + h - r, r); path.lineTo(x, y + r);
    path.arcTo(x, y, x + r, y, r); path.closePath();
    return path;
  }
  private color(value: string, opacity: number): string {
    const rgb: number = parseInt(value.substring(1), 16);
    return `rgba(${(rgb >> 16) & 255},${(rgb >> 8) & 255},${rgb & 255},${opacity})`;
  }
  private stroke(inset: number, color: string, width: number): void {
    const c = this.ctx; c.strokeStyle = color; c.lineWidth = width; c.stroke(this.shape(inset));
  }
  private edge(inset: number, top: string, bottom: string, width: number): void {
    const c = this.ctx;
    const gradient: CanvasGradient = c.createLinearGradient(inset, inset, c.width - inset, c.height - inset);
    gradient.addColorStop(0, top); gradient.addColorStop(.5, 'rgba(0,0,0,0)'); gradient.addColorStop(1, bottom);
    c.strokeStyle = gradient; c.lineWidth = width; c.stroke(this.shape(inset));
  }
  private inset(shade: string, light: string, shift: number, blur: number): void {
    // A blurred complement of each shifted opening produces an inner shadow.
    // The unshifted rounded clip keeps the wall inside the exact child bounds.
    const c = this.ctx, margin: number = blur * 5 + shift + 2;
    c.save(); c.clip(this.shape()); c.filter = `blur(${blur}vp)`;
    for (let index: number = 0; index < 2; index++) {
      const offset: number = index === 0 ? shift : -shift;
      const wall: Path2D = new Path2D();
      wall.rect(-margin, -margin, c.width + 2 * margin, c.height + 2 * margin);
      wall.addPath(this.shape(0, offset, offset));
      c.fillStyle = index === 0 ? shade : light; c.fill(wall, 'evenodd');
    }
    c.restore();
  }
  private draw(): void {
    const c = this.ctx; c.clearRect(0, 0, c.width, c.height);
    if (this.style === 'flat' || !Number.isFinite(c.width) || !Number.isFinite(c.height) || c.width < 8 || c.height < 8 ||
      !Number.isFinite(this.depth) || this.depth <= 0 ||
      !Number.isFinite(this.depthMultiplier) || this.depthMultiplier >= 0) { return; }
    const strength: number = Math.min(2, -this.depthMultiplier * this.depth), fade: number = Math.min(1, strength);
    const darkRgb: string = this.dark ? '#000000' : '#37323D';
    const darkEdge: string = this.color(darkRgb, (this.dark ? .72 : .52) * fade);
    const lightEdge: string = this.color('#FFFFFF', (this.dark ? .34 : .82) * fade);
    const shadow: string = this.color('#000000', (this.dark ? .48 : .18) * fade);
    switch (this.style) {
      case 'neumorphism': {
        const surface: string = /^#[0-9a-fA-F]{6}$/.test(this.surface) ? this.surface : (this.dark ? '#292634' : '#FFFFFF');
        const shade: string = this.color(mix(surface, '#000000', this.dark ? .85 : .75), (this.dark ? .52 : .24) * fade);
        const light: string = this.color(mix(surface, '#FFFFFF', this.dark ? .28 : .8), (this.dark ? .20 : .82) * fade);
        this.inset(shade, light, 1.25 * strength, 2); break;
      }
      case 'paper':
        this.stroke(0, this.color(darkRgb, (this.dark ? .4 : .28) * fade), 1);
        this.edge(1, lightEdge, darkEdge, 1); break;
      case 'clay':
        this.inset(shadow, lightEdge, 1.5 * strength, 4);
        this.stroke(1, this.color('#FFFFFF', .3 * fade), 1); break;
      case 'fluent':
        this.stroke(0, this.color(darkRgb, .35 * fade), 1);
        this.edge(1, shadow, lightEdge, .8); break;
      case 'brutalist': {
        const ink: string = this.dark ? '#FFFFFF' : '#211E26';
        this.stroke(0, this.color(ink, fade), 1.8);
        this.edge(1.4, this.color(ink, .45 * fade), 'rgba(0,0,0,0)', 1); break;
      }
      case 'industrial':
        this.stroke(0, darkEdge, 2 * strength);
        this.edge(1.5, shadow, lightEdge, 1.5);
        this.stroke(3, this.color('#FFFFFF', .2 * fade), .7); break;
    }
  }
  private resized(): void {
    if (this.resizeTimer !== -1) { clearTimeout(this.resizeTimer); }
    this.resizeTimer = setTimeout((): void => { this.resizeTimer = -1; this.draw(); }, 16);
  }
  aboutToDisappear(): void {
    if (this.resizeTimer !== -1) { clearTimeout(this.resizeTimer); this.resizeTimer = -1; }
  }
  build() {
    Canvas(this.ctx).width('100%').height('100%').onReady((): void => this.draw())
      .onAreaChange((): void => this.resized()).hitTestBehavior(HitTestMode.None)
  }
}
