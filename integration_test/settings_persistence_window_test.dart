import 'dart:io';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:morrow_i18n/morrow_i18n.dart';
import 'package:morrow_studio/main.dart';
import 'package:morrow_studio/desktop_frame.dart';
import 'package:morrow_studio/window_effects.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';
import 'package:morrow_studio/plugins/studio_storage.dart';
import 'package:morrow_studio/plugins/plugin_library.dart';
import 'package:morrow_studio/plugins/plugin_tools.dart';
import 'package:window_manager/window_manager.dart';
import 'live_ui_helpers.dart';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  testWidgets(
    'Explicit plugin activation restores UI and content saves across restart',
    (t) async {
      final directory = await Directory.systemTemp.createTemp(
        'morrow-settings-save-',
      );
      final output = Platform.environment['MORROW_WINDOW_TEST_OUTPUT']!;
      Future<RustWorkbench> open() => RustWorkbench.open(
        executable: Platform.environment['MORROW_WORKBENCH_HOST']!,
        package: Platform.environment['MORROW_WORKBENCH_PACKAGE']!,
        directory: directory,
        managed: true,
      );
      var backend = await open();
      final boundary = GlobalKey();
      Finder keyed(String key) => find.byKey(ValueKey(key));
      try {
        await initializeDesktopFrame();
        await windowManager.setSize(const Size(1280, 900));
        final state = await backend.pluginState();
        await backend.configurePlugin(state, false);
        final storage = await RustStudioStorage.open(backend);
        await t.pumpWidget(
          RepaintBoundary(
            key: boundary,
            child: MorrowApp(
              initialLocale: const Locale('en'),
              storage: storage,
              workbench: backend,
              nativeBackground: DesktopBackground(),
            ),
          ),
        );
        await waitForUi(
          t,
          () => keyed('readonly-plugin-settings').evaluate().isNotEmpty,
          reason: 'actionable read-only state',
        );
        expect(find.byType(PluginTools), findsNothing);
        expect(find.byType(PluginLibrary), findsNothing);
        await tapVisible(t, keyed('readonly-plugin-settings'));
        await waitForUi(
          t,
          () => keyed('plugin-enable').evaluate().isNotEmpty,
          reason: 'builtin approval in dedicated plugin page',
        );
        expect(find.byType(PluginTools), findsOneWidget);
        expect(find.byType(PluginLibrary), findsOneWidget);
        await saveBoundaryPng(
          t,
          boundary,
          '$output/01-plugin-settings-wide.png',
        );
        await tapVisible(t, keyed('plugin-enable'));
        await waitForUi(
          t,
          () => backend.writable,
          reason: 'explicit activation restores writes',
        );
        await tapVisible(t, keyed('io-settings-open'));
        await saveBoundaryPng(t, boundary, '$output/02-io-settings-wide.png');
        await windowManager.setSize(const Size(390, 820));
        await saveBoundaryPng(t, boundary, '$output/03-io-settings-narrow.png');
        expect(find.byType(Scrollbar), findsNothing);
        await leaveIoSettings(t);
        expect(keyed('readonly-plugin-settings'), findsNothing);
        final en = L10n.forLocale(const Locale('en'));
        await tapVisible(t, find.text(en.mainNewIdea));
        await waitForUi(
          t,
          () => keyed('idea-title').evaluate().isNotEmpty,
          reason: 'real guest editor opens',
        );
        await t.enterText(keyed('idea-title'), 'Settings recovery card');
        await t.enterText(
          keyed('idea-description'),
          'Durable content after explicit plugin activation.',
        );
        await tapVisible(t, keyed('idea-save'));
        await waitForUi(
          t,
          () => find.byType(NewIdeaDialog).evaluate().isEmpty,
          reason: 'card committed through Rust',
        );
        final saved = (await backend.load()).singleWhere(
          (i) => i.title == 'Settings recovery card',
        );
        await tapVisible(t, keyed('appearance-toggle'));
        await tapVisible(t, keyed('theme-dark'));
        await waitForUi(
          t,
          () => storage.read()['theme'] == 'dark',
          reason: 'appearance save confirmed',
        );
        await tapVisible(t, keyed('component-settings'));
        await tapVisible(t, keyed('component-entry:hero'));
        await tapVisible(t, keyed('component-custom-toggle'));
        await tapVisible(t, keyed('component-mode-liquid'));
        t.widget<Slider>(keyed('component-radius')).onChanged!(12);
        t.widget<Slider>(keyed('component-opacity')).onChanged!(.25);
        await t.pump();
        await windowManager.setSize(const Size(1280, 900));
        await saveBoundaryPng(t, boundary, '$output/04-component-editor.png');
        await tapVisible(t, keyed('component-apply'));
        await waitForUi(
          t,
          () =>
              (storage.read()['componentMaterials']
                  as Map?)?['hero']?['mode'] ==
              'liquid',
          reason: 'component material persisted',
        );
        expect(find.byType(SnackBar), findsNothing);
        await t.pumpWidget(const SizedBox());
        await backend.close();
        backend = await open();
        final restored = await RustStudioStorage.open(backend);
        expect(restored.read()['theme'], 'dark');
        expect(
          (restored.read()['componentMaterials']
              as Map)['hero']['cornerRadius'],
          12,
        );
        expect(
          (restored.read()['componentMaterials'] as Map)['hero']['mode'],
          'liquid',
        );
        expect(
          (await backend.load())
              .singleWhere((i) => i.id == saved.id)
              .description,
          'Durable content after explicit plugin activation.',
        );
        expect(backend.writable, isTrue);
        expect(t.takeException(), isNull);
      } finally {
        await t.pumpWidget(const SizedBox());
        await backend.close();
        await directory.delete(recursive: true);
      }
    },
    timeout: const Timeout(Duration(minutes: 3)),
  );
}
