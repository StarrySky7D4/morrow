import 'dart:async';

import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/editor_draft.dart';
import 'package:morrow_studio/plugins/editor_draft_codec.dart';
import 'package:morrow_studio/plugins/editor_draft_session.dart';

EditorDraftTextValue _text(
  String value, {
  int base = -1,
  int extent = -1,
  int composingStart = -1,
  int composingEnd = -1,
}) => EditorDraftTextValue(
  text: value,
  selectionBase: base,
  selectionExtent: extent,
  affinity: 1,
  directional: true,
  composingStart: composingStart,
  composingEnd: composingEnd,
);

EditorDraftSnapshot _snapshot(
  String title, {
  int base = -1,
  int extent = -1,
  int composingStart = -1,
  int composingEnd = -1,
  String description = '',
}) => EditorDraftSnapshot(
  values: EditorDraftValues(
    title: _text(
      title,
      base: base,
      extent: extent,
      composingStart: composingStart,
      composingEnd: composingEnd,
    ),
    description: _text(description),
    hypothesis: _text(''),
    conclusion: _text(''),
    todos: _text(''),
    category: '进行中',
    stage: '计划中',
  ),
  assets: const [],
);

EditorDraftRecord _receipt(
  EditorDraftWriteRequest request, {
  BigInt? currentGeneration,
  bool currentActive = true,
  bool repeated = false,
}) {
  final generation = request.expectedGeneration + BigInt.one;
  return EditorDraftRecord(
    request: request,
    generation: generation,
    active: true,
    currentGeneration: currentGeneration ?? generation,
    currentActive: currentActive,
    repeated: repeated,
    sourceFormat: request.sourceKind == EditorDraftSourceKind.newCard ? 0 : 2,
    sourceRevision: request.sourceRevision,
    sourceSha256: request.sourceKind == EditorDraftSourceKind.newCard
        ? const []
        : List.filled(32, 7),
    predecessorRevision: BigInt.zero,
    predecessorSha256: const [],
    assets: const [],
  );
}

final class _PendingSave {
  _PendingSave(this.request) : completion = Completer<EditorDraftRecord>();
  final EditorDraftWriteRequest request;
  final Completer<EditorDraftRecord> completion;
}

final class _DraftControl implements EditorDraftControl {
  final saves = <_PendingSave>[];
  Object? synchronousError;

  @override
  Future<EditorDraftRecord> save(EditorDraftWriteRequest request) {
    if (synchronousError case final error?) throw error;
    final pending = _PendingSave(request);
    saves.add(pending);
    return pending.completion.future;
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
  _DraftControl control, {
  bool Function()? isCurrent,
  EditorDraftSnapshot? initial,
  EditorDraftSourceKind sourceKind = EditorDraftSourceKind.existingCard,
}) {
  var nextOperation = 0;
  return EditorDraftSession.newSession(
    control: control,
    cardId: 'card-1',
    draftId: 'draft-1',
    sourceRevision: sourceKind == EditorDraftSourceKind.newCard
        ? BigInt.zero
        : BigInt.from(2),
    sourceKind: sourceKind,
    initialSnapshot: initial ?? _snapshot('A'),
    operationFactory: () => 'draft-op-${++nextOperation}',
    isCurrent: isCurrent ?? () => true,
    debounce: const Duration(days: 1),
  );
}

Future<void> _tick() => Future<void>.delayed(Duration.zero);

void main() {
  test(
    'delayed S1 keeps newer text, selection, and IME state for explicit S2',
    () async {
      final control = _DraftControl();
      final session = _session(control);
      addTearDown(session.dispose);
      final s1 = _snapshot('A😀B', base: 1, extent: 3);
      final s2 = _snapshot(
        'A😀B edited',
        base: 2,
        extent: 3,
        composingStart: 1,
        composingEnd: 4,
        description: 'new local text',
      );
      session.observe(s1);
      final first = session.flush();
      await _tick();
      expect(control.saves, hasLength(1));
      final original = control.saves.single.request;
      final originalBytes = EditorDraftCodec.encodeWrite(original);
      expect(original.values.title.text, 'A😀B');
      session.observe(s2);
      final queued = session.flush();
      await _tick();
      expect(
        control.saves,
        hasLength(1),
        reason: 'queued flush cannot submit S2',
      );
      expect(session.current.values.title.selectionBase, 2);
      expect(session.current.values.title.composingStart, 1);
      control.saves.single.completion.complete(_receipt(original));
      await first;
      await queued;
      expect(session.current.values.title.text, 'A😀B edited');
      expect(session.current.values.description.text, 'new local text');
      expect(session.dirty, isTrue);
      expect(session.confirmed!.request.operation, original.operation);
      expect(
        control.saves,
        hasLength(1),
        reason: 'S1 completion cannot auto-submit S2',
      );
      expect(EditorDraftCodec.encodeWrite(original), originalBytes);

      final second = session.flush();
      await _tick();
      expect(control.saves, hasLength(2));
      final successor = control.saves.last.request;
      expect(successor.operation, isNot(original.operation));
      expect(successor.expectedGeneration, BigInt.one);
      expect(successor.values.title.text, 'A😀B edited');
      expect(successor.values.title.selectionBase, 2);
      expect(successor.values.title.selectionExtent, 3);
      expect(successor.values.title.composingStart, 1);
      expect(successor.values.title.composingEnd, 4);
      control.saves.last.completion.complete(_receipt(successor));
      await second;
      expect(session.dirty, isFalse);
    },
  );

  test(
    'new card can explicitly persist an unchanged initial journal',
    () async {
      final control = _DraftControl();
      final session = _session(
        control,
        sourceKind: EditorDraftSourceKind.newCard,
      );
      addTearDown(session.dispose);
      expect(session.dirty, isTrue);
      final first = session.flush();
      await _tick();
      expect(control.saves, hasLength(1));
      final frozen = control.saves.single.request;
      expect(frozen.sourceKind, EditorDraftSourceKind.newCard);
      expect(frozen.expectedGeneration, BigInt.zero);
      control.saves.single.completion.complete(_receipt(frozen));
      await first;
      expect(session.dirty, isFalse);
      await session.flush();
      expect(control.saves, hasLength(1));
    },
  );
  test(
    'new-card session freezes source kind and restores without a card',
    () async {
      final control = _DraftControl();
      final session = _session(
        control,
        sourceKind: EditorDraftSourceKind.newCard,
      );
      addTearDown(session.dispose);
      expect(session.sourceRevision, BigInt.zero);
      expect(session.sourceKind, EditorDraftSourceKind.newCard);
      session.observe(_snapshot('new card text', base: 2, extent: 2));
      final saving = session.flush();
      await _tick();
      final frozen = control.saves.single.request;
      expect(frozen.sourceKind, EditorDraftSourceKind.newCard);
      expect(frozen.sourceRevision, BigInt.zero);
      expect(frozen.predecessorOperation, isEmpty);
      expect(frozen.predecessorDigest, isEmpty);
      final accepted = _receipt(frozen);
      control.saves.single.completion.complete(accepted);
      await saving;
      final restored = EditorDraftSession.restore(
        control: control,
        record: accepted,
        operationFactory: () => 'new-card-next',
        isCurrent: () => true,
        debounce: const Duration(days: 1),
      );
      addTearDown(restored.dispose);
      expect(restored.sourceKind, EditorDraftSourceKind.newCard);
      expect(restored.current.values.title.text, 'new card text');
      expect(restored.confirmed!.sourceFormat, 0);
      expect(await restored.flush(), same(accepted));
      expect(control.saves, hasLength(1));
      expect(
        () => EditorDraftSession.newSession(
          control: control,
          cardId: 'card-1',
          draftId: 'bad-kind-source',
          sourceRevision: BigInt.one,
          sourceKind: EditorDraftSourceKind.newCard,
          initialSnapshot: _snapshot('A'),
          operationFactory: () => 'bad-op',
          isCurrent: () => true,
        ),
        throwsFormatException,
      );
    },
  );
  test(
    'selection-only change persists once; equal snapshot never writes again',
    () async {
      final control = _DraftControl();
      final session = _session(
        control,
        initial: _snapshot('A😀B', base: 1, extent: 1),
      );
      addTearDown(session.dispose);
      expect(await session.flush(), isNull);
      expect(control.saves, isEmpty);
      session.observe(_snapshot('A😀B', base: 2, extent: 3));
      final first = session.flush();
      await _tick();
      expect(control.saves, hasLength(1));
      final frozen = control.saves.single.request;
      expect(frozen.values.title.text, 'A😀B');
      expect(frozen.values.title.selectionBase, 2);
      control.saves.single.completion.complete(_receipt(frozen));
      await first;
      expect(session.dirty, isFalse);
      session.observe(_snapshot('A😀B', base: 2, extent: 3));
      await session.flush();
      expect(control.saves, hasLength(1));
    },
  );

  test(
    'Unknown blocks queued and later saves until exact original retry',
    () async {
      final control = _DraftControl();
      final session = _session(control);
      addTearDown(session.dispose);
      session.observe(_snapshot('S1'));
      final first = session.flush();
      await _tick();
      final original = control.saves.single.request;
      final bytes = EditorDraftCodec.encodeWrite(original);
      session.observe(_snapshot('S2'));
      final unknown = EditorDraftSaveFailure(
        request: original,
        outcomeUnknown: true,
        cause: StateError('lost finish reply'),
      );
      control.saves.single.completion.completeError(unknown);
      await expectLater(first, throwsA(same(unknown)));
      expect(session.unknown, isTrue);
      await expectLater(session.flush(), throwsA(same(unknown)));
      expect(control.saves, hasLength(1));

      final retry = session.retryPending();
      await _tick();
      expect(control.saves, hasLength(2));
      expect(control.saves.last.request.operation, original.operation);
      expect(EditorDraftCodec.encodeWrite(control.saves.last.request), bytes);
      control.saves.last.completion.complete(
        _receipt(original, repeated: true),
      );
      await retry;
      expect(session.current.values.title.text, 'S2');
      expect(session.dirty, isTrue);
      expect(
        control.saves,
        hasLength(2),
        reason: 'retry cannot submit S2 automatically',
      );
    },
  );

  test(
    'rejected before submission can be corrected with a fresh operation',
    () async {
      final control = _DraftControl();
      final session = _session(control);
      addTearDown(session.dispose);
      session.observe(_snapshot('invalid local value'));
      final first = session.flush();
      await _tick();
      final original = control.saves.single.request;
      control.saves.single.completion.completeError(
        EditorDraftSaveFailure(
          request: original,
          outcomeUnknown: false,
          cause: const FormatException('invalid local value'),
        ),
      );
      await expectLater(first, throwsA(isA<EditorDraftSaveFailure>()));
      expect(session.unknown, isFalse);
      session.observe(_snapshot('corrected'));
      final second = session.flush();
      await _tick();
      expect(control.saves, hasLength(2));
      final corrected = control.saves.last.request;
      expect(corrected.operation, isNot(original.operation));
      expect(corrected.values.title.text, 'corrected');
      control.saves.last.completion.complete(_receipt(corrected));
      await second;
    },
  );

  test(
    'historical or inactive receipt cannot roll local generation back',
    () async {
      for (final inactive in [false, true]) {
        final control = _DraftControl();
        final session = _session(control);
        session.observe(_snapshot('S1'));
        final first = session.flush();
        await _tick();
        final original = control.saves.single.request;
        control.saves.single.completion.complete(
          _receipt(
            original,
            currentGeneration: BigInt.from(3),
            currentActive: !inactive,
            repeated: true,
          ),
        );
        await expectLater(first, throwsA(anything));
        expect(session.conflicted, isTrue);
        expect(session.confirmed, isNull);
        expect(session.current.values.title.text, 'S1');
        session.dispose();
      }
    },
  );

  test('restore is read-only and detach keeps the session alive', () async {
    final control = _DraftControl();
    final restoredRequest = EditorDraftWriteRequest(
      cardId: 'card-1',
      draftId: 'draft-1',
      operation: 'restored-op',
      expectedGeneration: BigInt.zero,
      sourceRevision: BigInt.from(2),
      predecessorOperation: '',
      predecessorDigest: const [],
      values: _snapshot('restored', base: 4, extent: 4).values,
      assets: const [],
    );
    var nextOperation = 0;
    final session = EditorDraftSession.restore(
      control: control,
      record: _receipt(restoredRequest),
      operationFactory: () => 'restored-next-${++nextOperation}',
      isCurrent: () => true,
      debounce: const Duration(days: 1),
    );
    addTearDown(session.dispose);
    expect(session.current.values.title.text, 'restored');
    expect(session.confirmed!.generation, BigInt.one);
    await session.flush();
    expect(control.saves, isEmpty);
    var notifications = 0;
    session.addListener(() => notifications++);
    session.detachUI();
    session.observe(_snapshot('new after detach'));
    expect(notifications, greaterThan(0));
    final pending = session.flush();
    await _tick();
    expect(control.saves, hasLength(1));
    expect(control.saves.single.request.expectedGeneration, BigInt.one);
    control.saves.single.completion.complete(
      _receipt(control.saves.single.request),
    );
    final beforeReceipt = notifications;
    await pending;
    expect(notifications, greaterThan(beforeReceipt));
  });

  test(
    'workspace invalidation and dispose stop late result from sending S2',
    () async {
      final control = _DraftControl();
      var current = true;
      final session = _session(control, isCurrent: () => current);
      session.observe(_snapshot('S1'));
      final first = session.flush();
      await _tick();
      final original = control.saves.single.request;
      session.observe(_snapshot('S2'));
      current = false;
      session.dispose();
      control.saves.single.completion.complete(_receipt(original));
      try {
        await first;
      } catch (_) {
        // A closed or replaced workspace may reject its late result.
      }
      await _tick();
      expect(control.saves, hasLength(1));
      expect(session.disposed, isTrue);
      expect(session.confirmed, isNull);
      expect(session.journalGeneration, BigInt.zero);
    },
  );
  test(
    'synchronous save throw clears in-flight state and retries frozen request',
    () async {
      final control = _DraftControl()
        ..synchronousError = StateError('sync transport failure');
      final session = _session(control);
      addTearDown(session.dispose);
      session.observe(_snapshot('S1'));
      await expectLater(
        session.flush(),
        throwsA(isA<EditorDraftSaveFailure>()),
      );
      expect(session.saving, isFalse);
      expect(session.unknown, isTrue);
      final frozen = session.pendingRequest!;
      final bytes = EditorDraftCodec.encodeWrite(frozen);
      control.synchronousError = null;
      final retry = session.retryPending();
      await _tick();
      expect(control.saves, hasLength(1));
      expect(control.saves.single.request.operation, frozen.operation);
      expect(EditorDraftCodec.encodeWrite(control.saves.single.request), bytes);
      control.saves.single.completion.complete(_receipt(frozen));
      await retry;
      expect(session.saving, isFalse);
      expect(session.unknown, isFalse);
    },
  );
  test(
    'foreign no-submit failure cannot clear this frozen operation',
    () async {
      final control = _DraftControl();
      var nextOperation = 0;
      final session = EditorDraftSession.newSession(
        control: control,
        cardId: 'card-1',
        draftId: 'draft-1',
        sourceRevision: BigInt.from(2),
        initialSnapshot: _snapshot('baseline'),
        operationFactory: () => 'draft-op-${++nextOperation}',
        isCurrent: () => true,
        debounce: const Duration(milliseconds: 10),
      );
      addTearDown(session.dispose);
      session.observe(_snapshot('S1'));
      final first = session.flush();
      await _tick();
      final original = control.saves.single.request;
      final foreign = EditorDraftWriteRequest(
        cardId: 'other-card',
        draftId: original.draftId,
        operation: 'other-operation',
        expectedGeneration: original.expectedGeneration,
        sourceRevision: original.sourceRevision,
        predecessorOperation: original.predecessorOperation,
        predecessorDigest: original.predecessorDigest,
        values: original.values,
        assets: original.assets,
      );
      control.saves.single.completion.completeError(
        EditorDraftSaveFailure(
          request: foreign,
          outcomeUnknown: false,
          cause: StateError('foreign request rejected before submit'),
        ),
      );
      await expectLater(first, throwsA(isA<EditorDraftSaveFailure>()));
      expect(session.unknown, isTrue);
      expect(session.pendingRequest, same(original));
      expect(session.lastFailure!.request, same(original));
      expect(session.lastFailure!.outcomeUnknown, isTrue);
      session.observe(_snapshot('S2'));
      await Future<void>.delayed(const Duration(milliseconds: 50));
      expect(
        control.saves,
        hasLength(1),
        reason: 'foreign claim cannot enable autosave',
      );
      await expectLater(session.flush(), throwsA(same(session.lastFailure)));
      expect(control.saves, hasLength(1));
    },
  );
  test(
    'replaced workspace ignores a late receipt without closing session',
    () async {
      final control = _DraftControl();
      var current = true;
      final session = _session(control, isCurrent: () => current);
      addTearDown(session.dispose);
      session.observe(_snapshot('S1'));
      final first = session.flush();
      await _tick();
      final original = control.saves.single.request;
      session.observe(_snapshot('S2'));
      current = false;
      control.saves.single.completion.complete(_receipt(original));
      await expectLater(first, throwsA(isA<EditorDraftSaveFailure>()));
      expect(session.confirmed, isNull);
      expect(session.journalGeneration, BigInt.zero);
      expect(session.current.values.title.text, 'S2');
      expect(session.pendingRequest, same(original));
      expect(control.saves, hasLength(1));
      await expectLater(session.flush(), throwsStateError);
    },
  );

  test(
    'dispose cancels debounce without pretending the draft was saved',
    () async {
      final control = _DraftControl();
      final session = EditorDraftSession.newSession(
        control: control,
        cardId: 'card-1',
        draftId: 'draft-1',
        sourceRevision: BigInt.from(2),
        initialSnapshot: _snapshot('baseline'),
        operationFactory: () => 'unused-operation',
        isCurrent: () => true,
        debounce: const Duration(milliseconds: 10),
      );
      session.observe(_snapshot('unsaved'));
      expect(session.dirty, isTrue);
      session.dispose();
      await Future<void>.delayed(const Duration(milliseconds: 50));
      expect(control.saves, isEmpty);
      expect(session.confirmed, isNull);
      expect(session.current.values.title.text, 'unsaved');
      expect(session.dirty, isTrue);
    },
  );
  test(
    'ensureJournal persists clean existing-card values exactly once',
    () async {
      final control = _DraftControl();
      final initial = EditorDraftSnapshot(
        values: _snapshot(
          'A😀B',
          base: 2,
          extent: 3,
          composingStart: 1,
          composingEnd: 4,
          description: 'unmodified original text',
        ).values,
        assets: [
          EditorDraftAssetSelection(
            origin: EditorDraftAssetOrigin.source,
            assetId: 'source-asset',
            aliases: ['local-preview'],
          ),
        ],
      );
      final session = _session(control, initial: initial);
      addTearDown(session.dispose);
      expect(await session.flush(), isNull);
      expect(control.saves, isEmpty);
      final first = session.ensureJournal();
      await _tick();
      expect(control.saves, hasLength(1));
      final request = control.saves.single.request;
      expect(request.expectedGeneration, BigInt.zero);
      expect(request.values.title.text, 'A😀B');
      expect(request.values.title.selectionBase, 2);
      expect(request.values.title.composingStart, 1);
      expect(request.values.description.text, 'unmodified original text');
      expect(request.assets.single.assetId, 'source-asset');
      control.saves.single.completion.complete(_receipt(request));
      final confirmed = await first;
      expect(confirmed.generation, BigInt.one);
      expect(session.dirty, isFalse);
      expect(await session.ensureJournal(), same(confirmed));
      expect(control.saves, hasLength(1));

      session.observe(_snapshot('new visible value'));
      expect(await session.ensureJournal(), same(confirmed));
      expect(session.dirty, isTrue);
      expect(control.saves, hasLength(1));
    },
  );

  test(
    'ensureJournal joins S1 and never retries an unknown original',
    () async {
      final control = _DraftControl();
      final session = _session(control);
      addTearDown(session.dispose);
      session.observe(_snapshot('S1'));
      final first = session.flush();
      await _tick();
      final original = control.saves.single.request;
      final bytes = EditorDraftCodec.encodeWrite(original);
      session.observe(_snapshot('S2'));
      final ensured = session.ensureJournal();
      await _tick();
      expect(control.saves, hasLength(1));
      final unknown = EditorDraftSaveFailure(
        request: original,
        outcomeUnknown: true,
        cause: StateError('lost reply'),
      );
      control.saves.single.completion.completeError(unknown);
      await expectLater(first, throwsA(same(unknown)));
      await expectLater(ensured, throwsA(same(unknown)));
      expect(session.pendingRequest, same(original));
      expect(session.current.values.title.text, 'S2');
      expect(EditorDraftCodec.encodeWrite(session.pendingRequest!), bytes);
      await expectLater(session.ensureJournal(), throwsA(same(unknown)));
      expect(control.saves, hasLength(1));
    },
  );

  test(
    'ensureJournal initializes a new-card draft at source revision zero',
    () async {
      final control = _DraftControl();
      final session = _session(
        control,
        sourceKind: EditorDraftSourceKind.newCard,
      );
      addTearDown(session.dispose);
      expect(session.dirty, isTrue);
      final first = session.ensureJournal();
      await _tick();
      expect(control.saves, hasLength(1));
      final request = control.saves.single.request;
      expect(request.expectedGeneration, BigInt.zero);
      expect(request.sourceRevision, BigInt.zero);
      expect(request.sourceKind, EditorDraftSourceKind.newCard);
      expect(request.predecessorOperation, isEmpty);
      expect(request.values.title.text, 'A');
      control.saves.single.completion.complete(_receipt(request));
      expect((await first).generation, BigInt.one);
      expect(session.dirty, isFalse);
      expect(control.saves, hasLength(1));
    },
  );

  test('handoff never retries an unknown frozen generation', () async {
    final control = _DraftControl();
    final session = _session(control);
    addTearDown(session.dispose);
    session.observe(_snapshot('S1'));
    final first = session.flush();
    await _tick();
    final original = control.saves.single.request;
    session.observe(_snapshot('S2'));
    final unknown = EditorDraftSaveFailure(
      request: original,
      outcomeUnknown: true,
      cause: StateError('lost finish reply'),
    );
    control.saves.single.completion.completeError(unknown);
    await expectLater(first, throwsA(same(unknown)));
    await expectLater(
      session.flushLatestVisible(timeout: const Duration(seconds: 1)),
      throwsA(same(unknown)),
    );
    expect(session.autoSavePaused, isTrue);
    expect(session.pendingRequest, same(original));
    expect(session.current.values.title.text, 'S2');
    expect(control.saves, hasLength(1));
  });

  test('pause and resume only control debounce, not explicit saves', () async {
    final control = _DraftControl();
    var next = 0;
    final session = EditorDraftSession.newSession(
      control: control,
      cardId: 'card-1',
      draftId: 'draft-1',
      sourceRevision: BigInt.from(2),
      initialSnapshot: _snapshot('baseline'),
      operationFactory: () => 'paused-op-${++next}',
      isCurrent: () => true,
      debounce: const Duration(milliseconds: 10),
    );
    addTearDown(session.dispose);
    session.observe(_snapshot('S1'));
    session.pauseAutoSave();
    expect(session.autoSavePaused, isTrue);
    await Future<void>.delayed(const Duration(milliseconds: 40));
    expect(control.saves, isEmpty);
    final explicit = session.flush();
    await _tick();
    expect(control.saves, hasLength(1));
    session.observe(_snapshot('S2'));
    control.saves.single.completion.complete(
      _receipt(control.saves.single.request),
    );
    await explicit;
    await Future<void>.delayed(const Duration(milliseconds: 40));
    expect(
      control.saves,
      hasLength(1),
      reason: 'send finally must respect paused autosave',
    );
    expect(session.dirty, isTrue);
    session.resumeAutoSave();
    expect(session.autoSavePaused, isFalse);
    await Future<void>.delayed(const Duration(milliseconds: 40));
    expect(control.saves, hasLength(2));
    control.saves.last.completion.complete(
      _receipt(control.saves.last.request),
    );
    await _tick();
    expect(session.dirty, isFalse);
  });

  test(
    'flushLatestVisible waits for S1 then saves the latest S2 once',
    () async {
      final control = _DraftControl();
      final session = _session(control);
      addTearDown(session.dispose);
      session.observe(_snapshot('S1'));
      final first = session.flush();
      await _tick();
      session.observe(_snapshot('S2'));
      final prepared = session.flushLatestVisible(
        timeout: const Duration(seconds: 2),
      );
      expect(session.autoSavePaused, isTrue);
      expect(control.saves, hasLength(1));
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
      final result = await prepared;
      expect(result!.generation, BigInt.two);
      expect(session.current.values.title.text, 'S2');
      expect(session.dirty, isFalse);
      expect(control.saves, hasLength(2));
      expect(session.autoSavePaused, isTrue);
    },
  );

  test('flushLatestVisible reports edits during its final save', () async {
    final control = _DraftControl();
    final session = _session(control);
    addTearDown(session.dispose);
    session.observe(_snapshot('S1'));
    final prepared = session.flushLatestVisible(
      timeout: const Duration(seconds: 2),
    );
    await _tick();
    expect(control.saves, hasLength(1));
    session.observe(_snapshot('S2'));
    control.saves.single.completion.complete(
      _receipt(control.saves.single.request),
    );
    await expectLater(prepared, throwsStateError);
    expect(session.confirmed!.request.values.title.text, 'S1');
    expect(session.current.values.title.text, 'S2');
    expect(session.dirty, isTrue);
    expect(control.saves, hasLength(1));
    expect(session.autoSavePaused, isTrue);
  });

  test(
    'flushLatestVisible timeout leaves original in flight and no new op',
    () async {
      final control = _DraftControl();
      final session = _session(control);
      addTearDown(session.dispose);
      session.observe(_snapshot('S1'));
      final first = session.flush();
      await _tick();
      final original = control.saves.single.request;
      session.observe(_snapshot('S2'));
      await expectLater(
        session.flushLatestVisible(timeout: const Duration(milliseconds: 10)),
        throwsA(isA<TimeoutException>()),
      );
      expect(session.autoSavePaused, isTrue);
      expect(session.saving, isTrue);
      expect(session.pendingRequest, same(original));
      expect(control.saves, hasLength(1));
      control.saves.single.completion.complete(_receipt(original));
      await first;
      await Future<void>.delayed(const Duration(milliseconds: 40));
      expect(control.saves, hasLength(1));
      expect(session.dirty, isTrue);
    },
  );

  test('nonpositive handoff timeout has no side effects', () async {
    final control = _DraftControl();
    final session = _session(control);
    addTearDown(session.dispose);
    session.observe(_snapshot('S1'));
    await expectLater(
      session.flushLatestVisible(timeout: Duration.zero),
      throwsArgumentError,
    );
    await expectLater(
      session.flushLatestVisible(timeout: const Duration(milliseconds: -1)),
      throwsArgumentError,
    );
    expect(session.autoSavePaused, isFalse);
    expect(control.saves, isEmpty);
  });
}
