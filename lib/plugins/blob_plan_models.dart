import 'dart:convert';
import 'dart:typed_data';
import 'blob_upload_models.dart';
import 'io_task_models.dart';

Uint8List _owned(Uint8List bytes) =>
    Uint8List.fromList(bytes).asUnmodifiableView();

/// One immutable request to the original pure planner. It grants no upload.
final class BlobPlanRequest {
  BlobPlanRequest({
    required Uint8List submission,
    required this.packageId,
    required Uint8List packageDigest,
    required this.registryRevision,
    required this.handler,
    required this.inputType,
    required Uint8List input,
    this.timeoutMs = 120000,
  }) : submission = _owned(submission),
       packageDigest = _owned(packageDigest),
       input = _owned(input);
  static const outputType = 'morrow.blob.upload.proposal.v1';
  final Uint8List submission, packageDigest, input;
  final String packageId, handler, inputType;
  final BigInt registryRevision;
  final int timeoutMs;
}

final class BlobPlannerMapping {
  BlobPlannerMapping(this.handler, this.uploadHandler) {
    BlobUploadValidation.text(handler);
    BlobUploadValidation.text(uploadHandler);
    if (handler == uploadHandler)
      throw const FormatException('Planner cannot upload');
  }
  final String handler, uploadHandler;
}

enum BlobPlanError {
  cancelled,
  expired,
  staleSelection,
  gateClosed,
  protocol,
  execution,
  pluginFailure,
  preparation,
  spawn,
  worker,
}

final class BlobPlanView {
  BlobPlanView({
    required this.packageId,
    required Uint8List packageDigest,
    required this.registryRevision,
    required this.plannerHandler,
    required this.uploadHandler,
    required Uint8List planningInputSha256,
    required this.label,
    required this.byteLength,
    required Uint8List wholeSha256,
    required Uint8List proposalSha256,
  }) : packageDigest = _owned(packageDigest),
       planningInputSha256 = _owned(planningInputSha256),
       wholeSha256 = _owned(wholeSha256),
       proposalSha256 = _owned(proposalSha256);
  final String packageId, plannerHandler, uploadHandler;

  /// Guest text only. Never interpret it as markup or trusted attribution.
  final String label;
  final BigInt registryRevision, byteLength;
  final Uint8List packageDigest,
      planningInputSha256,
      wholeSha256,
      proposalSha256;
}

final class BlobPlanSnapshot {
  BlobPlanSnapshot({
    required Uint8List key,
    required Uint8List submission,
    required this.running,
    required this.originalWorkerJoined,
    required this.ready,
    required this.delivered,
    required this.consumed,
    required this.error,
    required Uint8List? uploadKey,
    required Uint8List? uploadSubmission,
    required this.submissionFailed,
  }) : key = _owned(key),
       submission = _owned(submission),
       uploadKey = uploadKey == null ? null : _owned(uploadKey),
       uploadSubmission = uploadSubmission == null
           ? null
           : _owned(uploadSubmission);
  final Uint8List key, submission;
  final bool running,
      originalWorkerJoined,
      ready,
      delivered,
      consumed,
      submissionFailed;
  final BlobPlanError? error;

  /// Original host-generated upload identity; usable only to observe that task.
  final Uint8List? uploadKey, uploadSubmission;
}

final class BlobPlanResult {
  const BlobPlanResult({
    required this.view,
    required this.error,
    required this.hasExecution,
    required this.executionFault,
    required this.exitCode,
    required this.hostCalls,
    required this.pluginFailureCode,
  });
  final BlobPlanView? view;
  final BlobPlanError? error;
  final bool hasExecution;
  final IoExecutionFault executionFault;
  final int exitCode, hostCalls;
  final int? pluginFailureCode;
}

final class BlobPlanReply {
  const BlobPlanReply(this.snapshot, this.result);
  final BlobPlanSnapshot? snapshot;
  final BlobPlanResult? result;
}

abstract interface class BlobPlanBackend {
  Future<BlobPlanSnapshot> startBlobPlan(BlobPlanRequest request);
  Future<BlobPlanSnapshot> statusBlobPlan(Uint8List key);
  Future<BlobPlanReply> readBlobPlan(Uint8List key);
  Future<BlobPlanSnapshot> cancelBlobPlan(Uint8List key);
  Future<void> acknowledgeBlobPlan(Uint8List key);
  Future<BlobPlanSnapshot> approveBlobPlan(
    Uint8List key,
    BlobPlanView reviewed,
  );
  Future<BlobPlanSnapshot?> discoverBlobPlan(Uint8List submission);
}

abstract final class BlobPlanValidation {
  static void request(BlobPlanRequest value) {
    BlobUploadValidation.selection(
      value.packageId,
      value.packageDigest,
      value.registryRevision,
    );
    HttpTaskValidation.identity(value.submission);
    BlobUploadValidation.text(value.handler);
    BlobUploadValidation.text(value.inputType);
    if (value.input.length > 32768 ||
        value.timeoutMs < 1 ||
        value.timeoutMs > 120000) {
      throw const FormatException('Planning request exceeds budget');
    }
  }

  static void view(BlobPlanView value) {
    BlobUploadValidation.selection(
      value.packageId,
      value.packageDigest,
      value.registryRevision,
    );
    BlobUploadValidation.text(value.plannerHandler);
    BlobUploadValidation.text(value.uploadHandler);
    for (final hash in [
      value.planningInputSha256,
      value.wholeSha256,
      value.proposalSha256,
    ]) {
      if (hash.length != 32)
        throw const FormatException('Invalid proposal digest');
    }
    if (value.plannerHandler == value.uploadHandler ||
        value.byteLength < BigInt.zero ||
        value.byteLength > BigInt.from(16777216) ||
        value.label.isEmpty ||
        value.label.trim() != value.label ||
        utf8.encode(value.label).length > 512 ||
        value.label.runes.length > 128 ||
        value.label.runes.any(
          (c) =>
              c < 32 ||
              (c >= 127 && c <= 159) ||
              (c >= 0x202a && c <= 0x202e) ||
              (c >= 0x2066 && c <= 0x2069),
        )) {
      throw const FormatException('Invalid original proposal view');
    }
  }
}
