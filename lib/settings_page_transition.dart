import 'package:flutter/material.dart';

/// Fades through the background so workspace and settings never overlap.
class SettingsPageTransition extends StatefulWidget {
  const SettingsPageTransition({
    super.key,
    required this.showSettings,
    required this.duration,
    required this.content,
    required this.settings,
  });

  final bool showSettings;
  final Duration duration;
  final Widget content;
  final Widget settings;

  @override
  State<SettingsPageTransition> createState() => _SettingsPageTransitionState();
}

class _SettingsPageTransitionState extends State<SettingsPageTransition>
    with SingleTickerProviderStateMixin {
  late final AnimationController motion;

  @override
  void initState() {
    super.initState();
    motion = AnimationController(
      vsync: this,
      duration: widget.duration,
      value: widget.showSettings ? 1 : 0,
    );
  }

  @override
  void didUpdateWidget(SettingsPageTransition oldWidget) {
    super.didUpdateWidget(oldWidget);
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
  void dispose() {
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
                child: IgnorePointer(
                  ignoring: motion.isAnimating,
                  child: Opacity(
                    key: const ValueKey('workspace-transition-opacity'),
                    opacity: contentOpacity,
                    child: Transform.translate(
                      offset: Offset(-12 * (1 - contentOpacity), 0),
                      child: widget.content,
                    ),
                  ),
                ),
              ),
            ),
          ),
          if (settingsVisible)
            IgnorePointer(
              ignoring: motion.isAnimating,
              child: Opacity(
                key: const ValueKey('settings-transition-opacity'),
                opacity: settingsOpacity,
                child: Transform.translate(
                  offset: Offset(12 * (1 - settingsOpacity), 0),
                  child: widget.settings,
                ),
              ),
            ),
        ],
      );
    },
  );
}
