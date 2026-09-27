import 'dart:typed_data';

import 'generated/host.capnp.dart' as host;
import 'guest_mutation_codec_native.dart';
import 'guest_mutation_models.dart';

typedef GuestMutationExchange =
    Future<T> Function<T>(
      host.Action action, {
      void Function(host.RequestBuilder)? configure,
      required T Function(host.ResponseReader) decode,
      required bool clearReply,
    });

/// Private trusted-UI transport only. Every method performs one explicit
/// exchange; the caller retains submission/receipt for any exact retry.
final class NativeGuestMutationClient implements GuestMutationBackend {
  NativeGuestMutationClient(this._exchange);
  final GuestMutationExchange _exchange;

  @override
  Future<GuestMutationTaskReply> start(GuestMutationStartRequest request) {
    GuestMutationValidation.start(request);
    return _exchange(
      host.Action.guestMutationStart,
      configure: (row) =>
          GuestMutationCodec.writeStart(request, row.initGuestMutationStart()),
      clearReply: true,
      decode: (row) => GuestMutationCodec.reply(
        row,
        taskSubmission: request.selection.submission,
        receiptSubmission: request.selection.submission,
        kind: GuestMutationCommandKind.select,
        start: true,
      ),
    );
  }

  @override
  Future<GuestMutationTaskReply> submit(
    Uint8List key,
    Uint8List submission,
    GuestMutationCommand command,
  ) {
    final safeKey = GuestMutationValidation.identity(key);
    final safeSubmission = GuestMutationValidation.identity(submission);
    GuestMutationValidation.command(command);
    final expectedEnd = command.kind == GuestMutationCommandKind.chunk
        ? command.offset! + BigInt.from(command.bytes!.length)
        : null;
    return _exchange(
      host.Action.guestMutationSubmit,
      configure: (row) {
        row.ioKey = safeKey;
        GuestMutationCodec.writeCommand(
          safeSubmission,
          command,
          row.initGuestMutationCommand(),
        );
      },
      clearReply: true,
      decode: (row) => GuestMutationCodec.reply(
        row,
        key: safeKey,
        receiptSubmission: safeSubmission,
        kind: command.kind,
        operationId: command.operationId,
        disposition: command.disposition,
        expectedGuestReference: command.expectedGuestReference,
        expectedChunkEnd: expectedEnd,
      ),
    );
  }

  @override
  Future<GuestMutationTaskReply> status(Uint8List key) {
    final safeKey = GuestMutationValidation.identity(key);
    return _exchange(
      host.Action.guestMutationStatus,
      configure: (row) => row.ioKey = safeKey,
      clearReply: true,
      decode: (row) =>
          GuestMutationCodec.reply(row, key: safeKey, statusOnly: true),
    );
  }

  @override
  Future<GuestMutationTaskRead> read(GuestMutationReceipt receipt) {
    GuestMutationValidation.receipt(receipt);
    final key = GuestMutationValidation.identity(receipt.key);
    final commandId = receipt.commandId;
    return _exchange(
      host.Action.guestMutationRead,
      configure: (row) {
        row.ioKey = key;
        row.mutationCommandIdBigInt = commandId;
      },
      clearReply: true,
      decode: (row) {
        final reply = GuestMutationCodec.reply(
          row,
          key: key,
          receiptSubmission: receipt.submission,
          kind: receipt.kind,
          operationId: receipt.operationId,
          disposition: receipt.disposition,
          expectedGuestReference: receipt.expectedGuestReference,
          expectedChunkEnd: receipt.expectedChunkEnd,
          commandId: commandId,
        );
        final result = GuestMutationCodec.result(
          row.guestMutationResult,
          receipt,
        );
        if ((result == null &&
                reply.state.delivery == GuestMutationDelivery.consumed) ||
            (result != null &&
                reply.state.delivery != GuestMutationDelivery.consumed)) {
          throw const FormatException('Guest result delivery changed');
        }
        if (result?.kind == GuestMutationResultKind.owner &&
            receipt.kind == GuestMutationCommandKind.select) {
          final selected = result!.owner!.reference;
          final current = reply.state.reference;
          if (selected == null ||
              current == null ||
              !GuestMutationValidation.same(selected, current)) {
            throw const FormatException('Guest selected target changed');
          }
        }
        return GuestMutationTaskRead(reply: reply, result: result);
      },
    );
  }

  @override
  Future<GuestMutationTaskReply> cancelCommand(
    Uint8List key,
    BigInt commandId,
  ) {
    final safeKey = GuestMutationValidation.identity(key);
    GuestMutationValidation.u64(commandId, positive: true);
    return _exchange(
      host.Action.guestMutationCancelCommand,
      configure: (row) {
        row.ioKey = safeKey;
        row.mutationCommandIdBigInt = commandId;
      },
      clearReply: true,
      decode: (row) =>
          GuestMutationCodec.reply(row, key: safeKey, commandId: commandId),
    );
  }
}
