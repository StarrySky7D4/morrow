import 'dart:async';
import 'dart:typed_data';

import 'package:crypto/crypto.dart';
import 'package:test/test.dart';

import '../../../lib/plugins/guest_mutation_execution_session.dart';
import '../../../lib/plugins/guest_mutation_models.dart';
import '../../../lib/plugins/io_task_models.dart';
import '../../../lib/plugins/mutation_task_models.dart';

Uint8List _token(int value) => Uint8List.fromList(List.filled(32, value));
Uint8List _hash(List<int> value) =>
    Uint8List.fromList(sha256.convert(value).bytes);

GuestMutationStartRequest _request({int submission = 1}) =>
    GuestMutationStartRequest(
      selection: MutationStartRequest(
        submission: _token(submission),
        packageId: 'org.example.guest',
        packageDigest: _token(2),
        registryRevision: BigInt.one,
        disposition: MutationDisposition.create,
        selectedPath: r'C:\chosen',
        relativePath: 'new.bin',
        subject: 'guest.mutation',
        approvalSha256: _token(4),
        timeoutMs: 1000,
      ),
    );

MutationResult _owner(MutationResultKind kind, {Uint8List? plan}) =>
    MutationResult(
      kind: kind,
      reference: kind == MutationResultKind.selected ? _token(7) : null,
      expectedIdentity: kind == MutationResultKind.selected ? _token(8) : null,
      plan: plan,
      stagedBytes: BigInt.zero,
      durableContent: false,
      phase: MutationPhase.none,
      operationId: null,
      effect: MutationEffect.unspecified,
      osCode: null,
      scanned: 0,
      done: false,
    );

class _Host implements GuestMutationBackend, WorkbenchIoTaskControl {
  final key = _token(3);
  final lease = _token(6);
  final plan = Uint8List.fromList([1, 2, 3, 4]);
  final sent = <GuestMutationCommandKind>[];
  final tokens = <Uint8List>[];
  final chunks = <Uint8List>[];
  GuestMutationStartRequest? request;
  GuestMutationCommand? command;
  GuestMutationReceipt? receipt;
  GuestMutationResult? pending;
  BigInt commandId = BigInt.zero;
  GuestMutationCommandKind kind = GuestMutationCommandKind.select;
  GuestMutationDelivery delivery = GuestMutationDelivery.pending;
  IoStoragePhase storage = IoStoragePhase.local;
  IoTaskExit? exit;
  bool selected = false;
  bool prepared = false;
  bool durable = false;
  bool terminal = false;
  bool reconcileRequired = false;
  bool loseNextSubmit = false;
  bool loseNextStart = false;
  bool loseNextReadAfterConsume = false;
  bool loseNextReadBeforeConsume = false;
  bool wrongChunkEnd = false;
  bool denyPrepare = false;
  bool executeUnknown = false;
  GuestMutationFramePhase queryPhase = GuestMutationFramePhase.prepared;
  GuestMutationFrameStatus queryStatus = GuestMutationFrameStatus.completed;
  Completer<GuestMutationTaskReply>? stallChunk;
  Completer<void>? chunkSubmitted;
  BigInt staged = BigInt.zero;

  IoTaskSnapshot get ioState => IoTaskSnapshot(
    key: storage == IoStoragePhase.local ? null : key,
    submission: request?.selection.submission,
    storage: storage,
    delivery: IoDeliveryPhase.consumed,
    exit: exit,
  );
  GuestMutationState get state => GuestMutationState(
    commandId: commandId,
    kind: kind,
    delivery: delivery,
    selected: selected,
    reference: selected ? _token(7) : null,
    expectedIdentity: selected ? _token(8) : null,
    reviewedPlanSha256: sent.contains(GuestMutationCommandKind.buildPlan)
        ? _hash(plan)
        : null,
    approvalDelivered: prepared,
    permitDelivered: sent.contains(GuestMutationCommandKind.execute),
    stagedBytes: staged,
    durableContent: durable,
    effectAttempted: sent.contains(GuestMutationCommandKind.execute),
    reconcileRequired: reconcileRequired,
    terminal: terminal,
  );

  GuestMutationTaskReply _reply(GuestMutationReceipt r) =>
      GuestMutationTaskReply(
        io: ioState,
        state: state,
        commandId: commandId,
        receipt: r,
      );

  @override
  Future<GuestMutationTaskReply> start(GuestMutationStartRequest value) async {
    request = value;
    storage = IoStoragePhase.running;
    commandId = BigInt.one;
    kind = GuestMutationCommandKind.select;
    delivery = GuestMutationDelivery.ready;
    selected = true;
    sent.add(kind);
    pending = GuestMutationResult(
      kind: GuestMutationResultKind.owner,
      owner: _owner(MutationResultKind.selected),
    );
    receipt = GuestMutationReceipt(
      key: key,
      submission: value.selection.submission,
      commandId: commandId,
      kind: kind,
    );
    if (loseNextStart) {
      loseNextStart = false;
      throw StateError('lost start reply');
    }
    return _reply(receipt!);
  }

  @override
  Future<GuestMutationTaskReply> submit(
    Uint8List taskKey,
    Uint8List submission,
    GuestMutationCommand value,
  ) async {
    expect(taskKey, key);
    if (receipt != null &&
        receipt!.kind == value.kind &&
        GuestMutationValidation.same(receipt!.submission, submission)) {
      return _reply(receipt!); // Host dedup: same token, same command.
    }
    commandId += BigInt.one;
    kind = value.kind;
    command = value;
    delivery = GuestMutationDelivery.ready;
    sent.add(kind);
    tokens.add(Uint8List.fromList(submission));
    receipt = GuestMutationReceipt(
      key: key,
      submission: submission,
      commandId: commandId,
      kind: kind,
      operationId: value.operationId,
      disposition: value.disposition,
      expectedGuestReference: value.expectedGuestReference,
      expectedChunkEnd: value.kind == GuestMutationCommandKind.chunk
          ? value.offset! +
                BigInt.from(value.bytes!.length) +
                (wrongChunkEnd ? BigInt.one : BigInt.zero)
          : null,
    );
    if (value.kind == GuestMutationCommandKind.buildPlan) {
      pending = GuestMutationResult(
        kind: GuestMutationResultKind.owner,
        owner: _owner(MutationResultKind.planned, plan: plan),
      );
    } else if (value.kind == GuestMutationCommandKind.hostQuery) {
      pending = GuestMutationResult(
        kind: GuestMutationResultKind.owner,
        owner: _owner(MutationResultKind.history),
      );
    } else if (value.kind == GuestMutationCommandKind.hostRelease) {
      pending = GuestMutationResult(
        kind: GuestMutationResultKind.owner,
        owner: _owner(MutationResultKind.released),
      );
    } else {
      if (value.kind == GuestMutationCommandKind.chunk) {
        chunks.add(Uint8List.fromList(value.bytes!));
        staged = value.offset! + BigInt.from(value.bytes!.length);
        if (chunkSubmitted != null && !chunkSubmitted!.isCompleted) {
          chunkSubmitted!.complete();
        }
      }
      if (value.kind == GuestMutationCommandKind.commitContent) durable = true;
      pending = GuestMutationResult(
        kind: GuestMutationResultKind.frame,
        frame: GuestMutationFrame(
          callId: commandId,
          reference: lease,
          submission: submission,
          operationId: value.operationId!,
          kind: GuestMutationValidation.frameKind(kind, value.disposition),
          status: kind == GuestMutationCommandKind.execute && executeUnknown
              ? GuestMutationFrameStatus.outcomeUnknown
              : kind == GuestMutationCommandKind.prepare && denyPrepare
              ? GuestMutationFrameStatus.denied
              : kind == GuestMutationCommandKind.query
              ? queryStatus
              : GuestMutationFrameStatus.completed,
          phase: kind == GuestMutationCommandKind.execute && executeUnknown
              ? GuestMutationFramePhase.outcomeUnknown
              : kind == GuestMutationCommandKind.prepare && denyPrepare
              ? GuestMutationFramePhase.none
              : kind == GuestMutationCommandKind.execute
              ? GuestMutationFramePhase.observed
              : kind == GuestMutationCommandKind.release
              ? GuestMutationFramePhase.none
              : kind == GuestMutationCommandKind.query
              ? queryPhase
              : GuestMutationFramePhase.prepared,
          effect:
              (kind == GuestMutationCommandKind.execute && !executeUnknown) ||
                  (kind == GuestMutationCommandKind.query &&
                      queryPhase == GuestMutationFramePhase.observed)
              ? GuestMutationFrameEffect.osSucceeded
              : GuestMutationFrameEffect.unspecified,
          stagedBytes:
              kind == GuestMutationCommandKind.execute ||
                  kind == GuestMutationCommandKind.release ||
                  (kind == GuestMutationCommandKind.query &&
                      queryPhase != GuestMutationFramePhase.prepared)
              ? BigInt.zero
              : staged,
          durableContent:
              kind == GuestMutationCommandKind.commitContent ||
              (kind == GuestMutationCommandKind.query &&
                  queryPhase == GuestMutationFramePhase.prepared &&
                  durable),
          encoded: Uint8List.fromList([1]),
        ),
      );
    }
    if (loseNextSubmit) {
      loseNextSubmit = false;
      throw StateError('lost submit reply');
    }
    if (value.kind == GuestMutationCommandKind.chunk && stallChunk != null) {
      return stallChunk!.future;
    }
    return _reply(receipt!);
  }

  @override
  Future<GuestMutationTaskRead> read(GuestMutationReceipt expected) async {
    expect(expected.key, key);
    expect(expected.commandId, commandId);
    expect(expected.submission, receipt!.submission);
    if (loseNextReadBeforeConsume) {
      loseNextReadBeforeConsume = false;
      throw StateError('read failed before consumption');
    }
    delivery = GuestMutationDelivery.consumed;
    if (kind == GuestMutationCommandKind.prepare && !denyPrepare) {
      prepared = true;
    }
    if (kind == GuestMutationCommandKind.execute ||
        kind == GuestMutationCommandKind.release ||
        kind == GuestMutationCommandKind.hostRelease) {
      terminal = true;
    }
    if (kind == GuestMutationCommandKind.release ||
        kind == GuestMutationCommandKind.hostRelease) {
      selected = false;
      storage = IoStoragePhase.stopping;
    }
    if (loseNextReadAfterConsume) {
      loseNextReadAfterConsume = false;
      throw StateError('lost read reply');
    }
    return GuestMutationTaskRead(reply: _reply(expected), result: pending);
  }

  @override
  Future<GuestMutationTaskReply> status(Uint8List taskKey) async {
    expect(taskKey, key);
    return GuestMutationTaskReply(
      io: ioState,
      state: state,
      commandId: BigInt.zero,
    );
  }

  @override
  Future<GuestMutationTaskReply> cancelCommand(Uint8List taskKey, BigInt id) =>
      throw UnimplementedError();

  @override
  Future<IoTaskSnapshot> ioStatus() async => ioState;
  @override
  Future<IoTaskSnapshot> cancelIo(Uint8List taskKey) async {
    expect(taskKey, key);
    storage = IoStoragePhase.stopping;
    return ioState;
  }

  @override
  Future<IoTaskSnapshot> repairIo(Uint8List taskKey) async {
    expect(taskKey, key);
    storage = IoStoragePhase.reclaimed;
    return ioState;
  }

  @override
  Future<IoTaskSnapshot> acknowledgeIo(Uint8List taskKey) async {
    expect(taskKey, key);
    storage = IoStoragePhase.local;
    exit = null;
    return ioState;
  }

  @override
  Future<IoTaskRead> readIo(Uint8List key) => throw UnimplementedError();
  @override
  Future<IoTaskSnapshot> pollIo(Uint8List key) => throw UnimplementedError();
  @override
  Future<IoTaskSnapshot> startHttp(HttpTaskRequest request) =>
      throw UnimplementedError();
}

void main() {
  test('review and prepare require distinct exact plan approvals', () async {
    final host = _Host();
    final session = GuestMutationExecutionSession.forBackend(host, host);
    expect(
      identical(session, GuestMutationExecutionSession.forBackend(host, host)),
      isTrue,
    );
    await session.refresh();
    final content = Uint8List.fromList([1, 2, 3]);
    await session.review(_request(), operationId: 'guest-op', content: content);
    content[0] = 99;
    expect(session.planSha256, _hash(host.plan));
    expect(session.canPrepare, isTrue);
    expect(host.sent, [
      GuestMutationCommandKind.select,
      GuestMutationCommandKind.buildPlan,
    ]);
    await expectLater(
      session.prepare(reviewedPlanSha256: _token(99)),
      throwsStateError,
    );
    expect(host.sent, hasLength(2));
    await session.prepare(reviewedPlanSha256: session.planSha256!);
    expect(host.chunks.single, [1, 2, 3]);
    expect(session.canExecute, isTrue);
    expect(host.sent, isNot(contains(GuestMutationCommandKind.execute)));
    await expectLater(
      session.execute(reviewedPlanSha256: _token(99)),
      throwsStateError,
    );
    await session.execute(reviewedPlanSha256: session.planSha256!);
    expect(session.outcome!.effect, GuestMutationFrameEffect.osSucceeded);
    expect(session.canExecute, isFalse);
    expect(
      host.sent.where((k) => k == GuestMutationCommandKind.execute),
      hasLength(1),
    );
  });

  test(
    'lost Execute submit retries only exact token and explicit read',
    () async {
      final host = _Host();
      final session = GuestMutationExecutionSession(host, host);
      await session.refresh();
      await session.review(_request(), operationId: 'guest-op');
      await session.prepare(reviewedPlanSha256: session.planSha256!);
      host.loseNextSubmit = true;
      await session.execute(reviewedPlanSha256: session.planSha256!);
      expect(session.uncertain, isTrue);
      expect(session.canExecute, isFalse);
      expect(session.canRetrySubmission, isTrue);
      final original = host.tokens.last;
      await session.retrySubmission();
      expect(host.tokens.last, original);
      expect(
        host.sent.where((k) => k == GuestMutationCommandKind.execute),
        hasLength(1),
      );
      expect(session.canRead, isTrue);
      await session.read();
      expect(session.outcome!.phase, GuestMutationFramePhase.observed);
    },
  );

  test('matching Prepared Query preserves existing execution review', () async {
    final host = _Host();
    final session = GuestMutationExecutionSession(host, host);
    await session.refresh();
    await session.review(
      _request(),
      operationId: 'guest-op',
      content: Uint8List.fromList([1, 2]),
    );
    await session.prepare(reviewedPlanSha256: session.planSha256!);
    expect(session.canExecute, isTrue);
    await session.query();
    expect(session.history!.frame!.phase, GuestMutationFramePhase.prepared);
    expect(session.canExecute, isTrue);
    expect(host.sent, isNot(contains(GuestMutationCommandKind.execute)));
  });

  test('Absent and Unknown Query never preserve execution review', () async {
    for (final phase in [
      GuestMutationFramePhase.absent,
      GuestMutationFramePhase.outcomeUnknown,
    ]) {
      final host = _Host();
      final session = GuestMutationExecutionSession(host, host);
      await session.refresh();
      await session.review(_request(), operationId: 'guest-op');
      await session.prepare(reviewedPlanSha256: session.planSha256!);
      expect(session.canExecute, isTrue);
      host.queryPhase = phase;
      if (phase == GuestMutationFramePhase.outcomeUnknown) {
        host.queryStatus = GuestMutationFrameStatus.outcomeUnknown;
      }
      await session.query();
      expect(session.canExecute, isFalse, reason: '$phase');
      expect(host.sent, isNot(contains(GuestMutationCommandKind.execute)));
    }
  });

  test(
    'denied Prepare does not yield a guest lease; cleanup uses owner',
    () async {
      final host = _Host()..denyPrepare = true;
      final session = GuestMutationExecutionSession(host, host);
      await session.refresh();
      await session.review(_request(), operationId: 'guest-op');
      await session.prepare(reviewedPlanSha256: session.planSha256!);
      expect(session.canExecute, isFalse);
      expect(session.canRelease, isTrue);
      await session.release();
      expect(host.sent.last, GuestMutationCommandKind.hostRelease);
    },
  );

  test(
    'OutcomeUnknown Execute remains uncertain and never re-executes',
    () async {
      final host = _Host()..executeUnknown = true;
      final session = GuestMutationExecutionSession(host, host);
      await session.refresh();
      await session.review(_request(), operationId: 'guest-op');
      await session.prepare(reviewedPlanSha256: session.planSha256!);
      await session.execute(reviewedPlanSha256: session.planSha256!);
      expect(session.outcome!.phase, GuestMutationFramePhase.outcomeUnknown);
      expect(session.uncertain, isTrue);
      expect(session.canExecute, isFalse);
      expect(session.canHostQuery, isTrue);
      await session.hostQuery();
      expect(session.history!.owner!.kind, MutationResultKind.history);
      expect(session.canExecute, isFalse);
      expect(
        host.sent.where((k) => k == GuestMutationCommandKind.execute),
        hasLength(1),
      );
    },
  );

  test(
    'a ready lost read is explicitly recovered without a new command',
    () async {
      final host = _Host();
      final session = GuestMutationExecutionSession(host, host);
      await session.refresh();
      await session.review(_request(), operationId: 'guest-op');
      await session.prepare(reviewedPlanSha256: session.planSha256!);
      host.loseNextReadBeforeConsume = true;
      await session.execute(reviewedPlanSha256: session.planSha256!);
      expect(session.canRead, isFalse);
      await session.refresh();
      expect(session.canRead, isTrue);
      await session.read();
      expect(session.outcome!.phase, GuestMutationFramePhase.observed);
      expect(
        host.sent.where((k) => k == GuestMutationCommandKind.execute),
        hasLength(1),
      );
    },
  );

  test('a forged Chunk end cannot advance preparation', () async {
    final host = _Host();
    final session = GuestMutationExecutionSession(host, host);
    await session.refresh();
    await session.review(
      _request(),
      operationId: 'guest-op',
      content: Uint8List.fromList([1, 2, 3]),
    );
    host.wrongChunkEnd = true;
    await session.prepare(reviewedPlanSha256: session.planSha256!);
    expect(session.canExecute, isFalse);
    expect(host.sent, isNot(contains(GuestMutationCommandKind.commitContent)));
    expect(session.canRetrySubmission, isTrue);
  });

  test(
    'consumed lost read cannot replay Execute; owner history stays separate',
    () async {
      final host = _Host();
      final session = GuestMutationExecutionSession(host, host);
      await session.refresh();
      await session.review(_request(), operationId: 'guest-op');
      await session.prepare(reviewedPlanSha256: session.planSha256!);
      host.loseNextReadAfterConsume = true;
      await session.execute(reviewedPlanSha256: session.planSha256!);
      await session.refresh();
      expect(session.uncertain, isTrue);
      expect(session.canRead, isFalse);
      expect(session.canExecute, isFalse);
      expect(session.canHostQuery, isTrue);
      await session.hostQuery();
      expect(session.history!.owner!.kind, MutationResultKind.history);
      expect(session.outcome, isNull);
      expect(
        host.sent.where((k) => k == GuestMutationCommandKind.execute),
        hasLength(1),
      );
    },
  );

  test('stop interrupts stalled chunk and ACK waits for actual exit', () async {
    final host = _Host();
    final session = GuestMutationExecutionSession(host, host);
    await session.refresh();
    await session.review(
      _request(),
      operationId: 'guest-op',
      content: Uint8List.fromList([4, 5, 6]),
    );
    host.stallChunk = Completer<GuestMutationTaskReply>();
    final preparing = session.prepare(reviewedPlanSha256: session.planSha256!);
    for (
      var i = 0;
      i < 20 && !host.sent.contains(GuestMutationCommandKind.chunk);
      i++
    ) {
      await Future<void>.delayed(const Duration(milliseconds: 1));
    }
    expect(host.sent, contains(GuestMutationCommandKind.chunk));
    expect(session.canStop, isTrue);
    await session.stop();
    await preparing;
    expect(host.sent, isNot(contains(GuestMutationCommandKind.commitContent)));
    expect(session.canAcknowledge, isFalse);
    host.exit = const IoTaskExit(
      execution: IoJobError.none,
      disconnect: IoJobError.none,
      maintenance: IoJobError.none,
    );
    host.storage = IoStoragePhase.reclaimed;
    await session.refresh();
    expect(session.canAcknowledge, isTrue);
    await session.acknowledge();
    expect(session.canStart, isTrue);
  });

  test(
    'approval expiry before Prepare blocks commands and cleanup permits a new review',
    () async {
      final host = _Host();
      var now = Duration.zero;
      final session = GuestMutationExecutionSession(
        host,
        host,
        monotonicNow: () => now,
      );
      await session.refresh();
      await session.review(_request(), operationId: 'guest-op');
      expect(session.canPrepare, isTrue);
      final sentBeforeExpiry = List<GuestMutationCommandKind>.of(host.sent);

      now = const Duration(milliseconds: 1000);
      expect(session.authorizationExpired, isTrue);
      expect(session.canPrepare, isFalse);
      expect(session.canQuery, isFalse);
      expect(session.canHostQuery, isFalse);
      expect(session.canCancelPlan, isFalse);
      expect(session.canRelease, isFalse);
      await expectLater(
        session.prepare(reviewedPlanSha256: session.planSha256!),
        throwsStateError,
      );
      expect(host.sent, sentBeforeExpiry);
      await session.refresh();
      expect(session.authorizationExpired, isTrue);
      expect(host.sent, sentBeforeExpiry);

      expect(session.canStop, isTrue);
      await session.stop();
      host.exit = const IoTaskExit(
        execution: IoJobError.none,
        disconnect: IoJobError.none,
        maintenance: IoJobError.none,
      );
      host.storage = IoStoragePhase.recoveryRequired;
      await session.refresh();
      expect(session.canRepair, isTrue);
      await session.repair();
      expect(session.canAcknowledge, isTrue);
      await session.acknowledge();
      expect(session.authorizationExpired, isFalse);
      expect(session.canStart, isTrue);
      await session.review(
        _request(submission: 9),
        operationId: 'guest-op-new',
      );
      expect(session.authorizationExpired, isFalse);
      expect(session.canPrepare, isTrue);
    },
  );

  test('approval expiry before Execute cannot dispatch an effect', () async {
    final host = _Host();
    var now = Duration.zero;
    final session = GuestMutationExecutionSession(
      host,
      host,
      monotonicNow: () => now,
    );
    await session.refresh();
    await session.review(_request(), operationId: 'guest-op');
    await session.prepare(reviewedPlanSha256: session.planSha256!);
    expect(session.canExecute, isTrue);
    final sentBeforeExpiry = List<GuestMutationCommandKind>.of(host.sent);

    now = const Duration(milliseconds: 1000);
    expect(session.authorizationExpired, isTrue);
    expect(session.canExecute, isFalse);
    expect(session.canQuery, isFalse);
    expect(session.canHostQuery, isFalse);
    expect(session.canCancelPlan, isFalse);
    expect(session.canRelease, isFalse);
    await expectLater(
      session.execute(reviewedPlanSha256: session.planSha256!),
      throwsStateError,
    );
    await expectLater(session.query(), throwsStateError);
    await expectLater(session.hostQuery(), throwsStateError);
    await expectLater(session.cancelPlan(), throwsStateError);
    await expectLater(session.release(), throwsStateError);
    expect(host.sent, sentBeforeExpiry);
    expect(host.sent, isNot(contains(GuestMutationCommandKind.execute)));
  });

  test(
    'expiry while a Chunk reply is pending prevents further chunks and Commit',
    () async {
      final host = _Host()
        ..stallChunk = Completer<GuestMutationTaskReply>()
        ..chunkSubmitted = Completer<void>();
      var now = Duration.zero;
      final session = GuestMutationExecutionSession(
        host,
        host,
        monotonicNow: () => now,
      );
      await session.refresh();
      await session.review(
        _request(),
        operationId: 'guest-op',
        content: Uint8List(GuestMutationValidation.maxChunkBytes + 1),
      );
      final preparing = session.prepare(
        reviewedPlanSha256: session.planSha256!,
      );
      await host.chunkSubmitted!.future;
      expect(host.chunks, hasLength(1));
      expect(host.sent.last, GuestMutationCommandKind.chunk);

      now = const Duration(milliseconds: 1000);
      host.stallChunk!.complete(host._reply(host.receipt!));
      await preparing;
      expect(session.authorizationExpired, isTrue);
      expect(host.chunks, hasLength(1));
      expect(
        host.sent,
        isNot(contains(GuestMutationCommandKind.commitContent)),
      );
      expect(host.sent, isNot(contains(GuestMutationCommandKind.execute)));
      expect(session.canExecute, isFalse);
    },
  );

  test('refresh and exact retry before expiry do not renew approval', () async {
    final host = _Host();
    var now = Duration.zero;
    final session = GuestMutationExecutionSession(
      host,
      host,
      monotonicNow: () => now,
    );
    await session.refresh();
    await session.review(_request(), operationId: 'guest-op');
    await session.prepare(reviewedPlanSha256: session.planSha256!);
    host.loseNextSubmit = true;
    await session.execute(reviewedPlanSha256: session.planSha256!);
    expect(session.canRetrySubmission, isTrue);
    final originalToken = Uint8List.fromList(host.tokens.last);
    final issuedCount = host.sent.length;

    now = const Duration(milliseconds: 500);
    await session.refresh();
    expect(session.canRetrySubmission, isTrue);
    await session.retrySubmission();
    expect(session.authorizationExpired, isFalse);
    expect(host.tokens.last, originalToken);
    expect(host.sent, hasLength(issuedCount));
    expect(session.canRead, isTrue);
    await session.read();
    expect(session.outcome!.phase, GuestMutationFramePhase.observed);
    now = const Duration(milliseconds: 1000);
    expect(session.authorizationExpired, isTrue);
    expect(session.canQuery, isFalse);
    expect(
      host.sent.where((kind) => kind == GuestMutationCommandKind.execute),
      hasLength(1),
    );
  });

  test('expired Execute submission cannot retry its original token', () async {
    final host = _Host();
    var now = Duration.zero;
    final session = GuestMutationExecutionSession(
      host,
      host,
      monotonicNow: () => now,
    );
    await session.refresh();
    await session.review(_request(), operationId: 'guest-op');
    await session.prepare(reviewedPlanSha256: session.planSha256!);
    host.loseNextSubmit = true;
    await session.execute(reviewedPlanSha256: session.planSha256!);
    expect(session.canRetrySubmission, isTrue);
    final originalToken = Uint8List.fromList(host.tokens.last);
    final issuedCount = host.sent.length;

    now = const Duration(milliseconds: 1000);
    await session.refresh();
    expect(session.authorizationExpired, isTrue);
    expect(session.canRetrySubmission, isFalse);
    await expectLater(session.retrySubmission(), throwsStateError);
    expect(host.tokens.last, originalToken);
    expect(host.sent, hasLength(issuedCount));
    expect(
      host.sent.where((kind) => kind == GuestMutationCommandKind.execute),
      hasLength(1),
    );
  });

  test('expired Start submission cannot retry its original token', () async {
    final host = _Host()..loseNextStart = true;
    var now = Duration.zero;
    final session = GuestMutationExecutionSession(
      host,
      host,
      monotonicNow: () => now,
    );
    await session.refresh();
    await session.review(_request(), operationId: 'guest-op');
    expect(session.canRetrySubmission, isTrue);
    expect(host.sent, [GuestMutationCommandKind.select]);

    now = const Duration(milliseconds: 1000);
    await session.refresh();
    expect(session.authorizationExpired, isTrue);
    expect(session.canRetrySubmission, isFalse);
    await expectLater(session.retrySubmission(), throwsStateError);
    expect(host.sent, [GuestMutationCommandKind.select]);
  });

  test('an issued Execute receipt remains readable after expiry', () async {
    final host = _Host();
    var now = Duration.zero;
    final session = GuestMutationExecutionSession(
      host,
      host,
      monotonicNow: () => now,
    );
    await session.refresh();
    await session.review(_request(), operationId: 'guest-op');
    await session.prepare(reviewedPlanSha256: session.planSha256!);
    host.loseNextReadBeforeConsume = true;
    await session.execute(reviewedPlanSha256: session.planSha256!);
    final issuedCount = host.sent.length;

    now = const Duration(milliseconds: 1000);
    await session.refresh();
    expect(session.authorizationExpired, isTrue);
    expect(session.canRetrySubmission, isFalse);
    expect(session.canRead, isTrue);
    await session.read();
    expect(session.outcome!.phase, GuestMutationFramePhase.observed);
    expect(host.sent, hasLength(issuedCount));
    expect(
      host.sent.where((kind) => kind == GuestMutationCommandKind.execute),
      hasLength(1),
    );
  });

  test('foreign active owner is never adopted', () async {
    final host = _Host();
    host.request = _request();
    host.storage = IoStoragePhase.running;
    final session = GuestMutationExecutionSession(host, host);
    await session.refresh();
    expect(session.canStart, isFalse);
    expect(session.canStop, isFalse);
    expect(session.canAcknowledge, isFalse);
  });
}
