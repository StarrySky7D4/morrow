import 'dart:io';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/studio_storage.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';

void main() {
  final exe = Platform.environment['MORROW_WORKBENCH_HOST'];
  final package = Platform.environment['MORROW_WORKBENCH_PACKAGE'];
  test(
    'disabled workbench saves settings through storage and survives restart',
    () async {
      final directory = await Directory.systemTemp.createTemp(
        'morrow-disabled-prefs-',
      );
      RustWorkbench? backend;
      try {
        backend = await RustWorkbench.open(
          executable: exe!,
          package: package!,
          directory: directory,
        );
        var storage = await RustStudioStorage.open(backend);
        await backend.configurePlugin(await backend.pluginState(), false);
        expect((await backend.pluginState()).enabled, isFalse);
        await storage.write({
          ...storage.read(),
          'theme': 'dark',
          'styleDepth': 1.7,
          'visualStyle': 'brutalist',
          'uiLocale': 'de',
        });
        expect(storage.read()['theme'], 'dark');
        expect(backend.pendingPreferencesOperation, isNull);
        await backend.close();
        backend = null;
        backend = await RustWorkbench.open(
          executable: exe,
          package: package,
          directory: directory,
        );
        expect((await backend.pluginState()).enabled, isFalse);
        storage = await RustStudioStorage.open(backend);
        expect(storage.read()['theme'], 'dark');
        expect(storage.read()['styleDepth'], 1.7);
        expect(storage.read()['uiLocale'], 'de');
        await storage.write({
          ...storage.read(),
          'theme': 'white',
          'styleDepth': .3,
        });
        expect(
          decodePreferences((await backend.readPreferences())!)['theme'],
          'white',
        );
        expect(backend.pendingPreferencesOperation, isNull);
      } finally {
        await backend?.close();
        await directory.delete(recursive: true);
      }
    },
    skip: !Platform.isWindows || exe == null || package == null,
  );
}
