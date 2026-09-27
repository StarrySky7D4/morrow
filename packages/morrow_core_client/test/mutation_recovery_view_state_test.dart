import 'dart:typed_data';

import 'package:test/test.dart';

import '../../../lib/plugins/io_task_models.dart';
import '../../../lib/plugins/mutation_recovery_session.dart';
import '../../../lib/plugins/mutation_recovery_view_state.dart';
import '../../../lib/plugins/mutation_task_models.dart';

Uint8List id(int value) => Uint8List.fromList(List.filled(32, value));

MutationDiscoverRequest discover(int token, String subject) =>
    MutationDiscoverRequest(
      submission: id(token),
      packageId: 'org.example.recovery',
      packageDigest: id(2),
      registryRevision: BigInt.one,
      subject: subject,
      disposition: MutationDisposition.create,
      scanLimit: 1,
      timeoutMs: 1000,
    );

MutationReconcileRequest reconcile(int token) => MutationReconcileRequest(
  submission: id(token),
  packageId: 'org.example.recovery',
  packageDigest: id(2),
  registryRevision: BigInt.one,
  plan: Uint8List.fromList([1]),
  timeoutMs: 1000,
);

MutationResult result(
  MutationResultKind kind, {
  List<Uint8List>? plans,
  Uint8List? checkpoint,
  int scanned = 0,
  bool done = false,
  MutationPhase phase = MutationPhase.none,
}) => MutationResult(
  kind: kind,
  plans: plans,
  checkpoint: checkpoint,
  stagedBytes: BigInt.zero,
  durableContent: false,
  phase: phase,
  operationId: null,
  effect: MutationEffect.unspecified,
  osCode: null,
  scanned: scanned,
  done: done,
);

MutationResult page(int plan, int checkpoint) => result(
  MutationResultKind.plans,
  plans: [
    Uint8List.fromList([plan]),
  ],
  checkpoint: id(checkpoint),
  scanned: 1,
);

class ControlledBackend implements MutationTaskBackend, WorkbenchIoTaskControl {
  Uint8List? key, submission;
  IoStoragePhase storage = IoStoragePhase.local;
  IoTaskExit? exit;
  MutationCommandKind kind = MutationCommandKind.discover;
  MutationDelivery delivery = MutationDelivery.pending;
  BigInt command = BigInt.one;
  bool terminal = false;
  MutationResult? pending;
  final queuedPages = <MutationResult>[];
  int starts = 0, reads = 0, controls = 0, statuses = 0;

  IoTaskSnapshot snapshot() => IoTaskSnapshot(
    key: key,
    submission: submission,
    storage: storage,
    delivery: switch (delivery) {
      MutationDelivery.pending => IoDeliveryPhase.pending,
      MutationDelivery.ready => IoDeliveryPhase.ready,
      MutationDelivery.consumed => IoDeliveryPhase.consumed,
    },
    exit: exit,
  );

  MutationTaskReply reply({bool status = false}) => MutationTaskReply(
    io: snapshot(),
    state: MutationState(
      commandId: command,
      kind: kind,
      delivery: delivery,
      selected: false,
      reconcileRequired: false,
      terminal: terminal,
    ),
    commandId: status ? BigInt.zero : command,
  );

  Future<MutationTaskReply> _start(
    Uint8List token,
    MutationCommandKind next,
  ) async {
    starts++;
    key = id(3);
    submission = token;
    storage = IoStoragePhase.running;
    exit = null;
    kind = next;
    command = BigInt.one;
    delivery = MutationDelivery.ready;
    pending = next == MutationCommandKind.discover
        ? queuedPages.removeAt(0)
        : result(
            MutationResultKind.reconciled,
            phase: MutationPhase.outcomeUnknown,
          );
    terminal = pending!.done;
    return reply();
  }

  @override
  Future<MutationTaskReply> startDiscovery(MutationDiscoverRequest request) =>
      _start(request.submission, MutationCommandKind.discover);

  @override
  Future<MutationTaskReply> startReconciliation(
    MutationReconcileRequest request,
  ) => _start(request.submission, MutationCommandKind.reconcile);

  @override
  Future<MutationTaskReply> submitRelease(
    Uint8List taskKey,
    Uint8List token,
  ) async {
    controls++;
    command += BigInt.one;
    kind = MutationCommandKind.release;
    delivery = MutationDelivery.ready;
    terminal = true;
    pending = result(MutationResultKind.released);
    return reply();
  }

  @override
  Future<MutationTaskReply> status(Uint8List taskKey) async {
    statuses++;
    return reply(status: true);
  }

  @override
  Future<MutationTaskRead> read(Uint8List taskKey, BigInt commandId) async {
    reads++;
    delivery = MutationDelivery.consumed;
    final value = pending;
    pending = null;
    return MutationTaskRead(reply: reply(), result: value);
  }

  @override
  Future<IoTaskSnapshot> ioStatus() async {
    statuses++;
    return snapshot();
  }

  @override
  Future<IoTaskSnapshot> cancelIo(Uint8List taskKey) async {
    controls++;
    storage = IoStoragePhase.stopping;
    return snapshot();
  }

  void joined() {
    storage = IoStoragePhase.reclaimed;
    exit = const IoTaskExit(
      execution: IoJobError.none,
      disconnect: IoJobError.none,
      maintenance: IoJobError.none,
    );
  }

  @override
  Future<IoTaskSnapshot> acknowledgeIo(Uint8List taskKey) async {
    controls++;
    key = null;
    submission = null;
    storage = IoStoragePhase.local;
    exit = null;
    return snapshot();
  }

  @override
  dynamic noSuchMethod(Invocation invocation) => super.noSuchMethod(invocation);
}

void main() {
  test('stable per-session view performs no IO and isolates backends', () {
    final first = ControlledBackend();
    final session = MutationRecoverySession(first, first);
    final view = MutationRecoveryViewState.forSession(session);
    expect(
      identical(view, MutationRecoveryViewState.forSession(session)),
      isTrue,
    );
    expect(first.starts, 0);
    expect(first.statuses, 0);
    expect(first.controls, 0);

    final other = ControlledBackend();
    final otherSession = MutationRecoverySession(other, other);
    final otherView = MutationRecoveryViewState.forSession(otherSession);
    expect(identical(view, otherView), isFalse);
    expect(otherView.page, isNull);
  });

  test(
    'delivered page, scope, choice and checkpoint survive ack and view detach',
    () async {
      final backend = ControlledBackend()..queuedPages.add(page(7, 21));
      final session = MutationRecoverySession(backend, backend);
      final view = MutationRecoveryViewState.forSession(session);
      var notices = 0;
      void listener() => notices++;
      view.addListener(listener);
      view.addListener(() => throw StateError('view callback'));
      await session.refresh();
      final request = discover(1, 'scope.one');
      await session.startDiscovery(request);
      await session.read();
      expect(view.page!.plans!.single, [7]);
      expect(identical(view.pageRequest, request), isTrue);
      expect(view.checkpoint, id(21));
      expect(() => view.checkpoint![0] = 9, throwsUnsupportedError);
      view.selectPlan(0);
      expect(view.selectedPlan, [7]);
      expect(identical(view.selectedRequest, request), isTrue);
      expect(() => view.selectedPlan![0] = 9, throwsUnsupportedError);
      view.removeListener(listener);
      final detachedNotices = notices;

      await session.release(id(4));
      await session.read();
      await session.stop();
      backend.joined();
      await session.refresh();
      await session.acknowledge();
      expect(view.page!.plans!.single, [7]);
      expect(view.checkpoint, id(21));
      expect(view.selectedPlan, [7]);
      expect(identical(view.selectedRequest, request), isTrue);
      expect(notices, detachedNotices);
      expect(
        identical(view, MutationRecoveryViewState.forSession(session)),
        isTrue,
      );
      expect(backend.starts, 1);
    },
  );

  test(
    'new scope replaces page without rebinding an older selection',
    () async {
      final backend = ControlledBackend()
        ..queuedPages.addAll([page(7, 21), page(9, 22)]);
      final session = MutationRecoverySession(backend, backend);
      final view = MutationRecoveryViewState.forSession(session);
      await session.refresh();
      final original = discover(1, 'scope.one');
      await session.startDiscovery(original);
      await session.read();
      view.selectPlan(0);
      await session.release(id(4));
      await session.read();
      await session.stop();
      backend.joined();
      await session.refresh();
      await session.acknowledge();

      final next = discover(5, 'scope.two');
      await session.startDiscovery(next);
      await session.read();
      expect(view.page!.plans!.single, [9]);
      expect(identical(view.pageRequest, next), isTrue);
      expect(view.checkpoint, id(22));
      expect(view.selectedPlan, [7]);
      expect(identical(view.selectedRequest, original), isTrue);
      view.selectPlan(0);
      expect(view.selectedPlan, [9]);
      expect(identical(view.selectedRequest, next), isTrue);
      view.clearSelection();
      expect(view.selectedPlan, isNull);
      expect(view.selectedRequest, isNull);
      expect(() => view.selectPlan(1), throwsRangeError);
    },
  );

  test(
    'delivered reconciliation result survives cleanup without history',
    () async {
      final backend = ControlledBackend();
      final session = MutationRecoverySession(backend, backend);
      final view = MutationRecoveryViewState.forSession(session);
      await session.refresh();
      await session.startReconciliation(reconcile(1));
      await session.read();
      expect(view.lastReconciliation!.kind, MutationResultKind.reconciled);
      expect(view.lastReconciliation!.phase, MutationPhase.outcomeUnknown);
      expect(view.page, isNull);
      await session.stop();
      backend.joined();
      await session.refresh();
      await session.acknowledge();
      expect(view.lastReconciliation!.kind, MutationResultKind.reconciled);
      expect(view.page, isNull);
      expect(backend.starts, 1);
    },
  );
}
