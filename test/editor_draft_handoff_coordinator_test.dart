import 'dart:async';

import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/editor_draft.dart';
import 'package:morrow_studio/plugins/editor_draft_handoff_coordinator.dart';

final class HandoffControl implements EditorDraftHandoffProposalControl {
  Future<List<EditorDraftHandoffProposalSummary>> Function()? onDiscover;
  Future<EditorDraftHandoffProposalRecord?> Function()? onInspect;
  Future<EditorDraftHandoffProposalRecord> Function()? onComplete;
  Future<EditorDraftHandoffProposalRecord> Function()? onRetire;
  Future<EditorDraftHandoffProposalRecord> Function()? onCancel;
  int discoveries = 0;
  int inspections = 0;
  int writes = 0;

  @override
  Future<List<EditorDraftHandoffProposalSummary>> discover({
    int pageSize = 32,
  }) {
    discoveries++;
    return onDiscover?.call() ?? Future.value([proposalRecord().summary]);
  }

  @override
  Future<EditorDraftHandoffProposalRecord?> inspect({
    required String cardId,
    required String parentDraftId,
    required String childOperation,
  }) {
    inspections++;
    expect(
      (cardId, parentDraftId, childOperation),
      ('card', 'parent', 'child-op'),
    );
    return onInspect?.call() ?? Future.value(proposalRecord());
  }

  @override
  Future<EditorDraftHandoffProposalRecord> complete({
    required String cardId,
    required String parentDraftId,
    required String childOperation,
  }) {
    writes++;
    return onComplete?.call() ??
        Future.value(
          proposalRecord(
            status: EditorDraftHandoffProposalStatus.childCommitted,
          ),
        );
  }

  @override
  Future<EditorDraftHandoffProposalRecord> retire({
    required String cardId,
    required String parentDraftId,
    required String childOperation,
  }) {
    writes++;
    return onRetire?.call() ??
        Future.value(
          proposalRecord(
            status: EditorDraftHandoffProposalStatus.parentRetired,
          ),
        );
  }

  @override
  Future<EditorDraftHandoffProposalRecord> cancel({
    required String cardId,
    required String parentDraftId,
    required String childOperation,
  }) {
    writes++;
    return onCancel?.call() ??
        Future.value(
          proposalRecord(status: EditorDraftHandoffProposalStatus.cancelled),
        );
  }

  @override
  Future<EditorDraftHandoffProposalRecord> prepare(
    EditorDraftHandoffProposal proposal,
  ) {
    writes++;
    return Future.value(proposalRecord());
  }

  @override
  Future<EditorDraftHandoffProposalPage> page({
    String cursor = '',
    int limit = 32,
  }) => Future.value(
    EditorDraftHandoffProposalPage(
      proposals: [proposalRecord().summary],
      nextCursor: '',
      requestCursor: cursor,
      requestLimit: limit,
    ),
  );
}

EditorDraftTextValue _text(String value) => EditorDraftTextValue(
  text: value,
  selectionBase: -1,
  selectionExtent: -1,
  affinity: 0,
  directional: false,
  composingStart: -1,
  composingEnd: -1,
);

EditorDraftHandoffProposalRecord proposalRecord({
  EditorDraftHandoffProposalStatus status =
      EditorDraftHandoffProposalStatus.pending,
  String title = 'frozen',
  BigInt? revision,
}) {
  final request = EditorDraftWriteRequest(
    cardId: 'card',
    draftId: 'child',
    operation: 'child-op',
    expectedGeneration: BigInt.zero,
    sourceRevision: BigInt.one,
    predecessorOperation: '',
    predecessorDigest: const [],
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
  final parent = EditorDraftParentLink(
    parentDraftId: 'parent',
    parentGeneration: BigInt.one,
    parentSaveOperation: 'parent-op',
    parentRequestSha256: List<int>.filled(32, 3),
    committedOperation: 'commit-op',
    committedSha256: List<int>.filled(32, 4),
    childOperation: 'child-op',
  );
  final childExists =
      status == EditorDraftHandoffProposalStatus.childCommitted ||
      status == EditorDraftHandoffProposalStatus.parentRetired;
  return EditorDraftHandoffProposalRecord(
    proposal: EditorDraftHandoffProposal(
      handoff: EditorDraftHandoffRequest(request: request, parentLink: parent),
      retirementOperation: 'retire-op',
    ),
    summary: EditorDraftHandoffProposalSummary(
      cardId: 'card',
      parentDraftId: 'parent',
      childDraftId: 'child',
      childOperation: 'child-op',
      retirementOperation: 'retire-op',
      revision:
          revision ??
          (status == EditorDraftHandoffProposalStatus.cancelled
              ? BigInt.from(2)
              : BigInt.one),
      status: status,
      parentGeneration: status == EditorDraftHandoffProposalStatus.parentRetired
          ? BigInt.from(2)
          : BigInt.one,
      parentActive: status != EditorDraftHandoffProposalStatus.parentRetired,
      childGeneration: childExists ? BigInt.one : BigInt.zero,
      childActive: status == EditorDraftHandoffProposalStatus.childCommitted,
      cursor: "morrow-host-editor-handoff-proposal-${'a' * 64}",
    ),
  );
}

void main() {
  test(
    'discovery and selection stay read-only and held by coordinator',
    () async {
      final control = HandoffControl();
      final coordinator = EditorDraftHandoffCoordinator(
        control,
        isCurrent: () => true,
        canWrite: () => true,
      );
      var notifications = 0;
      coordinator.addListener(() => notifications++);
      final summaries = await coordinator.refresh();
      expect(summaries, hasLength(1));
      expect(() => summaries.clear(), throwsUnsupportedError);
      final selected = await coordinator.inspect(summaries.single);
      expect(selected!.proposal.handoff.request.values.title.text, 'frozen');
      expect(coordinator.selected, same(selected));
      expect(control.inspections, 1);
      expect(control.writes, 0);
      expect(coordinator.busy, isFalse);
      expect(notifications, greaterThan(0));
      coordinator.dispose();
    },
  );

  test('inspect and confirmed action replace the visible summary', () async {
    final control = HandoffControl();
    final coordinator = EditorDraftHandoffCoordinator(
      control,
      isCurrent: () => true,
      canWrite: () => true,
    );
    final original = (await coordinator.refresh()).single;
    control.onInspect = () async =>
        proposalRecord(status: EditorDraftHandoffProposalStatus.childCommitted);
    final inspected = await coordinator.inspect(original);
    expect(
      inspected!.summary.status,
      EditorDraftHandoffProposalStatus.childCommitted,
    );
    expect(
      coordinator.summaries.single.status,
      EditorDraftHandoffProposalStatus.childCommitted,
    );
    control.onInspect = () async => proposalRecord();
    final confirmed = await coordinator.act(
      proposalRecord(),
      EditorDraftHandoffAction.complete,
    );
    expect(coordinator.summaries.single.status, confirmed.summary.status);
    expect(coordinator.selected, same(confirmed));
    coordinator.dispose();
  });

  test('listener-driven workspace switch stops the first host call', () async {
    final control = HandoffControl();
    var current = true;
    final coordinator = EditorDraftHandoffCoordinator(
      control,
      isCurrent: () => current,
      canWrite: () => true,
    );
    coordinator.addListener(() {
      if (coordinator.busy) current = false;
    });
    await expectLater(coordinator.refresh(), throwsStateError);
    expect(control.discoveries, 0);
    expect(control.inspections, 0);
    expect(control.writes, 0);
    expect(coordinator.busy, isFalse);
    coordinator.dispose();
  });

  test('notification check cannot replace the original host error', () async {
    final control = HandoffControl();
    control.onDiscover = () =>
        Future.error(StateError('original host failure'));
    var checks = 0;
    final coordinator = EditorDraftHandoffCoordinator(
      control,
      isCurrent: () {
        checks++;
        if (checks >= 5) throw StateError('notification predicate');
        return true;
      },
      canWrite: () => true,
    );
    await expectLater(
      coordinator.refresh(),
      throwsA(
        isA<StateError>().having(
          (error) => error.message,
          'message',
          'original host failure',
        ),
      ),
    );
    expect(control.discoveries, 1);
    coordinator.dispose();
  });

  test('stale state and changed frozen body refuse a write', () async {
    final control = HandoffControl();
    final coordinator = EditorDraftHandoffCoordinator(
      control,
      isCurrent: () => true,
      canWrite: () => true,
    );
    final observed = proposalRecord();
    control.onInspect = () async =>
        proposalRecord(status: EditorDraftHandoffProposalStatus.childCommitted);
    await expectLater(
      coordinator.act(observed, EditorDraftHandoffAction.complete),
      throwsStateError,
    );
    expect(control.writes, 0);
    expect(coordinator.lastError, isA<StateError>());
    control.onInspect = () async => proposalRecord(title: 'changed');
    await expectLater(
      coordinator.act(observed, EditorDraftHandoffAction.complete),
      throwsStateError,
    );
    expect(control.writes, 0);
    coordinator.dispose();
  });

  test('failed action preflight leaves proposal known and unwritten', () async {
    final control = HandoffControl();
    final original = StateError('source changed');
    final coordinator = EditorDraftHandoffCoordinator(
      control,
      isCurrent: () => true,
      canWrite: () => true,
      beforeAction: (record, action) async {
        expect(record.summary.childOperation, 'child-op');
        expect(action, EditorDraftHandoffAction.complete);
        throw original;
      },
    );
    await expectLater(
      coordinator.act(proposalRecord(), EditorDraftHandoffAction.complete),
      throwsA(same(original)),
    );
    expect(coordinator.lastError, same(original));
    expect(coordinator.uncertain, isFalse);
    expect(control.inspections, 0);
    expect(control.writes, 0);
    coordinator.dispose();
  });

  test('workspace switch during action preflight stops inspection', () async {
    final control = HandoffControl();
    final waiting = Completer<void>();
    var current = true;
    final coordinator = EditorDraftHandoffCoordinator(
      control,
      isCurrent: () => current,
      canWrite: () => true,
      beforeAction: (_, _) => waiting.future,
    );
    final pending = coordinator.act(
      proposalRecord(),
      EditorDraftHandoffAction.complete,
    );
    expect(coordinator.busy, isTrue);
    current = false;
    waiting.complete();
    await expectLater(pending, throwsStateError);
    expect(coordinator.uncertain, isFalse);
    expect(control.inspections, 0);
    expect(control.writes, 0);
    coordinator.dispose();
  });

  test('permission revoked during inspect prevents submission', () async {
    final control = HandoffControl();
    final waiting = Completer<EditorDraftHandoffProposalRecord?>();
    control.onInspect = () => waiting.future;
    var allowed = true;
    final coordinator = EditorDraftHandoffCoordinator(
      control,
      isCurrent: () => true,
      canWrite: () => allowed,
    );
    final pending = coordinator.act(
      proposalRecord(),
      EditorDraftHandoffAction.complete,
    );
    expect(coordinator.busy, isTrue);
    expect(() => coordinator.refresh(), throwsA(isA<StateError>()));
    allowed = false;
    waiting.complete(proposalRecord());
    await expectLater(pending, throwsStateError);
    expect(control.writes, 0);
    expect(coordinator.busy, isFalse);
    coordinator.dispose();
  });

  test(
    'unknown outcome blocks writes until original inspect succeeds',
    () async {
      final control = HandoffControl();
      control.onComplete = () => Future.error(
        EditorDraftHandoffProposalFailure(
          cardId: 'card',
          parentDraftId: 'parent',
          childOperation: 'child-op',
          outcomeUnknown: true,
          cause: StateError('lost receipt'),
        ),
      );
      final coordinator = EditorDraftHandoffCoordinator(
        control,
        isCurrent: () => true,
        canWrite: () => true,
      );
      final observed = proposalRecord();
      await expectLater(
        coordinator.act(observed, EditorDraftHandoffAction.complete),
        throwsA(isA<EditorDraftHandoffProposalFailure>()),
      );
      expect(coordinator.uncertain, isTrue);
      expect(control.writes, 1);
      final inspections = control.inspections;
      await expectLater(
        coordinator.act(observed, EditorDraftHandoffAction.complete),
        throwsStateError,
      );
      expect(control.inspections, inspections);
      expect(control.writes, 1);
      control.onInspect = () async => null;
      expect(await coordinator.inspect(observed.summary), isNull);
      expect(coordinator.uncertain, isTrue);
      control.onInspect = () async => proposalRecord(title: 'changed');
      await expectLater(
        coordinator.inspect(observed.summary),
        throwsStateError,
      );
      expect(coordinator.uncertain, isTrue);
      control.onInspect = () async => proposalRecord();
      expect(await coordinator.inspect(observed.summary), isNotNull);
      expect(coordinator.uncertain, isFalse);
      control.onComplete = null;
      final confirmed = await coordinator.act(
        observed,
        EditorDraftHandoffAction.complete,
      );
      expect(
        confirmed.summary.status,
        EditorDraftHandoffProposalStatus.childCommitted,
      );
      expect(coordinator.lastConfirmed, same(confirmed));
      expect(control.writes, 2);
      coordinator.dispose();
    },
  );

  test(
    'late receipt remains saved without publishing into another workspace',
    () async {
      final control = HandoffControl();
      final waiting = Completer<EditorDraftHandoffProposalRecord>();
      control.onComplete = () => waiting.future;
      var current = true;
      final coordinator = EditorDraftHandoffCoordinator(
        control,
        isCurrent: () => current,
        canWrite: () => true,
      );
      var notifications = 0;
      coordinator.addListener(() => notifications++);
      final pending = coordinator.act(
        proposalRecord(),
        EditorDraftHandoffAction.complete,
      );
      await Future<void>.delayed(Duration.zero);
      expect(control.writes, 1);
      final beforeSwitch = notifications;
      current = false;
      final receipt = proposalRecord(
        status: EditorDraftHandoffProposalStatus.childCommitted,
      );
      waiting.complete(receipt);
      await expectLater(pending, throwsStateError);
      expect(coordinator.lastConfirmed, same(receipt));
      expect(coordinator.selected, isNull);
      expect(notifications, beforeSwitch);
      await expectLater(coordinator.refresh(), throwsStateError);
      coordinator.dispose();
    },
  );

  test('completed transition is never submitted again', () async {
    final control = HandoffControl();
    control.onInspect = () async =>
        proposalRecord(status: EditorDraftHandoffProposalStatus.childCommitted);
    final coordinator = EditorDraftHandoffCoordinator(
      control,
      isCurrent: () => true,
      canWrite: () => true,
    );
    await expectLater(
      coordinator.act(
        proposalRecord(status: EditorDraftHandoffProposalStatus.childCommitted),
        EditorDraftHandoffAction.complete,
      ),
      throwsStateError,
    );
    expect(control.writes, 0);
    coordinator.dispose();
  });
}
