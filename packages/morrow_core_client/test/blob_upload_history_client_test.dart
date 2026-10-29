import 'dart:typed_data';
import 'package:capnproto_dart/capnproto_dart.dart';
import 'package:test/test.dart';
import '../../../lib/plugins/generated/host.capnp.dart' as host;
import '../../../lib/plugins/blob_upload_history_native.dart';
import '../../../lib/plugins/blob_upload_models.dart';

Uint8List id(int value) => Uint8List.fromList(List.filled(32, value));
void record(
  host.BlobUploadHistoryBuilder r, {
  int submission = 1,
  int revision = 3,
  bool unknown = false,
}) {
  r.submission = id(submission);
  r.packageId = 'org.example.original';
  r.packageDigest = id(2);
  r.registryRevisionBigInt = (BigInt.one << 63) + BigInt.from(9);
  r.nominationRef = id(3);
  r.ownerDomain = id(4);
  r.operationId = 'immutable-original-operation';
  r.byteLengthBigInt = BigInt.from(41);
  r.wholeSha256 = id(5);
  r.handler = 'blob.upload';
  r.inputSha256 = id(6);
  r.bindingSha256 = id(7);
  r.reservedAtBigInt = BigInt.from(1793260800000);
  r.revision = revision;
  r.resultRecorded = revision >= 2;
  r.acknowledged = revision == 3;
  if (revision >= 2) {
    r.observedAtBigInt = BigInt.from(1793260800001);
    final result = r.initOriginalResult();
    result.kind = unknown ? 2 : 1;
    result.phase = unknown ? 0 : 2;
    result.error = unknown ? 7 : 0;
    result.unknown = unknown;
    result.hasExecution = true;
    result.calls = 5;
    result.hostCalls = 5;
    result.wireBytesBigInt = BigInt.from(200);
    result.nativeWorkBytesBigInt = BigInt.from(41);
  }
  if (revision == 3) r.acknowledgedAtBigInt = BigInt.from(1793260800002);
}

final class Peer {
  final actions = <host.Action>[];
  host.RequestReader? last;
  int revision = 3;
  bool unknown = false, fail = false;
  void Function(host.ResponseBuilder)? corrupt;
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
    if (fail) throw StateError('lost read reply');
    final output = MessageBuilder(),
        row = output.initRoot(host.responseFactory);
    record(
      row.initBlobUploadHistory(1)[0],
      revision: revision,
      unknown: unknown,
    );
    if (action == host.Action.blobUploadHistoryList) {
      row.blobUploadHistoryCursor = id(1);
    } else {
      row.blobUploadCurrentStage = 2;
    }
    corrupt?.call(row);
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
    'private history actions are appended without changing prior actions',
    () {
      expect(host.Action.blobPlanDiscover.index, 174);
      expect(host.Action.blobUploadHistoryList.index, 175);
      expect(host.Action.blobUploadHistoryInspect.index, 176);
    },
  );
  test(
    'binary history survives reply clearing and preserves full unsigned revision',
    () async {
      final peer = Peer(),
          client = NativeBlobUploadHistoryClient(peer.exchange);
      final page = await client.listBlobUploadHistory();
      expect(page.records.single.submission, id(1));
      expect(
        page.records.single.registryRevision,
        (BigInt.one << 63) + BigInt.from(9),
      );
      expect(page.records.single.acknowledgedAt, isNotNull);
      expect(page.cursor, id(1));
      expect(peer.last!.ioKey?.isEmpty ?? true, isTrue);
      expect(
        () => page.records.single.submission[0] = 0,
        throwsUnsupportedError,
      );
    },
  );
  test(
    'current staged content never promotes a missing or Unknown original result',
    () async {
      for (final revision in [1, 2, 3]) {
        final peer = Peer()
          ..revision = revision
          ..unknown = true;
        final value = await NativeBlobUploadHistoryClient(
          peer.exchange,
        ).inspectBlobUploadHistory(id(1));
        expect(value.currentStage, BlobUploadObservation.staged);
        expect(value.record.originalUnknown, isTrue);
        expect(value.record.acknowledgedAt != null, revision == 3);
        expect(peer.actions, [host.Action.blobUploadHistoryInspect]);
      }
    },
  );
  test(
    'invalid query fails before exchange and a read failure is not retried',
    () async {
      final peer = Peer(),
          client = NativeBlobUploadHistoryClient(peer.exchange);
      for (final limit in [0, 17]) {
        expect(
          () => client.listBlobUploadHistory(limit: limit),
          throwsFormatException,
        );
      }
      expect(
        () => client.inspectBlobUploadHistory(id(0)),
        throwsFormatException,
      );
      expect(peer.actions, isEmpty);
      peer.fail = true;
      await expectLater(client.listBlobUploadHistory(), throwsStateError);
      expect(peer.actions, [host.Action.blobUploadHistoryList]);
    },
  );
  test(
    'rejects foreign submissions, inconsistent cursors and out-of-order pages',
    () async {
      final peer = Peer(),
          client = NativeBlobUploadHistoryClient(peer.exchange);
      await expectLater(
        client.inspectBlobUploadHistory(id(2)),
        throwsFormatException,
      );
      await expectLater(
        client.listBlobUploadHistory(after: id(1)),
        throwsFormatException,
      );
      peer.corrupt = (r) => r.blobUploadHistoryCursor = id(2);
      await expectLater(client.listBlobUploadHistory(), throwsFormatException);
      peer.corrupt = (r) {
        final rows = r.initBlobUploadHistory(2);
        record(rows[0]);
        record(rows[1]);
      };
      await expectLater(client.listBlobUploadHistory(), throwsFormatException);
    },
  );
  test(
    'rejects impossible history transitions and contradictory original results',
    () async {
      for (final corrupt in <void Function(host.BlobUploadHistoryBuilder)>[
        (r) => r.revision = 0,
        (r) => r.acknowledged = false,
        (r) => r.observedAtBigInt = BigInt.zero,
        (r) {
          r.initOriginalResult()
            ..kind = 2
            ..error = 7;
        },
        (r) {
          r.initOriginalResult()
            ..kind = 2
            ..error = 6
            ..hasExecution = true
            ..executionFault = 10;
        },
        (r) => r.bindingSha256 = id(0),
      ]) {
        final peer = Peer()
          ..corrupt = (response) {
            final row = response.initBlobUploadHistory(1)[0];
            record(row);
            corrupt(row);
          };
        await expectLater(
          NativeBlobUploadHistoryClient(peer.exchange).listBlobUploadHistory(),
          throwsFormatException,
        );
      }
    },
  );
}
