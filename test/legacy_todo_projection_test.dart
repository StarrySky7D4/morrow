import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/main.dart';
import 'package:morrow_studio/storage.dart';
import 'package:morrow_studio/plugins/workbench_ids.dart';

void main() {
  testWidgets(
    'duplicate legacy text has matching stage/progress and stage side effects require confirmation',
    (t) async {
      t.view.devicePixelRatio = 1;
      t.view.physicalSize = const Size(1440, 1000);
      addTearDown(t.view.reset);
      final idea = Idea(
        'duplicate project',
        'body',
        '进行中',
        Idea.icons[0],
        Colors.blue,
        id: 'legacy',
        todos: ['same', 'same'],
        completed: {'same'},
        stage: '推进中',
      );
      final storage = MemoryStorage()
        ..data = {
          'version': 1,
          'theme': 'white',
          'glass': 'frosted',
          'background': 'ambient',
          'ideas': [idea.toJson()],
        };
      await t.pumpWidget(
        MorrowApp(storage: storage, initialLocale: const Locale('en')),
      );
      await t.pumpAndSettle();
      await t.tap(find.byKey(ValueKey('nav-${WorkbenchPage.projects.id}')));
      await t.pumpAndSettle();
      final menu = find.byType(PopupMenuButton<String>).first;
      expect(
        find.descendant(of: menu, matching: find.text('Completed')),
        findsOneWidget,
      );
      expect(idea.legacyCompletedCount, 2);
      await t.tap(menu);
      await t.pumpAndSettle();
      await t.tap(find.text('Planned').last);
      await t.pumpAndSettle();
      expect(find.textContaining('uncheck the last task'), findsOneWidget);
      await t.tap(find.text('Cancel').last);
      await t.pumpAndSettle();
      expect((storage.data!['ideas'] as List).single['completed'], ['same']);
      await t.tap(menu);
      await t.pumpAndSettle();
      await t.tap(find.text('Planned').last);
      await t.pumpAndSettle();
      await t.tap(find.text('Continue').last);
      await t.pumpAndSettle();
      expect((storage.data!['ideas'] as List).single['completed'], isEmpty);
      expect((storage.data!['ideas'] as List).single['stage'], '计划中');
    },
  );
}
