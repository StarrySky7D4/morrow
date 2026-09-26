import 'dart:typed_data';
import 'dart:convert';
import 'io_task_models.dart';

Uint8List _owned(Uint8List bytes) =>
    Uint8List.fromList(bytes).asUnmodifiableView();

/// A fresh explicit native selection. Never automatically replay a lost start.
class FileTaskRequest {
  FileTaskRequest({
    required Uint8List submission,
    required this.packageId,
    required Uint8List packageDigest,
    required this.registryRevision,
    required this.handler,
    required this.selectedPath,
    required this.maxBytes,
    required this.timeoutMs,
  }) : submission = _owned(submission),
       packageDigest = _owned(packageDigest);
  final Uint8List submission, packageDigest;
  final String packageId, handler, selectedPath;
  final BigInt registryRevision, maxBytes;
  final int timeoutMs;
}

sealed class FileTaskResult {}

/// Metadata describes retained bytes, not an atomic source-file snapshot.
final class FileTaskCaptured extends FileTaskResult {
  FileTaskCaptured({required this.length, required Uint8List sha256})
    : sha256 = _owned(sha256);
  final BigInt length;
  final Uint8List sha256;
}

final class FileTaskChunk extends FileTaskResult {
  FileTaskChunk({
    required this.offset,
    required Uint8List bytes,
    required this.eof,
  }) : bytes = _owned(bytes);
  final BigInt offset;
  final Uint8List bytes;
  final bool eof;
}

final class FileTaskFinished extends FileTaskResult {}

class FileTaskRead {
  FileTaskRead({required this.snapshot, required this.result});
  final IoTaskSnapshot snapshot;
  final FileTaskResult? result;
}

/// Shares poll/cancel/repair/acknowledge with IoTaskBackend. One pending command
/// per task; consume Captured before Chunk/Finish. A lost read is never replayed.
abstract interface class FileTaskBackend {
  Future<IoTaskSnapshot> startFile(FileTaskRequest request);
  Future<IoTaskSnapshot> requestFileChunk(
    Uint8List key,
    BigInt offset,
    int limit,
  );
  Future<IoTaskSnapshot> finishFile(Uint8List key);
  Future<FileTaskRead> readFile(Uint8List key);
}

abstract final class FileTaskValidation {
  static final _u64 = (BigInt.one << 64) - BigInt.one;
  static final maxFileBytes = BigInt.from(256 * 1024 * 1024);
  static void validateRequest(FileTaskRequest value) {
    HttpTaskValidation.identity(value.submission);
    HttpTaskValidation.identity(value.packageDigest);
    bool text(String v, int max) =>
        v.isNotEmpty && !v.contains('\u0000') && utf8.encode(v).length <= max;
    if (!text(value.packageId, 256) ||
        !text(value.handler, 256) ||
        !text(value.selectedPath, 4096) ||
        value.registryRevision < BigInt.zero ||
        value.registryRevision > _u64 ||
        value.maxBytes < BigInt.zero ||
        value.maxBytes > maxFileBytes ||
        value.timeoutMs < 1 ||
        value.timeoutMs > 30000) {
      throw const FormatException('Invalid selected file request');
    }
  }
}

/// The trusted channel must advertise native selected-path support explicitly.
abstract interface class FileTaskPlatformCapabilities {
  bool get supportsSelectedFileTasks;
}
