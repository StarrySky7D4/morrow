import 'dart:ui';
import 'package:flutter/material.dart';
import 'media/texture_source.dart';

enum GlassMode { frosted, clear }

enum StudioTheme { white, custom, dark }

enum BackgroundMode { ambient, solid, texture, transparent }

class Palette {
  const Palette(
    this.theme,
    this.mode, [
    this.backdrop = BackgroundMode.ambient,
    this.solidTint = 0,
    this.frostedOpacity = .76,
    this.customColor,
    this.texture,
    this.mediaPlaying = true,
    this.cornerRadius = 20,
    this.grayscale = 0,
    this.themeLightness,
    this.windowRadius = 20,
  ]);
  final StudioTheme theme;
  final GlassMode mode;
  final BackgroundMode backdrop;
  final int solidTint;
  final double frostedOpacity;
  final Color? customColor;
  final TextureSource? texture;
  final bool mediaPlaying;
  final double cornerRadius, grayscale, windowRadius;
  final double? themeLightness;
  BorderRadius borderRadius(double base) =>
      BorderRadius.circular(base * cornerRadius.clamp(0, 32) / 20);
  Color tone(Color color) {
    final gray = .2126 * color.r + .7152 * color.g + .0722 * color.b;
    return Color.lerp(
      color,
      Color.from(alpha: color.a, red: gray, green: gray, blue: gray),
      isCustom ? grayscale.clamp(0, 1) : 0,
    )!;
  }

  bool get isCustom => theme == StudioTheme.custom;
  double get lightness => switch (theme) {
    StudioTheme.white => .975,
    StudioTheme.custom => themeLightness ?? .885,
    StudioTheme.dark => .095,
  };
  bool get dark => isCustom ? lightness < .46 : theme == StudioTheme.dark;
  bool get clear => mode == GlassMode.clear;
  Color get ink => isCustom
      ? (dark ? Colors.white : Colors.black)
      : tone(dark ? const Color(0xFFF0EDF8) : const Color(0xFF302D43));
  Color get muted => isCustom
      ? Color.lerp(ink, background, .12)!
      : tone(dark ? const Color(0xFFB4AEC5) : const Color(0xFF777184));
  Color get accent =>
      tone(dark ? const Color(0xFFC0AFFA) : const Color(0xFF7662BA));
  Color get background => isCustom
      ? Color.from(alpha: 1, red: lightness, green: lightness, blue: lightness)
      : tone(switch (theme) {
          StudioTheme.white => const Color(0xFFF9F8FC),
          StudioTheme.custom => const Color(0xFFE1E3E9),
          StudioTheme.dark => const Color(0xFF181720),
        });
  Color get surface => isCustom
      ? Color.lerp(
          background,
          dark ? Colors.white : Colors.black,
          dark ? .065 : .015,
        )!
      : tone(dark ? const Color(0xFF292634) : Colors.white);
  Color get solidColor =>
      customColor ??
      (solidTint == 0
          ? background
          : (dark
                ? const [
                    Color(0xFF181720),
                    Color(0xFF282437),
                    Color(0xFF20322D),
                    Color(0xFF342923),
                  ]
                : const [
                    Color(0xFFF9F8FC),
                    Color(0xFFE5DEF0),
                    Color(0xFFDDE9E2),
                    Color(0xFFF0E3D8),
                  ])[solidTint]);
  Color get line => ink.withValues(alpha: dark ? .13 : .075);
  Color get glassEdge => Color.lerp(
    backdrop == BackgroundMode.solid ? solidColor : surface,
    ink,
    .18,
  )!.withValues(alpha: clear ? .22 : .32 * frostedOpacity.clamp(.2, 1));

  Color get captionColor =>
      backdrop == BackgroundMode.transparent ||
          backdrop == BackgroundMode.texture
      ? Colors.transparent
      : (backdrop == BackgroundMode.solid ? solidColor : surface).withValues(
          alpha: clear ? .12 : frostedOpacity.clamp(.2, 1),
        );
}

class Glass extends StatelessWidget {
  const Glass({
    super.key,
    required this.p,
    required this.child,
    this.radius = 20,
    this.dialog = false,
  });
  final Palette p;
  final Widget child;
  final double radius;
  final bool dialog;
  @override
  Widget build(BuildContext context) => Container(
    decoration: BoxDecoration(
      borderRadius: p.borderRadius(radius),
      boxShadow: [
        BoxShadow(
          color: (p.dark ? Colors.black : const Color(0xFF716386)).withValues(
            alpha: p.clear ? .07 : .035,
          ),
          blurRadius: p.clear ? 24 : 18,
          offset: const Offset(0, 8),
        ),
      ],
    ),
    child: ClipRRect(
      borderRadius: p.borderRadius(radius),
      child: BackdropFilter(
        filter: ImageFilter.blur(
          sigmaX: p.clear ? 4 : 22,
          sigmaY: p.clear ? 4 : 22,
        ),
        child: AnimatedContainer(
          duration: motionDuration(context, 220),
          decoration: BoxDecoration(
            borderRadius: p.borderRadius(radius),
            gradient: LinearGradient(
              begin: Alignment.topLeft,
              end: Alignment.bottomRight,
              colors: [
                (dialog && p.backdrop == BackgroundMode.solid
                        ? p.solidColor
                        : p.surface)
                    .withValues(
                      alpha: p.clear
                          ? (p.dark ? .23 : .28)
                          : p.frostedOpacity.clamp(.2, 1),
                    ),
                (dialog && p.backdrop == BackgroundMode.solid
                        ? p.solidColor
                        : p.surface)
                    .withValues(
                      alpha: p.clear ? .07 : p.frostedOpacity.clamp(.2, 1),
                    ),
              ],
            ),
            border: Border.all(color: p.glassEdge, width: p.clear ? 1.2 : 1),
          ),
          child: child,
        ),
      ),
    ),
  );
}

Duration motionDuration(BuildContext context, int milliseconds) =>
    MediaQuery.disableAnimationsOf(context)
    ? Duration.zero
    : Duration(milliseconds: milliseconds);

class AppearanceScope extends InheritedWidget {
  const AppearanceScope({
    super.key,
    required this.palette,
    required super.child,
  });
  final Palette palette;
  static Palette of(BuildContext context) =>
      context.dependOnInheritedWidgetOfExactType<AppearanceScope>()?.palette ??
      const Palette(StudioTheme.white, GlassMode.frosted);
  @override
  bool updateShouldNotify(AppearanceScope oldWidget) =>
      oldWidget.palette != palette;
}

/// Shared by editors, pickers and detail dialogs. Media is never duplicated
/// inside a dialog: the existing canvas shows through the glass instead.
class StudioDialog extends StatelessWidget {
  const StudioDialog({
    super.key,
    required this.title,
    required this.content,
    this.subtitle,
    this.icon = Icons.auto_awesome_outlined,
    this.actions = const [],
    this.width = 460,
  });
  final String title;
  final String? subtitle;
  final IconData icon;
  final Widget content;
  final List<Widget> actions;
  final double width;
  @override
  Widget build(BuildContext context) {
    final p = AppearanceScope.of(context);
    final height =
        (MediaQuery.sizeOf(context).height -
                MediaQuery.viewInsetsOf(context).bottom -
                40)
            .clamp(160.0, 760.0);
    return Dialog(
      key: const ValueKey('studio-dialog'),
      backgroundColor: Colors.transparent,
      surfaceTintColor: Colors.transparent,
      elevation: 0,
      insetPadding: const EdgeInsets.symmetric(horizontal: 20, vertical: 20),
      child: ConstrainedBox(
        constraints: BoxConstraints(maxWidth: width, maxHeight: height),
        child: Glass(
          p: p,
          dialog: true,
          radius: 28,
          child: Padding(
            padding: const EdgeInsets.all(24),
            child: Column(
              mainAxisSize: MainAxisSize.min,
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                Row(
                  children: [
                    Container(
                      width: 38,
                      height: 38,
                      decoration: BoxDecoration(
                        color: p.accent.withValues(alpha: .15),
                        borderRadius: p.borderRadius(13),
                      ),
                      child: Icon(icon, color: p.accent, size: 21),
                    ),
                    const SizedBox(width: 12),
                    Expanded(
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Text(
                            'DAEMON / A LITTLE POSSIBILITY',
                            style: TextStyle(
                              color: p.muted,
                              fontSize: 8,
                              letterSpacing: 1,
                            ),
                          ),
                          const SizedBox(height: 5),
                          Text(
                            title,
                            style: TextStyle(
                              color: p.ink,
                              fontSize: 20,
                              fontWeight: FontWeight.w600,
                            ),
                          ),
                        ],
                      ),
                    ),
                    IconButton(
                      tooltip: '关闭弹窗',
                      onPressed: () => Navigator.pop(context),
                      icon: Icon(Icons.close_rounded, size: 18, color: p.muted),
                    ),
                  ],
                ),
                if (subtitle != null) ...[
                  const SizedBox(height: 12),
                  Text(
                    subtitle!,
                    style: TextStyle(color: p.muted, fontSize: 11, height: 1.7),
                  ),
                ],
                const SizedBox(height: 22),
                Flexible(child: SingleChildScrollView(child: content)),
                if (actions.isNotEmpty) ...[
                  const SizedBox(height: 20),
                  Divider(height: 1, color: p.line),
                  const SizedBox(height: 16),
                  Wrap(
                    alignment: WrapAlignment.end,
                    spacing: 8,
                    runSpacing: 8,
                    children: actions,
                  ),
                ],
              ],
            ),
          ),
        ),
      ),
    );
  }
}

Future<T?> showStudioDialog<T>({
  required BuildContext context,
  required WidgetBuilder builder,
}) => showGeneralDialog<T>(
  context: context,
  barrierDismissible: true,
  barrierLabel: '关闭弹窗',
  barrierColor: AppearanceScope.of(context).backdrop == BackgroundMode.texture
      ? Colors.black.withValues(alpha: .12)
      : Colors.black.withValues(alpha: .20),
  transitionDuration: motionDuration(context, 260),
  pageBuilder: (context, _, _) => builder(context),
  transitionBuilder: (context, animation, _, child) => FadeTransition(
    opacity: animation,
    child: ScaleTransition(
      scale: Tween<double>(
        begin: .96,
        end: 1,
      ).animate(CurvedAnimation(parent: animation, curve: Curves.easeOutCubic)),
      child: child,
    ),
  ),
);

class SoftSwap extends StatelessWidget {
  const SoftSwap({super.key, required this.child});
  final Widget child;
  @override
  Widget build(BuildContext context) => AnimatedSwitcher(
    duration: motionDuration(context, 220),
    switchInCurve: Curves.easeOutCubic,
    layoutBuilder: (current, previous) => Stack(
      alignment: Alignment.center,
      children: [
        ...previous.map(
          (child) => IgnorePointer(child: ExcludeSemantics(child: child)),
        ),
        ?current,
      ],
    ),
    transitionBuilder: (child, animation) => FadeTransition(
      opacity: animation,
      child: ScaleTransition(
        scale: Tween<double>(begin: .9, end: 1).animate(animation),
        child: child,
      ),
    ),
    child: child,
  );
}

class SoftSize extends StatelessWidget {
  const SoftSize({
    super.key,
    required this.child,
    required this.duration,
    this.alignment = Alignment.center,
  });
  final Widget child;
  final Duration duration;
  final Alignment alignment;
  @override
  Widget build(BuildContext context) => MediaQuery.disableAnimationsOf(context)
      ? child
      : AnimatedSize(duration: duration, alignment: alignment, child: child);
}
