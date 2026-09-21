import 'dart:io';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:morrow_studio/main.dart';
import 'package:morrow_studio/desktop_frame.dart';
import 'package:morrow_studio/storage.dart';
import 'package:morrow_studio/window_effects.dart';
import 'package:window_manager/window_manager.dart';
import 'live_ui_helpers.dart';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  testWidgets(
    'Windows canvas, component editor, tips and narrow settings',
    (t) async {
      final output = Platform.environment['MORROW_WINDOW_TEST_OUTPUT']!;
      final boundary = GlobalKey();
      Finder keyed(String key) => find.byKey(ValueKey(key));
      await initializeDesktopFrame();
      await windowManager.setSize(const Size(1180, 820));
      final storage = MemoryStorage();
      try {
        await t.pumpWidget(
          RepaintBoundary(
            key: boundary,
            child: MorrowApp(
              initialLocale: const Locale('zh'),
              storage: storage,
              nativeBackground: DesktopBackground(),
            ),
          ),
        );
        await saveBoundaryPng(t, boundary, '$output/01-floating-tips.png');
        await tapVisible(t, keyed('settings-expand'));
        await windowManager.setSize(const Size(1540, 900));
        await saveBoundaryPng(t, boundary, '$output/00-expanded-settings.png');
        expect(find.byType(Scrollbar), findsNothing);
        await tapVisible(t, keyed('compact-settings-back'));
        await windowManager.setSize(const Size(1180, 820));
        await tapVisible(t, keyed('theme-dark'));
        await tapVisible(t, keyed('background-solid'));
        await tapVisible(t, keyed('component-settings'));
        await saveBoundaryPng(t, boundary, '$output/02-dark-components.png');
        await t.scrollUntilVisible(
          keyed('component-entry:footer'),
          400,
          scrollable: find.descendant(
            of: find.byKey(const PageStorageKey('component-list-scroll')),
            matching: find.byType(Scrollable),
          ),
        );
        await tapVisible(t, keyed('component-entry:footer'));
        await saveBoundaryPng(t, boundary, '$output/03-default-tips.png');
        await tapVisible(t, keyed('component-custom-toggle'));
        t.widget<Slider>(keyed('component-opacity')).onChanged!(.35);
        await t.pump();
        await saveBoundaryPng(t, boundary, '$output/04-tips-material.png');
        await tapVisible(t, keyed('component-apply'));
        await tapVisible(t, find.byType(BackButton));
        expect(
          t
              .widget<Studio>(find.byType(Studio))
              .palette
              .surfaces
              .components['footer']!
              .opacity,
          .35,
        );
        await tapVisible(t, keyed('background-transparent'));
        await tapVisible(t, keyed('component-settings'));
        await tapVisible(t, keyed('component-entry:hero'));
        expect(keyed('transparent-canvas-tint'), findsOneWidget);
        await saveBoundaryPng(t, boundary, '$output/05-transparent-editor.png');
        await tapVisible(t, find.byType(BackButton));
        await tapVisible(t, find.byType(BackButton));
        await tapVisible(t, keyed('background-ambient'));
        await windowManager.setSize(const Size(390, 760));
        await waitForUi(
          t,
          () => keyed('appearance-toggle').hitTestable().evaluate().isNotEmpty,
          reason: 'compact workspace after resize',
        );
        await tapVisible(t, keyed('appearance-toggle'));
        await saveBoundaryPng(t, boundary, '$output/06-narrow-settings.png');
        expect(t.takeException(), isNull);
      } finally {
        await t.pumpWidget(const SizedBox());
      }
    },
    timeout: const Timeout(Duration(minutes: 2)),
  );
}
