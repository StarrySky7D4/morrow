import 'package:flutter/material.dart';
import 'host_notices.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:morrow_i18n/morrow_i18n.dart';

String workbenchFailureMessage(
  Object error, {
  String? fallback,
  AppLocalizations? localization,
}) {
  final l = localization ?? L10n.forLocale(const Locale('zh'));
  if (error is RecoverySwitchUnconfirmed) {
    return l.recoverySwitchUnconfirmed(error.path);
  }
  const prefix = 'Morrow workbench host: ';
  final detail = error is StateError ? error.message.toString() : '';
  final plain = detail.startsWith(prefix)
      ? detail.substring(prefix.length)
      : detail;
  return hostStorageNotice(l, plain) ??
      (detail.startsWith(prefix) ? plain : fallback ?? l.recoveryOpenFailed);
}

final class RecoverySwitchUnconfirmed implements Exception {
  const RecoverySwitchUnconfirmed(this.path);
  final String path;
}

/// Recovery actions are host supplied; this view never reads protection files.
class WorkbenchRecovery extends StatefulWidget {
  const WorkbenchRecovery({
    super.key,
    this.message,
    this.failure,
    this.locale,
    required this.onRetry,
    this.onRestore,
    this.onRestoreSnapshot,
  });
  final String? message;
  final Object? failure;
  final Locale? locale;
  final Future<void> Function() onRetry;
  final Future<void> Function()? onRestore;
  final Future<void> Function()? onRestoreSnapshot;
  @override
  State<WorkbenchRecovery> createState() => _WorkbenchRecoveryState();
}

class _WorkbenchRecoveryState extends State<WorkbenchRecovery> {
  bool _busy = false;
  Object? _error;
  Future<void> _run(Future<void> Function() action) async {
    if (_busy) return;
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      await action();
    } catch (error) {
      if (mounted) {
        setState(() {
          _error = error;
        });
      }
    } finally {
      if (mounted) setState(() => _busy = false);
    }
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
                      boxShadow: [
                        BoxShadow(
                          color: colors.primary.withValues(alpha: .08),
                          blurRadius: 30,
                          offset: const Offset(0, 10),
                        ),
                      ],
                    ),
                    child: Padding(
                      padding: const EdgeInsets.all(28),
                      child: Column(
                        mainAxisSize: MainAxisSize.min,
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Icon(
                            Icons.lock_reset_rounded,
                            size: 36,
                            color: colors.primary,
                          ),
                          const SizedBox(height: 16),
                          Text(
                            l.recoveryTitle,
                            style: TextStyle(
                              fontSize: 24,
                              fontWeight: FontWeight.w600,
                            ),
                          ),
                          const SizedBox(height: 12),
                          Text(
                            widget.message ??
                                workbenchFailureMessage(
                                  widget.failure ?? StateError('startup'),
                                  localization: l,
                                ),
                            style: const TextStyle(height: 1.6),
                          ),
                          if (widget.onRestoreSnapshot != null) ...[
                            const SizedBox(height: 12),
                            Text(
                              l.recoverySnapshotGuide,
                              style: TextStyle(fontSize: 12, height: 1.6),
                            ),
                            const SizedBox(height: 10),
                            OutlinedButton.icon(
                              key: const ValueKey('restore-library-snapshot'),
                              onPressed: _busy
                                  ? null
                                  : () => _run(widget.onRestoreSnapshot!),
                              icon: const Icon(
                                Icons.inventory_2_outlined,
                                size: 18,
                              ),
                              label: Text(l.recoverySnapshot),
                            ),
                          ],
                          if (widget.onRestore != null) ...[
                            const SizedBox(height: 12),
                            Text(
                              l.recoveryKeyGuide,
                              style: TextStyle(
                                height: 1.6,
                                color: Color(0xff686578),
                              ),
                            ),
                          ],
                          if (_error != null) ...[
                            const SizedBox(height: 16),
                            Text(
                              workbenchFailureMessage(
                                _error!,
                                fallback: l.recoveryFailed,
                                localization: l,
                              ),
                              style: TextStyle(
                                color: colors.error,
                                height: 1.5,
                              ),
                            ),
                          ],
                          const SizedBox(height: 24),
                          if (_busy)
                            const Padding(
                              padding: EdgeInsets.only(bottom: 16),
                              child: LinearProgressIndicator(),
                            ),
                          Wrap(
                            spacing: 12,
                            runSpacing: 12,
                            children: [
                              FilledButton.tonal(
                                onPressed: _busy
                                    ? null
                                    : () => _run(widget.onRetry),
                                child: Text(l.recoveryRetry),
                              ),
                              if (widget.onRestore != null)
                                OutlinedButton.icon(
                                  onPressed: _busy
                                      ? null
                                      : () => _run(widget.onRestore!),
                                  icon: const Icon(Icons.key_rounded, size: 18),
                                  label: Text(l.recoveryChooseKey),
                                ),
                            ],
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
