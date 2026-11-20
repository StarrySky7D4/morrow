import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';
import 'package:morrow_studio/settings_page_transition.dart';

void main() {
  testWidgets(
    'Only the active page is announced throughout the settings fade',
    (tester) async {
      Future<void> showSettings(bool show) => tester.pumpWidget(
        MaterialApp(
          home: SettingsPageTransition(
            showSettings: show,
            duration: const Duration(milliseconds: 320),
            content: const Text('Workspace page'),
            settings: const Text('Settings page'),
          ),
        ),
      );

      void expectPage({required bool settings}) {
        expect(
          find.semantics.byLabel('Settings page'),
          settings ? findsOne : findsNothing,
        );
        expect(
          find.semantics.byLabel('Workspace page'),
          settings ? findsNothing : findsOne,
        );
      }

      await showSettings(false);
      expectPage(settings: false);
      await showSettings(true);
      await tester.pump(const Duration(milliseconds: 159));
      expectPage(settings: false);
      await tester.pump(const Duration(milliseconds: 1));
      expectPage(settings: true);
      await tester.pumpAndSettle();
      expectPage(settings: true);

      await showSettings(false);
      await tester.pump(const Duration(milliseconds: 160));
      expectPage(settings: true);
      await tester.pump(const Duration(milliseconds: 1));
      expectPage(settings: false);
      await tester.pumpAndSettle();
      expectPage(settings: false);
    },
  );
}
