import 'dart:typed_data';

import 'io_task_models.dart';
import 'mutation_task_models.dart';

Uint8List _owned(Uint8List bytes) =>
    Uint8List.fromList(bytes).asUnmodifiableView();

/// Host approval, distinct from a package's published budget ceiling.
final class ApprovedGuestBudget {
  const ApprovedGuestBudget({
    required this.maxJobBytes,
    required this.maxBytes,
  });
  final BigInt maxJobBytes, maxBytes;
}

final class GuestMutationStartRequest {
  const GuestMutationStartRequest({
    required this.selection,
    this.approvedBudget,
  });
  final MutationStartRequest selection;
  final ApprovedGuestBudget? approvedBudget;
}

enum GuestMutationCommandKind {
  select,
  buildPlan,
  prepare,
  chunk,
  commitContent,
  execute,
  query,
  cancelPlan,
  release,
  hostQuery,
  hostCancelPlan,
  hostRelease,
}

enum GuestMutationDelivery { pending, ready, consumed, unavailable }

enum GuestMutationResultKind { pending, owner, frame, failure }

enum GuestMutationFailureKind { job, execution, protocol, cancelled }

enum GuestMutationJobError {
  invalidOptions,
  busy,
  closed,
  unavailable,
  consumed,
  readBound,
  limit,
  spawn,
  disconnect,
}

enum GuestMutationFault {
  taskProtocol,
  deadline,
  packageBinding,
  inactiveConnection,
  invalidModule,
  unsupportedAbi,
  limits,
  cancelled,
  trap,
}

enum GuestMutationFrameKind {
  prepareCreate,
  prepareDelete,
  chunk,
  commit,
  execute,
  query,
  cancelPlan,
  release,
}

enum GuestMutationFrameStatus {
  completed,
  denied,
  revoked,
  expired,
  unsupported,
  quota,
  notFound,
  conflict,
  cancelled,
  outcomeUnknown,
  failed,
}

enum GuestMutationFramePhase {
  none,
  absent,
  prepared,
  outcomeUnknown,
  observed,
  cancelledBeforeDispatch,
}

enum GuestMutationFrameEffect { unspecified, osSucceeded, osRejected }

/// A typed command. Operation identity and guest reference are local expected
/// values for the independent Core reply; only BuildPlan sends operationId to
/// the private host request. They are never permission or a renewed lease.
final class GuestMutationCommand {
  GuestMutationCommand._(
    this.kind, {
    Uint8List? planSha256,
    this.offset,
    Uint8List? bytes,
    this.operationId,
    this.disposition,
    this.contentLength,
    Uint8List? contentSha256,
    Uint8List? expectedGuestReference,
  }) : planSha256 = planSha256 == null ? null : _owned(planSha256),
       bytes = bytes == null ? null : _owned(bytes),
       contentSha256 = contentSha256 == null ? null : _owned(contentSha256),
       expectedGuestReference = expectedGuestReference == null
           ? null
           : _owned(expectedGuestReference);

  factory GuestMutationCommand.buildPlan({
    required String operationId,
    required BigInt contentLength,
    Uint8List? contentSha256,
  }) => GuestMutationCommand._(
    GuestMutationCommandKind.buildPlan,
    operationId: operationId,
    contentLength: contentLength,
    contentSha256: contentSha256,
  );
  factory GuestMutationCommand.prepare({
    required Uint8List planSha256,
    required String operationId,
    required MutationDisposition disposition,
  }) => GuestMutationCommand._(
    GuestMutationCommandKind.prepare,
    planSha256: planSha256,
    operationId: operationId,
    disposition: disposition,
  );
  factory GuestMutationCommand.chunk({
    required BigInt offset,
    required Uint8List bytes,
    required String operationId,
    required Uint8List guestReference,
  }) => GuestMutationCommand._(
    GuestMutationCommandKind.chunk,
    offset: offset,
    bytes: bytes,
    operationId: operationId,
    expectedGuestReference: guestReference,
  );
  factory GuestMutationCommand.commitContent({
    required String operationId,
    required Uint8List guestReference,
  }) => GuestMutationCommand._(
    GuestMutationCommandKind.commitContent,
    operationId: operationId,
    expectedGuestReference: guestReference,
  );
  factory GuestMutationCommand.execute({
    required Uint8List planSha256,
    required String operationId,
    required Uint8List guestReference,
  }) => GuestMutationCommand._(
    GuestMutationCommandKind.execute,
    planSha256: planSha256,
    operationId: operationId,
    expectedGuestReference: guestReference,
  );
  factory GuestMutationCommand.query({
    required String operationId,
    required Uint8List guestReference,
  }) => GuestMutationCommand._(
    GuestMutationCommandKind.query,
    operationId: operationId,
    expectedGuestReference: guestReference,
  );
  factory GuestMutationCommand.cancelPlan({
    required String operationId,
    required Uint8List guestReference,
  }) => GuestMutationCommand._(
    GuestMutationCommandKind.cancelPlan,
    operationId: operationId,
    expectedGuestReference: guestReference,
  );
  factory GuestMutationCommand.release({
    required String operationId,
    required Uint8List guestReference,
  }) => GuestMutationCommand._(
    GuestMutationCommandKind.release,
    operationId: operationId,
    expectedGuestReference: guestReference,
  );
  factory GuestMutationCommand.hostQuery() =>
      GuestMutationCommand._(GuestMutationCommandKind.hostQuery);
  factory GuestMutationCommand.hostCancelPlan() =>
      GuestMutationCommand._(GuestMutationCommandKind.hostCancelPlan);
  factory GuestMutationCommand.hostRelease() =>
      GuestMutationCommand._(GuestMutationCommandKind.hostRelease);

  final GuestMutationCommandKind kind;
  final Uint8List? planSha256, bytes, contentSha256, expectedGuestReference;
  final BigInt? offset, contentLength;
  final String? operationId;
  final MutationDisposition? disposition;
}

final class GuestMutationState {
  GuestMutationState({
    required this.commandId,
    required this.kind,
    required this.delivery,
    required this.selected,
    Uint8List? reference,
    Uint8List? expectedIdentity,
    Uint8List? reviewedPlanSha256,
    required this.approvalDelivered,
    required this.permitDelivered,
    required this.stagedBytes,
    required this.durableContent,
    required this.effectAttempted,
    required this.reconcileRequired,
    required this.terminal,
  }) : reference = reference == null ? null : _owned(reference),
       expectedIdentity = expectedIdentity == null
           ? null
           : _owned(expectedIdentity),
       reviewedPlanSha256 = reviewedPlanSha256 == null
           ? null
           : _owned(reviewedPlanSha256);
  final BigInt commandId, stagedBytes;
  final GuestMutationCommandKind kind;
  final GuestMutationDelivery delivery;
  final bool selected, approvalDelivered, permitDelivered, durableContent;
  final bool effectAttempted, reconcileRequired, terminal;

  /// Original selected target, distinct from Core's guest lease reference.
  final Uint8List? reference, expectedIdentity, reviewedPlanSha256;
}

/// Explicit evidence retained by the caller. Status alone cannot reconstruct
/// the original command, and retry is always a separate explicit submit.
final class GuestMutationReceipt {
  GuestMutationReceipt({
    required Uint8List key,
    required Uint8List submission,
    required this.commandId,
    required this.kind,
    this.operationId,
    this.disposition,
    Uint8List? expectedGuestReference,
    this.expectedChunkEnd,
  }) : key = _owned(key),
       submission = _owned(submission),
       expectedGuestReference = expectedGuestReference == null
           ? null
           : _owned(expectedGuestReference);
  final Uint8List key, submission;
  final BigInt commandId;
  final GuestMutationCommandKind kind;
  final String? operationId;
  final MutationDisposition? disposition;
  final Uint8List? expectedGuestReference;
  final BigInt? expectedChunkEnd;
}

final class GuestMutationTaskReply {
  const GuestMutationTaskReply({
    required this.io,
    required this.state,
    required this.commandId,
    this.receipt,
  });
  final IoTaskSnapshot io;
  final GuestMutationState state;
  final BigInt commandId;
  final GuestMutationReceipt? receipt;
}

final class GuestMutationFrame {
  GuestMutationFrame({
    required this.callId,
    required Uint8List reference,
    required Uint8List submission,
    required this.operationId,
    required this.kind,
    required this.status,
    required this.phase,
    required this.effect,
    required this.stagedBytes,
    required this.durableContent,
    required Uint8List encoded,
  }) : reference = _owned(reference),
       submission = _owned(submission),
       encoded = _owned(encoded);
  final BigInt callId, stagedBytes;
  final Uint8List reference, submission, encoded;
  final String operationId;
  final GuestMutationFrameKind kind;
  final GuestMutationFrameStatus status;
  final GuestMutationFramePhase phase;
  final GuestMutationFrameEffect effect;
  final bool durableContent;
}

final class GuestMutationFailure {
  const GuestMutationFailure({required this.kind, this.job, this.fault});
  final GuestMutationFailureKind kind;
  final GuestMutationJobError? job;
  final GuestMutationFault? fault;
}

final class GuestMutationResult {
  const GuestMutationResult({
    required this.kind,
    this.owner,
    this.frame,
    this.failure,
  });
  final GuestMutationResultKind kind;
  final MutationResult? owner;
  final GuestMutationFrame? frame;
  final GuestMutationFailure? failure;
}

final class GuestMutationTaskRead {
  const GuestMutationTaskRead({required this.reply, required this.result});
  final GuestMutationTaskReply reply;
  final GuestMutationResult? result;
}

abstract interface class GuestMutationBackend {
  Future<GuestMutationTaskReply> start(GuestMutationStartRequest request);
  Future<GuestMutationTaskReply> submit(
    Uint8List key,
    Uint8List submission,
    GuestMutationCommand command,
  );
  Future<GuestMutationTaskReply> status(Uint8List key);
  Future<GuestMutationTaskRead> read(GuestMutationReceipt receipt);
  Future<GuestMutationTaskReply> cancelCommand(Uint8List key, BigInt commandId);
}

abstract interface class GuestMutationSupport {
  bool get supportsGuestMutationTasks;
  GuestMutationBackend get guestMutationTasks;
}

abstract final class GuestMutationValidation {
  static final maxUint64 = (BigInt.one << 64) - BigInt.one;
  static const maxFrameBytes = 131072;
  static const maxChunkBytes = 61440;
  static const maxContentBytes = 16777216;
  static const maxApprovedJobBytes = 32 * 1024 * 1024;
  static const maxApprovedBytes = 256 * 1024 * 1024;

  static Uint8List identity(Uint8List bytes) =>
      MutationTaskValidation.identity(bytes);
  static void u64(BigInt value, {bool positive = false}) {
    if (value < (positive ? BigInt.one : BigInt.zero) || value > maxUint64) {
      throw const FormatException('Invalid guest mutation UInt64');
    }
  }

  static void budget(ApprovedGuestBudget? value) {
    if (value == null) return;
    u64(value.maxJobBytes, positive: true);
    u64(value.maxBytes, positive: true);
    if (value.maxJobBytes > BigInt.from(maxApprovedJobBytes) ||
        value.maxBytes > BigInt.from(maxApprovedBytes) ||
        value.maxBytes < value.maxJobBytes) {
      throw const FormatException('Invalid approved guest budget');
    }
  }

  static void start(GuestMutationStartRequest value) {
    MutationTaskValidation.validateStart(value.selection);
    if (value.selection.disposition == MutationDisposition.replace ||
        value.selection.subject.runes.any(
          (r) =>
              r < 32 ||
              (r >= 127 && r <= 159) ||
              r == 0x2f ||
              r == 0x5c ||
              r == 0x3a,
        )) {
      throw const FormatException('Invalid guest mutation scope');
    }
    budget(value.approvedBudget);
  }

  static void operation(String? value) {
    if (value == null) {
      throw const FormatException('Missing guest operation identity');
    }
    MutationTaskValidation.buildPlan(value, BigInt.zero, null);
  }

  static void command(GuestMutationCommand value) {
    if (value.kind == GuestMutationCommandKind.select) {
      throw const FormatException('Select is a start receipt, not a command');
    }
    final frame =
        value.kind.index >= GuestMutationCommandKind.prepare.index &&
        value.kind.index <= GuestMutationCommandKind.release.index;
    if (value.kind == GuestMutationCommandKind.buildPlan) {
      if (value.operationId == null || value.contentLength == null) {
        throw const FormatException('Missing guest plan specification');
      }
      MutationTaskValidation.buildPlan(
        value.operationId!,
        value.contentLength!,
        value.contentSha256,
      );
    } else if (frame) {
      operation(value.operationId);
      if (value.kind == GuestMutationCommandKind.prepare) {
        if (value.planSha256 == null ||
            value.disposition == null ||
            value.disposition == MutationDisposition.replace) {
          throw const FormatException('Missing guest Prepare identity');
        }
        identity(value.planSha256!);
        if (value.expectedGuestReference != null) {
          throw const FormatException('Premature guest reference');
        }
      } else {
        if (value.expectedGuestReference == null) {
          throw const FormatException('Missing guest reference');
        }
        identity(value.expectedGuestReference!);
        if (value.kind == GuestMutationCommandKind.execute) {
          if (value.planSha256 == null) {
            throw const FormatException('Missing reviewed guest plan');
          }
          identity(value.planSha256!);
        }
      }
      if (value.kind == GuestMutationCommandKind.chunk) {
        if (value.offset == null || value.bytes == null) {
          throw const FormatException('Missing guest chunk');
        }
        MutationTaskValidation.chunk(value.offset!, value.bytes!);
        if (value.bytes!.length > maxChunkBytes ||
            value.offset! + BigInt.from(value.bytes!.length) >
                BigInt.from(maxContentBytes)) {
          throw const FormatException('Invalid guest chunk');
        }
      }
    }
  }

  static GuestMutationFrameKind frameKind(
    GuestMutationCommandKind kind,
    MutationDisposition? disposition,
  ) => switch (kind) {
    GuestMutationCommandKind.prepare
        when disposition == MutationDisposition.create =>
      GuestMutationFrameKind.prepareCreate,
    GuestMutationCommandKind.prepare
        when disposition == MutationDisposition.delete =>
      GuestMutationFrameKind.prepareDelete,
    GuestMutationCommandKind.chunk => GuestMutationFrameKind.chunk,
    GuestMutationCommandKind.commitContent => GuestMutationFrameKind.commit,
    GuestMutationCommandKind.execute => GuestMutationFrameKind.execute,
    GuestMutationCommandKind.query => GuestMutationFrameKind.query,
    GuestMutationCommandKind.cancelPlan => GuestMutationFrameKind.cancelPlan,
    GuestMutationCommandKind.release => GuestMutationFrameKind.release,
    _ => throw const FormatException('Guest command has no Core frame'),
  };

  static bool same(Uint8List a, Uint8List b) =>
      a.length == b.length &&
      Iterable<int>.generate(a.length).every((i) => a[i] == b[i]);

  static void receipt(GuestMutationReceipt value) {
    identity(value.key);
    identity(value.submission);
    u64(value.commandId, positive: true);
    final frame =
        value.kind.index >= GuestMutationCommandKind.prepare.index &&
        value.kind.index <= GuestMutationCommandKind.release.index;
    if ((frame || value.kind == GuestMutationCommandKind.buildPlan) &&
        value.operationId == null) {
      throw const FormatException('Missing guest operation identity');
    }
    if (value.operationId != null) {
      operation(value.operationId);
    }
    if (value.kind == GuestMutationCommandKind.prepare) {
      if (value.disposition == null ||
          value.disposition == MutationDisposition.replace ||
          value.expectedGuestReference != null) {
        throw const FormatException('Invalid guest Prepare receipt');
      }
    } else {
      if (value.disposition != null ||
          (frame && value.expectedGuestReference == null) ||
          (!frame && value.expectedGuestReference != null)) {
        throw const FormatException('Invalid guest lease receipt');
      }
    }
    if (value.expectedGuestReference != null) {
      identity(value.expectedGuestReference!);
    }
    if ((value.kind == GuestMutationCommandKind.chunk) !=
        (value.expectedChunkEnd != null)) {
      throw const FormatException('Invalid guest chunk receipt');
    }
    if (value.expectedChunkEnd != null) {
      u64(value.expectedChunkEnd!);
      if (value.expectedChunkEnd! > BigInt.from(maxContentBytes)) {
        throw const FormatException('Invalid guest chunk end');
      }
    }
  }

  static void frame(
    GuestMutationFrame value,
    GuestMutationReceipt expected, {
    MutationDisposition? disposition,
  }) {
    receipt(expected);
    u64(value.callId, positive: true);
    u64(value.stagedBytes);
    identity(value.reference);
    identity(value.submission);
    if (value.encoded.isEmpty ||
        value.encoded.length > maxFrameBytes ||
        value.callId != expected.commandId ||
        !same(value.submission, expected.submission) ||
        value.operationId != expected.operationId ||
        (expected.expectedGuestReference != null &&
            !same(value.reference, expected.expectedGuestReference!))) {
      throw const FormatException('Guest Core frame identity changed');
    }
    if (value.kind != frameKind(expected.kind, expected.disposition)) {
      throw const FormatException('Guest Core frame kind changed');
    }
    if (value.stagedBytes > BigInt.from(maxContentBytes)) {
      throw const FormatException('Guest staged bytes exceed content cap');
    }
    final completed = value.status == GuestMutationFrameStatus.completed;
    final unknown = value.status == GuestMutationFrameStatus.outcomeUnknown;
    if (unknown) {
      if (value.kind != GuestMutationFrameKind.execute &&
              value.kind != GuestMutationFrameKind.query ||
          value.phase != GuestMutationFramePhase.outcomeUnknown ||
          value.effect != GuestMutationFrameEffect.unspecified ||
          value.stagedBytes != BigInt.zero ||
          value.durableContent) {
        throw const FormatException('Contradictory guest Unknown');
      }
      return;
    }
    if (!completed) {
      if (value.phase != GuestMutationFramePhase.none ||
          value.effect != GuestMutationFrameEffect.unspecified ||
          value.stagedBytes != BigInt.zero ||
          value.durableContent) {
        throw const FormatException('Contradictory guest failure');
      }
      return;
    }
    switch (value.kind) {
      case GuestMutationFrameKind.prepareCreate:
      case GuestMutationFrameKind.prepareDelete:
        if (value.phase != GuestMutationFramePhase.prepared ||
            value.effect != GuestMutationFrameEffect.unspecified ||
            value.stagedBytes != BigInt.zero ||
            value.durableContent) {
          throw const FormatException('Contradictory guest Prepare');
        }
      case GuestMutationFrameKind.chunk:
        if (value.phase != GuestMutationFramePhase.prepared ||
            value.effect != GuestMutationFrameEffect.unspecified ||
            value.stagedBytes != expected.expectedChunkEnd ||
            value.durableContent) {
          throw const FormatException('Contradictory guest Chunk');
        }
      case GuestMutationFrameKind.commit:
        if (value.phase != GuestMutationFramePhase.prepared ||
            value.effect != GuestMutationFrameEffect.unspecified ||
            !value.durableContent) {
          throw const FormatException('Contradictory guest Commit');
        }
      case GuestMutationFrameKind.execute:
        if (value.phase != GuestMutationFramePhase.observed ||
            value.effect == GuestMutationFrameEffect.unspecified) {
          throw const FormatException('Contradictory guest Execute');
        }
      case GuestMutationFrameKind.query:
        if (value.phase == GuestMutationFramePhase.none ||
            (value.phase == GuestMutationFramePhase.observed) !=
                (value.effect != GuestMutationFrameEffect.unspecified) ||
            (value.phase == GuestMutationFramePhase.absent ||
                    value.phase ==
                        GuestMutationFramePhase.cancelledBeforeDispatch) &&
                (value.stagedBytes != BigInt.zero || value.durableContent)) {
          throw const FormatException('Contradictory guest Query');
        }
      case GuestMutationFrameKind.cancelPlan:
        if (value.phase != GuestMutationFramePhase.cancelledBeforeDispatch ||
            value.effect != GuestMutationFrameEffect.unspecified ||
            value.stagedBytes != BigInt.zero ||
            value.durableContent) {
          throw const FormatException('Contradictory guest Cancel');
        }
      case GuestMutationFrameKind.release:
        if (value.phase != GuestMutationFramePhase.none ||
            value.effect != GuestMutationFrameEffect.unspecified ||
            value.stagedBytes != BigInt.zero ||
            value.durableContent) {
          throw const FormatException('Contradictory guest Release');
        }
    }
  }
}
