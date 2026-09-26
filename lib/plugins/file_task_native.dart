import 'dart:typed_data';
import 'generated/host.capnp.dart' as host;
import 'file_task_models.dart';
import 'file_task_codec_native.dart';
import 'io_task_models.dart';
import 'io_task_codec_native.dart';

typedef FileTaskExchange =
    Future<T> Function<T>(
      host.Action action, {
      void Function(host.RequestBuilder)? configure,
      required T Function(host.ResponseReader) decode,
      required bool clearReply,
    });

/// Native file protocol client; models and codec can be checked without a UI.
final class NativeFileTaskClient implements FileTaskBackend {
  NativeFileTaskClient(this._exchange);
  final FileTaskExchange _exchange;
  static bool _same(Uint8List? a, Uint8List b) =>
      a != null &&
      a.length == b.length &&
      Iterable<int>.generate(b.length).every((i) => a[i] == b[i]);
  IoTaskSnapshot _ioSnapshot(host.ResponseReader reply, {Uint8List? key}) {
    final result = HttpTaskCodec.snapshot(reply.ioState);
    if (key != null && !_same(result.key, key)) {
      throw const FormatException('File task response identity changed');
    }
    return result;
  }

  @override
  Future<IoTaskSnapshot> startFile(FileTaskRequest request) {
    FileTaskCodec.validateRequest(request);
    return _exchange(
      host.Action.fileStart,
      configure: (r) => FileTaskCodec.writeRequest(request, r.initFileStart()),
      clearReply: true,
      decode: (reply) {
        final snapshot = _ioSnapshot(reply);
        if (snapshot.key == null ||
            !_same(snapshot.submission, request.submission)) {
          throw const FormatException(
            'File submission acknowledgement changed',
          );
        }
        return snapshot;
      },
    );
  }

  @override
  Future<IoTaskSnapshot> requestFileChunk(
    Uint8List key,
    BigInt offset,
    int limit,
  ) {
    final expected = HttpTaskCodec.identity(key);
    FileTaskCodec.validateChunk(offset, limit);
    return _exchange(
      host.Action.fileChunk,
      clearReply: true,
      configure: (r) {
        r.ioKey = expected;
        r.offsetBigInt = offset;
        r.limit = limit;
      },
      decode: (reply) => _ioSnapshot(reply, key: expected),
    );
  }

  @override
  Future<IoTaskSnapshot> finishFile(Uint8List key) {
    final expected = HttpTaskCodec.identity(key);
    return _exchange(
      host.Action.fileFinish,
      configure: (r) => r.ioKey = expected,
      clearReply: true,
      decode: (reply) => _ioSnapshot(reply, key: expected),
    );
  }

  @override
  Future<FileTaskRead> readFile(Uint8List key) {
    final expected = HttpTaskCodec.identity(key);
    return _exchange(
      host.Action.fileRead,
      configure: (r) => r.ioKey = expected,
      clearReply: true,
      decode: (reply) => FileTaskRead(
        snapshot: _ioSnapshot(reply, key: expected),
        result: FileTaskCodec.result(reply.fileResult),
      ),
    );
  }
}
