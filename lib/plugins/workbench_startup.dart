import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'workbench_loading.dart';

/// Keeps the loading surface alive while the restored workbench lays out and
/// paints underneath it. Only the outgoing cover animates; the live
/// workbench is never rebuilt or faded into an expensive offscreen layer.
class WorkbenchStartup extends StatefulWidget {
  const WorkbenchStartup({
    super.key,
    required this.ready,
    this.child,
    this.locale,
    this.onRevealed,
  });

  final ValueListenable<bool> ready;
  final Widget? child;
  final Locale? locale;
  final VoidCallback? onRevealed;

  @override
  State<WorkbenchStartup> createState() => _WorkbenchStartupState();
}

class _WorkbenchStartupState extends State<WorkbenchStartup>
    with SingleTickerProviderStateMixin {
  late final _fade = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 100),
  )..addStatusListener(_status);
  late final _curve = CurvedAnimation(
    parent: _fade,
    curve: Curves.easeOutCubic,
  );
  // Keep the cover's opacity layer in use from its first frame, so revealing
  // content does not introduce a new compositing path during the animation.
  late final _opacity = Tween<double>(begin: .999, end: 0).animate(_curve);
  bool _started = false;
  bool _revealing = false;
  bool _revealed = false;

  @override
  void initState() {
    super.initState();
    widget.ready.addListener(_ready);
    _ready();
  }

  @override
  void didUpdateWidget(WorkbenchStartup oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.ready != widget.ready) {
      oldWidget.ready.removeListener(_ready);
      widget.ready.addListener(_ready);
    }
    _ready();
  }

  void _ready() {
    if (_started || !widget.ready.value || widget.child == null) return;
    _started = true;
    // Let the ready workbench submit its first frame beneath the cover before
    // beginning the reveal. No minimum splash delay or artificial progress.
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted) return;
      setState(() => _revealing = true);
      if (WidgetsBinding
          .instance
          .platformDispatcher
          .accessibilityFeatures
          .disableAnimations) {
        _fade.value = 1;
      } else {
        _fade.forward();
      }
    });
    WidgetsBinding.instance.ensureVisualUpdate();
  }

  void _status(AnimationStatus status) {
    if (status != AnimationStatus.completed || !mounted) return;
    setState(() => _revealed = true);
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) widget.onRevealed?.call();
    });
  }

  @override
  void dispose() {
    widget.ready.removeListener(_ready);
    _curve.dispose();
    _fade.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => Directionality(
    textDirection: TextDirection.ltr,
    child: Stack(
      fit: StackFit.expand,
      children: [
        if (widget.child != null)
          IgnorePointer(
            ignoring: !_revealing,
            child: ExcludeFocus(
              excluding: !_revealing,
              child: ExcludeSemantics(
                excluding: !_revealing,
                child: widget.child!,
              ),
            ),
          ),
        if (!_revealed)
          FadeTransition(
            opacity: _opacity,
            child: IgnorePointer(
              ignoring: _revealing,
              child: ExcludeSemantics(
                excluding: _revealing,
                child: RepaintBoundary(
                  child: WorkbenchLoading(locale: widget.locale),
                ),
              ),
            ),
          ),
      ],
    ),
  );
}
