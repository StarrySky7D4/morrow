import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:morrow_i18n/morrow_i18n.dart';
import '../appearance.dart';
import '../desktop_frame.dart';

/// A responsive window while the authoritative library is opening. This view
/// owns no storage, editable content, plugin calls or automatic save path.
class WorkbenchLoading extends StatelessWidget {
  const WorkbenchLoading({super.key, this.locale});
  final Locale? locale;

  @override
  Widget build(BuildContext context) {
    final dark =
        (MediaQuery.maybePlatformBrightnessOf(context) ??
            WidgetsBinding.instance.platformDispatcher.platformBrightness) ==
        Brightness.dark;
    final palette = Palette(
      dark ? StudioTheme.dark : StudioTheme.white,
      GlassMode.frosted,
    );
    return MaterialApp(
      debugShowCheckedModeBanner: false,
      locale: locale,
      supportedLocales: L10n.supportedLocales,
      localizationsDelegates: const [
        L10n.delegate,
        GlobalMaterialLocalizations.delegate,
        GlobalWidgetsLocalizations.delegate,
        GlobalCupertinoLocalizations.delegate,
      ],
      theme: ThemeData(
        brightness: dark ? Brightness.dark : Brightness.light,
        fontFamily: 'Segoe UI',
        colorSchemeSeed: palette.accent,
        scaffoldBackgroundColor: palette.background,
      ),
      builder: (context, child) => AppearanceScope(
        palette: palette,
        child: isWindowsDesktop
            ? DesktopFrame(palette: palette, child: child!)
            : child!,
      ),
      home: Builder(
        builder: (context) => Scaffold(
          key: const ValueKey('startup-loading'),
          body: Center(
            child: ConstrainedBox(
              constraints: const BoxConstraints(maxWidth: 320),
              child: Glass(
                p: palette,
                child: Padding(
                  padding: const EdgeInsets.all(24),
                  child: Column(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      Icon(
                        Icons.all_inclusive_rounded,
                        color: palette.accent,
                        size: 40,
                      ),
                      const SizedBox(height: 16),
                      Text(
                        'Morrow',
                        style: TextStyle(
                          color: palette.ink,
                          fontSize: 24,
                          fontWeight: FontWeight.w600,
                        ),
                      ),
                      const SizedBox(height: 24),
                      SizedBox(
                        width: 180,
                        child: LinearProgressIndicator(
                          semanticsLabel: L10n.of(context).visualLoading,
                          minHeight: 2,
                        ),
                      ),
                      const SizedBox(height: 12),
                      Text(
                        L10n.of(context).visualLoading,
                        style: TextStyle(color: palette.muted),
                      ),
                    ],
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
