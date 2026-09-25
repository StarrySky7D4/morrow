import 'surface_paint_boundary.dart';
import 'dart:math' as math;

import 'package:flutter/material.dart';

import 'appearance.dart';
import 'experimental_controls.dart';
import 'style_input_border.dart';
import 'switch_icon_transition.dart';
import 'styled_checkbox.dart';

/// A visual layer only: the child keeps its own gestures, focus and semantics.
/// Positive depth is raised, negative depth is recessed; at zero only an
/// requested fill and explicit focus or high-contrast outlines remain.
class NeumorphicSurface extends StatefulWidget {
  const NeumorphicSurface({
    super.key,
    required this.palette,
    required this.child,
    this.depth = 1,
    this.borderRadius,
    this.enabled = true,
    this.fill = false,
    this.color,
    this.focused = false,
  });

  final Palette palette;
  final Widget child;
  final double depth;
  final BorderRadius? borderRadius;
  final bool enabled;
  final bool fill;
  final Color? color;
  final bool focused;

  @override
  State<NeumorphicSurface> createState() => _NeumorphicSurfaceState();
}

class _NeumorphicSurfaceState extends State<NeumorphicSurface>
    with SingleTickerProviderStateMixin {
  late final AnimationController _controller = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 160),
    value: 1,
  );
  late _SurfaceFrame _from = _SurfaceFrame.target(widget);
  late _SurfaceFrame _to = _from;
  bool _reduceMotion = false;
  bool _tickerEnabled = true;

  _SurfaceFrame get _displayed => _controller.value >= 1
      ? _to
      : _SurfaceFrame.lerp(
          _from,
          _to,
          Curves.easeOutCubic.transform(_controller.value),
        );

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    _reduceMotion = MediaQuery.disableAnimationsOf(context);
    _tickerEnabled = TickerMode.valuesOf(context).enabled;
    if ((_reduceMotion || !_tickerEnabled) && _controller.isAnimating) {
      _controller.stop();
      _from = _to;
      _controller.value = 1;
    }
  }

  @override
  void didUpdateWidget(covariant NeumorphicSurface oldWidget) {
    super.didUpdateWidget(oldWidget);
    final next = _SurfaceFrame.target(widget);
    if (_to.sameAs(next)) return;
    final current = _displayed;
    _from = current;
    _to = next;
    if (_reduceMotion ||
        !_tickerEnabled ||
        (current.depth == 0 && next.depth == 0)) {
      _controller.stop();
      _from = _to;
      _controller.value = 1;
    } else {
      _controller.forward(from: 0);
    }
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final highContrast = MediaQuery.highContrastOf(context);
    return AnimatedBuilder(
      animation: _controller,
      child: widget.child,
      builder: (context, child) {
        final frame = _displayed;
        final settled = _controller.value >= 1;
        final active =
            frame.activeWeight > 0 &&
            (frame.depth != 0 || widget.fill || widget.focused || highContrast);
        CustomPainter? edge;
        if (active) {
          if (settled) {
            final style = widget.palette.visualStyle;
            edge = style == VisualStyle.neumorphism
                ? NeumorphicSurfacePainter(
                    depth: frame.depth,
                    radius: frame.radius,
                    surface: frame.surface,
                    dark: frame.dark,
                    enabled: widget.enabled,
                    highContrast: highContrast,
                    fill: widget.fill && frame.depth > 0,
                    focused: widget.focused,
                    focusColor: frame.focusColor,
                  )
                : style == VisualStyle.flat
                ? null
                : ExperimentalSurfacePainter(
                    style: style,
                    depth: frame.depth,
                    radius: frame.radius,
                    surface: frame.surface,
                    dark: frame.dark,
                    enabled: widget.enabled,
                    highContrast: highContrast,
                    fill: widget.fill && frame.depth > 0,
                    focused: widget.focused,
                    focusColor: frame.focusColor,
                  );
          } else {
            edge = SurfaceStyleBlendPainter._(
              frame: frame,
              enabled: widget.enabled,
              highContrast: highContrast,
              fill: widget.fill && frame.depth > 0,
              focused: widget.focused,
            );
          }
        }
        return SurfacePaintBoundary(
          child: CustomPaint(
            painter: frame.depth > 0
                ? edge
                : widget.fill && active
                ? _NeumorphicFillPainter(
                    color: frame.surface,
                    radius: frame.radius,
                    opacity: frame.activeWeight,
                  )
                : null,
            foregroundPainter: frame.depth <= 0 ? edge : null,
            child: child,
          ),
        );
      },
    );
  }
}

/// A fixed seven-slot visual state. Reversals interpolate from the actual
/// displayed weights, so an interrupted A to B to C transition does not jump to B.
class _SurfaceFrame {
  _SurfaceFrame({
    required this.weights,
    required this.depth,
    required this.radius,
    required this.surface,
    required this.focusColor,
    required this.darkness,
  });

  factory _SurfaceFrame.target(NeumorphicSurface widget) {
    final weights = List<double>.filled(VisualStyle.values.length, 0);
    weights[widget.palette.visualStyle.index] = 1;
    return _SurfaceFrame(
      weights: weights,
      depth: widget.depth * widget.palette.surfaces.styleDepth,
      radius: widget.borderRadius ?? widget.palette.borderRadius(12),
      surface: widget.color ?? widget.palette.surface,
      focusColor: widget.palette.accent,
      darkness: widget.palette.dark ? 1 : 0,
    );
  }

  final List<double> weights;
  final double depth;
  final BorderRadius radius;
  final Color surface;
  final Color focusColor;
  final double darkness;
  bool get dark => darkness >= .5;

  double get activeWeight => 1 - weights[VisualStyle.flat.index];

  bool sameAs(_SurfaceFrame other) {
    if (depth != other.depth ||
        radius != other.radius ||
        surface != other.surface ||
        focusColor != other.focusColor ||
        darkness != other.darkness) {
      return false;
    }
    for (var i = 0; i < weights.length; i++) {
      if (weights[i] != other.weights[i]) {
        return false;
      }
    }
    return true;
  }

  static _SurfaceFrame lerp(_SurfaceFrame a, _SurfaceFrame b, double t) =>
      _SurfaceFrame(
        weights: List<double>.generate(
          VisualStyle.values.length,
          (i) => a.weights[i] + (b.weights[i] - a.weights[i]) * t,
        ),
        depth: a.depth + (b.depth - a.depth) * t,
        radius: BorderRadius.lerp(a.radius, b.radius, t)!,
        surface: Color.lerp(a.surface, b.surface, t)!,
        focusColor: Color.lerp(a.focusColor, b.focusColor, t)!,
        darkness: a.darkness + (b.darkness - a.darkness) * t,
      );
}

/// Paints a bounded blend of style edges. No extra child, opacity layer or
/// saveLayer is created; the seven weights only affect border geometry.
class SurfaceStyleBlendPainter extends CustomPainter {
  const SurfaceStyleBlendPainter._({
    required this._frame,
    required this.enabled,
    required this.highContrast,
    required this.fill,
    required this.focused,
  });

  final _SurfaceFrame _frame;
  final bool enabled;
  final bool highContrast;
  final bool fill;
  final bool focused;

  List<double> get styleWeights => List<double>.unmodifiable(_frame.weights);
  BorderRadius get radius => _frame.radius;
  Color get surface => _frame.surface;
  double get depth => _frame.depth;
  double get darkness => _frame.darkness;

  @override
  void paint(Canvas canvas, Size size) {
    if (size.isEmpty || (depth == 0 && !fill && !focused && !highContrast)) {
      return;
    }
    if (fill && _frame.activeWeight > 0) {
      canvas.drawRRect(
        _frame.radius.toRRect(Offset.zero & size),
        Paint()
          ..color = _frame.surface.withValues(
            alpha: _frame.surface.a * _frame.activeWeight,
          ),
      );
    }
    for (final style in VisualStyle.values) {
      final opacity = _frame.weights[style.index];
      if (style == VisualStyle.flat || opacity <= .001) {
        continue;
      }
      if (style == VisualStyle.neumorphism) {
        NeumorphicSurfacePainter(
          depth: depth,
          radius: _frame.radius,
          surface: _frame.surface,
          dark: _frame.dark,
          darkMix: darkness,
          enabled: enabled,
          highContrast: highContrast,
          fill: false,
          focused: focused,
          focusColor: _frame.focusColor,
          opacity: opacity,
        ).paint(canvas, size);
      } else {
        ExperimentalSurfacePainter(
          style: style,
          depth: depth,
          radius: _frame.radius,
          surface: _frame.surface,
          dark: _frame.dark,
          darkMix: darkness,
          enabled: enabled,
          highContrast: highContrast,
          fill: false,
          focused: focused,
          focusColor: _frame.focusColor,
          opacity: opacity,
        ).paint(canvas, size);
      }
    }
  }

  @override
  bool shouldRepaint(covariant SurfaceStyleBlendPainter old) =>
      !_frame.sameAs(old._frame) ||
      enabled != old.enabled ||
      highContrast != old.highContrast ||
      fill != old.fill ||
      focused != old.focused;
}

class _NeumorphicFillPainter extends CustomPainter {
  const _NeumorphicFillPainter({
    required this.color,
    required this.radius,
    this.opacity = 1,
  });

  final Color color;
  final BorderRadius radius;
  final double opacity;

  @override
  void paint(Canvas canvas, Size size) {
    canvas.drawRRect(
      radius.toRRect(Offset.zero & size),
      Paint()..color = color.withValues(alpha: color.a * opacity),
    );
  }

  @override
  bool shouldRepaint(covariant _NeumorphicFillPainter oldDelegate) =>
      color != oldDelegate.color ||
      radius != oldDelegate.radius ||
      opacity != oldDelegate.opacity;
}

/// Paints the same top-left light source for raised and recessed controls.
/// Recessed shadows are clipped to the *inside* of the rounded rectangle.
class NeumorphicSurfacePainter extends CustomPainter {
  const NeumorphicSurfacePainter({
    required this.depth,
    required this.radius,
    required this.surface,
    required this.dark,
    required this.enabled,
    required this.highContrast,
    required this.fill,
    required this.focused,
    required this.focusColor,
    this.opacity = 1,
    this.darkMix,
  });

  final double depth;
  final BorderRadius radius;
  final Color surface;
  final bool dark;
  final double? darkMix;
  double get darkness => (darkMix ?? (dark ? 1.0 : 0.0)).clamp(0.0, 1.0);
  double _lightValue(double light, double dark) =>
      light + (dark - light) * darkness;
  final bool enabled;
  final bool highContrast;
  final bool fill;
  final bool focused;
  final Color focusColor;
  final double opacity;

  @override
  void paint(Canvas canvas, Size size) {
    if (size.isEmpty || (depth == 0 && !fill && !focused && !highContrast)) {
      return;
    }
    final rect = Offset.zero & size;
    final rrect = radius.toRRect(rect);
    final strength = depth.abs().clamp(0.0, 2.0);
    final reliefStrength = strength.clamp(0.0, 1.0);
    final alpha =
        (enabled ? 1.0 : .38) *
        (highContrast ? 0.0 : 1.0) *
        opacity *
        reliefStrength;
    final light = Color.lerp(
      surface,
      Colors.white,
      _lightValue(.8, .28),
    )!.withValues(alpha: (_lightValue(.82, .20)) * alpha);
    final shade = Color.lerp(
      surface,
      Colors.black,
      _lightValue(.75, .85),
    )!.withValues(alpha: (_lightValue(.24, .52)) * alpha);
    final shift = 1.25 * strength;
    // Keep the Gaussian kernel stable while depth animates.
    const blur = 2.0;

    // Raised relief restores its fill after painting outer casts. Paint only
    // once: a translucent color must not become more opaque before a press.
    if (fill && (depth <= 0 || highContrast)) {
      canvas.drawRRect(
        rrect,
        Paint()..color = surface.withValues(alpha: surface.a * opacity),
      );
    }
    if (!highContrast && depth != 0) {
      if (depth > 0) {
        // Outer casts are visible on free surfaces. The inner edge also reads
        // clearly when Material clips a button's background builder.
        canvas.drawRRect(
          rrect.shift(Offset(-shift, -shift)),
          Paint()
            ..color = light
            ..style = PaintingStyle.stroke
            ..strokeWidth = 1.5 * strength
            ..maskFilter = MaskFilter.blur(BlurStyle.normal, blur),
        );
        canvas.drawRRect(
          rrect.shift(Offset(shift, shift)),
          Paint()
            ..color = shade
            ..style = PaintingStyle.stroke
            ..strokeWidth = 1.5 * strength
            ..maskFilter = MaskFilter.blur(BlurStyle.normal, blur),
        );
        if (fill) {
          canvas.drawRRect(
            rrect,
            Paint()..color = surface.withValues(alpha: surface.a * opacity),
          );
        }
        _paintEdge(canvas, rrect, light, shade, strength);
      } else {
        paintInset(canvas, rrect, shade, light, shift, blur);
      }
    }
    if (highContrast || focused) {
      canvas.drawRRect(
        rrect.deflate(1),
        Paint()
          ..color =
              (focused
                      ? focusColor
                      : Color.lerp(Colors.black, Colors.white, darkness)!)
                  .withValues(alpha: opacity)
          ..style = PaintingStyle.stroke
          ..strokeWidth = highContrast ? 2 : 1.5,
      );
    }
  }

  static void paintInset(
    Canvas canvas,
    RRect rrect,
    Color topShade,
    Color bottomLight,
    double shift,
    double blur,
  ) {
    canvas.save();
    canvas.clipRRect(rrect);
    // A blurred complement of a shifted opening creates a real inner shadow.
    // The two opposing offsets shade the upper-left and light the lower-right.
    final outer = Path()
      ..addRect(rrect.outerRect.inflate(blur * 5 + shift + 2));
    for (final (offset, color) in [
      (Offset(shift, shift), topShade),
      (Offset(-shift, -shift), bottomLight),
    ]) {
      final opening = Path()..addRRect(rrect.shift(offset));
      final wall = Path.combine(PathOperation.difference, outer, opening);
      canvas.drawPath(
        wall,
        Paint()
          ..color = color
          ..maskFilter = MaskFilter.blur(BlurStyle.normal, blur),
      );
    }
    canvas.restore();
  }

  static void _paintEdge(
    Canvas canvas,
    RRect rrect,
    Color light,
    Color shade,
    double strength,
  ) {
    final inner = rrect.deflate(
      math.min(1.2 * strength, rrect.outerRect.shortestSide / 5),
    );
    canvas.drawRRect(
      inner,
      Paint()
        ..shader = LinearGradient(
          begin: Alignment.topLeft,
          end: Alignment.bottomRight,
          colors: [light, Colors.transparent, shade],
        ).createShader(inner.outerRect)
        ..style = PaintingStyle.stroke
        ..strokeWidth = 1.2 * strength,
    );
  }

  @override
  bool shouldRepaint(covariant NeumorphicSurfacePainter oldDelegate) =>
      depth != oldDelegate.depth ||
      radius != oldDelegate.radius ||
      surface != oldDelegate.surface ||
      dark != oldDelegate.dark ||
      darkMix != oldDelegate.darkMix ||
      enabled != oldDelegate.enabled ||
      highContrast != oldDelegate.highContrast ||
      fill != oldDelegate.fill ||
      focused != oldDelegate.focused ||
      focusColor != oldDelegate.focusColor ||
      opacity != oldDelegate.opacity;
}

class NeumorphicInputBorder extends StyledInputBorder {
  const NeumorphicInputBorder({
    required this.surface,
    required this.dark,
    this.darkMix,
    this.styleDepth = 1,
    super.borderRadius,
    super.borderSide,
    super.gapPadding,
  });

  final Color surface;
  final bool dark;
  final double? darkMix;
  final double styleDepth;
  double get darkness => darkMix ?? (dark ? 1 : 0);

  @override
  InputRelief get relief =>
      InputRelief(neumorphic: 1, darkness: darkness, depth: styleDepth);

  @override
  NeumorphicInputBorder copyWith({
    BorderSide? borderSide,
    BorderRadius? borderRadius,
    double? gapPadding,
  }) => NeumorphicInputBorder(
    surface: surface,
    dark: dark,
    darkMix: darkMix,
    styleDepth: styleDepth,
    borderSide: borderSide ?? this.borderSide,
    borderRadius: borderRadius ?? this.borderRadius,
    gapPadding: gapPadding ?? this.gapPadding,
  );

  @override
  NeumorphicInputBorder scale(double t) => NeumorphicInputBorder(
    surface: surface,
    dark: dark,
    darkMix: darkMix,
    styleDepth: styleDepth,
    borderSide: borderSide.scale(t),
    borderRadius: borderRadius * t,
    gapPadding: gapPadding * t,
  );

  @override
  ShapeBorder? lerpFrom(ShapeBorder? a, double t) {
    if (a is NeumorphicInputBorder) {
      return NeumorphicInputBorder(
        surface: Color.lerp(a.surface, surface, t)!,
        dark: t < .5 ? a.dark : dark,
        darkMix: a.darkness + (darkness - a.darkness) * t,
        styleDepth: a.styleDepth + (styleDepth - a.styleDepth) * t,
        borderSide: BorderSide.lerp(a.borderSide, borderSide, t),
        borderRadius: BorderRadius.lerp(a.borderRadius, borderRadius, t)!,
        gapPadding: a.gapPadding + (gapPadding - a.gapPadding) * t,
      );
    }
    return super.lerpFrom(a, t);
  }

  @override
  ShapeBorder? lerpTo(ShapeBorder? b, double t) {
    if (b is NeumorphicInputBorder) return b.lerpFrom(this, t);
    return super.lerpTo(b, t);
  }

  @override
  bool operator ==(Object other) =>
      other is NeumorphicInputBorder &&
      super == other &&
      surface == other.surface &&
      darkness == other.darkness;

  @override
  int get hashCode => Object.hash(super.hashCode, surface, darkness);
}

/// Changes only visual theme fields. State, callbacks, padding, text and
/// accessibility behavior remain owned by each standard Material control.
ThemeData applyNeumorphicControls(ThemeData base, Palette palette) {
  if (palette.visualStyle != VisualStyle.neumorphism) return base;

  ButtonStyle buttonStyle(
    ButtonStyle? original,
    double radius, {
    bool filled = false,
  }) {
    return (original ?? const ButtonStyle()).copyWith(
      elevation: const WidgetStatePropertyAll(0),
      shadowColor: const WidgetStatePropertyAll(Colors.transparent),
      shape: WidgetStatePropertyAll(
        RoundedRectangleBorder(borderRadius: palette.borderRadius(radius)),
      ),
      backgroundBuilder: (context, states, child) => NeumorphicSurface(
        palette: palette,
        depth:
            states.contains(WidgetState.pressed) ||
                states.contains(WidgetState.selected)
            ? -1
            : 1,
        borderRadius: palette.borderRadius(radius),
        enabled: !states.contains(WidgetState.disabled),
        focused: states.contains(WidgetState.focused),
        color: filled ? palette.accent : palette.surface,
        child: child ?? const SizedBox.shrink(),
      ),
    );
  }

  final input = base.inputDecorationTheme;
  NeumorphicInputBorder border(Color line, double width) =>
      NeumorphicInputBorder(
        surface: palette.surface,
        dark: palette.dark,
        styleDepth: palette.surfaces.styleDepth,
        borderRadius: palette.borderRadius(13),
        borderSide: BorderSide(color: line, width: width),
      );

  return base.copyWith(
    filledButtonTheme: FilledButtonThemeData(
      style: buttonStyle(base.filledButtonTheme.style, 12, filled: true),
    ),
    outlinedButtonTheme: OutlinedButtonThemeData(
      style: buttonStyle(base.outlinedButtonTheme.style, 12),
    ),
    textButtonTheme: TextButtonThemeData(
      style: buttonStyle(base.textButtonTheme.style, 10),
    ),
    iconButtonTheme: IconButtonThemeData(
      style: buttonStyle(base.iconButtonTheme.style, 12),
    ),
    inputDecorationTheme: input.copyWith(
      enabledBorder: border(palette.line, 1),
      focusedBorder: border(palette.accent, 1.5),
      errorBorder: border(base.colorScheme.error, 1),
      focusedErrorBorder: border(base.colorScheme.error, 1.5),
      disabledBorder: border(palette.line.withValues(alpha: .45), 1),
      border: border(palette.line, 1),
    ),
    checkboxTheme: base.checkboxTheme.copyWith(
      shape: StyledCheckboxThemeShape(
        insetPalette: palette,
        borderRadius: palette.borderRadius(5),
      ),
      side: BorderSide(color: palette.line, width: 1.5),
    ),
    switchTheme: base.switchTheme.copyWith(
      trackColor: WidgetStateProperty.resolveWith((states) {
        final selected = states.contains(WidgetState.selected);
        final disabled = states.contains(WidgetState.disabled);
        final color = selected
            ? Color.lerp(palette.surface, palette.accent, .58)!
            : Color.lerp(palette.surface, palette.background, .42)!;
        return color.withValues(alpha: disabled ? .4 : 1);
      }),
      trackOutlineColor: WidgetStatePropertyAll(
        palette.dark ? Colors.black.withValues(alpha: .45) : palette.line,
      ),
      trackOutlineWidth: const WidgetStatePropertyAll(2),
      thumbColor: WidgetStateProperty.resolveWith((states) {
        // Contrast belongs to the native moving thumb, not a separately
        // positioned cast that would remain behind during a drag.
        final raised = Color.lerp(
          palette.surface,
          Colors.white,
          palette.dark ? .14 : .5,
        )!;
        return states.contains(WidgetState.disabled)
            ? raised.withValues(alpha: .55)
            : raised;
      }),
    ),
    sliderTheme: base.sliderTheme.copyWith(
      trackHeight: 7,
      trackShape: _NeumorphicSliderTrack(palette),
      thumbShape: _NeumorphicSliderThumb(palette),
      inactiveTrackColor: palette.surface,
      disabledInactiveTrackColor: palette.surface.withValues(
        alpha: palette.surface.a * .45,
      ),
      activeTrackColor: palette.accent.withValues(
        alpha: palette.accent.a * .76,
      ),
      disabledActiveTrackColor: palette.accent.withValues(
        alpha: palette.accent.a * .3,
      ),
      secondaryActiveTrackColor:
          base.sliderTheme.secondaryActiveTrackColor ??
          palette.accent.withValues(alpha: palette.accent.a * .36),
      disabledSecondaryActiveTrackColor:
          base.sliderTheme.disabledSecondaryActiveTrackColor ??
          palette.accent.withValues(alpha: palette.accent.a * .16),
      thumbColor: palette.surface,
      disabledThumbColor: palette.surface.withValues(alpha: .5),
    ),
    chipTheme: base.chipTheme.copyWith(
      elevation: 0,
      pressElevation: 0,
      shadowColor: Colors.transparent,
      selectedShadowColor: Colors.transparent,
      side: BorderSide(color: palette.line),
      backgroundColor: palette.surface,
      selectedColor: Color.lerp(palette.surface, palette.accent, .16),
    ),
  );
}

class _NeumorphicSliderTrack extends SliderTrackShape
    with BaseSliderTrackShape {
  const _NeumorphicSliderTrack(this.palette);
  final Palette palette;

  @override
  bool get isRounded => true;

  @override
  void paint(
    PaintingContext context,
    Offset offset, {
    required RenderBox parentBox,
    required SliderThemeData sliderTheme,
    required Animation<double> enableAnimation,
    required Offset thumbCenter,
    Offset? secondaryOffset,
    bool isEnabled = false,
    bool isDiscrete = false,
    required TextDirection textDirection,
  }) {
    if ((sliderTheme.trackHeight ?? 0) <= 0) return;
    final progress = enableAnimation.value.clamp(0.0, 1.0);
    Color animated(Color? disabled, Color? enabled, Color fallback) =>
        Color.lerp(
          disabled ?? fallback.withValues(alpha: fallback.a * .45),
          enabled ?? fallback,
          progress,
        )!;
    final rect = getPreferredRect(
      parentBox: parentBox,
      offset: offset,
      sliderTheme: sliderTheme,
      isEnabled: isEnabled,
      isDiscrete: isDiscrete,
    );
    final rrect = RRect.fromRectAndRadius(
      rect,
      Radius.circular(rect.height / 2),
    );
    final canvas = context.canvas;
    canvas.drawRRect(
      rrect,
      Paint()
        ..color = animated(
          sliderTheme.disabledInactiveTrackColor,
          sliderTheme.inactiveTrackColor,
          palette.surface,
        ),
    );
    final active = textDirection == TextDirection.ltr
        ? Rect.fromLTRB(rect.left, rect.top, thumbCenter.dx, rect.bottom)
        : Rect.fromLTRB(thumbCenter.dx, rect.top, rect.right, rect.bottom);
    canvas.save();
    canvas.clipRRect(rrect);
    canvas.drawRect(
      active,
      Paint()
        ..color = animated(
          sliderTheme.disabledActiveTrackColor,
          sliderTheme.activeTrackColor,
          palette.accent,
        ),
    );
    final isLTR = textDirection == TextDirection.ltr;
    if (secondaryOffset != null &&
        (isLTR
            ? secondaryOffset.dx > thumbCenter.dx
            : secondaryOffset.dx < thumbCenter.dx)) {
      final buffer = isLTR
          ? Rect.fromLTRB(
              thumbCenter.dx,
              rect.top,
              secondaryOffset.dx,
              rect.bottom,
            )
          : Rect.fromLTRB(
              secondaryOffset.dx,
              rect.top,
              thumbCenter.dx,
              rect.bottom,
            );
      final radius = Radius.circular(rect.height / 2);
      canvas.drawRRect(
        RRect.fromRectAndCorners(
          buffer,
          topLeft: isLTR ? Radius.zero : radius,
          bottomLeft: isLTR ? Radius.zero : radius,
          topRight: isLTR ? radius : Radius.zero,
          bottomRight: isLTR ? radius : Radius.zero,
        ),
        Paint()
          ..color = animated(
            sliderTheme.disabledSecondaryActiveTrackColor,
            sliderTheme.secondaryActiveTrackColor,
            palette.accent.withValues(alpha: .36),
          ),
      );
    }
    canvas.restore();
    final reliefOpacity =
        (.45 + .55 * progress) * palette.surfaces.styleDepth.clamp(0, 1);
    NeumorphicSurfacePainter.paintInset(
      canvas,
      rrect,
      Colors.black.withValues(
        alpha: (palette.dark ? .48 : .24) * reliefOpacity,
      ),
      Colors.white.withValues(
        alpha: (palette.dark ? .12 : .72) * reliefOpacity,
      ),
      1.5 * palette.surfaces.styleDepth,
      2.5,
    );
  }
}

class _NeumorphicSliderThumb extends SliderComponentShape {
  const _NeumorphicSliderThumb(this.palette);
  final Palette palette;

  @override
  Size getPreferredSize(bool isEnabled, bool isDiscrete) => const Size(20, 20);

  @override
  void paint(
    PaintingContext context,
    Offset center, {
    required Animation<double> activationAnimation,
    required Animation<double> enableAnimation,
    required bool isDiscrete,
    required TextPainter labelPainter,
    required RenderBox parentBox,
    required SliderThemeData sliderTheme,
    required TextDirection textDirection,
    required double value,
    required double textScaleFactor,
    required Size sizeWithOverflow,
  }) {
    final canvas = context.canvas;
    final alpha = .45 + .55 * enableAnimation.value;
    final size = 20.0;
    final rrect = RRect.fromRectAndRadius(
      Rect.fromCenter(center: center, width: size, height: size),
      const Radius.circular(10),
    );
    canvas.drawRRect(
      rrect.shift(const Offset(-1.5, -1.5) * palette.surfaces.styleDepth),
      Paint()
        ..color = Colors.white.withValues(
          alpha:
              (palette.dark ? .16 : .8) *
              alpha *
              palette.surfaces.styleDepth.clamp(0, 1),
        )
        ..maskFilter = const MaskFilter.blur(BlurStyle.normal, 3),
    );
    canvas.drawRRect(
      rrect.shift(const Offset(1.5, 1.5) * palette.surfaces.styleDepth),
      Paint()
        ..color = Colors.black.withValues(
          alpha:
              (palette.dark ? .55 : .25) *
              alpha *
              palette.surfaces.styleDepth.clamp(0, 1),
        )
        ..maskFilter = const MaskFilter.blur(BlurStyle.normal, 3),
    );
    canvas.drawRRect(
      rrect,
      Paint()
        ..color = Color.lerp(
          sliderTheme.disabledThumbColor ??
              palette.surface.withValues(alpha: palette.surface.a * .45),
          sliderTheme.thumbColor ?? palette.surface,
          enableAnimation.value.clamp(0.0, 1.0),
        )!,
    );
    canvas.drawRRect(
      rrect.deflate(.6),
      Paint()
        ..color = palette.line.withValues(alpha: palette.line.a * alpha)
        ..style = PaintingStyle.stroke,
    );
  }
}

/// Uses the native Switch for gestures, keyboard focus, animation and semantics.
/// Its exact Material track bounds receive the recessed painter underneath it.
/// Adaptive switches on Apple platforms keep their native Cupertino appearance.
class NeumorphicSwitch extends StatefulWidget {
  const NeumorphicSwitch({
    super.key,
    required this.palette,
    required this.value,
    required this.onChanged,
    this.adaptive = false,
    this.focusNode,
    this.autofocus = false,
    this.activeThumbColor,
    this.materialTapTargetSize,
    this.mouseCursor,
    this.thumbIcon,
  });

  final Palette palette;
  final bool value;
  final ValueChanged<bool>? onChanged;
  final bool adaptive;
  final FocusNode? focusNode;
  final bool autofocus;
  final Color? activeThumbColor;
  final MaterialTapTargetSize? materialTapTargetSize;
  final MouseCursor? mouseCursor;
  final WidgetStateProperty<Icon?>? thumbIcon;

  @override
  State<NeumorphicSwitch> createState() => _NeumorphicSwitchState();
}

class _NeumorphicSwitchState extends State<NeumorphicSwitch>
    with SingleTickerProviderStateMixin {
  late final _controller = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 180),
    value: 1,
  );
  _SwitchRelief? _from, _to;
  bool _motion = true, _apple = false, _highContrast = false;

  _SwitchRelief get displayed => _controller.value == 1
      ? _to!
      : _SwitchRelief.lerp(
          _from!,
          _to!,
          Curves.easeOutCubic.transform(_controller.value),
        );

  void retarget() {
    final p = widget.palette;
    final next = _SwitchRelief(
      amount:
          p.visualStyle == VisualStyle.neumorphism && !_apple && !_highContrast
          ? 1
          : 0,
      depth: p.surfaces.styleDepth,
      darkness: p.dark ? 1 : 0,
      enabled: widget.onChanged == null ? 0 : 1,
      surface: p.surface,
      accent: p.accent,
    );
    if (_to == null) {
      _from = _to = next;
    } else if (!_to!.sameAs(next)) {
      _from = displayed;
      _to = next;
      if (_motion && !_apple && !_highContrast) {
        _controller.forward(from: 0);
      } else {
        _controller.stop();
        _from = _to;
        _controller.value = 1;
      }
    }
    if (!_motion || _apple || _highContrast) {
      _controller.stop();
      _from = _to;
      _controller.value = 1;
    }
  }

  void readEnvironment() {
    final theme = Theme.of(context);
    _apple =
        widget.adaptive &&
        (theme.platform == TargetPlatform.iOS ||
            theme.platform == TargetPlatform.macOS);
    _highContrast = MediaQuery.highContrastOf(context);
    _motion =
        !MediaQuery.disableAnimationsOf(context) &&
        TickerMode.valuesOf(context).enabled;
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    readEnvironment();
    retarget();
  }

  @override
  void didUpdateWidget(NeumorphicSwitch oldWidget) {
    super.didUpdateWidget(oldWidget);
    readEnvironment();
    retarget();
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => AnimatedBuilder(
    animation: _controller,
    builder: (context, _) {
      final theme = Theme.of(context);
      final frame = displayed;
      final weight = frame.amount;
      // Resolve fallbacks only during the handoff. At zero native defaults,
      // custom SwitchThemes and Cupertino adaptations own the entire track.
      final track = weight == 0
          ? null
          : WidgetStateProperty.resolveWith<Color?>((states) {
              final color =
                  SwitchTheme.of(context).trackColor?.resolve(states) ??
                  (states.contains(WidgetState.selected) &&
                          !states.contains(WidgetState.disabled) &&
                          widget.activeThumbColor != null
                      ? widget.activeThumbColor!.withAlpha(0x80)
                      : _switchTrackFallback(theme, states));
              return color.withValues(alpha: color.a * (1 - weight));
            });
      final outline = weight == 0
          ? null
          : WidgetStateProperty.resolveWith<Color?>((states) {
              final color =
                  SwitchTheme.of(context).trackOutlineColor?.resolve(states) ??
                  (!theme.useMaterial3 || states.contains(WidgetState.selected)
                      ? Colors.transparent
                      : states.contains(WidgetState.disabled)
                      ? theme.colorScheme.onSurface.withValues(alpha: .12)
                      : theme.colorScheme.outline);
              return color.withValues(alpha: color.a * (1 - weight));
            });
      // Keep an icon slot in all Material styles. A transparent slot uses the
      // same native thumb radius, avoiding a size jump when glyphs appear.
      final targetIcons = WidgetStateProperty.resolveWith<Icon?>((states) {
        final custom = widget.thumbIcon?.resolve(states);
        if (custom != null) return custom;
        final glyph = switch (widget.palette.visualStyle) {
          VisualStyle.industrial => Icons.power_settings_new_rounded,
          VisualStyle.brutalist => Icons.stop_rounded,
          _ => null,
        };
        return Icon(
          glyph,
          size: 14,
          color:
              (widget.palette.visualStyle == VisualStyle.brutalist
                      ? widget.palette.surface
                      : widget.palette.ink)
                  .withValues(
                    alpha: states.contains(WidgetState.disabled) ? .38 : 1,
                  ),
        );
      });
      return SwitchIconTransition(
        icons: targetIcons,
        animate: _motion && !_apple && !_highContrast,
        fallbackColor: widget.palette.ink,
        builder: (context, icons) {
          final native = widget.adaptive
              ? Switch.adaptive(
                  value: widget.value,
                  onChanged: widget.onChanged,
                  activeThumbColor: widget.activeThumbColor,
                  focusNode: widget.focusNode,
                  autofocus: widget.autofocus,
                  materialTapTargetSize: widget.materialTapTargetSize,
                  mouseCursor: widget.mouseCursor,
                  thumbIcon: _apple ? widget.thumbIcon : icons,
                  trackColor: track,
                  trackOutlineColor: outline,
                )
              : Switch(
                  value: widget.value,
                  onChanged: widget.onChanged,
                  activeThumbColor: widget.activeThumbColor,
                  focusNode: widget.focusNode,
                  autofocus: widget.autofocus,
                  materialTapTargetSize: widget.materialTapTargetSize,
                  mouseCursor: widget.mouseCursor,
                  thumbIcon: _apple ? widget.thumbIcon : icons,
                  trackColor: track,
                  trackOutlineColor: outline,
                );
          return TweenAnimationBuilder<double>(
            tween: Tween<double>(
              begin: widget.value ? 1 : 0,
              end: widget.value ? 1 : 0,
            ),
            duration: _motion
                ? motionDuration(context, theme.useMaterial3 ? 300 : 200)
                : Duration.zero,
            curve: theme.useMaterial3 ? Curves.easeOutBack : Curves.easeInOut,
            child: native,
            builder: (context, position, child) => CustomPaint(
              painter: weight == 0
                  ? null
                  : _NeumorphicSwitchPainter(
                      frame: frame,
                      position: position,
                      textDirection: Directionality.of(context),
                      material3: theme.useMaterial3,
                    ),
              child: child,
            ),
          );
        },
      );
    },
  );
}

// Matches the installed Material Switch defaults. Keep endpoint tests against
// the native control when upgrading Flutter; no fallback applies on Cupertino.
Color _switchTrackFallback(ThemeData theme, Set<WidgetState> states) {
  final disabled = states.contains(WidgetState.disabled);
  final selected = states.contains(WidgetState.selected);
  if (theme.useMaterial3) {
    if (disabled) {
      return (selected
              ? theme.colorScheme.onSurface
              : theme.colorScheme.surfaceContainerHighest)
          .withValues(alpha: .12);
    }
    return selected
        ? theme.colorScheme.primary
        : theme.colorScheme.surfaceContainerHighest;
  }
  final dark = theme.brightness == Brightness.dark;
  if (disabled) return dark ? Colors.white10 : Colors.black12;
  if (selected) return theme.colorScheme.secondary.withAlpha(0x80);
  return dark ? Colors.white30 : const Color(0x52000000);
}

@immutable
class _SwitchRelief {
  const _SwitchRelief({
    required this.amount,
    required this.depth,
    required this.darkness,
    required this.enabled,
    required this.surface,
    required this.accent,
  });
  final double amount, depth, darkness, enabled;
  final Color surface, accent;
  bool sameAs(_SwitchRelief b) =>
      amount == b.amount &&
      depth == b.depth &&
      darkness == b.darkness &&
      enabled == b.enabled &&
      surface == b.surface &&
      accent == b.accent;
  static _SwitchRelief lerp(_SwitchRelief a, _SwitchRelief b, double t) =>
      _SwitchRelief(
        amount: a.amount + (b.amount - a.amount) * t,
        depth: a.depth + (b.depth - a.depth) * t,
        darkness: a.darkness + (b.darkness - a.darkness) * t,
        enabled: a.enabled + (b.enabled - a.enabled) * t,
        surface: Color.lerp(a.surface, b.surface, t)!,
        accent: Color.lerp(a.accent, b.accent, t)!,
      );
}

class _NeumorphicSwitchPainter extends CustomPainter {
  const _NeumorphicSwitchPainter({
    required this.frame,
    required this.position,
    required this.textDirection,
    required this.material3,
  });

  final _SwitchRelief frame;
  final double position;
  final TextDirection textDirection;
  final bool material3;

  @override
  void paint(Canvas canvas, Size size) {
    final width = material3 ? 52.0 : 33.0;
    final height = material3 ? 32.0 : 14.0;
    final rect = Rect.fromCenter(
      center: size.center(Offset.zero),
      width: width,
      height: height,
    );
    final rrect = RRect.fromRectAndRadius(rect, Radius.circular(height / 2));
    final opacity = (.42 + .58 * frame.enabled) * frame.amount;
    final darkness = frame.darkness;
    final relief = opacity * frame.depth.clamp(0, 1);
    final fill = Color.lerp(
      Color.lerp(frame.surface, Colors.black, .02 + .10 * darkness)!,
      frame.accent,
      position * .55,
    )!;
    canvas.drawRRect(rrect, Paint()..color = fill.withValues(alpha: opacity));
    NeumorphicSurfacePainter.paintInset(
      canvas,
      rrect,
      Colors.black.withValues(alpha: (.23 + .27 * darkness) * relief),
      Colors.white.withValues(alpha: (.8 - .68 * darkness) * relief),
      2 * frame.depth,
      3,
    );
    // The native switch alone owns its moving thumb and reaction. A second
    // logical-value shadow cannot follow native drag/pressed geometry.
  }

  @override
  bool shouldRepaint(covariant _NeumorphicSwitchPainter oldDelegate) =>
      !frame.sameAs(oldDelegate.frame) ||
      position != oldDelegate.position ||
      textDirection != oldDelegate.textDirection ||
      material3 != oldDelegate.material3;
}

/// Keeps the native Checkbox, including its tristate cycle and semantics.
/// The custom border paints *inside* Flutter's 18px checkbox shape.
class NeumorphicCheckbox extends StatelessWidget {
  const NeumorphicCheckbox({
    super.key,
    required this.palette,
    required this.value,
    required this.onChanged,
    this.tristate = false,
    this.adaptive = false,
    this.focusNode,
    this.autofocus = false,
    this.semanticLabel,
    this.isError = false,
  });

  final Palette palette;
  final bool? value;
  final ValueChanged<bool?>? onChanged;
  final bool tristate;
  final bool adaptive;
  final FocusNode? focusNode;
  final bool autofocus;
  final String? semanticLabel;
  final bool isError;

  @override
  Widget build(BuildContext context) => StyledCheckbox(
    palette: palette,
    value: value,
    onChanged: onChanged,
    tristate: tristate,
    adaptive: adaptive,
    focusNode: focusNode,
    autofocus: autofocus,
    semanticLabel: semanticLabel,
    isError: isError,
  );
}

/// A native ChoiceChip with state-specific bevels at its actual chip bounds.
class NeumorphicChoiceChip extends StatelessWidget {
  const NeumorphicChoiceChip({
    super.key,
    required this.palette,
    required this.label,
    required this.selected,
    this.onSelected,
    this.avatar,
    this.tooltip,
    this.focusNode,
    this.autofocus = false,
  });

  final Palette palette;
  final Widget label;
  final bool selected;
  final ValueChanged<bool>? onSelected;
  final Widget? avatar;
  final String? tooltip;
  final FocusNode? focusNode;
  final bool autofocus;

  @override
  Widget build(BuildContext context) {
    final active =
        palette.visualStyle == VisualStyle.neumorphism &&
        !MediaQuery.highContrastOf(context);
    return ChoiceChip(
      avatar: avatar,
      label: label,
      selected: selected,
      onSelected: onSelected,
      tooltip: tooltip,
      focusNode: focusNode,
      autofocus: autofocus,
      shape: active
          ? _NeumorphicChipBorder(
              palette: palette,
              selected: selected,
              enabled: onSelected != null,
            )
          : null,
    );
  }
}

class _NeumorphicChipBorder extends StadiumBorder {
  const _NeumorphicChipBorder({
    required this.palette,
    required this.selected,
    required this.enabled,
    super.side,
  });

  final Palette palette;
  final bool selected;
  final bool enabled;

  @override
  _NeumorphicChipBorder copyWith({BorderSide? side}) => _NeumorphicChipBorder(
    palette: palette,
    selected: selected,
    enabled: enabled,
    side: side ?? this.side,
  );

  @override
  bool operator ==(Object other) =>
      other is _NeumorphicChipBorder &&
      super == other &&
      identical(palette, other.palette) &&
      selected == other.selected &&
      enabled == other.enabled;

  @override
  int get hashCode => Object.hash(super.hashCode, palette, selected, enabled);
  @override
  void paint(Canvas canvas, Rect rect, {TextDirection? textDirection}) {
    super.paint(canvas, rect, textDirection: textDirection);
    final rrect = RRect.fromRectAndRadius(
      rect,
      Radius.circular(rect.height / 2),
    );
    final opacity =
        (enabled ? 1.0 : .38) * palette.surfaces.styleDepth.clamp(0, 1);
    if (selected) {
      NeumorphicSurfacePainter.paintInset(
        canvas,
        rrect,
        Colors.black.withValues(alpha: (palette.dark ? .48 : .22) * opacity),
        Colors.white.withValues(alpha: (palette.dark ? .14 : .72) * opacity),
        1.5 * palette.surfaces.styleDepth,
        3,
      );
    } else {
      NeumorphicSurfacePainter._paintEdge(
        canvas,
        rrect,
        Colors.white.withValues(alpha: (palette.dark ? .18 : .75) * opacity),
        Colors.black.withValues(alpha: (palette.dark ? .48 : .22) * opacity),
        palette.surfaces.styleDepth,
      );
    }
  }
}

/// SwitchListTile layout and one merged semantic control with the recessed
/// native switch track. The ListTile owns the row tap and focus as in Flutter.
class NeumorphicSwitchListTile extends StatelessWidget {
  const NeumorphicSwitchListTile({
    super.key,
    required this.palette,
    required this.value,
    required this.onChanged,
    this.title,
    this.subtitle,
    this.secondary,
    this.contentPadding,
    this.focusNode,
    this.autofocus = false,
    this.adaptive = true,
    this.selected = false,
    this.dense,
  });

  final Palette palette;
  final bool value;
  final ValueChanged<bool>? onChanged;
  final Widget? title;
  final Widget? subtitle;
  final Widget? secondary;
  final EdgeInsetsGeometry? contentPadding;
  final FocusNode? focusNode;
  final bool autofocus;
  final bool adaptive;
  final bool selected;
  final bool? dense;

  @override
  Widget build(BuildContext context) => MergeSemantics(
    child: ListTile(
      title: title,
      subtitle: subtitle,
      leading: secondary,
      trailing: ExcludeFocus(
        child: NeumorphicSwitch(
          palette: palette,
          value: value,
          onChanged: onChanged,
          adaptive: adaptive,
          materialTapTargetSize: MaterialTapTargetSize.shrinkWrap,
        ),
      ),
      enabled: onChanged != null,
      onTap: onChanged == null ? null : () => onChanged!(!value),
      contentPadding: contentPadding,
      focusNode: focusNode,
      autofocus: autofocus,
      selected: selected,
      dense: dense,
    ),
  );
}
