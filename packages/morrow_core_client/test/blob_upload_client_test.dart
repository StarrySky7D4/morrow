import 'dart:typed_data';
import 'package:capnproto_dart/capnproto_dart.dart';
import 'package:test/test.dart';
import '../../../lib/plugins/generated/host.capnp.dart' as host;
import '../../../lib/plugins/blob_upload_models.dart';
import '../../../lib/plugins/blob_upload_native.dart';

Uint8List id(int n) => Uint8List.fromList(List.filled(32, n));
BlobUploadRequest request({
  int timeout = 120000,
  int length = 150017,
  int inputLength = 2,
  BigInt? revision,
  Uint8List? submission,
}) => BlobUploadRequest(
  submission: submission ?? id(1),
  packageId: 'org.example.upload',
  packageDigest: id(2),
  registryRevision: revision ?? (BigInt.one << 63) + BigInt.from(7),
  nominationRef: id(4),
  ownerDomain: id(5),
  transferEpoch: id(6),
  objectRef: id(7),
  operationId: id(8),
  byteLength: BigInt.from(length),
  wholeSha256: id(9),
  timeoutMs: timeout,
  handler: 'upload',
  inputType: 'data.v1',
  outputType: 'result.v1',
  input: Uint8List(inputLength),
);

final class Envelope {
  Envelope(host.ResponseBuilder row)
    : ioState = row.initIoState(),
      blobUploadState = row.initBlobUploadState(),
      blobUploadResult = row.initBlobUploadResult();
  final host.IoStateBuilder ioState;
  final host.BlobUploadStateBuilder blobUploadState;
  final host.BlobUploadResultBuilder blobUploadResult;
}

final class Peer {
  final actions = <host.Action>[];
  host.RequestReader? last;
  bool fail = false;
  void Function(Envelope)? corrupt;
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
    if (fail) throw StateError('lost reply');
    final output = MessageBuilder();
    final row = output.initRoot(host.responseFactory);
    final envelope = Envelope(row);
    final state = envelope.ioState;
    state.key = id(3);
    state.submission = id(1);
    state.storage = 1;
    corrupt?.call(envelope);
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

void completed(Envelope row) {
  row.ioState.storage = 3;
  row.ioState.hasExit = true;
  row.blobUploadState.delivered = true;
  final result = row.blobUploadResult;
  result.kind = 1;
  result.phase = 2;
  result.hasExecution = true;
  result.calls = 7;
  result.hostCalls = 9;
  result.wireBytesBigInt = BigInt.from(151000);
  result.nativeWorkBytesBigInt = BigInt.from(760000);
}

void main() {
  test('append-only actions preserve existing identities', () {
    expect(host.Action.directoryPick.index, 161);
    expect(host.Action.pluginApproveBlobUpload.index, 162);
    expect(host.Action.blobUploadConfirm.index, 167);
  });
  test(
    'exact immutable operation crosses binary channel with uint64 revision',
    () async {
      final original = id(1), p = Peer();
      final value = request(submission: original);
      original[0] = 99;
      final state = await NativeBlobUploadClient(
        p.exchange,
      ).startBlobUpload(value);
      final sent = p.last!.blobUploadStart!;
      expect(sent.submission, id(1));
      expect(sent.registryRevisionBigInt, value.registryRevision);
      expect(sent.ownerDomain, id(5));
      expect(sent.transferEpoch, id(6));
      expect(sent.objectRef, id(7));
      expect(sent.operationId, id(8));
      expect(sent.wholeSha256, id(9));
      expect(sent.byteLengthBigInt, BigInt.from(150017));
      expect(state.task.key, id(3));
      expect(state.task.submission, id(1));
      expect(() => state.task.key![0] = 0, throwsUnsupportedError);
      expect(() => value.input[0] = 0, throwsUnsupportedError);
    },
  );
  test('malformed and out of budget attempts never dispatch', () {
    final p = Peer(),
        requests = [
          request(timeout: 0),
          request(timeout: 120001),
          request(length: -1),
          request(length: 16777217),
          request(inputLength: 65537),
          request(submission: id(0)),
          request(submission: Uint8List(31)),
          request(revision: -BigInt.one),
          request(revision: BigInt.one << 64),
        ];
    final client = NativeBlobUploadClient(p.exchange);
    for (final value in requests) {
      expect(() => client.startBlobUpload(value), throwsFormatException);
    }
    expect(p.actions, isEmpty);
  });
  test('lost start acknowledgement is not replayed', () async {
    final p = Peer()..fail = true;
    await expectLater(
      NativeBlobUploadClient(p.exchange).startBlobUpload(request()),
      throwsStateError,
    );
    expect(p.actions, [host.Action.blobUploadStart]);
  });
  test('original key and original submission are mandatory', () async {
    for (final change in <void Function(Envelope)>[
      (r) => r.ioState.submission = id(17),
      (r) => r.ioState.key = Uint8List(0),
    ]) {
      final p = Peer()..corrupt = change;
      await expectLater(
        NativeBlobUploadClient(p.exchange).startBlobUpload(request()),
        throwsFormatException,
      );
    }
    final p = Peer();
    await expectLater(
      NativeBlobUploadClient(p.exchange).statusBlobUpload(id(8)),
      throwsFormatException,
    );
  });
  test(
    'package approval is exact and does not set enable or operation fields',
    () async {
      final p = Peer();
      await NativeBlobUploadClient(
        p.exchange,
      ).approveBlobUpload('org.example.upload', id(2), BigInt.one, true);
      expect(p.actions, [host.Action.pluginApproveBlobUpload]);
      expect(p.last!.id, 'org.example.upload');
      expect(p.last!.sha256, id(2));
      expect(p.last!.blobUploadApproved, isTrue);
      expect(p.last!.limit, 0);
      expect(p.last!.blobUploadStart, isNull);
    },
  );
  test(
    'copied result contains terminal metadata and separate observation',
    () async {
      final p = Peer()..corrupt = completed;
      final result = await NativeBlobUploadClient(
        p.exchange,
      ).readBlobUpload(id(3));
      expect(result.result!.phase, BlobUploadObservation.staged);
      expect(result.result!.wireBytes, BigInt.from(151000));
      expect(result.snapshot.observed, isNull);
    },
  );
  final corruptions = <String, void Function(Envelope)>{
    'nonterminal phase': (r) => r.blobUploadResult.phase = 4,
    'success with error': (r) => r.blobUploadResult.error = 1,
    'success without execution': (r) => r.blobUploadResult.hasExecution = false,
    'success with trap': (r) => r.blobUploadResult.executionFault = 9,
    'success with nonzero exit': (r) => r.blobUploadResult.exitCode = 1,
    'undeclared error': (r) {
      r.blobUploadResult.kind = 2;
      r.blobUploadResult.phase = 0;
    },
    'unknown without recovery': (r) => r.blobUploadResult.unknown = true,
    'not consumed': (r) => r.blobUploadState.delivered = false,
    'double delivery states': (r) => r.blobUploadState.ready = true,
    'no original join': (r) => r.ioState.hasExit = false,
    'wire overrun': (r) =>
        r.blobUploadResult.wireBytesBigInt = BigInt.from(67108865),
    'native work overrun': (r) =>
        r.blobUploadResult.nativeWorkBytesBigInt = BigInt.from(134217729),
    'calls overrun': (r) => r.blobUploadResult.calls = 1025,
    'unknown observation': (r) => r.blobUploadState.observed = 4,
    'unexpected payload on status': (_) {},
  };
  for (final item in corruptions.entries) {
    test('rejects ${item.key}', () async {
      final p = Peer()
        ..corrupt = (r) {
          completed(r);
          item.value(r);
        };
      final client = NativeBlobUploadClient(p.exchange);
      await expectLater(
        item.key == 'unexpected payload on status'
            ? client.statusBlobUpload(id(3))
            : client.readBlobUpload(id(3)),
        throwsFormatException,
      );
    });
  }
  test('empty result rejects every non-default trailing value', () async {
    for (final mutate in <void Function(host.BlobUploadResultBuilder)>[
      (r) => r.hostCalls = 1,
      (r) => r.exitCode = 1,
      (r) => r.unknown = true,
      (r) => r.calls = 1,
      (r) => r.phase = 1,
      (r) => r.error = 1,
      (r) => r.wireBytesBigInt = BigInt.one,
      (r) => r.nativeWorkBytesBigInt = BigInt.one,
    ]) {
      final p = Peer()..corrupt = (r) => mutate(r.blobUploadResult);
      await expectLater(
        NativeBlobUploadClient(p.exchange).statusBlobUpload(id(3)),
        throwsFormatException,
      );
    }
  });
}
