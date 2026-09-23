import 'package:flutter/material.dart';

/// Keeps one live child through reversals: scroll, focus and player state survive.
class CollapsiblePanel extends StatefulWidget {
  const CollapsiblePanel({
    super.key,
    required this.expanded,
    required this.child,
    this.axis = Axis.vertical,
    this.extent,
  });

  final bool expanded;
  final Widget child;
  final Axis axis;
  final double? extent;

  @override
  State<CollapsiblePanel> createState() => _CollapsiblePanelState();
}

class _CollapsiblePanelState extends State<CollapsiblePanel>
    with SingleTickerProviderStateMixin {
  late final AnimationController _progress = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 300),
    reverseDuration: const Duration(milliseconds: 220),
    value: widget.expanded ? 1 : 0,
  );
  bool _motionEnabled = true;

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    _motionEnabled =
        !MediaQuery.disableAnimationsOf(context) &&
        TickerMode.valuesOf(context).enabled;
    if (!_motionEnabled) {
      _progress.value = widget.expanded ? 1 : 0;
    }
  }

  @override
  void didUpdateWidget(CollapsiblePanel oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.expanded == widget.expanded) return;
    if (!_motionEnabled) {
      _progress.value = widget.expanded ? 1 : 0;
    } else if (widget.expanded) {
      _progress.forward();
    } else {
      _progress.reverse();
    }
  }

  @override
  void dispose() {
    _progress.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => AnimatedBuilder(
    animation: _progress,
    child: widget.child,
    builder: (context, child) {
      final value = Curves.easeInOutCubic.transform(_progress.value);
      final reveal = Curves.easeOut.transform((value * 1.4).clamp(0.0, 1.0));
      final horizontal = widget.axis == Axis.horizontal;
      final panel = IgnorePointer(
        ignoring: !widget.expanded,
        child: ExcludeFocus(
          excluding: !widget.expanded,
          child: ExcludeSemantics(
            excluding: !widget.expanded,
            child: TickerMode(
              enabled: _progress.value > 0,
              child: Opacity(
                opacity: reveal,
                child: FractionalTranslation(
                  translation: horizontal
                      ? Offset(-0.045 * (1 - value), 0)
                      : Offset(0, -0.065 * (1 - value)),
                  child: horizontal
                      ? SizedBox(width: widget.extent, child: child)
                      : child,
                ),
              ),
            ),
          ),
        ),
      );
      return ClipRect(
        child: Align(
          alignment: Alignment.topLeft,
          widthFactor: horizontal ? value : null,
          heightFactor: horizontal ? null : value,
          child: panel,
        ),
      );
    },
  );
}
