import 'package:material_ui/material_ui.dart';
import '../appearance.dart';
import '../neumorphic_controls.dart';

/// Unified motion curves for the modern workbench layout.
abstract final class WorkbenchCurves {
  static const Curve springOut = Cubic(0.16, 1.0, 0.3, 1.0);
  static const Curve emphasized = Cubic(0.2, 0.0, 0.0, 1.0);
  static const Curve snappy = Cubic(0.25, 1.0, 0.5, 1.0);
}

/// Standardized motion durations in milliseconds.
abstract final class WorkbenchDurations {
  static const int micro = 140;
  static const int short = 220;
  static const int medium = 300;
  static const int page = 360;
}

Duration workbenchDuration(BuildContext context, int milliseconds) =>
    MediaQuery.disableAnimationsOf(context) ||
        !TickerMode.valuesOf(context).enabled
    ? Duration.zero
    : Duration(milliseconds: milliseconds);

/// Shared-axis crossfade + subtle slide + scale transition for panels and tabs.
class SharedAxisSwitcher extends StatelessWidget {
  const SharedAxisSwitcher({
    super.key,
    required this.child,
    this.axis = Axis.vertical,
    this.durationMs = WorkbenchDurations.short,
    this.alignment = Alignment.topCenter,
    this.slideOffset = 0.03,
  });

  final Widget child;
  final Axis axis;
  final int durationMs;
  final Alignment alignment;
  final double slideOffset;

  @override
  Widget build(BuildContext context) {
    final duration = workbenchDuration(context, durationMs);
    return AnimatedSwitcher(
      duration: duration,
      reverseDuration: workbenchDuration(context, (durationMs * 0.75).round()),
      switchInCurve: WorkbenchCurves.springOut,
      switchOutCurve: Curves.easeInCubic,
      layoutBuilder: (currentChild, previousChildren) => Stack(
        alignment: alignment,
        children: [?currentChild],
      ),
      transitionBuilder: (child, animation) {
        if (duration == Duration.zero) return child;
        final beginOffset = axis == Axis.vertical
            ? Offset(0, slideOffset)
            : Offset(slideOffset, 0);
        final curved = CurvedAnimation(
          parent: animation,
          curve: WorkbenchCurves.springOut,
        );
        return FadeTransition(
          opacity: animation,
          alwaysIncludeSemantics: true,
          child: SlideTransition(
            position: Tween<Offset>(
              begin: beginOffset,
              end: Offset.zero,
            ).animate(curved),
            child: ScaleTransition(
              scale: Tween<double>(begin: 0.985, end: 1.0).animate(curved),
              child: child,
            ),
          ),
        );
      },
      child: child,
    );
  }
}

/// Bounded staggered entrance for cards and rows without dangling timers.
class StaggeredReveal extends StatefulWidget {
  const StaggeredReveal({
    super.key,
    required this.index,
    required this.child,
  });

  final int index;
  final Widget child;

  @override
  State<StaggeredReveal> createState() => _StaggeredRevealState();
}

class _StaggeredRevealState extends State<StaggeredReveal>
    with SingleTickerProviderStateMixin {
  late final AnimationController _controller;

  @override
  void initState() {
    super.initState();
    final delayMs = widget.index.clamp(0, 8) * 24;
    _controller = AnimationController(
      vsync: this,
      duration: Duration(milliseconds: WorkbenchDurations.medium + delayMs),
    );
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    final disabled =
        MediaQuery.disableAnimationsOf(context) ||
        !TickerMode.valuesOf(context).enabled;
    if (disabled) {
      _controller.value = 1;
    } else if (_controller.status == AnimationStatus.dismissed) {
      _controller.forward();
    }
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final delayMs = widget.index.clamp(0, 8) * 24;
    final totalMs = WorkbenchDurations.medium + delayMs;
    final start = totalMs == 0 ? 0.0 : (delayMs / totalMs).clamp(0.0, 0.7);
    final curve = CurvedAnimation(
      parent: _controller,
      curve: Interval(start, 1.0, curve: WorkbenchCurves.springOut),
    );
    return AnimatedBuilder(
      animation: curve,
      child: widget.child,
      builder: (context, child) {
        final v = curve.value;
        return Opacity(
          opacity: v,
          alwaysIncludeSemantics: true,
          child: Transform.translate(
            offset: Offset(0, 12 * (1 - v)),
            child: child,
          ),
        );
      },
    );
  }
}

/// Segment metadata for [SlidingSegmentBar].
class SegmentOption<T> {
  const SegmentOption({
    required this.value,
    required this.icon,
    required this.label,
    this.badge,
    this.itemKey,
  });

  final T value;
  final IconData icon;
  final String label;
  final Widget? badge;
  final Key? itemKey;
}

/// Segmented pill control with an animated sliding indicator pill.
class SlidingSegmentBar<T> extends StatelessWidget {
  const SlidingSegmentBar({
    super.key,
    required this.value,
    required this.options,
    required this.onChanged,
    this.height = 38,
  });

  final T value;
  final List<SegmentOption<T>> options;
  final ValueChanged<T> onChanged;
  final double height;

  @override
  Widget build(BuildContext context) {
    final p = AppearanceScope.of(context);
    final count = options.length;
    final selectedIndex = options.indexWhere((o) => o.value == value);
    final safeIndex = selectedIndex < 0 ? 0 : selectedIndex;
    final alignX = count <= 1 ? 0.0 : -1.0 + (2.0 * safeIndex) / (count - 1);
    final duration = workbenchDuration(context, WorkbenchDurations.short);

    return NeumorphicSurface(
      palette: p,
      depth: -0.7,
      borderRadius: p.borderRadius(12),
      child: Container(
        height: height,
        padding: const EdgeInsets.all(3),
        decoration: BoxDecoration(
          color: p.surface.withValues(alpha: p.dark ? 0.28 : 0.42),
          borderRadius: p.borderRadius(12),
          border: Border.all(color: p.line),
        ),
        child: Stack(
          children: [
            AnimatedAlign(
              duration: duration,
              curve: WorkbenchCurves.springOut,
              alignment: Alignment(alignX, 0),
              child: FractionallySizedBox(
                widthFactor: count == 0 ? 1 : 1 / count,
                heightFactor: 1,
                child: NeumorphicSurface(
                  palette: p,
                  depth: 0.8,
                  borderRadius: p.borderRadius(9),
                  child: DecoratedBox(
                    decoration: BoxDecoration(
                      color: p.accent.withValues(alpha: p.dark ? 0.22 : 0.14),
                      borderRadius: p.borderRadius(9),
                      border: Border.all(
                        color: p.accent.withValues(alpha: 0.35),
                      ),
                    ),
                  ),
                ),
              ),
            ),
            Row(
              children: [
                for (final option in options)
                  Expanded(
                    child: Material(
                      color: Colors.transparent,
                      child: InkWell(
                        key: option.itemKey,
                        borderRadius: p.borderRadius(9),
                        onTap: () => onChanged(option.value),
                        child: Center(
                          child: Padding(
                            padding: const EdgeInsets.symmetric(horizontal: 4),
                            child: Row(
                              mainAxisSize: MainAxisSize.min,
                              children: [
                                Icon(
                                  option.icon,
                                  size: 14,
                                  color: option.value == value
                                      ? p.accent
                                      : p.muted,
                                ),
                                const SizedBox(width: 5),
                                Flexible(
                                  child: Text(
                                    option.label,
                                    maxLines: 1,
                                    overflow: TextOverflow.ellipsis,
                                    style: TextStyle(
                                      fontSize: 11,
                                      fontWeight: option.value == value
                                          ? FontWeight.w600
                                          : FontWeight.w500,
                                      color: option.value == value
                                          ? p.ink
                                          : p.muted,
                                    ),
                                  ),
                                ),
                                if (option.badge != null) ...[
                                  const SizedBox(width: 4),
                                  option.badge!,
                                ],
                              ],
                            ),
                          ),
                        ),
                      ),
                    ),
                  ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}
