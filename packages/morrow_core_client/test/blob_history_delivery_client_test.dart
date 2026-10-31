import 'dart:typed_data';
import 'dart:io';
import 'package:capnproto_dart/capnproto_dart.dart';
import 'package:crypto/crypto.dart';
import 'package:test/test.dart';
import '../../../lib/plugins/blob_history_delivery_models.dart';
import '../../../lib/plugins/blob_history_delivery_native.dart';
import '../../../lib/plugins/generated/host.capnp.dart' as host;
import '../../../lib/plugins/io_task_models.dart';

Uint8List id(int n) => Uint8List.fromList(List.filled(32, n));
final revision = (BigInt.one << 63) + BigInt.from(19);
BlobHistoryDeliveryRequest request({
  int timeout = 120000,
  String handler = 'history.inspect',
}) => BlobHistoryDeliveryRequest(
  requestId: id(1),
  submission: id(1),
  packageId: 'org.example.history',
  packageDigest: id(3),
  registryRevision: revision,
  handler: handler,
  outputType: 'example.history.receipt',
  timeoutMs: timeout,
);
// Original Rust-produced public fixture, also consumed by C and C++.
final packet = File(
  'packages/morrow_core_client/test/fixtures/history-unknown-acknowledged.capnp',
).readAsBytesSync();
String hex(Uint8List bytes) =>
    bytes.map((n) => n.toRadixString(16).padLeft(2, '0')).join();
host.BlobHistoryDeliveryRequestBuilder preview(
  host.BlobHistoryDeliveryPreviewBuilder p,
) {
  p.present = true;
  p.key = id(9);
  final r = p.initRequest(), expected = request();
  r.requestId = expected.requestId;
  r.submission = expected.submission;
  r.packageId = expected.packageId;
  r.packageDigest = expected.packageDigest;
  r.registryRevisionBigInt = expected.registryRevision;
  r.handler = expected.handler;
  r.outputType = expected.outputType;
  r.timeoutMs = expected.timeoutMs;
  p.taskId = 'history-v1.${hex(id(1))}.${hex(id(9))}';
  p.packet = packet;
  p.packetSha256 = Uint8List.fromList(sha256.convert(packet).bytes);
  return r;
}

void success(
  host.BlobHistoryDeliveryReceiptBuilder r, {
  bool acknowledged = false,
}) {
  r.found = true;
  r.key = id(9);
  r.requestId = id(1);
  r.consumed = true;
  r.dispatched = true;
  r.finished = true;
  r.acknowledged = acknowledged;
  r.close = 2;
  r.hasExecution = true;
  r.fuelRemainingBigInt = revision;
  r.hasOutput = true;
  r.outputType = request().outputType;
  r.output = Uint8List.fromList([7, 8, 9]);
}

final class ReplyParts {
  ReplyParts(
    this.root,
    this.blobHistoryDeliveryPreview,
    this.blobHistoryDeliveryReceipt,
    this.previewRequest,
  );
  final host.ResponseBuilder root;
  final host.BlobHistoryDeliveryPreviewBuilder? blobHistoryDeliveryPreview;
  final host.BlobHistoryDeliveryReceiptBuilder? blobHistoryDeliveryReceipt;
  final host.BlobHistoryDeliveryRequestBuilder? previewRequest;
  host.BlobHistoryDeliveryReceiptBuilder initBlobHistoryDeliveryReceipt() =>
      root.initBlobHistoryDeliveryReceipt();
}

final class Peer {
  final actions = <host.Action>[];
  host.RequestReader? last;
  bool lost = false, absent = false;
  void Function(ReplyParts)? corrupt;
  Future<T> exchange<T>(
    host.Action action, {
    void Function(host.RequestBuilder)? configure,
    required T Function(host.ResponseReader) decode,
    required bool clearReply,
  }) async {
    expect(clearReply, isTrue);
    actions.add(action);
    final input = MessageBuilder();
    configure?.call(input.initRoot(host.requestFactory)..action = action);
    last = MessageReader.deserialize(
      input.serialize(),
    ).getRoot(host.requestFactory);
    if (lost) throw StateError('lost original reply');
    final message = MessageBuilder(),
        out = message.initRoot(host.responseFactory);
    final p = out.initBlobHistoryDeliveryPreview(),
        r = out.initBlobHistoryDeliveryReceipt();
    host.BlobHistoryDeliveryRequestBuilder? nestedRequest;
    if (action == host.Action.blobHistoryDeliveryPrepare ||
        action == host.Action.blobHistoryDeliveryPreview) {
      nestedRequest = preview(p);
    } else if (!absent) {
      success(
        r,
        acknowledged: action == host.Action.blobHistoryDeliveryAcknowledge,
      );
    }
    corrupt?.call(ReplyParts(out, p, r, nestedRequest));
    final bytes = message.serialize();
    try {
      return decode(
        MessageReader.deserialize(bytes).getRoot(host.responseFactory),
      );
    } finally {
      bytes.fillRange(0, bytes.length, 0);
    }
  }
}

void main() {
  test('private controls remain appended after existing history actions', () {
    expect(host.Action.blobUploadMetadataComplete.index, 178);
    expect(host.Action.blobHistoryDeliveryPrepare.index, 179);
    expect(host.Action.blobHistoryDeliveryDiscover.index, 185);
  });
  test(
    'preview owns binary data after reply erasure and binds all request fields',
    () async {
      final peer = Peer(),
          client = NativeBlobHistoryDeliveryClient(peer.exchange);
      final value = await client.prepareBlobHistoryDelivery(request());
      expect(value.packet, packet);
      expect(value.request.registryRevision, revision);
      expect(
        peer.last!.blobHistoryDeliveryRequest!.registryRevisionBigInt,
        revision,
      );
      expect(peer.last!.ioKey?.isEmpty ?? true, isTrue);
      expect(() => value.packet[0] = 0, throwsUnsupportedError);
      expect(() => value.key[0] = 0, throwsUnsupportedError);
      expect(() => value.request.submission[0] = 0, throwsUnsupportedError);
      expect(peer.actions, [host.Action.blobHistoryDeliveryPrepare]);
    },
  );
  test('redirected preview selection or packet is rejected', () async {
    final mutations = <void Function(ReplyParts)>[
      (p) => p.previewRequest!.requestId = id(8),
      (p) => p.previewRequest!.submission = id(8),
      (p) => p.previewRequest!.packageId = 'org.other',
      (p) => p.previewRequest!.packageDigest = id(8),
      (p) => p.previewRequest!.registryRevisionBigInt = BigInt.one,
      (p) => p.previewRequest!.handler = 'other',
      (p) => p.previewRequest!.outputType = 'other',
      (p) => p.previewRequest!.timeoutMs = 1,
      (p) => p.blobHistoryDeliveryPreview!.taskId =
          'history-v1.${hex(id(8))}.${hex(id(4))}',
      (p) => p.blobHistoryDeliveryPreview!.taskId =
          'history-v1.${hex(id(2))}.${'0' * 64}',
      (p) => p.blobHistoryDeliveryPreview!.packetSha256 = id(8),
      (p) => p.blobHistoryDeliveryPreview!.packet = Uint8List(8193),
    ];
    for (final mutate in mutations) {
      final peer = Peer()..corrupt = mutate;
      await expectLater(
        NativeBlobHistoryDeliveryClient(
          peer.exchange,
        ).prepareBlobHistoryDelivery(request()),
        throwsFormatException,
      );
      expect(peer.actions.length, 1);
    }
  });
  test(
    'key preview checks caller selection and does not prepare or approve',
    () async {
      final peer = Peer(),
          client = NativeBlobHistoryDeliveryClient(peer.exchange);
      final value = await client.previewBlobHistoryDelivery(id(9), request());
      expect(value.key, id(9));
      expect(peer.actions, [host.Action.blobHistoryDeliveryPreview]);
      await expectLater(
        client.previewBlobHistoryDelivery(id(7), request()),
        throwsFormatException,
      );
    },
  );
  test(
    'lost prepare cannot be resubmitted; explicit discovery remains separate',
    () async {
      final peer = Peer()..lost = true;
      final client = NativeBlobHistoryDeliveryClient(peer.exchange);
      await expectLater(
        client.prepareBlobHistoryDelivery(request()),
        throwsStateError,
      );
      expect(
        () => client.prepareBlobHistoryDelivery(request()),
        throwsStateError,
      );
      peer
        ..lost = false
        ..absent = true;
      expect(await client.discoverBlobHistoryDelivery(id(1)), isNull);
      expect(peer.actions, [
        host.Action.blobHistoryDeliveryPrepare,
        host.Action.blobHistoryDeliveryDiscover,
      ]);
    },
  );
  test(
    'approval carries reviewed digest and lost reply cannot cause a second dispatch',
    () async {
      final peer = Peer(),
          client = NativeBlobHistoryDeliveryClient(peer.exchange);
      final value = await client.prepareBlobHistoryDelivery(request());
      peer.lost = true;
      await expectLater(
        client.approveBlobHistoryDelivery(value),
        throwsStateError,
      );
      expect(peer.last!.blobHistoryDeliveryKey, value.key);
      expect(
        peer.last!.blobHistoryDeliveryApproval!.packetSha256,
        value.packetSha256,
      );
      expect(
        peer.last!.blobHistoryDeliveryApproval!.registryRevisionBigInt,
        revision,
      );
      expect(() => client.approveBlobHistoryDelivery(value), throwsStateError);
      peer.lost = false;
      final original = await client.readBlobHistoryDelivery(
        value.key,
        value.request.requestId,
        expectedOutputType: value.request.outputType,
      );
      expect(original.output, [7, 8, 9]);
      expect(original.fuelRemaining, revision);
      expect(() => original.output![0] = 0, throwsUnsupportedError);
      expect(
        peer.actions
            .where((a) => a == host.Action.blobHistoryDeliveryApprove)
            .length,
        1,
      );
      expect(peer.actions.last, host.Action.blobHistoryDeliveryRead);
    },
  );
  test('absent discovery and malformed reply are different outcomes', () async {
    final peer = Peer()..absent = true;
    final client = NativeBlobHistoryDeliveryClient(peer.exchange);
    expect(await client.discoverBlobHistoryDelivery(id(1)), isNull);
    peer.corrupt = (r) => r.blobHistoryDeliveryReceipt!.key = id(9);
    await expectLater(
      client.discoverBlobHistoryDelivery(id(1)),
      throwsFormatException,
    );
    peer.corrupt = (r) => r.blobHistoryDeliveryPreview!.taskId = 'hidden';
    await expectLater(
      client.discoverBlobHistoryDelivery(id(1)),
      throwsFormatException,
    );
    peer.corrupt = null;
    await expectLater(
      client.readBlobHistoryDelivery(id(9), id(1)),
      throwsFormatException,
    );
  });
  test(
    'receipt correlation and contradictory execution fields fail closed',
    () async {
      final mutations = <void Function(host.BlobHistoryDeliveryReceiptBuilder)>[
        (r) => r.key = id(5),
        (r) => r.requestId = id(5),
        (r) => r.close = 5,
        (r) => r.error = 13,
        (r) => r.deliveryError = 13,
        (r) => r.consumed = false,
        (r) => r.dispatched = false,
        (r) => r.hasExecution = false,
        (r) => r.executionFault = 10,
        (r) {
          r.executionFault = 1;
          r.exitCode = 1;
        },
        (r) => r.pluginFailureCode = 2,
        (r) => r.closeCoreError = 1,
        (r) => r.hasOutput = false,
        (r) => r.outputType = 'other',
        (r) => r.output = Uint8List(8193),
        (r) => r.deliveryError = 4,
        (r) => r.hostCalls = 1,
      ];
      for (final mutate in mutations) {
        final peer = Peer()
          ..corrupt = (r) => mutate(r.blobHistoryDeliveryReceipt!);
        await expectLater(
          NativeBlobHistoryDeliveryClient(
            peer.exchange,
          ).readBlobHistoryDelivery(
            id(9),
            id(1),
            expectedOutputType: request().outputType,
          ),
          throwsFormatException,
        );
        expect(peer.actions, [host.Action.blobHistoryDeliveryRead]);
      }
    },
  );
  test(
    'original execution error and original close refusal remain separate',
    () async {
      final peer = Peer()
        ..corrupt = (reply) {
          final r = reply.blobHistoryDeliveryReceipt!;
          r.hasOutput = false;
          r.output = Uint8List(0);
          r.outputType = '';
          r.error = 8;
          r.executionFault = 9;
          r.close = 3;
          r.closeFailureKind = 1;
          r.closeCoreError = 2;
          r.closeDetail = 'stale instance';
        };
      final client = NativeBlobHistoryDeliveryClient(peer.exchange);
      final original = await client.readBlobHistoryDelivery(id(9), id(1));
      expect(original.error, BlobHistoryDeliveryError.execution);
      expect(original.executionFault, IoExecutionFault.trap);
      expect(original.closeFailure!.coreError, 2);
      expect(original.closeFailure!.detail, 'stale instance');
      expect(original.canAcknowledge, isFalse);
      expect(
        () => client.acknowledgeBlobHistoryDelivery(original),
        throwsStateError,
      );
      expect(peer.actions, [host.Action.blobHistoryDeliveryRead]);
    },
  );
  test(
    'close unwind keeps Unknown without inventing revocation or acknowledgement',
    () async {
      final peer = Peer()
        ..corrupt = (reply) {
          final r = reply.blobHistoryDeliveryReceipt!;
          r.hasOutput = false;
          r.output = Uint8List(0);
          r.outputType = '';
          r.error = 12;
          r.close = 4;
          r.closeFailureKind = 2;
        };
      final client = NativeBlobHistoryDeliveryClient(peer.exchange);
      final original = await client.discoverBlobHistoryDelivery(id(1));
      expect(original!.close, BlobHistoryDeliveryClose.unknown);
      expect(original.closeFailure!.kind, BlobHistoryCloseFailureKind.unwind);
      expect(original.hasExecution, isTrue);
      expect(original.canAcknowledge, isFalse);
      expect(
        () => client.acknowledgeBlobHistoryDelivery(original),
        throwsStateError,
      );
      expect(peer.actions, [host.Action.blobHistoryDeliveryDiscover]);
    },
  );
  test(
    'invalid inputs fail before transport and do not consume a valid submission',
    () async {
      final peer = Peer(),
          client = NativeBlobHistoryDeliveryClient(peer.exchange);
      for (final bad in [
        request(timeout: 0),
        request(timeout: 120001),
        request(handler: '\u0085'),
      ]) {
        expect(
          () => client.prepareBlobHistoryDelivery(bad),
          throwsFormatException,
        );
      }
      expect(
        () => client.discoverBlobHistoryDelivery(id(0)),
        throwsFormatException,
      );
      expect(peer.actions, isEmpty);
      await client.prepareBlobHistoryDelivery(request());
      expect(peer.actions.length, 1);
    },
  );
  test(
    'a completed receipt is acknowledged only by an explicit action',
    () async {
      final peer = Peer(),
          client = NativeBlobHistoryDeliveryClient(peer.exchange);
      final original = await client.readBlobHistoryDelivery(id(9), id(1));
      expect(original.canAcknowledge, isTrue);
      expect(peer.actions, [host.Action.blobHistoryDeliveryRead]);
      final acknowledged = await client.acknowledgeBlobHistoryDelivery(
        original,
      );
      expect(acknowledged.acknowledged, isTrue);
      expect(acknowledged.canAcknowledge, isFalse);
      expect(peer.actions.last, host.Action.blobHistoryDeliveryAcknowledge);
    },
  );
  test(
    'approval and acknowledgement reject a response that did not apply that action',
    () async {
      final peer = Peer(),
          client = NativeBlobHistoryDeliveryClient(peer.exchange);
      final shown = await client.prepareBlobHistoryDelivery(request());
      peer.corrupt = (r) {
        final value = r.initBlobHistoryDeliveryReceipt();
        value.found = true;
        value.key = id(9);
        value.requestId = id(1);
      };
      await expectLater(
        client.approveBlobHistoryDelivery(shown),
        throwsFormatException,
      );
      peer.corrupt = null;
      final original = await client.readBlobHistoryDelivery(id(9), id(1));
      peer.corrupt = (r) => r.blobHistoryDeliveryReceipt!.acknowledged = false;
      await expectLater(
        client.acknowledgeBlobHistoryDelivery(original),
        throwsFormatException,
      );
    },
  );
  test(
    'acknowledgement cannot rewrite the original execution counters',
    () async {
      final peer = Peer(),
          client = NativeBlobHistoryDeliveryClient(peer.exchange);
      final original = await client.readBlobHistoryDelivery(id(9), id(1));
      peer.corrupt = (r) =>
          r.blobHistoryDeliveryReceipt!.fuelRemainingBigInt = BigInt.zero;
      await expectLater(
        client.acknowledgeBlobHistoryDelivery(original),
        throwsFormatException,
      );
      expect(peer.actions, [
        host.Action.blobHistoryDeliveryRead,
        host.Action.blobHistoryDeliveryAcknowledge,
      ]);
    },
  );
  test(
    'cancel is explicit and preserves an already completed original result',
    () async {
      final peer = Peer(),
          client = NativeBlobHistoryDeliveryClient(peer.exchange);
      final value = await client.cancelBlobHistoryDelivery(id(9), id(1));
      expect(value.output, [7, 8, 9]);
      expect(value.error, isNull);
      expect(peer.actions, [host.Action.blobHistoryDeliveryCancel]);
    },
  );
  test(
    'a not-created label cannot hide execution or manufacture a clean finish',
    () async {
      for (final close in [0, 2]) {
        final peer = Peer()
          ..corrupt = (reply) {
            final r = reply.blobHistoryDeliveryReceipt!;
            r.hasOutput = false;
            r.output = Uint8List(0);
            r.outputType = '';
            r.close = close;
          };
        await expectLater(
          NativeBlobHistoryDeliveryClient(
            peer.exchange,
          ).readBlobHistoryDelivery(id(9), id(1)),
          throwsFormatException,
        );
      }
    },
  );

  test('acknowledgement payload cannot replace original output', () async {
    final peer = Peer(),
        client = NativeBlobHistoryDeliveryClient(peer.exchange);
    final original = await client.readBlobHistoryDelivery(id(9), id(1));
    peer.corrupt = (reply) => reply.blobHistoryDeliveryReceipt!.output =
        Uint8List.fromList([1, 2, 3]);
    await expectLater(
      client.acknowledgeBlobHistoryDelivery(original),
      throwsFormatException,
    );
    expect(peer.actions, [
      host.Action.blobHistoryDeliveryRead,
      host.Action.blobHistoryDeliveryAcknowledge,
    ]);
  });
  test(
    'acknowledgement payload cannot replace original plugin failure',
    () async {
      final peer = Peer();
      void failure(ReplyParts reply, int code) {
        final r = reply.blobHistoryDeliveryReceipt!;
        r.hasOutput = false;
        r.output = Uint8List(0);
        r.outputType = '';
        r.error = 9;
        r.hasPluginFailure = true;
        r.pluginFailureCode = code;
      }

      peer.corrupt = (reply) => failure(reply, 1);
      final client = NativeBlobHistoryDeliveryClient(peer.exchange);
      final original = await client.readBlobHistoryDelivery(id(9), id(1));
      peer.corrupt = (reply) => failure(reply, 2);
      await expectLater(
        client.acknowledgeBlobHistoryDelivery(original),
        throwsFormatException,
      );
    },
  );
  test(
    'acknowledgement permits a current veto to hide original output',
    () async {
      final peer = Peer(),
          client = NativeBlobHistoryDeliveryClient(peer.exchange);
      final original = await client.readBlobHistoryDelivery(id(9), id(1));
      peer.corrupt = (reply) {
        final r = reply.blobHistoryDeliveryReceipt!;
        r.hasOutput = false;
        r.output = Uint8List(0);
        r.outputType = '';
        r.deliveryError = 3;
      };
      final receipt = await client.acknowledgeBlobHistoryDelivery(original);
      expect(receipt.acknowledged, isTrue);
      expect(receipt.output, isNull);
      expect(receipt.error, isNull);
      expect(receipt.deliveryError, isNotNull);
      expect(receipt.fuelRemaining, original.fuelRemaining);
      expect(original.output, [7, 8, 9]);
    },
  );

  test(
    'preview exposes exact public history without erasing original Unknown',
    () async {
      final peer = Peer(),
          client = NativeBlobHistoryDeliveryClient(peer.exchange);
      final shown = await client.prepareBlobHistoryDelivery(request());
      expect(shown.selectedHistory.originalUnknown, isTrue);
      expect(shown.selectedHistory.operationId, 'upload-原记录');
      expect(
        shown.selectedHistory.originalResult!.execution!.exitCode,
        -2147483648,
      );
      expect(peer.actions, [host.Action.blobHistoryDeliveryPrepare]);
    },
  );
  test(
    'hash-correct garbage and mismatched public nonce never become a preview',
    () async {
      for (final corrupt in <void Function(ReplyParts)>[
        (reply) {
          final p = reply.blobHistoryDeliveryPreview!;
          final invalid = Uint8List.fromList([1, 2, 3]);
          p.packet = invalid;
          p.packetSha256 = Uint8List.fromList(sha256.convert(invalid).bytes);
        },
        (reply) => reply.blobHistoryDeliveryPreview!.taskId =
            'history-v1.${hex(id(1))}.${hex(id(8))}',
      ]) {
        final peer = Peer()..corrupt = corrupt;
        await expectLater(
          NativeBlobHistoryDeliveryClient(
            peer.exchange,
          ).prepareBlobHistoryDelivery(request()),
          throwsFormatException,
        );
        expect(peer.actions, [host.Action.blobHistoryDeliveryPrepare]);
      }
    },
  );
}
