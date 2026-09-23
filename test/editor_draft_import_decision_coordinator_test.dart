import 'dart:async';

import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/editor_draft_import.dart';
import 'package:morrow_studio/plugins/editor_draft_import_decision_coordinator.dart';

final class _Control implements EditorDraftImportControl {
  List<EditorDraftImportDecision> found = [];
  EditorDraftImportDecision? inspected;
  Completer<List<EditorDraftImportDecision>>? pendingScan;
  Completer<EditorDraftImportSnapshot>? pendingAction;
  int scans = 0, inspects = 0, retries = 0, cancels = 0;

  @override
  Future<List<EditorDraftImportDecision>> discoverDecisions() async {
    scans++;
    return pendingScan == null ? found : pendingScan!.future;
  }

  @override
  Future<EditorDraftImportSnapshot> inspectDecision(
    EditorDraftImportAbandon intent,
  ) async {
    inspects++;
    return _snapshot(
      inspected == null
          ? EditorDraftImportResultKind.absent
          : EditorDraftImportResultKind.decision,
      decision: inspected,
    );
  }

  @override
  Future<EditorDraftImportSnapshot> abandon(
    EditorDraftImportAbandon intent,
  ) async {
    retries++;
    return pendingAction == null
        ? _snapshot(EditorDraftImportResultKind.record)
        : pendingAction!.future;
  }

  @override
  Future<EditorDraftImportSnapshot> cancelDecision(
    EditorDraftImportAbandon intent,
  ) async {
    cancels++;
    return pendingAction == null
        ? _snapshot(EditorDraftImportResultKind.decision)
        : pendingAction!.future;
  }

  @override
  dynamic noSuchMethod(Invocation invocation) => super.noSuchMethod(invocation);
}

EditorDraftImportRequest _request({String name = 'file.bin'}) =>
    EditorDraftImportRequest(
      cardId: 'card-1',
      draftId: 'draft-1',
      operation: 'import-1',
      expectedGeneration: BigInt.one,
      name: name,
      kind: 'file',
      bytes: BigInt.from(3),
      sha256: List<int>.filled(32, 7),
    );

EditorDraftImportDecision _decision({
  EditorDraftImportRequest? request,
  String operation = 'abandon-1',
  EditorDraftImportDecisionStatus status =
      EditorDraftImportDecisionStatus.pending,
}) => EditorDraftImportDecision(
  request: request ?? _request(),
  operation: operation,
  expectedGeneration: BigInt.one,
  status: status,
  currentGeneration: BigInt.one,
  mainActive: true,
  stagingRevision: BigInt.one,
  decisionRevision: BigInt.one,
  committedRevision: BigInt.zero,
);

EditorDraftImportSnapshot _snapshot(
  EditorDraftImportResultKind kind, {
  EditorDraftImportDecision? decision,
}) => EditorDraftImportSnapshot(
  kind: kind,
  cardId: 'card-1',
  draftId: 'draft-1',
  operation: 'abandon-1',
  expectedGeneration: BigInt.one,
  importOperation: 'import-1',
  currentGeneration: BigInt.one,
  mainActive: true,
  stagingRevision: BigInt.one,
  exportBytes: BigInt.zero,
  decision: decision,
);

void main() {
  test('read-only discovery publishes an immutable list', () async {
    final control = _Control()..found = [_decision()];
    final coordinator = EditorDraftImportDecisionCoordinator(
      control,
      isCurrent: () => true,
    );
    final found = await coordinator.refresh();
    expect(found, hasLength(1));
    expect(control.inspects, 0);
    expect(control.retries, 0);
    expect(control.cancels, 0);
    expect(() => found.add(_decision()), throwsUnsupportedError);
  });

  test('late discovery after workspace switch does not publish', () async {
    final control = _Control()
      ..found = [_decision()]
      ..pendingScan = Completer<List<EditorDraftImportDecision>>();
    var current = true;
    final coordinator = EditorDraftImportDecisionCoordinator(
      control,
      isCurrent: () => current,
    );
    final pending = coordinator.refresh();
    current = false;
    control.pendingScan!.complete(control.found);
    await expectLater(pending, throwsStateError);
    expect(coordinator.decisions, isEmpty);
    expect(control.retries, 0);
  });

  test('double action is rejected while first recovery is in flight', () async {
    final observed = _decision();
    final control = _Control()
      ..inspected = observed
      ..found = [observed]
      ..pendingAction = Completer<EditorDraftImportSnapshot>();
    final coordinator = EditorDraftImportDecisionCoordinator(
      control,
      isCurrent: () => true,
    );
    final first = coordinator.retry(observed);
    await Future<void>.delayed(Duration.zero);
    await expectLater(coordinator.cancel(observed), throwsStateError);
    control.pendingAction!.complete(
      _snapshot(EditorDraftImportResultKind.record),
    );
    await first;
    expect(control.retries, 1);
    expect(control.cancels, 0);
    expect(control.scans, 1);
  });

  test(
    'changed original request and operation are rejected before mutation',
    () async {
      final observed = _decision();
      final control = _Control()
        ..inspected = _decision(request: _request(name: 'changed'));
      final coordinator = EditorDraftImportDecisionCoordinator(
        control,
        isCurrent: () => true,
      );
      await expectLater(coordinator.retry(observed), throwsStateError);
      control.inspected = _decision(operation: 'other-abandon');
      await expectLater(coordinator.cancel(observed), throwsStateError);
      expect(control.retries, 0);
      expect(control.cancels, 0);
    },
  );

  test('committed cannot cancel; conflict cannot retry', () async {
    final observed = _decision();
    final control = _Control()
      ..inspected = _decision(
        status: EditorDraftImportDecisionStatus.committed,
      );
    final coordinator = EditorDraftImportDecisionCoordinator(
      control,
      isCurrent: () => true,
    );
    await expectLater(coordinator.cancel(observed), throwsStateError);
    control.inspected = _decision(
      status: EditorDraftImportDecisionStatus.conflict,
    );
    await expectLater(coordinator.retry(observed), throwsStateError);
    expect(control.retries, 0);
    expect(control.cancels, 0);
  });

  test('workspace switch during inspect forbids action', () async {
    final observed = _decision();
    final control = _Control()..inspected = observed;
    var current = true;
    final coordinator = EditorDraftImportDecisionCoordinator(
      control,
      isCurrent: () => current,
    );
    final pending = coordinator.retry(observed);
    current = false;
    await expectLater(pending, throwsStateError);
    expect(control.retries, 0);
  });

  test(
    'confirmed action followed by failed refresh keeps the receipt',
    () async {
      final observed = _decision();
      final control = _Control()
        ..inspected = observed
        ..pendingScan = Completer<List<EditorDraftImportDecision>>();
      final coordinator = EditorDraftImportDecisionCoordinator(
        control,
        isCurrent: () => true,
      );
      final pending = coordinator.retry(observed);
      await Future<void>.delayed(Duration.zero);
      control.pendingScan!.completeError(StateError('read failed'));
      await expectLater(
        pending,
        throwsA(
          isA<EditorDraftImportDecisionRefreshFailure>().having(
            (e) => e.confirmed.operation,
            'receipt',
            observed.operation,
          ),
        ),
      );
      expect(control.retries, 1);
      expect(coordinator.decisions, isEmpty);
    },
  );

  test('workspace switch after confirmed action does not publish', () async {
    final observed = _decision();
    final control = _Control()
      ..inspected = observed
      ..pendingAction = Completer<EditorDraftImportSnapshot>();
    var current = true;
    final coordinator = EditorDraftImportDecisionCoordinator(
      control,
      isCurrent: () => current,
    );
    final pending = coordinator.retry(observed);
    await Future<void>.delayed(Duration.zero);
    current = false;
    control.pendingAction!.complete(
      _snapshot(EditorDraftImportResultKind.record),
    );
    await expectLater(
      pending,
      throwsA(
        isA<EditorDraftImportDecisionRefreshFailure>().having(
          (e) => e.confirmed.operation,
          'confirmed operation',
          observed.operation,
        ),
      ),
    );
    expect(coordinator.decisions, isEmpty);
  });

  test(
    'uncertain action failure remains unknown and keeps original intent',
    () async {
      final observed = _decision();
      final control = _Control()
        ..inspected = observed
        ..pendingAction = Completer<EditorDraftImportSnapshot>();
      final coordinator = EditorDraftImportDecisionCoordinator(
        control,
        isCurrent: () => true,
      );
      final pending = coordinator.retry(observed);
      await Future<void>.delayed(Duration.zero);
      control.pendingAction!.completeError(
        EditorDraftImportFailure(
          intent: observed.intent,
          outcomeUnknown: true,
          cause: StateError('lost reply'),
        ),
      );
      await expectLater(
        pending,
        throwsA(
          isA<EditorDraftImportFailure>()
              .having((e) => e.outcomeUnknown, 'unknown', true)
              .having(
                (e) => (e.intent as EditorDraftImportAbandon).operation,
                'original operation',
                observed.operation,
              ),
        ),
      );
      expect(control.retries, 1);
      expect(coordinator.decisions, isEmpty);
    },
  );
  test('dispose rejects new work and ignores a late scan', () async {
    final control = _Control()
      ..pendingScan = Completer<List<EditorDraftImportDecision>>();
    final coordinator = EditorDraftImportDecisionCoordinator(
      control,
      isCurrent: () => true,
    );
    final pending = coordinator.refresh();
    coordinator.dispose();
    control.pendingScan!.complete([_decision()]);
    await expectLater(pending, throwsStateError);
    await expectLater(coordinator.refresh(), throwsStateError);
    expect(coordinator.decisions, isEmpty);
  });
}
