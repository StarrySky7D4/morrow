import 'dart:io';

import 'package:morrow_studio/attachments/attachment.dart';
import 'package:morrow_studio/media/texture_source.dart';
import 'package:morrow_studio/plugins/editor_session.dart';
import 'package:morrow_studio/plugins/versioned_editor.dart';
import 'package:morrow_studio/plugins/versioned_editor_adapter.dart';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/main.dart';
import 'package:morrow_studio/plugins/studio_storage.dart';
import 'package:morrow_studio/plugins/versioned_content.dart';
import 'package:morrow_studio/plugins/workbench_backend.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';

final _executable = Platform.environment['MORROW_WORKBENCH_HOST'];
final _package = Platform.environment['MORROW_WORKBENCH_PACKAGE'];
final _nativeAvailable =
    Platform.isWindows && _executable != null && _package != null;

void main() {
  test(
    'storage opens a mixed library and keeps V2 edits in typed records',
    () async {
      final directory = await Directory.systemTemp.createTemp(
        'morrow-mixed-workspace-',
      );
      RustWorkbench? host;
      try {
        host = await RustWorkbench.open(
          executable: _executable!,
          package: _package!,
          directory: directory,
        );
        Future<void> create(String id) async {
          await host!.apply(
            PluginAction.create,
            Idea(
              id,
              'original body',
              '进行中',
              Idea.icons[0],
              const Color(0xff8866aa),
              id: id,
              stage: '计划中',
              todos: const ['same', 'same'],
              completed: const {'same'},
            ),
          );
        }

        await create('mixed-v1');
        await create('mixed-v2');
        final plan = await host.versionedContent.planMigration('mixed-v2');
        await host.versionedContent.migrate(plan);

        var storage = await RustStudioStorage.open(host);
        final snapshot = storage.read();
        expect(snapshot['ideas'], isEmpty);
        final rows = (snapshot['workspaceIdeas'] as List).cast<Idea>();
        expect(
          rows.map((row) => row.id),
          containsAll(['mixed-v1', 'mixed-v2']),
        );
        final legacy = rows.singleWhere((row) => row.id == 'mixed-v1');
        var modern = rows.singleWhere((row) => row.id == 'mixed-v2');
        expect(legacy.versioned, isNull);
        expect(legacy.todos, ['same', 'same']);
        expect(legacy.completed, {'same'});
        expect(modern.versioned?.formatVersion, 2);
        expect(modern.todos, isEmpty);
        expect(modern.completed, isEmpty);
        expect(modern.versioned?.tasks.length, 2);
        expect(modern.versioned?.origin?.cardId, 'mixed-v2');
        expect(() => modern.toJson(), throwsStateError);

        await storage.write({...snapshot, 'glass': 'clear'});
        expect(storage.read()['ideas'], isEmpty);
        expect((storage.read()['workspaceIdeas'] as List).length, 2);
        expect(
          await host.query(
            '概览',
            '全部',
            '',
            '最近添加',
            operation: 'mixed-workspace-query',
          ),
          containsAll(['mixed-v1', 'mixed-v2']),
        );

        final first = modern.versioned!.source.tasks.first.id;
        modern = await host.applyWorkspaceTask(
          'mixed-task-first',
          modern,
          TaskEditCommand.setCompletion(first, true),
        );
        expect(
          modern.versioned!.source.tasks.first.completion,
          VersionedTaskCompletion.complete,
        );
        expect(
          modern.versioned!.source.tasks.last.completion,
          VersionedTaskCompletion.legacyAmbiguous,
        );
        expect(modern.todos, isEmpty);
        await expectLater(
          host.apply(PluginAction.todo, modern, text: 'same'),
          throwsFormatException,
        );
        await expectLater(
          host.apply(PluginAction.edit, modern),
          throwsFormatException,
        );

        modern = await host.apply(PluginAction.stage, modern, text: '已完成');
        expect(modern.stage, '已完成');
        expect(
          modern.versioned!.source.tasks.last.completion,
          VersionedTaskCompletion.legacyAmbiguous,
        );
        modern = await host.applyWorkspaceCard(
          'mixed-category',
          modern,
          const CardEditCommand.setCategory('实验', '待验证'),
        );
        expect(modern.category, '实验');
        expect(modern.stage, '待验证');
        modern = await host.apply(PluginAction.favorite, modern, flag: true);
        expect(modern.favorite, isTrue);

        modern = await host.apply(PluginAction.delete, modern);
        expect(modern.contentDeleted, isTrue);
        expect(
          (await host.refreshEditorContent()).map((row) => row.id),
          isNot(contains('mixed-v2')),
        );
        modern = await host.apply(PluginAction.restore, modern);
        expect(modern.contentDeleted, isFalse);
        expect(
          (await host.refreshEditorContent()).map((row) => row.id),
          contains('mixed-v2'),
        );

        await host.close();
        host = null;
        host = await RustWorkbench.open(
          executable: _executable!,
          package:
              '${directory.path}${Platform.pathSeparator}missing-plugin.morrowplugin',
          directory: directory,
        );
        expect(host.writable, isFalse);
        storage = await RustStudioStorage.open(host);
        expect(storage.read()['ideas'], isEmpty);
        final reopened = (storage.read()['workspaceIdeas'] as List)
            .cast<Idea>();
        expect(
          reopened
              .singleWhere((row) => row.id == 'mixed-v2')
              .versioned
              ?.formatVersion,
          2,
        );
        expect(
          reopened.singleWhere((row) => row.id == 'mixed-v1').versioned,
          isNull,
        );
      } finally {
        await host?.close();
        await directory.delete(recursive: true);
      }
    },
    skip: !_nativeAvailable,
  );
  test(
    'real V2 dialog bridge preserves TaskIds and presents exact attachment bytes',
    () async {
      final directory = await Directory.systemTemp.createTemp(
        'morrow-workspace-editor-',
      );
      RustWorkbench? host;
      VersionedEditorSession? session;
      try {
        host = await RustWorkbench.open(
          executable: _executable!,
          package: _package!,
          directory: directory,
        );
        const id = 'workspace-editor-card';
        await host.apply(
          PluginAction.create,
          Idea(
            'Original',
            'body',
            '进行中',
            Idea.icons[0],
            const Color(0xff8866aa),
            id: id,
            stage: '计划中',
            todos: const ['same', 'same'],
            completed: const {'same'},
          ),
        );
        final migrated = (await host.versionedContent.migrate(
          await host.versionedContent.planMigration(id),
        )).current;
        final originalIds = migrated.tasks.map((task) => task.id).toList();
        final originalCompletion = migrated.tasks
            .map((task) => task.completion)
            .toList();
        final baseline = await host.workspaceRecord(migrated);
        session = await host.openVersionedEditor(
          id,
          expectedRevision: baseline.contentRevision,
        );
        final bridge = VersionedWorkbenchEditorAdapter(
          session,
          baseline.versioned!,
          host.workspaceRecord,
        );
        final capture = await bridge.studio.capture(
          'html',
          '<strong>Captured</strong>',
        );
        expect(capture.ticket, isNotEmpty);
        const before = 'A😀B';
        final after = before.replaceRange(1, 3, capture.markdown);
        await bridge.recordPaste(
          PasteInsertion(
            id: 'workspace-html-paste',
            field: 'description',
            before: before,
            startUtf16: 1,
            endUtf16: 3,
            parts: [PastePart.ticket(capture.ticket!)],
            after: after,
          ),
        );
        final file = File('${directory.path}/source note.txt');
        await file.writeAsString('note');
        final selected = IdeaAttachment(
          source: TextureSource(
            location: file.path,
            name: 'source note.txt',
            kind: TextureKind.file,
            local: true,
          ),
          size: 4,
        );
        final description =
            '$after\n[note](attachment:${Uri.encodeComponent(file.path)})';
        // The dialog's draft carries the immutable V2 baseline while its
        // contentRevision may be null; task fields must remain empty.
        final draft = Idea(
          'Updated',
          description,
          baseline.category,
          baseline.icon,
          baseline.color,
          id: id,
          stage: baseline.stage,
          favorite: baseline.favorite,
          attachments: [selected],
          hypothesis: 'Hypothesis',
          conclusion: 'Conclusion',
          versioned: baseline.versioned,
        );
        expect(draft.contentRevision, isNull);
        final saved = await bridge.save(
          draft,
          EditorFields(
            title: '  Updated  ',
            description: description,
            hypothesis: ' Hypothesis ',
            conclusion: ' Conclusion ',
            todos: '',
          ),
        );
        expect(saved.title, 'Updated');
        expect(saved.description, contains(capture.markdown));
        expect(saved.description, isNot(contains(file.path)));
        expect(
          saved.versioned!.source.tasks.map((task) => task.id),
          originalIds,
        );
        expect(
          saved.versioned!.source.tasks.map((task) => task.completion),
          originalCompletion,
        );
        expect(saved.versioned!.origin?.cardId, id);
        expect(saved.attachments.single.byteLength, BigInt.from(4));
        expect(saved.attachments.single.toJson()['exactSize'], '4');
        expect(saved.attachments.single.pluginId, isNotEmpty);
        expect(
          saved.description,
          contains('attachment:${saved.attachments.single.pluginId}'),
        );

        await bridge.close();
        session = null;
        await expectLater(
          host.openVersionedEditor(id, expectedRevision: migrated.revision),
          throwsStateError,
        );
        final currentSession = await host.openVersionedEditor(
          id,
          expectedRevision: saved.contentRevision,
        );
        await currentSession.close();
        final refreshed = (await host.refreshEditorContent()).singleWhere(
          (row) => row.id == id,
        );
        expect(refreshed.versioned?.revision, saved.versioned?.revision);
        expect(refreshed.attachments.single.byteLength, BigInt.from(4));
        final previewPath = refreshed.attachments.single.source.location;
        expect(await File(previewPath).exists(), isTrue);
        await host.close();
        host = null;
        expect(await File(previewPath).exists(), isFalse);
      } finally {
        await session?.close();
        await host?.close();
        await directory.delete(recursive: true);
      }
    },
    skip: !_nativeAvailable,
    timeout: const Timeout(Duration(minutes: 3)),
  );
  test(
    'post-commit asset export failure reports committed refresh and keeps the edit',
    () async {
      final directory = await Directory.systemTemp.createTemp(
        'morrow-workspace-export-failure-',
      );
      RustWorkbench? host;
      try {
        host = await RustWorkbench.open(
          executable: _executable!,
          package: _package!,
          directory: directory,
        );
        final file = File('${directory.path}/source.txt');
        await file.writeAsString('note');
        const id = 'export-failure-card';
        await host.apply(
          PluginAction.create,
          Idea(
            'Original',
            'body',
            '灵感',
            Idea.icons[0],
            const Color(0xff8866aa),
            id: id,
            todos: const ['one task'],
            attachments: [
              IdeaAttachment(
                source: TextureSource(
                  location: file.path,
                  name: 'source.txt',
                  kind: TextureKind.file,
                  local: true,
                ),
                size: 4,
              ),
            ],
          ),
        );
        final migrated = (await host.versionedContent.migrate(
          await host.versionedContent.planMigration(id),
        )).current;
        var modern = await host.workspaceRecord(migrated);
        expect(modern.attachments.single.byteLength, BigInt.from(4));
        final taskId = modern.versioned!.source.tasks.single.id;
        final previewPath = modern.attachments.single.source.location;
        final initialFileCount = await host.cache
            .list()
            .where((entry) => entry is File)
            .length;
        for (var index = 0; index < 3; index++) {
          modern = await host.applyWorkspaceTask(
            'cache-preserving-task-$index',
            modern,
            TaskEditCommand.setCompletion(taskId, index.isEven),
          );
          expect(modern.attachments.single.source.location, previewPath);
          expect(
            await host.cache.list().where((entry) => entry is File).length,
            initialFileCount,
          );
        }
        // A direct typed mutation is not a locally proven preserve transition.
        // A changed preview file must not be reused just because its old
        // recorded digest matches the new host export.
        await File(previewPath).writeAsString('tampered');
        final externallyEdited = await host.versionedContent.editTasks(
          'cache-untrusted-task',
          id,
          modern.versioned!.revision,
          TaskEditCommand.setCompletion(taskId, false),
        );
        modern = await host.workspaceRecord(externallyEdited.current);
        expect(modern.attachments.single.source.location, isNot(previewPath));
        expect(
          await File(modern.attachments.single.source.location).readAsString(),
          'note',
        );
        await host.cache.delete(recursive: true);
        await File(host.cache.path).writeAsString('blocked preview path');
        await expectLater(
          host.apply(PluginAction.favorite, modern, flag: true),
          throwsA(isA<WorkbenchCommittedRefreshFailure>()),
        );
        final committed = await host.versionedContent.read(id);
        expect(committed.favorite, isTrue);
        expect(committed.revision, modern.contentRevision! + BigInt.one);
      } finally {
        await host?.close();
        await directory.delete(recursive: true);
      }
    },
    skip: !_nativeAvailable,
  );
  test(
    'an Idea from another host cannot mutate an equal ID and revision',
    () async {
      final first = await Directory.systemTemp.createTemp('morrow-owner-a-');
      final second = await Directory.systemTemp.createTemp('morrow-owner-b-');
      RustWorkbench? hostA;
      RustWorkbench? hostB;
      try {
        hostA = await RustWorkbench.open(
          executable: _executable!,
          package: _package!,
          directory: first,
        );
        hostB = await RustWorkbench.open(
          executable: _executable!,
          package: _package!,
          directory: second,
        );
        const id = 'shared-id';
        Future<Idea> create(RustWorkbench host) => host.apply(
          PluginAction.create,
          Idea(
            'Original',
            'body',
            '进行中',
            Idea.icons[0],
            const Color(0xff8866aa),
            id: id,
            stage: '计划中',
            todos: const ['task'],
          ),
        );
        final legacyA = await create(hostA);
        final legacyB = await create(hostB);
        expect(legacyA.contentRevision, legacyB.contentRevision);
        expect(identical(legacyA.contentOwner, hostA), isTrue);
        await expectLater(
          hostB.apply(PluginAction.favorite, legacyA, flag: true),
          throwsFormatException,
        );
        expect((await hostB.versionedContent.read(id)).favorite, isFalse);
        final migratedA = (await hostA.versionedContent.migrate(
          await hostA.versionedContent.planMigration(id),
        )).current;
        final migratedB = (await hostB.versionedContent.migrate(
          await hostB.versionedContent.planMigration(id),
        )).current;
        expect(migratedA.revision, migratedB.revision);
        final foreign = await hostA.workspaceRecord(migratedA);
        expect(identical(foreign.contentOwner, hostA), isTrue);
        await expectLater(
          hostB.applyWorkspaceTask(
            'cross-host-task',
            foreign,
            TaskEditCommand.setCompletion(
              foreign.versioned!.source.tasks.single.id,
              true,
            ),
          ),
          throwsFormatException,
        );
        await expectLater(
          hostB.applyWorkspaceCard(
            'cross-host-card',
            foreign,
            const CardEditCommand.setFavorite(true),
          ),
          throwsFormatException,
        );
        await expectLater(
          hostB.apply(PluginAction.favorite, foreign, flag: true),
          throwsFormatException,
        );
        final untouched = await hostB.versionedContent.read(id);
        expect(untouched.revision, migratedB.revision);
        expect(untouched.favorite, isFalse);
        expect(
          untouched.tasks.single.completion,
          VersionedTaskCompletion.incomplete,
        );
      } finally {
        await hostA?.close();
        await hostB?.close();
        await first.delete(recursive: true);
        await second.delete(recursive: true);
      }
    },
    skip: !_nativeAvailable,
  );
}
