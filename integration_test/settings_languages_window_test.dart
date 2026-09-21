import 'dart:io';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:morrow_i18n/morrow_i18n.dart';
import 'package:morrow_studio/desktop_frame.dart';
import 'package:morrow_studio/main.dart';
import 'package:morrow_studio/storage.dart';
import 'package:window_manager/window_manager.dart';
import 'live_ui_helpers.dart';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  testWidgets(
    'Seven languages in the Windows settings window',
    (t) async {
      final output = Platform.environment['MORROW_WINDOW_TEST_OUTPUT']!;
      await initializeDesktopFrame();
      final boundary = GlobalKey();
      for (final code in ['ru', 'fr', 'de', 'es', 'ja', 'ko', 'pt']) {
        await windowManager.setSize(const Size(1180, 820));
        await t.pumpWidget(
          RepaintBoundary(
            key: boundary,
            child: MorrowApp(
              storage: MemoryStorage(),
              initialLocale: Locale(code),
            ),
          ),
        );
        await tapVisible(t, find.byKey(const ValueKey('settings-expand')));
        await saveBoundaryPng(t, boundary, '$output/$code-wide.png');
        final title = find.byKey(const ValueKey('appearance-heading-title'));
        expect(
          t.widget<Text>(title).data,
          L10n.forLocale(Locale(code)).mainAppearance,
        );
        await windowManager.setMinimumSize(const Size(320, 600));
        await windowManager.setSize(const Size(390, 820));
        await saveBoundaryPng(t, boundary, '$output/$code-narrow.png');
        await tapVisible(t, find.byKey(const ValueKey('component-settings')));
        await tapVisible(t, find.byKey(const ValueKey('component-entry:hero')));
        await saveBoundaryPng(t, boundary, '$output/$code-component.png');
        expect(t.takeException(), isNull, reason: code);
        await t.pumpWidget(const SizedBox());
      }
    },
    timeout: const Timeout(Duration(minutes: 4)),
  );
}
