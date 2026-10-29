// Deterministic session/UI peer only; this is not native runtime evidence.
import 'dart:async';
import 'dart:typed_data';
import '../../../lib/plugins/blob_upload_models.dart';

Uint8List metadataId(int value) => Uint8List.fromList(List.filled(32, value));

final class MetadataPeer
    implements BlobUploadBackend, BlobUploadMetadataBackend {
  MetadataPeer(this.original);
  final BlobUploadBackend original;
  final calls = <String>[];
  final attempted = <BlobUploadMetadataPhase>{};
  BlobUploadMetadataPhase phase = BlobUploadMetadataPhase.observation;
  bool unknown = false, wrongKey = false, loseCompletion = false;
  Completer<void>? reviewWait, completionWait;
  Future<BlobUploadMetadataCompletion> Function(BlobUploadMetadataReview)?
  onComplete;
  @override
  Future<BlobUploadMetadataReview> reviewBlobUploadMetadata(
    Uint8List key,
  ) async {
    calls.add('review');
    await reviewWait?.future;
    if (attempted.contains(phase)) {
      throw StateError('completion already attempted');
    }
    return BlobUploadMetadataReview(
      key: wrongKey ? metadataId(55) : key,
      submission: metadataId(1),
      permit: metadataId(31),
      phase: phase,
      writeRequired: true,
      originalUnknown: unknown,
      originalPhase: unknown ? null : BlobUploadObservation.staged,
      originalError: unknown ? BlobUploadError.unknown : null,
      observedAt: BigInt.from(100),
      acknowledgementAt: phase == BlobUploadMetadataPhase.acknowledgement
          ? BigInt.from(101)
          : null,
    );
  }

  @override
  Future<BlobUploadMetadataCompletion> completeBlobUploadMetadata(
    BlobUploadMetadataReview review,
  ) async {
    calls.add('complete');
    if (!attempted.add(review.phase)) throw StateError('duplicate completion');
    await completionWait?.future;
    final result = await onComplete!(review);
    if (loseCompletion) throw StateError('reply lost after completion');
    return result;
  }

  @override
  Future<void> approveBlobUpload(
    String id,
    Uint8List digest,
    BigInt revision,
    bool approved,
  ) => original.approveBlobUpload(id, digest, revision, approved);
  @override
  Future<BlobUploadSnapshot> startBlobUpload(BlobUploadRequest request) =>
      original.startBlobUpload(request);
  @override
  Future<BlobUploadSnapshot> statusBlobUpload(Uint8List key) =>
      original.statusBlobUpload(key);
  @override
  Future<BlobUploadReply> readBlobUpload(Uint8List key) =>
      original.readBlobUpload(key);
  @override
  Future<BlobUploadSnapshot> inspectBlobUpload(Uint8List key) =>
      original.inspectBlobUpload(key);
  @override
  Future<BlobUploadSnapshot> confirmBlobUpload(
    Uint8List key,
    BlobUploadObservation expected,
  ) => original.confirmBlobUpload(key, expected);
}
