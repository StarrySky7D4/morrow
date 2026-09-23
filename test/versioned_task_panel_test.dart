import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_i18n/morrow_i18n.dart';
import 'package:morrow_studio/plugins/versioned_content.dart';
import 'package:morrow_studio/plugins/versioned_idea_view.dart';
import 'package:morrow_studio/versioned_task_panel.dart';

VersionedIdeaView _view({
  int formatVersion = 2,
  int revision = 7,
  String category = '进行中',
  String stage = '计划中',
  List<VersionedTask> tasks = const [],
  List<String> todos = const [],
  List<String> completed = const [],
  int complete = 0,
  int incomplete = 0,
  int ambiguous = 0,
}) => VersionedIdeaView.fromRecord(
  VersionedContentRecord(
    id: 'task-panel-card',
    title: 'Card',
    revision: BigInt.from(revision),
    formatVersion: formatVersion,
    description: 'body',
    category: category,
    stage: stage,
    hypothesis: '',
    conclusion: '',
    favorite: false,
    assets: const [],
    icon: 0,
    color: 0xff123456,
    deleted: false,
    deletedAt: BigInt.zero,
    todos: todos,
    completed: completed,
    tasks: tasks,
    origin: null,
    retiredTaskIds: const [],
    projectedStage: '宿主阶段',
    completeCount: complete,
    incompleteCount: incomplete,
    ambiguousCount: ambiguous,
  ),
);

Widget _frame(
  VersionedIdeaView view,
  Future<void> Function(TaskEditCommand) onCommand, {
  bool writable = true,
  bool busy = false,
  double? width,
  double textScale = 1,
}) => MaterialApp(
  locale: const Locale('en'),
  localizationsDelegates: AppLocalizations.localizationsDelegates,
  supportedLocales: AppLocalizations.supportedLocales,
  builder: (context, child) => MediaQuery(
    data: MediaQuery.of(
      context,
    ).copyWith(textScaler: TextScaler.linear(textScale)),
    child: child!,
  ),
  home: Scaffold(
    body: SingleChildScrollView(
      child: SizedBox(
        width: width,
        child: VersionedTaskPanel(
          view: view,
          writable: writable,
          busy: busy,
          onCommand: onCommand,
        ),
      ),
    ),
  ),
);

const _ambiguousA = VersionedTask(
  id: 'task-a',
  text: 'same',
  completion: VersionedTaskCompletion.legacyAmbiguous,
  legacyCompleted: true,
  legacyDuplicates: 2,
);
const _ambiguousB = VersionedTask(
  id: 'task-b',
  text: 'same',
  completion: VersionedTaskCompletion.legacyAmbiguous,
  legacyCompleted: true,
  legacyDuplicates: 2,
);

void main() {
  testWidgets('narrow task controls remain usable with enlarged text', (
    tester,
  ) async {
    Future<void> submit(TaskEditCommand command) async {}
    final view = _view(
      tasks: [
        const VersionedTask(
          id: 'narrow-task',
          text: 'A long task title that wraps at narrow widths',
          completion: VersionedTaskCompletion.incomplete,
          legacyCompleted: false,
          legacyDuplicates: 0,
        ),
      ],
      incomplete: 1,
    );
    for (final (width, scale) in [(280.0, 2.0), (390.0, 1.3)]) {
      await tester.pumpWidget(
        _frame(view, submit, width: width, textScale: scale),
      );
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull, reason: '$width at $scale');
      expect(find.byKey(const ValueKey('task-add')), findsOneWidget);
      expect(
        find.byKey(const ValueKey('task-rename-narrow-task')),
        findsOneWidget,
      );
      expect(find.byKey(const ValueKey('task-set-stage')), findsOneWidget);
      expect(
        find.byKey(const ValueKey('task-complete-all-stage')),
        findsOneWidget,
      );

      final stage = find.byType(DropdownButtonFormField<String>);
      await tester.ensureVisible(stage);
      await tester.tap(stage);
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull, reason: 'open $width at $scale');
      await tester.tapAt(const Offset(1, 1));
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull, reason: 'close $width at $scale');
    }
  });

  testWidgets('ambiguous duplicate rows require separate explicit decisions', (
    tester,
  ) async {
    final commands = <TaskEditCommand>[];
    Future<void> submit(TaskEditCommand command) async => commands.add(command);
    await tester.pumpWidget(
      _frame(_view(tasks: [_ambiguousA, _ambiguousB], ambiguous: 2), submit),
    );
    expect(find.byType(Checkbox), findsNothing);
    expect(find.byKey(const ValueKey('task-row-task-a')), findsOneWidget);
    expect(find.byKey(const ValueKey('task-row-task-b')), findsOneWidget);
    expect(
      find.text(
        L10n.forLocale(const Locale('en')).mainTaskProgressThreeWay(2, 0, 0),
      ),
      findsOneWidget,
    );
    await tester.tap(
      find.byKey(const ValueKey('task-ambiguous-complete-task-a')),
    );
    await tester.pump();
    expect(commands, hasLength(1));
    expect(commands.single.kind, TaskEditKind.setCompletion);
    expect(commands.single.taskId, 'task-a');
    expect(commands.single.complete, isTrue);

    final decidedA = VersionedTask(
      id: 'task-a',
      text: 'same',
      completion: VersionedTaskCompletion.complete,
      legacyCompleted: true,
      legacyDuplicates: 2,
    );
    await tester.pumpWidget(
      _frame(
        _view(
          revision: 8,
          tasks: [decidedA, _ambiguousB],
          complete: 1,
          ambiguous: 1,
        ),
        submit,
      ),
    );
    await tester.pump();
    await tester.tap(
      find.byKey(const ValueKey('task-ambiguous-incomplete-task-b')),
    );
    await tester.pump();
    expect(commands, hasLength(2));
    expect(commands.last.taskId, 'task-b');
    expect(commands.last.complete, isFalse);
    expect(find.byKey(const ValueKey('task-checkbox-task-a')), findsOneWidget);
    expect(find.byKey(const ValueKey('task-checkbox-task-b')), findsNothing);
  });

  testWidgets('add, rename, reorder, remove, and stage keep distinct intents', (
    tester,
  ) async {
    final commands = <TaskEditCommand>[];
    Future<void> submit(TaskEditCommand command) async => commands.add(command);
    const first = VersionedTask(
      id: 'task-1',
      text: 'first',
      completion: VersionedTaskCompletion.incomplete,
      legacyCompleted: false,
      legacyDuplicates: 1,
    );
    const second = VersionedTask(
      id: 'task-2',
      text: 'second',
      completion: VersionedTaskCompletion.complete,
      legacyCompleted: false,
      legacyDuplicates: 1,
    );
    await tester.pumpWidget(
      _frame(_view(tasks: [first, second], complete: 1, incomplete: 1), submit),
    );
    await tester.enterText(
      find.byKey(const ValueKey('task-add-input')),
      ' new task ',
    );
    await tester.tap(find.byKey(const ValueKey('task-add')));
    await tester.pump();
    expect(commands.single.kind, TaskEditKind.add);
    expect(commands.single.text, 'new task');
    expect(commands.single.taskId, matches(RegExp(r'^task-[0-9a-f]{32}$')));
    final added = VersionedTask(
      id: commands.single.taskId,
      text: 'new task',
      completion: VersionedTaskCompletion.incomplete,
      legacyCompleted: false,
      legacyDuplicates: 1,
    );
    await tester.pumpWidget(
      _frame(
        _view(
          revision: 8,
          tasks: [first, second, added],
          complete: 1,
          incomplete: 2,
        ),
        submit,
      ),
    );
    await tester.tap(find.byKey(const ValueKey('task-rename-task-1')));
    await tester.pumpAndSettle();
    await tester.enterText(
      find.descendant(
        of: find.byType(AlertDialog),
        matching: find.byType(TextField),
      ),
      'renamed',
    );
    await tester.tap(find.byKey(const ValueKey('task-rename-confirm-task-1')));
    await tester.pumpAndSettle();
    expect(commands.last.kind, TaskEditKind.rename);
    expect(commands.last.taskId, 'task-1');
    expect(commands.last.text, 'renamed');

    const renamed = VersionedTask(
      id: 'task-1',
      text: 'renamed',
      completion: VersionedTaskCompletion.incomplete,
      legacyCompleted: false,
      legacyDuplicates: 1,
    );
    await tester.pumpWidget(
      _frame(
        _view(
          revision: 9,
          tasks: [renamed, second, added],
          complete: 1,
          incomplete: 2,
        ),
        submit,
      ),
    );
    await tester.tap(find.byKey(const ValueKey('task-down-task-1')));
    await tester.pump();
    expect(commands.last.kind, TaskEditKind.reorder);
    expect(commands.last.order, ['task-2', 'task-1', added.id]);

    await tester.pumpWidget(
      _frame(
        _view(
          revision: 10,
          tasks: [second, renamed, added],
          complete: 1,
          incomplete: 2,
        ),
        submit,
      ),
    );
    final beforeRemove = commands.length;
    await tester.tap(find.byKey(const ValueKey('task-remove-task-2')));
    await tester.pumpAndSettle();
    expect(commands, hasLength(beforeRemove));
    await tester.tap(find.byKey(const ValueKey('task-confirm-action')));
    await tester.pumpAndSettle();
    expect(commands.last.kind, TaskEditKind.remove);
    expect(commands.last.taskId, 'task-2');

    await tester.pumpWidget(
      _frame(
        _view(revision: 11, tasks: [renamed, added], incomplete: 2),
        submit,
      ),
    );
    await tester.tap(find.byType(DropdownButtonFormField<String>));
    await tester.pumpAndSettle();
    await tester.tap(
      find.text(L10n.forLocale(const Locale('en')).mainStageActive).last,
    );
    await tester.pumpAndSettle();
    await tester.tap(find.byKey(const ValueKey('task-set-stage')));
    await tester.pump();
    expect(commands.last.kind, TaskEditKind.setStage);
    expect(commands.last.text, '推进中');

    await tester.pumpWidget(
      _frame(
        _view(
          revision: 12,
          stage: '推进中',
          tasks: [renamed, added],
          incomplete: 2,
        ),
        submit,
      ),
    );
    await tester.tap(find.byType(DropdownButtonFormField<String>));
    await tester.pumpAndSettle();
    await tester.tap(
      find.text(L10n.forLocale(const Locale('en')).mainStageCompleted).last,
    );
    await tester.pumpAndSettle();
    final beforeCompleteAll = commands.length;
    await tester.tap(find.byKey(const ValueKey('task-complete-all-stage')));
    await tester.pumpAndSettle();
    expect(commands, hasLength(beforeCompleteAll));
    expect(
      find.text(
        L10n.forLocale(const Locale('en')).mainTaskCompleteAllConfirm(
          L10n.forLocale(const Locale('en')).mainStageCompleted,
        ),
      ),
      findsOneWidget,
    );
    await tester.tap(find.byKey(const ValueKey('task-confirm-action')));
    await tester.pumpAndSettle();
    expect(commands.last.kind, TaskEditKind.completeAllAndSetStage);
    expect(commands.last.text, '已完成');
  });

  testWidgets(
    'failure only exposes retry of original command across revisions',
    (tester) async {
      final commands = <TaskEditCommand>[];
      var failFirst = true;
      Future<void> submit(TaskEditCommand command) async {
        commands.add(command);
        if (failFirst) {
          failFirst = false;
          throw StateError('private host detail');
        }
      }

      const task = VersionedTask(
        id: 'task-1',
        text: 'task',
        completion: VersionedTaskCompletion.incomplete,
        legacyCompleted: false,
        legacyDuplicates: 1,
      );
      await tester.pumpWidget(
        _frame(_view(tasks: [task], incomplete: 1), submit),
      );
      await tester.tap(find.byKey(const ValueKey('task-checkbox-task-1')));
      await tester.pump();
      expect(commands, hasLength(1));
      expect(find.textContaining('private host detail'), findsNothing);
      await tester.pumpWidget(
        _frame(_view(revision: 8, tasks: [task], incomplete: 1), submit),
      );
      expect(
        tester
            .widget<TextButton>(find.byKey(const ValueKey('task-add')))
            .onPressed,
        isNull,
      );
      await tester.tap(find.byKey(const ValueKey('task-retry-original')));
      await tester.pump();
      expect(commands, hasLength(2));
      expect(identical(commands.first, commands.last), isTrue);
      expect(find.byKey(const ValueKey('task-retry-original')), findsNothing);
      expect(
        tester
            .widget<TextButton>(find.byKey(const ValueKey('task-add')))
            .onPressed,
        isNull,
      );
      // The accepted historical retry can have the same revision as the
      // current view. A newly delivered view still releases the guard.
      await tester.pumpWidget(
        _frame(_view(revision: 8, tasks: [task], incomplete: 1), submit),
      );
      expect(
        tester
            .widget<TextButton>(find.byKey(const ValueKey('task-add')))
            .onPressed,
        isNotNull,
      );
    },
  );

  testWidgets('successful retry waits for delayed parent view', (tester) async {
    final commands = <TaskEditCommand>[];
    Future<void> submit(TaskEditCommand command) async {
      commands.add(command);
      if (commands.length == 1) throw StateError('reply lost');
    }

    await tester.pumpWidget(_frame(_view(), submit));
    await tester.enterText(
      find.byKey(const ValueKey('task-add-input')),
      'retained draft',
    );
    await tester.tap(find.byKey(const ValueKey('task-add')));
    await tester.pump();
    await tester.tap(find.byKey(const ValueKey('task-retry-original')));
    await tester.pump();
    expect(commands, hasLength(2));
    expect(identical(commands.first, commands.last), isTrue);
    expect(find.byKey(const ValueKey('task-retry-original')), findsNothing);
    expect(
      tester
          .widget<TextButton>(find.byKey(const ValueKey('task-add')))
          .onPressed,
      isNull,
    );
    await tester.pump();
    expect(
      tester
          .widget<TextButton>(find.byKey(const ValueKey('task-add')))
          .onPressed,
      isNull,
    );
    await tester.pumpWidget(_frame(_view(revision: 8), submit));
    expect(
      tester
          .widget<TextButton>(find.byKey(const ValueKey('task-add')))
          .onPressed,
      isNotNull,
    );
    expect(
      tester
          .widget<TextField>(find.byKey(const ValueKey('task-add-input')))
          .controller!
          .text,
      'retained draft',
    );
  });

  testWidgets('parent can deliver accepted view before retry completes', (
    tester,
  ) async {
    final commands = <TaskEditCommand>[];
    final finishRetry = Completer<void>();
    Future<void> submit(TaskEditCommand command) async {
      commands.add(command);
      if (commands.length == 1) throw StateError('reply lost');
      if (commands.length == 2) await finishRetry.future;
    }

    await tester.pumpWidget(_frame(_view(), submit));
    await tester.enterText(
      find.byKey(const ValueKey('task-add-input')),
      'draft',
    );
    await tester.tap(find.byKey(const ValueKey('task-add')));
    await tester.pump();
    await tester.tap(find.byKey(const ValueKey('task-retry-original')));
    await tester.pump();
    expect(commands, hasLength(2));
    await tester.pumpWidget(_frame(_view(revision: 8), submit));
    expect(
      tester
          .widget<TextButton>(find.byKey(const ValueKey('task-add')))
          .onPressed,
      isNull,
    );
    finishRetry.complete();
    await tester.pump();
    expect(find.byKey(const ValueKey('task-retry-original')), findsNothing);
    expect(
      tester
          .widget<TextButton>(find.byKey(const ValueKey('task-add')))
          .onPressed,
      isNotNull,
    );
  });

  testWidgets('a locally unsent retry keeps the earlier unknown frozen', (
    tester,
  ) async {
    final commands = <TaskEditCommand>[];
    Future<void> submit(TaskEditCommand command) async {
      commands.add(command);
      if (commands.length == 1) throw StateError('reply lost');
      if (commands.length == 2) throw const VersionedMutationNotSubmitted();
      if (commands.length == 3) {
        throw VersionedMutationNoCommit(
          id: 'task-panel-card',
          operation: 'original-operation',
          sourceRevision: BigInt.from(7),
        );
      }
    }

    await tester.pumpWidget(_frame(_view(), submit));
    await tester.enterText(
      find.byKey(const ValueKey('task-add-input')),
      'retained draft',
    );
    await tester.tap(find.byKey(const ValueKey('task-add')));
    await tester.pump();
    expect(find.byKey(const ValueKey('task-retry-original')), findsOneWidget);
    await tester.tap(find.byKey(const ValueKey('task-retry-original')));
    await tester.pump();
    expect(commands, hasLength(2));
    expect(identical(commands.first, commands.last), isTrue);
    expect(find.byKey(const ValueKey('task-retry-original')), findsOneWidget);
    expect(
      find.text(L10n.forLocale(const Locale('en')).mainSaveUnknown),
      findsWidgets,
    );
    expect(
      tester
          .widget<TextButton>(find.byKey(const ValueKey('task-add')))
          .onPressed,
      isNull,
    );
    await tester.tap(find.byKey(const ValueKey('task-retry-original')));
    await tester.pump();
    expect(commands, hasLength(3));
    expect(identical(commands.first, commands.last), isTrue);
    expect(find.byKey(const ValueKey('task-retry-original')), findsNothing);
    expect(
      tester
          .widget<TextButton>(find.byKey(const ValueKey('task-add')))
          .onPressed,
      isNotNull,
    );
    expect(
      tester
          .widget<TextField>(find.byKey(const ValueKey('task-add-input')))
          .controller!
          .text,
      'retained draft',
    );
    await tester.tap(find.byKey(const ValueKey('task-add')));
    await tester.pump();
    expect(commands, hasLength(4));
    expect(identical(commands.first, commands.last), isFalse);
  });

  testWidgets('late Add completion cannot clear a disposed panel', (
    tester,
  ) async {
    final finish = Completer<void>();
    await tester.pumpWidget(_frame(_view(), (_) => finish.future));
    await tester.enterText(
      find.byKey(const ValueKey('task-add-input')),
      'draft',
    );
    await tester.tap(find.byKey(const ValueKey('task-add')));
    await tester.pump();
    await tester.pumpWidget(const SizedBox());
    finish.complete();
    await tester.pump();
    expect(tester.takeException(), isNull);
  });

  testWidgets('known unsent edit can be corrected and submitted', (
    tester,
  ) async {
    final commands = <TaskEditCommand>[];
    Future<void> submit(TaskEditCommand command) async {
      commands.add(command);
      if (command.text.length > 2048) {
        throw VersionedMutationNotSubmitted();
      }
    }

    await tester.pumpWidget(_frame(_view(), submit));
    await tester.enterText(
      find.byKey(const ValueKey('task-add-input')),
      'x' * 2049,
    );
    await tester.tap(find.byKey(const ValueKey('task-add')));
    await tester.pump();
    expect(commands, hasLength(1));
    expect(find.byKey(const ValueKey('task-retry-original')), findsNothing);
    expect(
      find.text(L10n.forLocale(const Locale('en')).mainSaveNotSubmitted),
      findsOneWidget,
    );
    expect(
      tester
          .widget<TextButton>(find.byKey(const ValueKey('task-add')))
          .onPressed,
      isNotNull,
    );
    await tester.enterText(
      find.byKey(const ValueKey('task-add-input')),
      'fixed',
    );
    await tester.tap(find.byKey(const ValueKey('task-add')));
    await tester.pump();
    expect(commands, hasLength(2));
    expect(commands.last.text, 'fixed');
    expect(commands.last.taskId, isNot(commands.first.taskId));
  });

  testWidgets('legacy, unavailable, and busy cards cannot issue TaskId edits', (
    tester,
  ) async {
    final commands = <TaskEditCommand>[];
    Future<void> submit(TaskEditCommand command) async => commands.add(command);
    await tester.pumpWidget(
      _frame(
        _view(
          formatVersion: 1,
          todos: ['same', 'same'],
          completed: ['same'],
          complete: 2,
        ),
        submit,
      ),
    );
    expect(find.byType(Checkbox), findsNothing);
    expect(find.byKey(const ValueKey('task-add')), findsNothing);
    await tester.pumpWidget(
      _frame(
        _view(tasks: [_ambiguousA], ambiguous: 1),
        submit,
        writable: false,
      ),
    );
    expect(
      tester
          .widget<TextButton>(
            find.byKey(const ValueKey('task-ambiguous-complete-task-a')),
          )
          .onPressed,
      isNull,
    );
    await tester.pumpWidget(
      _frame(_view(tasks: [_ambiguousA], ambiguous: 1), submit, busy: true),
    );
    expect(
      tester
          .widget<TextButton>(
            find.byKey(const ValueKey('task-ambiguous-complete-task-a')),
          )
          .onPressed,
      isNull,
    );
    expect(commands, isEmpty);
  });

  testWidgets('non-project cards expose only their canonical stage choices', (
    tester,
  ) async {
    final commands = <TaskEditCommand>[];
    Future<void> submit(TaskEditCommand command) async => commands.add(command);
    const task = VersionedTask(
      id: 'experiment-task',
      text: 'observe',
      completion: VersionedTaskCompletion.incomplete,
      legacyCompleted: false,
      legacyDuplicates: 1,
    );
    await tester.pumpWidget(
      _frame(
        _view(category: '实验', stage: '待验证', tasks: [task], incomplete: 1),
        submit,
      ),
    );
    await tester.tap(find.byType(DropdownButtonFormField<String>));
    await tester.pumpAndSettle();
    final en = L10n.forLocale(const Locale('en'));
    expect(find.text(en.mainStagePlanned), findsNothing);
    await tester.tap(find.text(en.mainStageVerifying).last);
    await tester.pumpAndSettle();
    await tester.tap(find.byKey(const ValueKey('task-complete-all-stage')));
    await tester.pumpAndSettle();
    expect(
      find.text(en.mainTaskCompleteAllConfirm(en.mainStageVerifying)),
      findsOneWidget,
    );
    await tester.tap(find.byKey(const ValueKey('task-confirm-action')));
    await tester.pumpAndSettle();
    expect(commands.single.kind, TaskEditKind.completeAllAndSetStage);
    expect(commands.single.text, '验证中');

    await tester.pumpWidget(
      _frame(_view(category: '灵感', stage: '待整理', revision: 8), submit),
    );
    await tester.tap(find.byType(DropdownButtonFormField<String>));
    await tester.pumpAndSettle();
    expect(find.text(en.mainStageActive), findsNothing);
    expect(find.text(en.mainStageOrganized), findsWidgets);
  });
}
