import 'dart:typed_data';
import 'generated/host.capnp.dart' as host;
import 'file_task_models.dart';

abstract final class FileTaskCodec {
  static final _u64 = (BigInt.one << 64) - BigInt.one;
  static final maxFileBytes = BigInt.from(256 * 1024 * 1024);

  static void validateChunk(BigInt offset, int limit) {
    if (offset < BigInt.zero || offset > _u64 || limit < 0 || limit > 65536) {
      throw const FormatException('Invalid file chunk bounds');
    }
  }

  static void validateRequest(FileTaskRequest value) =>
      FileTaskValidation.validateRequest(value);

  static void writeRequest(FileTaskRequest value, host.FileStartBuilder out) {
    validateRequest(value);
    out.submission = value.submission;
    out.packageId = value.packageId;
    out.packageDigest = value.packageDigest;
    out.registryRevisionBigInt = value.registryRevision;
    out.handler = value.handler;
    out.selectedPath = value.selectedPath;
    out.maxBytesBigInt = value.maxBytes;
    out.timeoutMs = value.timeoutMs;
  }

  static FileTaskResult? result(host.FileResultReader? row) {
    if (row == null) throw const FormatException('Missing file result');
    final hash = row.sha256 ?? Uint8List(0);
    final bytes = row.bytes ?? Uint8List(0);
    final length = row.lengthBigInt, offset = row.offsetBigInt;
    if (row.kind > 3 ||
        length > maxFileBytes ||
        bytes.length > 65536 ||
        (row.kind != 1 && (length != BigInt.zero || hash.isNotEmpty)) ||
        (row.kind != 2 &&
            (offset != BigInt.zero || bytes.isNotEmpty || row.eof))) {
      throw const FormatException('Inconsistent file result');
    }
    switch (row.kind) {
      case 0:
        return null;
      case 1:
        if (hash.length != 32)
          throw const FormatException('Invalid file digest');
        return FileTaskCaptured(length: length, sha256: hash);
      case 2:
        if (offset + BigInt.from(bytes.length) > _u64 ||
            (bytes.isEmpty && !row.eof)) {
          throw const FormatException('Invalid file chunk');
        }
        return FileTaskChunk(offset: offset, bytes: bytes, eof: row.eof);
      default:
        return FileTaskFinished();
    }
  }
}
