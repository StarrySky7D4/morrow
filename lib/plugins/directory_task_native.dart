import 'dart:typed_data';
import 'generated/host.capnp.dart' as host;
import 'directory_task_models.dart';
import 'file_task_native.dart' show FileTaskExchange;
import 'io_task_models.dart';
import 'io_task_codec_native.dart';

/// Private binary adapter. Every payload is copied before its reply is cleared.
final class NativeDirectoryTaskClient implements DirectoryTaskBackend {
  NativeDirectoryTaskClient(this._exchange);
  final FileTaskExchange _exchange;
  static bool _same(Uint8List? a, Uint8List b) =>
      a != null &&
      a.length == b.length &&
      Iterable<int>.generate(b.length).every((i) => a[i] == b[i]);
  IoTaskSnapshot _snapshot(host.ResponseReader r, {Uint8List? key}) {
    final value = HttpTaskCodec.snapshot(r.ioState);
    if (key != null && !_same(value.key, key)) {
      throw const FormatException('Directory task identity changed');
    }
    return value;
  }

  @override
  Future<IoTaskSnapshot> startDirectory(DirectoryTaskRequest value) {
    DirectoryTaskValidation.request(value);
    return _exchange(
      value.nativePicker
          ? host.Action.directoryPick
          : host.Action.directoryStart,
      clearReply: true,
      configure: (r) {
        final out = r.initDirectoryStart();
        out.submission = value.submission;
        out.packageId = value.packageId;
        out.packageDigest = value.packageDigest;
        out.registryRevisionBigInt = value.registryRevision;
        out.handler = value.handler;
        final units = value.selectedPath.codeUnits;
        final path = out.initSelectedPathUtf16(units.length);
        for (var i = 0; i < units.length; i++) {
          path[i] = units[i];
        }
        out.maxEntries = value.maxEntries;
        out.maxMetadataBytesBigInt = value.maxMetadataBytes;
        out.timeoutMs = value.timeoutMs;
      },
      decode: (r) {
        final result = _snapshot(r);
        if (result.key == null || !_same(result.submission, value.submission)) {
          throw const FormatException(
            'Directory submission acknowledgement changed',
          );
        }
        return result;
      },
    );
  }

  Future<IoTaskSnapshot> _command(
    host.Action action,
    Uint8List key, {
    int? timeout,
  }) {
    final expected = HttpTaskCodec.identity(key);
    return _exchange(
      action,
      clearReply: true,
      configure: (r) {
        r.ioKey = expected;
        if (timeout != null) r.directoryTimeoutMs = timeout;
      },
      decode: (r) => _snapshot(r, key: expected),
    );
  }

  @override
  Future<IoTaskSnapshot> runDirectoryGuest(Uint8List key, int timeoutMs) {
    DirectoryTaskValidation.timeout(timeoutMs);
    return _command(host.Action.directoryRunGuest, key, timeout: timeoutMs);
  }

  @override
  Future<IoTaskSnapshot> finishDirectory(Uint8List key) =>
      _command(host.Action.directoryFinish, key);
  @override
  Future<DirectoryTaskRead> readDirectory(Uint8List key) =>
      _read(host.Action.directoryRead, key, guest: false);
  @override
  Future<DirectoryTaskRead> readDirectoryGuest(Uint8List key) =>
      _read(host.Action.directoryReadGuest, key, guest: true);
  Future<DirectoryTaskRead> _read(
    host.Action action,
    Uint8List key, {
    required bool guest,
  }) {
    final expected = HttpTaskCodec.identity(key);
    return _exchange(
      action,
      clearReply: true,
      configure: (r) => r.ioKey = expected,
      decode: (r) => DirectoryTaskRead(
        snapshot: _snapshot(r, key: expected),
        result: decodeResult(r.directoryResult, guest: guest),
      ),
    );
  }

  static DirectoryTaskResult? decodeResult(
    host.DirectoryResultReader? row, {
    required bool guest,
  }) {
    if (row == null) throw const FormatException('Missing directory result');
    final frame = row.frame ?? Uint8List(0);
    final metadata = row.metadataBytesBigInt;
    if (row.kind > 4 ||
        (guest
            ? row.kind == 1 || row.kind == 3 || row.kind == 4
            : row.kind == 2) ||
        (row.kind == 4
            ? row.commandFault < 1 || row.commandFault > 3
            : row.commandFault != 0) ||
        row.entries > 1024 ||
        metadata > BigInt.from(1024 * 1024) ||
        frame.length > 65536 ||
        row.executionFault > 9 ||
        (row.executionFault != 0 && row.exitCode != 0) ||
        (row.kind != 1 && (row.entries != 0 || metadata != BigInt.zero)) ||
        (row.kind != 2 &&
            (frame.isNotEmpty ||
                row.unknown ||
                row.executionFault != 0 ||
                row.exitCode != 0 ||
                row.hostCalls != 0))) {
      throw const FormatException('Inconsistent directory result');
    }
    return switch (row.kind) {
      0 => null,
      1 => DirectoryTaskCaptured(row.entries, metadata),
      2 => DirectoryTaskGuest(
        frame: frame,
        unknown: row.unknown,
        executionFault: row.executionFault,
        exitCode: row.exitCode,
        hostCalls: row.hostCalls,
      ),
      3 => DirectoryTaskFinished(),
      _ => DirectoryTaskRefused(
        DirectoryCommandFault.values[row.commandFault - 1],
      ),
    };
  }
}
