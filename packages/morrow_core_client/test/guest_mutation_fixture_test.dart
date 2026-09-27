import 'dart:io';
import 'dart:typed_data';

import 'package:capnproto_dart/capnproto_dart.dart';
import 'package:test/test.dart';

import '../../../lib/plugins/generated/host.capnp.dart' as host;
import '../../../lib/plugins/guest_mutation_codec_native.dart';
import '../../../lib/plugins/guest_mutation_models.dart';
import '../../../lib/plugins/guest_mutation_native.dart';
import '../../../lib/plugins/mutation_task_models.dart';

Uint8List _token(int value) => Uint8List.fromList(List.filled(32, value));

void main() {
  final fixtures = Platform.environment['MORROW_GUEST_MUTATION_WIRE_FIXTURES'];
  host.ResponseReader response(String name) {
    final file = File('$fixtures${Platform.pathSeparator}rust-guest-$name.bin');
    expect(file.existsSync(), isTrue, reason: name);
    return MessageReader.deserialize(
      file.readAsBytesSync(),
    ).getRoot(host.responseFactory);
  }

  Future<GuestMutationTaskRead> readFixture(
    String name,
    GuestMutationReceipt receipt,
  ) {
    Future<T> exchange<T>(
      host.Action action, {
      void Function(host.RequestBuilder)? configure,
      required T Function(host.ResponseReader) decode,
      required bool clearReply,
    }) async {
      expect(action, host.Action.guestMutationRead);
      final request = MessageBuilder().initRoot(host.requestFactory);
      configure?.call(request);
      expect(request.asReader().ioKey, receipt.key);
      expect(request.asReader().mutationCommandIdBigInt, receipt.commandId);
      return decode(response(name));
    }

    return NativeGuestMutationClient(exchange).read(receipt);
  }

  test(
    'real Rust Release reaches full native read decoding',
    () async {
      final prepared = response('kind-2-3');
      final prepareReceipt = GuestMutationReceipt(
        key: prepared.ioState!.key!,
        submission: _token(3),
        commandId: prepared.guestMutationState!.commandBigInt,
        kind: GuestMutationCommandKind.prepare,
        operationId: 'private-guest-create',
        disposition: MutationDisposition.create,
      );
      final reference = (await readFixture(
        'kind-2-3',
        prepareReceipt,
      )).result!.frame!.reference;
      final row = response('kind-8-12');
      final receipt = GuestMutationReceipt(
        key: row.ioState!.key!,
        submission: _token(12),
        commandId: row.guestMutationState!.commandBigInt,
        kind: GuestMutationCommandKind.release,
        operationId: 'private-guest-create',
        expectedGuestReference: reference,
      );
      final read = await readFixture('kind-8-12', receipt);
      expect(read.result!.frame!.phase, GuestMutationFramePhase.none);
      expect(read.reply.state.selected, isFalse);
      expect(read.reply.state.approvalDelivered, isTrue);
    },
    skip: fixtures == null ? 'Set MORROW_GUEST_MUTATION_WIRE_FIXTURES' : false,
  );

  test(
    'real Rust owner, status and internal IssueGuest failure decode',
    () async {
      final started = response('started');
      final begin = GuestMutationCodec.reply(
        started,
        taskSubmission: _token(1),
        receiptSubmission: _token(1),
        kind: GuestMutationCommandKind.select,
        start: true,
      );
      final key = begin.io.key!;
      expect(begin.receipt!.commandId, BigInt.one);
      final selectedReceipt = GuestMutationReceipt(
        key: key,
        submission: _token(1),
        commandId: BigInt.one,
        kind: GuestMutationCommandKind.select,
      );
      expect(
        (await readFixture('selected', selectedReceipt)).result!.owner!.kind,
        MutationResultKind.selected,
      );
      final plannedReceipt = GuestMutationReceipt(
        key: key,
        submission: _token(2),
        commandId: BigInt.from(2),
        kind: GuestMutationCommandKind.buildPlan,
        operationId: 'private-guest-create',
      );
      final result = (await readFixture('planned', plannedReceipt)).result!;
      expect(result.owner!.kind, MutationResultKind.planned);
      expect(result.owner!.plan, isNotEmpty);
      final status = GuestMutationCodec.reply(
        response('planned-status'),
        key: key,
        statusOnly: true,
      );
      expect(status.receipt, isNull);
      expect(status.state.reviewedPlanSha256, hasLength(32));

      final failure = response('owner-issue-failure');
      final failureReceipt = GuestMutationReceipt(
        key: failure.ioState!.key!,
        submission: _token(73),
        commandId: failure.guestMutationState!.commandBigInt,
        kind: GuestMutationCommandKind.prepare,
        operationId: 'private-owner-failure',
        disposition: MutationDisposition.create,
      );
      final failed = (await readFixture(
        'owner-issue-failure',
        failureReceipt,
      )).result!;
      expect(failed.owner!.kind, MutationResultKind.failure);
      expect(failure.guestMutationState!.reconcileRequired, isTrue);
    },
    skip: fixtures == null ? 'Set MORROW_GUEST_MUTATION_WIRE_FIXTURES' : false,
  );

  test(
    'real Rust Core frames require exact explicit receipts',
    () async {
      Uint8List? createReference;
      for (final (name, token, kind, operation, disposition, chunkEnd) in [
        (
          'kind-2-3',
          3,
          GuestMutationCommandKind.prepare,
          'private-guest-create',
          MutationDisposition.create,
          null,
        ),
        (
          'kind-3-4',
          4,
          GuestMutationCommandKind.chunk,
          'private-guest-create',
          null,
          61440,
        ),
        (
          'kind-3-5',
          5,
          GuestMutationCommandKind.chunk,
          'private-guest-create',
          null,
          122880,
        ),
        (
          'kind-3-6',
          6,
          GuestMutationCommandKind.chunk,
          'private-guest-create',
          null,
          184320,
        ),
        (
          'kind-3-7',
          7,
          GuestMutationCommandKind.chunk,
          'private-guest-create',
          null,
          184357,
        ),
        (
          'kind-4-8',
          8,
          GuestMutationCommandKind.commitContent,
          'private-guest-create',
          null,
          null,
        ),
        (
          'kind-5-9',
          9,
          GuestMutationCommandKind.execute,
          'private-guest-create',
          null,
          null,
        ),
        (
          'kind-6-11',
          11,
          GuestMutationCommandKind.query,
          'private-guest-create',
          null,
          null,
        ),
        (
          'kind-8-12',
          12,
          GuestMutationCommandKind.release,
          'private-guest-create',
          null,
          null,
        ),
      ]) {
        final row = response(name);
        final receipt = GuestMutationReceipt(
          key: row.ioState!.key!,
          submission: _token(token),
          commandId: row.guestMutationState!.commandBigInt,
          kind: kind,
          operationId: operation,
          disposition: disposition,
          expectedGuestReference: kind == GuestMutationCommandKind.prepare
              ? null
              : createReference,
          expectedChunkEnd: chunkEnd == null ? null : BigInt.from(chunkEnd),
        );
        final decoded = (await readFixture(name, receipt)).result!;
        expect(decoded.kind, GuestMutationResultKind.frame, reason: name);
        createReference ??= decoded.frame!.reference;
        expect(decoded.frame!.reference, createReference, reason: name);
      }
      expect(createReference, hasLength(32));
      Uint8List? deleteReference;
      for (final (name, token, kind, disposition) in [
        (
          'kind-2-15',
          15,
          GuestMutationCommandKind.prepare,
          MutationDisposition.delete,
        ),
        ('kind-5-16', 16, GuestMutationCommandKind.execute, null),
        ('kind-8-17', 17, GuestMutationCommandKind.release, null),
      ]) {
        final row = response(name);
        final receipt = GuestMutationReceipt(
          key: row.ioState!.key!,
          submission: _token(token),
          commandId: row.guestMutationState!.commandBigInt,
          kind: kind,
          operationId: 'private-guest-delete',
          disposition: disposition,
          expectedGuestReference: kind == GuestMutationCommandKind.prepare
              ? null
              : deleteReference,
        );
        final decoded = (await readFixture(name, receipt)).result!;
        expect(decoded.kind, GuestMutationResultKind.frame, reason: name);
        deleteReference ??= decoded.frame!.reference;
        expect(decoded.frame!.reference, deleteReference, reason: name);
      }
      expect(deleteReference, hasLength(32));
      final executed = response('kind-5-9');
      expect(
        () => GuestMutationCodec.result(
          executed.guestMutationResult,
          GuestMutationReceipt(
            key: executed.ioState!.key!,
            submission: _token(9),
            commandId: executed.guestMutationState!.commandBigInt,
            kind: GuestMutationCommandKind.execute,
            operationId: 'private-guest-create',
            expectedGuestReference: _token(255),
          ),
        ),
        throwsFormatException,
      );
    },
    skip: fixtures == null ? 'Set MORROW_GUEST_MUTATION_WIRE_FIXTURES' : false,
  );
}
