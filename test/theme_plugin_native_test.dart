import 'dart:io';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';
import 'package:morrow_studio/theme_plugins/theme_plugin_controller.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  final executable = Platform.environment['MORROW_WORKBENCH_HOST'];
  final bundle = Platform.environment['MORROW_WORKBENCH_PACKAGE'];
  final themePackage = Platform.environment['MORROW_THEME_PACKAGE'];
  test(
    'real standalone package imports disabled, renders, restarts and uninstalls independently',
    () async {
      final directory = await Directory.systemTemp.createTemp(
        'morrow-theme-package-',
      );
      var host = await RustWorkbench.open(
        executable: executable!,
        package: bundle!,
        directory: directory,
      );
      final store = MemoryThemePluginStore();
      var c = ThemePluginController(store: store, backend: host);
      try {
        await c.restore();
        expect(c.entries, isEmpty);
        final before = await host.pluginState();
        final preview = await host.inspectPlugin(themePackage!);
        final entry = preview.entries.single;
        expect(entry.isTheme, isTrue);
        expect(entry.declared, isEmpty);
        expect(entry.declaredIo, isEmpty);
        await host.importPlugin(themePackage, entry.digest, preview.revision);
        await c.refresh();
        expect(c.active, isFalse);
        expect(await c.activate(entry.id), isTrue, reason: '${c.error}');
        expect(c.plugin!.artwork, isNotEmpty);
        expect(c.plugin!.caption(const Locale('zh')), contains('桂香'));
        expect(host.writable, isTrue);
        final after = await host.pluginState();
        expect(after.digest, before.digest);
        expect(after.approved, before.approved);
        expect(after.enabled, before.enabled);
        expect(
          (await host.studio.parseLyrics(
            '[00:01.2]test',
          )).single.time.inMilliseconds,
          1200,
        );
        c.dispose();
        await host.close();
        host = await RustWorkbench.open(
          executable: executable,
          package: bundle,
          directory: directory,
        );
        c = ThemePluginController(store: store, backend: host);
        await c.restore();
        expect(c.active, isTrue, reason: '${c.error}');
        await c.deactivate();
        expect(c.active, isFalse);
        final page = await host.pluginPage();
        final installed = c.entries.single;
        await host.removeExternal(installed, page.revision);
        await c.refresh();
        expect(c.entries, isEmpty);
        expect(host.writable, isTrue);
      } finally {
        c.dispose();
        await host.close();
        await directory.delete(recursive: true);
      }
    },
    skip: executable == null || bundle == null || themePackage == null,
  );
}
