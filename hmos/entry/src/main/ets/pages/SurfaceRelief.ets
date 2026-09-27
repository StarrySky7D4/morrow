// Edge relief derived from Flutter appearance.dart / NeumorphicSurfacePainter.
// The outer shadow lives on the panel; this canvas never fills its transparent interior.
@Component
export struct SurfaceRelief {
  @Prop @Watch('draw') style: string = 'flat';
  @Prop @Watch('draw') depth: number = 1;
  @Prop @Watch('draw') dark: boolean = false;
  @Prop @Watch('draw') radius: number = 20;
  private ctx: CanvasRenderingContext2D = new CanvasRenderingContext2D(new RenderingContextSettings(true));
  private resizeTimer: number = -1;
  private resized(): void {
    // Canvas dimensions update after the enclosing panel's area callback.
    if (this.resizeTimer !== -1) { clearTimeout(this.resizeTimer); }
    this.resizeTimer = setTimeout(() => { this.resizeTimer = -1; this.draw(); }, 16);
  }
  aboutToDisappear(): void { if (this.resizeTimer !== -1) { clearTimeout(this.resizeTimer); } }
  private path(inset: number): void {
    const c = this.ctx; const x = inset; const y = inset;
    const w = c.width - 2 * inset; const h = c.height - 2 * inset;
    const r = Math.max(0, Math.min(this.radius - inset, w / 2, h / 2));
    c.beginPath(); c.moveTo(x + r, y); c.lineTo(x + w - r, y);
    c.arcTo(x + w, y, x + w, y + r, r); c.lineTo(x + w, y + h - r);
    c.arcTo(x + w, y + h, x + w - r, y + h, r); c.lineTo(x + r, y + h);
    c.arcTo(x, y + h, x, y + h - r, r); c.lineTo(x, y + r);
    c.arcTo(x, y, x + r, y, r); c.closePath();
  }
  private draw(): void {
    const c = this.ctx; const w = c.width; const h = c.height;
    c.clearRect(0, 0, w, h);
    if (w < 8 || h < 8 || this.style === 'flat') { return; }
    const d = Math.max(0, Math.min(2, this.depth));
    const opacity = Math.min(1, d);
    if (this.style === 'neumorphism' || this.style === 'clay') {
      const layers = this.style === 'clay' ? 6 : 4;
      for (let i = layers; i >= 1; i--) {
        const g = c.createLinearGradient(0, 0, w, h);
        g.addColorStop(0, `rgba(255,255,255,${(this.dark ? .08 : .3) * opacity / i})`);
        g.addColorStop(.45, 'rgba(255,255,255,0)');
        g.addColorStop(1, `rgba(62,52,78,${(this.dark ? .3 : .18) * opacity / i})`);
        c.strokeStyle = g; c.lineWidth = Math.max(.5, d * i);
        this.path(d * i / 2 + .5); c.stroke();
      }
    } else if (this.style === 'industrial') {
      const g = c.createLinearGradient(0, 0, 0, h);
      g.addColorStop(0, `rgba(255,255,255,${(this.dark ? .1 : .58) * opacity})`);
      g.addColorStop(1, `rgba(48,45,67,${.28 * opacity})`);
      c.strokeStyle = g; c.lineWidth = 1; this.path(2 * d + .5); c.stroke();
    } else if (this.style === 'paper' && d > 0) {
      c.strokeStyle = `rgba(48,45,67,${.12 * opacity})`; c.lineWidth = 1;
      c.beginPath(); c.moveTo(3, h - 2 * d); c.lineTo(w - 3, h - 2 * d); c.stroke();
    }
  }
  build() {
    Canvas(this.ctx).width('100%').height('100%').onReady(() => this.draw())
      .onAreaChange(() => this.resized()).hitTestBehavior(HitTestMode.None)
  }
}
