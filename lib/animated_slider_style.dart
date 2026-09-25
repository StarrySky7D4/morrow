import 'dart:math' as math;
import 'package:flutter/material.dart';
import 'appearance.dart';
import 'neumorphic_controls.dart';

/// Animates decoration only. The same Material Slider owns all input and state.
class AnimatedSliderStyle extends StatefulWidget {
  const AnimatedSliderStyle({
    super.key,
    required this.palette,
    required this.child,
    this.flatThumbRadius = 10,
    this.flatTrackHeight = 4,
    this.neumorphicTrackHeight = 7,
    this.overlayRadius,
  });
  final Palette palette;
  final Widget child;
  final double flatThumbRadius, flatTrackHeight, neumorphicTrackHeight;
  final double? overlayRadius;
  @override
  State<AnimatedSliderStyle> createState() => _AnimatedSliderStyleState();
}

class _AnimatedSliderStyleState extends State<AnimatedSliderStyle>
    with SingleTickerProviderStateMixin {
  late final _animation = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 180),
    value: 1,
  );
  late SliderStyleFrame _to = target(), _from = _to;
  bool _motion = true;
  SliderStyleFrame target() => SliderStyleFrame.target(
    widget.palette,
    flatThumbRadius: widget.flatThumbRadius,
    flatTrackHeight: widget.flatTrackHeight,
    neumorphicTrackHeight: widget.neumorphicTrackHeight,
  );
  SliderStyleFrame get displayed => _animation.value == 1
      ? _to
      : SliderStyleFrame.lerp(
          _from,
          _to,
          Curves.easeOutCubic.transform(_animation.value),
        );
  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    _motion =
        !MediaQuery.disableAnimationsOf(context) &&
        TickerMode.valuesOf(context).enabled;
    if (!_motion) {
      _animation.stop();
      _from = _to;
      _animation.value = 1;
    }
  }

  @override
  void didUpdateWidget(AnimatedSliderStyle oldWidget) {
    super.didUpdateWidget(oldWidget);
    final next = target();
    if (_to.sameAs(next)) return;
    _from = displayed;
    _to = next;
    if (_motion) {
      _animation.forward(from: 0);
    } else {
      _animation.stop();
      _from = _to;
      _animation.value = 1;
    }
  }

  @override
  void dispose() {
    _animation.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => AnimatedBuilder(
    animation: _animation,
    child: widget.child,
    builder: (context, child) {
      final frame = displayed;
      return SliderTheme(
        data: SliderTheme.of(context).copyWith(
          trackHeight: frame.trackHeight,
          thumbShape: MorphingSliderThumb(frame),
          trackShape: MorphingSliderTrack(frame),
          overlayShape: widget.overlayRadius == null
              ? null
              : RoundSliderOverlayShape(overlayRadius: widget.overlayRadius!),
        ),
        child: child!,
      );
    },
  );
}

/// Fixed-size geometry and light state; no prior widgets, painters or tweens.
@immutable
class SliderStyleFrame {
  SliderStyleFrame._(
    List<double> values,
    this.surface,
    this.line,
    this.ink,
    this.accent,
  ) : values = List.unmodifiable(values);
  factory SliderStyleFrame.target(
    Palette p, {
    double flatThumbRadius = 10,
    double flatTrackHeight = 4,
    double neumorphicTrackHeight = 7,
  }) {
    final style = p.visualStyle;
    final flat = style == VisualStyle.flat;
    final neumo = style == VisualStyle.neumorphism;
    final clay = style == VisualStyle.clay;
    final hard =
        style == VisualStyle.brutalist || style == VisualStyle.industrial;
    final size = flat
        ? flatThumbRadius * 2
        : clay
        ? 22.0
        : 20.0;
    return SliderStyleFrame._(
      [
        size,
        flat
            ? flatThumbRadius
            : hard
            ? 1
            : style == VisualStyle.paper
            ? 3
            : size / 2,
        flat
            ? 0
            : style == VisualStyle.brutalist
            ? 2
            : 1,
        neumo ? .6 : .7,
        flat
            ? flatTrackHeight
            : neumo
            ? neumorphicTrackHeight
            : hard
            ? (style == VisualStyle.brutalist ? 6 : 7)
            : clay
            ? 7
            : 5,
        hard ? 0 : 1,
        neumo || hard ? 0 : 2,
        flat ? 1 : 0,
        neumo ? 1 : 0,
        clay ? 1 : 0,
        style == VisualStyle.brutalist ? 1 : 0,
        style == VisualStyle.industrial ? 1 : 0,
        p.dark ? 1 : 0,
        p.surfaces.styleDepth,
      ],
      p.surface,
      flat
          ? Colors.transparent
          : style == VisualStyle.brutalist
          ? p.ink
          : p.line,
      p.ink,
      p.accent,
    );
  }
  final List<double> values;
  final Color surface, line, ink, accent;
  double get size => values[0];
  double get radius => values[1];
  double get strokeWidth => values[2];
  double get strokeInset => values[3];
  double get trackHeight => values[4];
  double get roundness => values[5];
  double get activeGrowth => values[6];
  double get flat => values[7];
  double get neumo => values[8];
  double get clay => values[9];
  double get brutalist => values[10];
  double get industrial => values[11];
  double get darkness => values[12];
  double get depth => values[13];
  bool sameAs(SliderStyleFrame other) =>
      surface == other.surface &&
      line == other.line &&
      ink == other.ink &&
      accent == other.accent &&
      List.generate(
        values.length,
        (i) => values[i] == other.values[i],
      ).every((v) => v);
  static SliderStyleFrame lerp(
    SliderStyleFrame a,
    SliderStyleFrame b,
    double t,
  ) => SliderStyleFrame._(
    List.generate(
      a.values.length,
      (i) => a.values[i] + (b.values[i] - a.values[i]) * t,
    ),
    Color.lerp(a.surface, b.surface, t)!,
    Color.lerp(a.line, b.line, t)!,
    Color.lerp(a.ink, b.ink, t)!,
    Color.lerp(a.accent, b.accent, t)!,
  );
}

class MorphingSliderThumb extends SliderComponentShape {
  const MorphingSliderThumb(this.frame);
  final SliderStyleFrame frame;
  @override
  Size getPreferredSize(bool isEnabled, bool isDiscrete) =>
      Size.square(frame.size);
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
    final progress = enableAnimation.value.clamp(0.0, 1.0);
    final alpha = .45 + .55 * progress;
    final rect = Rect.fromCenter(
      center: center,
      width: frame.size,
      height: frame.size,
    );
    final shape = RRect.fromRectAndRadius(rect, Radius.circular(frame.radius));
    if (frame.flat > 0 && !debugDisableShadows) {
      final path = Path()..addArc(rect, 0, math.pi * 2);
      canvas.drawShadow(
        path,
        Colors.black.withValues(alpha: frame.flat),
        1 + 5 * activationAnimation.value,
        true,
      );
    }
    void shadow(Offset offset, Color color, double blur) {
      if (color.a <= 0) return;
      canvas.drawRRect(
        shape.shift(offset * frame.depth),
        Paint()
          ..color = color.withValues(alpha: color.a * frame.depth.clamp(0, 1))
          ..maskFilter = blur == 0
              ? null
              : MaskFilter.blur(BlurStyle.normal, blur),
      );
    }

    shadow(
      const Offset(-1.5, -1.5),
      Colors.white.withValues(
        alpha: (.8 - .64 * frame.darkness) * alpha * frame.neumo,
      ),
      3,
    );
    shadow(
      const Offset(1.5, 1.5),
      Colors.black.withValues(
        alpha: (.25 + .30 * frame.darkness) * alpha * frame.neumo,
      ),
      3,
    );
    shadow(
      const Offset(1, 2),
      Colors.black.withValues(alpha: .22 * alpha * frame.clay),
      4,
    );
    shadow(
      const Offset(2, 2),
      frame.ink.withValues(alpha: frame.ink.a * alpha * frame.brutalist),
      0,
    );
    final color = Color.lerp(
      sliderTheme.disabledThumbColor ??
          frame.surface.withValues(alpha: frame.surface.a * .45),
      sliderTheme.thumbColor ?? frame.surface,
      progress,
    )!;
    canvas.drawRRect(shape, Paint()..color = color);
    if (frame.strokeWidth > 0 && frame.line.a > 0) {
      canvas.drawRRect(
        shape.deflate(frame.strokeInset),
        Paint()
          ..color = frame.line.withValues(alpha: frame.line.a * alpha)
          ..style = PaintingStyle.stroke
          ..strokeWidth = frame.strokeWidth,
      );
    }
    if (frame.industrial > 0) {
      canvas.drawLine(
        center + const Offset(0, -5),
        center + const Offset(0, 5),
        Paint()
          ..color = frame.accent.withValues(
            alpha: frame.accent.a * alpha * frame.industrial,
          )
          ..strokeWidth = 2,
      );
    }
  }
}

class MorphingSliderTrack extends SliderTrackShape with BaseSliderTrackShape {
  const MorphingSliderTrack(this.frame);
  final SliderStyleFrame frame;
  // Material's boolean rounded flag changes discrete thumb padding abruptly.
  // Move that padding into the continuous preferred rect; paint the full track.
  @override
  bool get isRounded => false;

  @override
  Rect getPreferredRect({
    required RenderBox parentBox,
    Offset offset = Offset.zero,
    required SliderThemeData sliderTheme,
    bool isEnabled = false,
    bool isDiscrete = false,
  }) {
    final rect = super.getPreferredRect(
      parentBox: parentBox,
      offset: offset,
      sliderTheme: sliderTheme,
      isEnabled: isEnabled,
      isDiscrete: isDiscrete,
    );
    final inset = isDiscrete
        ? math.min(rect.height * frame.roundness / 2, rect.width / 2)
        : 0.0;
    return Rect.fromLTRB(
      rect.left + inset,
      rect.top,
      rect.right - inset,
      rect.bottom,
    );
  }

  @override
  void paint(
    PaintingContext context,
    Offset offset, {
    required RenderBox parentBox,
    required SliderThemeData sliderTheme,
    required Animation<double> enableAnimation,
    required TextDirection textDirection,
    required Offset thumbCenter,
    Offset? secondaryOffset,
    bool isDiscrete = false,
    bool isEnabled = false,
  }) {
    if ((sliderTheme.trackHeight ?? 0) <= 0) return;
    final rect = super.getPreferredRect(
      parentBox: parentBox,
      offset: offset,
      sliderTheme: sliderTheme,
      isEnabled: isEnabled,
      isDiscrete: isDiscrete,
    );
    if (rect.isEmpty) return;
    final t = enableAnimation.value.clamp(0.0, 1.0);
    Color color(Color? disabled, Color? enabled, Color fallback) => Color.lerp(
      disabled ?? fallback.withValues(alpha: fallback.a * .45),
      enabled ?? fallback,
      t,
    )!;
    final active = color(
      sliderTheme.disabledActiveTrackColor,
      sliderTheme.activeTrackColor,
      frame.accent,
    );
    final inactive = color(
      sliderTheme.disabledInactiveTrackColor,
      sliderTheme.inactiveTrackColor,
      frame.surface,
    );
    final canvas = context.canvas;
    final isLTR = textDirection == TextDirection.ltr;
    final radius = Radius.circular(rect.height / 2 * frame.roundness);
    final activeRadius = Radius.circular(
      (rect.height + frame.activeGrowth) / 2 * frame.roundness,
    );
    final overlap = rect.height / 2 * frame.roundness * (1 - frame.neumo);
    if (frame.neumo > 0) {
      canvas.save();
      canvas.clipRect(
        isLTR
            ? Rect.fromLTRB(rect.left, rect.top, thumbCenter.dx, rect.bottom)
            : Rect.fromLTRB(thumbCenter.dx, rect.top, rect.right, rect.bottom),
      );
      canvas.drawRRect(
        RRect.fromRectAndRadius(rect, radius),
        Paint()..color = inactive.withValues(alpha: inactive.a * frame.neumo),
      );
      canvas.restore();
    }
    void segment(bool left, bool enabledSegment, Color paintColor) {
      final bounds = Rect.fromLTRB(
        left ? rect.left : thumbCenter.dx - overlap,
        rect.top - (enabledSegment ? frame.activeGrowth / 2 : 0),
        left ? thumbCenter.dx + overlap : rect.right,
        rect.bottom + (enabledSegment ? frame.activeGrowth / 2 : 0),
      );
      final outside = enabledSegment ? activeRadius : radius;
      final inside = Radius.circular(outside.x * (1 - frame.neumo));
      canvas.drawRRect(
        RRect.fromRectAndCorners(
          bounds,
          topLeft: left ? outside : inside,
          bottomLeft: left ? outside : inside,
          topRight: left ? inside : outside,
          bottomRight: left ? inside : outside,
        ),
        Paint()..color = paintColor,
      );
    }

    if (thumbCenter.dx < rect.right - overlap) {
      segment(false, !isLTR, isLTR ? inactive : active);
    }
    if (thumbCenter.dx > rect.left + overlap) {
      segment(true, isLTR, isLTR ? active : inactive);
    }
    final secondary = secondaryOffset?.dx.clamp(rect.left, rect.right);
    if (secondary != null &&
        (isLTR ? secondary > thumbCenter.dx : secondary < thumbCenter.dx)) {
      final buffer = isLTR
          ? Rect.fromLTRB(thumbCenter.dx, rect.top, secondary, rect.bottom)
          : Rect.fromLTRB(secondary, rect.top, thumbCenter.dx, rect.bottom);
      canvas.drawRRect(
        RRect.fromRectAndCorners(
          buffer,
          topLeft: isLTR ? Radius.zero : radius,
          bottomLeft: isLTR ? Radius.zero : radius,
          topRight: isLTR ? radius : Radius.zero,
          bottomRight: isLTR ? radius : Radius.zero,
        ),
        Paint()
          ..color = color(
            sliderTheme.disabledSecondaryActiveTrackColor,
            sliderTheme.secondaryActiveTrackColor,
            frame.accent.withValues(alpha: .36),
          ),
      );
    }
    if (frame.neumo > 0) {
      final strength = frame.neumo * (.45 + .55 * t) * frame.depth.clamp(0, 1);
      NeumorphicSurfacePainter.paintInset(
        canvas,
        RRect.fromRectAndRadius(rect, radius),
        Colors.black.withValues(alpha: (.24 + .24 * frame.darkness) * strength),
        Colors.white.withValues(alpha: (.72 - .60 * frame.darkness) * strength),
        1.5 * frame.depth,
        2.5,
      );
    }
  }
}
