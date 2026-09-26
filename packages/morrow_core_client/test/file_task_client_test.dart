import 'dart:io';
import 'dart:typed_data';
import 'package:capnproto_dart/capnproto_dart.dart';
import 'package:crypto/crypto.dart';
import 'package:test/test.dart';
import '../../../lib/plugins/generated/host.capnp.dart' as host;
import '../../../lib/plugins/generated/identity.dart' as identity;
import '../../../lib/plugins/file_task_models.dart';
import '../../../lib/plugins/file_task_codec_native.dart';
import '../../../lib/plugins/file_task_native.dart';

Uint8List id(int n) => Uint8List.fromList(List.filled(32, n));
FileTaskRequest request({
  int timeout = 1000,
  BigInt? revision,
  BigInt? maxBytes,
}) => FileTaskRequest(
  submission: id(1),
  packageId: 'org.example.file',
  packageDigest: id(2),
  registryRevision: revision ?? BigInt.parse('9007199254740993'),
  handler: 'file.read-selected',
  selectedPath: '/chosen/source',
  maxBytes: maxBytes ?? BigInt.from(90007),
  timeoutMs: timeout,
);

class Peer {
  final actions = <host.Action>[];
  host.RequestReader? last;
  void Function(host.ResponseBuilder)? reply;
  bool fail = false;
  Future<T> exchange<T>(
    host.Action action, {
    void Function(host.RequestBuilder)? configure,
    required T Function(host.ResponseReader) decode,
    required bool clearReply,
  }) async {
    actions.add(action);
    final input = MessageBuilder().initRoot(host.requestFactory);
    input.action = action;
    configure?.call(input);
    last = input.asReader();
    if (fail) throw StateError('lost transport reply');
    final output = MessageBuilder();
    final row = output.initRoot(host.responseFactory);
    final state = row.initIoState();
    state.key = id(3);
    state.submission = id(1);
    state.storage = 1;
    state.delivery = 2;
    if (action == host.Action.fileRead) row.initFileResult().kind = 0;
    reply?.call(row);
    final frame = output.serialize();
    try {
      return decode(
        MessageReader.deserialize(frame).getRoot(host.responseFactory),
      );
    } finally {
      if (clearReply) frame.fillRange(0, frame.length, 0);
    }
  }
}

void main() {
  test('start binds immutable identity and exact uint64 revision', () async {
    final peer = Peer(), r = request();
    final client = NativeFileTaskClient(peer.exchange);
    expect((await client.startFile(r)).key, id(3));
    expect(
      peer.last!.fileStart!.registryRevisionBigInt,
      BigInt.parse('9007199254740993'),
    );
    expect(peer.last!.fileStart!.selectedPath, '/chosen/source');
    expect(() => r.submission[0] = 7, throwsUnsupportedError);
    peer.reply = (r) => (r.initIoState()..key = id(3)..storage = 1).submission = id(8);
    await expectLater(client.startFile(request()), throwsFormatException);
  });
  test('bounds rejected before sending, including integer narrowing', () async {
    final peer = Peer();
    final client = NativeFileTaskClient(peer.exchange);
    for (final value in [
      request(timeout: 0),
      request(timeout: 30001),
      request(revision: BigInt.one << 64),
      request(maxBytes: BigInt.from(-1)),
      request(maxBytes: FileTaskCodec.maxFileBytes + BigInt.one),
    ]) {
      expect(() => client.startFile(value), throwsFormatException);
    }
    for (final pair in [
      (BigInt.from(-1), 1),
      (BigInt.one << 64, 1),
      (BigInt.zero, 65537),
    ]) {
      expect(
        () => client.requestFileChunk(id(3), pair.$1, pair.$2),
        throwsFormatException,
      );
    }
    expect(peer.actions, isEmpty);
  });
  test('chunk and finish correlate keys and keep exact offsets', () async {
    final peer = Peer();
    final client = NativeFileTaskClient(peer.exchange);
    final offset = BigInt.parse('9007199254740993');
    await client.requestFileChunk(id(3), offset, 65536);
    expect(peer.last!.offsetBigInt, offset);
    expect(peer.last!.limit, 65536);
    await client.finishFile(id(3));
    expect(peer.actions.last, host.Action.fileFinish);
    peer.reply = (r) => (r.initIoState()..storage = 1).key = id(5);
    await expectLater(client.readFile(id(3)), throwsFormatException);
  });
  test('once-only read detaches bytes before private reply wipe', () async {
    final peer = Peer();
    final client = NativeFileTaskClient(peer.exchange);
    peer.reply = (r) {
      final row = r.initFileResult();
      row.kind = 2;
      row.offsetBigInt = BigInt.from(5);
      row.bytes = Uint8List.fromList([0, 255, 13, 10]);
      row.eof = true;
    };
    final value = (await client.readFile(id(3))).result! as FileTaskChunk;
    expect(value.bytes, [0, 255, 13, 10]);
    expect(value.offset, BigInt.from(5));
    expect(() => value.bytes[0] = 1, throwsUnsupportedError);
    peer.reply = (r) => r.initFileResult().kind = 3;
    expect((await client.readFile(id(3))).result, isA<FileTaskFinished>());
    peer.reply = null;
    expect((await client.readFile(id(3))).result, isNull);
  });
  test('ambiguous transport failures never replay starts or reads', () async {
    final peer = Peer()..fail = true;
    final client = NativeFileTaskClient(peer.exchange);
    await expectLater(client.startFile(request()), throwsStateError);
    await expectLater(client.readFile(id(3)), throwsStateError);
    expect(peer.actions, [host.Action.fileStart, host.Action.fileRead]);
  });
  test(
    'malformed union shapes and oversized or overflowing chunks fail closed',
    () {
      void bad(void Function(host.FileResultBuilder) configure) {
        final row = MessageBuilder().initRoot(host.fileResultFactory);
        configure(row);
        expect(
          () => FileTaskCodec.result(row.asReader()),
          throwsFormatException,
        );
      }

      expect(() => FileTaskCodec.result(null), throwsFormatException);
      bad((r) => r.kind = 4);
      bad((r) {
        r.kind = 1;
        r.sha256 = id(2);
        r.bytes = id(1);
      });
      bad((r) {
        r.kind = 1;
        r.sha256 = Uint8List(31);
      });
      bad((r) {
        r.kind = 1;
        r.sha256 = id(2);
        r.lengthBigInt = BigInt.one << 40;
      });
      bad((r) {
        r.kind = 0;
        r.eof = true;
      });
      bad((r) {
        r.kind = 3;
        r.offset = 1;
      });
      bad((r) {
        r.kind = 2;
      });
      bad((r) {
        r.kind = 2;
        r.bytes = Uint8List(65537);
      });
      bad((r) {
        r.kind = 2;
        r.offsetBigInt = (BigInt.one << 64) - BigInt.one;
        r.bytes = id(1);
      });
    },
  );
  final fixtures = Platform.environment['MORROW_FILE_WIRE_FIXTURES'];
  test(
    'real Rust private frames from original Rust C C++ guests decode unchanged',
    () {
      for (final language in ['rust', 'c', 'cpp']) {
        host.ResponseReader read(String name) {
          final bytes = File('$fixtures/$language-$name.bin').readAsBytesSync();
          final row = MessageReader.deserialize(
            bytes,
          ).getRoot(host.responseFactory);
          expect(row.digest, identity.hostDigest);
          expect(row.version, 1);
          expect(row.error ?? '', isEmpty);
          return row;
        }

        final captured =
            FileTaskCodec.result(read('captured').fileResult)!
                as FileTaskCaptured;
        final a =
            FileTaskCodec.result(read('chunk-0').fileResult)! as FileTaskChunk;
        final b =
            FileTaskCodec.result(read('chunk-65536').fileResult)!
                as FileTaskChunk;
        final bytes = [...a.bytes, ...b.bytes];
        expect(captured.length, BigInt.from(90007));
        expect(a.offset, BigInt.zero);
        expect(a.eof, isFalse);
        expect(b.offset, BigInt.from(65536));
        expect(b.eof, isTrue);
        expect(bytes, List.generate(90007, (i) => i % 251));
        expect(captured.sha256, sha256.convert(bytes).bytes);
      }
    },
    skip: fixtures == null
        ? 'Set MORROW_FILE_WIRE_FIXTURES to Rust-produced fixtures'
        : false,
  );
}
