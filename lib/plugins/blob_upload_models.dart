import 'dart:convert';
import 'dart:typed_data';
import 'io_task_models.dart';

Uint8List _owned(Uint8List bytes) =>
    Uint8List.fromList(bytes).asUnmodifiableView();

/// Exact input captured for one explicit approval. Identifiers are internal
/// correlation data, never a bearer grant or a reason to replay an attempt.
final class BlobUploadRequest {
  BlobUploadRequest({
    required Uint8List submission,
    required this.packageId,
    required Uint8List packageDigest,
    required this.registryRevision,
    required Uint8List nominationRef,
    required Uint8List ownerDomain,
    required Uint8List transferEpoch,
    required Uint8List objectRef,
    required Uint8List operationId,
    required this.byteLength,
    required Uint8List wholeSha256,
    required this.timeoutMs,
    required this.handler,
    required this.inputType,
    required this.outputType,
    required Uint8List input,
  }) : submission = _owned(submission),
       packageDigest = _owned(packageDigest),
       nominationRef = _owned(nominationRef),
       ownerDomain = _owned(ownerDomain),
       transferEpoch = _owned(transferEpoch),
       objectRef = _owned(objectRef),
       operationId = _owned(operationId),
       wholeSha256 = _owned(wholeSha256),
       input = _owned(input);
  final Uint8List submission,
      packageDigest,
      nominationRef,
      ownerDomain,
      transferEpoch,
      objectRef,
      operationId,
      wholeSha256,
      input;
  final String packageId, handler, inputType, outputType;
  final BigInt registryRevision, byteLength;
  final int timeoutMs;
}

enum BlobUploadObservation { absent, staged, released }

enum BlobUploadError {
  denied,
  limit,
  protocol,
  cancelled,
  expired,
  storage,
  unknown,
  owner,
  spawn,
  consumed,
}

final class BlobUploadResult {
  const BlobUploadResult({
    required this.phase,
    required this.error,
    required this.unknown,
    required this.calls,
    required this.wireBytes,
    required this.nativeWorkBytes,
    required this.hasExecution,
    required this.executionFault,
    required this.exitCode,
    required this.hostCalls,
  });
  final BlobUploadObservation? phase;
  final BlobUploadError? error;
  final bool unknown, hasExecution;
  final int calls, exitCode, hostCalls;
  final BigInt wireBytes, nativeWorkBytes;
  final IoExecutionFault executionFault;
}

final class BlobUploadSnapshot {
  const BlobUploadSnapshot({
    required this.task,
    required this.ready,
    required this.delivered,
    required this.recoveryRequired,
    required this.observed,
  });
  final IoTaskSnapshot task;
  final bool ready, delivered, recoveryRequired;

  /// Read-only original-history observation. Never replaces the task outcome.
  final BlobUploadObservation? observed;
}

final class BlobUploadReply {
  const BlobUploadReply(this.snapshot, this.result);
  final BlobUploadSnapshot snapshot;
  final BlobUploadResult? result;
}

abstract interface class BlobUploadBackend {
  Future<void> approveBlobUpload(
    String packageId,
    Uint8List digest,
    BigInt revision,
    bool approved,
  );
  Future<BlobUploadSnapshot> startBlobUpload(BlobUploadRequest request);
  Future<BlobUploadSnapshot> statusBlobUpload(Uint8List key);
  Future<BlobUploadReply> readBlobUpload(Uint8List key);
  Future<BlobUploadSnapshot> inspectBlobUpload(Uint8List key);
  Future<BlobUploadSnapshot> confirmBlobUpload(
    Uint8List key,
    BlobUploadObservation expected,
  );
}

abstract final class BlobUploadValidation {
  static final _u64 = (BigInt.one << 64) - BigInt.one;
  static void selection(String id, Uint8List digest, BigInt revision) {
    text(id);
    HttpTaskValidation.identity(digest);
    if (revision < BigInt.zero || revision > _u64) {
      throw const FormatException('Invalid upload catalog revision');
    }
  }

  static void text(String value) {
    if (value.isEmpty ||
        utf8.encode(value).length > 256 ||
        value.runes.any((c) => c < 32 || c == 127)) {
      throw const FormatException('Invalid upload handler identity');
    }
  }

  static void request(BlobUploadRequest value) {
    selection(value.packageId, value.packageDigest, value.registryRevision);
    for (final id in [
      value.submission,
      value.nominationRef,
      value.ownerDomain,
      value.transferEpoch,
      value.objectRef,
      value.operationId,
    ]) {
      HttpTaskValidation.identity(id);
    }
    for (final name in [value.handler, value.inputType, value.outputType]) {
      text(name);
    }
    if (value.wholeSha256.length != 32 ||
        value.input.length > 65536 ||
        value.timeoutMs < 1 ||
        value.timeoutMs > 120000 ||
        value.byteLength < BigInt.zero ||
        value.byteLength > BigInt.from(16 * 1024 * 1024)) {
      throw const FormatException('Invalid upload bounds');
    }
  }
}
