import 'package:flutter/material.dart';

import 'appearance.dart';
import 'neumorphic_controls.dart';
import 'style_input_border.dart';
import 'styled_checkbox.dart';

/// Keeps Material's controls in charge of state, focus, gestures and semantics.
ThemeData applyVisualStyleControls(ThemeData base, Palette palette) {
  final style = palette.surfaces.visualStyle;
  if (style == VisualStyle.flat) return base;
  if (style == VisualStyle.neumorphism) {
    return applyNeumorphicControls(base, palette);
  }

  ButtonStyle button(
    ButtonStyle? original,
    double radius, {
    bool filled = false,
  }) => (original ?? const ButtonStyle()).copyWith(
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

  OutlineInputBorder inputBorder(
    Color line,
    double width, {
    bool enabled = true,
  }) => ExperimentalInputBorder(
    palette: palette,
    style: style,
    enabled: enabled,
    borderRadius: palette.borderRadius(13),
    borderSide: BorderSide(color: line, width: width),
  );

  final lineWidth = switch (style) {
    VisualStyle.brutalist => 2.0,
    VisualStyle.industrial => 1.6,
    _ => 1.0,
  };
  final trackHeight = switch (style) {
    VisualStyle.brutalist => 6.0,
    VisualStyle.industrial => 7.0,
    VisualStyle.clay => 7.0,
    _ => 5.0,
  };
  final input = base.inputDecorationTheme;
  return base.copyWith(
    filledButtonTheme: FilledButtonThemeData(
      style: button(base.filledButtonTheme.style, 12, filled: true),
    ),
    outlinedButtonTheme: OutlinedButtonThemeData(
      style: button(base.outlinedButtonTheme.style, 12),
    ),
    textButtonTheme: TextButtonThemeData(
      style: button(base.textButtonTheme.style, 10),
    ),
    iconButtonTheme: IconButtonThemeData(
      style: button(base.iconButtonTheme.style, 12),
    ),
    inputDecorationTheme: input.copyWith(
      enabledBorder: inputBorder(palette.line, lineWidth),
      focusedBorder: inputBorder(palette.accent, lineWidth + .5),
      errorBorder: inputBorder(base.colorScheme.error, lineWidth),
      focusedErrorBorder: inputBorder(base.colorScheme.error, lineWidth + .5),
      disabledBorder: inputBorder(
        palette.line.withValues(alpha: palette.line.a * .45),
        lineWidth,
        enabled: false,
      ),
      border: inputBorder(palette.line, lineWidth),
    ),
    checkboxTheme: base.checkboxTheme.copyWith(
      shape: StyledCheckboxThemeShape(borderRadius: palette.borderRadius(5)),
      side: BorderSide(color: palette.line, width: lineWidth),
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
        style == VisualStyle.brutalist
            ? palette.ink.withValues(alpha: .8)
            : palette.line,
      ),
      trackOutlineWidth: WidgetStatePropertyAll(
        style == VisualStyle.industrial ? 2.5 : lineWidth,
      ),
      thumbColor: WidgetStateProperty.resolveWith(
        (states) => states.contains(WidgetState.disabled)
            ? palette.surface.withValues(alpha: .55)
            : style == VisualStyle.brutalist
            ? palette.ink
            : palette.surface,
      ),
      thumbIcon: style == VisualStyle.industrial
          ? const WidgetStatePropertyAll(
              Icon(Icons.power_settings_new_rounded, size: 14),
            )
          : style == VisualStyle.brutalist
          ? const WidgetStatePropertyAll(Icon(Icons.stop_rounded, size: 13))
          : null,
    ),
    sliderTheme: base.sliderTheme.copyWith(
      trackHeight: trackHeight,
      trackShape:
          style == VisualStyle.brutalist || style == VisualStyle.industrial
          ? const RectangularSliderTrackShape()
          : const RoundedRectSliderTrackShape(),
      thumbShape: ExperimentalSliderThumb(palette, style),
      activeTrackColor: palette.accent,
      inactiveTrackColor: palette.surface,
      thumbColor: palette.surface,
      disabledThumbColor: palette.surface.withValues(alpha: .5),
    ),
    chipTheme: base.chipTheme.copyWith(
      elevation: 0,
      pressElevation: 0,
      shadowColor: Colors.transparent,
      selectedShadowColor: Colors.transparent,
      side: BorderSide(color: palette.line, width: lineWidth),
      backgroundColor: palette.surface,
      selectedColor: Color.lerp(palette.surface, palette.accent, .16),
    ),
  );
}

/// Decorative outlines only. The center stays transparent unless fill is asked
/// for by the caller; the wrapped child retains its element and interaction.
class ExperimentalSurfacePainter extends CustomPainter {
  const ExperimentalSurfacePainter({
    required this.style,
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

  final VisualStyle style;
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

  Color _fade(Color color, {bool relief = true}) => color.withValues(
    alpha:
        color.a *
        opacity.clamp(0.0, 1.0) *
        (relief ? depth.abs().clamp(0.0, 1.0) : 1.0),
  );

  @override
  void paint(Canvas canvas, Size size) {
    if (size.isEmpty ||
        opacity <= 0 ||
        (depth == 0 && !fill && !focused && !highContrast)) {
      return;
    }
    final bounds = radius.toRRect(Offset.zero & size);
    final strength = depth.abs().clamp(0.0, 2.0);
    final enabledOpacity = enabled ? 1.0 : .38;
    final darkEdge = Color.lerp(
      const Color(0xFF37323D),
      Colors.black,
      darkness,
    )!.withValues(alpha: (_lightValue(.52, .72)) * enabledOpacity);
    final lightEdge = Colors.white.withValues(
      alpha: (_lightValue(.82, .34)) * enabledOpacity,
    );
    final shadow = Colors.black.withValues(
      alpha: (_lightValue(.18, .48)) * enabledOpacity,
    );
    final inset = depth < 0;
    if (fill && !inset) {
      canvas.drawRRect(bounds, Paint()..color = _fade(surface, relief: false));
    }
    if (highContrast) {
      _stroke(
        canvas,
        bounds.deflate(1),
        Color.lerp(Colors.black, Colors.white, darkness)!,
        2,
        relief: false,
      );
    } else if (depth != 0) {
      switch (style) {
        case VisualStyle.paper:
          if (inset) {
            _stroke(
              canvas,
              bounds,
              darkEdge.withValues(alpha: _lightValue(.28, .4)),
              1,
            );
            _edge(canvas, bounds.deflate(1), lightEdge, darkEdge, 1);
          } else {
            _stroke(
              canvas,
              bounds.shift(Offset(1.5 * strength, 2 * strength)),
              shadow,
              1.2 * strength,
            );
            _stroke(canvas, bounds, lightEdge, 1);
            _edge(canvas, bounds.deflate(.6), lightEdge, darkEdge, .7);
          }
        case VisualStyle.clay:
          if (inset) {
            _inset(canvas, bounds, shadow, lightEdge, 1.5 * strength, 4);
            _stroke(
              canvas,
              bounds.deflate(1),
              lightEdge.withValues(alpha: .3),
              1,
            );
          } else {
            _stroke(
              canvas,
              bounds.shift(Offset(strength, 1.5 * strength)),
              shadow,
              1.5 * strength,
              blur: 2.5,
            );
            _stroke(canvas, bounds.deflate(.8), lightEdge, 2 * strength);
            _edge(canvas, bounds.deflate(2), lightEdge, shadow, 1.5);
          }
        case VisualStyle.fluent:
          if (inset) {
            _stroke(canvas, bounds, darkEdge.withValues(alpha: .35), 1);
            _edge(canvas, bounds.deflate(1), shadow, lightEdge, .8);
          } else {
            _stroke(
              canvas,
              bounds.shift(Offset(0, 1.5 * strength)),
              shadow.withValues(alpha: _lightValue(.12, .22)),
              1.5,
              blur: 3,
            );
            _stroke(
              canvas,
              bounds,
              lightEdge.withValues(alpha: _lightValue(.65, .23)),
              1,
            );
            _edge(canvas, bounds.deflate(.7), lightEdge, shadow, .6);
          }
        case VisualStyle.brutalist:
          final ink = Color.lerp(
            const Color(0xFF211E26),
            Colors.white,
            darkness,
          )!;
          final line = ink.withValues(alpha: enabledOpacity);
          if (inset) {
            _stroke(canvas, bounds, line, 1.8);
            _edge(
              canvas,
              bounds.deflate(1.4),
              line.withValues(alpha: .45 * enabledOpacity),
              Colors.transparent,
              1,
            );
          } else {
            final shift = 2.5 * strength;
            final extrusion = Path.combine(
              PathOperation.difference,
              Path()..addRRect(bounds.shift(Offset(shift, shift))),
              Path()..addRRect(bounds),
            );
            canvas.drawPath(extrusion, Paint()..color = _fade(line));
            _stroke(canvas, bounds, line, 1.8);
          }
        case VisualStyle.industrial:
          if (inset) {
            _stroke(canvas, bounds, darkEdge, 2 * strength);
            _edge(canvas, bounds.deflate(1.5), shadow, lightEdge, 1.5);
            _stroke(
              canvas,
              bounds.deflate(3),
              lightEdge.withValues(alpha: .2),
              .7,
            );
          } else {
            _stroke(
              canvas,
              bounds.shift(Offset(1.5 * strength, 2 * strength)),
              shadow,
              2 * strength,
            );
            _stroke(canvas, bounds, darkEdge, 1.5);
            _edge(canvas, bounds.deflate(1.5), lightEdge, shadow, 1.5);
          }
        case VisualStyle.flat:
        case VisualStyle.neumorphism:
          break;
      }
    }
    if (focused) {
      _stroke(
        canvas,
        bounds.deflate(2),
        focusColor,
        highContrast ? 2.5 : 1.5,
        relief: false,
      );
    }
  }

  void _inset(
    Canvas canvas,
    RRect bounds,
    Color shade,
    Color light,
    double shift,
    double blur,
  ) {
    canvas.save();
    canvas.clipRRect(bounds);
    final outer = Path()
      ..addRect(bounds.outerRect.inflate(blur * 5 + shift + 2));
    for (final (offset, color) in [
      (Offset(shift, shift), shade),
      (Offset(-shift, -shift), light),
    ]) {
      final opening = Path()..addRRect(bounds.shift(offset));
      final wall = Path.combine(PathOperation.difference, outer, opening);
      canvas.drawPath(
        wall,
        Paint()
          ..color = _fade(color)
          ..maskFilter = MaskFilter.blur(BlurStyle.normal, blur),
      );
    }
    canvas.restore();
  }

  void _stroke(
    Canvas canvas,
    RRect shape,
    Color color,
    double width, {
    double blur = 0,
    bool relief = true,
  }) {
    canvas.drawRRect(
      shape,
      Paint()
        ..color = _fade(color, relief: relief)
        ..style = PaintingStyle.stroke
        ..strokeWidth = width
        ..maskFilter = blur > 0
            ? MaskFilter.blur(BlurStyle.normal, blur)
            : null,
    );
  }

  void _edge(
    Canvas canvas,
    RRect shape,
    Color top,
    Color bottom,
    double width,
  ) {
    canvas.drawRRect(
      shape,
      Paint()
        ..shader = LinearGradient(
          begin: Alignment.topLeft,
          end: Alignment.bottomRight,
          colors: [_fade(top), Colors.transparent, _fade(bottom)],
        ).createShader(shape.outerRect)
        ..style = PaintingStyle.stroke
        ..strokeWidth = width,
    );
  }

  @override
  bool shouldRepaint(covariant ExperimentalSurfacePainter old) =>
      opacity != old.opacity ||
      darkMix != old.darkMix ||
      style != old.style ||
      depth != old.depth ||
      radius != old.radius ||
      surface != old.surface ||
      dark != old.dark ||
      enabled != old.enabled ||
      highContrast != old.highContrast ||
      fill != old.fill ||
      focused != old.focused ||
      focusColor != old.focusColor;
}

/// Adds a visible recessed wall around clay and industrial text fields.
class ExperimentalInputBorder extends StyledInputBorder {
  const ExperimentalInputBorder({
    required this.palette,
    required this.style,
    this.enabled = true,
    this.clayInset,
    this.industrialInset,
    this.darkMix,
    this.depthMix,
    super.borderRadius,
    super.borderSide,
    super.gapPadding,
  });

  final Palette palette;
  final VisualStyle style;
  final bool enabled;
  // Fixed-size state: interpolation never retains prior borders or widgets.
  final double? clayInset, industrialInset, darkMix, depthMix;
  double get styleDepth => depthMix ?? palette.surfaces.styleDepth;
  double get darkness => darkMix ?? (palette.dark ? 1 : 0);
  double get clayWeight => clayInset ?? (style == VisualStyle.clay ? 1 : 0);
  double get industrialWeight =>
      industrialInset ?? (style == VisualStyle.industrial ? 1 : 0);

  @override
  InputRelief get relief => InputRelief(
    depth: styleDepth,
    clay: clayWeight,
    industrial: industrialWeight,
    darkness: darkness,
    enabled: enabled,
  );

  @override
  ExperimentalInputBorder copyWith({
    BorderSide? borderSide,
    BorderRadius? borderRadius,
    double? gapPadding,
  }) => ExperimentalInputBorder(
    palette: palette,
    style: style,
    enabled: enabled,
    clayInset: clayInset,
    industrialInset: industrialInset,
    darkMix: darkMix,
    depthMix: depthMix,
    borderSide: borderSide ?? this.borderSide,
    borderRadius: borderRadius ?? this.borderRadius,
    gapPadding: gapPadding ?? this.gapPadding,
  );

  @override
  ExperimentalInputBorder scale(double t) => ExperimentalInputBorder(
    palette: palette,
    style: style,
    enabled: enabled,
    clayInset: clayInset,
    industrialInset: industrialInset,
    darkMix: darkMix,
    depthMix: depthMix,
    borderSide: borderSide.scale(t),
    borderRadius: borderRadius * t,
    gapPadding: gapPadding * t,
  );

  @override
  ShapeBorder? lerpFrom(ShapeBorder? a, double t) {
    if (a is ExperimentalInputBorder) {
      if (t <= 0) return a;
      if (t >= 1) return this;
      return ExperimentalInputBorder(
        palette: palette,
        style: style,
        enabled: t < .5 ? a.enabled : enabled,
        darkMix: a.darkness + (darkness - a.darkness) * t,
        depthMix: a.styleDepth + (styleDepth - a.styleDepth) * t,
        clayInset: a.clayWeight + (clayWeight - a.clayWeight) * t,
        industrialInset:
            a.industrialWeight + (industrialWeight - a.industrialWeight) * t,
        borderSide: BorderSide.lerp(a.borderSide, borderSide, t),
        borderRadius: BorderRadius.lerp(a.borderRadius, borderRadius, t)!,
        gapPadding: a.gapPadding + (gapPadding - a.gapPadding) * t,
      );
    }
    return super.lerpFrom(a, t);
  }

  @override
  ShapeBorder? lerpTo(ShapeBorder? b, double t) {
    if (b is ExperimentalInputBorder) {
      return b.lerpFrom(this, t);
    }
    return super.lerpTo(b, t);
  }

  @override
  bool operator ==(Object other) =>
      other is ExperimentalInputBorder &&
      super == other &&
      style == other.style &&
      enabled == other.enabled &&
      clayWeight == other.clayWeight &&
      industrialWeight == other.industrialWeight &&
      darkness == other.darkness &&
      palette == other.palette;

  @override
  int get hashCode => Object.hash(
    super.hashCode,
    style,
    enabled,
    palette,
    clayWeight,
    industrialWeight,
    darkness,
  );
}

/// Material Slider keeps drag, focus and value behavior; only the thumb paints.
class ExperimentalSliderThumb extends SliderComponentShape {
  const ExperimentalSliderThumb(this.palette, this.style);

  final Palette palette;
  final VisualStyle style;

  @override
  Size getPreferredSize(bool isEnabled, bool isDiscrete) =>
      Size.square(style == VisualStyle.clay ? 22 : 20);

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
    final size = getPreferredSize(enableAnimation.value > .5, isDiscrete).width;
    final hard =
        style == VisualStyle.brutalist ||
        style == VisualStyle.industrial ||
        style == VisualStyle.paper;
    final rect = Rect.fromCenter(center: center, width: size, height: size);
    final rrect = RRect.fromRectAndRadius(
      rect,
      Radius.circular(hard ? (style == VisualStyle.paper ? 3 : 1) : size / 2),
    );
    final alpha = .45 + .55 * enableAnimation.value;
    if (style == VisualStyle.brutalist) {
      canvas.drawRRect(
        rrect.shift(const Offset(2, 2) * palette.surfaces.styleDepth),
        Paint()
          ..color = palette.ink.withValues(
            alpha: alpha * palette.surfaces.styleDepth.clamp(0, 1),
          ),
      );
    } else if (style == VisualStyle.clay) {
      canvas.drawRRect(
        rrect.shift(const Offset(1, 2) * palette.surfaces.styleDepth),
        Paint()
          ..color = Colors.black.withValues(
            alpha: .22 * alpha * palette.surfaces.styleDepth.clamp(0, 1),
          )
          ..maskFilter = const MaskFilter.blur(BlurStyle.normal, 4),
      );
    }
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
      rrect.deflate(.7),
      Paint()
        ..color = (style == VisualStyle.brutalist
            ? palette.ink.withValues(alpha: alpha)
            : palette.line.withValues(alpha: palette.line.a * alpha))
        ..style = PaintingStyle.stroke
        ..strokeWidth = style == VisualStyle.brutalist ? 2 : 1,
    );
    if (style == VisualStyle.industrial) {
      canvas.drawLine(
        center + const Offset(0, -5),
        center + const Offset(0, 5),
        Paint()
          ..color = palette.accent.withValues(alpha: alpha)
          ..strokeWidth = 2,
      );
    }
  }
}
