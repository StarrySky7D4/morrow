import 'liquid_glass.dart';
import 'package:flutter/gestures.dart';
import 'package:material_ui/material_ui.dart';
import 'ui/workbench_motion.dart';

Duration _motionDuration(BuildContext context, int milliseconds) =>
    MediaQuery.disableAnimationsOf(context) ||
        !TickerMode.valuesOf(context).enabled
    ? Duration.zero
    : Duration(milliseconds: milliseconds);

/// Adds a small hover lift and press response without owning the child's tap.
class SurfaceInteraction extends StatefulWidget {
  const SurfaceInteraction({
    super.key,
    required this.child,
    this.enabled = true,
    this.borderRadius = const BorderRadius.all(Radius.circular(20)),
  });

  final Widget child;
  final bool enabled;
  final BorderRadius borderRadius;

  @override
  State<SurfaceInteraction> createState() => _SurfaceInteractionState();
}

class _SurfaceInteractionState extends State<SurfaceInteraction> {
  bool _hovered = false;
  int? _pointer;
  Offset? _downPosition;

  void _clearInteraction() {
    _hovered = false;
    _pointer = null;
    _downPosition = null;
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    if (_motionDuration(context, 1) == Duration.zero) {
      _clearInteraction();
    }
  }

  @override
  void didUpdateWidget(SurfaceInteraction oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (!widget.enabled) _clearInteraction();
  }

  void _release(int pointer) {
    if (_pointer != pointer) return;
    setState(() {
      _pointer = null;
      _downPosition = null;
    });
  }

  @override
  Widget build(BuildContext context) {
    final motion = _motionDuration(context, 150);
    final active = widget.enabled && motion > Duration.zero;
    final pressed = active && _pointer != null;
    final hovered = active && _hovered && !pressed;
    // Hover must not grow with the card width and consume adjacent gutters.
    final scale = pressed ? 0.984 : 1.0;
    final hoverTarget = hovered ? 1.0 : 0.0;
    return MouseRegion(
      onEnter: (_) {
        if (widget.enabled) setState(() => _hovered = true);
      },
      onExit: (_) {
        if (_hovered) setState(() => _hovered = false);
      },
      child: Listener(
        onPointerDown: (event) {
          if (!active ||
              _pointer != null ||
              (event.buttons & kPrimaryButton) == 0) {
            return;
          }
          setState(() {
            _pointer = event.pointer;
            _downPosition = event.position;
          });
        },
        onPointerMove: (event) {
          if (_pointer != event.pointer || _downPosition == null) return;
          if ((event.position - _downPosition!).distance > 18) {
            _release(event.pointer);
          }
        },
        onPointerUp: (event) => _release(event.pointer),
        onPointerCancel: (event) => _release(event.pointer),
        child: TweenAnimationBuilder<double>(
          tween: Tween(end: hoverTarget),
          duration: motion,
          curve: WorkbenchCurves.snappy,
          child: AnimatedScale(
            scale: scale,
            duration: motion,
            curve: WorkbenchCurves.snappy,
            child: widget.child,
          ),
          builder: (context, hoverAmount, child) => Transform.translate(
            offset: Offset(0, -2.5 * hoverAmount),
            child: child,
          ),
        ),
      ),
    );
  }
}

/// Moves one persistent surface between a floating and an attached position.
class SurfaceAttachment extends StatelessWidget {
  const SurfaceAttachment({
    super.key,
    required this.child,
    required this.attached,
    this.axis = Axis.vertical,
    this.borderRadius = const BorderRadius.all(Radius.circular(20)),
  });

  final Widget child;
  final bool attached;
  final Axis axis;
  final BorderRadius borderRadius;

  @override
  Widget build(BuildContext context) {
    final duration = _motionDuration(context, 280);
    final offset = attached
        ? Offset.zero
        : axis == Axis.vertical
        ? const Offset(0, 0.035)
        : const Offset(0.035, 0);
    return AnimatedSlide(
      offset: offset,
      duration: duration,
      curve: Curves.easeOutCubic,
      child: AnimatedScale(
        scale: attached ? 1 : 0.985,
        duration: duration,
        curve: Curves.easeOutCubic,
        child: AnimatedOpacity(
          opacity: attached ? 1 : 0.95,
          duration: duration,
          curve: Curves.easeOutCubic,
          child: TweenAnimationBuilder<double>(
            tween: Tween(end: attached ? 0.0 : 1.0),
            duration: duration,
            curve: Curves.easeOutCubic,
            child: child,
            builder: (context, floatAmount, child) => CustomPaint(
              painter: floatAmount <= 0.001
                  ? null
                  : OuterShadowPainter(
                      borderRadius: borderRadius,
                      shadows: [
                        BoxShadow(
                          color: Colors.black.withValues(
                            alpha: 0.10 * floatAmount,
                          ),
                          blurRadius: 16,
                          offset: Offset(0, 7 * floatAmount),
                        ),
                      ],
                    ),
              child: child,
            ),
          ),
        ),
      ),
    );
  }
}
