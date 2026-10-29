import 'dart:typed_data';
import 'package:capnproto_dart/capnproto_dart.dart';
import 'package:test/test.dart';
import '../../../lib/plugins/generated/host.capnp.dart' as host;
import '../../../lib/plugins/blob_upload_models.dart';
import '../../../lib/plugins/blob_upload_native.dart';
import 'blob_upload_client_test.dart' show id;

final class Peer {
  final actions = <host.Action>[];
  host.RequestReader? last;
  bool acknowledgement = false;
  bool wrongCompletion = false;
  void Function(host.BlobUploadMetadataReviewBuilder)? corrupt;
  Future<T> exchange<T>(
    host.Action action, {
    void Function(host.RequestBuilder)? configure,
    required T Function(host.ResponseReader) decode,
    required bool clearReply,
  }) async {
    actions.add(action);
    final input = MessageBuilder();
    configure?.call(input.initRoot(host.requestFactory)..action = action);
    last = MessageReader.deserialize(
      input.serialize(),
    ).getRoot(host.requestFactory);
    final output = MessageBuilder(),
        row = output.initRoot(host.responseFactory);
    if (action == host.Action.blobUploadMetadataReview) {
      final review = row.initBlobUploadMetadataReview();
      review.key = id(3);
      review.submission = id(1);
      review.permit = id(9);
      review.phase = acknowledgement ? 2 : 1;
      review.writeRequired = true;
      review.originalPhase = 2;
      review.observedAtBigInt = (BigInt.one << 53) + BigInt.from(7);
      if (acknowledgement)
        review.acknowledgementAtBigInt = (BigInt.one << 53) + BigInt.from(8);
      corrupt?.call(review);
    } else {
      row.blobUploadMetadataCompletedPermit = id(wrongCompletion ? 88 : 9);
      row.blobUploadMetadataAcknowledged = acknowledgement;
      final task = row.initIoState();
      if (!acknowledgement) {
        task.key = id(3);
        task.submission = id(1);
        task.storage = 3;
        task.hasExit = true;
        row.initBlobUploadState().delivered = true;
        row.initBlobUploadResult();
      }
    }
    final bytes = output.serialize();
    try {
      return decode(
        MessageReader.deserialize(bytes).getRoot(host.responseFactory),
      );
    } finally {
      if (clearReply) bytes.fillRange(0, bytes.length, 0);
    }
  }
}

void main() {
  test(
    'idle acknowledgement with a different completion permit is refused',
    () async {
      final peer = Peer()..acknowledgement = true;
      final client = NativeBlobUploadClient(peer.exchange);
      final review = await client.reviewBlobUploadMetadata(id(3));
      peer.wrongCompletion = true;
      await expectLater(
        client.completeBlobUploadMetadata(review),
        throwsFormatException,
      );
      expect(
        peer.actions
            .where((a) => a == host.Action.blobUploadMetadataComplete)
            .length,
        1,
      );
    },
  );
  test(
    'private append-only actions keep history read-only and carry only original permit',
    () async {
      expect(host.Action.blobUploadHistoryInspect.index, 176);
      expect(host.Action.blobUploadMetadataReview.index, 177);
      expect(host.Action.blobUploadMetadataComplete.index, 178);
      for (final ack in [false, true]) {
        final peer = Peer()..acknowledgement = ack;
        final client = NativeBlobUploadClient(peer.exchange);
        final review = await client.reviewBlobUploadMetadata(id(3));
        expect(review.observedAt, (BigInt.one << 53) + BigInt.from(7));
        expect(() => review.permit[0] = 0, throwsUnsupportedError);
        final done = await client.completeBlobUploadMetadata(review);
        expect(done.upload == null, ack);
        expect(peer.last!.ioKey, id(3));
        expect(peer.last!.blobUploadMetadataPermit, id(9));
        expect(peer.last!.blobUploadStart, isNull);
        expect(peer.actions, [
          host.Action.blobUploadMetadataReview,
          host.Action.blobUploadMetadataComplete,
        ]);
      }
    },
  );
  final changes = <String, void Function(host.BlobUploadMetadataReviewBuilder)>{
    'wrong task': (r) => r.key = id(4),
    'empty permit': (r) => r.permit = Uint8List(0),
    'zero submission': (r) => r.submission = id(0),
    'missing phase': (r) => r.phase = 0,
    'invented phase': (r) => r.phase = 3,
    'both success and failure': (r) => r.originalError = 1,
    'no original result': (r) => r.originalPhase = 0,
    'false known unknown': (r) {
      r.originalPhase = 0;
      r.originalError = 7;
    },
    'ack time on observation': (r) => r.acknowledgementAtBigInt = BigInt.one,
  };
  for (final entry in changes.entries) {
    test('reject metadata review: ${entry.key}', () async {
      final peer = Peer()..corrupt = entry.value;
      await expectLater(
        NativeBlobUploadClient(peer.exchange).reviewBlobUploadMetadata(id(3)),
        throwsFormatException,
      );
      expect(peer.actions, [host.Action.blobUploadMetadataReview]);
    });
  }
  test('completion cannot change its reviewed phase', () async {
    final peer = Peer();
    final current = NativeBlobUploadClient(peer.exchange);
    final review = await current.reviewBlobUploadMetadata(id(3));
    expect(review.phase, BlobUploadMetadataPhase.observation);
    peer.acknowledgement = true;
    await expectLater(
      current.completeBlobUploadMetadata(review),
      throwsFormatException,
    );
  });
}
