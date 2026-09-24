import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:morrow_i18n/morrow_i18n.dart';

import 'session_coordinator.dart';

/// Closing has no recovery actions: the original host remains the library owner
/// until its process exit is observed.
class WorkbenchShutdown extends StatefulWidget {
  const WorkbenchShutdown({
    super.key,
    required this.session,
    required this.onBackground,
    this.locale,
    this.windowError,
  });

  final SessionCoordinator session;
  final Future<void> Function() onBackground;
  final Locale? locale;
  final Object? windowError;

  @override
  State<WorkbenchShutdown> createState() => _WorkbenchShutdownState();
}

class _WorkbenchShutdownState extends State<WorkbenchShutdown> {
  @override
  void initState() {
    super.initState();
    widget.session.addListener(_changed);
  }

  @override
  void didUpdateWidget(WorkbenchShutdown oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.session != widget.session) {
      oldWidget.session.removeListener(_changed);
      widget.session.addListener(_changed);
    }
  }

  void _changed() {
    if (mounted) setState(() {});
  }

  @override
  void dispose() {
    widget.session.removeListener(_changed);
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final colors = ColorScheme.fromSeed(seedColor: const Color(0xff7468bc));
    return MaterialApp(
      locale: widget.locale,
      supportedLocales: L10n.supportedLocales,
      localizationsDelegates: const [
        L10n.delegate,
        GlobalMaterialLocalizations.delegate,
        GlobalWidgetsLocalizations.delegate,
        GlobalCupertinoLocalizations.delegate,
      ],
      theme: ThemeData(
        useMaterial3: true,
        colorScheme: colors,
        scaffoldBackgroundColor: const Color(0xfff5f4fa),
      ),
      home: Builder(
        builder: (context) {
          final l = L10n.of(context);
          final delayed =
              widget.session.phase == SessionPhase.closingUnconfirmed;
          final failure = widget.windowError ?? widget.session.closeError;
          return Scaffold(
            body: Center(
              child: SingleChildScrollView(
                padding: const EdgeInsets.all(24),
                child: ConstrainedBox(
                  constraints: const BoxConstraints(maxWidth: 440),
                  child: DecoratedBox(
                    decoration: BoxDecoration(
                      color: Colors.white.withValues(alpha: .88),
                      borderRadius: BorderRadius.circular(28),
                      border: Border.all(color: Colors.white),
                    ),
                    child: Padding(
                      padding: const EdgeInsets.all(28),
                      child: Column(
                        mainAxisSize: MainAxisSize.min,
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Icon(
                            Icons.power_settings_new_rounded,
                            size: 36,
                            color: colors.primary,
                          ),
                          const SizedBox(height: 16),
                          Text(
                            l.shutdownTitle,
                            style: const TextStyle(
                              fontSize: 24,
                              fontWeight: FontWeight.w600,
                            ),
                          ),
                          const SizedBox(height: 12),
                          Text(
                            failure != null
                                ? l.shutdownFailure
                                : delayed
                                ? l.shutdownStillRunning
                                : l.shutdownWaiting,
                            style: const TextStyle(height: 1.6),
                          ),
                          if (failure != null) ...[
                            const SizedBox(height: 12),
                            SelectableText(
                              failure.toString(),
                              style: TextStyle(color: colors.error),
                            ),
                          ],
                          const SizedBox(height: 20),
                          Text(
                            l.shutdownBackgroundHint,
                            style: const TextStyle(fontSize: 12, height: 1.5),
                          ),
                          const SizedBox(height: 20),
                          FilledButton.tonalIcon(
                            key: const ValueKey('shutdown-background'),
                            onPressed: widget.onBackground,
                            icon: const Icon(Icons.minimize_rounded),
                            label: Text(l.shutdownBackground),
                          ),
                        ],
                      ),
                    ),
                  ),
                ),
              ),
            ),
          );
        },
      ),
    );
  }
}
