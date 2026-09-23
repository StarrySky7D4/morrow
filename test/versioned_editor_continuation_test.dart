import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/attachments/attachment.dart';
import 'package:morrow_studio/main.dart' show Idea;
import 'package:morrow_studio/plugins/editor_recovery.dart';
import 'package:morrow_studio/plugins/editor_session.dart';
import 'package:morrow_studio/plugins/studio_backend.dart';
import 'package:morrow_studio/plugins/versioned_content.dart';
import 'package:morrow_studio/plugins/versioned_editor.dart';
import 'package:morrow_studio/plugins/versioned_editor_adapter.dart';
import 'package:morrow_studio/plugins/versioned_idea_view.dart';

VersionedContentRecord record(int revision, {bool deleted = false}) =>
    VersionedContentRecord(
      id: 'card-v2',
      title: 'Edited',
      revision: BigInt.from(revision),
      formatVersion: 2,
      description: 'Edited body',
      category: '进行中',
      stage: '计划中',
      hypothesis: '',
      conclusion: '',
      favorite: true,
      assets: const [],
      icon: 0,
      color: 0xff8866aa,
      deleted: deleted,
      deletedAt: BigInt.zero,
      todos: const [],
      completed: const [],
      tasks: const [],
      origin: null,
      retiredTaskIds: const [],
      projectedStage: '计划中',
      completeCount: 0,
      incompleteCount: 0,
      ambiguousCount: 0,
    );

Idea idea(VersionedContentRecord content, {bool contentDeleted = false}) {
  final view = VersionedIdeaView.fromRecord(content);
  return Idea(
    content.title,
    content.description,
    view.category,
    Idea.icons[view.iconIndex],
    Color(view.colorArgb),
    id: view.id,
    stage: view.stage,
    favorite: view.favorite,
    hypothesis: content.hypothesis,
    conclusion: content.conclusion,
    contentRevision: view.revision,
    contentDeleted: contentDeleted || view.deleted,
    versioned: view,
  );
}

EditorFields get fields => EditorFields(
  title: ' Edited ',
  description: 'Edited body',
  hypothesis: '',
  conclusion: '',
  todos: '',
);

class FakeStudio implements StudioBackend {
  @override
  dynamic noSuchMethod(Invocation invocation) => super.noSuchMethod(invocation);
}

class OriginalSession
    implements VersionedEditorSession, VersionedEditorAcknowledgement {
  final studioBackend = FakeStudio();
  Future<VersionedMutationResult> Function()? onSave;
  Future<void> Function()? onAcknowledge;
  int saves = 0;
  int closes = 0;
  int acknowledgements = 0;

  @override
  String get targetId => 'card-v2';
  @override
  StudioBackend get studio => studioBackend;
  @override
  Future<void> recordPaste(PasteInsertion insertion) async {}
  @override
  Future<VersionedAsset> stageAttachment(IdeaAttachment attachment) =>
      Future.error(UnsupportedError('No attachments in this test'));
  @override
  Future<VersionedMutationResult> save(
    CardEditFields edit,
    EditorFields snapshot,
  ) {
    saves++;
    return onSave!();
  }

  @override
  Future<void> acknowledgePresented(VersionedCommitReceipt receipt) async {
    acknowledgements++;
    await onAcknowledge?.call();
  }

  @override
  Future<void> close() async {
    closes++;
  }
}

class SuccessorSession implements WorkbenchEditorSession {
  SuccessorSession(this.targetId);
  @override
  final String targetId;
  final studioBackend = FakeStudio();
  int closes = 0;

  @override
  StudioBackend get studio => studioBackend;
  @override
  Future<void> recordPaste(PasteInsertion insertion) async {}
  @override
  Future<Idea> save(Idea draft, EditorFields fields) =>
      Future.error(UnsupportedError('Not used by this test'));
  @override
  Future<void> close() async {
    closes++;
  }
}

VersionedMutationResult committed() => VersionedMutationResult(
  receipt: VersionedCommitReceipt(
    id: 'card-v2',
    operation: 'original-operation',
    revision: BigInt.from(8),
    repeated: false,
  ),
  current: record(8),
);

VersionedWorkbenchEditorAdapter adapter(
  OriginalSession original, {
  Future<WorkbenchEditorSession> Function(Idea)? reopen,
  Future<Idea> Function(VersionedContentRecord)? present,
  bool Function()? isCurrent,
}) => VersionedWorkbenchEditorAdapter(
  original,
  VersionedIdeaView.fromRecord(record(7)),
  present ?? (current) async => idea(current),
  reopen: reopen,
  isCurrent: isCurrent,
);

void main() {
  test(
    'successor opens once after acknowledgement and closes old afterward',
    () async {
      final original = OriginalSession()..onSave = () async => committed();
      final opened = Completer<WorkbenchEditorSession>();
      final next = SuccessorSession('card-v2');
      var opens = 0;
      final editor = adapter(
        original,
        reopen: (_) {
          opens++;
          return opened.future;
        },
      );
      await expectLater(
        editor.continueAfterCommit(idea(record(8))),
        throwsStateError,
      );
      final confirmed = await editor.save(idea(record(7)), fields);
      expect(original.saves, 1);
      expect(original.acknowledgements, 1);

      final first = editor.continueAfterCommit(confirmed);
      final second = editor.continueAfterCommit(confirmed);
      expect(identical(first, second), isTrue);
      expect(opens, 1);
      expect(original.closes, 0);
      opened.complete(next);
      expect(await first, same(next));
      expect(await second, same(next));
      expect(original.closes, 1);
      expect(original.saves, 1);
      await expectLater(editor.save(idea(record(7)), fields), throwsStateError);
    },
  );

  test('failed reopen retries without resubmitting original save', () async {
    final original = OriginalSession()..onSave = () async => committed();
    var opens = 0;
    final next = SuccessorSession('card-v2');
    final editor = adapter(
      original,
      reopen: (_) {
        opens++;
        if (opens == 1) throw StateError('fresh read unavailable');
        return Future.value(next);
      },
    );
    final confirmed = await editor.save(idea(record(7)), fields);
    await expectLater(editor.continueAfterCommit(confirmed), throwsStateError);
    expect(original.closes, 0);
    expect(original.saves, 1);
    expect(await editor.continueAfterCommit(confirmed), same(next));
    expect(opens, 2);
    expect(original.saves, 1);
    expect(original.closes, 1);
  });

  test(
    'unacknowledged and newer presentations cannot start a successor',
    () async {
      final original = OriginalSession();
      original.onSave = () async => committed();
      original.onAcknowledge = () async => throw StateError('ack reply lost');
      var opens = 0;
      final editor = adapter(
        original,
        reopen: (_) async {
          opens++;
          return SuccessorSession('card-v2');
        },
      );
      await expectLater(
        editor.save(idea(record(7)), fields),
        throwsA(isA<VersionedEditorPresentationFailure>()),
      );
      await expectLater(
        editor.continueAfterCommit(idea(record(8))),
        throwsStateError,
      );
      expect(opens, 0);

      original.onAcknowledge = () async {};
      final confirmed = await editor.save(idea(record(7)), fields);
      await expectLater(
        editor.continueAfterCommit(idea(record(9))),
        throwsStateError,
      );
      await expectLater(
        editor.continueAfterCommit(idea(record(8), contentDeleted: true)),
        throwsStateError,
      );
      expect(
        await editor.continueAfterCommit(confirmed),
        isA<SuccessorSession>(),
      );

      final newer = OriginalSession()..onSave = () async => committed();
      final stale = adapter(
        newer,
        present: (_) async => idea(record(9)),
        reopen: (_) async => SuccessorSession('card-v2'),
      );
      final later = await stale.save(idea(record(7)), fields);
      await expectLater(stale.continueAfterCommit(later), throwsStateError);
    },
  );

  test('failed original save cannot open a successor', () async {
    final original = OriginalSession()
      ..onSave = () => Future.error(StateError('NoCommit'));
    var opens = 0;
    final editor = adapter(
      original,
      reopen: (_) async {
        opens++;
        return SuccessorSession('card-v2');
      },
    );
    await expectLater(editor.save(idea(record(7)), fields), throwsStateError);
    await expectLater(
      editor.continueAfterCommit(idea(record(8))),
      throwsStateError,
    );
    expect(opens, 0);
    expect(original.saves, 1);
  });
  test('workspace switch closes newly opened stale successor', () async {
    final original = OriginalSession()..onSave = () async => committed();
    final opened = Completer<WorkbenchEditorSession>();
    final successor = SuccessorSession('card-v2');
    var current = true;
    final editor = adapter(
      original,
      isCurrent: () => current,
      reopen: (_) => opened.future,
    );
    final confirmed = await editor.save(idea(record(7)), fields);
    final transition = editor.continueAfterCommit(confirmed);
    current = false;
    opened.complete(successor);
    await expectLater(transition, throwsStateError);
    expect(successor.closes, 1);
    expect(original.closes, 0);
    expect(original.saves, 1);
  });
}
