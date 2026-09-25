import 'dart:async';
import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:morrow_i18n/morrow_i18n.dart';
import 'package:shared_preferences/shared_preferences.dart';
import 'main.dart' show MorrowApp;
import 'storage.dart';
import 'window_effects.dart';
import 'plugins/device_library_web.dart';
import 'plugins/studio_storage.dart';
import 'plugins/workbench_channel_web.dart';
import 'plugins/workbench_loading.dart';
import 'plugins/workbench_native.dart';
import 'plugins/workbench_startup.dart';

/// The default Web application uses the same workbench as Windows. A legacy
/// snapshot remains independently accessible until its explicit migration exists.
class BrowserWorkspace extends StatefulWidget {
  const BrowserWorkspace({super.key});
  @override
  State<BrowserWorkspace> createState() => _BrowserWorkspaceState();
}

class _BrowserWorkspaceState extends State<BrowserWorkspace> {
  final _ready = ValueNotifier(false);
  SharedPreferences? _preferences;
  RustWorkbench? _host;
  RustStudioStorage? _storage;
  bool _busy = false, _legacy = false, _legacyExists = false;
  DeviceLibraryState? _state;
  Object? _failure;
  @override
  void initState() {
    super.initState();
    unawaited(_inspect());
  }

  Future<void> _inspect() async {
    if (_busy) return;
    setState(() {
      _busy = true;
      _failure = null;
      _state = null;
    });
    try {
      final preferences = await SharedPreferences.getInstance();
      await preferences.reload();
      _preferences = preferences;
      _legacyExists = preferences.containsKey(LocalStorage.key);
      final state = await inspectDeviceLibrary();
      if (!mounted) return;
      _state = state;
      if (!_legacyExists &&
          (state == DeviceLibraryState.ready ||
              state == DeviceLibraryState.prepared)) {
        await _open(create: false);
      }
    } catch (error) {
      _failure = error;
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _open({required bool create}) async {
    RustWorkbench? opened;
    try {
      if (create) {
        await _preferences!.reload();
        if (_preferences!.containsKey(LocalStorage.key)) {
          throw StateError('Existing browser content requires migration');
        }
      }
      final channel = await BrowserWorkbenchChannel.open(
        name: 'main',
        create: create,
      );
      opened = await RustWorkbench.connect(channel);
      final storage = await RustStudioStorage.open(opened);
      if (!mounted) {
        await opened.close();
        return;
      }
      _host = opened;
      _storage = storage;
    } catch (error, stack) {
      try {
        await opened?.close();
      } catch (_) {}
      Error.throwWithStackTrace(error, stack);
    }
  }

  Future<void> _startSelected({required bool create}) async {
    if (_busy) return;
    setState(() {
      _busy = true;
      _failure = null;
    });
    try {
      await _open(create: create);
    } catch (error) {
      _failure = error;
      _state = null;
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  void _openLegacy() {
    try {
      // Validate before opening: corrupt legacy JSON is not an empty snapshot.
      LocalStorage(_preferences!).read();
      setState(() => _legacy = true);
    } catch (error) {
      setState(() => _failure = error);
    }
  }

  @override
  void dispose() {
    _ready.dispose();
    unawaited(
      _host?.close().catchError((Object error, StackTrace stack) {
        FlutterError.reportError(
          FlutterErrorDetails(exception: error, stack: stack),
        );
      }),
    );
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    if (_legacy) {
      return MorrowApp(
        storage: LocalStorage(_preferences!),
        nativeBackground: DesktopBackground(),
      );
    }
    if (_host case final host?) {
      return WorkbenchStartup(
        ready: _ready,
        child: MorrowApp(
          storage: _storage,
          workbench: host,
          nativeBackground: DesktopBackground(),
          onFirstFrame: () {
            if (mounted) _ready.value = true;
          },
          initialWarning: host.maintenanceWarning,
        ),
      );
    }
    if (_busy) return const WorkbenchLoading();
    return MaterialApp(
      debugShowCheckedModeBanner: false,
      supportedLocales: L10n.supportedLocales,
      localizationsDelegates: const [
        L10n.delegate,
        GlobalMaterialLocalizations.delegate,
        GlobalWidgetsLocalizations.delegate,
        GlobalCupertinoLocalizations.delegate,
      ],
      theme: ThemeData(
        useMaterial3: true,
        colorScheme: ColorScheme.fromSeed(seedColor: const Color(0xff7468bc)),
      ),
      home: Builder(
        builder: (context) {
          final l = L10n.of(context);
          final canCreate =
              _state == DeviceLibraryState.empty &&
              !_legacyExists &&
              _failure == null;
          final canOpen =
              _failure == null &&
              (_state == DeviceLibraryState.ready ||
                  _state == DeviceLibraryState.prepared);
          return Scaffold(
            body: Center(
              child: SingleChildScrollView(
                padding: const EdgeInsets.all(24),
                child: ConstrainedBox(
                  constraints: const BoxConstraints(maxWidth: 440),
                  child: Card(
                    child: Padding(
                      padding: const EdgeInsets.all(28),
                      child: Column(
                        mainAxisSize: MainAxisSize.min,
                        crossAxisAlignment: CrossAxisAlignment.stretch,
                        children: [
                          Text(
                            'Morrow',
                            style: Theme.of(context).textTheme.headlineMedium,
                          ),
                          const SizedBox(height: 20),
                          Text(
                            canCreate
                                ? l.recoveryWebLocal
                                : _legacyExists && _failure == null
                                ? l.recoveryWebLegacy
                                : l.recoveryWebUnavailable,
                          ),
                          const SizedBox(height: 24),
                          if (canCreate)
                            FilledButton(
                              onPressed: () => _startSelected(create: true),
                              child: Text(l.recoveryWebCreate),
                            ),
                          if (canOpen)
                            FilledButton(
                              onPressed: () => _startSelected(create: false),
                              child: Text(l.recoveryTitle),
                            ),
                          if (_legacyExists)
                            FilledButton(
                              onPressed: _openLegacy,
                              child: Text(l.recoveryWebOpenLegacy),
                            ),
                          TextButton(
                            onPressed: _inspect,
                            child: Text(l.recoveryRetry),
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
