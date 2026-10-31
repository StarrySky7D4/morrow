part of history_delivery_test_support;

Uint8List id(int n) => Uint8List.fromList(List.filled(32, n));
String hex(List<int> bytes) =>
    bytes.map((n) => n.toRadixString(16).padLeft(2, '0')).join();
final packet = File(
  'packages/morrow_core_client/test/fixtures/history-unknown-acknowledged.capnp',
).readAsBytesSync();
BlobHistoryDeliveryRequest request() => BlobHistoryDeliveryRequest(
  requestId: id(2),
  submission: id(1),
  packageId: 'org.example.history',
  packageDigest: id(3),
  registryRevision: BigInt.one,
  handler: 'history.inspect',
  outputType: 'text.utf8',
  timeoutMs: 120000,
);
BlobHistoryDeliveryPreview preview(BlobHistoryDeliveryRequest r) =>
    BlobHistoryDeliveryPreview(
      key: id(9),
      request: r,
      taskId: 'history-v1.${hex(id(1))}.${hex(id(9))}',
      packet: packet,
      packetSha256: Uint8List.fromList(sha256.convert(packet).bytes),
    );
BlobHistoryDeliveryReceipt receipt(
  BlobHistoryDeliveryRequest r, {
  bool consumed = true,
  bool acknowledged = false,
  bool cancelled = false,
  BlobHistoryDeliveryClose close = BlobHistoryDeliveryClose.closed,
  BlobHistoryDeliveryError? error,
  Uint8List? requestId,
}) => BlobHistoryDeliveryReceipt(
  key: id(9),
  requestId: requestId ?? r.requestId,
  consumed: consumed,
  dispatched: consumed && !cancelled,
  finished: consumed,
  acknowledged: acknowledged,
  close: !consumed || cancelled ? BlobHistoryDeliveryClose.notCreated : close,
  closeFailure: null,
  error: cancelled ? BlobHistoryDeliveryError.cancelled : error,
  deliveryError: null,
  hasExecution: consumed && !cancelled,
  executionFault: IoExecutionFault.none,
  exitCode: 0,
  hostCalls: 0,
  fuelRemaining: BigInt.zero,
  pluginFailureCode: null,
  outputType: consumed && !cancelled ? 'text.utf8' : null,
  output: consumed && !cancelled ? Uint8List.fromList([111, 107]) : null,
);

class Backend implements BlobHistoryDeliveryBackend {
  int prepares = 0,
      approvals = 0,
      reads = 0,
      previews = 0,
      cancels = 0,
      acks = 0,
      discoveries = 0;
  BlobHistoryDeliveryRequest? original;
  BlobHistoryDeliveryReceipt? current;
  bool losePrepare = false,
      loseApproval = false,
      loseCancel = false,
      loseAck = false,
      absent = false;
  Completer<BlobHistoryDeliveryPreview>? pending;
  BlobHistoryDeliveryClose close = BlobHistoryDeliveryClose.closed;
  @override
  Future<BlobHistoryDeliveryPreview> prepareBlobHistoryDelivery(
    BlobHistoryDeliveryRequest r,
  ) async {
    prepares++;
    original = r;
    current = receipt(r, consumed: false);
    if (pending != null) return pending!.future;
    if (losePrepare) throw StateError('reply lost');
    return preview(r);
  }

  @override
  Future<BlobHistoryDeliveryPreview> previewBlobHistoryDelivery(
    Uint8List key,
    BlobHistoryDeliveryRequest expected,
  ) async {
    previews++;
    return preview(expected);
  }

  @override
  Future<BlobHistoryDeliveryReceipt> approveBlobHistoryDelivery(
    BlobHistoryDeliveryPreview reviewed,
  ) async {
    approvals++;
    current = receipt(
      original!,
      close: close,
      error: close == BlobHistoryDeliveryClose.unknown
          ? BlobHistoryDeliveryError.close
          : null,
    );
    if (loseApproval) throw StateError('reply lost');
    return current!;
  }

  @override
  Future<BlobHistoryDeliveryReceipt?> discoverBlobHistoryDelivery(
    Uint8List requestId, {
    String? expectedOutputType,
  }) async {
    discoveries++;
    return absent ? null : current;
  }

  @override
  Future<BlobHistoryDeliveryReceipt> readBlobHistoryDelivery(
    Uint8List key,
    Uint8List requestId, {
    String? expectedOutputType,
  }) async {
    reads++;
    return current!;
  }

  @override
  Future<BlobHistoryDeliveryReceipt> cancelBlobHistoryDelivery(
    Uint8List key,
    Uint8List requestId,
  ) async {
    cancels++;
    current = receipt(original!, cancelled: true);
    if (loseCancel) throw StateError('reply lost');
    return current!;
  }

  @override
  Future<BlobHistoryDeliveryReceipt> acknowledgeBlobHistoryDelivery(
    BlobHistoryDeliveryReceipt original,
  ) async {
    acks++;
    current = receipt(
      this.original!,
      cancelled: original.error == BlobHistoryDeliveryError.cancelled,
      acknowledged: true,
    );
    if (loseAck) throw StateError('reply lost');
    return current!;
  }
}
