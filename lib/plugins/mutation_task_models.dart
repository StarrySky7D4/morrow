import 'dart:convert';
import 'dart:typed_data';
import 'io_task_models.dart';

Uint8List _copy(Uint8List v) => Uint8List.fromList(v).asUnmodifiableView();
Uint8List _checkpoint(Uint8List v) {
  if (v.length != 32 || v.every((byte) => byte == 0)) {
    throw const FormatException('Invalid mutation checkpoint');
  }
  return _copy(v);
}
List<Uint8List>? _copyPlans(List<Uint8List>? v) =>
    v == null ? null : List<Uint8List>.unmodifiable(v.map(_copy));

enum MutationDisposition { create, replace, delete }

enum MutationCommandKind {
  select,
  prepare,
  chunk,
  commitContent,
  execute,
  query,
  cancelPlan,
  release,
  reconcile,
  buildPlan,
  discover,
}

enum MutationDelivery { pending, ready, consumed }

enum MutationResultKind {
  pending,
  selected,
  prepared,
  staged,
  created,
  deleted,
  history,
  released,
  planCancelled,
  failure,
  reconciled,
  planned,
  plans,
}

enum MutationPhase {
  none,
  prepared,
  outcomeUnknown,
  observed,
  cancelledBeforeDispatch,
}

enum MutationEffect { unspecified, osSucceeded, osRejected }

enum MutationFailureLayer { delivery, target }

enum MutationDeliveryFailure {
  busy,
  closed,
  limit,
  cancelled,
  unknown,
  consumed,
}

enum MutationTargetFailure {
  admission,
  committedButDeliveryDenied,
  restartRequired,
  busy,
  outcomeUnknown,
  alreadyDispatched,
  unsupportedConditionalReplacement,
  cancelledBeforeDispatch,
  committedButDeliveryCancelled,
  invalidSelection,
  missing,
  mismatch,
  changed,
  io,
  limit,
  persistence,
  commitUnknown,
  revisionConflict,
}

/// A fresh user-authorized selected target. Never replay a lost acknowledgement.
final class MutationStartRequest {
  MutationStartRequest({
    required Uint8List submission,
    required this.packageId,
    required Uint8List packageDigest,
    required this.registryRevision,
    required this.disposition,
    required this.selectedPath,
    required this.relativePath,
    required this.subject,
    required Uint8List approvalSha256,
    required this.timeoutMs,
  }) : submission = _copy(submission),
       packageDigest = _copy(packageDigest),
       approvalSha256 = _copy(approvalSha256);
  final Uint8List submission, packageDigest, approvalSha256;
  final String packageId, selectedPath, relativePath, subject;
  final BigInt registryRevision;
  final MutationDisposition disposition;
  final int timeoutMs;
}

/// Read-only fresh admission after a prior task is joined and acknowledged.
final class MutationReconcileRequest {
  MutationReconcileRequest({
    required Uint8List submission,
    required this.packageId,
    required Uint8List packageDigest,
    required this.registryRevision,
    required Uint8List plan,
    required this.timeoutMs,
  }) : submission = _copy(submission),
       packageDigest = _copy(packageDigest),
       plan = _copy(plan);
  final Uint8List submission, packageDigest, plan;
  final String packageId;
  final BigInt registryRevision;
  final int timeoutMs;
}

/// Read-only discovery of historical request plans for this package and scope.
final class MutationDiscoverRequest {
  MutationDiscoverRequest({
    required Uint8List submission,
    required this.packageId,
    required Uint8List packageDigest,
    required this.registryRevision,
    required this.subject,
    required this.disposition,
    required this.scanLimit,
    required this.timeoutMs,
    Uint8List? checkpoint,
  }) : submission = _copy(submission),
       packageDigest = _copy(packageDigest),
       checkpoint = checkpoint == null ? null : _checkpoint(checkpoint);
  final Uint8List submission, packageDigest;

  /// Opaque continuation for a new, explicitly authorized discovery session.
  /// Bound to the issuing host session; it is not durable or a permission.
  final Uint8List? checkpoint;
  final String packageId, subject;
  final BigInt registryRevision;
  final MutationDisposition disposition;
  final int scanLimit, timeoutMs;
}

final class MutationState {
  MutationState({
    required this.commandId,
    required this.kind,
    required this.delivery,
    required this.selected,
    required this.reconcileRequired,
    required this.terminal,
    Uint8List? reference,
    Uint8List? expectedIdentity,
  }) : reference = reference == null ? null : _copy(reference),
       expectedIdentity = expectedIdentity == null
           ? null
           : _copy(expectedIdentity);
  final BigInt commandId;
  final MutationCommandKind kind;
  final MutationDelivery delivery;
  final bool selected, reconcileRequired, terminal;

  /// Only valid in the original live selection; never a renewed permission.
  final Uint8List? reference, expectedIdentity;
}

final class MutationFailure {
  const MutationFailure({required this.layer, this.delivery, this.target});
  final MutationFailureLayer layer;
  final MutationDeliveryFailure? delivery;
  final MutationTargetFailure? target;
}

/// Core record/outcome containers are bounded opaque bytes. Observed alone does
/// not establish OS success; inspect effect and any outcome separately.
final class MutationResult {
  MutationResult({
    required this.kind,
    Uint8List? reference,
    Uint8List? expectedIdentity,
    Uint8List? record,
    Uint8List? outcome,
    Uint8List? plan,
    List<Uint8List>? plans,
    Uint8List? checkpoint,
    required this.stagedBytes,
    required this.durableContent,
    required this.phase,
    required this.operationId,
    required this.effect,
    required this.osCode,
    required this.scanned,
    required this.done,
    this.failure,
  }) : reference = reference == null ? null : _copy(reference),
       expectedIdentity = expectedIdentity == null
           ? null
           : _copy(expectedIdentity),
       record = record == null ? null : _copy(record),
       outcome = outcome == null ? null : _copy(outcome),
       plan = plan == null ? null : _copy(plan),
       plans = _copyPlans(plans),
       checkpoint = checkpoint == null ? null : _checkpoint(checkpoint);
  final MutationResultKind kind;
  final Uint8List? reference, expectedIdentity, record, outcome, plan;

  /// Historical request plans only; this does not report mutation completion.
  final List<Uint8List>? plans;

  /// Only a nonterminal Plans result may carry this opaque host cursor.
  final Uint8List? checkpoint;
  final int scanned;
  final bool done;
  final BigInt stagedBytes;
  final bool durableContent;
  final MutationPhase phase;
  final String? operationId;
  final MutationEffect effect;

  /// Only present for osRejected; Core outcomes require a nonzero OS code.
  final int? osCode;
  final MutationFailure? failure;
}

final class MutationTaskReply {
  const MutationTaskReply({
    required this.io,
    required this.state,
    required this.commandId,
  });
  final IoTaskSnapshot io;
  final MutationState state;

  /// A deduplicated submission may identify an older command than state.
  final BigInt commandId;
}

final class MutationTaskRead {
  const MutationTaskRead({required this.reply, required this.result});
  final MutationTaskReply reply;
  final MutationResult? result; // Null while pending.
}

abstract interface class MutationTaskBackend {
  Future<MutationTaskReply> startSelected(MutationStartRequest request);
  Future<MutationTaskReply> startReconciliation(
    MutationReconcileRequest request,
  );
  Future<MutationTaskReply> startDiscovery(MutationDiscoverRequest request);
  Future<MutationTaskReply> submitPrepare(
    Uint8List key,
    Uint8List submission,
    Uint8List plan,
  );
  Future<MutationTaskReply> submitBuildPlan(
    Uint8List key,
    Uint8List submission, {
    required String operationId,
    required BigInt contentLength,
    Uint8List? contentSha256,
  });
  Future<MutationTaskReply> submitNextPlans(
    Uint8List key,
    Uint8List submission,
    int scanLimit,
  );
  Future<MutationTaskReply> submitChunk(
    Uint8List key,
    Uint8List submission,
    BigInt offset,
    Uint8List bytes,
  );
  Future<MutationTaskReply> submitCommitContent(
    Uint8List key,
    Uint8List submission,
  );
  Future<MutationTaskReply> submitExecute(Uint8List key, Uint8List submission);
  Future<MutationTaskReply> submitQuery(Uint8List key, Uint8List submission);
  Future<MutationTaskReply> submitCancelPlan(
    Uint8List key,
    Uint8List submission,
  );
  Future<MutationTaskReply> submitRelease(Uint8List key, Uint8List submission);
  Future<MutationTaskReply> status(Uint8List key);
  Future<MutationTaskRead> read(Uint8List key, BigInt commandId);
  Future<MutationTaskReply> cancelCommand(Uint8List key, BigInt commandId);
}

/// Poll, stop, repair and acknowledge remain on WorkbenchIoTaskControl.
abstract interface class MutationTaskSupport {
  MutationTaskBackend get mutationTasks;
  bool get supportsMutationTasks;
}

abstract final class MutationTaskValidation {
  static final maxUint64 = (BigInt.one << 64) - BigInt.one;
  static const maxChunkBytes = 60 * 1024;
  static const maxPlanBytes = 8192 + 8192 ~/ 255 + 128;
  static const maxRecordBytes = 4096 + 4096 ~/ 255 + 128;
  static const maxOutcomeBytes = 1024 + 1024 ~/ 255 + 128;
  static const maxContentBytes = 16 * 1024 * 1024;
  static const maxPlanScanLimit = 8;
  static const _emptySha256 = [
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
  ];
  static Uint8List identity(Uint8List v) => HttpTaskValidation.identity(v);
  static void revision(BigInt v) {
    if (v < BigInt.zero || v > maxUint64) {
      throw const FormatException('Invalid mutation revision');
    }
  }

  static void commandId(BigInt v) {
    if (v < BigInt.one || v > maxUint64) {
      throw const FormatException('Invalid mutation command id');
    }
  }

  static void timeout(int v) {
    if (v < 1 || v > 30000) {
      throw const FormatException('Invalid mutation timeout');
    }
  }

  static void package(String v) {
    if (v.isEmpty || v.contains('\u0000') || utf8.encode(v).length > 256) {
      throw const FormatException('Invalid mutation package');
    }
  }

  static void plan(Uint8List v) {
    if (v.isEmpty || v.length > maxPlanBytes) {
      throw const FormatException('Invalid mutation plan');
    }
  }

  static void buildPlan(
    String operationId,
    BigInt contentLength,
    Uint8List? contentSha256,
  ) {
    final idLength = utf8.encode(operationId).length;
    if (idLength < 1 ||
        idLength > 256 ||
        operationId.runes.any(
          (r) =>
              r < 32 ||
              (r >= 127 && r <= 159) ||
              r == 0x2f ||
              r == 0x5c ||
              r == 0x3a,
        )) {
      throw const FormatException('Invalid mutation operation identity');
    }
    if (contentLength < BigInt.zero ||
        contentLength > BigInt.from(maxContentBytes)) {
      throw const FormatException('Invalid mutation content length');
    }
    if (contentSha256 == null) {
      if (contentLength != BigInt.zero) {
        throw const FormatException('Missing mutation content digest');
      }
      return;
    }
    if (contentSha256.length != 32 ||
        contentSha256.every((byte) => byte == 0) ||
        (contentLength == BigInt.zero &&
            !Iterable<int>.generate(
              32,
            ).every((index) => contentSha256[index] == _emptySha256[index]))) {
      throw const FormatException('Invalid mutation content digest');
    }
  }

  static void validateStart(MutationStartRequest v) {
    identity(v.submission);
    identity(v.packageDigest);
    identity(v.approvalSha256);
    package(v.packageId);
    revision(v.registryRevision);
    timeout(v.timeoutMs);
    if (v.subject.isEmpty ||
        v.subject.contains('\u0000') ||
        utf8.encode(v.subject).length > 256 ||
        v.selectedPath.isEmpty ||
        v.selectedPath.contains('\u0000') ||
        utf8.encode(v.selectedPath).length > 4096 ||
        !RegExp(
          r'^(?:[A-Za-z]:[\\/]|\\\\[^\\]+\\[^\\]+\\|/)',
        ).hasMatch(v.selectedPath) ||
        (v.disposition == MutationDisposition.create) !=
            v.relativePath.isNotEmpty) {
      throw const FormatException('Invalid mutation selection');
    }
    if (v.relativePath.isNotEmpty) relativePath(v.relativePath);
  }

  static void validateReconciliation(MutationReconcileRequest v) {
    identity(v.submission);
    identity(v.packageDigest);
    package(v.packageId);
    revision(v.registryRevision);
    timeout(v.timeoutMs);
    plan(v.plan);
  }

  static void scanLimit(int v) {
    if (v < 1 || v > maxPlanScanLimit) {
      throw const FormatException('Invalid mutation plan scan limit');
    }
  }

  static void subject(String v) {
    if (v.isEmpty ||
        utf8.encode(v).length > 256 ||
        v.runes.any(
          (r) =>
              r < 32 ||
              (r >= 127 && r <= 159) ||
              r == 0x2f ||
              r == 0x5c ||
              r == 0x3a,
        )) {
      throw const FormatException('Invalid mutation discovery subject');
    }
  }

  static void validateDiscovery(MutationDiscoverRequest v) {
    identity(v.submission);
    identity(v.packageDigest);
    package(v.packageId);
    revision(v.registryRevision);
    subject(v.subject);
    scanLimit(v.scanLimit);
    timeout(v.timeoutMs);
    if (v.checkpoint != null) identity(v.checkpoint!);
  }

  static void relativePath(String v) {
    final parts = v.split('/');
    const devices = {
      'CON',
      'PRN',
      'AUX',
      'NUL',
      r'CONIN$',
      r'CONOUT$',
      r'CLOCK$',
      'COM1',
      'COM2',
      'COM3',
      'COM4',
      'COM5',
      'COM6',
      'COM7',
      'COM8',
      'COM9',
      'LPT1',
      'LPT2',
      'LPT3',
      'LPT4',
      'LPT5',
      'LPT6',
      'LPT7',
      'LPT8',
      'LPT9',
      'COM¹',
      'COM²',
      'COM³',
      'LPT¹',
      'LPT²',
      'LPT³',
    };
    if (utf8.encode(v).length > 4096 ||
        parts.length > 128 ||
        parts.any((p) {
          final stem = p.split('.').first.trimRight().toUpperCase();
          return p.isEmpty ||
              p == '.' ||
              p == '..' ||
              p.endsWith(' ') ||
              p.endsWith('.') ||
              p.runes.any((r) => r < 32 || r == 127) ||
              RegExp(r'[\\:<>"|?*]').hasMatch(p) ||
              p.codeUnits.length > 255 ||
              devices.contains(stem);
        })) {
      throw const FormatException('Invalid mutation relative path');
    }
  }

  static void chunk(BigInt offset, Uint8List bytes) {
    if (offset < BigInt.zero ||
        offset > maxUint64 ||
        bytes.isEmpty ||
        bytes.length > maxChunkBytes ||
        offset + BigInt.from(bytes.length) > maxUint64) {
      throw const FormatException('Invalid mutation chunk');
    }
  }
}
