import 'package:flutter/material.dart';

/// Morphs decorative glyphs inside the one native Switch. Slots stay non-null,
/// so Material thumb geometry and drag tracking do not change with style.
class SwitchIconTransition extends StatefulWidget {
  const SwitchIconTransition({
    super.key,
    required this.icons,
    required this.animate,
    required this.fallbackColor,
    required this.builder,
  });
  final WidgetStateProperty<Icon?> icons;
  final bool animate;
  final Color fallbackColor;
  final Widget Function(BuildContext, WidgetStateProperty<Icon?>) builder;
  @override
  State<SwitchIconTransition> createState() => _SwitchIconTransitionState();
}

const _states = [
  WidgetState.selected,
  WidgetState.disabled,
  WidgetState.focused,
  WidgetState.hovered,
  WidgetState.pressed,
];

class _SwitchIconTransitionState extends State<SwitchIconTransition>
    with SingleTickerProviderStateMixin {
  late final _controller = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 180),
    value: 1,
  );
  late List<_Glyph> _to = target();
  late List<_Glyph> _from = _to;
  List<_Glyph> target() => List.generate(1 << _states.length, (mask) {
    final states = <WidgetState>{
      for (var i = 0; i < _states.length; i++)
        if ((mask & (1 << i)) != 0) _states[i],
    };
    return _Glyph.of(widget.icons.resolve(states), widget.fallbackColor);
  });
  List<_Glyph> get displayed => List.generate(
    _to.length,
    (i) => _Glyph.lerp(_from[i], _to[i], _controller.value),
  );
  @override
  void didUpdateWidget(SwitchIconTransition oldWidget) {
    super.didUpdateWidget(oldWidget);
    final next = target();
    if (!List.generate(
      next.length,
      (i) => next[i].sameAs(_to[i]),
    ).every((same) => same)) {
      _from = displayed;
      _to = next;
      _controller.forward(from: 0);
    }
    if (!widget.animate) {
      _controller.stop();
      _from = _to;
      _controller.value = 1;
    }
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
      final frame = displayed;
      return widget.builder(
        context,
        WidgetStateProperty.resolveWith((states) {
          var mask = 0;
          for (var i = 0; i < _states.length; i++) {
            if (states.contains(_states[i])) mask |= 1 << i;
          }
          return frame[mask].icon;
        }),
      );
    },
  );
}

class _Glyph {
  const _Glyph(this.source, this.color, this.size, this.opacity);
  factory _Glyph.of(Icon? icon, Color fallback) => _Glyph(
    icon ?? const Icon(null),
    icon?.color ?? fallback,
    icon?.size ?? 16,
    icon?.icon == null ? 0 : 1,
  );
  final Icon source;
  final Color color;
  final double size, opacity;
  bool sameAs(_Glyph b) =>
      source.icon == b.source.icon &&
      color == b.color &&
      size == b.size &&
      opacity == b.opacity &&
      source.fill == b.source.fill &&
      source.weight == b.source.weight &&
      source.grade == b.source.grade &&
      source.opticalSize == b.source.opticalSize &&
      source.shadows == b.source.shadows;
  static _Glyph lerp(_Glyph a, _Glyph b, double t) {
    if (a.source.icon == b.source.icon) {
      return _Glyph(
        b.source,
        Color.lerp(a.color, b.color, t)!,
        a.size + (b.size - a.size) * t,
        a.opacity + (b.opacity - a.opacity) * t,
      );
    }
    // Swap the glyph only while invisible; interruptions retain the displayed
    // glyph and alpha, not a chain of outgoing widgets or native switches.
    if (a.opacity == 0) return _Glyph(b.source, b.color, b.size, b.opacity * t);
    if (b.opacity == 0) {
      return _Glyph(a.source, a.color, a.size, a.opacity * (1 - t));
    }
    return t < .5
        ? _Glyph(a.source, a.color, a.size, a.opacity * (1 - 2 * t))
        : _Glyph(b.source, b.color, b.size, b.opacity * (2 * t - 1));
  }

  Icon get icon => Icon(
    source.icon,
    size: size,
    color: color.withValues(alpha: color.a * opacity),
    fill: source.fill,
    weight: source.weight,
    grade: source.grade,
    opticalSize: source.opticalSize,
    shadows: opacity == 1 ? source.shadows : null,
  );
}
