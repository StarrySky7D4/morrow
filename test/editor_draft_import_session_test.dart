import 'dart:async';

import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/editor_draft_import.dart';
import 'package:morrow_studio/plugins/editor_draft_import_session.dart';

EditorDraftImportRequest proposal() => EditorDraftImportRequest(
  cardId: 'card-one',
  draftId: 'draft-one',
  operation: 'original-import',
  expectedGeneration: BigInt.one,
  name: 'selected.bin',
  kind: 'file',
  bytes: BigInt.from(3),
  sha256: List<int>.filled(32, 4),
);

EditorDraftImportSnapshot snapshot(
  EditorDraftImportRequest request,
  EditorDraftImportPhase? phase, {
  bool retained = false,
  bool repeated = false,
  String operation = 'original-import',
  BigInt? expectedGeneration,
}) {
  final record = phase == null
      ? null
      : EditorDraftImportRecord(
          request: request,
          assetId: 'asset-one',
          phase: phase,
          currentActive: phase == EditorDraftImportPhase.ready,
          bytesRetained: retained,
          stagingRevision: BigInt.two,
          repeated: repeated,
          currentGeneration: BigInt.one,
          mainActive: true,
        );
  return EditorDraftImportSnapshot(
    kind: record == null
        ? EditorDraftImportResultKind.absent
        : EditorDraftImportResultKind.record,
    cardId: request.cardId,
    draftId: request.draftId,
    operation: operation,
    expectedGeneration: expectedGeneration ?? BigInt.one,
    importOperation: request.operation,
    currentGeneration: BigInt.one,
    mainActive: true,
    stagingRevision: BigInt.two,
    record: record,
    exportBytes: BigInt.zero,
  );
}

EditorDraftImportDecision decisionRecord(
  EditorDraftImportRequest request,
  EditorDraftImportAbandon intent,
  EditorDraftImportDecisionStatus status,
) => EditorDraftImportDecision(
  request: request,
  operation: intent.operation,
  expectedGeneration: intent.currentGeneration,
  status: status,
  currentGeneration: BigInt.one,
  mainActive: true,
  stagingRevision: BigInt.two,
  decisionRevision: status == EditorDraftImportDecisionStatus.cancelled
      ? BigInt.two
      : BigInt.one,
  committedRevision: status == EditorDraftImportDecisionStatus.committed
      ? BigInt.two
      : BigInt.zero,
);

EditorDraftImportSnapshot decisionSnapshot(
  EditorDraftImportRequest request,
  EditorDraftImportAbandon intent,
  EditorDraftImportDecisionStatus status,
) => EditorDraftImportSnapshot(
  kind: EditorDraftImportResultKind.decision,
  cardId: request.cardId,
  draftId: request.draftId,
  operation: intent.operation,
  expectedGeneration: intent.currentGeneration,
  importOperation: request.operation,
  currentGeneration: BigInt.one,
  mainActive: true,
  stagingRevision: BigInt.two,
  exportBytes: BigInt.zero,
  decision: decisionRecord(request, intent, status),
);

final class FakeImportControl implements EditorDraftImportControl {
  final writes = <(String, EditorDraftImportRequest, String)>[];
  final abandons = <EditorDraftImportAbandon>[];
  final prepares = <EditorDraftImportAbandon>[];
  final inspections = <EditorDraftImportAbandon>[];
  final cancellations = <EditorDraftImportAbandon>[];
  Future<EditorDraftImportSnapshot> Function(EditorDraftImportAbandon)?
  onPrepare;
  Future<EditorDraftImportSnapshot> Function(EditorDraftImportAbandon)?
  onInspectDecision;
  Future<EditorDraftImportSnapshot> Function(EditorDraftImportAbandon)?
  onCancel;
  Future<EditorDraftImportSnapshot> Function(EditorDraftImportRequest, String)?
  onComplete;
  Future<EditorDraftImportSnapshot> Function(EditorDraftImportRequest)? onBegin;
  Future<EditorDraftImportSnapshot> Function(EditorDraftImportAbandon)?
  onAbandon;
  late EditorDraftImportSnapshot inspection;

  @override
  Future<EditorDraftImportSnapshot> begin(EditorDraftImportRequest request) {
    writes.add(('begin', request, ''));
    return onBegin?.call(request) ??
        Future.value(snapshot(request, EditorDraftImportPhase.pending));
  }

  @override
  Future<EditorDraftImportSnapshot> complete(
    EditorDraftImportRequest request, {
    String selectedPath = '',
  }) {
    writes.add(('complete', request, selectedPath));
    return onComplete?.call(request, selectedPath) ??
        Future.value(
          snapshot(request, EditorDraftImportPhase.ready, retained: true),
        );
  }

  @override
  Future<EditorDraftImportSnapshot> inspect(
    String cardId,
    String draftId,
    String importOperation,
  ) async => inspection;
  @override
  Future<EditorDraftImportSnapshot> prepareDecision(
    EditorDraftImportAbandon intent,
  ) {
    prepares.add(intent);
    return onPrepare?.call(intent) ??
        Future.value(
          decisionSnapshot(
            proposal(),
            intent,
            EditorDraftImportDecisionStatus.pending,
          ),
        );
  }

  @override
  Future<EditorDraftImportSnapshot> inspectDecision(
    EditorDraftImportAbandon intent,
  ) {
    inspections.add(intent);
    return onInspectDecision?.call(intent) ??
        Future.value(
          decisionSnapshot(
            proposal(),
            intent,
            EditorDraftImportDecisionStatus.pending,
          ),
        );
  }

  @override
  Future<List<EditorDraftImportDecision>> discoverDecisions() async => const [];

  @override
  Future<EditorDraftImportScopePage> listDecisionScopes({
    String cursor = '',
    int limit = 32,
  }) async => EditorDraftImportScopePage(
    requestCursor: cursor,
    requestLimit: limit,
    nextCursor: '',
    scopes: const [],
  );
  @override
  Future<EditorDraftImportDecisionPage> listDecisions(
    String cardId,
    String draftId, {
    String cursor = '',
    int limit = 32,
  }) => throw UnimplementedError();
  @override
  Future<EditorDraftImportSnapshot> cancelDecision(
    EditorDraftImportAbandon intent,
  ) {
    cancellations.add(intent);
    return onCancel?.call(intent) ??
        Future.value(
          decisionSnapshot(
            proposal(),
            intent,
            EditorDraftImportDecisionStatus.cancelled,
          ),
        );
  }

  @override
  Future<EditorDraftImportSnapshot> abandon(EditorDraftImportAbandon intent) {
    abandons.add(intent);
    return onAbandon?.call(intent) ??
        Future.value(
          snapshot(
            proposal(),
            EditorDraftImportPhase.retired,
            operation: intent.operation,
          ),
        );
  }

  @override
  Future<EditorDraftImportSnapshot> list(String cardId, String draftId) =>
      throw UnimplementedError();
  @override
  Future<EditorDraftImportSnapshot> reconcile(String cardId, String draftId) =>
      throw UnimplementedError();
  @override
  Future<EditorDraftImportSnapshot> export(
    String cardId,
    String draftId,
    BigInt currentGeneration,
    String importOperation,
    String selectedPath,
  ) => throw UnimplementedError();
}

void main() {
  test('lost complete, pending inspection, and exact explicit retry', () async {
    final request = proposal();
    final fake = FakeImportControl();
    final gate = Completer<EditorDraftImportSnapshot>();
    fake.onComplete = (_, _) => gate.future;
    final session = EditorDraftImportSession(
      control: fake,
      request: request,
      isCurrent: () => true,
    );
    final first = session.complete(selectedPath: r'C:\original.bin');
    await Future<void>.delayed(Duration.zero);
    expect(fake.writes.single.$2, same(request));
    gate.completeError(
      EditorDraftImportFailure(
        intent: request,
        outcomeUnknown: true,
        cause: StateError('lost reply'),
      ),
    );
    await expectLater(first, throwsA(isA<EditorDraftImportFailure>()));
    expect(session.unknown, isTrue);
    fake.inspection = snapshot(
      request,
      EditorDraftImportPhase.pending,
      retained: true,
      expectedGeneration: BigInt.zero,
    );
    final inspected = await session.inspect();
    expect(inspected.record!.phase, EditorDraftImportPhase.pending);
    expect(session.unknown, isTrue);
    expect(
      () => session.complete(selectedPath: r'C:\new.bin'),
      throwsStateError,
    );
    expect(fake.writes, hasLength(1));
    fake.onComplete = (_, path) async {
      expect(path, isEmpty); // Never reopen the stale selected path.
      return snapshot(
        request,
        EditorDraftImportPhase.ready,
        retained: true,
        repeated: true,
      );
    };
    final ready = await session.retryOriginal();
    expect(ready.record!.phase, EditorDraftImportPhase.ready);
    expect(fake.writes, hasLength(2));
    expect(fake.writes.last.$2, same(request));
    expect(session.unknown, isFalse);
    expect(session.confirmed, same(ready));
  });

  test('late reply from changed workspace is not adopted', () async {
    final request = proposal();
    final fake = FakeImportControl();
    final gate = Completer<EditorDraftImportSnapshot>();
    fake.onComplete = (_, _) => gate.future;
    var current = true;
    final session = EditorDraftImportSession(
      control: fake,
      request: request,
      isCurrent: () => current,
    );
    final pending = session.complete();
    current = false;
    gate.complete(
      snapshot(request, EditorDraftImportPhase.ready, retained: true),
    );
    await expectLater(pending, throwsA(isA<EditorDraftImportFailure>()));
    expect(session.confirmed, isNull);
    expect(session.lastInspection, isNull);
    session.dispose();
  });

  test(
    'unknown abandon retries the exact caller operation after detach',
    () async {
      final request = proposal();
      final fake = FakeImportControl();
      final session = EditorDraftImportSession(
        control: fake,
        request: request,
        isCurrent: () => true,
      );
      final intent = EditorDraftImportAbandon(
        cardId: request.cardId,
        draftId: request.draftId,
        importOperation: request.operation,
        operation: 'abandon-original',
        currentGeneration: BigInt.one,
      );
      fake.onAbandon = (_) async => throw EditorDraftImportFailure(
        intent: intent,
        outcomeUnknown: true,
        cause: StateError('lost'),
      );
      await expectLater(
        session.abandon(intent),
        throwsA(isA<EditorDraftImportFailure>()),
      );
      expect(session.pendingAbandon, same(intent));
      session.detachUI();
      fake.onAbandon = (_) async => snapshot(
        request,
        EditorDraftImportPhase.retired,
        operation: intent.operation,
      );
      final result = await session.retryAbandon();
      expect(result.record!.phase, EditorDraftImportPhase.retired);
      expect(fake.abandons, [same(intent), same(intent)]);
      expect(session.pendingAbandon, isNull);
      expect(session.unknown, isFalse);
    },
  );
  test('NoCommit retry cannot erase an earlier unknown outcome', () async {
    final request = proposal();
    final fake = FakeImportControl();
    final session = EditorDraftImportSession(
      control: fake,
      request: request,
      isCurrent: () => true,
    );
    fake.onComplete = (_, _) async => throw EditorDraftImportFailure(
      intent: request,
      outcomeUnknown: true,
      cause: StateError('lost reply'),
    );
    await expectLater(
      session.complete(),
      throwsA(isA<EditorDraftImportFailure>()),
    );
    expect(session.unknown, isTrue);
    fake.onComplete = (_, _) async => throw EditorDraftImportFailure(
      intent: request,
      outcomeUnknown: false,
      cause: StateError('not sent'),
    );
    await expectLater(
      session.retryOriginal(),
      throwsA(isA<EditorDraftImportFailure>()),
    );
    expect(session.unknown, isTrue);
    expect(fake.writes.map((call) => call.$2), everyElement(same(request)));
    fake.onComplete = (_, _) async => snapshot(
      request,
      EditorDraftImportPhase.ready,
      retained: true,
      repeated: true,
    );
    await session.retryOriginal();
    expect(session.unknown, isFalse);
    expect(fake.writes, hasLength(3));
  });

  test(
    'pending abandon seals import retry; invalid self-abandon stays local',
    () async {
      final request = proposal();
      final fake = FakeImportControl();
      final session = EditorDraftImportSession(
        control: fake,
        request: request,
        isCurrent: () => true,
      );
      fake.onComplete = (_, _) async => throw EditorDraftImportFailure(
        intent: request,
        outcomeUnknown: true,
        cause: StateError('lost finish'),
      );
      await expectLater(
        session.complete(),
        throwsA(isA<EditorDraftImportFailure>()),
      );
      final bad = EditorDraftImportAbandon(
        cardId: request.cardId,
        draftId: request.draftId,
        importOperation: request.operation,
        operation: request.operation,
        currentGeneration: BigInt.one,
      );
      expect(() => session.abandon(bad), throwsFormatException);
      expect(fake.abandons, isEmpty);
      expect(session.unknown, isTrue);
      final good = EditorDraftImportAbandon(
        cardId: request.cardId,
        draftId: request.draftId,
        importOperation: request.operation,
        operation: 'abandon-original',
        currentGeneration: BigInt.one,
      );
      fake.onAbandon = (_) async => throw EditorDraftImportFailure(
        intent: good,
        outcomeUnknown: true,
        cause: StateError('lost abandon'),
      );
      await expectLater(
        session.abandon(good),
        throwsA(isA<EditorDraftImportFailure>()),
      );
      expect(session.pendingAbandon, same(good));
      expect(() => session.retryOriginal(), throwsStateError);
      expect(fake.writes, hasLength(1));
      fake.onAbandon = (_) async => snapshot(
        request,
        EditorDraftImportPhase.retired,
        operation: good.operation,
      );
      await session.retryAbandon();
      expect(fake.abandons, [same(good), same(good)]);
      expect(session.unknown, isFalse);
    },
  );
  test(
    'restored decisions are read-only and only Pending can be retried',
    () async {
      final request = proposal();
      final fake = FakeImportControl();
      final intent = EditorDraftImportAbandon(
        cardId: request.cardId,
        draftId: request.draftId,
        importOperation: request.operation,
        operation: 'abandon-persisted',
        currentGeneration: BigInt.one,
      );
      final pending = EditorDraftImportSession.restoreDecision(
        control: fake,
        decision: decisionRecord(
          request,
          intent,
          EditorDraftImportDecisionStatus.pending,
        ),
        isCurrent: () => true,
      );
      expect(fake.prepares, isEmpty);
      expect(fake.abandons, isEmpty);
      expect(pending.pendingAbandon!.operation, intent.operation);
      expect(pending.confirmed, isNull);
      await pending.retryAbandon();
      expect(fake.abandons.single.operation, intent.operation);
      for (final status in [
        EditorDraftImportDecisionStatus.committed,
        EditorDraftImportDecisionStatus.cancelled,
      ]) {
        final restored = EditorDraftImportSession.restoreDecision(
          control: fake,
          decision: decisionRecord(request, intent, status),
          isCurrent: () => true,
        );
        expect(restored.lastDecision!.status, status);
        expect(restored.pendingAbandon, isNull);
        expect(restored.unknown, isFalse);
        expect(() => restored.retryAbandon(), throwsStateError);
        expect(() => restored.abandon(intent), throwsStateError);
      }
      expect(fake.abandons, hasLength(1));
    },
  );

  test(
    'lost Prepare never commits, inspection proves Pending, then explicit retry',
    () async {
      final request = proposal();
      final fake = FakeImportControl();
      final intent = EditorDraftImportAbandon(
        cardId: request.cardId,
        draftId: request.draftId,
        importOperation: request.operation,
        operation: 'abandon-prepare-lost',
        currentGeneration: BigInt.one,
      );
      final session = EditorDraftImportSession(
        control: fake,
        request: request,
        isCurrent: () => true,
      );
      fake.onPrepare = (_) async => throw EditorDraftImportFailure(
        intent: intent,
        outcomeUnknown: true,
        cause: StateError('lost prepare'),
      );
      await expectLater(
        session.prepareDecision(intent),
        throwsA(isA<EditorDraftImportFailure>()),
      );
      expect(session.unknown, isTrue);
      expect(fake.abandons, isEmpty);
      final seen = await session.inspectDecision(intent);
      expect(seen.decision!.status, EditorDraftImportDecisionStatus.pending);
      expect(session.pendingAbandon, same(intent));
      expect(session.unknown, isFalse);
      expect(fake.abandons, isEmpty);
      await session.retryAbandon();
      expect(fake.abandons.single, same(intent));
    },
  );

  test(
    'conflicted decision can be cancelled without authorizing old abandon',
    () async {
      final request = proposal();
      final fake = FakeImportControl();
      final intent = EditorDraftImportAbandon(
        cardId: request.cardId,
        draftId: request.draftId,
        importOperation: request.operation,
        operation: 'abandon-conflict',
        currentGeneration: BigInt.one,
      );
      fake.onPrepare = (_) async => decisionSnapshot(
        request,
        intent,
        EditorDraftImportDecisionStatus.conflict,
      );
      final session = EditorDraftImportSession(
        control: fake,
        request: request,
        isCurrent: () => true,
      );
      await session.prepareDecision(intent);
      expect(session.pendingAbandon, same(intent));
      fake.onCancel = (_) async => throw EditorDraftImportFailure(
        intent: intent,
        outcomeUnknown: true,
        cause: StateError('lost cancel'),
      );
      await expectLater(
        session.cancelDecision(intent),
        throwsA(isA<EditorDraftImportFailure>()),
      );
      expect(session.pendingAbandon, same(intent));
      expect(session.unknown, isTrue);
      fake.onInspectDecision = (_) async => decisionSnapshot(
        request,
        intent,
        EditorDraftImportDecisionStatus.cancelled,
      );
      final confirmed = await session.inspectDecision(intent);
      expect(
        confirmed.decision!.status,
        EditorDraftImportDecisionStatus.cancelled,
      );
      expect(session.pendingAbandon, isNull);
      expect(session.unknown, isFalse);
      expect(() => session.retryAbandon(), throwsStateError);
      expect(() => session.abandon(intent), throwsStateError);
      expect(fake.abandons, isEmpty);
    },
  );

  test('late Prepare reply cannot adopt a replaced workspace', () async {
    final request = proposal();
    final fake = FakeImportControl();
    final intent = EditorDraftImportAbandon(
      cardId: request.cardId,
      draftId: request.draftId,
      importOperation: request.operation,
      operation: 'abandon-late',
      currentGeneration: BigInt.one,
    );
    final gate = Completer<EditorDraftImportSnapshot>();
    fake.onPrepare = (_) => gate.future;
    var current = true;
    final session = EditorDraftImportSession(
      control: fake,
      request: request,
      isCurrent: () => current,
    );
    final started = session.prepareDecision(intent);
    current = false;
    gate.complete(
      decisionSnapshot(
        request,
        intent,
        EditorDraftImportDecisionStatus.pending,
      ),
    );
    await expectLater(started, throwsA(isA<EditorDraftImportFailure>()));
    expect(session.lastDecision, isNull);
    expect(fake.abandons, isEmpty);
  });
}
