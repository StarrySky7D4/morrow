import 'dart:typed_data';
import 'io_task_models.dart';
import 'blob_upload_history_packet.dart';

Uint8List _owned(Uint8List value) =>
    Uint8List.fromList(value).asUnmodifiableView();

/// One explicit disclosure of selected history; never an upload retry.
final class BlobHistoryDeliveryRequest {
  BlobHistoryDeliveryRequest({
    required Uint8List requestId,
    required Uint8List submission,
    required this.packageId,
    required Uint8List packageDigest,
    required this.registryRevision,
    required this.handler,
    required this.outputType,
    required this.timeoutMs,
  }) : requestId = _owned(requestId),
       submission = _owned(submission),
       packageDigest = _owned(packageDigest);
  final Uint8List requestId, submission, packageDigest;
  final String packageId, handler, outputType;
  final BigInt registryRevision;
  final int timeoutMs;
}

/// Exact host preview with an independently correlated public history record.
/// Decoding the selected history grants no authority to deliver or retry.
final class BlobHistoryDeliveryPreview {
  BlobHistoryDeliveryPreview({
    required Uint8List key,
    required this.request,
    required this.taskId,
    required Uint8List packet,
    required Uint8List packetSha256,
  }) : key = _owned(key),
       packet = _owned(packet),
       packetSha256 = _owned(packetSha256);
  final Uint8List key, packet, packetSha256;
  final BlobHistoryDeliveryRequest request;
  final String taskId;
  PublicHistoryPacket get selectedHistory =>
      PublicHistoryPacket.decodeTask(packet, taskId);
}

enum BlobHistoryDeliveryClose { notCreated, pending, closed, failed, unknown }

enum BlobHistoryDeliveryError {
  cancelled,
  expired,
  staleOwner,
  staleSelection,
  historyChanged,
  gateClosed,
  preparation,
  execution,
  pluginFailure,
  protocol,
  unwind,
  close,
}

enum BlobHistoryCloseFailureKind { core, unwind }

final class BlobHistoryCloseFailure {
  const BlobHistoryCloseFailure(this.kind, this.coreError, this.detail);
  final BlobHistoryCloseFailureKind kind;
  final int? coreError;
  final String? detail;
}

final class BlobHistoryDeliveryReceipt {
  BlobHistoryDeliveryReceipt({
    required Uint8List key,
    required Uint8List requestId,
    required this.consumed,
    required this.dispatched,
    required this.finished,
    required this.acknowledged,
    required this.close,
    required this.closeFailure,
    required this.error,
    required this.deliveryError,
    required this.hasExecution,
    required this.executionFault,
    required this.exitCode,
    required this.hostCalls,
    required this.fuelRemaining,
    required this.pluginFailureCode,
    required this.outputType,
    required Uint8List? output,
  }) : key = _owned(key),
       requestId = _owned(requestId),
       output = output == null ? null : _owned(output);
  final Uint8List key, requestId;
  final bool consumed, dispatched, finished, acknowledged, hasExecution;
  final BlobHistoryDeliveryClose close;
  final BlobHistoryCloseFailure? closeFailure;

  /// Original execution error and later delivery veto are separate facts.
  final BlobHistoryDeliveryError? error, deliveryError;
  final IoExecutionFault executionFault;
  final int exitCode, hostCalls;
  final BigInt fuelRemaining;
  final int? pluginFailureCode;
  final String? outputType;
  final Uint8List? output;

  bool get canAcknowledge =>
      consumed &&
      finished &&
      !acknowledged &&
      (close == BlobHistoryDeliveryClose.notCreated ||
          close == BlobHistoryDeliveryClose.closed) &&
      deliveryError != BlobHistoryDeliveryError.staleOwner;
}

abstract interface class BlobHistoryDeliveryBackend {
  Future<BlobHistoryDeliveryPreview> prepareBlobHistoryDelivery(
    BlobHistoryDeliveryRequest request,
  );
  Future<BlobHistoryDeliveryPreview> previewBlobHistoryDelivery(
    Uint8List key,
    BlobHistoryDeliveryRequest expected,
  );
  Future<BlobHistoryDeliveryReceipt> approveBlobHistoryDelivery(
    BlobHistoryDeliveryPreview reviewed,
  );
  Future<BlobHistoryDeliveryReceipt> readBlobHistoryDelivery(
    Uint8List key,
    Uint8List requestId, {
    String? expectedOutputType,
  });
  Future<BlobHistoryDeliveryReceipt> cancelBlobHistoryDelivery(
    Uint8List key,
    Uint8List requestId,
  );
  Future<BlobHistoryDeliveryReceipt> acknowledgeBlobHistoryDelivery(
    BlobHistoryDeliveryReceipt original,
  );
  Future<BlobHistoryDeliveryReceipt?> discoverBlobHistoryDelivery(
    Uint8List requestId, {
    String? expectedOutputType,
  });
}
