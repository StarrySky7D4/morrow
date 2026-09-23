import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';

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
    final scale = pressed ? 0.988 : (hovered ? 1.006 : 1.0);
    final rise = hovered ? -2.0 : 0.0;
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
        child: AnimatedContainer(
          duration: motion,
          curve: Curves.easeOutCubic,
          transformAlignment: Alignment.center,
          transform: Matrix4.identity()
            ..setEntry(0, 0, scale)
            ..setEntry(1, 1, scale)
            ..setEntry(1, 3, rise),
          decoration: BoxDecoration(
            borderRadius: widget.borderRadius,
            boxShadow: [
              BoxShadow(
                color: Colors.black.withValues(alpha: hovered ? 0.13 : 0),
                blurRadius: hovered ? 20 : 0,
                offset: Offset(0, hovered ? 9 : 0),
              ),
            ],
          ),
          child: widget.child,
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
          child: AnimatedContainer(
            duration: duration,
            curve: Curves.easeOutCubic,
            decoration: BoxDecoration(
              borderRadius: borderRadius,
              boxShadow: [
                BoxShadow(
                  color: Colors.black.withValues(alpha: attached ? 0 : 0.10),
                  blurRadius: attached ? 0 : 18,
                  offset: Offset(0, attached ? 0 : 7),
                ),
              ],
            ),
            child: child,
          ),
        ),
      ),
    );
  }
}
