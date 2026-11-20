import 'dart:typed_data';
import 'package:capnproto_dart/capnproto_dart.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/generated/host.capnp.dart' as host;
import 'package:morrow_studio/plugins/generated/mutation.capnp.dart' as wire;
import 'package:morrow_studio/plugins/generated/identity.dart' as contract;
import 'package:morrow_studio/plugins/guest_mutation_codec_native.dart';
import 'package:morrow_studio/plugins/guest_mutation_models.dart';
import 'package:morrow_studio/plugins/guest_mutation_native.dart';
import 'package:morrow_studio/plugins/mutation_task_models.dart';

Uint8List id(int v) => Uint8List.fromList(List.filled(32, v));
void main() {
  test('a rejected selection preserves the owner admission failure', () async {
    final receipt = GuestMutationReceipt(
      key: id(1),
      submission: id(2),
      commandId: BigInt.one,
      kind: GuestMutationCommandKind.select,
    );
    final message = MessageBuilder();
    final row = message.initRoot(host.responseFactory);
    row.mutationCommandIdBigInt = receipt.commandId;
    row.initIoState()
      ..key = receipt.key
      ..submission = receipt.submission
      ..storage = 1;
    final state = row.initGuestMutationState()
      ..commandBigInt = receipt.commandId
      ..kind = receipt.kind.index
      ..delivery = GuestMutationDelivery.consumed.index;
    final result = row.initGuestMutationResult()
      ..kind = GuestMutationResultKind.owner.index;
    result.initOwner()
      ..kind = MutationResultKind.failure.index
      ..failureLayer = 2
      ..failureCode = 1;
    final client = NativeGuestMutationClient(<T>(
      action, {
      configure,
      required decode,
      required clearReply,
    }) async {
      expect(action, host.Action.guestMutationRead);
      return decode(
        MessageReader.deserialize(message.serialize())
            .getRoot(host.responseFactory),
      );
    });

    final read = await client.read(receipt);
    expect(
      read.result!.owner!.failure!.target,
      MutationTargetFailure.admission,
    );
    expect(read.reply.state.selected, isFalse);
    state
      ..selected = true
      ..reference = id(3);
    await expectLater(client.read(receipt), throwsFormatException);
  });

  test(
    'new PrepareReplace response is correlated to the original selected operation',
    () {
      final receipt = GuestMutationReceipt(
        key: id(1),
        submission: id(2),
        commandId: BigInt.from(7),
        kind: GuestMutationCommandKind.prepare,
        operationId: 'replace-original',
        disposition: MutationDisposition.replace,
      );
      final message = MessageBuilder();
      final row = message.initRoot(wire.responseFactory);
      row.version = 1;
      row.schemaSha256 = Uint8List.fromList(contract.mutationDigest);
      row.callIdBigInt = BigInt.from(7);
      row.reference = id(3);
      row.submission = id(2);
      row.operationId = 'replace-original';
      row.kind = wire.Kind.prepareReplace;
      row.status = wire.Status.completed;
      row.phase = wire.Phase.prepared;
      expect(
        GuestMutationCodec.frame(message.serialize(), receipt).kind,
        GuestMutationFrameKind.prepareReplace,
      );
      row.kind = wire.Kind.prepareCreate;
      expect(
        () => GuestMutationCodec.frame(message.serialize(), receipt),
        throwsFormatException,
      );
      row.kind = wire.Kind.prepareReplace;
      row.effect = wire.Effect.osSucceeded;
      expect(
        () => GuestMutationCodec.frame(message.serialize(), receipt),
        throwsFormatException,
      );
    },
  );
  test(
    'explicit resource approval round trips separately and keeps zero as legacy',
    () {
      for (final resources in [0, 1, 128]) {
        final request = GuestMutationStartRequest(
          selection: MutationStartRequest(
            submission: id(1),
            packageId: 'test.replace',
            packageDigest: id(2),
            registryRevision: BigInt.one,
            disposition: MutationDisposition.replace,
            selectedPath: r'C:\test\original.bin',
            relativePath: '',
            subject: 'replace.test',
            approvalSha256: id(3),
            timeoutMs: 30000,
          ),
          approvedBudget: ApprovedGuestBudget(
            maxJobBytes: BigInt.one,
            maxBytes: BigInt.one,
            maxResources: resources,
          ),
        );
        final message = MessageBuilder();
        GuestMutationCodec.writeStart(
          request,
          message.initRoot(host.guestMutationStartFactory),
        );
        final row = MessageReader.deserialize(
          message.serialize(),
        ).getRoot(host.guestMutationStartFactory);
        expect(row.approvedBudget!.maxResources, resources);
        expect(row.selection!.disposition, 2);
      }
      for (final resources in [-1, 129]) {
        expect(
          () => GuestMutationValidation.budget(
            ApprovedGuestBudget(
              maxJobBytes: BigInt.one,
              maxBytes: BigInt.one,
              maxResources: resources,
            ),
          ),
          throwsFormatException,
        );
      }
    },
  );
}
