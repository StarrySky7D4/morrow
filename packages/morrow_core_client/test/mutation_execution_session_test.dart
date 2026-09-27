import 'dart:async';
import 'dart:typed_data';

import 'package:test/test.dart';

import '../../../lib/plugins/io_task_models.dart';
import '../../../lib/plugins/mutation_execution_session.dart';
import '../../../lib/plugins/mutation_task_models.dart';

Uint8List _id(int value) => Uint8List.fromList(List<int>.filled(32, value));
bool _same(List<int>? a, List<int>? b) =>
    a != null &&
    b != null &&
    a.length == b.length &&
    Iterable<int>.generate(a.length).every((i) => a[i] == b[i]);

MutationStartRequest _start(MutationDisposition kind, [int token = 1]) =>
    MutationStartRequest(
      submission: _id(token),
      packageId: 'org.example.mutation',
      packageDigest: _id(2),
      registryRevision: BigInt.one,
      disposition: kind,
      selectedPath: kind == MutationDisposition.create
          ? r'C:\selected-directory'
          : r'C:\selected-directory\old.bin',
      relativePath: kind == MutationDisposition.create ? 'new.bin' : '',
      subject: 'selected.subject',
      approvalSha256: _id(3),
      timeoutMs: 30000,
    );

MutationResult _result(
  MutationResultKind kind, {
  Uint8List? plan,
  BigInt? staged,
  bool durable = false,
  MutationPhase phase = MutationPhase.none,
  MutationEffect effect = MutationEffect.unspecified,
  MutationFailure? failure,
  String? operationId,
}) => MutationResult(
  kind: kind,
  plan: plan,
  stagedBytes: staged ?? BigInt.zero,
  durableContent: durable,
  phase: phase,
  operationId: operationId,
  effect: effect,
  osCode: null,
  scanned: 0,
  done: false,
  failure: failure,
);

final class _Controlled implements MutationTaskBackend, WorkbenchIoTaskControl {
  Uint8List? key, startToken;
  IoStoragePhase storage = IoStoragePhase.local;
  IoTaskExit? exit;
  BigInt command = BigInt.zero;
  MutationCommandKind kind = MutationCommandKind.select;
  MutationDelivery delivery = MutationDelivery.consumed;
  MutationResult? pending;
  final commands = <MutationCommandKind>[];
  final tokens = <Uint8List>[];
  final offsets = <BigInt>[];
  final chunkLengths = <int>[];
  final _dedup = <String, BigInt>{};
  MutationCommandKind? loseSubmit, loseRead, rejectAt;
  bool burnStart = false, foreignIo = false, foreignReply = false;
  bool wrongKind = false, ackFails = false, stopFails = false;
  bool foreignAckSubmission = false;
  bool retainAckSubmission = false;
  int starts = 0, executes = 0, reads = 0, stops = 0, acks = 0;
  Completer<void>? heldStart;
  Completer<void>? heldChunkSubmit, heldRead;
  MutationCommandKind? holdReadKind;
  String? forgedOperationId;
  MutationPhase historyPhase = MutationPhase.prepared;
  bool reconcileRequired = false, terminal = false;
  BigInt staged = BigInt.zero;

  String _tag(Uint8List bytes) => bytes.join(',');
  IoTaskSnapshot _io({bool foreign = false}) => IoTaskSnapshot(
    key: (foreign || foreignIo) && key != null ? _id(88) : key,
    submission: startToken,
    storage: storage,
    delivery: switch (delivery) {
      MutationDelivery.pending => IoDeliveryPhase.pending,
      MutationDelivery.ready => IoDeliveryPhase.ready,
      MutationDelivery.consumed => IoDeliveryPhase.consumed,
    },
    exit: exit,
  );
  MutationState _state({bool wrong = false}) => MutationState(
    commandId: command,
    kind: (wrong || wrongKind) ? MutationCommandKind.reconcile : kind,
    delivery: delivery,
    selected: command > BigInt.zero,
    reconcileRequired: reconcileRequired,
    terminal: terminal,
  );
  MutationTaskReply _reply({BigInt? receipt, bool foreign = false}) =>
      MutationTaskReply(
        io: _io(foreign: foreign || foreignReply),
        state: _state(),
        commandId: receipt ?? command,
      );

  Future<MutationTaskReply> _submit(
    Uint8List taskKey,
    Uint8List token,
    MutationCommandKind next,
    MutationResult result,
  ) async {
    expect(_same(taskKey, key), isTrue);
    commands.add(next);
    tokens.add(Uint8List.fromList(token));
    final previous = _dedup[_tag(token)];
    if (previous != null) return _reply(receipt: previous);
    expect(delivery, MutationDelivery.consumed);
    _dedup[_tag(token)] = command + BigInt.one;
    command += BigInt.one;
    kind = next;
    delivery = MutationDelivery.ready;
    pending = result;
    if (rejectAt == next) {
      pending = _result(
        MutationResultKind.failure,
        failure: const MutationFailure(
          layer: MutationFailureLayer.target,
          target: MutationTargetFailure.admission,
        ),
      );
    }
    if (next == MutationCommandKind.chunk) await heldChunkSubmit?.future;
    if (loseSubmit == next) throw StateError('submission receipt lost: $next');
    return _reply();
  }

  @override
  Future<MutationTaskReply> startSelected(MutationStartRequest request) async {
    starts++;
    tokens.add(request.submission);
    final old = _dedup[_tag(request.submission)];
    if (old != null) return _reply(receipt: old);
    _dedup[_tag(request.submission)] = BigInt.one;
    startToken = request.submission;
    if (burnStart) throw StateError('start rejected after burning token');
    key = _id(11);
    storage = IoStoragePhase.running;
    command = BigInt.one;
    kind = MutationCommandKind.select;
    delivery = MutationDelivery.ready;
    pending = _result(MutationResultKind.selected);
    await heldStart?.future;
    if (loseSubmit == MutationCommandKind.select) {
      throw StateError('start receipt lost');
    }
    return _reply();
  }

  @override
  Future<MutationTaskReply> submitBuildPlan(
    Uint8List taskKey,
    Uint8List submission, {
    required String operationId,
    required BigInt contentLength,
    Uint8List? contentSha256,
  }) {
    expect(operationId, 'owned-op');
    expect(contentLength >= BigInt.zero, isTrue);
    return _submit(
      taskKey,
      submission,
      MutationCommandKind.buildPlan,
      _result(MutationResultKind.planned, plan: Uint8List.fromList([1, 2, 3])),
    );
  }

  @override
  Future<MutationTaskReply> submitPrepare(
    Uint8List taskKey,
    Uint8List submission,
    Uint8List plan,
  ) {
    expect(plan, [1, 2, 3]);
    return _submit(
      taskKey,
      submission,
      MutationCommandKind.prepare,
      _result(
        MutationResultKind.prepared,
        phase: MutationPhase.prepared,
        operationId: forgedOperationId ?? 'owned-op',
      ),
    );
  }

  @override
  Future<MutationTaskReply> submitChunk(
    Uint8List taskKey,
    Uint8List submission,
    BigInt offset,
    Uint8List bytes,
  ) {
    expect(offset, staged);
    expect(
      bytes.length,
      lessThanOrEqualTo(MutationTaskValidation.maxChunkBytes),
    );
    offsets.add(offset);
    chunkLengths.add(bytes.length);
    staged += BigInt.from(bytes.length);
    return _submit(
      taskKey,
      submission,
      MutationCommandKind.chunk,
      _result(MutationResultKind.staged, staged: staged),
    );
  }

  @override
  Future<MutationTaskReply> submitCommitContent(
    Uint8List taskKey,
    Uint8List submission,
  ) => _submit(
    taskKey,
    submission,
    MutationCommandKind.commitContent,
    _result(MutationResultKind.staged, staged: staged, durable: true),
  );

  @override
  Future<MutationTaskReply> submitExecute(
    Uint8List taskKey,
    Uint8List submission,
  ) {
    executes++;
    return _submit(
      taskKey,
      submission,
      MutationCommandKind.execute,
      _result(
        staged == BigInt.zero && isDelete
            ? MutationResultKind.deleted
            : MutationResultKind.created,
        phase: MutationPhase.observed,
        effect: MutationEffect.osSucceeded,
        operationId: forgedOperationId ?? 'owned-op',
      ),
    );
  }

  bool isDelete = false;
  @override
  Future<MutationTaskReply> submitQuery(Uint8List key, Uint8List token) =>
      _submit(
        key,
        token,
        MutationCommandKind.query,
        _result(
          MutationResultKind.history,
          phase: historyPhase,
          operationId: 'owned-op',
        ),
      );
  @override
  Future<MutationTaskReply> submitCancelPlan(Uint8List key, Uint8List token) =>
      _submit(
        key,
        token,
        MutationCommandKind.cancelPlan,
        _result(MutationResultKind.planCancelled),
      );
  @override
  Future<MutationTaskReply> submitRelease(Uint8List key, Uint8List token) =>
      _submit(
        key,
        token,
        MutationCommandKind.release,
        _result(MutationResultKind.released),
      );
  @override
  Future<MutationTaskReply> status(Uint8List taskKey) async {
    expect(_same(taskKey, key), isTrue);
    return _reply(receipt: BigInt.zero);
  }

  @override
  Future<MutationTaskRead> read(Uint8List taskKey, BigInt commandId) async {
    expect(_same(taskKey, key), isTrue);
    expect(commandId, command);
    reads++;
    if (kind == holdReadKind) await heldRead?.future;
    delivery = MutationDelivery.consumed;
    final result = pending;
    pending = null;
    if (loseRead == kind) throw StateError('read receipt lost: $kind');
    return MutationTaskRead(reply: _reply(), result: result);
  }

  @override
  Future<IoTaskSnapshot> ioStatus() async => _io();
  @override
  Future<IoTaskSnapshot> cancelIo(Uint8List taskKey) async {
    expect(_same(taskKey, key), isTrue);
    stops++;
    if (stopFails) throw StateError('stop receipt lost');
    storage = IoStoragePhase.reclaimed;
    exit = const IoTaskExit(
      execution: IoJobError.none,
      disconnect: IoJobError.none,
      maintenance: IoJobError.none,
    );
    return _io();
  }

  @override
  Future<IoTaskSnapshot> repairIo(Uint8List taskKey) async {
    expect(_same(taskKey, key), isTrue);
    storage = IoStoragePhase.reclaimed;
    return _io();
  }

  @override
  Future<IoTaskSnapshot> acknowledgeIo(Uint8List taskKey) async {
    expect(_same(taskKey, key), isTrue);
    expect(storage, IoStoragePhase.reclaimed);
    expect(exit, isNotNull);
    acks++;
    if (ackFails) throw StateError('ack receipt lost');
    if (foreignAckSubmission) {
      return IoTaskSnapshot(
        key: null,
        submission: _id(99),
        storage: IoStoragePhase.local,
        delivery: IoDeliveryPhase.absent,
        exit: null,
      );
    }
    key = null;
    if (!retainAckSubmission) startToken = null;
    storage = IoStoragePhase.local;
    exit = null;
    return _io();
  }

  @override
  dynamic noSuchMethod(Invocation invocation) => super.noSuchMethod(invocation);
}

void main() {
  test(
    'Create stages multiple bounded chunks and never executes during prepare',
    () async {
      final backend = _Controlled();
      final session = MutationExecutionSession.forBackend(backend, backend);
      expect(
        identical(
          session,
          MutationExecutionSession.forBackend(backend, backend),
        ),
        isTrue,
      );
      var notifications = 0;
      session.addListener(() => notifications++);
      session.addListener(() => throw StateError('view failed'));
      await session.refresh();
      final content = Uint8List.fromList(
        List<int>.generate(130000, (i) => i & 255),
      );
      await session.prepare(
        _start(MutationDisposition.create),
        operationId: 'owned-op',
        content: content,
      );
      expect(session.error, isNull);
      expect(session.ready, isTrue);
      expect(session.stagedBytes, BigInt.from(content.length));
      expect(backend.chunkLengths, [61440, 61440, 7120]);
      expect(backend.offsets, [
        BigInt.zero,
        BigInt.from(61440),
        BigInt.from(122880),
      ]);
      expect(backend.executes, 0);
      expect(notifications, greaterThan(0));
      await session.execute();
      expect(backend.executes, 1);
      expect(session.canExecute, isFalse);
      expect(session.outcome?.effect, MutationEffect.osSucceeded);
      await session.release();
      await session.stop();
      expect(session.canAcknowledge, isTrue);
      await session.acknowledge();
      expect(session.canStart, isTrue);
      expect(session.request?.subject, 'selected.subject');
      expect(session.plan, [1, 2, 3]);
      expect(session.outcome?.kind, MutationResultKind.created);
    },
  );

  test(
    'Delete has no body commands and Execute remains a second explicit act',
    () async {
      final backend = _Controlled()..isDelete = true;
      final session = MutationExecutionSession(backend, backend);
      await session.refresh();
      await session.prepare(
        _start(MutationDisposition.delete),
        operationId: 'owned-op',
      );
      expect(session.ready, isTrue);
      expect(backend.commands, [
        MutationCommandKind.buildPlan,
        MutationCommandKind.prepare,
      ]);
      expect(backend.executes, 0);
      await session.execute();
      expect(session.outcome?.kind, MutationResultKind.deleted);
      expect(() => session.execute(), throwsStateError);
    },
  );

  test(
    'lost submission keeps exact token and does not continue preparation',
    () async {
      final backend = _Controlled()..loseSubmit = MutationCommandKind.buildPlan;
      final session = MutationExecutionSession(backend, backend);
      await session.refresh();
      await session.prepare(
        _start(MutationDisposition.create),
        operationId: 'owned-op',
      );
      expect(session.uncertain, isTrue);
      expect(session.ready, isFalse);
      expect(backend.commands, [MutationCommandKind.buildPlan]);
      expect(backend.executes, 0);
      final firstToken = backend.tokens.last;
      await session.refresh();
      expect(session.canRetrySubmission, isTrue);
      await session.retrySubmission();
      expect(backend.tokens.last, firstToken);
      expect(session.canRead, isTrue);
      await session.read();
      expect(
        session.ready,
        isFalse,
      ); // No implicit continuation after retry/read.
      expect(backend.commands, [
        MutationCommandKind.buildPlan,
        MutationCommandKind.buildPlan,
      ]);
      await session.stop();
      await session.acknowledge();
      expect(session.error, isNotNull);
    },
  );

  test('lost consumed read cannot continue or execute', () async {
    final backend = _Controlled()..loseRead = MutationCommandKind.prepare;
    final session = MutationExecutionSession(backend, backend);
    await session.refresh();
    await session.prepare(
      _start(MutationDisposition.create),
      operationId: 'owned-op',
    );
    expect(session.uncertain, isTrue);
    expect(session.ready, isFalse);
    expect(backend.commands, [
      MutationCommandKind.buildPlan,
      MutationCommandKind.prepare,
    ]);
    await session.refresh();
    expect(session.canExecute, isFalse);
    expect(backend.executes, 0);
    await session.stop();
    await session.acknowledge();
    expect(session.error, isNotNull);
  });

  test('foreign task and wrong command kind never enable effect', () async {
    final backend = _Controlled()..wrongKind = true;
    final session = MutationExecutionSession(backend, backend);
    await session.refresh();
    await session.prepare(
      _start(MutationDisposition.delete),
      operationId: 'owned-op',
    );
    expect(session.uncertain, isTrue);
    expect(session.canExecute, isFalse);
    backend.foreignIo = true;
    await session.refresh();
    expect(session.canStop, isFalse);
    expect(session.canAcknowledge, isFalse);
    expect(backend.executes, 0);
  });

  test('burned local start can only be abandoned after observation', () async {
    final backend = _Controlled()..burnStart = true;
    final session = MutationExecutionSession(backend, backend);
    await session.refresh();
    await session.prepare(
      _start(MutationDisposition.delete),
      operationId: 'owned-op',
    );
    expect(session.uncertain, isTrue);
    expect(session.canStart, isFalse);
    await session.refresh();
    expect(session.canAbandon, isTrue);
    await session.abandon();
    expect(session.canStart, isTrue);
    expect(session.error, isNotNull);
  });

  test(
    'revoked preparation retains primary error while cleanup and ack fail',
    () async {
      final backend = _Controlled()
        ..rejectAt = MutationCommandKind.prepare
        ..ackFails = true;
      final session = MutationExecutionSession(backend, backend);
      await session.refresh();
      await session.prepare(
        _start(MutationDisposition.delete),
        operationId: 'owned-op',
      );
      final original = session.error;
      expect(original, isNotNull);
      expect(session.canExecute, isFalse);
      await session.stop();
      await session.acknowledge();
      expect(session.error, same(original));
      expect(session.cleanupError, isNotNull);
      backend.ackFails = false;
      await session.refresh();
      expect(session.canAcknowledge, isTrue);
      await session.acknowledge();
      expect(session.error, same(original));
      expect(session.canStart, isTrue);
    },
  );

  test('stop intent blocks Execute when stop receipt is lost', () async {
    final backend = _Controlled()..stopFails = true;
    final session = MutationExecutionSession(backend, backend);
    await session.refresh();
    await session.prepare(
      _start(MutationDisposition.delete),
      operationId: 'owned-op',
    );
    expect(session.canExecute, isTrue);
    await session.stop();
    expect(session.canExecute, isFalse);
    expect(session.cleanupError, isNotNull);
    await session.refresh();
    expect(session.canExecute, isFalse);
    backend.stopFails = false;
    expect(session.canStop, isTrue);
    await session.stop();
    await session.acknowledge();
    expect(backend.executes, 0);
  });

  test(
    'busy concurrent calls reject while a selected start remains pending',
    () async {
      final backend = _Controlled()..heldStart = Completer<void>();
      final session = MutationExecutionSession(backend, backend);
      await session.refresh();
      final pending = session.prepare(
        _start(MutationDisposition.create),
        operationId: 'owned-op',
      );
      await Future<void>.delayed(Duration.zero);
      expect(session.busy, isTrue);
      await expectLater(session.refresh(), throwsStateError);
      await expectLater(session.execute(), throwsStateError);
      backend.heldStart!.complete();
      await pending;
      expect(session.ready, isTrue);
    },
  );

  test(
    'Stop interrupts a held chunk submission without another chunk or commit',
    () async {
      final backend = _Controlled()..heldChunkSubmit = Completer<void>();
      final session = MutationExecutionSession(backend, backend);
      await session.refresh();
      final pending = session.prepare(
        _start(MutationDisposition.create),
        operationId: 'owned-op',
        content: Uint8List(MutationTaskValidation.maxChunkBytes * 2 + 9),
      );
      for (var i = 0; i < 100 && backend.chunkLengths.isEmpty; i++) {
        await Future<void>.delayed(const Duration(milliseconds: 1));
      }
      expect(backend.chunkLengths, hasLength(1));
      expect(session.busy, isTrue);
      expect(session.canStop, isTrue);
      await session.stop();
      await pending;
      backend.heldChunkSubmit!.complete();
      await Future<void>.delayed(Duration.zero);
      expect(backend.chunkLengths, hasLength(1));
      expect(
        backend.commands,
        isNot(contains(MutationCommandKind.commitContent)),
      );
      expect(backend.executes, 0);
      expect(session.canAcknowledge, isTrue);
      await session.acknowledge();
    },
  );

  test(
    'Stop interrupts a held read without accepting its late receipt',
    () async {
      final backend = _Controlled()
        ..holdReadKind = MutationCommandKind.chunk
        ..heldRead = Completer<void>();
      final session = MutationExecutionSession(backend, backend);
      await session.refresh();
      final pending = session.prepare(
        _start(MutationDisposition.create),
        operationId: 'owned-op',
        content: Uint8List(MutationTaskValidation.maxChunkBytes + 1),
      );
      for (var i = 0; i < 100 && backend.chunkLengths.isEmpty; i++) {
        await Future<void>.delayed(const Duration(milliseconds: 1));
      }
      expect(backend.chunkLengths, hasLength(1));
      expect(session.canStop, isTrue);
      await session.stop();
      await pending;
      backend.heldRead!.complete();
      await Future<void>.delayed(Duration.zero);
      expect(backend.chunkLengths, hasLength(1));
      expect(
        backend.commands,
        isNot(contains(MutationCommandKind.commitContent)),
      );
      expect(session.canExecute, isFalse);
      await session.acknowledge();
    },
  );

  for (final phase in [
    MutationPhase.outcomeUnknown,
    MutationPhase.observed,
    MutationPhase.cancelledBeforeDispatch,
  ]) {
    test('Query $phase revokes Execute while retaining cleanup', () async {
      final backend = _Controlled()..historyPhase = phase;
      final session = MutationExecutionSession(backend, backend);
      await session.refresh();
      await session.prepare(
        _start(MutationDisposition.delete),
        operationId: 'owned-op',
      );
      expect(session.canExecute, isTrue);
      await session.query();
      expect(session.history?.phase, phase);
      expect(session.canExecute, isFalse);
      expect(session.canRelease, isTrue);
      expect(backend.executes, 0);
    });
  }

  test(
    'stopping storage and reconcileRequired status both disable Execute',
    () async {
      final backend = _Controlled();
      final session = MutationExecutionSession(backend, backend);
      await session.refresh();
      await session.prepare(
        _start(MutationDisposition.delete),
        operationId: 'owned-op',
      );
      expect(session.canExecute, isTrue);
      backend.storage = IoStoragePhase.stopping;
      await session.refresh();
      expect(session.canExecute, isFalse);
      backend.storage = IoStoragePhase.running;
      backend.reconcileRequired = true;
      await session.refresh();
      expect(session.canExecute, isFalse);
      expect(backend.executes, 0);
    },
  );

  test('foreign operation ID cannot establish readiness or success', () async {
    final preparedBackend = _Controlled()..forgedOperationId = 'other-op';
    final prepared = MutationExecutionSession(preparedBackend, preparedBackend);
    await prepared.refresh();
    await prepared.prepare(
      _start(MutationDisposition.create),
      operationId: 'owned-op',
      content: Uint8List(1),
    );
    expect(prepared.ready, isFalse);
    expect(prepared.uncertain, isTrue);
    expect(preparedBackend.chunkLengths, isEmpty);
    expect(preparedBackend.executes, 0);

    final executedBackend = _Controlled()..isDelete = true;
    final executed = MutationExecutionSession(executedBackend, executedBackend);
    await executed.refresh();
    await executed.prepare(
      _start(MutationDisposition.delete),
      operationId: 'owned-op',
    );
    executedBackend.forgedOperationId = 'other-op';
    await executed.execute();
    expect(executed.uncertain, isTrue);
    expect(executed.outcome, isNull);
    expect(executed.canExecute, isFalse);
  });

  test('confirmed stop and ACK allow a fresh explicit preparation', () async {
    final backend = _Controlled()..isDelete = true;
    final session = MutationExecutionSession.forBackend(backend, backend);
    await session.refresh();
    await session.prepare(
      _start(MutationDisposition.delete),
      operationId: 'owned-op',
    );
    await session.stop();
    await session.acknowledge();
    expect(session.canStart, isTrue);
    await session.prepare(
      _start(MutationDisposition.delete, 4),
      operationId: 'owned-op',
    );
    expect(session.ready, isTrue);
    expect(backend.starts, 2);
    expect(backend.executes, 0);
  });

  test('foreign Local ACK submission cannot clear the owned task', () async {
    final backend = _Controlled()..foreignAckSubmission = true;
    final session = MutationExecutionSession(backend, backend);
    await session.refresh();
    await session.prepare(
      _start(MutationDisposition.delete),
      operationId: 'owned-op',
    );
    await session.stop();
    expect(session.canAcknowledge, isTrue);
    await session.acknowledge();
    expect(session.cleanupError, isA<FormatException>());
    expect(session.snapshot?.key, _id(11));
    expect(session.request?.submission, _id(1));
    expect(session.canStart, isFalse);
    backend.foreignAckSubmission = false;
    await session.refresh();
    expect(session.canAcknowledge, isTrue);
    await session.acknowledge();
    expect(session.canStart, isTrue);
  });

  test(
    'matching historical Local submission remains safe after ACK and refresh',
    () async {
      final backend = _Controlled()..retainAckSubmission = true;
      final session = MutationExecutionSession(backend, backend);
      await session.refresh();
      await session.prepare(
        _start(MutationDisposition.delete),
        operationId: 'owned-op',
      );
      await session.stop();
      await session.acknowledge();
      expect(session.canStart, isTrue);
      expect(session.snapshot?.submission, _id(1));
      await session.refresh();
      expect(session.canStart, isTrue);
      expect(session.cleanupError, isNull);
    },
  );
}
