import 'dart:typed_data';
import 'blob_upload_models.dart';

Uint8List _owned(Uint8List bytes) =>
    Uint8List.fromList(bytes).asUnmodifiableView();

/// Historical correlations only. There is no live task key or start authority.
final class BlobUploadHistoryRecord {
  BlobUploadHistoryRecord({
    required Uint8List submission,
    required this.packageId,
    required Uint8List packageDigest,
    required this.registryRevision,
    required Uint8List nominationRef,
    required Uint8List ownerDomain,
    required this.operationId,
    required this.byteLength,
    required Uint8List wholeSha256,
    required this.handler,
    required Uint8List inputSha256,
    required Uint8List bindingSha256,
    required this.reservedAt,
    required this.revision,
    required this.originalResult,
    required this.observedAt,
    required this.acknowledgedAt,
  }) : submission = _owned(submission),
       packageDigest = _owned(packageDigest),
       nominationRef = _owned(nominationRef),
       ownerDomain = _owned(ownerDomain),
       wholeSha256 = _owned(wholeSha256),
       inputSha256 = _owned(inputSha256),
       bindingSha256 = _owned(bindingSha256);

  final Uint8List submission,
      packageDigest,
      nominationRef,
      ownerDomain,
      wholeSha256,
      inputSha256,
      bindingSha256;
  final String packageId, operationId, handler;
  final BigInt registryRevision, byteLength, reservedAt;
  final int revision;
  final BlobUploadResult? originalResult;
  final BigInt? observedAt, acknowledgedAt;
  bool get originalUnknown => originalResult == null || originalResult!.unknown;
}

final class BlobUploadHistoryPage {
  BlobUploadHistoryPage(
    List<BlobUploadHistoryRecord> records,
    Uint8List? cursor,
  ) : records = List.unmodifiable(records),
      cursor = cursor == null ? null : _owned(cursor);
  final List<BlobUploadHistoryRecord> records;
  final Uint8List? cursor;
}

final class BlobUploadHistoryInspection {
  const BlobUploadHistoryInspection(this.record, this.currentStage);
  final BlobUploadHistoryRecord record;

  /// Current durable stage state, never a replacement for the original result.
  final BlobUploadObservation currentStage;
}

abstract interface class BlobUploadHistoryBackend {
  Future<BlobUploadHistoryPage> listBlobUploadHistory({
    Uint8List? after,
    int limit = 16,
  });
  Future<BlobUploadHistoryInspection> inspectBlobUploadHistory(
    Uint8List submission,
  );
}
