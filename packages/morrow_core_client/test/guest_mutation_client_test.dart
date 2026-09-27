import 'dart:typed_data';

import 'package:capnproto_dart/capnproto_dart.dart';
import 'package:test/test.dart';

import '../../../lib/plugins/generated/host.capnp.dart' as host;
import '../../../lib/plugins/generated/identity.dart' as contract;
import '../../../lib/plugins/generated/mutation.capnp.dart' as core;
import '../../../lib/plugins/guest_mutation_models.dart';
import '../../../lib/plugins/guest_mutation_native.dart';
import '../../../lib/plugins/mutation_task_models.dart';

Uint8List token(int n) => Uint8List.fromList(List.filled(32, n));
final huge = BigInt.parse('9007199254740993');
final emptyHash = Uint8List.fromList(const [
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

GuestMutationStartRequest start({ApprovedGuestBudget? budget}) =>
    GuestMutationStartRequest(
      selection: MutationStartRequest(
        submission: token(1),
        packageId: 'org.example.guest',
        packageDigest: token(2),
        registryRevision: huge,
        disposition: MutationDisposition.create,
        selectedPath: r'C:\chosen',
        relativePath: 'new.bin',
        subject: 'guest.mutation',
        approvalSha256: token(4),
        timeoutMs: 1000,
      ),
      approvedBudget: budget,
    );

Uint8List coreFrame({
  required BigInt callId,
  required Uint8List submission,
  core.Kind kind = core.Kind.prepareCreate,
  core.Status status = core.Status.completed,
  core.Phase phase = core.Phase.prepared,
  core.Effect effect = core.Effect.unspecified,
  BigInt? staged,
  bool durable = false,
  Uint8List? reference,
  String operationId = 'guest-op',
}) {
  final builder = MessageBuilder();
  final row = builder.initRoot(core.responseFactory);
  row.version = 1;
  row.schemaSha256 = Uint8List.fromList(contract.mutationDigest);
  row.callIdBigInt = callId;
  row.reference = reference ?? token(6);
  row.submission = submission;
  row.operationId = operationId;
  row.kind = kind;
  row.status = status;
  row.phase = phase;
  row.effect = effect;
  row.stagedBytesBigInt = staged ?? BigInt.zero;
  row.durableContent = durable;
  return builder.serialize();
}

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
    io.submission = token(1); // Start token, never a command token.
    io.storage = 1;
    io.delivery = 2;
    final state = row.initGuestMutationState();
    state.commandBigInt = BigInt.one;
    state.kind = 0;
    state.delivery = 0;
    if (action == host.Action.guestMutationStart) {
      row.mutationCommandIdBigInt = BigInt.one;
    } else if (action == host.Action.guestMutationSubmit) {
      row.mutationCommandIdBigInt = BigInt.from(2);
      state.commandBigInt = BigInt.from(2);
      state.kind = last!.guestMutationCommand!.kind;
    } else if (action == host.Action.guestMutationRead ||
        action == host.Action.guestMutationCancelCommand) {
      row.mutationCommandIdBigInt = last!.mutationCommandIdBigInt;
      state.commandBigInt = last!.mutationCommandIdBigInt;
      state.kind = 2;
      state.delivery = action == host.Action.guestMutationRead ? 2 : 1;
      if (action == host.Action.guestMutationRead) {
        row.initGuestMutationResult().kind = 0;
      }
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
    'explicit budget and start retain exact UInt64 and independent approval',
    () async {
      final peer = Peer();
      final client = NativeGuestMutationClient(peer.exchange);
      final selected = start(
        budget: ApprovedGuestBudget(
          maxJobBytes: BigInt.from(32 * 1024 * 1024),
          maxBytes: BigInt.from(256 * 1024 * 1024),
        ),
      );
      final reply = await client.start(selected);
      expect(reply.receipt!.commandId, BigInt.one);
      expect(reply.state.kind, GuestMutationCommandKind.select);
      final sent = peer.last!.guestMutationStart!;
      expect(sent.selection!.registryRevisionBigInt, huge);
      expect(
        sent.approvedBudget!.maxJobBytesBigInt,
        BigInt.from(32 * 1024 * 1024),
      );
      expect(
        sent.approvedBudget!.maxBytesBigInt,
        BigInt.from(256 * 1024 * 1024),
      );
      expect(() => reply.receipt!.submission[0] = 9, throwsUnsupportedError);
      expect(peer.actions, [host.Action.guestMutationStart]);
    },
  );

  test(
    'bad approval, operation, chunk or token fails before one exchange',
    () async {
      final peer = Peer();
      final client = NativeGuestMutationClient(peer.exchange);
      expect(
        () => client.start(
          start(
            budget: ApprovedGuestBudget(
              maxJobBytes: BigInt.from(33 * 1024 * 1024),
              maxBytes: BigInt.from(256 * 1024 * 1024),
            ),
          ),
        ),
        throwsFormatException,
      );
      expect(
        () => client.submit(
          token(3),
          token(5),
          GuestMutationCommand.buildPlan(
            operationId: 'bad/operation',
            contentLength: BigInt.zero,
          ),
        ),
        throwsFormatException,
      );
      expect(
        () => client.submit(
          token(3),
          token(5),
          GuestMutationCommand.chunk(
            offset: BigInt.zero,
            bytes: Uint8List(61441),
            operationId: 'guest-op',
            guestReference: token(6),
          ),
        ),
        throwsFormatException,
      );
      expect(
        () => client.submit(
          Uint8List(32),
          token(5),
          GuestMutationCommand.hostQuery(),
        ),
        throwsFormatException,
      );
      expect(peer.actions, isEmpty);
    },
  );

  test(
    'single submit, explicit receipt, correlated and copied Core frame',
    () async {
      final peer = Peer();
      final client = NativeGuestMutationClient(peer.exchange);
      final command = GuestMutationCommand.prepare(
        planSha256: token(7),
        operationId: 'guest-op',
        disposition: MutationDisposition.create,
      );
      final submitted = await client.submit(token(3), token(5), command);
      expect(submitted.receipt!.kind, GuestMutationCommandKind.prepare);
      expect(peer.last!.guestMutationCommand!.kind, 2);
      expect(peer.last!.guestMutationCommand!.planSha256, token(7));
      expect(peer.last!.guestMutationCommand!.operationId, isNull);
      final frame = coreFrame(callId: BigInt.from(2), submission: token(5));
      peer.customize = (row) {
        final state = row.initGuestMutationState();
        state.commandBigInt = BigInt.from(2);
        state.kind = 2;
        state.delivery = 2;
        final result = row.initGuestMutationResult();
        result.kind = 2;
        result.frame = frame;
      };
      final read = await client.read(submitted.receipt!);
      expect(read.result!.frame!.status, GuestMutationFrameStatus.completed);
      expect(read.result!.frame!.reference, token(6));
      expect(read.result!.frame!.encoded, frame);
      expect(
        () => read.result!.frame!.reference[0] = 0,
        throwsUnsupportedError,
      );
      expect(peer.actions, [
        host.Action.guestMutationSubmit,
        host.Action.guestMutationRead,
      ]);
    },
  );

  test(
    'Core response must match exact command, phase, digest and trailing bound',
    () async {
      final peer = Peer();
      final client = NativeGuestMutationClient(peer.exchange);
      final receipt = (await client.submit(
        token(3),
        token(5),
        GuestMutationCommand.prepare(
          planSha256: token(7),
          operationId: 'guest-op',
          disposition: MutationDisposition.create,
        ),
      )).receipt!;
      Future<void> invalid(Uint8List frame) async {
        peer.customize = (row) {
          final state = row.initGuestMutationState();
          state.commandBigInt = receipt.commandId;
          state.kind = 2;
          state.delivery = 2;
          final result = row.initGuestMutationResult();
          result.kind = 2;
          result.frame = frame;
        };
        await expectLater(client.read(receipt), throwsFormatException);
      }

      await invalid(coreFrame(callId: BigInt.from(3), submission: token(5)));
      await invalid(coreFrame(callId: BigInt.from(2), submission: token(8)));
      await invalid(
        coreFrame(
          callId: BigInt.from(2),
          submission: token(5),
          operationId: 'other',
        ),
      );
      await invalid(
        coreFrame(
          callId: BigInt.from(2),
          submission: token(5),
          kind: core.Kind.chunk,
        ),
      );
      await invalid(
        coreFrame(
          callId: BigInt.from(2),
          submission: token(5),
          phase: core.Phase.observed,
        ),
      );
      await invalid(
        Uint8List.fromList([
          ...coreFrame(callId: BigInt.from(2), submission: token(5)),
          0,
        ]),
      );
    },
  );

  test('mixed host result and failure code are rejected', () async {
    final peer = Peer();
    final client = NativeGuestMutationClient(peer.exchange);
    final receipt = (await client.submit(
      token(3),
      token(5),
      GuestMutationCommand.prepare(
        planSha256: token(7),
        operationId: 'guest-op',
        disposition: MutationDisposition.create,
      ),
    )).receipt!;
    peer.customize = (row) {
      final state = row.initGuestMutationState();
      state.commandBigInt = receipt.commandId;
      state.kind = 2;
      state.delivery = 2;
      final result = row.initGuestMutationResult();
      result.kind = 3;
      result.failureKind = 1;
      result.failureCode = 99;
    };
    await expectLater(client.read(receipt), throwsFormatException);
    peer.customize = (row) {
      final state = row.initGuestMutationState();
      state.commandBigInt = receipt.commandId;
      state.kind = 2;
      state.delivery = 2;
      final result = row.initGuestMutationResult();
      result.kind = 2;
      result.frame = coreFrame(callId: receipt.commandId, submission: token(5));
      result.failureKind = 4;
    };
    await expectLater(client.read(receipt), throwsFormatException);
  });

  test('a forged frame receipt cannot omit the guest lease reference', () {
    final peer = Peer();
    final client = NativeGuestMutationClient(peer.exchange);
    final forged = GuestMutationReceipt(
      key: token(3),
      submission: token(5),
      commandId: BigInt.from(4),
      kind: GuestMutationCommandKind.chunk,
      operationId: 'guest-op',
      expectedChunkEnd: BigInt.from(3),
    );
    expect(() => client.read(forged), throwsFormatException);
    expect(peer.actions, isEmpty);
  });

  test('an internal Owner failure on a frame command is decoded', () async {
    final peer = Peer();
    final client = NativeGuestMutationClient(peer.exchange);
    final receipt = (await client.submit(
      token(3),
      token(5),
      GuestMutationCommand.prepare(
        planSha256: token(7),
        operationId: 'guest-op',
        disposition: MutationDisposition.create,
      ),
    )).receipt!;
    peer.customize = (row) {
      row.initGuestMutationState()
        ..commandBigInt = receipt.commandId
        ..kind = GuestMutationCommandKind.prepare.index
        ..delivery = 2;
      final result = row.initGuestMutationResult();
      result.kind = GuestMutationResultKind.owner.index;
      result.initOwner()
        ..kind = MutationResultKind.failure.index
        ..failureLayer = 1
        ..failureCode = 1;
    };
    final read = await client.read(receipt);
    expect(read.result!.owner!.kind, MutationResultKind.failure);
    expect(peer.actions.length, 2);
  });

  test(
    'empty Create digest and independent owner/Frame kind are strict',
    () async {
      final peer = Peer();
      final client = NativeGuestMutationClient(peer.exchange);
      final valid = GuestMutationCommand.buildPlan(
        operationId: 'guest-empty',
        contentLength: BigInt.zero,
        contentSha256: emptyHash,
      );
      await client.submit(token(3), token(5), valid);
      expect(peer.last!.guestMutationCommand!.contentSha256, emptyHash);
      final bad = GuestMutationCommand.buildPlan(
        operationId: 'guest-empty',
        contentLength: BigInt.zero,
        contentSha256: token(8),
      );
      expect(
        () => client.submit(token(3), token(5), bad),
        throwsFormatException,
      );
      expect(peer.actions.length, 1);
    },
  );
}
