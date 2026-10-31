import 'dart:io';
import 'dart:typed_data';
import 'package:capnproto_dart/capnproto_dart.dart';
import 'package:test/test.dart';
import '../../../lib/plugins/blob_upload_history_packet.dart';
import '../../../lib/plugins/generated/blob_upload_history.capnp.dart' as wire;
import '../../../lib/plugins/generated/blob_upload_history_identity.dart';

Uint8List id(int n) => Uint8List.fromList(List.filled(32, n));
String hex(Uint8List value) =>
    value.map((n) => n.toRadixString(16).padLeft(2, '0')).join();
String taskId() => 'history-v1.${hex(id(1))}.${hex(id(9))}';
final vector = File(
  'packages/morrow_core_client/test/fixtures/history-unknown-acknowledged.capnp',
).readAsBytesSync();
Uint8List sample([void Function(wire.PacketBuilder)? change]) {
  final message = MessageBuilder(), p = message.initRoot(wire.packetFactory);
  p.version = 1;
  p.schemaSha256 = Uint8List.fromList(historySchemaSha256);
  p.queryNonce = id(9);
  p.submission = id(1);
  p.packageId = 'sample.history';
  p.packageDigest = id(2);
  p.registryRevisionBigInt = (BigInt.one << 64) - BigInt.one;
  p.operationId = '历史记录';
  p.wholeSha256 = id(0);
  p.handler = 'history.inspect';
  p.inputSha256 = id(0);
  p.requestBindingSha256 = id(3);
  p.reservedAtBigInt = -BigInt.one;
  p.revision = 1;
  p.originalUnknown = true;
  p.currentStage = wire.Stage.staged;
  change?.call(p);
  return message.serialize();
}

PublicHistoryPacket decode(Uint8List bytes) =>
    PublicHistoryPacket.decodeTask(bytes, taskId());
void observed(wire.PacketBuilder p) {
  p.revision = 2;
  p.outcome = wire.Outcome.error;
  p.errorCode = 7;
  p.observedAtBigInt = BigInt.zero;
}

void main() {
  test(
    'original Rust C C++ vector preserves unknown, signed exit and u64 maximum',
    () {
      final p = decode(vector), max = (BigInt.one << 64) - BigInt.one;
      expect(p.revision, 3);
      expect(p.originalUnknown, isTrue);
      expect(p.currentStage, PublicHistoryStage.staged);
      expect(p.operationId, 'upload-原记录');
      expect(p.byteLength, BigInt.from(16 * 1024 * 1024));
      expect(p.registryRevision, max);
      expect(p.originalResult!.wireBytes, max);
      expect(p.originalResult!.nativeWorkBytes, max);
      expect(p.originalResult!.calls, 0xffffffff);
      expect(p.originalResult!.errorCode, 7);
      expect(p.originalResult!.execution!.exitCode, -2147483648);
      expect(p.originalResult!.execution!.hostCalls, 0xffffffff);
      expect(p.reservedAt, BigInt.from(-10));
      expect(p.observedAt, BigInt.zero);
      expect(p.acknowledgedAt, BigInt.one);
    },
  );
  test(
    'reserved history remains unknown despite a currently staged object',
    () {
      final p = decode(sample());
      expect(p.currentStage, PublicHistoryStage.staged);
      expect(p.originalUnknown, isTrue);
      expect(p.originalResult, isNull);
      expect(p.observedAt, isNull);
      expect(p.acknowledgedAt, isNull);
      expect(p.wholeSha256, id(0));
      expect(p.inputSha256, id(0));
    },
  );
  test(
    'decoder owns identities after caller buffers are cleared and accepts unaligned input',
    () {
      final outer = Uint8List(vector.length + 1)
        ..setRange(1, vector.length + 1, vector);
      final p = decode(Uint8List.sublistView(outer, 1));
      outer.fillRange(0, outer.length, 0);
      expect(p.submission, id(1));
      expect(p.queryNonce, id(9));
      expect(() => p.submission[0] = 0, throwsUnsupportedError);
      expect(() => p.packageDigest[0] = 0, throwsUnsupportedError);
    },
  );
  test('task correlation must be independently exact and canonical', () {
    for (final bad in [
      taskId().toUpperCase(),
      '${taskId()}\u0000',
      'history-v1.${hex(id(2))}.${hex(id(9))}',
      'history-v1.${hex(id(1))}.${hex(id(8))}',
      'history-v1.${hex(id(0))}.${hex(id(9))}',
    ]) {
      expect(
        () => PublicHistoryPacket.decodeTask(vector, bad),
        throwsFormatException,
      );
    }
    expect(
      () => PublicHistoryPacket.decodeFor(vector, Uint8List(31), id(9)),
      throwsFormatException,
    );
  });
  test(
    'all truncations, trailing bytes and impossible segment tables reject',
    () {
      for (var n = 0; n < vector.length; n++) {
        expect(
          () => decode(Uint8List.sublistView(vector, 0, n)),
          throwsA(anything),
          reason: '$n',
        );
      }
      expect(
        () => decode(Uint8List.fromList([...vector, 0])),
        throwsFormatException,
      );
      expect(() => decode(Uint8List(8193)), throwsFormatException);
      final huge = Uint8List.fromList(vector);
      ByteData.sublistView(huge).setUint32(0, 0xffffffff, Endian.little);
      expect(() => decode(huge), throwsFormatException);
    },
  );
  test(
    'wrong schema, root shape, missing identity, control text and oversized object reject',
    () {
      for (final change in <void Function(wire.PacketBuilder)>[
        (p) => p.version = 2,
        (p) => p.schemaSha256 = id(9),
        (p) => p.packageDigest = id(0),
        (p) => p.requestBindingSha256 = id(0),
        (p) => p.packageId = 'path/name',
        (p) => p.operationId = 'x\u0085',
        (p) => p.handler = 'a:b',
        (p) => p.handler = 'x' * 257,
        (p) => p.byteLengthBigInt = BigInt.from(16 * 1024 * 1024 + 1),
      ]) {
        expect(() => decode(sample(change)), throwsFormatException);
      }
      final short = MessageBuilder();
      short.initDynamicRoot(dataWords: 10, pointerWords: 10);
      expect(() => decode(short.serialize()), throwsFormatException);
      final capability = Uint8List.fromList(vector);
      ByteData.sublistView(capability).setUint32(8, 3, Endian.little);
      expect(() => decode(capability), throwsA(anything));
    },
  );
  test(
    'observed and acknowledged records preserve original errors separately from current state',
    () {
      final a = decode(
        sample((p) {
          observed(p);
          p.currentStage = wire.Stage.released;
        }),
      );
      final b = decode(
        sample((p) {
          observed(p);
          p.revision = 3;
          p.acknowledgedAtBigInt = BigInt.one;
        }),
      );
      expect(a.originalResult!.errorCode, 7);
      expect(a.originalUnknown, isTrue);
      expect(a.currentStage, PublicHistoryStage.released);
      expect(b.originalUnknown, isTrue);
      expect(b.acknowledgedAt, BigInt.one);
    },
  );
  test(
    'inconsistent revision, timestamps, counters and execution cannot manufacture a result',
    () {
      for (final change in <void Function(wire.PacketBuilder)>[
        (p) => p.revision = 0,
        (p) => p.originalUnknown = false,
        (p) => p.calls = 1,
        (p) => p.wireBytesBigInt = BigInt.one,
        (p) => p.execution = wire.Execution.returned,
        (p) => p.errorCode = 1,
        (p) => p.observedAtBigInt = BigInt.one,
        (p) => p.revision = 2,
        (p) {
          observed(p);
          p.originalUnknown = false;
        },
        (p) {
          observed(p);
          p.errorCode = 11;
        },
        (p) {
          observed(p);
          p.observedAtBigInt = BigInt.from(-2);
        },
        (p) {
          observed(p);
          p.acknowledgedAtBigInt = BigInt.one;
        },
        (p) {
          observed(p);
          p.revision = 3;
          p.acknowledgedAtBigInt = -BigInt.one;
        },
        (p) {
          observed(p);
          p.execution = wire.Execution.rejected;
          p.executionError = 10;
        },
        (p) {
          observed(p);
          p.execution = wire.Execution.returned;
          p.executionError = 1;
        },
        (p) {
          observed(p);
          p.hostCalls = 1;
        },
      ]) {
        expect(() => decode(sample(change)), throwsFormatException);
      }
    },
  );
  test(
    'signed timestamp extrema and guest error range retain exact historical meanings',
    () {
      final p = decode(
        sample((p) {
          observed(p);
          p.reservedAtBigInt = -(BigInt.one << 63);
          p.observedAtBigInt = (BigInt.one << 63) - BigInt.one;
          p.execution = wire.Execution.rejected;
          p.executionError = 9;
          p.hostCalls = 0xffffffff;
        }),
      );
      expect(p.reservedAt, -(BigInt.one << 63));
      expect(p.observedAt, (BigInt.one << 63) - BigInt.one);
      expect(p.originalResult!.execution!.errorCode, 9);
      expect(p.originalResult!.execution!.exitCode, isNull);
    },
  );
}
