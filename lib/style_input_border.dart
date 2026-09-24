import 'package:flutter/material.dart';

import 'appearance.dart';
import 'experimental_controls.dart';
import 'neumorphic_controls.dart';

/// Fixed-size visual state; an interrupted tween never retains earlier borders.
@immutable
class InputRelief {
  const InputRelief({
    this.neumorphic = 0,
    this.depth = 1,
    this.clay = 0,
    this.industrial = 0,
    this.darkness = 0,
    this.enabled = true,
  });

  final double depth;
  final double neumorphic, clay, industrial, darkness;
  final bool enabled;

  static InputRelief lerp(InputRelief a, InputRelief b, double t) =>
      InputRelief(
        depth: a.depth + (b.depth - a.depth) * t,
        neumorphic: a.neumorphic + (b.neumorphic - a.neumorphic) * t,
        clay: a.clay + (b.clay - a.clay) * t,
        industrial: a.industrial + (b.industrial - a.industrial) * t,
        darkness: a.darkness + (b.darkness - a.darkness) * t,
        enabled: t < .5 ? a.enabled : b.enabled,
      );

  @override
  bool operator ==(Object other) =>
      other is InputRelief &&
      depth == other.depth &&
      neumorphic == other.neumorphic &&
      clay == other.clay &&
      industrial == other.industrial &&
      darkness == other.darkness &&
      enabled == other.enabled;

  @override
  int get hashCode =>
      Object.hash(depth, neumorphic, clay, industrial, darkness, enabled);
}

/// Used by the flat theme too: OutlineInputBorder.lerpFrom otherwise strips
/// custom relief before the source border gets a chance to interpolate it.
class StyledInputBorder extends OutlineInputBorder {
  const StyledInputBorder({
    this.relief = const InputRelief(),
    super.borderSide,
    super.borderRadius,
    super.gapPadding,
  });

  final InputRelief relief;

  @override
  StyledInputBorder copyWith({
    BorderSide? borderSide,
    BorderRadius? borderRadius,
    double? gapPadding,
  }) => StyledInputBorder(
    relief: relief,
    borderSide: borderSide ?? this.borderSide,
    borderRadius: borderRadius ?? this.borderRadius,
    gapPadding: gapPadding ?? this.gapPadding,
  );

  @override
  StyledInputBorder scale(double t) => StyledInputBorder(
    relief: relief,
    borderSide: borderSide.scale(t),
    borderRadius: borderRadius * t,
    gapPadding: gapPadding * t,
  );

  @override
  ShapeBorder? lerpFrom(ShapeBorder? a, double t) {
    if (a is StyledInputBorder) {
      if (t <= 0) return a;
      if (t >= 1) return this;
      return StyledInputBorder(
        relief: InputRelief.lerp(a.relief, relief, t),
        borderSide: BorderSide.lerp(a.borderSide, borderSide, t),
        borderRadius: BorderRadius.lerp(a.borderRadius, borderRadius, t)!,
        gapPadding: a.gapPadding + (gapPadding - a.gapPadding) * t,
      );
    }
    return super.lerpFrom(a, t);
  }

  @override
  ShapeBorder? lerpTo(ShapeBorder? b, double t) =>
      b is StyledInputBorder ? b.lerpFrom(this, t) : super.lerpTo(b, t);

  @override
  bool operator ==(Object other) =>
      other is StyledInputBorder && super == other && relief == other.relief;

  @override
  int get hashCode => Object.hash(super.hashCode, relief);

  @override
  void paint(
    Canvas canvas,
    Rect rect, {
    double? gapStart,
    double gapExtent = 0,
    double gapPercentage = 0,
    TextDirection? textDirection,
  }) {
    final state = relief;
    if (state.neumorphic > 0 || state.clay > 0 || state.industrial > 0) {
      canvas.save();
      if (gapStart != null && gapExtent > 0 && gapPercentage > 0) {
        final width = (gapExtent + 2 * gapPadding) * gapPercentage;
        final start = textDirection == TextDirection.rtl
            ? rect.left + gapStart + gapPadding - width
            : rect.left + gapStart - gapPadding;
        final outer = Path()..addRect(rect.inflate(8));
        final gap = Path()
          ..addRect(Rect.fromLTWH(start, rect.top - 6, width, 12));
        canvas.clipPath(Path.combine(PathOperation.difference, outer, gap));
      }
      if (state.neumorphic > 0) {
        NeumorphicSurfacePainter.paintInset(
          canvas,
          borderRadius.toRRect(rect),
          Colors.black.withValues(
            alpha:
                (.20 + .28 * state.darkness) *
                state.neumorphic *
                state.depth.clamp(0, 1),
          ),
          Colors.white.withValues(
            alpha:
                (.75 - .61 * state.darkness) *
                state.neumorphic *
                state.depth.clamp(0, 1),
          ),
          2 * state.depth,
          4,
        );
      }
      canvas.translate(rect.left, rect.top);
      for (final (style, weight) in [
        (VisualStyle.clay, state.clay),
        (VisualStyle.industrial, state.industrial),
      ]) {
        if (weight <= 0) continue;
        ExperimentalSurfacePainter(
          style: style,
          opacity: weight,
          depth: -state.depth,
          radius: borderRadius,
          surface: Colors.grey,
          dark: state.darkness >= .5,
          darkMix: state.darkness,
          enabled: state.enabled,
          highContrast: false,
          fill: false,
          focused: false,
          focusColor: Colors.transparent,
        ).paint(canvas, rect.size);
      }
      canvas.restore();
    }
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
