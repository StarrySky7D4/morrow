import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'appearance.dart';

/// Multiline EditableText asks its inherited behavior to re-enable scrollbars.
/// Override the builder itself so that request cannot introduce side rails.
class StudioScrollBehavior extends MaterialScrollBehavior {
  const StudioScrollBehavior();
  @override
  Widget buildScrollbar(
    BuildContext context,
    Widget child,
    ScrollableDetails details,
  ) => child;
}

/// Navigation already animates the destination. Keep hover, focus and pressed
/// highlights, without a second clipped ripple over the outgoing glass page.
class SettingsNavigationFeedback extends StatelessWidget {
  const SettingsNavigationFeedback({super.key, required this.child});
  final Widget child;

  @override
  Widget build(BuildContext context) => Theme(
    data: Theme.of(context).copyWith(splashFactory: NoSplash.splashFactory),
    child: child,
  );
}

/// Keep the shared canvas live. Material's zoom transition snapshots entire
/// glass pages, delays entrance opacity and adds a forward-only opaque scrim.
/// Settings use the same short, unscaled transition in both directions instead.
class CanvasSettingsRoute<T> extends PageRouteBuilder<T> {
  CanvasSettingsRoute({required WidgetBuilder builder})
    : super(
        allowSnapshotting: false,
        transitionDuration: const Duration(milliseconds: 220),
        reverseTransitionDuration: const Duration(milliseconds: 220),
        pageBuilder: (context, animation, secondaryAnimation) => Semantics(
          scopesRoute: true,
          explicitChildNodes: true,
          child: builder(context),
        ),
        transitionsBuilder: (context, animation, secondaryAnimation, child) {
          if (MediaQuery.disableAnimationsOf(context)) return child;
          return AnimatedBuilder(
            animation: Listenable.merge([animation, secondaryAnimation]),
            child: child,
            builder: (context, child) {
              final enter = Curves.easeOutCubic.transform(animation.value);
              final leave = Curves.easeOutCubic.transform(
                secondaryAnimation.value,
              );
              return IgnorePointer(
                ignoring:
                    animation.status == AnimationStatus.reverse ||
                    secondaryAnimation.value > 0,
                child: Opacity(
                  key: const ValueKey('settings-route-opacity'),
                  opacity: enter * (1 - leave),
                  // Navigator owns when a covered route leaves semantics.
                  // Do not independently detach it at the zero-opacity frame.
                  alwaysIncludeSemantics: true,
                  child: child,
                ),
              );
            },
          );
        },
      );
}

/// Routes live above the existing canvas, so video, tint and native transparency
/// stay continuous. Only the foreground changes when a settings page opens.
class CanvasNavigation extends StatelessWidget {
  const CanvasNavigation({
    super.key,
    required this.navigatorKey,
    required this.child,
  });
  final GlobalKey<NavigatorState> navigatorKey;
  final Widget child;

  @override
  Widget build(BuildContext context) => _CanvasHomeScope(
    content: child,
    child: NavigatorPopHandler<Object?>(
      onPopWithResult: (result) => navigatorKey.currentState!.maybePop(result),
      child: Navigator(
        key: navigatorKey,
        onGenerateRoute: (_) => CanvasSettingsRoute<void>(
          builder: (context) => context
              .dependOnInheritedWidgetOfExactType<_CanvasHomeScope>()!
              .content,
        ),
      ),
    ),
  );
}

class _CanvasHomeScope extends InheritedWidget {
  const _CanvasHomeScope({required this.content, required super.child});
  final Widget content;
  @override
  bool updateShouldNotify(_CanvasHomeScope oldWidget) =>
      content != oldWidget.content;
}

/// Shared page chrome. The app canvas also covers the native caption above us.
class SettingsSurface extends StatelessWidget {
  const SettingsSurface({
    super.key,
    required this.title,
    required this.child,
    this.onBack,
    this.backKey,
    this.backTooltip,
  });
  final String title;
  final Widget child;
  final VoidCallback? onBack;
  final Key? backKey;
  final String? backTooltip;

  @override
  Widget build(BuildContext context) {
    void goBack() {
      if (onBack != null) {
        onBack!();
      } else {
        Navigator.of(context).maybePop();
      }
    }

    return CallbackShortcuts(
      bindings: {const SingleActivator(LogicalKeyboardKey.escape): goBack},
      child: Focus(
        autofocus: true,
        child: Scaffold(
          backgroundColor: Colors.transparent,
          body: LayoutBuilder(
            builder: (context, constraints) => Padding(
              padding: EdgeInsets.symmetric(
                horizontal: constraints.maxWidth >= 600 ? 24 : 12,
              ),
              child: Align(
                alignment: Alignment.topCenter,
                child: ConstrainedBox(
                  constraints: const BoxConstraints(maxWidth: 920),
                  child: SizedBox.expand(
                    key: const ValueKey('settings-content-column'),
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.stretch,
                      children: [
                        SettingsPageHeader(
                          title: title,
                          onBack: goBack,
                          backKey: backKey,
                          backTooltip: backTooltip,
                        ),
                        Expanded(child: child),
                      ],
                    ),
                  ),
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}

/// Plain page chrome over the shared canvas, independent of card materials.
class SettingsPageHeader extends StatelessWidget {
  const SettingsPageHeader({
    super.key,
    required this.title,
    required this.onBack,
    this.backKey,
    this.backTooltip,
  });
  final String title;
  final VoidCallback onBack;
  final Key? backKey;
  final String? backTooltip;

  @override
  Widget build(BuildContext context) {
    final p = AppearanceScope.of(context);
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 12),
      child: Row(
        children: [
          if (backTooltip == null)
            BackButton(key: backKey, color: p.ink, onPressed: onBack)
          else
            IconButton(
              key: backKey,
              color: p.ink,
              onPressed: onBack,
              tooltip: backTooltip,
              icon: const BackButtonIcon(),
            ),
          const SizedBox(width: 8),
          Expanded(
            child: Semantics(
              header: true,
              child: Text(
                title,
                maxLines: 2,
                overflow: TextOverflow.ellipsis,
                style: TextStyle(
                  color: p.ink,
                  fontSize: 18,
                  fontWeight: FontWeight.w600,
                ),
              ),
            ),
          ),
        ],
      ),
    );
  }
}

/// Sections keep one reading order within the page's bounded content column.
class SettingsSections extends StatelessWidget {
  const SettingsSections({super.key, required this.children});
  final List<Widget> children;

  @override
  Widget build(BuildContext context) => Column(
    crossAxisAlignment: CrossAxisAlignment.stretch,
    children: [
      for (var index = 0; index < children.length; index++) ...[
        if (index > 0) const SizedBox(height: 20),
        children[index],
      ],
    ],
  );
}
