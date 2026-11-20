import 'dart:ui' show SemanticsAction;

import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';
import 'package:morrow_studio/settings_page_transition.dart';

void main() {
  testWidgets(
    'The fade blocks pointer hits without removing accessible actions',
    (tester) async {
      var activations = 0;
      Widget action(String label) =>
          TextButton(onPressed: () => activations++, child: Text(label));
      Future<void> showSettings(bool show) => tester.pumpWidget(
        MaterialApp(
          home: Material(
            child: SettingsPageTransition(
              showSettings: show,
              duration: const Duration(milliseconds: 320),
              content: action('Workspace action'),
              settings: action('Settings action'),
            ),
          ),
        ),
      );
      Future<void> checkActive(String label) async {
        final node = find.semantics.byLabel(label).evaluate().single;
        expect(node.getSemanticsData().hasAction(SemanticsAction.tap), isTrue);
        await tester.tapAt(tester.getCenter(find.text(label)));
        expect(activations, 0);
      }

      await showSettings(false);
      await showSettings(true);
      await tester.pump(const Duration(milliseconds: 80));
      await checkActive('Workspace action');
      await tester.pump(const Duration(milliseconds: 160));
      await checkActive('Settings action');
      await tester.pumpAndSettle();
      await tester.tap(find.text('Settings action'));
      expect(activations, 1);
    },
  );

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
