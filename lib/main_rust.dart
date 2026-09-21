import 'package:file_selector/file_selector.dart';
import 'plugins/workbench_recovery.dart';
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

Future<void> main(List<String> arguments) async {
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
    await initializeDesktopFrame();
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
      initializeDesktopFrame().then((_) => startup.mark('desktop')),
      RustWorkbench.open(
        executable: '$executable/morrow-workbench-host.exe',
        package: '$executable/plugins/workbench.morrowplugin',
        directory: directory,
        managed: managed,
      ).then((backend) {
        opened = backend;
        startup.mark('host');
      }),
    ]);
    final backend = opened!;
    if (check != null) await seedQualification(backend, directory);
    final storage = await RustStudioStorage.open(backend);
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
    try {
      await opened?.close();
    } catch (_) {
      // Keep the original library/preference failure actionable in recovery.
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
        failure: error,
        locale: previewLocale,
        onRetry: () => main(arguments),
        onRestoreSnapshot: !managed || targetDirectory == null
            ? null
            : () async {
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
                await main(arguments);
              },
        onRestore: targetDirectory == null
            ? null
            : () async {
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
                await main(arguments);
              },
      ),
    );
  }
}
