import 'dart:convert';
import 'dart:typed_data';
import 'generated/host.capnp.dart' as host;
import 'io_task_codec_native.dart';
import 'mutation_task_models.dart';

abstract final class MutationTaskCodec {
  static T _enumeration<T>(List<T> values, int code) {
    if (code < 0 || code >= values.length) {
      throw const FormatException('Unknown mutation wire value');
    }
    return values[code];
  }

  static Uint8List? _data(
    Uint8List? value,
    int bound, {
    bool identity = false,
  }) {
    if (value == null || value.isEmpty) return null;
    if (value.length > bound || (identity && value.length != 32)) {
      throw const FormatException('Invalid mutation data');
    }
    return Uint8List.fromList(value).asUnmodifiableView();
  }

  static void writeStart(
    MutationStartRequest value,
    host.MutationStartBuilder out,
  ) {
    MutationTaskValidation.validateStart(value);
    out.submission = value.submission;
    out.packageId = value.packageId;
    out.packageDigest = value.packageDigest;
    out.registryRevisionBigInt = value.registryRevision;
    out.disposition = value.disposition.index + 1;
    out.selectedPath = value.selectedPath;
    out.relativePath = value.relativePath;
    out.subject = value.subject;
    out.approvalSha256 = value.approvalSha256;
    out.timeoutMs = value.timeoutMs;
  }

  static void writeReconciliation(
    MutationReconcileRequest value,
    host.MutationReconcileBuilder out,
  ) {
    MutationTaskValidation.validateReconciliation(value);
    out.submission = value.submission;
    out.packageId = value.packageId;
    out.packageDigest = value.packageDigest;
    out.registryRevisionBigInt = value.registryRevision;
    out.plan = value.plan;
    out.timeoutMs = value.timeoutMs;
  }

  static void writeDiscovery(
    MutationDiscoverRequest value,
    host.MutationDiscoverBuilder out,
  ) {
    MutationTaskValidation.validateDiscovery(value);
    out.submission = value.submission;
    out.packageId = value.packageId;
    out.packageDigest = value.packageDigest;
    out.registryRevisionBigInt = value.registryRevision;
    out.subject = value.subject;
    out.disposition = value.disposition.index + 1;
    out.scanLimit = value.scanLimit;
    out.timeoutMs = value.timeoutMs;
    out.checkpoint = value.checkpoint;
  }

  static void writeCommand({
    required Uint8List submission,
    required MutationCommandKind kind,
    Uint8List? plan,
    BigInt? offset,
    Uint8List? bytes,
    String? operationId,
    BigInt? contentLength,
    Uint8List? contentSha256,
    int? scanLimit,
    required host.MutationCommandBuilder out,
  }) {
    MutationTaskValidation.identity(submission);
    if ((kind.index < 1 || kind.index > 7) &&
        kind != MutationCommandKind.buildPlan &&
        kind != MutationCommandKind.discover) {
      throw const FormatException('Invalid submitted mutation command');
    }
    if (kind == MutationCommandKind.prepare) {
      if (plan == null) throw const FormatException('Missing mutation plan');
      MutationTaskValidation.plan(plan);
    } else if (plan != null) {
      throw const FormatException('Unexpected mutation plan');
    }
    if (kind == MutationCommandKind.chunk) {
      if (offset == null || bytes == null) {
        throw const FormatException('Missing mutation chunk');
      }
      MutationTaskValidation.chunk(offset, bytes);
    } else if (offset != null || bytes != null) {
      throw const FormatException('Unexpected mutation chunk');
    }
    if (kind == MutationCommandKind.buildPlan) {
      if (operationId == null || contentLength == null) {
        throw const FormatException('Missing mutation plan specification');
      }
      MutationTaskValidation.buildPlan(
        operationId,
        contentLength,
        contentSha256,
      );
    } else if (operationId != null ||
        contentLength != null ||
        contentSha256 != null) {
      throw const FormatException('Unexpected mutation plan specification');
    }
    if (kind == MutationCommandKind.discover) {
      if (scanLimit == null) {
        throw const FormatException('Missing mutation plan scan limit');
      }
      MutationTaskValidation.scanLimit(scanLimit);
    } else if (scanLimit != null) {
      throw const FormatException('Unexpected mutation plan scan limit');
    }
    out.submission = submission;
    out.kind = kind.index;
    if (kind == MutationCommandKind.prepare) out.plan = plan;
    if (kind == MutationCommandKind.chunk) {
      out.offsetBigInt = offset!;
      out.bytes = bytes;
    }
    if (kind == MutationCommandKind.buildPlan) {
      out.operationId = operationId;
      out.contentLengthBigInt = contentLength!;
      out.contentSha256 = contentSha256 ?? Uint8List(0);
    }
    if (kind == MutationCommandKind.discover) out.scanLimit = scanLimit!;
  }

  static MutationState state(host.MutationStateReader? row) {
    if (row == null) throw const FormatException('Missing mutation state');
    final id = row.commandBigInt;
    MutationTaskValidation.commandId(id);
    final kind = _enumeration(MutationCommandKind.values, row.kind);
    final delivery = _enumeration(MutationDelivery.values, row.delivery);
    final reference = _data(row.reference, 32, identity: true);
    final identity = _data(row.expectedIdentity, 32, identity: true);
    if ((row.selected && reference == null) ||
        (!row.selected && (reference != null || identity != null)) ||
        (identity != null && reference == null) ||
        (kind == MutationCommandKind.reconcile && row.selected) ||
        (kind == MutationCommandKind.discover &&
            (row.selected || row.reconcileRequired))) {
      throw const FormatException('Inconsistent mutation state');
    }
    return MutationState(
      commandId: id,
      kind: kind,
      delivery: delivery,
      selected: row.selected,
      reconcileRequired: row.reconcileRequired,
      terminal: row.terminal,
      reference: reference,
      expectedIdentity: identity,
    );
  }

  static MutationResult? result(host.MutationResultReader? row) {
    if (row == null) throw const FormatException('Missing mutation result');
    final kind = _enumeration(MutationResultKind.values, row.kind);
    final reference = _data(row.reference, 32, identity: true);
    final identity = _data(row.expectedIdentity, 32, identity: true);
    final record = _data(row.record, MutationTaskValidation.maxRecordBytes);
    final outcome = _data(row.outcome, MutationTaskValidation.maxOutcomeBytes);
    final plan = _data(row.plan, MutationTaskValidation.maxPlanBytes);
    final rawPlans = row.plans;
    final checkpoint = _data(row.checkpoint, 32, identity: true);
    if (checkpoint != null) MutationTaskValidation.identity(checkpoint);
    final scanned = row.scanned;
    final done = row.done;
    List<Uint8List>? plans;
    if (kind == MutationResultKind.plans) {
      if (rawPlans != null &&
          rawPlans.length > MutationTaskValidation.maxPlanScanLimit) {
        throw const FormatException('Too many mutation discovery plans');
      }
      var total = 0;
      for (var i = 0; i < (rawPlans?.length ?? 0); i++) {
        final bytes = rawPlans![i];
        if (bytes == null ||
            bytes.isEmpty ||
            bytes.length > MutationTaskValidation.maxPlanBytes) {
          throw const FormatException('Invalid mutation discovery plan');
        }
        total += bytes.length;
      }
      if (total >
          MutationTaskValidation.maxPlanScanLimit *
              MutationTaskValidation.maxPlanBytes) {
        throw const FormatException('Oversize mutation discovery page');
      }
      plans = List<Uint8List>.generate(
        rawPlans?.length ?? 0,
        (i) => rawPlans![i]!,
        growable: false,
      );
    } else if (rawPlans != null || scanned != 0 || done || checkpoint != null) {
      throw const FormatException('Discovery fields without discovery result');
    }
    final staged = row.stagedBytesBigInt;
    final phase = _enumeration(MutationPhase.values, row.phase);
    final effect = _enumeration(MutationEffect.values, row.effect);
    final operation = row.operationId ?? '';
    if (utf8.encode(operation).length > 256 ||
        operation.runes.any(
          (r) =>
              r < 32 ||
              (r >= 127 && r <= 159) ||
              r == 0x2f ||
              r == 0x5c ||
              r == 0x3a,
        )) {
      throw const FormatException('Invalid mutation operation identity');
    }
    final layer = row.failureLayer;
    final code = row.failureCode;
    final osCode = row.osCode;
    if ((identity != null && reference == null) ||
        (effect == MutationEffect.osRejected && osCode == 0) ||
        (effect != MutationEffect.osRejected && osCode != 0)) {
      throw const FormatException('Inconsistent mutation result');
    }
    MutationFailure? failure;
    if (kind == MutationResultKind.failure) {
      if (layer == 1 && code >= 1 && code <= 6) {
        failure = MutationFailure(
          layer: MutationFailureLayer.delivery,
          delivery: MutationDeliveryFailure.values[code - 1],
        );
      } else if (layer == 2 && code >= 1 && code <= 18) {
        failure = MutationFailure(
          layer: MutationFailureLayer.target,
          target: MutationTargetFailure.values[code - 1],
        );
      } else {
        throw const FormatException('Unknown mutation failure');
      }
    } else if (layer != 0 || code != 0) {
      throw const FormatException('Failure fields without failure');
    }
    final hasRecord = record != null;
    final hasOutcome = outcome != null;
    final hasPlan = plan != null;
    final hasOperation = operation.isNotEmpty;
    bool clean() =>
        reference == null &&
        identity == null &&
        !hasRecord &&
        !hasOutcome &&
        !hasPlan &&
        staged == BigInt.zero &&
        !row.durableContent &&
        phase == MutationPhase.none &&
        !hasOperation &&
        effect == MutationEffect.unspecified;
    switch (kind) {
      case MutationResultKind.pending:
        if (!clean() || failure != null) {
          throw const FormatException('Invalid pending result');
        }
        return null;
      case MutationResultKind.selected:
        if (reference == null ||
            hasPlan ||
            hasRecord ||
            hasOutcome ||
            staged != BigInt.zero ||
            row.durableContent ||
            phase != MutationPhase.none ||
            hasOperation ||
            effect != MutationEffect.unspecified) {
          throw const FormatException('Invalid selected result');
        }
      case MutationResultKind.prepared:
        if (reference != null ||
            hasPlan ||
            hasOutcome ||
            !hasRecord ||
            phase != MutationPhase.prepared ||
            !hasOperation ||
            staged != BigInt.zero ||
            row.durableContent ||
            effect != MutationEffect.unspecified) {
          throw const FormatException('Invalid prepared result');
        }
      case MutationResultKind.staged:
        if (reference != null ||
            hasPlan ||
            hasRecord ||
            hasOutcome ||
            phase != MutationPhase.none ||
            hasOperation ||
            effect != MutationEffect.unspecified) {
          throw const FormatException('Invalid staged result');
        }
      case MutationResultKind.created:
      case MutationResultKind.deleted:
        if (reference != null ||
            hasPlan ||
            hasRecord ||
            !hasOutcome ||
            phase != MutationPhase.observed ||
            !hasOperation ||
            staged != BigInt.zero ||
            row.durableContent ||
            effect == MutationEffect.unspecified) {
          throw const FormatException('Invalid effect result');
        }
      case MutationResultKind.history:
        if (reference != null ||
            hasPlan ||
            hasOutcome ||
            (hasRecord != hasOperation) ||
            (hasRecord == (phase == MutationPhase.none)) ||
            staged < BigInt.zero ||
            effect != MutationEffect.unspecified) {
          throw const FormatException('Invalid history result');
        }
      case MutationResultKind.released:
        if (!clean()) throw const FormatException('Invalid released result');
      case MutationResultKind.planCancelled:
        if (reference != null ||
            hasPlan ||
            !hasRecord ||
            hasOutcome ||
            phase != MutationPhase.cancelledBeforeDispatch ||
            !hasOperation ||
            staged != BigInt.zero ||
            row.durableContent ||
            effect != MutationEffect.unspecified) {
          throw const FormatException('Invalid cancelled result');
        }
      case MutationResultKind.failure:
        if (!clean() || failure == null) {
          throw const FormatException('Invalid failure result');
        }
      case MutationResultKind.reconciled:
        if (reference != null ||
            hasPlan ||
            staged != BigInt.zero ||
            row.durableContent ||
            (hasRecord != hasOperation) ||
            (hasRecord == (phase == MutationPhase.none)) ||
            (hasOutcome != (effect != MutationEffect.unspecified)) ||
            (hasOutcome != (phase == MutationPhase.observed))) {
          throw const FormatException('Invalid reconciliation result');
        }
      case MutationResultKind.planned:
        if (!hasPlan ||
            reference != null ||
            identity != null ||
            hasRecord ||
            hasOutcome ||
            staged != BigInt.zero ||
            row.durableContent ||
            phase != MutationPhase.none ||
            hasOperation ||
            effect != MutationEffect.unspecified ||
            failure != null) {
          throw const FormatException('Invalid planned result');
        }
      case MutationResultKind.plans:
        if (!clean() ||
            failure != null ||
            scanned > MutationTaskValidation.maxPlanScanLimit ||
            plans!.length > scanned ||
            (!done && (scanned == 0 || checkpoint == null)) ||
            (done && checkpoint != null)) {
          throw const FormatException('Invalid mutation discovery result');
        }
    }
    return MutationResult(
      kind: kind,
      reference: reference,
      expectedIdentity: identity,
      record: record,
      outcome: outcome,
      plan: plan,
      plans: plans,
      checkpoint: checkpoint,
      stagedBytes: staged,
      durableContent: row.durableContent,
      phase: phase,
      operationId: hasOperation ? operation : null,
      effect: effect,
      osCode: effect == MutationEffect.osRejected ? osCode : null,
      scanned: scanned,
      done: done,
      failure: failure,
    );
  }

  static MutationTaskReply reply(
    host.ResponseReader row, {
    Uint8List? key,
    Uint8List? submission,
    BigInt? commandId,
    MutationCommandKind? startedKind,
    MutationCommandKind? submittedKind,
    bool status = false,
  }) {
    final io = HttpTaskCodec.snapshot(row.ioState);
    final stateValue = state(row.mutationState);
    final receipt = row.mutationCommandIdBigInt;
    bool same(Uint8List? a, Uint8List b) =>
        a != null &&
        a.length == b.length &&
        Iterable<int>.generate(b.length).every((i) => a[i] == b[i]);
    if (io.key == null ||
        (key != null && !same(io.key, key)) ||
        (submission != null && !same(io.submission, submission))) {
      throw const FormatException('Mutation task identity changed');
    }
    if (startedKind != null) {
      if (receipt != BigInt.one ||
          stateValue.commandId < BigInt.one ||
          (stateValue.commandId == BigInt.one &&
              stateValue.kind != startedKind) ||
          (startedKind == MutationCommandKind.reconcile &&
              (stateValue.commandId != BigInt.one ||
                  stateValue.kind != MutationCommandKind.reconcile))) {
        throw const FormatException('Invalid mutation start receipt');
      }
    } else if (status) {
      if (receipt != BigInt.zero) {
        throw const FormatException('Invalid mutation status receipt');
      }
    } else {
      MutationTaskValidation.commandId(receipt);
      if (commandId != null &&
          (receipt != commandId || stateValue.commandId != commandId)) {
        throw const FormatException('Mutation command identity changed');
      }
      if (submittedKind != null &&
          (stateValue.commandId < receipt ||
              (stateValue.commandId == receipt &&
                  stateValue.kind != submittedKind))) {
        throw const FormatException('Mutation submission identity changed');
      }
    }
    return MutationTaskReply(io: io, state: stateValue, commandId: receipt);
  }
}
