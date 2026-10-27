import 'dart:io';
import 'dart:convert';
import 'dart:typed_data';
import 'package:capnproto_dart/capnproto_dart.dart';
import 'package:crypto/crypto.dart';
import 'package:test/test.dart';
import '../../../lib/plugins/generated/host.capnp.dart' as host;
import '../../../lib/plugins/generated/identity.dart' as identity;
import '../../../lib/plugins/directory_task_models.dart';
import '../../../lib/plugins/directory_task_native.dart';

Uint8List id(int n) => Uint8List.fromList(List.filled(32, n));
DirectoryTaskRequest request({
  bool native = true,
  String path = '',
  int timeout = 10000,
}) => DirectoryTaskRequest(
  submission: id(1),
  packageId: 'test.directory',
  packageDigest: id(2),
  registryRevision: BigInt.parse('9007199254740993'),
  handler: 'directory.immediate-selected',
  selectedPath: path,
  nativePicker: native,
  maxEntries: 1024,
  maxMetadataBytes: BigInt.from(1048576),
  timeoutMs: timeout,
);

class Peer {
  final actions = <host.Action>[];
  host.RequestReader? last;
  bool fail = false, wrongSubmission = false;
  Future<T> exchange<T>(
    host.Action action, {
    void Function(host.RequestBuilder)? configure,
    required T Function(host.ResponseReader) decode,
    required bool clearReply,
  }) async {
    actions.add(action);
    final input = MessageBuilder();
    final row = input.initRoot(host.requestFactory)..action = action;
    configure?.call(row);
    last = MessageReader.deserialize(
      input.serialize(),
    ).getRoot(host.requestFactory);
    if (fail) throw StateError('lost reply');
    final output = MessageBuilder();
    final state = output.initRoot(host.responseFactory).initIoState();
    state.key = id(3);
    state.submission = id(wrongSubmission ? 9 : 1);
    state.storage = 1;
    state.delivery = 1;
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
  test(
    'native picker sends metadata only and preserves exact uint64 revision',
    () async {
      final p = Peer(), value = request();
      final result = await NativeDirectoryTaskClient(
        p.exchange,
      ).startDirectory(value);
      expect(p.actions, [host.Action.directoryPick]);
      expect(p.last!.directoryStart!.selectedPathUtf16!.length, 0);
      expect(
        p.last!.directoryStart!.registryRevisionBigInt,
        value.registryRevision,
      );
      expect(p.last!.directoryStart!.packageDigest, id(2));
      expect(result.key, id(3));
      expect(result.submission, id(1));
      expect(() => result.key![0] = 0, throwsUnsupportedError);
    },
  );
  test(
    'caller path and invalid timeout are refused before picker dispatch',
    () {
      final p = Peer(), client = NativeDirectoryTaskClient(Peer().exchange);
      final bound = NativeDirectoryTaskClient(p.exchange);
      for (final r in [
        request(path: r'C:\caller'),
        request(path: '\u0000'),
        request(timeout: 0),
        request(timeout: 30001),
      ]) {
        expect(() => bound.startDirectory(r), throwsFormatException);
      }
      expect(
        () => client.startDirectory(request(native: false)),
        throwsFormatException,
      );
      expect(p.actions, isEmpty);
    },
  );
  test(
    'explicit fixture path lane retains its old ordinal and UTF16 units',
    () async {
      final p = Peer();
      const path = 'C:\\folder 😀';
      await NativeDirectoryTaskClient(
        p.exchange,
      ).startDirectory(request(native: false, path: path));
      expect(p.actions, [host.Action.directoryStart]);
      expect(
        p.last!.directoryStart!.selectedPathUtf16!.toList(),
        path.codeUnits,
      );
    },
  );
  test(
    'picker acknowledgement cannot substitute a different submission',
    () async {
      final p = Peer()..wrongSubmission = true;
      await expectLater(
        NativeDirectoryTaskClient(p.exchange).startDirectory(request()),
        throwsFormatException,
      );
      expect(p.actions.length, 1);
    },
  );
  test('lost native picker reply is not replayed', () async {
    final p = Peer()..fail = true;
    await expectLater(
      NativeDirectoryTaskClient(p.exchange).startDirectory(request()),
      throwsStateError,
    );
    expect(p.actions, [host.Action.directoryPick]);
  });
  test(
    'new picker ordinal appends without moving old actions and matches schema',
    () {
      expect(host.Action.directoryStart.index, 156);
      expect(host.Action.directoryReadGuest.index, 159);
      expect(host.Action.directoryFinish.index, 160);
      expect(host.Action.directoryPick.index, 161);
      final schema = File(
        '../../workbench_host/schemas/host.capnp',
      ).readAsStringSync().replaceAll('\r\n', '\n');
      expect(sha256.convert(utf8.encode(schema)).bytes, identity.hostDigest);
    },
  );
}
