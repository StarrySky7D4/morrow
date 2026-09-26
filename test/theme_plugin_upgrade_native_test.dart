import 'dart:io';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';
import 'package:morrow_studio/theme_plugins/theme_plugin_controller.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  final executable = Platform.environment['MORROW_WORKBENCH_HOST'];
  final bundle = Platform.environment['MORROW_WORKBENCH_PACKAGE'];
  final nextPackage = Platform.environment['MORROW_THEME_PACKAGE'];
  test(
    'released theme upgrades to rebuilt package and survives restart',
    () async {
      final directory = await Directory.systemTemp.createTemp(
        'morrow-theme-upgrade-',
      );
      var host = await RustWorkbench.open(
        executable: executable!,
        package: bundle!,
        directory: directory,
      );
      final store = MemoryThemePluginStore();
      var controller = ThemePluginController(store: store, backend: host);
      try {
        final businessBefore = await host.pluginState();
        final oldPath = File(
          'test/fixtures/plugins/morrow-mid-autumn-1.0.0.morrowplugin',
        ).absolute.path;
        final oldPreview = await host.inspectPlugin(oldPath);
        final old = oldPreview.entries.single;
        expect(old.version, '1.0.0');
        await host.importPlugin(oldPath, old.digest, oldPreview.revision);
        await controller.restore();
        expect(
          await controller.activate(old.id),
          isTrue,
          reason: '${controller.error}',
        );
        final preview = await host.inspectPlugin(nextPackage!);
        final next = preview.entries.single;
        expect(next.id, old.id);
        expect(next.version, '1.0.1');
        expect(next.digest, isNot(orderedEquals(old.digest)));
        expect(next.declared, isEmpty);
        expect(next.declaredIo, isEmpty);
        await host.importPlugin(nextPackage, next.digest, preview.revision);
        await controller.refresh();
        expect(controller.entries.single.version, next.version);
        expect(
          await controller.activate(next.id),
          isTrue,
          reason: '${controller.error}',
        );
        expect(controller.plugin!.artwork, isNotEmpty);
        controller.dispose();
        await host.close();
        host = await RustWorkbench.open(
          executable: executable,
          package: bundle,
          directory: directory,
        );
        controller = ThemePluginController(store: store, backend: host);
        await controller.restore();
        expect(controller.active, isTrue, reason: '${controller.error}');
        expect(controller.entries.single.version, '1.0.1');
        expect(controller.entries.single.digest, orderedEquals(next.digest));
        expect(controller.plugin!.artwork, isNotEmpty);
        final businessAfter = await host.pluginState();
        expect(businessAfter.digest, businessBefore.digest);
        expect(businessAfter.approved, businessBefore.approved);
        expect(businessAfter.enabled, businessBefore.enabled);
        await controller.deactivate();
        final page = await host.pluginPage();
        await host.removeExternal(controller.entries.single, page.revision);
        await controller.refresh();
        expect(controller.entries, isEmpty);
        expect(host.writable, isTrue);
      } finally {
        controller.dispose();
        await host.close();
        await directory.delete(recursive: true);
      }
    },
    skip: executable == null || bundle == null || nextPackage == null,
  );
}
