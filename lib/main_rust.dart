import 'package:file_selector/file_selector.dart';
import 'plugins/workbench_recovery.dart';
import 'plugins/workbench_self_check.dart';
import 'plugins/canvas_self_check.dart';
import 'dart:io';
import 'package:flutter/material.dart';
import 'package:path_provider/path_provider.dart';
import 'main.dart' show MorrowApp;
import 'desktop_frame.dart';
import 'window_effects.dart';
import 'plugins/workbench_native.dart';
import 'plugins/studio_storage.dart';

Future<void> main(List<String> arguments) async {
  WidgetsFlutterBinding.ensureInitialized();
  await initializeDesktopFrame();
  final canvasCheck = arguments
      .where((v) => v.startsWith('--canvas-check='))
      .firstOrNull;
  if (canvasCheck != null) {
    await qualifyCanvas(canvasCheck.substring('--canvas-check='.length));
    return;
  }
  final managed =
      !arguments.any((v) => v.startsWith('--data-directory=')) ||
      arguments.contains('--managed-library');
  Directory? recoveryDirectory;
  RustWorkbench? opened;
  final executable = File(Platform.resolvedExecutable).parent.path;
  try {
    final check = arguments
        .where((v) => v.startsWith('--self-check='))
        .firstOrNull;
    final errors = <String>[];
    if (check != null) {
      final original = FlutterError.onError;
      FlutterError.onError = (details) {
        errors.add(details.exceptionAsString());
        original?.call(details);
      };
    }
    final selected = arguments
        .where((v) => v.startsWith('--data-directory='))
        .firstOrNull;
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
    final backend = await RustWorkbench.open(
      executable: '$executable/morrow-workbench-host.exe',
      package: '$executable/plugins/workbench.morrowplugin',
      directory: directory,
      managed: managed,
    );
    opened = backend;
    if (check != null) await seedQualification(backend, directory);
    final storage = await RustStudioStorage.open(backend);
    final boundary = GlobalKey();
    runApp(
      RepaintBoundary(
        key: boundary,
        child: MorrowApp(
          storage: storage,
          workbench: backend,
          nativeBackground: DesktopBackground(),
          initialWarning:
              backend.maintenanceWarning ??
              (backend.writable ? null : '工作台插件不可用，已有内容仍可查看和导出。'),
        ),
      ),
    );
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
    await opened?.close();
    final check = arguments
        .where((v) => v.startsWith('--self-check='))
        .firstOrNull;
    if (check != null) {
      await File(
        '${check.substring('--self-check='.length)}.md',
      ).writeAsString('Startup failed: $error\n$stack');
      exit(1);
    }
    final targetDirectory = recoveryDirectory;
    runApp(
      WorkbenchRecovery(
        message: workbenchFailureMessage(error),
        onRetry: () => main(arguments),
        onRestoreSnapshot: !managed || targetDirectory == null
            ? null
            : () async {
                final selected = await openFile(
                  acceptedTypeGroups: const [
                    XTypeGroup(label: '内容库备份', extensions: ['morrowbackup']),
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
                  throw StateError(
                    'Morrow workbench host: 备份已恢复至 ${destination.path}，切换结果未确认，请保留此目录并重新打开工作台核对。',
                  );
                }
                await main(arguments);
              },
        onRestore: targetDirectory == null
            ? null
            : () async {
                final selected = await openFile(
                  acceptedTypeGroups: const [
                    XTypeGroup(
                      label: '内容库保护文件',
                      extensions: ['audit-key', 'backup'],
                    ),
                    XTypeGroup(label: '所有文件'),
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
