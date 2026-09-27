import 'dart:io';
import 'dart:typed_data';
import 'package:capnproto_dart/capnproto_dart.dart';
import 'package:test/test.dart';
import '../../../lib/plugins/generated/host.capnp.dart' as host;
import '../../../lib/plugins/generated/identity.dart' as identity;
import '../../../lib/plugins/mutation_task_models.dart';
import '../../../lib/plugins/mutation_task_codec_native.dart';
import '../../../lib/plugins/mutation_task_native.dart';

Uint8List token(int value) => Uint8List.fromList(List.filled(32, value));
final huge = BigInt.parse('9007199254740993');
final emptyHash = Uint8List.fromList([
  0xe3,
  0xb0,
  0xc4,
  0x42,
  0x98,
  0xfc,
  0x1c,
  0x14,
  0x9a,
  0xfb,
  0xf4,
  0xc8,
  0x99,
  0x6f,
  0xb9,
  0x24,
  0x27,
  0xae,
  0x41,
  0xe4,
  0x64,
  0x9b,
  0x93,
  0x4c,
  0xa4,
  0x95,
  0x99,
  0x1b,
  0x78,
  0x52,
  0xb8,
  0x55,
]);

MutationStartRequest selected({BigInt? revision, int timeout = 1000}) =>
    MutationStartRequest(
      submission: token(1),
      packageId: 'org.example.mutation',
      packageDigest: token(2),
      registryRevision: revision ?? huge,
      disposition: MutationDisposition.create,
      selectedPath: r'C:\chosen',
      relativePath: 'new.bin',
      subject: 'plugin.mutation',
      approvalSha256: token(4),
      timeoutMs: timeout,
    );

MutationReconcileRequest reconciliation() => MutationReconcileRequest(
  submission: token(1),
  packageId: 'org.example.mutation',
  packageDigest: token(2),
  registryRevision: huge,
  plan: Uint8List.fromList([1, 2, 3]),
  timeoutMs: 1000,
);

MutationDiscoverRequest discovery({
  String subject = 'plugin.mutation',
  BigInt? revision,
  int scanLimit = 2,
  int timeoutMs = 1000,
  Uint8List? checkpoint,
}) => MutationDiscoverRequest(
  submission: token(1),
  packageId: 'org.example.mutation',
  packageDigest: token(2),
  registryRevision: revision ?? huge,
  subject: subject,
  disposition: MutationDisposition.delete,
  scanLimit: scanLimit,
  timeoutMs: timeoutMs,
  checkpoint: checkpoint,
);

class Peer {
  final actions = <host.Action>[];
  host.RequestReader? last;
  void Function(host.ResponseBuilder)? customize;
  bool fail = false;
  Future<T> exchange<T>(
    host.Action action, {
    void Function(host.RequestBuilder)? configure,
    required T Function(host.ResponseReader) decode,
    required bool clearReply,
  }) async {
    actions.add(action);
    final input = MessageBuilder().initRoot(host.requestFactory);
    input.action = action;
    configure?.call(input);
    last = input.asReader();
    if (fail) throw StateError('lost reply');
    final output = MessageBuilder();
    final row = output.initRoot(host.responseFactory);
    final io = row.initIoState();
    io.key = token(3);
    io.submission = token(1);
    io.storage = 1;
    io.delivery = 2;
    final state = row.initMutationState();
    state.commandBigInt = BigInt.one;
    state.kind = action == host.Action.mutationReconcile
        ? 8
        : action == host.Action.mutationDiscover
        ? 10
        : 0;
    state.delivery = 0;
    if (action == host.Action.mutationStart ||
        action == host.Action.mutationReconcile ||
        action == host.Action.mutationDiscover) {
      row.mutationCommandIdBigInt = BigInt.one;
    } else if (action == host.Action.mutationSubmit) {
      row.mutationCommandIdBigInt = BigInt.from(2);
      state.commandBigInt = BigInt.from(2);
      state.kind = last!.mutationCommand!.kind;
    } else if (action == host.Action.mutationRead ||
        action == host.Action.mutationCancelCommand) {
      row.mutationCommandIdBigInt = last!.mutationCommandIdBigInt;
      state.commandBigInt = last!.mutationCommandIdBigInt;
      row.initMutationResult().kind = 0;
    }
    customize?.call(row);
    final frame = output.serialize();
    try {
      return decode(
        MessageReader.deserialize(frame).getRoot(host.responseFactory),
      );
    } finally {
      if (clearReply) frame.fillRange(0, frame.length, 0);
    }
  }
}

void main() {
  test(
    'selected and history starts retain exact revision, correlate acknowledgement',
    () async {
      final peer = Peer();
      final client = NativeMutationTaskClient(peer.exchange);
      final source = token(1), digest = token(2);
      final start = MutationStartRequest(
        submission: source,
        packageId: 'org.example.mutation',
        packageDigest: digest,
        registryRevision: huge,
        disposition: MutationDisposition.create,
        selectedPath: r'C:\chosen',
        relativePath: 'new.bin',
        subject: 'plugin.mutation',
        approvalSha256: token(4),
        timeoutMs: 1000,
      );
      source[0] = 9;
      digest[0] = 9;
      final reply = await client.startSelected(start);
      expect(reply.commandId, BigInt.one);
      expect(reply.state.kind, MutationCommandKind.select);
      expect(peer.last!.mutationStart!.registryRevisionBigInt, huge);
      expect(peer.last!.mutationStart!.submission, token(1));
      expect(() => start.submission[0] = 7, throwsUnsupportedError);
      peer.customize = (row) {
        final state = row.initMutationState();
        state.commandBigInt = BigInt.from(3);
        state.kind = 2;
      };
      expect(
        (await client.startSelected(start)).state.commandId,
        BigInt.from(3),
      );
      peer.customize = null;
      await client.startReconciliation(reconciliation());
      expect(peer.actions.last, host.Action.mutationReconcile);
      expect(peer.last!.mutationReconcile!.plan, [1, 2, 3]);
      peer.customize = (row) => row.initIoState().submission = token(9);
      await expectLater(
        client.startSelected(selected()),
        throwsFormatException,
      );
    },
  );

  test(
    'discovery start and next page retain scope and allow deduplicated start',
    () async {
      final peer = Peer(), client = NativeMutationTaskClient(peer.exchange);
      final submission = token(1), digest = token(2), checkpoint = token(7);
      final request = MutationDiscoverRequest(
        submission: submission,
        packageId: 'org.example.mutation',
        packageDigest: digest,
        registryRevision: huge,
        subject: 'plugin.mutation',
        disposition: MutationDisposition.delete,
        scanLimit: 2,
        timeoutMs: 1000,
        checkpoint: checkpoint,
      );
      submission[0] = 9;
      digest[0] = 9;
      checkpoint[0] = 9;
      final start = await client.startDiscovery(request);
      expect(start.commandId, BigInt.one);
      expect(start.state.kind, MutationCommandKind.discover);
      expect(peer.actions, [host.Action.mutationDiscover]);
      expect(peer.last!.mutationDiscover!.submission, token(1));
      expect(peer.last!.mutationDiscover!.packageDigest, token(2));
      expect(peer.last!.mutationDiscover!.registryRevisionBigInt, huge);
      expect(peer.last!.mutationDiscover!.subject, 'plugin.mutation');
      expect(peer.last!.mutationDiscover!.disposition, 3);
      expect(peer.last!.mutationDiscover!.scanLimit, 2);
      expect(peer.last!.mutationDiscover!.checkpoint, token(7));
      expect(() => request.packageDigest[0] = 8, throwsUnsupportedError);
      expect(() => request.checkpoint![0] = 8, throwsUnsupportedError);
      peer.customize = (row) {
        row.initMutationState()
          ..commandBigInt = BigInt.from(3)
          ..kind = MutationCommandKind.discover.index;
      };
      expect((await client.startDiscovery(request)).commandId, BigInt.one);
      peer.customize = null;
      await client.submitNextPlans(token(3), token(5), 8);
      expect(peer.actions.last, host.Action.mutationSubmit);
      expect(
        peer.last!.mutationCommand!.kind,
        MutationCommandKind.discover.index,
      );
      expect(peer.last!.mutationCommand!.scanLimit, 8);
      peer.fail = true;
      await expectLater(
        client.submitNextPlans(token(3), token(6), 1),
        throwsStateError,
      );
      expect(peer.actions, [
        host.Action.mutationDiscover,
        host.Action.mutationDiscover,
        host.Action.mutationSubmit,
        host.Action.mutationSubmit,
      ]);
    },
  );

  test('invalid discovery scope and scan limits never exchange', () {
    final peer = Peer(), client = NativeMutationTaskClient(peer.exchange);
    for (final request in [
      discovery(subject: ''),
      discovery(subject: 'bad/path'),
      discovery(subject: r'bad\path'),
      discovery(subject: 'bad:path'),
      discovery(subject: 'bad\u0000subject'),
      discovery(subject: '字' * 86),
      discovery(scanLimit: 0),
      discovery(scanLimit: 9),
      discovery(timeoutMs: 0),
      discovery(revision: BigInt.one << 64),
    ]) {
      expect(() => client.startDiscovery(request), throwsFormatException);
    }
    for (final checkpoint in [
      Uint8List(0),
      token(0),
      Uint8List(31),
      Uint8List(33),
    ]) {
      expect(
        () => discovery(checkpoint: checkpoint),
        throwsFormatException,
      );
    }
    for (final limit in [0, 9]) {
      expect(
        () => client.submitNextPlans(token(3), token(5), limit),
        throwsFormatException,
      );
    }
    expect(peer.actions, isEmpty);
    final command = MessageBuilder().initRoot(host.mutationCommandFactory);
    expect(
      () => MutationTaskCodec.writeCommand(
        submission: token(5),
        kind: MutationCommandKind.discover,
        scanLimit: 2,
        plan: Uint8List(0),
        out: command,
      ),
      throwsFormatException,
    );
    expect(
      () => MutationTaskCodec.writeCommand(
        submission: token(5),
        kind: MutationCommandKind.release,
        scanLimit: 2,
        out: command,
      ),
      throwsFormatException,
    );
  });

  test(
    'all submitted commands bind identity and payload without replay',
    () async {
      final peer = Peer();
      final client = NativeMutationTaskClient(peer.exchange);
      await client.submitPrepare(token(3), token(5), Uint8List.fromList([1]));
      expect(peer.last!.mutationCommand!.kind, 1);
      expect(peer.last!.mutationCommand!.plan, [1]);
      await client.submitChunk(
        token(3),
        token(6),
        huge,
        Uint8List.fromList([8, 9]),
      );
      expect(peer.last!.mutationCommand!.kind, 2);
      expect(peer.last!.mutationCommand!.offsetBigInt, huge);
      for (final call in [
        () => client.submitCommitContent(token(3), token(7)),
        () => client.submitExecute(token(3), token(8)),
        () => client.submitQuery(token(3), token(9)),
        () => client.submitCancelPlan(token(3), token(10)),
        () => client.submitRelease(token(3), token(11)),
      ]) {
        await call();
      }
      expect(peer.actions, List.filled(7, host.Action.mutationSubmit));
      expect(peer.last!.mutationCommand!.kind, 7);
      peer.customize = (row) {
        row.mutationCommandIdBigInt = BigInt.from(2);
        final state = row.initMutationState();
        state.commandBigInt = huge;
        state.kind = 5;
      };
      expect(
        (await client.submitPrepare(
          token(3),
          token(5),
          Uint8List.fromList([1]),
        )).commandId,
        BigInt.from(2),
      );
      peer.customize = (row) => row.initMutationState().kind = 5;
      await expectLater(
        client.submitExecute(token(3), token(8)),
        throwsFormatException,
      );
    },
  );

  test('build plan submits bounded create and delete specifications', () async {
    final peer = Peer(), client = NativeMutationTaskClient(peer.exchange);
    final digest = token(6);
    await client.submitBuildPlan(
      token(3),
      token(5),
      operationId: 'create-1',
      contentLength: BigInt.from(1234),
      contentSha256: digest,
    );
    digest[0] = 9;
    expect(
      peer.last!.mutationCommand!.kind,
      MutationCommandKind.buildPlan.index,
    );
    expect(peer.last!.mutationCommand!.operationId, 'create-1');
    expect(peer.last!.mutationCommand!.contentLengthBigInt, BigInt.from(1234));
    expect(peer.last!.mutationCommand!.contentSha256, token(6));
    await client.submitBuildPlan(
      token(3),
      token(7),
      operationId: 'delete-1',
      contentLength: BigInt.zero,
    );
    expect(peer.last!.mutationCommand!.contentLengthBigInt, BigInt.zero);
    expect(peer.last!.mutationCommand!.contentSha256 ?? Uint8List(0), isEmpty);
    await client.submitBuildPlan(
      token(3),
      token(8),
      operationId: 'create-empty',
      contentLength: BigInt.zero,
      contentSha256: emptyHash,
    );
    expect(peer.last!.mutationCommand!.contentSha256, emptyHash);
    expect(peer.actions, List.filled(3, host.Action.mutationSubmit));
    peer.fail = true;
    await expectLater(
      client.submitBuildPlan(
        token(3),
        token(9),
        operationId: 'lost-reply',
        contentLength: BigInt.zero,
      ),
      throwsStateError,
    );
    expect(peer.actions, List.filled(4, host.Action.mutationSubmit));
  });

  test('invalid build plan input is rejected before exchange', () {
    final peer = Peer(), client = NativeMutationTaskClient(peer.exchange);
    Future<MutationTaskReply> call(
      String operationId,
      BigInt length,
      Uint8List? digest,
    ) => client.submitBuildPlan(
      token(3),
      token(5),
      operationId: operationId,
      contentLength: length,
      contentSha256: digest,
    );
    for (final (id, length, digest) in [
      ('', BigInt.zero, null),
      ('a' * 257, BigInt.zero, null),
      ('bad\u0000id', BigInt.zero, null),
      ('bad/id', BigInt.zero, null),
      (r'bad\id', BigInt.zero, null),
      ('bad:id', BigInt.zero, null),
      ('bad\u0085id', BigInt.zero, null),
      ('字' * 86, BigInt.zero, null),
      ('ok', BigInt.from(-1), null),
      ('ok', BigInt.from(MutationTaskValidation.maxContentBytes + 1), token(6)),
      ('ok', BigInt.one, null),
      ('ok', BigInt.one, Uint8List(31)),
      ('ok', BigInt.one, Uint8List(32)),
      ('ok', BigInt.zero, token(6)),
      ('ok', BigInt.zero, Uint8List(0)),
    ]) {
      expect(() => call(id, length, digest), throwsFormatException);
    }
    expect(peer.actions, isEmpty);
    final row = MessageBuilder().initRoot(host.mutationCommandFactory);
    expect(
      () => MutationTaskCodec.writeCommand(
        submission: token(5),
        kind: MutationCommandKind.buildPlan,
        operationId: 'op',
        contentLength: BigInt.zero,
        plan: Uint8List(0),
        out: row,
      ),
      throwsFormatException,
    );
    expect(
      () => MutationTaskCodec.writeCommand(
        submission: token(5),
        kind: MutationCommandKind.prepare,
        plan: Uint8List.fromList([1]),
        contentLength: BigInt.zero,
        out: row,
      ),
      throwsFormatException,
    );
  });

  test(
    'planned result is copied, strict, and bound to build plan command',
    () async {
      final peer = Peer(), client = NativeMutationTaskClient(peer.exchange);
      peer.customize = (row) {
        final state = row.initMutationState();
        state.commandBigInt = BigInt.from(3);
        state.kind = MutationCommandKind.buildPlan.index;
        state.delivery = 2;
        row.mutationCommandIdBigInt = BigInt.from(3);
        final result = row.initMutationResult();
        result.kind = MutationResultKind.planned.index;
        result.plan = Uint8List.fromList([1, 2, 3]);
      };
      final read = await client.read(token(3), BigInt.from(3));
      expect(read.reply.state.kind, MutationCommandKind.buildPlan);
      expect(read.result!.kind, MutationResultKind.planned);
      expect(read.result!.plan, [1, 2, 3]);
      expect(read.result!.record, isNull);
      expect(read.result!.phase, MutationPhase.none);
      expect(() => read.result!.plan![0] = 9, throwsUnsupportedError);

      host.MutationResultReader build(
        void Function(host.MutationResultBuilder) fill,
      ) {
        final row = MessageBuilder().initRoot(host.mutationResultFactory);
        row.kind = MutationResultKind.planned.index;
        row.plan = Uint8List.fromList([1]);
        fill(row);
        return row.asReader();
      }

      for (final fill in <void Function(host.MutationResultBuilder)>[
        (r) => r.plan = Uint8List(0),
        (r) => r.plan = Uint8List(MutationTaskValidation.maxPlanBytes + 1),
        (r) => r.record = Uint8List.fromList([1]),
        (r) => r.outcome = Uint8List.fromList([1]),
        (r) => r.phase = 1,
        (r) => r.effect = 1,
        (r) => r.stagedBytesBigInt = BigInt.one,
        (r) => r.durableContent = true,
        (r) => r.operationId = 'op',
        (r) => r.reference = token(7),
        (r) => r.expectedIdentity = token(8),
      ]) {
        expect(
          () => MutationTaskCodec.result(build(fill)),
          throwsFormatException,
        );
      }
      peer.customize = (row) {
        row.initMutationState()
          ..commandBigInt = BigInt.from(3)
          ..kind = MutationCommandKind.prepare.index
          ..delivery = 2;
        row.mutationCommandIdBigInt = BigInt.from(3);
        row.initMutationResult()
          ..kind = MutationResultKind.planned.index
          ..plan = Uint8List.fromList([1]);
      };
      await expectLater(
        client.read(token(3), BigInt.from(3)),
        throwsFormatException,
      );
    },
  );

  test('discovery pages are bounded, copied, and bound to state', () async {
    final peer = Peer(), client = NativeMutationTaskClient(peer.exchange);
    peer.customize = (row) {
      row.initMutationState()
        ..commandBigInt = BigInt.from(3)
        ..kind = MutationCommandKind.discover.index
        ..delivery = 2;
      row.mutationCommandIdBigInt = BigInt.from(3);
      final result = row.initMutationResult();
      result.kind = MutationResultKind.plans.index;
      result.scanned = 3;
      result.checkpoint = token(8);
      final plans = result.initPlans(2);
      plans[0] = Uint8List.fromList([1, 2]);
      plans[1] = Uint8List.fromList([3]);
    };
    final read = await client.read(token(3), BigInt.from(3));
    expect(read.reply.state.kind, MutationCommandKind.discover);
    expect(read.result!.plans, [
      [1, 2],
      [3],
    ]);
    expect(read.result!.scanned, 3);
    expect(read.result!.done, isFalse);
    expect(read.result!.checkpoint, token(8));
    expect(() => read.result!.checkpoint![0] = 9, throwsUnsupportedError);
    expect(read.result!.phase, MutationPhase.none);
    expect(read.result!.effect, MutationEffect.unspecified);
    expect(() => read.result!.plans![0][0] = 9, throwsUnsupportedError);
    expect(() => read.result!.plans![0] = token(7), throwsUnsupportedError);

    peer.customize = (row) {
      row.initMutationState()
        ..commandBigInt = BigInt.from(4)
        ..kind = MutationCommandKind.discover.index
        ..delivery = 2;
      row.mutationCommandIdBigInt = BigInt.from(4);
      row.initMutationResult()
        ..kind = MutationResultKind.plans.index
        ..scanned = 1
        ..checkpoint = token(9);
    };
    final emptyPartial = await client.read(token(3), BigInt.from(4));
    expect(emptyPartial.result!.plans, isEmpty);
    expect(emptyPartial.result!.done, isFalse);
    expect(emptyPartial.result!.checkpoint, token(9));

    peer.customize = (row) {
      row.initMutationState()
        ..commandBigInt = BigInt.from(5)
        ..kind = MutationCommandKind.discover.index
        ..delivery = 2
        ..terminal = true;
      row.mutationCommandIdBigInt = BigInt.from(5);
      row.initMutationResult()
        ..kind = MutationResultKind.plans.index
        ..done = true;
    };
    final complete = await client.read(token(3), BigInt.from(5));
    expect(complete.result!.plans, isEmpty);
    expect(complete.result!.scanned, 0);
    expect(complete.result!.done, isTrue);
    expect(complete.result!.checkpoint, isNull);

    peer.customize = (row) {
      row.initMutationState()
        ..commandBigInt = BigInt.from(6)
        ..kind = MutationCommandKind.discover.index
        ..delivery = 2
        ..terminal = true;
      row.mutationCommandIdBigInt = BigInt.from(6);
      row.initMutationResult()
        ..kind = MutationResultKind.plans.index
        ..scanned = 1;
    };
    await expectLater(
      client.read(token(3), BigInt.from(6)),
      throwsFormatException,
    );
  });

  test('discovery result and state reject mixed fields', () {
    host.MutationResultReader build(
      void Function(host.MutationResultBuilder) fill,
    ) {
      final row = MessageBuilder().initRoot(host.mutationResultFactory);
      row.kind = MutationResultKind.plans.index;
      row.scanned = 1;
      row.checkpoint = token(8);
      fill(row);
      return row.asReader();
    }

    for (final fill in <void Function(host.MutationResultBuilder)>[
      (r) => r.scanned = 0,
      (r) => r.scanned = 9,
      (r) => r.initPlans(2),
      (r) => r.initPlans(9),
      (r) => r.initPlans(1)[0] = Uint8List(0),
      (r) => r.initPlans(1)[0] = Uint8List(
        MutationTaskValidation.maxPlanBytes + 1,
      ),
      (r) => r.plan = Uint8List.fromList([1]),
      (r) => r.record = Uint8List.fromList([1]),
      (r) => r.outcome = Uint8List.fromList([1]),
      (r) => r.reference = token(7),
      (r) => r.phase = 1,
      (r) => r.effect = 1,
      (r) => r.stagedBytesBigInt = BigInt.one,
      (r) => r.durableContent = true,
      (r) => r.operationId = 'op',
      (r) => r.checkpoint = Uint8List(31),
      (r) => r.checkpoint = token(0),
      (r) => r.checkpoint = Uint8List(0),
      (r) => r.done = true,
    ]) {
      expect(
        () => MutationTaskCodec.result(build(fill)),
        throwsFormatException,
      );
    }
    for (final fill in <void Function(host.MutationResultBuilder)>[
      (r) => r.initPlans(0),
      (r) => r.scanned = 1,
      (r) => r.done = true,
      (r) => r.checkpoint = token(8),
    ]) {
      final row = MessageBuilder().initRoot(host.mutationResultFactory);
      row.kind = MutationResultKind.pending.index;
      fill(row);
      expect(
        () => MutationTaskCodec.result(row.asReader()),
        throwsFormatException,
      );
    }
    for (final fill in <void Function(host.MutationStateBuilder)>[
      (s) => s.reconcileRequired = true,
      (s) {
        s.selected = true;
        s.reference = token(7);
      },
    ]) {
      final row = MessageBuilder().initRoot(host.mutationStateFactory);
      row.commandBigInt = BigInt.one;
      row.kind = MutationCommandKind.discover.index;
      fill(row);
      expect(
        () => MutationTaskCodec.state(row.asReader()),
        throwsFormatException,
      );
    }
  });

  test(
    'bounds reject before exchange and lost replies make one call',
    () async {
      final peer = Peer(), client = NativeMutationTaskClient(peer.exchange);
      expect(
        () => client.startSelected(selected(timeout: 0)),
        throwsFormatException,
      );
      expect(
        () => client.startSelected(selected(revision: BigInt.one << 64)),
        throwsFormatException,
      );
      expect(
        () => client.submitPrepare(
          token(3),
          token(5),
          Uint8List(MutationTaskValidation.maxPlanBytes + 1),
        ),
        throwsFormatException,
      );
      expect(
        () => client.submitChunk(
          token(3),
          token(5),
          huge,
          Uint8List(MutationTaskValidation.maxChunkBytes + 1),
        ),
        throwsFormatException,
      );
      expect(() => client.read(token(3), BigInt.zero), throwsFormatException);
      expect(peer.actions, isEmpty);
      peer.fail = true;
      await expectLater(client.startSelected(selected()), throwsStateError);
      await expectLater(client.read(token(3), BigInt.one), throwsStateError);
      expect(peer.actions, [
        host.Action.mutationStart,
        host.Action.mutationRead,
      ]);
    },
  );

  test('read is command-bound and detached before reply wipe', () async {
    final peer = Peer(), client = NativeMutationTaskClient(peer.exchange);
    peer.customize = (row) {
      final state = row.initMutationState();
      state.commandBigInt = huge;
      state.kind = 5;
      state.delivery = 2;
      row.mutationCommandIdBigInt = huge;
      final result = row.initMutationResult();
      result.kind = 6;
      result.record = Uint8List.fromList([7, 8, 9]);
      result.phase = 2;
      result.operationId = 'operation';
      result.stagedBytesBigInt = huge;
    };
    final read = await client.read(token(3), huge);
    expect(read.reply.commandId, huge);
    expect(read.result!.phase, MutationPhase.outcomeUnknown);
    expect(read.result!.effect, MutationEffect.unspecified);
    expect(read.result!.stagedBytes, huge);
    expect(read.result!.record, [7, 8, 9]);
    expect(() => read.result!.record![0] = 1, throwsUnsupportedError);
    peer.customize = (row) => row.mutationCommandIdBigInt = BigInt.from(2);
    await expectLater(client.read(token(3), huge), throwsFormatException);
  });

  test('effect, failure and reconciled projections remain distinct', () {
    host.MutationResultReader build(
      void Function(host.MutationResultBuilder) fill,
    ) {
      final row = MessageBuilder().initRoot(host.mutationResultFactory);
      fill(row);
      return row.asReader();
    }

    final observed = MutationTaskCodec.result(
      build((r) {
        r.kind = 6;
        r.record = Uint8List.fromList([1]);
        r.phase = 3;
        r.operationId = 'op';
      }),
    )!;
    expect(observed.phase, MutationPhase.observed);
    expect(observed.effect, MutationEffect.unspecified);
    final rejected = MutationTaskCodec.result(
      build((r) {
        r.kind = 5;
        r.outcome = Uint8List.fromList([1]);
        r.phase = 3;
        r.operationId = 'op';
        r.effect = 2;
        r.osCode = 5;
      }),
    )!;
    expect(rejected.effect, MutationEffect.osRejected);
    expect(rejected.osCode, 5);
    final reconciled = MutationTaskCodec.result(
      build((r) {
        r.kind = 10;
        r.record = Uint8List.fromList([1]);
        r.phase = 3;
        r.operationId = 'op';
        r.outcome = Uint8List.fromList([2]);
        r.effect = 1;
      }),
    )!;
    expect(reconciled.kind, MutationResultKind.reconciled);
    expect(reconciled.effect, MutationEffect.osSucceeded);
    for (final phase in [2, 3]) {
      expect(
        () => MutationTaskCodec.result(
          build((r) {
            r.kind = 10;
            r.record = Uint8List.fromList([1]);
            r.phase = phase;
            r.operationId = 'op';
            if (phase == 2) {
              r.outcome = Uint8List.fromList([2]);
              r.effect = 1;
            }
          }),
        ),
        throwsFormatException,
      );
    }
    expect(
      () => MutationTaskCodec.result(
        build((r) {
          r.kind = 5;
          r.outcome = Uint8List.fromList([1]);
          r.phase = 3;
          r.operationId = 'op';
          r.effect = 2;
          r.osCode = 0;
        }),
      ),
      throwsFormatException,
    );
    final failure = MutationTaskCodec.result(
      build((r) {
        r.kind = 9;
        r.failureLayer = 2;
        r.failureCode = 18;
      }),
    )!;
    expect(failure.failure!.target, MutationTargetFailure.revisionConflict);
    expect(
      () => MutationTaskCodec.result(
        build((r) {
          r.kind = 6;
          r.record = Uint8List.fromList([1]);
          r.phase = 3;
          r.operationId = 'op';
          r.effect = 1;
        }),
      ),
      throwsFormatException,
    );
    expect(
      () => MutationTaskCodec.result(
        build((r) {
          r.kind = 5;
          r.outcome = Uint8List.fromList([1]);
          r.phase = 3;
          r.operationId = 'op';
          r.effect = 0;
        }),
      ),
      throwsFormatException,
    );
  });

  test('terminal plan and last-command uncertainty are independent', () {
    final row = MessageBuilder().initRoot(host.mutationStateFactory);
    row.commandBigInt = BigInt.from(4);
    row.kind = 5;
    row.terminal = true;
    row.reconcileRequired = true;
    expect(MutationTaskCodec.state(row.asReader()).terminal, isTrue);
    expect(MutationTaskCodec.state(row.asReader()).reconcileRequired, isTrue);
  });

  test('selected read checks both target identities', () async {
    final peer = Peer();
    final client = NativeMutationTaskClient(peer.exchange);
    peer.customize = (row) {
      final state = row.initMutationState();
      state.commandBigInt = BigInt.one;
      state.kind = 0;
      state.delivery = 2;
      state.selected = true;
      state.reference = token(7);
      state.expectedIdentity = token(8);
      final result = row.initMutationResult();
      result.kind = 1;
      result.reference = token(7);
      result.expectedIdentity = token(9);
    };
    await expectLater(client.read(token(3), BigInt.one), throwsFormatException);
    peer.customize = (row) {
      final state = row.initMutationState();
      state.commandBigInt = BigInt.one;
      state.kind = 0;
      state.delivery = 2;
      state.selected = true;
      state.reference = token(7);
      state.expectedIdentity = token(8);
      final result = row.initMutationResult();
      result.kind = 1;
      result.reference = token(7);
      result.expectedIdentity = token(8);
    };
    final read = await client.read(token(3), BigInt.one);
    expect(read.result!.expectedIdentity, token(8));
  });

  final fixtures = Platform.environment['MORROW_MUTATION_WIRE_FIXTURES'];
  test(
    'Rust private frames decode selection, effect and history',
    () {
      for (final (name, expected) in [
        ('selected', MutationResultKind.selected),
        ('prepared', MutationResultKind.prepared),
        ('created', MutationResultKind.created),
        ('query', MutationResultKind.history),
        ('reconciled', MutationResultKind.reconciled),
        ('planned', MutationResultKind.planned),
        ('plans', MutationResultKind.plans),
      ]) {
        final bytes = File(
          '$fixtures/rust-mutation-$name.bin',
        ).readAsBytesSync();
        final row = MessageReader.deserialize(
          bytes,
        ).getRoot(host.responseFactory);
        expect(row.digest, identity.hostDigest);
        expect(row.version, 1);
        expect(row.error ?? '', isEmpty);
        final result = MutationTaskCodec.result(row.mutationResult)!;
        expect(result.kind, expected);
        if (name == 'query') expect(result.effect, MutationEffect.unspecified);
        if (name == 'created')
          expect(result.effect, MutationEffect.osSucceeded);
        if (name == 'reconciled') {
          expect(result.phase, MutationPhase.observed);
          expect(result.effect, MutationEffect.osSucceeded);
          expect(result.operationId, 'wire-reconciled-create');
        }
        if (name == 'planned') {
          expect(result.plan, isNotEmpty);
          expect(result.record, isNull);
          expect(result.outcome, isNull);
          expect(result.phase, MutationPhase.none);
          expect(result.effect, MutationEffect.unspecified);
        }
        if (name == 'plans') {
          expect(result.plans, isNotEmpty);
          expect(result.scanned, greaterThanOrEqualTo(result.plans!.length));
          if (result.done) {
            expect(result.checkpoint, isNull);
          } else {
            expect(result.checkpoint, hasLength(32));
          }
          expect(result.plan, isNull);
          expect(result.record, isNull);
          expect(result.outcome, isNull);
          expect(result.phase, MutationPhase.none);
          expect(result.effect, MutationEffect.unspecified);
        }
      }
    },
    skip: fixtures == null ? 'Set MORROW_MUTATION_WIRE_FIXTURES' : false,
  );
}
