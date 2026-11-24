import 'package:material_ui/material_ui.dart';
import 'liquid_glass.dart';

/// Fades through the background so workspace and settings never overlap.
class SettingsPageTransition extends StatefulWidget {
  const SettingsPageTransition({
    super.key,
    required this.showSettings,
    required this.duration,
    required this.content,
    this.settings,
    this.settingsBuilder,
  }) : assert((settings != null) ^ (settingsBuilder != null));

  final bool showSettings;
  final Duration duration;
  final Widget content;
  final Widget? settings;
  final WidgetBuilder? settingsBuilder;

  @override
  State<SettingsPageTransition> createState() => _SettingsPageTransitionState();
}

class _SettingsPageTransitionState extends State<SettingsPageTransition>
    with SingleTickerProviderStateMixin, WidgetsBindingObserver {
  late final AnimationController motion;
  bool _settingsVisited = false;
  bool _settingsWereVisible = false;
  final _settingsFocus = FocusScopeNode(debugLabel: 'retained settings');
  void _syncFocus() {
    final visible = motion.value >= .5;
    if (visible && !_settingsWereVisible) {
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (mounted && motion.value >= .5) _settingsFocus.requestFocus();
      });
    }
    _settingsWereVisible = visible;
  }

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    _settingsVisited = widget.showSettings;
    motion = AnimationController(
      vsync: this,
      duration: widget.duration,
      value: widget.showSettings ? 1 : 0,
    )..addListener(_syncFocus);
    _syncFocus();
  }

  @override
  void didUpdateWidget(SettingsPageTransition oldWidget) {
    super.didUpdateWidget(oldWidget);
    _settingsVisited |= widget.showSettings;
    motion.duration = widget.duration;
    if (widget.duration == Duration.zero) {
      motion.value = widget.showSettings ? 1 : 0;
    } else if (widget.showSettings != oldWidget.showSettings) {
      if (widget.showSettings) {
        motion.forward();
      } else {
        motion.reverse();
      }
    }
  }

  @override
  void didHaveMemoryPressure() {
    if (!widget.showSettings && motion.value == 0 && _settingsVisited) {
      setState(() => _settingsVisited = false);
    }
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    _settingsFocus.dispose();
    motion.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => AnimatedBuilder(
    animation: motion,
    builder: (context, _) {
      final settingsVisible = motion.value >= .5;
      final contentOpacity =
          1 - Curves.easeInCubic.transform((motion.value * 2).clamp(0.0, 1.0));
      final settingsOpacity = Curves.easeOutCubic.transform(
        (motion.value * 2 - 1).clamp(0.0, 1.0),
      );
      return Stack(
        children: [
          Offstage(
            offstage: settingsVisible,
            child: ExcludeFocus(
              excluding: settingsVisible,
              child: TickerMode(
                enabled: !settingsVisible,
                child: Opacity(
                  key: const ValueKey('workspace-transition-opacity'),
                  opacity: contentOpacity,
                  // Offstage selects the accessible page even at zero opacity.
                  alwaysIncludeSemantics: true,
                  child: BackdropFilterGuard(
                    suspended: motion.isAnimating || contentOpacity < 0.999,
                    child: Transform.translate(
                      offset: Offset(-12 * (1 - contentOpacity), 0),
                      child: Transform.scale(
                        scale: 0.988 + 0.012 * contentOpacity,
                        child: widget.content,
                      ),
                    ),
                  ),
                ),
              ),
            ),
          ),
          if (_settingsVisited)
            Offstage(
              offstage: !settingsVisible,
              child: ExcludeFocus(
                excluding: !settingsVisible,
                child: TickerMode(
                  enabled: settingsVisible,
                  child: Opacity(
                    key: const ValueKey('settings-transition-opacity'),
                    opacity: settingsOpacity,
                    alwaysIncludeSemantics: true,
                    child: BackdropFilterGuard(
                      suspended: motion.isAnimating || settingsOpacity < 0.999,
                      child: Transform.translate(
                        offset: Offset(12 * (1 - settingsOpacity), 0),
                        child: Transform.scale(
                          scale: 0.988 + 0.012 * settingsOpacity,
                          child: FocusScope(
                            node: _settingsFocus,
                            child:
                                widget.settings ??
                                widget.settingsBuilder!(context),
                          ),
                        ),
                      ),
                    ),
                  ),
                ),
              ),
            ),
          // Intercept physical clicks without rewriting the active page's
          // accessibility actions as the transition starts and finishes.
          if (motion.isAnimating)
            const Positioned.fill(
              child: ExcludeSemantics(child: AbsorbPointer()),
            ),
        ],
      );
    },
  );
}
