import 'package:flutter/material.dart';
import 'appearance.dart';

/// Keeps one live child through reversals: scroll, focus and player state survive.
class CollapsiblePanel extends StatelessWidget {
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
  Widget build(BuildContext context) => TweenAnimationBuilder<double>(
    tween: Tween(end: expanded ? 1 : 0),
    duration: motionDuration(context, 340),
    curve: Curves.easeInOutCubic,
    child: child,
    builder: (context, value, child) {
      final panel = IgnorePointer(
        ignoring: !expanded,
        child: ExcludeFocus(
          excluding: !expanded,
          child: ExcludeSemantics(
            excluding: !expanded,
            child: TickerMode(
              enabled: value > 0,
              child: Opacity(
                opacity: value.clamp(0, 1),
                child: axis == Axis.horizontal
                    ? SizedBox(width: extent, child: child)
                    : child,
              ),
            ),
          ),
        ),
      );
      return ClipRect(
        child: Align(
          alignment: Alignment.topLeft,
          widthFactor: axis == Axis.horizontal ? value : null,
          heightFactor: axis == Axis.vertical ? value : null,
          child: panel,
        ),
      );
    },
  );
}
