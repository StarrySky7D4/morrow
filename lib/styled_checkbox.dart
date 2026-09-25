import 'package:flutter/material.dart';

import 'appearance.dart';
import 'neumorphic_controls.dart';

/// A native checkbox with a continuously changing outline and inset relief.
/// The border only paints decoration; Checkbox still owns state and semantics.
class StyledCheckbox extends StatefulWidget {
  const StyledCheckbox({
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
  State<StyledCheckbox> createState() => _StyledCheckboxState();
}

@immutable
class _ThemeInset {
  const _ThemeInset({
    required this.strength,
    required this.shade,
    required this.light,
    required this.shift,
  });

  static const zero = _ThemeInset(
    strength: 0,
    shade: Colors.black,
    light: Colors.white,
    shift: 0,
  );

  factory _ThemeInset.fromPalette(Palette? palette, bool enabled) {
    if (palette == null) return zero;
    final factor = enabled ? 1.0 : .38;
    return _ThemeInset(
      strength: palette.surfaces.styleDepth.clamp(0, 1),
      shade: Colors.black.withValues(alpha: (palette.dark ? .5 : .24) * factor),
      light: Colors.white.withValues(alpha: (palette.dark ? .16 : .8) * factor),
      shift: 1.5 * palette.surfaces.styleDepth,
    );
  }

  final double strength;
  final Color shade;
  final Color light;
  final double shift;

  static _ThemeInset lerp(_ThemeInset a, _ThemeInset b, double t) {
    // A zero-relief endpoint has no visible lighting. Borrow the other end's
    // colors so only the inset's intensity fades at the flat boundary.
    final shadeA = a.strength == 0 ? b.shade : a.shade;
    final shadeB = b.strength == 0 ? a.shade : b.shade;
    final lightA = a.strength == 0 ? b.light : a.light;
    final lightB = b.strength == 0 ? a.light : b.light;
    return _ThemeInset(
      strength: a.strength + (b.strength - a.strength) * t,
      shade: Color.lerp(shadeA, shadeB, t)!,
      light: Color.lerp(lightA, lightB, t)!,
      shift: a.shift + (b.shift - a.shift) * t,
    );
  }

  @override
  bool operator ==(Object other) =>
      other is _ThemeInset &&
      strength == other.strength &&
      shade == other.shade &&
      light == other.light &&
      shift == other.shift;

  @override
  int get hashCode => Object.hash(strength, shade, light, shift);
}

/// Marks a shape installed by Morrow's style themes. This lets the animated
/// widget distinguish it from an application-supplied CheckboxTheme shape.
/// Ordinary native checkboxes interpolate this shape's inset when ThemeData
/// animates between visual styles.
@immutable
class StyledCheckboxThemeShape extends RoundedRectangleBorder {
  const StyledCheckboxThemeShape({
    super.side,
    super.borderRadius,
    this.insetPalette,
    this.enabled = true,
  }) : _interpolatedInset = null;

  const StyledCheckboxThemeShape._frame({
    required super.side,
    required super.borderRadius,
    required _ThemeInset inset,
  }) : insetPalette = null,
       enabled = true,
       _interpolatedInset = inset;

  final Palette? insetPalette;
  final bool enabled;
  final _ThemeInset? _interpolatedInset;

  _ThemeInset get _inset =>
      _interpolatedInset ?? _ThemeInset.fromPalette(insetPalette, enabled);

  double get insetStrength => _inset.strength;
  Color get insetShade => _inset.shade;
  Color get insetLight => _inset.light;
  double get insetShift => _inset.shift;

  @override
  StyledCheckboxThemeShape copyWith({
    BorderSide? side,
    BorderRadiusGeometry? borderRadius,
  }) => _interpolatedInset == null
      ? StyledCheckboxThemeShape(
          side: side ?? this.side,
          borderRadius: borderRadius ?? this.borderRadius,
          insetPalette: insetPalette,
          enabled: enabled,
        )
      : StyledCheckboxThemeShape._frame(
          side: side ?? this.side,
          borderRadius: borderRadius ?? this.borderRadius,
          inset: _interpolatedInset,
        );

  @override
  StyledCheckboxThemeShape scale(double t) => StyledCheckboxThemeShape._frame(
    side: side.scale(t),
    borderRadius: borderRadius * t,
    inset: _ThemeInset.lerp(_ThemeInset.zero, _inset, t),
  );

  @override
  ShapeBorder? lerpFrom(ShapeBorder? a, double t) {
    if (a == null || a is StyledCheckboxThemeShape) {
      return StyledCheckboxThemeShape._frame(
        side: BorderSide.lerp(
          a is StyledCheckboxThemeShape ? a.side : BorderSide.none,
          side,
          t,
        ),
        borderRadius: BorderRadiusGeometry.lerp(
          a is StyledCheckboxThemeShape
              ? a.borderRadius
              : const BorderRadius.all(Radius.circular(2)),
          borderRadius,
          t,
        )!,
        inset: _ThemeInset.lerp(
          a is StyledCheckboxThemeShape ? a._inset : _ThemeInset.zero,
          _inset,
          t,
        ),
      );
    }
    return super.lerpFrom(a, t);
  }

  @override
  ShapeBorder? lerpTo(ShapeBorder? b, double t) {
    if (b == null) {
      if (t >= 1) return null;
      return StyledCheckboxThemeShape._frame(
        side: BorderSide.lerp(side, BorderSide.none, t),
        borderRadius: BorderRadiusGeometry.lerp(
          borderRadius,
          const BorderRadius.all(Radius.circular(2)),
          t,
        )!,
        inset: _ThemeInset.lerp(_inset, _ThemeInset.zero, t),
      );
    }
    return super.lerpTo(b, t);
  }

  @override
  void paint(Canvas canvas, Rect rect, {TextDirection? textDirection}) {
    super.paint(canvas, rect, textDirection: textDirection);
    final inset = _inset;
    if (inset.strength <= 0) return;
    NeumorphicSurfacePainter.paintInset(
      canvas,
      borderRadius.resolve(textDirection).toRRect(rect),
      inset.shade.withValues(alpha: inset.shade.a * inset.strength),
      inset.light.withValues(alpha: inset.light.a * inset.strength),
      inset.shift,
      2.5,
    );
  }

  @override
  bool operator ==(Object other) =>
      other is StyledCheckboxThemeShape &&
      super == other &&
      identical(insetPalette, other.insetPalette) &&
      enabled == other.enabled &&
      _interpolatedInset == other._interpolatedInset;

  @override
  int get hashCode =>
      Object.hash(super.hashCode, insetPalette, enabled, _interpolatedInset);
}

/// A shape with an inset overlay. [outline] supplies the native checkbox path;
/// the native painter supplies its state-dependent side and fill colors.
@immutable
class StyledCheckboxBorder extends OutlinedBorder {
  const StyledCheckboxBorder({
    required this.outline,
    required this.recess,
    required this.shade,
    required this.light,
    required this.shift,
    super.side = BorderSide.none,
  });

  final OutlinedBorder outline;
  final double recess;
  final Color shade;
  final Color light;
  final double shift;

  @override
  EdgeInsetsGeometry get dimensions => outline.copyWith(side: side).dimensions;

  @override
  bool get preferPaintInterior => outline.preferPaintInterior;

  @override
  void paintInterior(
    Canvas canvas,
    Rect rect,
    Paint paint, {
    TextDirection? textDirection,
  }) {
    outline.paintInterior(canvas, rect, paint, textDirection: textDirection);
  }

  @override
  Path getInnerPath(Rect rect, {TextDirection? textDirection}) =>
      outline.getInnerPath(rect, textDirection: textDirection);

  @override
  Path getOuterPath(Rect rect, {TextDirection? textDirection}) =>
      outline.getOuterPath(rect, textDirection: textDirection);

  @override
  StyledCheckboxBorder copyWith({BorderSide? side}) => StyledCheckboxBorder(
    outline: outline,
    recess: recess,
    shade: shade,
    light: light,
    shift: shift,
    side: side ?? this.side,
  );

  @override
  StyledCheckboxBorder scale(double t) => StyledCheckboxBorder(
    outline: outline.scale(t) as OutlinedBorder,
    recess: recess,
    shade: shade,
    light: light,
    shift: shift * t,
    side: side.scale(t),
  );

  @override
  void paint(Canvas canvas, Rect rect, {TextDirection? textDirection}) {
    outline
        .copyWith(side: side)
        .paint(canvas, rect, textDirection: textDirection);
    if (recess <= 0) return;
    // The overlay is clipped to the same outline as the native checkbox.
    final rrect = outline is RoundedRectangleBorder
        ? (outline as RoundedRectangleBorder).borderRadius
              .resolve(textDirection)
              .toRRect(rect)
        : RRect.fromRectAndRadius(rect, Radius.zero);
    canvas.save();
    canvas.clipPath(outline.getOuterPath(rect, textDirection: textDirection));
    NeumorphicSurfacePainter.paintInset(
      canvas,
      rrect,
      shade.withValues(alpha: shade.a * recess),
      light.withValues(alpha: light.a * recess),
      shift,
      2.5,
    );
    canvas.restore();
  }

  @override
  bool operator ==(Object other) =>
      other is StyledCheckboxBorder &&
      outline == other.outline &&
      recess == other.recess &&
      shade == other.shade &&
      light == other.light &&
      shift == other.shift &&
      side == other.side;

  @override
  int get hashCode => Object.hash(outline, recess, shade, light, shift, side);
}

@immutable
class _CheckboxFrame {
  const _CheckboxFrame({
    required this.outline,
    required this.recess,
    required this.shade,
    required this.light,
    required this.shift,
  });

  final OutlinedBorder outline;
  final double recess;
  final Color shade;
  final Color light;
  final double shift;

  static _CheckboxFrame lerp(_CheckboxFrame a, _CheckboxFrame b, double t) {
    final shape = ShapeBorder.lerp(a.outline, b.outline, t);
    return _CheckboxFrame(
      outline: shape is OutlinedBorder
          ? shape
          : (t < .5 ? a.outline : b.outline),
      recess: a.recess + (b.recess - a.recess) * t,
      shade: Color.lerp(a.shade, b.shade, t)!,
      light: Color.lerp(a.light, b.light, t)!,
      shift: a.shift + (b.shift - a.shift) * t,
    );
  }

  @override
  bool operator ==(Object other) =>
      other is _CheckboxFrame &&
      outline == other.outline &&
      recess == other.recess &&
      shade == other.shade &&
      light == other.light &&
      shift == other.shift;

  @override
  int get hashCode => Object.hash(outline, recess, shade, light, shift);
}

class _StyledCheckboxState extends State<StyledCheckbox>
    with SingleTickerProviderStateMixin {
  late final AnimationController _controller = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 180),
    value: 1,
  );
  _CheckboxFrame? _from;
  _CheckboxFrame? _to;
  bool _apple = false;
  bool _highContrast = false;
  bool _motion = true;
  OutlinedBorder? _customShape;

  _CheckboxFrame get _displayed => _controller.value == 1
      ? _to!
      : _CheckboxFrame.lerp(
          _from!,
          _to!,
          Curves.easeOutCubic.transform(_controller.value),
        );

  OutlinedBorder _defaultShape(ThemeData theme) => RoundedRectangleBorder(
    borderRadius: BorderRadius.circular(theme.useMaterial3 ? 2 : 1),
  );

  // Generated theme shapes carry a marker through ThemeData.lerp. Its
  // intermediate radius is deliberately ignored: the palette is already at
  // the requested style target and drives this animation just once.
  void _readShape(ThemeData theme) {
    final supplied = theme.checkboxTheme.shape;
    _customShape = supplied == null || supplied is StyledCheckboxThemeShape
        ? null
        : supplied;
  }

  _CheckboxFrame _target(ThemeData theme) {
    final palette = widget.palette;
    final isNeumorphic = palette.visualStyle == VisualStyle.neumorphism;
    final outline =
        _customShape ??
        (palette.visualStyle == VisualStyle.flat
            ? _defaultShape(theme)
            : RoundedRectangleBorder(borderRadius: palette.borderRadius(5)));
    final factor = widget.onChanged == null ? .38 : 1.0;
    return _CheckboxFrame(
      outline: outline,
      recess: isNeumorphic && !_apple && !_highContrast
          ? palette.surfaces.styleDepth.clamp(0, 1)
          : 0,
      shade: Colors.black.withValues(alpha: (palette.dark ? .5 : .24) * factor),
      light: Colors.white.withValues(alpha: (palette.dark ? .16 : .8) * factor),
      shift: 1.5 * palette.surfaces.styleDepth.clamp(0, 2),
    );
  }

  void _retarget() {
    final theme = Theme.of(context);
    _apple =
        widget.adaptive &&
        (theme.platform == TargetPlatform.iOS ||
            theme.platform == TargetPlatform.macOS);
    _highContrast = MediaQuery.highContrastOf(context);
    _motion =
        !MediaQuery.disableAnimationsOf(context) &&
        TickerMode.valuesOf(context).enabled;
    _readShape(theme);
    final next = _target(theme);
    if (_to == null) {
      _from = _to = next;
    } else if (_to != next) {
      _from = _displayed;
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

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    _retarget();
  }

  @override
  void didUpdateWidget(StyledCheckbox oldWidget) {
    super.didUpdateWidget(oldWidget);
    _retarget();
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
      final frame = _displayed;
      final shape = _apple
          ? null
          : StyledCheckboxBorder(
              outline: frame.outline,
              recess: frame.recess,
              shade: frame.shade,
              light: frame.light,
              shift: frame.shift,
            );
      return widget.adaptive
          ? Checkbox.adaptive(
              value: widget.value,
              tristate: widget.tristate,
              onChanged: widget.onChanged,
              focusNode: widget.focusNode,
              autofocus: widget.autofocus,
              semanticLabel: widget.semanticLabel,
              isError: widget.isError,
              shape: shape,
            )
          : Checkbox(
              value: widget.value,
              tristate: widget.tristate,
              onChanged: widget.onChanged,
              focusNode: widget.focusNode,
              autofocus: widget.autofocus,
              semanticLabel: widget.semanticLabel,
              isError: widget.isError,
              shape: shape,
            );
    },
  );
}
