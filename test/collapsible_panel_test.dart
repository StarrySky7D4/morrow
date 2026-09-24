import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/collapsible_panel.dart';

void main() {
  testWidgets(
    'collapse preserves input and focus boundaries through reversal',
    (tester) async {
      final controller = TextEditingController(text: 'draft');
      final focus = FocusNode();
      addTearDown(controller.dispose);
      addTearDown(focus.dispose);
      final childKey = GlobalKey();
      const panelKey = ValueKey('panel');
      Widget scene(bool expanded, {bool reduced = false}) => MaterialApp(
        home: MediaQuery(
          data: MediaQueryData(disableAnimations: reduced),
          child: Scaffold(
            body: Column(
              children: [
                CollapsiblePanel(
                  key: panelKey,
                  expanded: expanded,

                  child: SizedBox(
                    height: 160,
                    child: TextField(
                      key: childKey,
                      controller: controller,
                      focusNode: focus,
                    ),
                  ),
                ),
                const SizedBox(height: 20),
              ],
            ),
          ),
        ),
      );
      await tester.pumpWidget(scene(true));
      await tester.pumpAndSettle();
      await tester.tap(find.byType(TextField));
      await tester.pump();
      final state = childKey.currentState;
      expect(focus.hasFocus, isTrue);
      await tester.pumpWidget(scene(false));
      await tester.pump(const Duration(milliseconds: 120));
      final during = tester.getSize(find.byKey(panelKey)).height;
      expect(during, inExclusiveRange(0, 160));
      final opacity = tester.widget<Opacity>(
        find
            .descendant(
              of: find.byKey(panelKey),
              matching: find.byType(Opacity),
            )
            .first,
      );
      expect(opacity.opacity, inExclusiveRange(0, 1));
      expect(focus.hasFocus, isFalse);
      final ignore = tester.widget<IgnorePointer>(
        find
            .descendant(
              of: find.byKey(panelKey),
              matching: find.byType(IgnorePointer),
            )
            .first,
      );
      expect(ignore.ignoring, isTrue);
      expect(
        tester
            .widget<ExcludeSemantics>(
              find
                  .descendant(
                    of: find.byKey(panelKey),
                    matching: find.byType(ExcludeSemantics),
                  )
                  .first,
            )
            .excluding,
        isTrue,
      );
      await tester.pumpWidget(scene(true));
      expect(tester.getSize(find.byKey(panelKey)).height, closeTo(during, .01));
      await tester.pumpAndSettle();
      expect(childKey.currentState, same(state));
      expect(controller.text, 'draft');
      await tester.pumpWidget(scene(false, reduced: true));
      expect(tester.getSize(find.byKey(panelKey)).height, 0);
      expect(find.byType(TextField).hitTestable(), findsNothing);
      await tester.pumpWidget(scene(false));
      await tester.pumpWidget(scene(true));
      await tester.pump(const Duration(milliseconds: 80));
      final legacy = tester.widget<Opacity>(
        find
            .descendant(
              of: find.byKey(panelKey),
              matching: find.byType(Opacity),
            )
            .first,
      );
      expect(legacy.opacity, inExclusiveRange(0, 1));
      expect(childKey.currentState, same(state));
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull);
    },
  );
}
