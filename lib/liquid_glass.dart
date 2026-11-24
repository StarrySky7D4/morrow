import 'dart:math' as math;
import 'dart:ui' as ui;
import 'package:flutter/foundation.dart' show listEquals;
import 'package:flutter/gestures.dart';
import 'package:material_ui/material_ui.dart';

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
    List<BoxShadow>? boxShadow,
  }) => GlassMaterial(
    blur: 5,
    liquid: 1,
    decoration: BoxDecoration(
      borderRadius: borderRadius,
      border: Border.all(color: Colors.transparent, width: 1.6),
      boxShadow:
          boxShadow ??
          [
            BoxShadow(
              color: (dark ? Colors.black : const Color(0xFF3E3552)).withValues(
                alpha: readable ? (dark ? .44 : .15) : (dark ? .28 : .08),
              ),
              blurRadius: readable ? 26 : 16,
              offset: Offset(0, readable ? 10 : 6),
            ),
            BoxShadow(
              color: (dark ? Colors.black : const Color(0xFF3E3552)).withValues(
                alpha: readable ? (dark ? .22 : .06) : (dark ? .14 : .04),
              ),
              blurRadius: readable ? 8 : 4,
              offset: Offset(0, readable ? 3 : 1.5),
            ),
          ],
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

/// Temporarily suspends descendant [BackdropFilter] layers while an ancestor
/// opacity transition (`0 < opacity < 1`) is active, avoiding Skia
/// `OpacityLayer + BackdropFilter` alpha overexposure over transparent/dark surfaces.
class BackdropFilterSuspension extends InheritedWidget {
  const BackdropFilterSuspension({
    super.key,
    required this.suspended,
    required super.child,
  });

  final bool suspended;

  static bool isSuspended(BuildContext context) =>
      context
          .dependOnInheritedWidgetOfExactType<BackdropFilterSuspension>()
          ?.suspended ??
      false;

  @override
  bool updateShouldNotify(BackdropFilterSuspension oldWidget) =>
      suspended != oldWidget.suspended;
}

/// Propagates [suspended] downward while preserving any active outer
/// [BackdropFilterSuspension] so nested animations never re-enable blurs early.
class BackdropFilterGuard extends StatelessWidget {
  const BackdropFilterGuard({
    super.key,
    required this.suspended,
    required this.child,
  });

  final bool suspended;
  final Widget child;

  @override
  Widget build(BuildContext context) => BackdropFilterSuspension(
    suspended: suspended || BackdropFilterSuspension.isSuspended(context),
    child: child,
  );
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
    this.useBackdropGroup = true,
    this.backdropEnabled = true,
  });
  final GlassMaterial? material;
  final Widget child;
  final Color tint;
  final bool dark, canvas, readable;

  /// Only transparent canvases must leave the desktop-facing interior unpainted.
  final bool transparentCanvas;
  final BorderRadius borderRadius;
  final bool useBackdropGroup;
  final bool backdropEnabled;
  @override
  State<LiquidGlassSurface> createState() => _LiquidGlassSurfaceState();
}

class _LiquidGlassSurfaceState extends State<LiquidGlassSurface>
    with SingleTickerProviderStateMixin {
  static Future<ui.FragmentProgram>? _program;
  static final Map<double, ui.ImageFilter> _blurCache = <double, ui.ImageFilter>{};

  static ui.ImageFilter _cachedBlurFilter(double rawBlur) {
    final mapped = rawBlur <= 6.0
        ? rawBlur.clamp(0.0, 6.0)
        : (6.0 + (rawBlur - 6.0) * 0.25).clamp(0.0, 10.0);
    final quantized = (mapped * 2.0).round() / 2.0;
    return _blurCache.putIfAbsent(quantized, () {
      if (_blurCache.length >= 48) {
        _blurCache.remove(_blurCache.keys.first);
      }
      return ui.ImageFilter.blur(sigmaX: quantized, sigmaY: quantized);
    });
  }

  static bool _isOpaqueSurface(GlassMaterial material) {
    final color = material.decoration.color;
    if (color != null && color.a >= 0.65) return true;
    final gradient = material.decoration.gradient;
    if (gradient is LinearGradient && gradient.colors.isNotEmpty) {
      var minAlpha = 1.0;
      for (final c in gradient.colors) {
        if (c.a < minAlpha) minAlpha = c.a;
      }
      return minAlpha >= 0.65;
    }
    return false;
  }

  ui.FragmentShader? _shader;
  bool _loadingShader = false;
  AnimationController? _press;
  ValueNotifier<Offset>? _light;
  Listenable? _optics;
  bool get _filterEnabled => !widget.canvas || !widget.transparentCanvas;
  bool get _needsRefraction => (widget.material?.liquid ?? 1) > .001;

  Listenable _ensureOptics() {
    if (_optics case final optics?) return optics;
    final press = _press ??=
        AnimationController(
          vsync: this,
          duration: const Duration(milliseconds: 180),
          reverseDuration: const Duration(milliseconds: 320),
        )..addStatusListener((status) {
          if (status == AnimationStatus.completed) _press?.reverse();
        });
    final light = _light ??= ValueNotifier(const Offset(-.65, -.8));
    return _optics = Listenable.merge([light, press]);
  }

  @override
  void initState() {
    super.initState();
    if (_filterEnabled && _needsRefraction) {
      _ensureOptics();
      if (ui.ImageFilter.isShaderFilterSupported) {
        _loadShader();
      }
    }
  }

  @override
  void didUpdateWidget(LiquidGlassSurface oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (!_needsRefraction) {
      _resetInteraction();
      _shader?.dispose();
      _shader = null;
    } else if (_filterEnabled) {
      _ensureOptics();
      if (_shader == null && ui.ImageFilter.isShaderFilterSupported) {
        _loadShader();
      }
    }
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    if (!_motionEnabled) _resetInteraction();
  }

  bool get _motionEnabled =>
      !MediaQuery.disableAnimationsOf(context) &&
      TickerMode.valuesOf(context).enabled;

  void _resetInteraction() {
    final press = _press;
    if (press != null && (press.isAnimating || press.value != 0)) {
      press.reset();
    }
    final light = _light;
    if (light != null && light.value != const Offset(-.65, -.8)) {
      light.value = const Offset(-.65, -.8);
    }
  }

  Future<void> _loadShader() async {
    if (_loadingShader) return;
    _loadingShader = true;
    try {
      final program = await (_program ??= ui.FragmentProgram.fromAsset(
        'shaders/liquid_glass.frag',
      ));
      if (mounted && _filterEnabled && _needsRefraction) {
        setState(() => _shader = program.fragmentShader());
      }
    } catch (_) {
      // A backend/asset failure retains the magnifying backdrop material.
    } finally {
      _loadingShader = false;
    }
  }

  @override
  void dispose() {
    _press?.dispose();
    _light?.dispose();
    _shader?.dispose();
    super.dispose();
  }

  ui.ImageFilter _filter(Size size, GlassMaterial material) {
    if (material.liquid <= .001) {
      return _cachedBlurFilter(material.blur);
    }
    final light = _light?.value ?? const Offset(-.65, -.8);
    final pressValue = _press?.value ?? 0.0;
    final ui.ImageFilter refraction;
    if (_shader case final shader?) {
      shader.setFloat(2, widget.borderRadius.topLeft.x);
      shader.setFloat(3, (widget.canvas ? 4 : 9) * material.liquid);
      shader.setFloat(4, light.dx);
      shader.setFloat(5, light.dy);
      shader.setFloat(6, pressValue);
      refraction = ui.ImageFilter.shader(shader);
    } else {
      final zoom =
          1 +
          (widget.canvas ? .004 : .014) *
              material.liquid *
              (1 + .3 * pressValue);
      final transform = Matrix4.identity()
        ..setEntry(0, 0, zoom)
        ..setEntry(1, 1, zoom)
        ..setEntry(0, 3, size.width * (1 - zoom) / 2)
        ..setEntry(1, 3, size.height * (1 - zoom) / 2);
      refraction = ui.ImageFilter.matrix(transform.storage);
    }
    return ui.ImageFilter.compose(
      outer: _cachedBlurFilter(material.blur),
      inner: refraction,
    );
  }

  void _updateLight(PointerEvent event) {
    final box = context.findRenderObject() as RenderBox?;
    if (box == null || box.size.isEmpty) return;
    final light = _light;
    if (light == null) return;
    final point = box.globalToLocal(event.position);
    final next = Offset(
      (point.dx / box.size.width * 2 - 1).clamp(-1.0, 1.0),
      (point.dy / box.size.height * 2 - 1).clamp(-1.0, 1.0),
    );
    if ((light.value - next).distanceSquared > .0025) light.value = next;
  }

  void _pressAt(PointerDownEvent event) {
    if (!_needsRefraction || !_motionEnabled) return;
    if (event.kind != ui.PointerDeviceKind.touch &&
        (event.buttons & kPrimaryButton) == 0) {
      return;
    }
    _ensureOptics();
    _updateLight(event);
    _press?.forward(from: 0);
  }

  @override
  Widget build(BuildContext context) {
    final motionEnabled = _motionEnabled;
    final material =
        widget.material ??
        GlassMaterial.liquid(
          tint: widget.tint,
          dark: widget.dark,
          readable: MediaQuery.highContrastOf(context) || widget.readable,
          borderRadius: widget.borderRadius,
        );
    final needsRefraction = _needsRefraction;
    final optics = needsRefraction ? _ensureOptics() : null;
    final surfaceDecoration = material.decoration.copyWith(boxShadow: const []);
    final canGroup =
        widget.useBackdropGroup &&
        !widget.canvas &&
        !widget.readable &&
        BackdropGroup.of(context) != null;
    final suspendBackdrop =
        !widget.backdropEnabled ||
        BackdropFilterSuspension.isSuspended(context);

    Widget buildBackdrop() {
      final box = DecoratedBox(decoration: surfaceDecoration);
      if (suspendBackdrop) return box;
      if (!needsRefraction) {
        if (material.blur <= .001) return box;
        final blur = _cachedBlurFilter(material.blur);
        return canGroup
            ? BackdropFilter.grouped(filter: blur, child: box)
            : BackdropFilter(filter: blur, child: box);
      }
      if (_shader != null) {
        return AnimatedBuilder(
          animation: optics!,
          child: DecoratedBox(decoration: surfaceDecoration),
          builder: (_, backdropChild) => BackdropFilter(
            filter: _filter(Size.zero, material),
            child: backdropChild,
          ),
        );
      }
      return AnimatedBuilder(
        animation: _press!,
        child: DecoratedBox(decoration: surfaceDecoration),
        builder: (_, backdropChild) {
          if (canGroup && _press!.value <= 0.001) {
            return BackdropFilter.grouped(
              filter: _cachedBlurFilter(material.blur),
              child: backdropChild,
            );
          }
          return LayoutBuilder(
            builder: (_, constraints) => BackdropFilter(
              filter: _filter(constraints.biggest, material),
              child: backdropChild,
            ),
          );
        },
      );
    }

    return Listener(
      behavior: HitTestBehavior.translucent,
      onPointerDown: _pressAt,
      onPointerCancel: (_) => _resetInteraction(),
      child: MouseRegion(
        onHover: !motionEnabled || !needsRefraction ? null : _updateLight,
        onExit: !motionEnabled || !needsRefraction
            ? null
            : (_) {
                final light = _light;
                if (light != null) light.value = const Offset(-.65, -.8);
              },
        child: CustomPaint(
          painter: widget.canvas
              ? null
              : OuterShadowPainter(
                  borderRadius: widget.borderRadius,
                  shadows: material.decoration.boxShadow ?? const [],
                  opaqueInterior: _isOpaqueSurface(material),
                ),
          child: ClipRRect(
            borderRadius: widget.borderRadius,
            child: Stack(
              children: [
                // The transparent canvas cannot sample the OS desktop through a
                // Flutter backdrop. Leave its interior unpainted; retain the rim.
                if (_filterEnabled) Positioned.fill(child: buildBackdrop()),
                widget.child,
                Positioned.fill(
                  child: IgnorePointer(
                    child: optics != null
                        ? AnimatedBuilder(
                            animation: optics,
                            builder: (_, _) => CustomPaint(
                              painter: LiquidRimPainter(
                                radius: widget.borderRadius.topLeft.x,
                                light:
                                    _light?.value ?? const Offset(-.65, -.8),
                                dark: widget.dark,
                                canvas: widget.canvas,
                                intensity: material.liquid,
                                press: _press?.value ?? 0.0,
                              ),
                            ),
                          )
                        : CustomPaint(
                            painter: LiquidRimPainter(
                              radius: widget.borderRadius.topLeft.x,
                              light: const Offset(-.65, -.8),
                              dark: widget.dark,
                              canvas: widget.canvas,
                              intensity: 0,
                              press: 0,
                            ),
                          ),
                  ),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

/// Keeps blurred shadows outside the original outline. Clear materials have a
/// transparent fill, so a shifted shadow must not tint their interior.
class OuterShadowPainter extends CustomPainter {
  const OuterShadowPainter({
    required this.borderRadius,
    required this.shadows,
    this.opaqueInterior = false,
  });

  final BorderRadius borderRadius;
  final List<BoxShadow> shadows;
  final bool opaqueInterior;

  @override
  void paint(Canvas canvas, Size size) {
    if (size.isEmpty || shadows.isEmpty) return;
    final outline = borderRadius.toRRect(Offset.zero & size);
    if (opaqueInterior) {
      for (final shadow in shadows) {
        canvas.drawRRect(
          outline.shift(shadow.offset).inflate(shadow.spreadRadius),
          shadow.toPaint(),
        );
      }
      return;
    }
    for (final shadow in shadows) {
      final sigma = shadow.blurSigma;
      if (sigma > 0.01) {
        final dx = shadow.offset.dx;
        final dy = shadow.offset.dy;
        final spread = shadow.spreadRadius;
        final envelope = RRect.fromLTRBAndCorners(
          outline.left + math.min(0.0, dx * 0.45) - spread,
          outline.top + math.min(0.0, dy * 0.45) - spread,
          outline.right + math.max(0.0, dx * 0.45) + spread,
          outline.bottom + math.max(0.0, dy * 0.45) + spread,
          topLeft: outline.tlRadius,
          topRight: outline.trRadius,
          bottomRight: outline.brRadius,
          bottomLeft: outline.blRadius,
        );
        canvas.drawRRect(
          envelope,
          Paint()
            ..color = shadow.color
            ..maskFilter = MaskFilter.blur(ui.BlurStyle.outer, sigma),
        );
      } else if (shadow.offset != Offset.zero || shadow.spreadRadius > 0) {
        final expand =
            shadow.spreadRadius.abs() +
            shadow.offset.dx.abs() +
            shadow.offset.dy.abs() +
            2;
        canvas.save();
        final outside = Path()
          ..fillType = ui.PathFillType.evenOdd
          ..addRect((Offset.zero & size).inflate(expand))
          ..addRRect(outline);
        canvas.clipPath(outside);
        canvas.drawRRect(
          outline.shift(shadow.offset).inflate(shadow.spreadRadius),
          shadow.toPaint(),
        );
        canvas.restore();
      }
    }
  }

  @override
  bool shouldRepaint(OuterShadowPainter old) =>
      old.opaqueInterior != opaqueInterior ||
      old.borderRadius != borderRadius ||
      !listEquals(old.shadows, shadows);
}

class LiquidRimPainter extends CustomPainter {
  LiquidRimPainter({
    required this.radius,
    required this.light,
    required this.dark,
    this.canvas = false,
    this.intensity = 1,
    this.press = 0,
  });
  final double intensity;
  final double press;
  final double radius;
  final Offset light;
  final bool dark, canvas;
  @override
  void paint(Canvas canvas, Size size) {
    if (size.isEmpty || intensity <= 0) return;
    final rect = (Offset.zero & size).deflate(.8);
    final rrect = RRect.fromRectAndRadius(rect, Radius.circular(radius));
    final glow = 1 + .38 * press;
    final gradient = LinearGradient(
      begin: Alignment(light.dx, light.dy),
      end: Alignment(-light.dx, -light.dy),
      colors: [
        Colors.white.withValues(
          alpha: ((dark ? .70 : .94) * intensity * glow)
              .clamp(0.0, 1.0)
              .toDouble(),
        ),
        Colors.white.withValues(alpha: .08 * intensity),
        const Color(0xFF77799F).withValues(alpha: .12 * intensity),
        Colors.white.withValues(
          alpha: (.55 * intensity * glow).clamp(0.0, 1.0).toDouble(),
        ),
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
      old.press != press ||
      old.radius != radius ||
      old.light != light ||
      old.dark != dark ||
      old.canvas != canvas;
}
