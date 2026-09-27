import 'dart:typed_data';

import 'package:capnproto_dart/capnproto_dart.dart';

import 'generated/host.capnp.dart' as host;
import 'generated/identity.dart' as contract;
import 'generated/mutation.capnp.dart' as core;
import 'guest_mutation_models.dart';
import 'io_task_codec_native.dart';
import 'mutation_task_codec_native.dart';
import 'mutation_task_models.dart';

abstract final class GuestMutationCodec {
  static Uint8List? _data(Uint8List? v, int max, {bool id = false}) {
    if (v == null) return null;
    if (v.isEmpty || v.length > max || (id && v.length != 32)) {
      throw const FormatException('Invalid guest mutation data');
    }
    final copy = Uint8List.fromList(v).asUnmodifiableView();
    if (id) GuestMutationValidation.identity(copy);
    return copy;
  }

  static T _enum<T>(List<T> values, int code) {
    if (code < 0 || code >= values.length) {
      throw const FormatException('Unknown guest mutation enum');
    }
    return values[code];
  }

  static void writeStart(
    GuestMutationStartRequest value,
    host.GuestMutationStartBuilder out,
  ) {
    GuestMutationValidation.start(value);
    MutationTaskCodec.writeStart(value.selection, out.initSelection());
    final budget = value.approvedBudget;
    if (budget != null) {
      final row = out.initApprovedBudget();
      row.maxJobBytesBigInt = budget.maxJobBytes;
      row.maxBytesBigInt = budget.maxBytes;
    }
  }

  static void writeCommand(
    Uint8List submission,
    GuestMutationCommand command,
    host.GuestMutationCommandBuilder out,
  ) {
    GuestMutationValidation.identity(submission);
    GuestMutationValidation.command(command);
    out.submission = submission;
    out.kind = command.kind.index;
    switch (command.kind) {
      case GuestMutationCommandKind.buildPlan:
        out.operationId = command.operationId;
        out.contentLengthBigInt = command.contentLength!;
        out.contentSha256 = command.contentSha256;
      case GuestMutationCommandKind.prepare:
      case GuestMutationCommandKind.execute:
        out.planSha256 = command.planSha256;
      case GuestMutationCommandKind.chunk:
        out.offsetBigInt = command.offset!;
        out.bytes = command.bytes;
      case GuestMutationCommandKind.select:
      case GuestMutationCommandKind.commitContent:
      case GuestMutationCommandKind.query:
      case GuestMutationCommandKind.cancelPlan:
      case GuestMutationCommandKind.release:
      case GuestMutationCommandKind.hostQuery:
      case GuestMutationCommandKind.hostCancelPlan:
      case GuestMutationCommandKind.hostRelease:
        break;
    }
  }

  static GuestMutationState state(host.GuestMutationStateReader? row) {
    if (row == null) {
      throw const FormatException('Missing guest mutation state');
    }
    final command = row.commandBigInt;
    GuestMutationValidation.u64(command, positive: true);
    final kind = _enum(GuestMutationCommandKind.values, row.kind);
    final delivery = _enum(GuestMutationDelivery.values, row.delivery);
    final reference = _data(row.reference, 32, id: true);
    final expectedIdentity = _data(row.expectedIdentity, 32, id: true);
    final reviewedPlanSha256 = _data(row.reviewedPlanSha256, 32, id: true);
    final stagedBytes = row.stagedBytesBigInt;
    if ((row.selected != (reference != null)) ||
        (expectedIdentity != null && reference == null) ||
        (row.approvalDelivered &&
            (reviewedPlanSha256 == null ||
                (!row.selected &&
                    !(row.terminal &&
                        (kind == GuestMutationCommandKind.release ||
                            kind == GuestMutationCommandKind.hostRelease))))) ||
        (row.permitDelivered && !row.approvalDelivered) ||
        stagedBytes > BigInt.from(GuestMutationValidation.maxContentBytes)) {
      throw const FormatException('Contradictory guest mutation state');
    }
    return GuestMutationState(
      commandId: command,
      kind: kind,
      delivery: delivery,
      selected: row.selected,
      reference: reference,
      expectedIdentity: expectedIdentity,
      reviewedPlanSha256: reviewedPlanSha256,
      approvalDelivered: row.approvalDelivered,
      permitDelivered: row.permitDelivered,
      stagedBytes: stagedBytes,
      durableContent: row.durableContent,
      effectAttempted: row.effectAttempted,
      reconcileRequired: row.reconcileRequired,
      terminal: row.terminal,
    );
  }

  static MessageReader _frameMessage(Uint8List bytes) {
    if (bytes.length < 8 ||
        bytes.length > GuestMutationValidation.maxFrameBytes) {
      throw const FormatException('Invalid Core guest frame length');
    }
    final view = ByteData.sublistView(bytes);
    final count = view.getUint32(0, Endian.little) + 1;
    if (count > 512) throw const FormatException('Invalid Core guest segments');
    var total = ((count + 2) ~/ 2) * 8;
    if (total > bytes.length) {
      throw const FormatException('Invalid Core guest segment table');
    }
    for (var i = 0; i < count; i++) {
      total += view.getUint32(4 + i * 4, Endian.little) * 8;
      if (total > bytes.length) {
        throw const FormatException('Oversize Core guest segment');
      }
    }
    if (total != bytes.length) {
      throw const FormatException('Trailing Core guest frame bytes');
    }
    return MessageReader.deserialize(
      bytes,
      MessageReaderOptions(
        traversalLimitInWords: 32768,
        nestingLimit: 8,
        maxSegments: 512,
      ),
    );
  }

  static GuestMutationFrame frame(
    Uint8List bytes,
    GuestMutationReceipt receipt,
  ) {
    final row = _frameMessage(bytes).getRoot(core.responseFactory);
    final digest = row.schemaSha256;
    if (row.version != 1 ||
        digest == null ||
        !GuestMutationValidation.same(
          digest,
          Uint8List.fromList(contract.mutationDigest),
        )) {
      throw const FormatException('Core guest schema changed');
    }
    final kind = row.kind, status = row.status;
    final phase = row.phase, effect = row.effect;
    if (kind == null ||
        kind == core.Kind.invalid ||
        status == null ||
        status == core.Status.invalid ||
        phase == null ||
        effect == null) {
      throw const FormatException('Invalid Core guest response enum');
    }
    final reference = _data(row.reference, 32, id: true);
    final submission = _data(row.submission, 32, id: true);
    if (reference == null || submission == null || row.operationId == null) {
      throw const FormatException('Missing Core guest response identity');
    }
    final result = GuestMutationFrame(
      callId: row.callIdBigInt,
      reference: reference,
      submission: submission,
      operationId: row.operationId!,
      kind: GuestMutationFrameKind.values[kind.index - 1],
      status: GuestMutationFrameStatus.values[status.index - 1],
      phase: GuestMutationFramePhase.values[phase.index],
      effect: GuestMutationFrameEffect.values[effect.index],
      stagedBytes: row.stagedBytesBigInt,
      durableContent: row.durableContent,
      encoded: bytes,
    );
    GuestMutationValidation.frame(result, receipt);
    return result;
  }

  static GuestMutationResult? result(
    host.GuestMutationResultReader? row,
    GuestMutationReceipt receipt,
  ) {
    if (row == null) {
      throw const FormatException('Missing guest mutation result');
    }
    final kind = _enum(GuestMutationResultKind.values, row.kind);
    final frameBytes = row.frame;
    final hasFrame = frameBytes != null;
    final owner = row.owner;
    final failureKind = row.failureKind, failureCode = row.failureCode;
    switch (kind) {
      case GuestMutationResultKind.pending:
        if (owner != null || hasFrame || failureKind != 0 || failureCode != 0) {
          throw const FormatException('Mixed pending guest result');
        }
        return null;
      case GuestMutationResultKind.owner:
        if (owner == null || hasFrame || failureKind != 0 || failureCode != 0) {
          throw const FormatException('Mixed owner guest result');
        }
        final decoded = MutationTaskCodec.result(owner);
        if (decoded == null) {
          throw const FormatException('Empty owner guest result');
        }
        if (decoded.kind == MutationResultKind.failure) {
          return GuestMutationResult(kind: kind, owner: decoded);
        }
        final expected = switch (receipt.kind) {
          GuestMutationCommandKind.select => MutationResultKind.selected,
          GuestMutationCommandKind.buildPlan => MutationResultKind.planned,
          GuestMutationCommandKind.hostQuery => MutationResultKind.history,
          GuestMutationCommandKind.hostCancelPlan =>
            MutationResultKind.planCancelled,
          GuestMutationCommandKind.hostRelease => MutationResultKind.released,
          _ => throw const FormatException(
            'Owner result on guest frame command',
          ),
        };
        if (decoded.kind != expected) {
          throw const FormatException('Wrong owner guest result kind');
        }
        return GuestMutationResult(kind: kind, owner: decoded);
      case GuestMutationResultKind.frame:
        if (!hasFrame ||
            frameBytes.length > GuestMutationValidation.maxFrameBytes ||
            owner != null ||
            failureKind != 0 ||
            failureCode != 0 ||
            receipt.kind.index < GuestMutationCommandKind.prepare.index ||
            receipt.kind.index > GuestMutationCommandKind.release.index) {
          throw const FormatException('Mixed Core guest frame result');
        }
        return GuestMutationResult(
          kind: kind,
          frame: frame(frameBytes, receipt),
        );
      case GuestMutationResultKind.failure:
        if (owner != null || hasFrame || failureKind < 1 || failureKind > 4) {
          throw const FormatException('Mixed guest failure');
        }
        final type = GuestMutationFailureKind.values[failureKind - 1];
        GuestMutationJobError? job;
        GuestMutationFault? fault;
        if (type == GuestMutationFailureKind.job) {
          if (failureCode < 1 ||
              failureCode > GuestMutationJobError.values.length) {
            throw const FormatException('Unknown guest job failure');
          }
          job = GuestMutationJobError.values[failureCode - 1];
        } else if (type == GuestMutationFailureKind.execution) {
          if (failureCode < 1 ||
              failureCode > GuestMutationFault.values.length) {
            throw const FormatException('Unknown guest execution fault');
          }
          fault = GuestMutationFault.values[failureCode - 1];
        } else if (failureCode != 0) {
          throw const FormatException('Unexpected guest failure code');
        }
        return GuestMutationResult(
          kind: kind,
          failure: GuestMutationFailure(kind: type, job: job, fault: fault),
        );
    }
  }

  static GuestMutationTaskReply reply(
    host.ResponseReader row, {
    Uint8List? key,
    Uint8List? taskSubmission,
    Uint8List? receiptSubmission,
    GuestMutationCommandKind? kind,
    String? operationId,
    MutationDisposition? disposition,
    Uint8List? expectedGuestReference,
    BigInt? expectedChunkEnd,
    BigInt? commandId,
    bool statusOnly = false,
    bool start = false,
  }) {
    final io = HttpTaskCodec.snapshot(row.ioState);
    final stateValue = state(row.guestMutationState);
    final receiptId = row.mutationCommandIdBigInt;
    if (io.key == null ||
        (key != null && !GuestMutationValidation.same(io.key!, key)) ||
        (taskSubmission != null &&
            (io.submission == null ||
                !GuestMutationValidation.same(
                  io.submission!,
                  taskSubmission,
                )))) {
      throw const FormatException('Guest task identity changed');
    }
    if (statusOnly) {
      if (receiptId != BigInt.zero) {
        throw const FormatException('Unexpected guest status receipt');
      }
    } else {
      GuestMutationValidation.u64(receiptId, positive: true);
      if ((start && receiptId != BigInt.one) ||
          (commandId != null && receiptId != commandId) ||
          stateValue.commandId < receiptId ||
          (stateValue.commandId == receiptId &&
              kind != null &&
              stateValue.kind != kind) ||
          (commandId != null && stateValue.commandId != commandId)) {
        throw const FormatException('Guest command receipt changed');
      }
    }
    final receipt = statusOnly || receiptSubmission == null || kind == null
        ? null
        : GuestMutationReceipt(
            key: io.key!,
            submission: receiptSubmission,
            commandId: receiptId,
            kind: kind,
            operationId: operationId,
            disposition: disposition,
            expectedGuestReference: expectedGuestReference,
            expectedChunkEnd: expectedChunkEnd,
          );
    return GuestMutationTaskReply(
      io: io,
      state: stateValue,
      commandId: receiptId,
      receipt: receipt,
    );
  }
}
