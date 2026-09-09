import 'dart:ui' as ui;
import 'package:flutter/material.dart';

/// Interpolated optical values; content is deliberately outside this state.
@immutable
class GlassMaterial {
  const GlassMaterial({
    required this.blur,
    required this.liquid,
    required this.decoration,
  });
  final double blur, liquid;
  final BoxDecoration decoration;
  factory GlassMaterial.liquid({
    required Color tint,
    required bool dark,
    required bool readable,
    required BorderRadius borderRadius,
  }) => GlassMaterial(
    blur: 5,
    liquid: 1,
    decoration: BoxDecoration(
      borderRadius: borderRadius,
      border: Border.all(color: Colors.transparent, width: 1.6),
      gradient: LinearGradient(
        begin: Alignment.topLeft,
        end: Alignment.bottomRight,
        colors: [
          tint.withValues(
            alpha: readable
                ? .88
                : dark
                ? .40
                : .25,
          ),
          tint.withValues(
            alpha: readable
                ? .82
                : dark
                ? .24
                : .09,
          ),
          tint.withValues(
            alpha: readable
                ? .88
                : dark
                ? .34
                : .18,
          ),
        ],
      ),
    ),
  );
  static GlassMaterial lerp(GlassMaterial a, GlassMaterial b, double t) =>
      GlassMaterial(
        blur: ui.lerpDouble(a.blur, b.blur, t)!,
        liquid: ui.lerpDouble(a.liquid, b.liquid, t)!,
        decoration: BoxDecoration.lerp(a.decoration, b.decoration, t)!,
      );
  @override
  bool operator ==(Object other) =>
      other is GlassMaterial &&
      blur == other.blur &&
      liquid == other.liquid &&
      decoration == other.decoration;
  @override
  int get hashCode => Object.hash(blur, liquid, decoration);
}

class GlassMaterialTween extends Tween<GlassMaterial> {
  GlassMaterialTween({super.begin, super.end});
  @override
  GlassMaterial lerp(double t) => GlassMaterial.lerp(begin!, end!, t);
}

/// A cross-platform optical material. Impeller gets curved per-pixel refraction;
/// other backends use a real magnifying backdrop filter with the same light rim.
class LiquidGlassSurface extends StatefulWidget {
  const LiquidGlassSurface({
    super.key,
    required this.child,
    required this.tint,
    required this.dark,
    required this.borderRadius,
    this.canvas = false,
    this.readable = false,
    this.transparentCanvas = true,
    this.material,
  });
  final GlassMaterial? material;
  final Widget child;
  final Color tint;
  final bool dark, canvas, readable;

  /// Only transparent canvases must leave the desktop-facing interior unpainted.
  final bool transparentCanvas;
  final BorderRadius borderRadius;
  @override
  State<LiquidGlassSurface> createState() => _LiquidGlassSurfaceState();
}

class _LiquidGlassSurfaceState extends State<LiquidGlassSurface> {
  static Future<ui.FragmentProgram>? _program;
  ui.FragmentShader? _shader;
  bool _loadingShader = false;
  bool get _filterEnabled => !widget.canvas || !widget.transparentCanvas;
  Offset _light = const Offset(-.65, -.8);

  @override
  void initState() {
    super.initState();
    if (_filterEnabled && ui.ImageFilter.isShaderFilterSupported) {
      _loadShader();
    }
  }

  @override
  void didUpdateWidget(LiquidGlassSurface oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (_filterEnabled &&
        _shader == null &&
        ui.ImageFilter.isShaderFilterSupported) {
      _loadShader();
    }
  }

  Future<void> _loadShader() async {
    if (_loadingShader) return;
    _loadingShader = true;
    try {
      final program = await (_program ??= ui.FragmentProgram.fromAsset(
        'shaders/liquid_glass.frag',
      ));
      if (mounted) setState(() => _shader = program.fragmentShader());
    } catch (_) {
      // A backend/asset failure retains the magnifying backdrop material.
    } finally {
      _loadingShader = false;
    }
  }

  @override
  void dispose() {
    _shader?.dispose();
    super.dispose();
  }

  ui.ImageFilter _filter(Size size, GlassMaterial material) {
    final ui.ImageFilter refraction;
    if (_shader case final shader?) {
      shader.setFloat(2, widget.borderRadius.topLeft.x);
      shader.setFloat(3, (widget.canvas ? 6 : 12) * material.liquid);
      refraction = ui.ImageFilter.shader(shader);
    } else {
      final zoom = 1 + (widget.canvas ? .006 : .025) * material.liquid;
      final transform = Matrix4.identity()
        ..setEntry(0, 0, zoom)
        ..setEntry(1, 1, zoom)
        ..setEntry(0, 3, size.width * (1 - zoom) / 2)
        ..setEntry(1, 3, size.height * (1 - zoom) / 2);
      refraction = ui.ImageFilter.matrix(transform.storage);
    }
    return ui.ImageFilter.compose(
      outer: ui.ImageFilter.blur(sigmaX: material.blur, sigmaY: material.blur),
      inner: refraction,
    );
  }

  @override
  Widget build(BuildContext context) {
    final reduce = MediaQuery.disableAnimationsOf(context);
    final material =
        widget.material ??
        GlassMaterial.liquid(
          tint: widget.tint,
          dark: widget.dark,
          readable: MediaQuery.highContrastOf(context) || widget.readable,
          borderRadius: widget.borderRadius,
        );
    return MouseRegion(
      onHover: reduce
          ? null
          : (event) {
              final box = context.findRenderObject() as RenderBox?;
              if (box == null || box.size.isEmpty) return;
              final point = box.globalToLocal(event.position);
              setState(
                () => _light = Offset(
                  (point.dx / box.size.width * 2 - 1).clamp(-1, 1),
                  (point.dy / box.size.height * 2 - 1).clamp(-1, 1),
                ),
              );
            },
      onExit: (_) => setState(() => _light = const Offset(-.65, -.8)),
      child: Container(
        decoration: BoxDecoration(
          borderRadius: widget.borderRadius,
          boxShadow: widget.canvas ? null : material.decoration.boxShadow,
        ),
        child: ClipRRect(
          borderRadius: widget.borderRadius,
          child: Stack(
            children: [
              // The transparent canvas cannot sample the OS desktop through a
              // Flutter backdrop. Leave its interior unpainted; retain the rim.
              if (_filterEnabled)
                Positioned.fill(
                  child: LayoutBuilder(
                    builder: (_, constraints) => BackdropFilter(
                      filter: _filter(constraints.biggest, material),
                      child: DecoratedBox(
                        decoration: material.decoration.copyWith(
                          boxShadow: const [],
                        ),
                      ),
                    ),
                  ),
                ),
              widget.child,
              Positioned.fill(
                child: IgnorePointer(
                  child: TweenAnimationBuilder<Offset>(
                    tween: Tween(end: _light),
                    duration: reduce
                        ? Duration.zero
                        : const Duration(milliseconds: 180),
                    curve: Curves.easeOutCubic,
                    builder: (_, light, _) => CustomPaint(
                      painter: LiquidRimPainter(
                        radius: widget.borderRadius.topLeft.x,
                        light: light,
                        dark: widget.dark,
                        canvas: widget.canvas,
                        intensity: material.liquid,
                      ),
                    ),
                  ),
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

class LiquidRimPainter extends CustomPainter {
  LiquidRimPainter({
    required this.radius,
    required this.light,
    required this.dark,
    this.canvas = false,
    this.intensity = 1,
  });
  final double intensity;
  final double radius;
  final Offset light;
  final bool dark, canvas;
  @override
  void paint(Canvas canvas, Size size) {
    if (size.isEmpty || intensity <= 0) return;
    final rect = (Offset.zero & size).deflate(.8);
    final rrect = RRect.fromRectAndRadius(rect, Radius.circular(radius));
    final gradient = LinearGradient(
      begin: Alignment(light.dx, light.dy),
      end: Alignment(-light.dx, -light.dy),
      colors: [
        Colors.white.withValues(alpha: (dark ? .70 : .94) * intensity),
        Colors.white.withValues(alpha: .08 * intensity),
        const Color(0xFF77799F).withValues(alpha: .12 * intensity),
        Colors.white.withValues(alpha: .55 * intensity),
      ],
      stops: const [0, .36, .66, 1],
    );
    canvas.drawRRect(
      rrect,
      Paint()
        ..style = PaintingStyle.stroke
        ..strokeWidth = 1.6
        ..shader = gradient.createShader(rect),
    );
    canvas.drawRRect(
      rrect.deflate(2),
      Paint()
        ..style = PaintingStyle.stroke
        ..strokeWidth = 3
        ..shader = LinearGradient(
          begin: Alignment(light.dx, light.dy),
          end: Alignment.center,
          colors: [
            Colors.white.withValues(alpha: .17 * intensity),
            Colors.transparent,
          ],
        ).createShader(rect),
    );
    canvas.drawRRect(
      rrect.deflate(5),
      Paint()
        ..style = PaintingStyle.stroke
        ..strokeWidth = .65
        ..color = Colors.white.withValues(alpha: .10 * intensity),
    );
  }

  @override
  bool shouldRepaint(LiquidRimPainter old) =>
      old.intensity != intensity ||
      old.radius != radius ||
      old.light != light ||
      old.dark != dark ||
      old.canvas != canvas;
}
