import 'package:file_selector/file_selector.dart';
import 'plugins/workbench_recovery.dart';
import 'plugins/workbench_shutdown.dart';
import 'plugins/application_shutdown.dart';
import 'plugins/workbench_self_check.dart';
import 'plugins/canvas_self_check.dart';
import 'dart:io';
import 'plugins/startup_probe.dart';
import 'plugins/workbench_startup.dart';
import 'package:flutter/material.dart';
import 'package:morrow_i18n/morrow_i18n.dart';
import 'package:path_provider/path_provider.dart';
import 'main.dart' show MorrowApp;
import 'desktop_frame.dart';
import 'window_effects.dart';
import 'plugins/workbench_native.dart';
import 'plugins/studio_storage.dart';
import 'dart:async';
import 'plugins/session_coordinator.dart';
import 'package:window_manager/window_manager.dart';

final _sessions = SessionCoordinator();
Future<void>? _desktopInitialization;
final _applicationClose = _ApplicationClose();

class _ApplicationClose with WindowListener {
  late final ApplicationShutdown shutdown = ApplicationShutdown(
    session: _sessions,
    showClosing: () => runApp(
      WorkbenchShutdown(
        session: _sessions,
        locale: WidgetsBinding.instance.platformDispatcher.locale,
        onBackground: shutdown.continueInBackground,
        windowError: shutdown.windowError,
      ),
    ),
    hideWindow: windowManager.hide,
    showWindow: () async {
      await windowManager.show();
      await windowManager.focus();
    },
    destroyWindow: windowManager.destroy,
  );

  bool get requested => shutdown.requested;
  void observe() => shutdown.observe();

  @override
  void onWindowClose() => shutdown.request();
}

Future<void> _initializeApplicationWindow() async {
  await initializeDesktopFrame();
  if (isWindowsDesktop) {
    await windowManager.setPreventClose(true);
    windowManager.addListener(_applicationClose);
    _sessions.addListener(_applicationClose.observe);
  }
}

Future<void> main(List<String> arguments) async {
  WidgetsFlutterBinding.ensureInitialized();
  _desktopInitialization ??= _initializeApplicationWindow();
  await _sessions.run(() => _startSession(arguments));
}

Future<void> _startSession(List<String> arguments) async {
  final startupCheck = arguments
      .where((v) => v.startsWith('--startup-check='))
      .firstOrNull
      ?.substring('--startup-check='.length);
  final startup = StartupProbe(startupCheck);
  WidgetsFlutterBinding.ensureInitialized();
  startup.attach();
  startup.mark('binding');
  final canvasCheck = arguments
      .where((v) => v.startsWith('--canvas-check='))
      .firstOrNull;
  if (canvasCheck != null) {
    await _desktopInitialization;
    await qualifyCanvas(canvasCheck.substring('--canvas-check='.length));
    return;
  }
  final managed =
      !arguments.any((v) => v.startsWith('--data-directory=')) ||
      arguments.contains('--managed-library');
  final requestedLocale = arguments
      .where((v) => v.startsWith('--locale='))
      .firstOrNull
      ?.substring('--locale='.length);
  final previewLocale = L10n.isSupportedCode(requestedLocale)
      ? Locale(requestedLocale!)
      : null;
  final startupLocale =
      previewLocale ?? WidgetsBinding.instance.platformDispatcher.locale;
  final startupMessages = L10n.forLocale(startupLocale);
  final workbenchReady = ValueNotifier(false);
  runApp(WorkbenchStartup(ready: workbenchReady, locale: previewLocale));
  Directory? recoveryDirectory;
  RustWorkbench? opened;
  final executable = File(Platform.resolvedExecutable).parent.path;
  try {
    final check = arguments
        .where((v) => v.startsWith('--self-check='))
        .firstOrNull;
    final errors = <String>[];
    if (check != null || startupCheck != null) {
      final original = FlutterError.onError;
      FlutterError.onError = (details) {
        errors.add(details.exceptionAsString());
        original?.call(details);
      };
    }
    final selected = arguments
        .where((v) => v.startsWith('--data-directory='))
        .firstOrNull;
    if (startupCheck != null && selected == null) {
      throw StateError(
        'Startup qualification requires an explicit data directory',
      );
    }
    final directory = Directory(
      selected == null
          ? '${(await getApplicationSupportDirectory()).path}/rust-workbench'
          : selected.substring('--data-directory='.length),
    );
    recoveryDirectory = directory;
    if (_applicationClose.requested) return;
    if (await File('${directory.path}/MIGRATION_INCOMPLETE.txt').exists()) {
      throw StateError(startupMessages.recoveryMigrationIncomplete);
    }
    if (check != null &&
        (selected == null ||
            await File('${directory.path}/workbench.db').exists())) {
      throw StateError(
        'Qualification requires an explicit fresh data directory',
      );
    }
    // Both branches must finish before failure cleanup, so an open host never
    // escapes if desktop initialization fails. Neither branch needs the other.
    await Future.wait<void>([
      _desktopInitialization!.then((_) => startup.mark('desktop')),
      RustWorkbench.open(
        executable: '$executable/morrow-workbench-host.exe',
        package: '$executable/plugins/workbench.morrowplugin',
        directory: directory,
        managed: managed,
        onStarted: (backend) {
          opened = backend;
          _sessions.attach(
            process: backend.process,
            library: directory.absolute.path,
            exited: backend.process.exitCode,
            close: backend.close,
          );
        },
      ).then((backend) {
        opened = backend;
        startup.mark('host');
      }),
    ]);
    final backend = opened!;
    if (_applicationClose.requested) {
      _applicationClose.observe();
      return;
    }
    if (check != null) await seedQualification(backend, directory);
    if (_applicationClose.requested) {
      _applicationClose.observe();
      return;
    }
    final storage = await RustStudioStorage.open(backend);
    if (_applicationClose.requested) {
      _applicationClose.observe();
      return;
    }
    _sessions.active();
    startup.mark('storage');
    final boundary = GlobalKey();
    startup.mark('runApp');
    runApp(
      WorkbenchStartup(
        ready: workbenchReady,
        locale: previewLocale,
        onRevealed: startup.revealed,
        child: RepaintBoundary(
          key: boundary,
          child: MorrowApp(
            libraryDirectory: directory.path,
            storage: storage,
            onFirstFrame: () {
              startup.workbenchPainted();
              workbenchReady.value = true;
            },
            initialLocale: previewLocale,
            workbench: backend,
            nativeBackground: DesktopBackground(),
            initialWarning:
                backend.maintenanceWarning ??
                (backend.writable ? null : '工作台插件不可用，已有内容仍可查看和导出。'),
          ),
        ),
      ),
    );
    if (startupCheck != null) {
      await startup.finish();
      if (errors.isNotEmpty) {
        throw StateError('Startup rendering failed: ${errors.join('\n')}');
      }
      await backend.close();
      exit(0);
    }
    if (check != null) {
      WidgetsBinding.instance.addPostFrameCallback((_) {
        finishQualification(
          boundary,
          backend,
          storage,
          check.substring('--self-check='.length),
          errors,
        );
      });
    }
  } catch (error, stack) {
    // Render recovery immediately. Waiting for a worker is owned and observed
    // outside the ordinary RPC lane, and does not authorize another writer.
    unawaited(_sessions.close().catchError((Object _) {}));
    if (_applicationClose.requested) {
      FlutterError.reportError(FlutterErrorDetails(
        exception: error,
        stack: stack,
        context: ErrorDescription('while closing the workspace'),
      ));
      _applicationClose.observe();
      return;
    }
    final check = arguments
        .where((v) => v.startsWith('--self-check='))
        .firstOrNull;
    if (check != null) {
      await File(
        '${check.substring('--self-check='.length)}.md',
      ).writeAsString('Startup failed: $error\n$stack');
      exit(1);
    }
    if (startupCheck != null) {
      await startup.fail(error);
      exit(1);
    }
    final targetDirectory = recoveryDirectory;
    runApp(
      WorkbenchRecovery(
        session: _sessions,
        failure: error,
        locale: previewLocale,
        onRetry: () => _sessions.run(() => _startSession(arguments)),
        onRestoreSnapshot: !managed || targetDirectory == null
            ? null
            : () async {
                if (!_sessions.mayRecover) {
                  throw StateError('Previous library owner has not exited');
                }
                final selected = await openFile(
                  acceptedTypeGroups: [
                    XTypeGroup(
                      label: startupMessages.recoveryBackupFile,
                      extensions: ['morrowbackup'],
                    ),
                  ],
                );
                if (selected == null) return;
                final parent = Directory('${targetDirectory.path}/recovered');
                await parent.create(recursive: true);
                final destination = Directory(
                  '${parent.path}/library-${DateTime.now().microsecondsSinceEpoch}',
                );
                await RustWorkbench.restoreSnapshot(
                  executable: '$executable/morrow-workbench-host.exe',
                  archive: selected.path,
                  destination: destination,
                );
                try {
                  await RustWorkbench.activateLibrary(
                    executable: '$executable/morrow-workbench-host.exe',
                    root: targetDirectory,
                    selected: destination,
                  );
                } catch (_) {
                  throw RecoverySwitchUnconfirmed(destination.path);
                }
                await _sessions.run(() => _startSession(arguments));
              },
        onRestore: targetDirectory == null
            ? null
            : () async {
                if (!_sessions.mayRecover) {
                  throw StateError('Previous library owner has not exited');
                }
                final selected = await openFile(
                  acceptedTypeGroups: [
                    XTypeGroup(
                      label: startupMessages.recoveryKeyFile,
                      extensions: ['audit-key', 'backup'],
                    ),
                    XTypeGroup(label: startupMessages.recoveryAllFiles),
                  ],
                );
                if (selected == null) return;
                await RustWorkbench.restoreKey(
                  executable: '$executable/morrow-workbench-host.exe',
                  directory: targetDirectory,
                  selected: selected.path,
                  managed: managed,
                );
                await _sessions.run(() => _startSession(arguments));
              },
      ),
    );
  }
}
