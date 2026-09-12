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
    final executable = File(Platform.resolvedExecutable).parent.path;
    final selected = arguments
        .where((v) => v.startsWith('--data-directory='))
        .firstOrNull;
    final directory = Directory(
      selected == null
          ? '${(await getApplicationSupportDirectory()).path}/rust-workbench'
          : selected.substring('--data-directory='.length),
    );
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
    );
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
          initialWarning: backend.writable ? null : '工作台插件不可用，已有内容仍可查看和导出。',
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
    final check = arguments
        .where((v) => v.startsWith('--self-check='))
        .firstOrNull;
    if (check != null) {
      await File(
        '${check.substring('--self-check='.length)}.md',
      ).writeAsString('Startup failed: $error\n$stack');
      exit(1);
    }
    runApp(
      MaterialApp(
        home: Scaffold(
          body: Center(
            child: ConstrainedBox(
              constraints: const BoxConstraints(maxWidth: 400),
              child: const Padding(
                padding: EdgeInsets.all(24),
                child: Text('工作台暂时无法打开。请检查插件文件与数据目录，然后重新启动。原有资料未被覆盖。'),
              ),
            ),
          ),
        ),
      ),
    );
  }
}
