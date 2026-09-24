import 'dart:async';

import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/main.dart' show Idea;
import 'package:morrow_studio/plugins/editor_recovery.dart';
import 'package:morrow_studio/plugins/editor_session.dart';
import 'package:morrow_studio/plugins/versioned_content.dart';
import 'package:morrow_studio/plugins/versioned_editor_adapter.dart';
import 'package:morrow_studio/plugins/versioned_idea_view.dart';

import 'versioned_editor_continuation_test.dart' as fixture;

final _digest = List<int>.filled(32, 7);

EditorRecovery _proof({BigInt? currentRevision}) => EditorRecovery(
  id: 'card-v2',
  title: 'Edited',
  operation: 'original-operation',
  digest: _digest,
  sourceRevision: BigInt.from(7),
  currentRevision: currentRevision ?? BigInt.from(8),
  status: EditorRecoveryStatus.committed,
);

class _ObservedSession extends fixture.OriginalSession
    implements VersionedEditorCommitObservation, EditorDraftPredecessorSource {
  Future<EditorRecovery> Function()? onObserve;
  int observations = 0;
  EditorRecovery? _observed;

  @override
  EditorRecovery? get draftPredecessor => _observed;

  @override
  Future<EditorRecovery> observePresented(
    VersionedCommitReceipt receipt,
  ) async {
    observations++;
    expect(receipt.id, 'card-v2');
    expect(receipt.operation, 'original-operation');
    expect(receipt.revision, BigInt.from(8));
    final value = await (onObserve?.call() ?? Future.value(_proof()));
    _observed ??= value;
    return value;
  }
}

VersionedWorkbenchEditorAdapter _adapter(
  fixture.OriginalSession session, {
  bool Function()? isCurrent,
  Future<WorkbenchEditorSession> Function(Idea)? reopen,
  Future<Idea> Function(VersionedContentRecord)? present,
}) => VersionedWorkbenchEditorAdapter(
  session,
  VersionedIdeaView.fromRecord(fixture.record(7)),
  present ?? (current) async => fixture.idea(current),
  isCurrent: isCurrent,
  reopen: reopen,
  deferRecoveryAcknowledgement: true,
);

void main() {
  test(
    'deferred save observes without cleanup and explicit ack works after continue',
    () async {
      final session = _ObservedSession()
        ..onSave = () async => fixture.committed();
      final next = fixture.SuccessorSession('card-v2');
      final editor = _adapter(session, reopen: (_) async => next);
      final confirmed = await editor.save(
        fixture.idea(fixture.record(7)),
        fixture.fields,
      );
      expect(session.saves, 1);
      expect(session.observations, 1);
      expect(session.acknowledgements, 0);
      expect(editor.draftPredecessor!.digest, _digest);
      expect(await editor.continueAfterCommit(confirmed), same(next));
      expect(session.closes, 1);
      expect(session.acknowledgements, 0);
      await editor.acknowledgeAccepted(fixture.committed().receipt);
      expect(session.acknowledgements, 1);
      expect(session.onAcknowledge, isNull);
      await editor.acknowledgeAccepted(fixture.committed().receipt);
      expect(session.acknowledgements, 1);
    },
  );

  test(
    'failed presentation and changed intent do not observe or clear',
    () async {
      final session = _ObservedSession()
        ..onSave = () async => fixture.committed();
      var fail = true;
      final editor = _adapter(
        session,
        present: (current) async {
          if (fail) throw StateError('view export failed');
          return fixture.idea(current);
        },
      );
      await expectLater(
        editor.save(fixture.idea(fixture.record(7)), fixture.fields),
        throwsA(isA<VersionedEditorPresentationFailure>()),
      );
      expect(session.observations, 0);
      expect(session.acknowledgements, 0);
      await expectLater(
        editor.save(
          fixture.idea(fixture.record(7))..title = 'Changed',
          fixture.fields,
        ),
        throwsStateError,
      );
      fail = false;
      await editor.save(fixture.idea(fixture.record(7)), fixture.fields);
      expect(session.saves, 2);
      expect(session.observations, 1);
      expect(session.acknowledgements, 0);
    },
  );

  test('observation failure keeps original intent and never clears', () async {
    final session = _ObservedSession()
      ..onSave = () async => fixture.committed();
    var fail = true;
    session.onObserve = () async {
      if (fail) throw StateError('read-only proof unavailable');
      return _proof();
    };
    final editor = _adapter(session);
    await expectLater(
      editor.save(fixture.idea(fixture.record(7)), fixture.fields),
      throwsA(isA<VersionedEditorPresentationFailure>()),
    );
    expect(session.acknowledgements, 0);
    fail = false;
    await editor.save(fixture.idea(fixture.record(7)), fixture.fields);
    expect(session.saves, 2);
    expect(session.observations, 2);
    expect(session.acknowledgements, 0);
  });

  test(
    'late workspace switch during observation cannot accept or clear',
    () async {
      final session = _ObservedSession()
        ..onSave = () async => fixture.committed();
      final waiting = Completer<EditorRecovery>();
      session.onObserve = () => waiting.future;
      var current = true;
      final editor = _adapter(session, isCurrent: () => current);
      final pending = editor.save(
        fixture.idea(fixture.record(7)),
        fixture.fields,
      );
      await Future<void>.delayed(Duration.zero);
      expect(session.observations, 1);
      current = false;
      waiting.complete(_proof());
      await expectLater(
        pending,
        throwsA(isA<VersionedEditorPresentationFailure>()),
      );
      expect(session.acknowledgements, 0);
      expect(editor.draftPredecessor, isNull);
      await expectLater(
        editor.acknowledgeAccepted(fixture.committed().receipt),
        throwsStateError,
      );
    },
  );

  test(
    'successor opening failure leaves proof pending for explicit cleanup',
    () async {
      final session = _ObservedSession()
        ..onSave = () async => fixture.committed();
      final editor = _adapter(
        session,
        reopen: (_) async => throw StateError('successor unavailable'),
      );
      final confirmed = await editor.save(
        fixture.idea(fixture.record(7)),
        fixture.fields,
      );
      await expectLater(
        editor.continueAfterCommit(confirmed),
        throwsStateError,
      );
      expect(session.acknowledgements, 0);
      expect(editor.draftPredecessor, isNotNull);
    },
  );

  test('unknown explicit ack retries only the accepted operation', () async {
    final session = _ObservedSession()
      ..onSave = () async => fixture.committed();
    var fail = true;
    session.onAcknowledge = () async {
      if (fail) throw StateError('ack reply lost');
    };
    final editor = _adapter(session);
    await editor.save(fixture.idea(fixture.record(7)), fixture.fields);
    final exact = fixture.committed().receipt;
    await expectLater(editor.acknowledgeAccepted(exact), throwsStateError);
    expect(session.acknowledgements, 1);
    expect(session.saves, 1);
    expect(session.observations, 1);
    await expectLater(
      editor.acknowledgeAccepted(
        VersionedCommitReceipt(
          id: exact.id,
          operation: 'different-operation',
          revision: exact.revision,
          repeated: false,
        ),
      ),
      throwsStateError,
    );
    fail = false;
    await editor.acknowledgeAccepted(exact);
    expect(session.acknowledgements, 2);
    expect(session.saves, 1);
    expect(session.observations, 1);
  });

  test(
    'late explicit acknowledgement remains known after close and switch',
    () async {
      final session = _ObservedSession()
        ..onSave = () async => fixture.committed();
      final waiting = Completer<void>();
      session.onAcknowledge = () => waiting.future;
      var current = true;
      final editor = _adapter(session, isCurrent: () => current);
      await editor.save(fixture.idea(fixture.record(7)), fixture.fields);
      final exact = fixture.committed().receipt;
      final pending = editor.acknowledgeAccepted(exact);
      await Future<void>.delayed(Duration.zero);
      expect(session.acknowledgements, 1);
      await editor.close();
      current = false;
      waiting.complete();
      await expectLater(pending, throwsStateError);
      expect(session.acknowledgements, 1);
      current = true;
      await editor.acknowledgeAccepted(exact);
      expect(session.acknowledgements, 1);
    },
  );

  test(
    'default mode acknowledges historical commit after newer presentation',
    () async {
      final session = _ObservedSession()
        ..onSave = () async => fixture.committed();
      final editor = VersionedWorkbenchEditorAdapter(
        session,
        VersionedIdeaView.fromRecord(fixture.record(7)),
        (_) async => fixture.idea(fixture.record(9)),
      );
      final shown = await editor.save(
        fixture.idea(fixture.record(7)),
        fixture.fields,
      );
      expect(shown.contentRevision, BigInt.from(9));
      expect(session.observations, 0);
      expect(session.acknowledgements, 1);
      expect(editor.draftPredecessor, isNull);
    },
  );

  test('deferred observation refuses an advanced recovery baseline', () async {
    final session = _ObservedSession()
      ..onSave = () async => fixture.committed();
    session.onObserve = () async => _proof(currentRevision: BigInt.from(9));
    final editor = _adapter(session);
    await expectLater(
      editor.save(fixture.idea(fixture.record(7)), fixture.fields),
      throwsA(isA<VersionedEditorPresentationFailure>()),
    );
    expect(session.observations, 1);
    expect(session.acknowledgements, 0);
    expect(editor.draftPredecessor, isNull);
  });

  test('recovery matching separates exact observation from historical ack', () {
    final receipt = fixture.committed().receipt;
    final advanced = _proof(currentRevision: BigInt.from(9));
    expect(
      advanced.matchesPresentedReceipt(
        receipt,
        sourceRevision: BigInt.from(7),
        exactCurrentRevision: false,
      ),
      isTrue,
    );
    expect(
      advanced.matchesPresentedReceipt(
        receipt,
        sourceRevision: BigInt.from(7),
        exactCurrentRevision: true,
      ),
      isFalse,
    );
  });
  test(
    'deferred mode rejects sessions without read-only observation',
    () async {
      final session = fixture.OriginalSession()
        ..onSave = () async => fixture.committed();
      final editor = _adapter(session);
      await expectLater(
        editor.save(fixture.idea(fixture.record(7)), fixture.fields),
        throwsA(isA<VersionedEditorPresentationFailure>()),
      );
      expect(session.acknowledgements, 0);
    },
  );
}
