import 'dart:async';
import 'dart:typed_data';

import 'package:test/test.dart';

import '../../../lib/plugins/io_task_models.dart';
import '../../../lib/plugins/mutation_recovery_session.dart';
import '../../../lib/plugins/mutation_task_models.dart';

Uint8List id(int value) => Uint8List.fromList(List.filled(32, value));

bool same(List<int>? a, List<int>? b) =>
    a != null &&
    b != null &&
    a.length == b.length &&
    Iterable<int>.generate(a.length).every((i) => a[i] == b[i]);

MutationDiscoverRequest discover([int token = 1, Uint8List? checkpoint]) =>
    MutationDiscoverRequest(
      submission: id(token),
      packageId: 'org.example.recovery',
      packageDigest: id(2),
      registryRevision: BigInt.one,
      subject: 'mutation.recovery',
      disposition: MutationDisposition.create,
      scanLimit: 1,
      timeoutMs: 1000,
      checkpoint: checkpoint,
    );

MutationReconcileRequest reconcile([int token = 1]) => MutationReconcileRequest(
  submission: id(token),
  packageId: 'org.example.recovery',
  packageDigest: id(2),
  registryRevision: BigInt.one,
  plan: Uint8List.fromList([1, 2, 3]),
  timeoutMs: 1000,
);

MutationResult result(
  MutationResultKind kind, {
  List<Uint8List>? plans,
  Uint8List? checkpoint,
  int scanned = 0,
  bool done = false,
  MutationPhase phase = MutationPhase.none,
  MutationEffect effect = MutationEffect.unspecified,
}) => MutationResult(
  kind: kind,
  plans: plans,
  checkpoint:
      checkpoint ?? (kind == MutationResultKind.plans && !done ? id(17) : null),
  stagedBytes: BigInt.zero,
  durableContent: false,
  phase: phase,
  operationId: null,
  effect: effect,
  osCode: null,
  scanned: scanned,
  done: done,
);

MutationResult page(
  List<int> value, {
  bool done = false,
  Uint8List? checkpoint,
}) => result(
  MutationResultKind.plans,
  plans: [Uint8List.fromList(value)],
  scanned: 1,
  done: done,
  checkpoint: checkpoint,
);

class ControlledBackend implements MutationTaskBackend, WorkbenchIoTaskControl {
  Uint8List? key, submission;
  IoStoragePhase storage = IoStoragePhase.local;
  IoTaskExit? exit;
  MutationCommandKind kind = MutationCommandKind.discover;
  MutationDelivery delivery = MutationDelivery.pending;
  bool terminal = false;
  BigInt command = BigInt.one;
  MutationResult? pendingResult;
  final pages = <MutationResult>[];
  MutationResult reconciliationResult = result(
    MutationResultKind.reconciled,
    phase: MutationPhase.outcomeUnknown,
  );
  bool loseStart = false,
      loseNext = false,
      loseRelease = false,
      loseRead = false,
      loseAck = false,
      loseAckBeforeEffect = false,
      loseStopBeforeEffect = false,
      burnStart = false,
      cleanupStart = false,
      autoJoin = true,
      failStatus = false,
      failMutationStatus = false;
  bool returnForeignKey = false,
      returnForeignSubmission = false,
      returnWrongCommand = false,
      returnWrongStatusKind = false;
  Completer<void>? heldStart, heldRead;
  int starts = 0,
      nexts = 0,
      releases = 0,
      reads = 0,
      statuses = 0,
      polls = 0,
      stops = 0,
      repairs = 0,
      acks = 0;
  final submissions = <Uint8List>[];
  final discoveryStarts = <MutationDiscoverRequest>[];
  final nextLimits = <int>[];
  final _dedup = <String, BigInt>{};

  IoTaskSnapshot ioSnapshot() => IoTaskSnapshot(
    key: returnForeignKey && key != null ? id(98) : key,
    submission: returnForeignSubmission && submission != null
        ? id(99)
        : submission,
    storage: storage,
    delivery: switch (delivery) {
      MutationDelivery.pending => IoDeliveryPhase.pending,
      MutationDelivery.ready => IoDeliveryPhase.ready,
      MutationDelivery.consumed => IoDeliveryPhase.consumed,
    },
    exit: exit,
  );

  MutationState mutationState({MutationCommandKind? overrideKind}) =>
      MutationState(
        commandId: returnWrongCommand ? command + BigInt.one : command,
        kind: overrideKind ?? kind,
        delivery: delivery,
        selected: false,
        reconcileRequired: false,
        terminal: terminal,
      );

  MutationTaskReply reply({
    BigInt? returnedCommand,
    MutationCommandKind? overrideKind,
  }) => MutationTaskReply(
    io: ioSnapshot(),
    state: mutationState(overrideKind: overrideKind),
    commandId: returnedCommand ?? command,
  );

  String tag(List<int> bytes) => bytes.join(',');

  Future<MutationTaskReply> _start(
    Uint8List token,
    MutationCommandKind started,
  ) async {
    starts++;
    submissions.add(Uint8List.fromList(token));
    final prior = _dedup[tag(token)];
    if (prior != null) return reply(returnedCommand: prior);
    _dedup[tag(token)] = BigInt.one;
    if (burnStart) {
      submission = Uint8List.fromList(token);
      storage = IoStoragePhase.local;
      delivery = MutationDelivery.consumed;
      throw StateError('start rejected after burning submission');
    }
    key = id(11);
    submission = Uint8List.fromList(token);
    storage = cleanupStart
        ? IoStoragePhase.recoveryRequired
        : IoStoragePhase.running;
    exit = cleanupStart
        ? const IoTaskExit(
            execution: IoJobError.none,
            disconnect: IoJobError.none,
            maintenance: IoJobError.busy,
          )
        : null;
    kind = started;
    command = BigInt.one;
    delivery = cleanupStart
        ? MutationDelivery.consumed
        : MutationDelivery.ready;
    pendingResult = cleanupStart
        ? null
        : started == MutationCommandKind.discover
        ? pages.removeAt(0)
        : reconciliationResult;
    terminal = pendingResult?.done ?? false;
    await heldStart?.future;
    if (loseStart || cleanupStart)
      throw StateError('start acknowledgement lost');
    return reply();
  }

  @override
  Future<MutationTaskReply> startDiscovery(MutationDiscoverRequest request) {
    discoveryStarts.add(request);
    return _start(request.submission, MutationCommandKind.discover);
  }

  @override
  Future<MutationTaskReply> startReconciliation(
    MutationReconcileRequest request,
  ) => _start(request.submission, MutationCommandKind.reconcile);

  @override
  Future<MutationTaskReply> submitNextPlans(
    Uint8List taskKey,
    Uint8List token,
    int scanLimit,
  ) async {
    nexts++;
    nextLimits.add(scanLimit);
    submissions.add(Uint8List.fromList(token));
    final prior = _dedup[tag(token)];
    if (prior != null) return reply(returnedCommand: prior);
    _dedup[tag(token)] = command + BigInt.one;
    command += BigInt.one;
    kind = MutationCommandKind.discover;
    delivery = MutationDelivery.ready;
    pendingResult = pages.removeAt(0);
    terminal = pendingResult!.done;
    if (loseNext) throw StateError('next acknowledgement lost');
    return reply();
  }

  @override
  Future<MutationTaskReply> submitRelease(
    Uint8List taskKey,
    Uint8List token,
  ) async {
    releases++;
    submissions.add(Uint8List.fromList(token));
    final prior = _dedup[tag(token)];
    if (prior != null) return reply(returnedCommand: prior);
    _dedup[tag(token)] = command + BigInt.one;
    command += BigInt.one;
    kind = MutationCommandKind.release;
    delivery = MutationDelivery.ready;
    pendingResult = result(MutationResultKind.released);
    terminal = true;
    if (loseRelease) throw StateError('release acknowledgement lost');
    return reply();
  }

  @override
  Future<MutationTaskReply> status(Uint8List taskKey) async {
    statuses++;
    if (failMutationStatus) throw StateError('mutation status unavailable');
    return reply(
      returnedCommand: BigInt.zero,
      overrideKind: returnWrongStatusKind
          ? MutationCommandKind.reconcile
          : null,
    );
  }

  @override
  Future<MutationTaskRead> read(Uint8List taskKey, BigInt commandId) async {
    reads++;
    await heldRead?.future;
    if (delivery == MutationDelivery.pending) {
      return MutationTaskRead(reply: reply(), result: null);
    }
    final value = pendingResult;
    delivery = MutationDelivery.consumed;
    pendingResult = null;
    if (loseRead) throw StateError('result delivery lost');
    return MutationTaskRead(reply: reply(), result: value);
  }

  @override
  Future<IoTaskSnapshot> ioStatus() async {
    statuses++;
    if (failStatus) throw StateError('IO status unavailable');
    return ioSnapshot();
  }

  @override
  Future<IoTaskSnapshot> pollIo(Uint8List taskKey) async {
    polls++;
    if (storage == IoStoragePhase.stopping && autoJoin) joined();
    return ioSnapshot();
  }

  void joined({bool recovery = false}) {
    storage = recovery
        ? IoStoragePhase.recoveryRequired
        : IoStoragePhase.reclaimed;
    exit = IoTaskExit(
      execution: IoJobError.none,
      disconnect: IoJobError.none,
      maintenance: recovery ? IoJobError.busy : IoJobError.none,
    );
  }

  @override
  Future<IoTaskSnapshot> cancelIo(Uint8List taskKey) async {
    stops++;
    if (loseStopBeforeEffect) throw StateError('stop was not applied');
    storage = IoStoragePhase.stopping;
    return ioSnapshot();
  }

  @override
  Future<IoTaskSnapshot> repairIo(Uint8List taskKey) async {
    repairs++;
    joined();
    return ioSnapshot();
  }

  @override
  Future<IoTaskSnapshot> acknowledgeIo(Uint8List taskKey) async {
    acks++;
    if (loseAckBeforeEffect) throw StateError('acknowledgement not applied');
    key = null;
    submission = null;
    storage = IoStoragePhase.local;
    exit = null;
    if (loseAck) throw StateError('acknowledgement reply lost');
    return ioSnapshot();
  }

  @override
  dynamic noSuchMethod(Invocation invocation) => super.noSuchMethod(invocation);
}

void main() {
  test(
    'discovery pages replace bounded state and survive listener detachment',
    () async {
      final backend = ControlledBackend()
        ..pages.addAll([
          page([1, 2], checkpoint: id(21)),
          page([3, 4], done: true),
        ]);
      final session = MutationRecoverySession.forBackend(backend, backend);
      expect(
        identical(
          session,
          MutationRecoverySession.forBackend(backend, backend),
        ),
        isTrue,
      );
      var notified = 0;
      void listener() => notified++;
      session.addListener(listener);
      await session.refresh();
      await session.startDiscovery(discover());
      expect(session.canRead, isTrue);
      await session.read();
      expect(session.page!.plans!.single, [1, 2]);
      final retainedCheckpoint = session.checkpoint!;
      expect(retainedCheckpoint, id(21));
      expect(() => retainedCheckpoint[0] = 1, throwsUnsupportedError);
      expect(session.canNext, isTrue);
      session.removeListener(listener);
      final count = notified;
      await session.nextPage(id(3), 1);
      expect(session.checkpoint, id(21));
      await session.read();
      expect(session.page!.plans!.single, [3, 4]);
      expect(session.page!.done, isTrue);
      expect(session.checkpoint, isNull);
      expect(session.canNext, isFalse);
      expect(notified, count);
      expect(
        identical(
          session,
          MutationRecoverySession.forBackend(backend, backend),
        ),
        isTrue,
      );
      await session.release(id(4));
      await session.read();
      await session.refresh();
      expect(session.canAcknowledge, isFalse);
      await session.stop();
      backend.joined();
      await session.refresh();
      expect(session.canAcknowledge, isTrue);
      await session.acknowledge();
      expect(session.canStart, isTrue);
      expect(retainedCheckpoint, id(21));
    },
  );

  test('continuation requires a fresh explicit start after cleanup', () async {
    final backend = ControlledBackend()
      ..pages.addAll([
        page([1], checkpoint: id(24)),
        page([2], done: true),
      ]);
    final session = MutationRecoverySession(backend, backend);
    await session.refresh();
    await session.startDiscovery(discover());
    await session.read();
    final saved = session.checkpoint!;
    await session.release(id(3));
    await session.read();
    await session.stop();
    backend.joined();
    await session.refresh();
    await session.acknowledge();
    expect(backend.discoveryStarts.length, 1);
    expect(session.checkpoint, isNull);
    expect(saved, id(24));

    await session.startDiscovery(discover(9, saved));
    expect(backend.discoveryStarts.length, 2);
    expect(backend.discoveryStarts.last.submission, id(9));
    expect(backend.discoveryStarts.last.checkpoint, id(24));
    await session.read();
    expect(session.page!.plans!.single, [2]);
    expect(session.page!.done, isTrue);
  });

  test('empty nonterminal page still allows an explicit next page', () async {
    final backend = ControlledBackend()
      ..pages.addAll([
        result(MutationResultKind.plans, plans: [], scanned: 1, done: false),
        page([7], done: true),
      ]);
    final session = MutationRecoverySession(backend, backend);
    await session.refresh();
    await session.startDiscovery(discover());
    await session.read();
    expect(session.page!.plans, isEmpty);
    expect(session.canNext, isTrue);
    await session.nextPage(id(3), 1);
    await session.read();
    expect(session.page!.plans!.single, [7]);
  });

  test(
    'uncertain start, next, and release preserve exact submission for explicit retry',
    () async {
      final backend = ControlledBackend()
        ..pages.addAll([
          page([1], checkpoint: id(21)),
          page([2], checkpoint: id(22)),
        ])
        ..loseStart = true;
      final session = MutationRecoverySession(backend, backend);
      await session.refresh();
      await session.startDiscovery(discover());
      await session.refresh();
      expect(session.canRetrySubmission, isTrue);
      expect(session.canRead, isFalse);
      expect(session.canNext, isFalse);
      expect(backend.starts, 1);
      backend.loseStart = false;
      await session.retrySubmission();
      expect(backend.starts, 2);
      expect(same(backend.submissions[0], backend.submissions[1]), isTrue);
      expect(
        identical(backend.discoveryStarts[0], backend.discoveryStarts[1]),
        isTrue,
      );
      await session.read();
      expect(session.checkpoint, id(21));
      backend.loseNext = true;
      await session.nextPage(id(3), 1);
      await session.refresh();
      expect(backend.nexts, 1);
      expect(session.canRetrySubmission, isTrue);
      expect(session.canRead, isFalse);
      expect(session.canNext, isFalse);
      expect(session.checkpoint, id(21));
      backend.loseNext = false;
      await session.retrySubmission();
      expect(backend.nexts, 2);
      expect(same(backend.submissions[2], backend.submissions[3]), isTrue);
      expect(backend.nextLimits, [1, 1]);
      await session.read();
      expect(session.checkpoint, id(22));
      backend.loseRelease = true;
      await session.release(id(4));
      await session.refresh();
      expect(backend.releases, 1);
      expect(session.canRetrySubmission, isTrue);
      expect(session.canRead, isFalse);
      backend.loseRelease = false;
      await session.retrySubmission();
      expect(backend.releases, 2);
      expect(same(backend.submissions[4], backend.submissions[5]), isTrue);
    },
  );

  test(
    'lost consumed read cannot be replayed or advanced after refresh',
    () async {
      final backend = ControlledBackend()
        ..pages.addAll([
          page([1]),
          page([2]),
        ]);
      final session = MutationRecoverySession(backend, backend);
      await session.refresh();
      await session.startDiscovery(discover());
      backend.loseRead = true;
      await session.read();
      await session.refresh();
      expect(session.phase, MutationRecoveryPhase.resultLost);
      expect(session.canRead, isFalse);
      expect(session.canNext, isFalse);
      expect(backend.reads, 1);
      expect(backend.nexts, 0);
    },
  );

  test('pending then ready status needs explicit read', () async {
    final backend = ControlledBackend()..pages.add(page([1]));
    final session = MutationRecoverySession(backend, backend);
    await session.refresh();
    await session.startDiscovery(discover());
    backend.delivery = MutationDelivery.pending;
    await session.refresh();
    expect(backend.reads, 0);
    backend.delivery = MutationDelivery.ready;
    await session.refresh();
    expect(backend.reads, 0);
    expect(session.canRead, isTrue);
    await session.read();
    expect(backend.reads, 1);
  });

  test('foreign identity and command replies are never adopted', () async {
    for (final mutation in <void Function(ControlledBackend)>[
      (b) => b.returnForeignKey = true,
      (b) => b.returnForeignSubmission = true,
      (b) => b.returnWrongCommand = true,
    ]) {
      final backend = ControlledBackend()..pages.add(page([1]));
      final session = MutationRecoverySession(backend, backend);
      await session.refresh();
      mutation(backend);
      await session.startDiscovery(discover());
      expect(session.page, isNull);
      expect(session.canNext, isFalse);
    }
  });

  test(
    'matching cleanup-only failed start requires repair, real exit and ack',
    () async {
      final backend = ControlledBackend()
        ..pages.add(page([1]))
        ..cleanupStart = true
        ..failMutationStatus = true
        ..autoJoin = false;
      final session = MutationRecoverySession(backend, backend);
      await session.refresh();
      await session.startDiscovery(discover());
      await session.refresh();
      expect(session.canRead, isFalse);
      expect(session.canNext, isFalse);
      expect(session.canAcknowledge, isFalse);
      expect(session.canStop, isFalse);
      expect(session.canRepair, isTrue);
      expect(session.canAcknowledge, isFalse);
      await session.repair();
      expect(session.canAcknowledge, isTrue);
      await session.acknowledge();
      expect(session.canStart, isTrue);
    },
  );

  test(
    'lost ack is resolved from local status without sending another ack',
    () async {
      final backend = ControlledBackend()
        ..pages.add(page([1], done: true))
        ..loseAck = true;
      final session = MutationRecoverySession(backend, backend);
      await session.refresh();
      await session.startDiscovery(discover());
      await session.read();
      await session.release(id(3));
      await session.read();
      await session.stop();
      backend.joined();
      await session.refresh();
      expect(session.canAcknowledge, isTrue);
      await session.acknowledge();
      expect(backend.acks, 1);
      await session.refresh();
      expect(session.canStart, isTrue);
      expect(backend.acks, 1);
    },
  );

  test(
    'lost ack before effect requires explicit retry on the same task',
    () async {
      final backend = ControlledBackend()
        ..pages.add(page([1], done: true))
        ..loseAckBeforeEffect = true;
      final session = MutationRecoverySession(backend, backend);
      await session.refresh();
      await session.startDiscovery(discover());
      await session.read();
      await session.release(id(3));
      await session.read();
      await session.stop();
      backend.joined();
      await session.refresh();
      expect(session.canAcknowledge, isTrue);
      await session.acknowledge();
      expect(backend.acks, 1);
      expect(session.canAcknowledge, isFalse);
      await session.refresh();
      expect(session.canAcknowledge, isTrue);
      expect(session.canStart, isFalse);
      backend.loseAckBeforeEffect = false;
      await session.acknowledge();
      expect(backend.acks, 2);
      expect(session.canStart, isTrue);
    },
  );

  test(
    'burned Local submission can be abandoned for a new explicit start',
    () async {
      final backend = ControlledBackend()
        ..pages.add(page([1], done: true))
        ..burnStart = true;
      final session = MutationRecoverySession(backend, backend);
      await session.refresh();
      await session.startDiscovery(discover());
      expect(backend.starts, 1);
      await session.refresh();
      expect(session.canStart, isFalse);
      expect(session.canRead, isFalse);
      expect(session.canAbandon, isTrue);
      expect(backend.key, isNull);
      expect(same(backend.submission, discover().submission), isTrue);
      final statusCalls = backend.statuses;
      await session.abandon();
      expect(backend.statuses, statusCalls);
      expect(session.canStart, isTrue);
      backend.burnStart = false;
      await session.startDiscovery(discover(9));
      await session.read();
      expect(session.page!.plans!.single, [1]);
      expect(backend.starts, 2);
      expect(same(backend.submissions[0], backend.submissions[1]), isFalse);
    },
  );

  test(
    'foreign historical Local token permits only local abandonment',
    () async {
      final backend = ControlledBackend()
        ..pages.add(page([1]))
        ..burnStart = true;
      final session = MutationRecoverySession(backend, backend);
      await session.refresh();
      await session.startDiscovery(discover());
      backend.submission = id(99);
      await session.refresh();
      expect(session.canAbandon, isTrue);
      expect(session.canRetrySubmission, isFalse);
      expect(session.canStart, isFalse);
      expect(session.canRead, isFalse);
      final starts = backend.starts;
      final statuses = backend.statuses;
      await session.abandon();
      expect(session.canStart, isTrue);
      expect(backend.starts, starts);
      expect(backend.statuses, statuses);
      expect(backend.submission, id(99));
    },
  );

  test('lost stop blocks mutation flow until an explicit stop retry', () async {
    final backend = ControlledBackend()
      ..pages.add(page([1]))
      ..loseStopBeforeEffect = true;
    final session = MutationRecoverySession(backend, backend);
    await session.refresh();
    await session.startDiscovery(discover());
    expect(session.canRead, isTrue);
    await session.stop();
    expect(backend.stops, 1);
    await session.refresh();
    expect(session.canRead, isFalse);
    expect(session.canNext, isFalse);
    expect(session.canRelease, isFalse);
    expect(session.canRetrySubmission, isFalse);
    expect(session.canStop, isTrue);
    backend.loseStopBeforeEffect = false;
    await session.stop();
    expect(backend.stops, 2);
    backend.joined();
    await session.refresh();
    expect(session.canAcknowledge, isTrue);
    await session.acknowledge();
  });

  test('same-command wrong-kind status does not enable read', () async {
    final backend = ControlledBackend()..pages.add(page([1]));
    final session = MutationRecoverySession(backend, backend);
    await session.refresh();
    await session.startDiscovery(discover());
    backend.returnWrongStatusKind = true;
    await session.refresh();
    expect(session.canRead, isFalse);
    expect(backend.reads, 0);
    await expectLater(session.read(), throwsStateError);
  });

  test(
    'busy operations reject overlap and throwing listeners cannot erase a known page',
    () async {
      final backend = ControlledBackend()..pages.add(page([1], done: true));
      final session = MutationRecoverySession(backend, backend);
      await session.refresh();
      session.addListener(() => throw StateError('view failed'));
      final held = backend.heldStart = Completer<void>();
      final starting = session.startDiscovery(discover());
      await Future<void>.delayed(Duration.zero);
      await expectLater(session.startDiscovery(discover(4)), throwsStateError);
      held.complete();
      await starting;
      await session.read();
      expect(session.page!.plans!.single, [1]);
      expect(session.phase, isNot(MutationRecoveryPhase.unknown));
    },
  );

  test(
    'unknown reconciliation is retained as unknown, never success',
    () async {
      final backend = ControlledBackend();
      final session = MutationRecoverySession(backend, backend);
      await session.refresh();
      await session.startReconciliation(reconcile());
      await session.read();
      expect(session.reconciliation!.phase, MutationPhase.outcomeUnknown);
      expect(session.reconciliation!.effect, MutationEffect.unspecified);
      expect(session.canNext, isFalse);
    },
  );
}
