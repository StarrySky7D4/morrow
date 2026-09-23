import 'dart:io';
import 'package:morrow_studio/plugins/workbench_native.dart';
import 'package:morrow_studio/plugins/studio_storage.dart';
import 'dart:async';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/main.dart';
import 'package:morrow_studio/storage.dart';
import 'package:morrow_studio/plugins/editor_session.dart';
import 'package:morrow_studio/plugins/versioned_content.dart';
import 'package:morrow_studio/plugins/versioned_idea_view.dart';
import 'package:morrow_studio/plugins/workbench_backend.dart';
import 'package:morrow_studio/plugins/workbench_ids.dart';
import 'package:morrow_studio/versioned_task_panel.dart';

VersionedContentRecord record({
  int revision = 3,
  bool deleted = false,
  String stage = '计划中',
  List<VersionedTask>? tasks,
}) {
  final values =
      tasks ??
      const [
        VersionedTask(
          id: 'a',
          text: 'same',
          completion: VersionedTaskCompletion.legacyAmbiguous,
          legacyCompleted: true,
          legacyDuplicates: 2,
        ),
        VersionedTask(
          id: 'b',
          text: 'same',
          completion: VersionedTaskCompletion.legacyAmbiguous,
          legacyCompleted: true,
          legacyDuplicates: 2,
        ),
      ];
  return VersionedContentRecord(
    id: 'v2-card',
    title: 'Mixed format card',
    revision: BigInt.from(revision),
    formatVersion: 2,
    description: 'body',
    category: '进行中',
    stage: stage,
    hypothesis: '',
    conclusion: '',
    favorite: false,
    assets: const [],
    icon: 0,
    color: 0xff778899,
    deleted: deleted,
    deletedAt: deleted ? BigInt.one : BigInt.zero,
    todos: const [],
    completed: const [],
    tasks: values,
    origin: null,
    retiredTaskIds: const [],
    projectedStage: stage,
    completeCount: values
        .where((t) => t.completion == VersionedTaskCompletion.complete)
        .length,
    incompleteCount: values
        .where((t) => t.completion == VersionedTaskCompletion.incomplete)
        .length,
    ambiguousCount: values
        .where((t) => t.completion == VersionedTaskCompletion.legacyAmbiguous)
        .length,
  );
}

Idea present(VersionedContentRecord record) => Idea(
  record.title,
  record.description,
  record.category,
  Idea.icons[0],
  Color(record.color),
  id: record.id,
  stage: record.stage,
  contentRevision: record.revision,
  contentDeleted: record.deleted,
  versioned: VersionedIdeaView.fromRecord(record),
);

class TransientStorage extends StudioStorage {
  TransientStorage(Idea idea)
    : data = {
        'version': 1,
        'theme': 'white',
        'glass': 'frosted',
        'background': 'ambient',
        'ideas': <Object>[],
        'workspaceIdeas': <Idea>[idea],
      };
  Map<String, dynamic> data;
  @override
  Map<String, dynamic> read() => data;
  @override
  Future<void> write(Map<String, dynamic> value) async => data = value;
}

class MixedBackend extends WorkbenchBackend
    implements
        WorkbenchMixedContent,
        WorkbenchContentRevisionSource,
        WorkbenchEditorSupport {
  MixedBackend(this.current);
  Idea current;
  bool failOnce = false;
  bool noCommitOnce = false;
  bool localRejectOnce = false;
  bool cardFailOnce = false;
  Completer<Idea>? held;
  final taskCalls =
      <({String operation, Idea source, TaskEditCommand command})>[];
  final cardCalls =
      <({String operation, Idea source, CardEditCommand command})>[];
  @override
  bool writable = true;
  @override
  Iterable<String> knownContentIds() => [current.id];
  @override
  BigInt? knownContentRevision(String id) => current.contentRevision;
  @override
  bool? knownContentDeleted(String id) => current.contentDeleted;
  @override
  Future<List<Idea>> loadWorkspaceContent() async =>
      current.contentDeleted ? [] : [current];
  @override
  Future<List<Idea>> refreshEditorContent() => loadWorkspaceContent();
  @override
  Future<Idea> workspaceRecord(VersionedContentRecord record) async =>
      present(record);
  @override
  Future<WorkbenchEditorSession> openEditor(
    String id, {
    required bool create,
  }) => throw StateError('V2 must not use the legacy editor');
  @override
  Future<Idea> apply(
    PluginAction action,
    Idea idea, {
    String text = '',
    bool flag = false,
  }) => throw StateError('V2 must not use legacy mutations');
  @override
  Future<List<String>> query(
    String section,
    String filter,
    String text,
    String sort, {
    String? operation,
  }) async => current.contentDeleted ? [] : [current.id];
  @override
  Future<Idea> applyWorkspaceTask(
    String operation,
    Idea source,
    TaskEditCommand command,
  ) async {
    taskCalls.add((operation: operation, source: source, command: command));
    if (noCommitOnce) {
      noCommitOnce = false;
      current = present(record(revision: current.contentRevision!.toInt() + 1));
      throw VersionedMutationNoCommit(
        id: source.id,
        operation: operation,
        sourceRevision: source.versioned!.revision,
      );
    }
    if (held != null) return held!.future;
    if (failOnce) {
      failOnce = false;
      throw StateError('Unknown test outcome');
    }
    if (localRejectOnce) {
      localRejectOnce = false;
      throw const VersionedMutationNotSubmitted();
    }
    final old = current.versioned!.source;
    final tasks = old.tasks
        .map(
          (task) =>
              task.id == command.taskId &&
                  command.kind == TaskEditKind.setCompletion
              ? VersionedTask(
                  id: task.id,
                  text: task.text,
                  completion: command.complete
                      ? VersionedTaskCompletion.complete
                      : VersionedTaskCompletion.incomplete,
                  legacyCompleted: task.legacyCompleted,
                  legacyDuplicates: task.legacyDuplicates,
                )
              : task,
        )
        .toList();
    return current = present(
      record(
        revision: old.revision.toInt() + 1,
        tasks: tasks,
        stage: command.kind == TaskEditKind.setStage ? command.text : old.stage,
      ),
    );
  }

  @override
  Future<Idea> applyWorkspaceCard(
    String operation,
    Idea source,
    CardEditCommand command,
  ) async {
    cardCalls.add((operation: operation, source: source, command: command));
    if (cardFailOnce) {
      cardFailOnce = false;
      throw StateError('Unknown delete outcome');
    }
    return current = present(
      record(
        revision: current.contentRevision!.toInt() + 1,
        deleted: command.kind == CardEditKind.delete,
      ),
    );
  }
}

void main() {
  testWidgets(
    'a task dialog from the old workspace cannot submit to its replacement',
    (t) async {
      final old = MixedBackend(present(record()));
      final storage = TransientStorage(old.current);
      await t.pumpWidget(MorrowApp(storage: storage, workbench: old));
      await t.pumpAndSettle();
      final dynamic studio = t.state(find.byType(Studio));
      final opening = studio.openIdea(old.current) as Future<void>;
      await t.pumpAndSettle();
      final oldPanel = t.widget<VersionedTaskPanel>(
        find.byType(VersionedTaskPanel),
      );
      final replacement = MixedBackend(present(record()));
      await t.pumpWidget(MorrowApp(storage: storage, workbench: replacement));
      await t.pumpAndSettle();
      await expectLater(
        oldPanel.onCommand(TaskEditCommand.setCompletion('a', true)),
        throwsA(isA<VersionedMutationNotSubmitted>()),
      );
      expect(replacement.taskCalls, isEmpty);
      Navigator.of(t.element(find.byType(VersionedTaskPanel))).pop();
      await t.pumpAndSettle();
      await opening;
      await t.pumpWidget(const SizedBox());
    },
  );
  testWidgets(
    'oversized task draft is not sent and a corrected draft can submit',
    (t) async {
      final backend = MixedBackend(present(record()));
      await t.pumpWidget(
        MorrowApp(
          storage: TransientStorage(backend.current),
          workbench: backend,
        ),
      );
      await t.pumpAndSettle();
      final dynamic studio = t.state(find.byType(Studio));
      final source = backend.current;
      await expectLater(
        studio.versionedChange(
              source,
              TaskEditCommand.rename('a', List.filled(700, '界').join()),
            )
            as Future<Idea>,
        throwsA(isA<VersionedMutationNotSubmitted>()),
      );
      expect(backend.taskCalls, isEmpty);
      await (studio.versionedChange(
            source,
            TaskEditCommand.setCompletion('a', true),
          )
          as Future<Idea>);
      expect(backend.taskCalls, hasLength(1));
      await t.pumpAndSettle();
      await t.pumpWidget(const SizedBox());
    },
  );
  testWidgets(
    'a locally unsent retry cannot release an earlier unknown intent',
    (t) async {
      final backend = MixedBackend(present(record()))
        ..failOnce = true
        ..localRejectOnce = true;
      await t.pumpWidget(
        MorrowApp(
          storage: TransientStorage(backend.current),
          workbench: backend,
          initialLocale: const Locale('en'),
        ),
      );
      await t.pumpAndSettle();
      final dynamic studio = t.state(find.byType(Studio));
      final source = backend.current;
      final original = TaskEditCommand.setCompletion('a', true);
      await expectLater(
        studio.versionedChange(source, original) as Future<Idea>,
        throwsStateError,
      );
      await expectLater(
        studio.versionedChange(source, original) as Future<Idea>,
        throwsA(isA<VersionedMutationNotSubmitted>()),
      );
      expect(backend.taskCalls, hasLength(2));
      expect(
        backend.taskCalls.last.operation,
        backend.taskCalls.first.operation,
      );
      await expectLater(
        studio.versionedChange(source, TaskEditCommand.setCompletion('b', true))
            as Future<Idea>,
        throwsStateError,
      );
      expect(backend.taskCalls, hasLength(2));
      await (studio.versionedChange(source, original) as Future<Idea>);
      expect(backend.taskCalls, hasLength(3));
      expect(
        backend.taskCalls.last.operation,
        backend.taskCalls.first.operation,
      );
      await t.pumpWidget(const SizedBox());
    },
  );
  testWidgets(
    'host NoCommit refreshes an open task dialog and keeps its draft',
    (t) async {
      final backend = MixedBackend(present(record()))..noCommitOnce = true;
      await t.pumpWidget(
        MorrowApp(
          storage: TransientStorage(backend.current),
          workbench: backend,
          initialLocale: const Locale('en'),
        ),
      );
      await t.pumpAndSettle();
      final dynamic studio = t.state(find.byType(Studio));
      final opening = studio.openIdea(backend.current) as Future<void>;
      await t.pumpAndSettle();
      await t.enterText(
        find.byKey(const ValueKey('task-add-input')),
        'keep this draft',
      );
      await t.ensureVisible(find.byKey(const ValueKey('task-add')));
      await t.tap(find.byKey(const ValueKey('task-add')));
      await t.pumpAndSettle();
      expect(backend.taskCalls, hasLength(1));
      expect(
        backend.taskCalls.single.source.versioned!.revision,
        BigInt.from(3),
      );
      expect(find.byKey(const ValueKey('task-retry-original')), findsNothing);
      expect(
        t
            .widget<TextField>(find.byKey(const ValueKey('task-add-input')))
            .controller!
            .text,
        'keep this draft',
      );
      expect(
        t.widget<TextButton>(find.byKey(const ValueKey('task-add'))).onPressed,
        isNotNull,
      );
      await t.ensureVisible(find.byKey(const ValueKey('task-add')));
      await t.tap(find.byKey(const ValueKey('task-add')));
      await t.pumpAndSettle();
      expect(backend.taskCalls, hasLength(2));
      expect(backend.taskCalls.last.source.versioned!.revision, BigInt.from(4));
      expect(
        backend.taskCalls.last.operation,
        isNot(backend.taskCalls.first.operation),
      );
      Navigator.of(t.element(find.byType(VersionedTaskPanel))).pop();
      await t.pumpAndSettle();
      await opening;
      await t.pumpWidget(const SizedBox());
    },
  );
  testWidgets(
    'confirmed original delete retry still offers Undo at its current revision',
    (t) async {
      final backend = MixedBackend(present(record()))..cardFailOnce = true;
      await t.pumpWidget(
        MorrowApp(
          storage: TransientStorage(backend.current),
          workbench: backend,
          initialLocale: const Locale('en'),
        ),
      );
      await t.pumpAndSettle();
      final dynamic studio = t.state(find.byType(Studio));
      final source = backend.current;
      expect(
        await (studio.pluginChange(PluginAction.delete, source)
            as Future<Idea?>),
        isNull,
      );
      final original = backend.cardCalls.single;
      await (studio.versionedChange(source, original.command) as Future<Idea>);
      await t.pumpAndSettle();
      expect(find.text('Undo'), findsOneWidget);
      final deletedRevision = backend.current.contentRevision;
      await t.tap(find.text('Undo'));
      await t.pumpAndSettle();
      expect(backend.cardCalls[1].operation, original.operation);
      expect(backend.cardCalls.last.command.kind, CardEditKind.restore);
      expect(backend.cardCalls.last.source.contentRevision, deletedRevision);
      expect((studio.ideas as List<Idea>).single.contentDeleted, isFalse);
      await t.pumpWidget(const SizedBox());
    },
  );
  testWidgets(
    'real mixed storage enters the main workspace after process restart',
    (t) async {
      final executable = Platform.environment['MORROW_WORKBENCH_HOST']!;
      final package = Platform.environment['MORROW_WORKBENCH_PACKAGE']!;
      Directory? directory;
      RustWorkbench? backend;
      RustStudioStorage? storage;
      t.view.devicePixelRatio = 1;
      t.view.physicalSize = const Size(1440, 1000);
      addTearDown(t.view.reset);
      try {
        await t.runAsync(() async {
          directory = await Directory.systemTemp.createTemp(
            'morrow-mixed-main-',
          );
          backend = await RustWorkbench.open(
            executable: executable,
            package: package,
            directory: directory!,
          );
          for (final id in ['native-legacy', 'native-versioned']) {
            await backend!.apply(
              PluginAction.create,
              Idea(
                'Visible $id',
                'native body',
                '进行中',
                Idea.icons[0],
                Colors.blue,
                id: id,
                stage: '计划中',
                todos: const ['same', 'same'],
                completed: {'same'},
              ),
            );
          }
          final content = backend!.versionedContent;
          await content.migrate(
            await content.planMigration('native-versioned'),
          );
          await backend!.close();
          backend = await RustWorkbench.open(
            executable: executable,
            package: package,
            directory: directory!,
          );
          storage = await RustStudioStorage.open(backend!);
        });
        await t.pumpWidget(
          MorrowApp(
            storage: storage,
            workbench: backend,
            initialLocale: const Locale('en'),
          ),
        );
        for (var i = 0; i < 80; i++) {
          await t.runAsync(
            () => Future<void>.delayed(const Duration(milliseconds: 25)),
          );
          await t.pump(const Duration(milliseconds: 50));
          if (find.text('Visible native-versioned').evaluate().isNotEmpty) {
            break;
          }
        }
        expect(find.byType(Studio), findsOneWidget);
        expect(find.text('Visible native-versioned'), findsWidgets);
        expect(find.text('Visible native-legacy'), findsWidgets);
        final dynamic studio = t.state(find.byType(Studio));
        final ideas = studio.ideas as List<Idea>;
        final v2 = ideas.singleWhere((v) => v.id == 'native-versioned');
        expect(v2.versioned!.ambiguousCount, 2);
        final opening = studio.openIdea(v2) as Future<void>;
        await t.pumpAndSettle();
        expect(find.byType(VersionedTaskPanel), findsOneWidget);
        expect(
          find.byKey(
            ValueKey(
              'task-ambiguous-complete-${v2.versioned!.source.tasks.first.id}',
            ),
          ),
          findsOneWidget,
        );
        Navigator.of(t.element(find.byType(VersionedTaskPanel))).pop();
        await t.pumpAndSettle();
        await opening;
        expect(t.takeException(), isNull);
      } finally {
        await t.pumpWidget(const SizedBox());
        await t.runAsync(() async {
          await backend?.close();
          if (directory != null) await directory!.delete(recursive: true);
        });
      }
    },
    skip:
        !Platform.isWindows ||
        Platform.environment['MORROW_WORKBENCH_HOST'] == null ||
        Platform.environment['MORROW_WORKBENCH_PACKAGE'] == null,
    timeout: const Timeout(Duration(minutes: 3)),
  );
  testWidgets(
    'mixed workspace restores typed rows, shows ambiguity and never serializes V2 as V1',
    (t) async {
      t.view.devicePixelRatio = 1;
      t.view.physicalSize = const Size(1440, 1000);
      addTearDown(t.view.reset);
      final backend = MixedBackend(present(record()));
      final storage = TransientStorage(backend.current);
      await t.pumpWidget(
        MorrowApp(
          storage: storage,
          workbench: backend,
          initialLocale: const Locale('en'),
        ),
      );
      await t.pumpAndSettle();
      final dynamic studio = t.state(find.byType(Studio));
      expect((studio.ideas as List<Idea>).single.versioned!.ambiguousCount, 2);
      expect(() => backend.current.toJson(), throwsStateError);
      await t.tap(find.byKey(ValueKey('nav-${WorkbenchPage.projects.id}')));
      await t.pumpAndSettle();
      expect(
        find.text('0 complete · 0 incomplete · 2 to confirm'),
        findsOneWidget,
      );
      expect(
        find.descendant(
          of: find.byType(PopupMenuButton<String>).first,
          matching: find.text('Planned'),
        ),
        findsOneWidget,
      );
      expect(
        find.byKey(const ValueKey('task-preview-v2-card-a')),
        findsOneWidget,
      );
      expect(
        find.byKey(const ValueKey('task-preview-v2-card-b')),
        findsOneWidget,
      );
      final opening = studio.openIdea(backend.current) as Future<void>;
      await t.pumpAndSettle();
      expect(find.byType(VersionedTaskPanel), findsOneWidget);
      final decide = find.byKey(const ValueKey('task-ambiguous-complete-a'));
      await t.ensureVisible(decide);
      await t.tap(decide);
      await t.pumpAndSettle();
      expect(backend.taskCalls.single.command.taskId, 'a');
      expect(backend.current.versioned!.completeCount, 1);
      expect(backend.current.versioned!.ambiguousCount, 1);
      expect(
        backend.current.versioned!.tasks.map(
          (v) => (v as IdentifiedIdeaTaskView).taskId,
        ),
        ['a', 'b'],
      );
      expect(storage.data['ideas'], isEmpty);
      expect((storage.data['workspaceIdeas'] as List).single, isA<Idea>());
      Navigator.of(t.element(find.byType(VersionedTaskPanel))).pop();
      await t.pumpAndSettle();
      await opening;
      await t.pumpWidget(const SizedBox());
    },
  );

  testWidgets(
    'unknown task retry keeps operation source revision and payload; changing intent is blocked',
    (t) async {
      final backend = MixedBackend(present(record()))..failOnce = true;
      await t.pumpWidget(
        MorrowApp(
          storage: TransientStorage(backend.current),
          workbench: backend,
        ),
      );
      await t.pumpAndSettle();
      final dynamic studio = t.state(find.byType(Studio));
      final original = backend.current;
      final command = TaskEditCommand.setCompletion('a', true);
      await expectLater(
        studio.versionedChange(original, command) as Future<Idea>,
        throwsStateError,
      );
      await expectLater(
        studio.versionedChange(
              original,
              TaskEditCommand.setCompletion('b', true),
            )
            as Future<Idea>,
        throwsStateError,
      );
      expect(backend.taskCalls, hasLength(1));
      final result =
          await (studio.versionedChange(original, command) as Future<Idea>);
      expect(backend.taskCalls, hasLength(2));
      expect(backend.taskCalls[1].operation, backend.taskCalls[0].operation);
      expect(backend.taskCalls[1].source.contentRevision, BigInt.from(3));
      expect(identical(backend.taskCalls[1].command, command), isTrue);
      expect(result.versioned!.ambiguousCount, 1);
      await t.pumpAndSettle();
      await t.pumpWidget(const SizedBox());
    },
  );

  testWidgets(
    'typed delete restore and late replies respect workspace identity',
    (t) async {
      final old = MixedBackend(present(record()));
      final storage = TransientStorage(old.current);
      await t.pumpWidget(MorrowApp(storage: storage, workbench: old));
      await t.pumpAndSettle();
      final dynamic studio = t.state(find.byType(Studio));
      final deleted =
          await (studio.pluginChange(PluginAction.delete, old.current)
              as Future<Idea?>);
      expect(deleted!.contentDeleted, isTrue);
      expect(studio.ideas, isEmpty);
      await (studio.pluginChange(PluginAction.restore, deleted)
          as Future<Idea?>);
      expect(old.cardCalls[1].source.contentRevision, deleted.contentRevision);
      expect((studio.ideas as List<Idea>).single.contentDeleted, isFalse);
      old.held = Completer<Idea>();
      final pending =
          studio.versionedChange(
                old.current,
                TaskEditCommand.setCompletion('a', true),
              )
              as Future<Idea>;
      final next = MixedBackend(present(record(revision: 20)));
      await t.pumpWidget(MorrowApp(storage: storage, workbench: next));
      await t.pumpAndSettle();
      old.held!.complete(present(record(revision: 7)));
      await pending;
      await t.pumpAndSettle();
      expect(
        (studio.ideas as List<Idea>).single.contentRevision,
        BigInt.from(20),
      );
      await t.pumpWidget(const SizedBox());
    },
  );
}
