// Design geometry ported from lib/main.dart AmbientPainter and OrbPainter.
// Keep these procedural: the Flutter source is the reference, not a screenshot asset.
@Component
export struct AmbientBackdrop {
  @Prop dark: boolean = false;
  private ctx: CanvasRenderingContext2D = new CanvasRenderingContext2D(new RenderingContextSettings(true));
  private paint(): void {
    const c = this.ctx; const w = c.width; const h = c.height;
    c.clearRect(0, 0, w, h);
    const alpha = this.dark ? 0.25 : 0.42;
    this.glow(w * .53, h * .27, w * .44, `rgba(196,172,238,${alpha})`);
    this.glow(w * .95, h * .82, w * .37, `rgba(144,180,177,${alpha})`);
    this.glow(w * .35, h * 1.03, w * .37, `rgba(230,195,171,${alpha})`);
    c.strokeStyle = this.dark ? 'rgba(192,175,250,0.045)' : 'rgba(118,98,186,0.045)'; c.lineWidth = 1;
    for (let i = 0; i < 5; i++) {
      c.beginPath(); c.moveTo(w * .1, h * .88 + i * 22);
      c.bezierCurveTo(w * .48, h * .36 + i * 25, w * .72, h * 1.2 + i * 30, w * 1.1, h * .25 + i * 40); c.stroke();
    }
  }
  private glow(x: number, y: number, radius: number, color: string): void {
    const g = this.ctx.createRadialGradient(x, y, 0, x, y, radius);
    g.addColorStop(0, color); g.addColorStop(1, 'rgba(255,255,255,0)');
    this.ctx.fillStyle = g; this.ctx.fillRect(0, 0, this.ctx.width, this.ctx.height);
  }
  build() {
    Canvas(this.ctx).width('100%').height('100%').onReady(() => this.paint())
      .onAreaChange(() => this.paint()).hitTestBehavior(HitTestMode.None)
  }
}

@Component
export struct IdeaOrb {
  private ctx: CanvasRenderingContext2D = new CanvasRenderingContext2D(new RenderingContextSettings(true));
  private paint(): void {
    const c = this.ctx; const x = c.width * .57; const y = c.height * .52;
    c.clearRect(0, 0, c.width, c.height); c.save(); c.translate(x, y); c.rotate(-.47);
    c.shadowColor = 'rgba(112,98,131,0.15)'; c.shadowBlur = 21; c.shadowOffsetY = 19;
    c.strokeStyle = 'rgba(112,98,131,0.15)'; c.lineWidth = 3;
    c.beginPath(); c.ellipse(0, 19, 98, 68, 0, 0, Math.PI * 2); c.stroke();
    c.shadowBlur = 0; c.shadowOffsetY = 0;
    for (let i = 0; i < 48; i++) {
      const t = i / 47; const rx = (202 - Math.sin(t * Math.PI) * 21) / 2;
      const ry = (133 - Math.sin(t * Math.PI) * 14) / 2;
      // Canvas has no sweep gradient: a continuous segmented gradient follows the same 48 ellipses.
      for (let j = 0; j < 96; j++) {
        const angle = j / 96 * Math.PI * 2;
        const light = (Math.sin(angle - t * .7) + 1) / 2;
        c.strokeStyle = `rgba(${150 + Math.round(light * 100)},${124 + Math.round(light * 129)},${175 + Math.round(light * 79)},${.48 + light * .4})`;
        c.lineWidth = 2.3; c.beginPath();
        c.ellipse(0, -10 + t * 23, rx, ry, 0, angle, angle + Math.PI * 2 / 96 + .007); c.stroke();
      }
    }
    c.strokeStyle = 'rgba(255,255,255,0.85)'; c.lineWidth = 1;
    c.beginPath(); c.ellipse(0, -8, 98, 64, 0, 0, Math.PI * 2); c.stroke(); c.restore();
    c.strokeStyle = 'rgba(118,98,186,0.5)'; c.beginPath();
    c.moveTo(x + 88, y - 91); c.lineTo(x + 88, y - 79);
    c.moveTo(x + 82, y - 85); c.lineTo(x + 94, y - 85); c.stroke();
  }
  build() { Canvas(this.ctx).width(340).height(354).onReady(() => this.paint()).hitTestBehavior(HitTestMode.None) }
}


// Flutter LiquidRimPainter geometry, with pointer-following lighting.
@Component
export struct GlassRim {
  @Prop @Watch('draw') dark: boolean = false;
  @Prop @Watch('draw') radius: number = 20;
  @Prop @Watch('draw') lightX: number = -.65;
  @Prop @Watch('draw') lightY: number = -.8;
  private ctx: CanvasRenderingContext2D = new CanvasRenderingContext2D(new RenderingContextSettings(true));
  private rimPath(inset: number, radius: number): void {
    const c = this.ctx; const w = c.width - inset * 2; const h = c.height - inset * 2;
    const r = Math.max(0, Math.min(radius, w / 2, h / 2));
    c.beginPath(); c.moveTo(inset + r, inset); c.lineTo(inset + w - r, inset);
    c.arcTo(inset + w, inset, inset + w, inset + r, r); c.lineTo(inset + w, inset + h - r);
    c.arcTo(inset + w, inset + h, inset + w - r, inset + h, r); c.lineTo(inset + r, inset + h);
    c.arcTo(inset, inset + h, inset, inset + h - r, r); c.lineTo(inset, inset + r);
    c.arcTo(inset, inset, inset + r, inset, r); c.closePath(); c.stroke();
  }
  private draw(): void {
    const c = this.ctx; const w = c.width; const h = c.height;
    if (w < 12 || h < 12) { return; }
    c.clearRect(0, 0, w, h);
    const g = c.createLinearGradient((1 + this.lightX) * w / 2, (1 + this.lightY) * h / 2, (1 - this.lightX) * w / 2, (1 - this.lightY) * h / 2);
    g.addColorStop(0, this.dark ? 'rgba(255,255,255,0.70)' : 'rgba(255,255,255,0.94)');
    g.addColorStop(.36, 'rgba(255,255,255,0.08)'); g.addColorStop(.66, 'rgba(119,121,159,0.12)'); g.addColorStop(1, 'rgba(255,255,255,0.55)');
    c.strokeStyle = g; c.lineWidth = 1.6; this.rimPath(.8, this.radius);
    const inner = c.createLinearGradient(0, 0, w / 2, h / 2); inner.addColorStop(0, 'rgba(255,255,255,0.17)'); inner.addColorStop(1, 'rgba(255,255,255,0)');
    c.strokeStyle = inner; c.lineWidth = 3; this.rimPath(2.8, this.radius - 2);
    c.strokeStyle = 'rgba(255,255,255,0.1)'; c.lineWidth = .65; this.rimPath(5.8, this.radius - 5);
  }
  build() { Canvas(this.ctx).width('100%').height('100%').onReady(() => this.draw()).onAreaChange(() => this.draw()).hitTestBehavior(HitTestMode.None) }
}

@Component
export struct PaperBackdrop {
  @Prop dark: boolean = false;
  private ctx: CanvasRenderingContext2D = new CanvasRenderingContext2D(new RenderingContextSettings(true));
  private draw(): void {
    const c = this.ctx; c.clearRect(0, 0, c.width, c.height);
    c.fillStyle = this.dark ? 'rgba(255,255,255,0.05)' : 'rgba(48,45,67,0.045)';
    for (let y = 3; y < c.height; y += 6) { for (let x = 3; x < c.width; x += 6) { c.beginPath(); c.arc(x, y, .6, 0, Math.PI * 2); c.fill(); } }
  }
  build() { Canvas(this.ctx).width('100%').height('100%').onReady(() => this.draw()).onAreaChange(() => this.draw()).hitTestBehavior(HitTestMode.None) }
}
