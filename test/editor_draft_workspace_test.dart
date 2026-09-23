import 'dart:async';

import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/editor_draft.dart';
import 'package:morrow_studio/plugins/editor_draft_session.dart';
import 'package:morrow_studio/plugins/editor_draft_workspace.dart';

EditorDraftTextValue _text(String value) => EditorDraftTextValue(
  text: value,
  selectionBase: -1,
  selectionExtent: -1,
  affinity: 1,
  directional: false,
  composingStart: -1,
  composingEnd: -1,
);

EditorDraftSnapshot _snapshot(String title) => EditorDraftSnapshot(
  values: EditorDraftValues(
    title: _text(title),
    description: _text(''),
    hypothesis: _text(''),
    conclusion: _text(''),
    todos: _text(''),
    category: '',
    stage: '',
  ),
  assets: const [],
);

EditorDraftRecord _receipt(EditorDraftWriteRequest request) {
  final generation = request.expectedGeneration + BigInt.one;
  return EditorDraftRecord(
    request: request,
    generation: generation,
    active: true,
    currentGeneration: generation,
    currentActive: true,
    repeated: false,
    sourceFormat: 2,
    sourceRevision: request.sourceRevision,
    sourceSha256: List<int>.filled(32, 7),
    predecessorRevision: BigInt.zero,
    predecessorSha256: const [],
    assets: const [],
  );
}

final class _Save {
  _Save(this.request);
  final EditorDraftWriteRequest request;
  final completion = Completer<EditorDraftRecord>();
}

final class _Control implements EditorDraftControl {
  final saves = <_Save>[];

  @override
  Future<EditorDraftRecord> save(EditorDraftWriteRequest request) {
    final item = _Save(request);
    saves.add(item);
    return item.completion.future;
  }

  @override
  Future<EditorDraftRecord?> read(String cardId, String draftId) async => null;
  @override
  Future<List<EditorDraftSummary>> list() async => const [];
  @override
  Future<EditorDraftRecord> discard(
    String cardId,
    String draftId,
    BigInt expectedGeneration,
    String operation,
  ) => throw UnimplementedError();
  @override
  Future<EditorDraftImportedAsset> importAsset(
    String cardId,
    String draftId,
    BigInt generation,
    String path,
    String name,
    String kind,
    BigInt bytes,
  ) => throw UnimplementedError();
  @override
  Future<void> exportAsset(
    String cardId,
    String draftId,
    BigInt generation,
    String assetId,
    String path,
  ) => throw UnimplementedError();
}

EditorDraftSession _session(
  _Control control,
  String id, {
  required bool Function() isCurrent,
  Duration debounce = const Duration(days: 1),
}) {
  var operation = 0;
  return EditorDraftSession.newSession(
    control: control,
    cardId: id,
    draftId: 'draft-$id',
    sourceRevision: BigInt.one,
    initialSnapshot: _snapshot('baseline-$id'),
    operationFactory: () => '$id-op-${++operation}',
    isCurrent: isCurrent,
    debounce: debounce,
  );
}

Future<void> _tick() => Future<void>.delayed(Duration.zero);

Future<EditorDraftRecord> _confirmInitial(
  EditorDraftSession session,
  _Control control,
) async {
  final first = session.ensureJournal();
  await _tick();
  final saved = control.saves.last;
  saved.completion.complete(_receipt(saved.request));
  return first;
}

void main() {
  test('releasing a view retains newer S2 and an unknown original', () async {
    final control = _Control();
    final workspace = EditorDraftWorkspace(isCurrent: () => true);
    final session = _session(control, 'card-a', isCurrent: () => true);
    addTearDown(session.dispose);
    final lease = workspace.attach(session);
    session.observe(_snapshot('S1'));
    final first = session.flush();
    await _tick();
    final original = control.saves.single.request;
    session.observe(_snapshot('S2'));
    lease.release();
    lease.release();
    expect(workspace.attachedViews, 0);
    expect(workspace.find('card-a', 'draft-card-a'), same(session));
    final unknown = EditorDraftSaveFailure(
      request: original,
      outcomeUnknown: true,
      cause: StateError('lost reply'),
    );
    control.saves.single.completion.completeError(unknown);
    await expectLater(first, throwsA(same(unknown)));
    expect(session.current.values.title.text, 'S2');
    expect(session.pendingRequest, same(original));
    expect(workspace.evictDurable('card-a', 'draft-card-a'), isFalse);
    expect(workspace.find('card-a', 'draft-card-a'), same(session));
  });

  test('one scope owns one session and one active view under capacity', () {
    final workspace = EditorDraftWorkspace(isCurrent: () => true, capacity: 1);
    final control = _Control();
    final first = _session(control, 'card-a', isCurrent: () => true);
    final foreign = _session(control, 'card-a', isCurrent: () => true);
    final other = _session(control, 'card-b', isCurrent: () => true);
    addTearDown(first.dispose);
    addTearDown(foreign.dispose);
    addTearDown(other.dispose);
    final lease = workspace.attach(first);
    expect(() => workspace.attach(first), throwsStateError);
    expect(() => workspace.attach(foreign), throwsStateError);
    expect(() => workspace.attach(other), throwsStateError);
    expect(workspace.sessions, [same(first)]);
    lease.release();
    final reopened = workspace.attach(first);
    expect(workspace.attachedViews, 1);
    reopened.release();
    expect(workspace.sessions, [same(first)]);
    expect(
      () => EditorDraftWorkspace(isCurrent: () => true, capacity: 0),
      throwsArgumentError,
    );
    expect(
      () => EditorDraftWorkspace(isCurrent: () => true, capacity: 17),
      throwsArgumentError,
    );
  });

  test(
    'attach publishes its scope before synchronous listener notification',
    () {
      final workspace = EditorDraftWorkspace(isCurrent: () => true);
      final session = _session(_Control(), 'card-a', isCurrent: () => true);
      addTearDown(session.dispose);
      EditorDraftSession? seen;
      Object? reentry;
      session.addListener(() {
        seen = workspace.find('card-a', 'draft-card-a');
        try {
          workspace.attach(session);
        } catch (error) {
          reentry = error;
        }
      });
      final lease = workspace.attach(session);
      expect(seen, same(session));
      expect(reentry, isA<StateError>());
      expect(workspace.attachedViews, 1);
      lease.release();
    },
  );

  test('only an unattached, current, clean journal may be evicted', () async {
    final control = _Control();
    final workspace = EditorDraftWorkspace(isCurrent: () => true);
    final session = _session(control, 'card-a', isCurrent: () => true);
    addTearDown(session.dispose);
    final lease = workspace.attach(session);
    expect(workspace.evictDurable('card-a', 'draft-card-a'), isFalse);
    final initial = session.ensureJournal();
    await _tick();
    expect(workspace.evictDurable('card-a', 'draft-card-a'), isFalse);
    control.saves.single.completion.complete(
      _receipt(control.saves.single.request),
    );
    await initial;
    expect(workspace.evictDurable('card-a', 'draft-card-a'), isFalse);
    lease.release();
    session.observe(_snapshot('newer'));
    expect(workspace.evictDurable('card-a', 'draft-card-a'), isFalse);
    final second = session.flush();
    await _tick();
    expect(workspace.evictDurable('card-a', 'draft-card-a'), isFalse);
    control.saves.last.completion.complete(
      _receipt(control.saves.last.request),
    );
    await second;
    expect(workspace.evictDurable('card-a', 'draft-card-a'), isTrue);
    expect(workspace.find('card-a', 'draft-card-a'), isNull);
    expect(session.disposed, isTrue);
  });

  test(
    'prepare waits S1, saves S2 once, and finish requires lease release',
    () async {
      final control = _Control();
      final workspace = EditorDraftWorkspace(isCurrent: () => true);
      final session = _session(control, 'card-a', isCurrent: () => true);
      final lease = workspace.attach(session);
      session.observe(_snapshot('S1'));
      final first = session.flush();
      await _tick();
      session.observe(_snapshot('S2'));
      final prepared = workspace.prepareClose(
        timeout: const Duration(seconds: 2),
      );
      expect(workspace.preparingClose, isTrue);
      expect(workspace.evictDurable('card-a', 'draft-card-a'), isFalse);
      control.saves.single.completion.complete(
        _receipt(control.saves.single.request),
      );
      await first;
      await _tick();
      expect(control.saves, hasLength(2));
      expect(control.saves.last.request.values.title.text, 'S2');
      control.saves.last.completion.complete(
        _receipt(control.saves.last.request),
      );
      final token = await prepared;
      expect(token.records.single.generation, BigInt.two);
      expect(control.saves, hasLength(2));
      expect(() => workspace.finishClose(token), throwsStateError);
      expect(workspace.preparingClose, isTrue);
      lease.release();
      workspace.finishClose(token);
      expect(workspace.disposed, isTrue);
      expect(session.disposed, isTrue);
    },
  );

  test(
    'failed finish keeps token live until explicit cancel restores policy',
    () async {
      final workspace = EditorDraftWorkspace(isCurrent: () => true);
      final session = _session(_Control(), 'card-a', isCurrent: () => true);
      addTearDown(session.dispose);
      final lease = workspace.attach(session);
      final token = await workspace.prepareClose(
        timeout: const Duration(seconds: 1),
      );
      expect(() => workspace.finishClose(token), throwsStateError);
      expect(workspace.preparingClose, isTrue);
      workspace.cancelClose(token);
      expect(workspace.preparingClose, isFalse);
      expect(session.autoSavePaused, isFalse);
      expect(session.disposed, isFalse);
      lease.release();
      final next = await workspace.prepareClose(
        timeout: const Duration(seconds: 1),
      );
      workspace.finishClose(next);
      expect(workspace.disposed, isTrue);
    },
  );

  test(
    'input after prepare invalidates finish without deleting that input',
    () async {
      final control = _Control();
      final workspace = EditorDraftWorkspace(isCurrent: () => true);
      final session = _session(control, 'card-a', isCurrent: () => true);
      addTearDown(session.dispose);
      final lease = workspace.attach(session);
      await _confirmInitial(session, control);
      final token = await workspace.prepareClose(
        timeout: const Duration(seconds: 1),
      );
      session.observe(_snapshot('late S2'));
      lease.release();
      expect(() => workspace.finishClose(token), throwsStateError);
      expect(workspace.preparingClose, isTrue);
      workspace.cancelClose(token);
      expect(session.current.values.title.text, 'late S2');
      expect(session.dirty, isTrue);
      expect(session.autoSavePaused, isFalse);
      expect(session.disposed, isFalse);
    },
  );

  test(
    'earlier draft change during later save fails the whole preparation',
    () async {
      final controlA = _Control(), controlB = _Control();
      final workspace = EditorDraftWorkspace(isCurrent: () => true);
      final first = _session(controlA, 'card-a', isCurrent: () => true);
      final second = _session(controlB, 'card-b', isCurrent: () => true);
      addTearDown(first.dispose);
      addTearDown(second.dispose);
      workspace.attach(first);
      workspace.attach(second);
      await _confirmInitial(first, controlA);
      second.observe(_snapshot('B1'));
      final prepared = workspace.prepareClose(
        timeout: const Duration(seconds: 2),
      );
      await _tick();
      expect(controlB.saves, hasLength(1));
      first.observe(_snapshot('A2'));
      controlB.saves.single.completion.complete(
        _receipt(controlB.saves.single.request),
      );
      await expectLater(prepared, throwsStateError);
      expect(workspace.preparingClose, isFalse);
      expect(first.current.values.title.text, 'A2');
      expect(first.dirty, isTrue);
      expect(second.confirmed!.request.values.title.text, 'B1');
      expect(second.dirty, isFalse);
    },
  );

  test(
    'timeout and unknown preserve original operation without resending',
    () async {
      final control = _Control();
      final workspace = EditorDraftWorkspace(isCurrent: () => true);
      final session = _session(control, 'card-a', isCurrent: () => true);
      addTearDown(session.dispose);
      workspace.attach(session);
      session.observe(_snapshot('S1'));
      final first = session.flush();
      await _tick();
      final original = control.saves.single.request;
      session.observe(_snapshot('S2'));
      await expectLater(
        workspace.prepareClose(timeout: const Duration(milliseconds: 10)),
        throwsA(isA<TimeoutException>()),
      );
      expect(workspace.preparingClose, isFalse);
      expect(session.pendingRequest, same(original));
      expect(control.saves, hasLength(1));
      final unknown = EditorDraftSaveFailure(
        request: original,
        outcomeUnknown: true,
        cause: StateError('lost finish reply'),
      );
      control.saves.single.completion.completeError(unknown);
      await expectLater(first, throwsA(same(unknown)));
      await expectLater(
        workspace.prepareClose(timeout: const Duration(seconds: 1)),
        throwsA(same(unknown)),
      );
      expect(session.current.values.title.text, 'S2');
      expect(control.saves, hasLength(1));
    },
  );

  test(
    'switch failure preserves original error and old timers stay paused',
    () async {
      final control = _Control();
      var current = true;
      final workspace = EditorDraftWorkspace(isCurrent: () => current);
      final session = _session(control, 'card-a', isCurrent: () => current);
      addTearDown(session.dispose);
      workspace.attach(session);
      session.observe(_snapshot('S1'));
      final first = session.flush();
      await _tick();
      final prepared = workspace.prepareClose(
        timeout: const Duration(seconds: 1),
      );
      Object? original;
      Object? preparationFailure;
      final firstObserved = first.then<void>(
        (_) => throw StateError('Unexpected successful save'),
        onError: (Object error) {
          original = error;
        },
      );
      final preparationObserved = prepared.then<void>(
        (_) => throw StateError('Unexpected successful preparation'),
        onError: (Object error) {
          preparationFailure = error;
        },
      );
      current = false;
      control.saves.single.completion.complete(
        _receipt(control.saves.single.request),
      );
      await firstObserved;
      await preparationObserved;
      expect(original, isA<EditorDraftSaveFailure>());
      expect(preparationFailure, same(original));
      expect(workspace.preparingClose, isFalse);
      expect(session.autoSavePaused, isTrue);
      expect(control.saves, hasLength(1));
    },
  );

  test(
    'pause failure restores prior policy and clears closing state',
    () async {
      final workspace = EditorDraftWorkspace(isCurrent: () => true);
      final first = _session(_Control(), 'card-a', isCurrent: () => true);
      final second = _session(_Control(), 'card-b', isCurrent: () => true);
      addTearDown(first.dispose);
      workspace.attach(first);
      workspace.attach(second);
      second.dispose();
      await expectLater(
        workspace.prepareClose(timeout: const Duration(seconds: 1)),
        throwsStateError,
      );
      expect(workspace.preparingClose, isFalse);
      expect(first.autoSavePaused, isFalse);
      expect(first.disposed, isFalse);
    },
  );

  test(
    'timer restoration blocks reentrant prepare until all sessions resume',
    () async {
      final workspace = EditorDraftWorkspace(isCurrent: () => true);
      final first = _session(_Control(), 'card-a', isCurrent: () => true);
      final second = _session(_Control(), 'card-b', isCurrent: () => true);
      addTearDown(first.dispose);
      addTearDown(second.dispose);
      workspace.attach(first);
      workspace.attach(second);
      Object? reentryFailure;
      var reentrySucceeded = false;
      first.addListener(() {
        if (!first.autoSavePaused &&
            workspace.preparingClose &&
            reentryFailure == null &&
            !reentrySucceeded) {
          workspace
              .prepareClose(timeout: const Duration(seconds: 1))
              .then<void>(
                (_) => reentrySucceeded = true,
                onError: (Object error) {
                  reentryFailure = error;
                },
              );
        }
      });
      final token = await workspace.prepareClose(
        timeout: const Duration(seconds: 1),
      );
      workspace.cancelClose(token);
      await _tick();
      expect(reentryFailure, isA<StateError>());
      expect(reentrySucceeded, isFalse);
      expect(first.autoSavePaused, isFalse);
      expect(second.autoSavePaused, isFalse);
      expect(workspace.preparingClose, isFalse);
    },
  );
  test(
    'workspace switch during resume leaves remaining old timers paused',
    () async {
      var current = true;
      final workspace = EditorDraftWorkspace(isCurrent: () => current);
      final first = _session(_Control(), 'card-a', isCurrent: () => true);
      final second = _session(_Control(), 'card-b', isCurrent: () => true);
      addTearDown(first.dispose);
      addTearDown(second.dispose);
      workspace.attach(first);
      workspace.attach(second);
      final token = await workspace.prepareClose(
        timeout: const Duration(seconds: 1),
      );
      first.addListener(() {
        if (!first.autoSavePaused) current = false;
      });
      workspace.cancelClose(token);
      expect(first.autoSavePaused, isFalse);
      expect(second.autoSavePaused, isTrue);
      expect(workspace.preparingClose, isFalse);
    },
  );
}
