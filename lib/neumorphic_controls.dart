import 'dart:math' as math;

import 'package:flutter/material.dart';

import 'appearance.dart';

/// A visual layer only: the child keeps its own gestures, focus and semantics.
/// Positive depth is raised, negative depth is recessed, and zero is unchanged.
class NeumorphicSurface extends StatelessWidget {
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
  Widget build(BuildContext context) {
    final active =
        palette.surfaces.visualStyle == VisualStyle.neumorphism && depth != 0;
    final targetDepth = active ? depth : 0.0;
    final highContrast = MediaQuery.highContrastOf(context);
    return TweenAnimationBuilder<double>(
      tween: Tween<double>(begin: targetDepth, end: targetDepth),
      duration: motionDuration(context, 160),
      curve: Curves.easeOutCubic,
      child: child,
      builder: (context, paintedDepth, child) {
        final painter = paintedDepth == 0
            ? null
            : NeumorphicSurfacePainter(
                depth: paintedDepth,
                radius: borderRadius ?? palette.borderRadius(12),
                surface: color ?? palette.surface,
                dark: palette.dark,
                enabled: enabled,
                highContrast: highContrast,
                fill: fill && paintedDepth > 0,
                focused: focused,
                focusColor: palette.accent,
              );
        return CustomPaint(
          painter: paintedDepth > 0
              ? painter
              : paintedDepth < 0 && fill
              ? _NeumorphicFillPainter(
                  color: color ?? palette.surface,
                  radius: borderRadius ?? palette.borderRadius(12),
                )
              : null,
          foregroundPainter: paintedDepth < 0 ? painter : null,
          child: child,
        );
      },
    );
  }
}

class _NeumorphicFillPainter extends CustomPainter {
  const _NeumorphicFillPainter({required this.color, required this.radius});

  final Color color;
  final BorderRadius radius;

  @override
  void paint(Canvas canvas, Size size) {
    canvas.drawRRect(
      radius.toRRect(Offset.zero & size),
      Paint()..color = color,
    );
  }

  @override
  bool shouldRepaint(covariant _NeumorphicFillPainter oldDelegate) =>
      color != oldDelegate.color || radius != oldDelegate.radius;
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
  });

  final double depth;
  final BorderRadius radius;
  final Color surface;
  final bool dark;
  final bool enabled;
  final bool highContrast;
  final bool fill;
  final bool focused;
  final Color focusColor;

  @override
  void paint(Canvas canvas, Size size) {
    if (size.isEmpty || depth == 0) return;
    final rect = Offset.zero & size;
    final rrect = radius.toRRect(rect);
    final strength = depth.abs().clamp(0.0, 2.0);
    final alpha = (enabled ? 1.0 : .38) * (highContrast ? 0.0 : 1.0);
    final light = Color.lerp(
      surface,
      Colors.white,
      dark ? .28 : .8,
    )!.withValues(alpha: (dark ? .20 : .82) * alpha);
    final shade = Color.lerp(
      surface,
      Colors.black,
      dark ? .85 : .75,
    )!.withValues(alpha: (dark ? .52 : .24) * alpha);
    final shift = 2.0 * strength;
    final blur = 4.0 * strength;

    if (fill) canvas.drawRRect(rrect, Paint()..color = surface);
    if (!highContrast) {
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
        if (fill) canvas.drawRRect(rrect, Paint()..color = surface);
        _paintEdge(canvas, rrect, light, shade, strength);
      } else {
        _paintInset(canvas, rrect, shade, light, shift, blur);
      }
    }
    if (highContrast || focused) {
      canvas.drawRRect(
        rrect.deflate(1),
        Paint()
          ..color = focused ? focusColor : (dark ? Colors.white : Colors.black)
          ..style = PaintingStyle.stroke
          ..strokeWidth = highContrast ? 2 : 1.5,
      );
    }
  }

  static void _paintInset(
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
      enabled != oldDelegate.enabled ||
      highContrast != oldDelegate.highContrast ||
      fill != oldDelegate.fill ||
      focused != oldDelegate.focused ||
      focusColor != oldDelegate.focusColor;
}

class NeumorphicInputBorder extends OutlineInputBorder {
  const NeumorphicInputBorder({
    required this.surface,
    required this.dark,
    super.borderRadius,
    super.borderSide,
    super.gapPadding,
  });

  final Color surface;
  final bool dark;

  @override
  NeumorphicInputBorder copyWith({
    BorderSide? borderSide,
    BorderRadius? borderRadius,
    double? gapPadding,
  }) => NeumorphicInputBorder(
    surface: surface,
    dark: dark,
    borderSide: borderSide ?? this.borderSide,
    borderRadius: borderRadius ?? this.borderRadius,
    gapPadding: gapPadding ?? this.gapPadding,
  );

  @override
  NeumorphicInputBorder scale(double t) => NeumorphicInputBorder(
    surface: surface,
    dark: dark,
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
      dark == other.dark;

  @override
  int get hashCode => Object.hash(super.hashCode, surface, dark);

  @override
  void paint(
    Canvas canvas,
    Rect rect, {
    double? gapStart,
    double gapExtent = 0,
    double gapPercentage = 0,
    TextDirection? textDirection,
  }) {
    NeumorphicSurfacePainter._paintInset(
      canvas,
      borderRadius.toRRect(rect),
      Colors.black.withValues(alpha: dark ? .48 : .20),
      Colors.white.withValues(alpha: dark ? .14 : .75),
      2,
      4,
    );
    super.paint(
      canvas,
      rect,
      gapStart: gapStart,
      gapExtent: gapExtent,
      gapPercentage: gapPercentage,
      textDirection: textDirection,
    );
  }
}

/// Changes only visual theme fields. State, callbacks, padding, text and
/// accessibility behavior remain owned by each standard Material control.
ThemeData applyNeumorphicControls(ThemeData base, Palette palette) {
  if (palette.surfaces.visualStyle != VisualStyle.neumorphism) return base;

  ButtonStyle buttonStyle(
    ButtonStyle? original,
    double radius, {
    bool filled = false,
  }) {
    return (original ?? const ButtonStyle()).copyWith(
      elevation: const WidgetStatePropertyAll(0),
      shadowColor: const WidgetStatePropertyAll(Colors.transparent),
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
      shape: _NeumorphicCheckboxBorder(
        palette: palette,
        enabled: true,
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
      thumbColor: WidgetStateProperty.resolveWith(
        (states) => states.contains(WidgetState.disabled)
            ? palette.surface.withValues(alpha: .55)
            : palette.surface,
      ),
    ),
    sliderTheme: base.sliderTheme.copyWith(
      trackHeight: 7,
      trackShape: _NeumorphicSliderTrack(palette),
      thumbShape: _NeumorphicSliderThumb(palette),
      inactiveTrackColor: palette.surface,
      activeTrackColor: palette.accent,
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
        ..color = isEnabled
            ? palette.surface
            : palette.surface.withValues(alpha: .45),
    );
    final active = textDirection == TextDirection.ltr
        ? Rect.fromLTRB(rect.left, rect.top, thumbCenter.dx, rect.bottom)
        : Rect.fromLTRB(thumbCenter.dx, rect.top, rect.right, rect.bottom);
    canvas.save();
    canvas.clipRRect(rrect);
    canvas.drawRect(
      active,
      Paint()
        ..color = isEnabled
            ? palette.accent.withValues(alpha: .76)
            : palette.accent.withValues(alpha: .3),
    );
    canvas.restore();
    NeumorphicSurfacePainter._paintInset(
      canvas,
      rrect,
      Colors.black.withValues(alpha: palette.dark ? .48 : .24),
      Colors.white.withValues(alpha: palette.dark ? .12 : .72),
      1.5,
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
      rrect.shift(const Offset(-1.5, -1.5)),
      Paint()
        ..color = Colors.white.withValues(
          alpha: (palette.dark ? .16 : .8) * alpha,
        )
        ..maskFilter = const MaskFilter.blur(BlurStyle.normal, 3),
    );
    canvas.drawRRect(
      rrect.shift(const Offset(1.5, 1.5)),
      Paint()
        ..color = Colors.black.withValues(
          alpha: (palette.dark ? .55 : .25) * alpha,
        )
        ..maskFilter = const MaskFilter.blur(BlurStyle.normal, 3),
    );
    canvas.drawRRect(
      rrect,
      Paint()..color = palette.surface.withValues(alpha: alpha),
    );
    canvas.drawRRect(
      rrect.deflate(.6),
      Paint()
        ..color = palette.line
        ..style = PaintingStyle.stroke,
    );
  }
}

/// Uses the native Switch for gestures, keyboard focus, animation and semantics.
/// Its exact Material track bounds receive the recessed painter underneath it.
/// Adaptive switches on Apple platforms keep their native Cupertino appearance.
class NeumorphicSwitch extends StatelessWidget {
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
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final appleAdaptive =
        adaptive &&
        (theme.platform == TargetPlatform.iOS ||
            theme.platform == TargetPlatform.macOS);
    final active =
        palette.surfaces.visualStyle == VisualStyle.neumorphism &&
        !appleAdaptive &&
        !MediaQuery.highContrastOf(context);
    const transparent = WidgetStatePropertyAll<Color>(Colors.transparent);
    final native = adaptive
        ? Switch.adaptive(
            value: value,
            onChanged: onChanged,
            activeThumbColor: activeThumbColor,
            focusNode: focusNode,
            autofocus: autofocus,
            materialTapTargetSize: materialTapTargetSize,
            mouseCursor: mouseCursor,
            thumbIcon: thumbIcon,
            trackColor: active ? transparent : null,
            trackOutlineColor: active ? transparent : null,
            trackOutlineWidth: active
                ? const WidgetStatePropertyAll<double>(0)
                : null,
          )
        : Switch(
            value: value,
            onChanged: onChanged,
            activeThumbColor: activeThumbColor,
            focusNode: focusNode,
            autofocus: autofocus,
            materialTapTargetSize: materialTapTargetSize,
            mouseCursor: mouseCursor,
            thumbIcon: thumbIcon,
            trackColor: active ? transparent : null,
            trackOutlineColor: active ? transparent : null,
            trackOutlineWidth: active
                ? const WidgetStatePropertyAll<double>(0)
                : null,
          );
    return TweenAnimationBuilder<double>(
      tween: Tween<double>(begin: value ? 1 : 0, end: value ? 1 : 0),
      duration: motionDuration(context, theme.useMaterial3 ? 300 : 200),
      curve: theme.useMaterial3 ? Curves.easeOutBack : Curves.easeInOut,
      child: native,
      builder: (context, position, child) => CustomPaint(
        painter: active
            ? _NeumorphicSwitchPainter(
                palette: palette,
                position: position,
                textDirection: Directionality.of(context),
                enabled: onChanged != null,
                material3: theme.useMaterial3,
              )
            : null,
        child: child,
      ),
    );
  }
}

class _NeumorphicSwitchPainter extends CustomPainter {
  const _NeumorphicSwitchPainter({
    required this.palette,
    required this.position,
    required this.textDirection,
    required this.enabled,
    required this.material3,
  });

  final Palette palette;
  final double position;
  final TextDirection textDirection;
  final bool enabled;
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
    final opacity = enabled ? 1.0 : .42;
    final fill = Color.lerp(
      Color.lerp(palette.surface, Colors.black, palette.dark ? .12 : .02)!,
      palette.accent,
      position * .55,
    )!;
    canvas.drawRRect(rrect, Paint()..color = fill.withValues(alpha: opacity));
    NeumorphicSurfacePainter._paintInset(
      canvas,
      rrect,
      Colors.black.withValues(alpha: (palette.dark ? .5 : .23) * opacity),
      Colors.white.withValues(alpha: (palette.dark ? .12 : .8) * opacity),
      2,
      3,
    );
    // Flutter paints the native thumb above this layer; these two casts sit
    // directly behind its actual circular area and leave its icon intact.
    final center = Offset(
      rect.left +
          height / 2 +
          (textDirection == TextDirection.rtl ? 1 - position : position) *
              (width - height),
      rect.center.dy,
    );
    final radius = material3 ? 8 + 4 * position : 10.0;
    for (final (shift, color) in [
      (
        const Offset(-1.5, -1.5),
        Colors.white.withValues(alpha: (palette.dark ? .34 : .78) * opacity),
      ),
      (
        const Offset(1.5, 1.5),
        Colors.black.withValues(alpha: (palette.dark ? .52 : .25) * opacity),
      ),
    ]) {
      canvas.drawCircle(
        center + shift,
        radius,
        Paint()
          ..color = color
          ..style = PaintingStyle.stroke
          ..strokeWidth = 1.5
          ..maskFilter = const MaskFilter.blur(BlurStyle.normal, 3),
      );
    }
  }

  @override
  bool shouldRepaint(covariant _NeumorphicSwitchPainter oldDelegate) =>
      palette != oldDelegate.palette ||
      position != oldDelegate.position ||
      textDirection != oldDelegate.textDirection ||
      enabled != oldDelegate.enabled ||
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
  Widget build(BuildContext context) {
    final platform = Theme.of(context).platform;
    final appleAdaptive =
        adaptive &&
        (platform == TargetPlatform.iOS || platform == TargetPlatform.macOS);
    final active =
        palette.surfaces.visualStyle == VisualStyle.neumorphism &&
        !appleAdaptive &&
        !MediaQuery.highContrastOf(context);
    final shape = active
        ? _NeumorphicCheckboxBorder(
            palette: palette,
            enabled: onChanged != null,
            side: BorderSide(color: palette.line),
            borderRadius: palette.borderRadius(5),
          )
        : null;
    return adaptive
        ? Checkbox.adaptive(
            value: value,
            tristate: tristate,
            onChanged: onChanged,
            focusNode: focusNode,
            autofocus: autofocus,
            semanticLabel: semanticLabel,
            isError: isError,
            shape: shape,
          )
        : Checkbox(
            value: value,
            tristate: tristate,
            onChanged: onChanged,
            focusNode: focusNode,
            autofocus: autofocus,
            semanticLabel: semanticLabel,
            isError: isError,
            shape: shape,
          );
  }
}

class _NeumorphicCheckboxBorder extends RoundedRectangleBorder {
  const _NeumorphicCheckboxBorder({
    required this.palette,
    required this.enabled,
    super.side,
    super.borderRadius,
  });

  final Palette palette;
  final bool enabled;

  @override
  _NeumorphicCheckboxBorder copyWith({
    BorderSide? side,
    BorderRadiusGeometry? borderRadius,
  }) => _NeumorphicCheckboxBorder(
    palette: palette,
    enabled: enabled,
    side: side ?? this.side,
    borderRadius: borderRadius ?? this.borderRadius,
  );

  @override
  bool operator ==(Object other) =>
      other is _NeumorphicCheckboxBorder &&
      super == other &&
      identical(palette, other.palette) &&
      enabled == other.enabled;

  @override
  int get hashCode => Object.hash(super.hashCode, palette, enabled);
  @override
  void paint(Canvas canvas, Rect rect, {TextDirection? textDirection}) {
    super.paint(canvas, rect, textDirection: textDirection);
    NeumorphicSurfacePainter._paintInset(
      canvas,
      borderRadius.resolve(textDirection).toRRect(rect),
      Colors.black.withValues(
        alpha: (palette.dark ? .5 : .24) * (enabled ? 1 : .38),
      ),
      Colors.white.withValues(
        alpha: (palette.dark ? .16 : .8) * (enabled ? 1 : .38),
      ),
      1.5,
      2.5,
    );
  }
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
        palette.surfaces.visualStyle == VisualStyle.neumorphism &&
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
    final opacity = enabled ? 1.0 : .38;
    if (selected) {
      NeumorphicSurfacePainter._paintInset(
        canvas,
        rrect,
        Colors.black.withValues(alpha: (palette.dark ? .48 : .22) * opacity),
        Colors.white.withValues(alpha: (palette.dark ? .14 : .72) * opacity),
        1.5,
        3,
      );
    } else {
      NeumorphicSurfacePainter._paintEdge(
        canvas,
        rrect,
        Colors.white.withValues(alpha: (palette.dark ? .18 : .75) * opacity),
        Colors.black.withValues(alpha: (palette.dark ? .48 : .22) * opacity),
        1,
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
