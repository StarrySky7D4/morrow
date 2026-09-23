import 'dart:async';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/main.dart';
import 'package:morrow_studio/attachments/attachment.dart';
import 'package:morrow_studio/media/texture_source.dart';
import 'package:morrow_studio/plugins/editor_session.dart';
import 'package:morrow_studio/plugins/versioned_content.dart';
import 'package:morrow_studio/plugins/versioned_editor.dart';
import 'package:morrow_studio/plugins/versioned_editor_adapter.dart';
import 'package:morrow_studio/plugins/workbench_backend.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';

final _executable = Platform.environment['MORROW_WORKBENCH_HOST'];
final _package = Platform.environment['MORROW_WORKBENCH_PACKAGE'];
final _available =
    Platform.isWindows && _executable != null && _package != null;

class _DelayedFirstSave
    implements WorkbenchEditorSession, WorkbenchEditorContinuation {
  _DelayedFirstSave(this.delegate);
  final WorkbenchEditorSession delegate;
  final release = Completer<void>();
  Idea? committed;
  Object? firstFailure;
  int saves = 0;
  int continuations = 0;

  @override
  String get targetId => delegate.targetId;
  @override
  get studio => delegate.studio;
  @override
  Future<void> recordPaste(PasteInsertion insertion) =>
      delegate.recordPaste(insertion);
  @override
  Future<void> close() => delegate.close();

  @override
  Future<Idea> save(Idea draft, EditorFields fields) async {
    final first = saves++ == 0;
    try {
      final result = await delegate.save(draft, fields);
      if (first) {
        committed = result; // Real host commit, presentation and ack finished.
        await release.future;
      }
      return result;
    } catch (error) {
      if (first) firstFailure = error;
      rethrow;
    }
  }

  @override
  Future<WorkbenchEditorSession> continueAfterCommit(Idea confirmed) {
    continuations++;
    return (delegate as WorkbenchEditorContinuation).continueAfterCommit(
      confirmed,
    );
  }
}

Future<void> _waitFor(WidgetTester tester, bool Function() ready) async {
  final deadline = DateTime.now().add(const Duration(seconds: 45));
  while (!ready()) {
    if (DateTime.now().isAfter(deadline)) {
      fail('native editor did not finish within 45 seconds');
    }
    await tester.runAsync(
      () => Future<void>.delayed(const Duration(milliseconds: 10)),
    );
    await tester.pump();
    final error = tester.takeException();
    if (error != null) throw error;
  }
  await tester.pump();
}

Future<T> _hostFuture<T>(WidgetTester tester, Future<T> request) async {
  var done = false;
  late T value;
  Object? failure;
  request.then(
    (result) {
      value = result;
      done = true;
    },
    onError: (Object error) {
      failure = error;
      done = true;
    },
  );
  await _waitFor(tester, () => done);
  if (failure != null) throw failure!;
  return value;
}

void main() {
  testWidgets(
    'real V2 editor commits S1, keeps typed S2, and commits S2 only on next Save',
    (tester) async {
      late final Directory directory;
      await tester.runAsync(() async {
        directory = await Directory.systemTemp.createTemp(
          'morrow-editor-draft-native-',
        );
      });
      RustWorkbench? host;
      VersionedEditorSession? initialSession;
      _DelayedFirstSave? delayed;
      Idea? returned;
      late File sourceFile;
      late Idea dialogInitial;
      const attachmentBytes = <int>[0, 37, 104, 255, 10, 3];
      try {
        late VersionedContentRecord migrated;
        late Idea baseline;
        await tester.runAsync(() async {
          host = await RustWorkbench.open(
            executable: _executable!,
            package: _package!,
            directory: directory,
          );
          const id = 'editor-draft-native';
          await host!.apply(
            PluginAction.create,
            Idea(
              'Original title',
              'Original body',
              '进行中',
              Idea.icons[0],
              const Color(0xff8866aa),
              id: id,
              stage: '计划中',
              todos: const ['first task', 'second task'],
              completed: const {'first task'},
            ),
          );
          migrated = (await host!.versionedContent.migrate(
            await host!.versionedContent.planMigration(id),
          )).current;
          baseline = await host!.workspaceRecord(migrated);
          sourceFile = File('${directory.path}/draft-attachment.bin');
          await sourceFile.writeAsBytes(attachmentBytes);
          final localAttachment = IdeaAttachment(
            source: TextureSource(
              location: sourceFile.path,
              name: 'draft-attachment.bin',
              kind: TextureKind.file,
              local: true,
            ),
            size: attachmentBytes.length,
          );
          dialogInitial = Idea(
            baseline.title,
            baseline.description,
            baseline.category,
            baseline.icon,
            baseline.color,
            id: baseline.id,
            stage: baseline.stage,
            favorite: baseline.favorite,
            todos: baseline.todos,
            completed: baseline.completed,
            hypothesis: baseline.hypothesis,
            conclusion: baseline.conclusion,
            attachments: [...baseline.attachments, localAttachment],
            contentRevision: baseline.contentRevision,
            contentOwner: baseline.contentOwner,
            versioned: baseline.versioned,
          );
          initialSession = await host!.openVersionedEditor(
            id,
            expectedRevision: baseline.contentRevision,
          );
        });
        debugPrint('native draft: setup complete');
        expect(migrated.revision, BigInt.from(2));
        final taskIds = migrated.tasks.map((task) => task.id).toList();
        final taskCompletion = migrated.tasks
            .map((task) => task.completion)
            .toList();
        var reopenedScopes = 0;
        late Future<WorkbenchEditorSession> Function(Idea) reopen;
        reopen = (confirmed) async {
          final current = await host!.versionedContent.read(confirmed.id);
          expect(current.revision, confirmed.contentRevision);
          final presented = await host!.workspaceRecord(current);
          expect(presented.versioned?.revision, confirmed.contentRevision);
          final session = await host!.openVersionedEditor(
            confirmed.id,
            expectedRevision: confirmed.contentRevision,
          );
          reopenedScopes++;
          return VersionedWorkbenchEditorAdapter(
            session,
            presented.versioned!,
            host!.workspaceRecord,
            reopen: reopen,
          );
        };
        final adapter = VersionedWorkbenchEditorAdapter(
          initialSession!,
          baseline.versioned!,
          host!.workspaceRecord,
          reopen: reopen,
        );
        delayed = _DelayedFirstSave(adapter);

        await tester.pumpWidget(
          MaterialApp(
            home: Builder(
              builder: (context) => Scaffold(
                body: TextButton(
                  onPressed: () => showDialog<Idea>(
                    context: context,
                    builder: (_) => NewIdeaDialog(
                      initialIdea: dialogInitial,
                      editor: delayed,
                    ),
                  ).then((value) => returned = value),
                  child: const Text('open editor'),
                ),
              ),
            ),
          ),
        );
        await tester.tap(find.text('open editor'));
        await tester.pumpAndSettle();
        final title = find.byKey(const ValueKey('idea-title'));
        final description = find.byKey(const ValueKey('idea-description'));
        final save = find.byKey(const ValueKey('idea-save'));
        await tester.enterText(title, 'S1 title');
        final localAlias = 'attachment:${Uri.encodeComponent(sourceFile.path)}';
        await tester.enterText(description, 'S1 body [asset]($localAlias)');
        await tester.ensureVisible(save);
        await tester.tap(save);
        await tester.pump();
        await _waitFor(
          tester,
          () => delayed!.committed != null || delayed.firstFailure != null,
        );
        if (delayed.firstFailure != null) {
          fail('S1 host save failed: ${delayed.firstFailure}');
        }
        debugPrint('native draft: S1 committed and acknowledged');
        expect(delayed.saves, 1);
        expect(delayed.committed!.title, 'S1 title');
        final savedAsset = delayed.committed!.attachments.single.pluginId!;
        expect(savedAsset, isNotEmpty);
        expect(
          delayed.committed!.description,
          contains('attachment:$savedAsset'),
        );
        await tester.runAsync(() => sourceFile.delete());
        expect(
          await _hostFuture(
            tester,
            host!.versionedContent.read('editor-draft-native'),
          ),
          isA<VersionedContentRecord>().having(
            (record) => record.revision,
            'revision',
            BigInt.from(3),
          ),
        );
        expect(
          await _hostFuture(
            tester,
            host!.inspectEditorRecoveries(id: 'editor-draft-native'),
          ),
          isEmpty,
        ); // The real adapter acknowledged S1.

        expect(tester.widget<TextField>(title).readOnly, isFalse);
        expect(tester.widget<TextField>(description).readOnly, isFalse);
        await tester.enterText(title, 'S2 title');
        await tester.enterText(description, 'S2 raw body [asset]($localAlias)');
        final descriptionController = tester
            .widget<TextField>(description)
            .controller!;
        expect(descriptionController.text, contains('S2 raw body'));
        debugPrint('native draft: S2 typed; release S1 result');
        delayed.release.complete();
        await _waitFor(
          tester,
          () =>
              reopenedScopes == 1 &&
              (tester.widget<FilledButton>(save).onPressed != null ||
                  find
                      .byKey(const ValueKey('idea-save-error'))
                      .evaluate()
                      .isNotEmpty),
        );
        await tester.pumpAndSettle();
        expect(find.byKey(const ValueKey('idea-save-error')), findsNothing);
        expect(tester.widget<FilledButton>(save).onPressed, isNotNull);
        debugPrint('native draft: continuation settled');
        expect(delayed.continuations, 1);
        expect(find.byType(NewIdeaDialog), findsOneWidget);
        expect(returned, isNull);
        expect(tester.widget<TextField>(title).controller!.text, 'S2 title');
        expect(
          descriptionController.text,
          'S2 raw body [asset](attachment:$savedAsset)',
        );
        final afterS1 = await _hostFuture(
          tester,
          host!.versionedContent.read('editor-draft-native'),
        );
        expect(afterS1.revision, BigInt.from(3));
        expect(afterS1.tasks.map((task) => task.id), taskIds);
        expect(afterS1.tasks.map((task) => task.completion), taskCompletion);

        await tester.ensureVisible(save);
        await tester.tap(save);
        await tester.pump();
        await _waitFor(tester, () => returned != null);
        await tester.pumpAndSettle();
        debugPrint('native draft: S2 committed');
        expect(find.byType(NewIdeaDialog), findsNothing);
        expect(returned!.title, 'S2 title');
        expect(
          returned!.description,
          'S2 raw body [asset](attachment:$savedAsset)',
        );
        expect(returned!.attachments.single.pluginId, savedAsset);
        expect(
          await tester.runAsync(
            () => File(
              returned!.attachments.single.source.location,
            ).readAsBytes(),
          ),
          attachmentBytes,
        );
        expect(delayed.saves, 1); // S1 was never submitted again.
        final afterS2 = await _hostFuture(
          tester,
          host!.versionedContent.read('editor-draft-native'),
        );
        expect(afterS2.revision, BigInt.from(4));
        expect(afterS2.title, 'S2 title');
        expect(
          afterS2.description,
          'S2 raw body [asset](attachment:$savedAsset)',
        );
        expect(afterS2.assets.single.id, savedAsset);
        expect(afterS2.tasks.map((task) => task.id), taskIds);
        expect(afterS2.tasks.map((task) => task.completion), taskCompletion);
        expect(tester.takeException(), isNull);
      } finally {
        if (delayed != null && !delayed.release.isCompleted) {
          delayed.release.complete();
          await tester.pump();
        }
        await tester.pumpWidget(const SizedBox());
        await tester.runAsync(() async {
          await initialSession?.close();
          await host?.close();
          await directory.delete(recursive: true);
        });
      }
    },
    skip: !_available,
    timeout: const Timeout(Duration(minutes: 4)),
  );
}
