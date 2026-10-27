import 'dart:convert';
import 'dart:typed_data';
import 'io_task_models.dart';

Uint8List _owned(Uint8List value) =>
    Uint8List.fromList(value).asUnmodifiableView();

/// One explicit capture request. Native picker requests carry no caller path.
class DirectoryTaskRequest {
  DirectoryTaskRequest({
    required Uint8List submission,
    required this.packageId,
    required Uint8List packageDigest,
    required this.registryRevision,
    required this.handler,
    required this.selectedPath,
    required this.maxEntries,
    required this.maxMetadataBytes,
    required this.timeoutMs,
    this.nativePicker = false,
  }) : submission = _owned(submission),
       packageDigest = _owned(packageDigest);
  final Uint8List submission, packageDigest;
  final String packageId, handler, selectedPath;
  final BigInt registryRevision, maxMetadataBytes;
  final int maxEntries, timeoutMs;
  final bool nativePicker;
}

sealed class DirectoryTaskResult {}

final class DirectoryTaskCaptured extends DirectoryTaskResult {
  DirectoryTaskCaptured(this.entries, this.metadataBytes);
  final int entries;
  final BigInt metadataBytes;
}

final class DirectoryTaskGuest extends DirectoryTaskResult {
  DirectoryTaskGuest({
    required Uint8List frame,
    required this.unknown,
    required this.executionFault,
    required this.exitCode,
    required this.hostCalls,
  }) : frame = _owned(frame);
  final Uint8List frame;
  final bool unknown;
  final int executionFault, exitCode, hostCalls;
  bool get succeeded =>
      !unknown && executionFault == 0 && exitCode == 0 && frame.isNotEmpty;
}

final class DirectoryTaskFinished extends DirectoryTaskResult {}

enum DirectoryCommandFault { cancelled, expired, rejected }

/// A definitive original capture refusal; worker reclamation remains separate.
final class DirectoryTaskRefused extends DirectoryTaskResult {
  DirectoryTaskRefused(this.fault);
  final DirectoryCommandFault fault;
}

class DirectoryTaskRead {
  DirectoryTaskRead({required this.snapshot, required this.result});
  final IoTaskSnapshot snapshot;
  final DirectoryTaskResult? result;
}

/// Original IO poll/cancel/repair/acknowledge remain separate from once-only
/// result delivery. No new owner, task retry or private guest grant is exposed.
abstract interface class DirectoryTaskBackend {
  Future<IoTaskSnapshot> startDirectory(DirectoryTaskRequest request);
  Future<IoTaskSnapshot> runDirectoryGuest(Uint8List key, int timeoutMs);
  Future<IoTaskSnapshot> finishDirectory(Uint8List key);
  Future<DirectoryTaskRead> readDirectory(Uint8List key);
  Future<DirectoryTaskRead> readDirectoryGuest(Uint8List key);
}

abstract interface class DirectoryTaskPlatformCapabilities {
  bool get supportsSelectedDirectoryTasks;
}

/// A trusted picker captures directory identity inside the original IO owner.
abstract interface class NativeDirectoryPickerCapabilities {
  bool get supportsNativeDirectoryPicker;
}

abstract final class DirectoryTaskValidation {
  static final _u64 = (BigInt.one << 64) - BigInt.one;
  static void timeout(int value) {
    if (value < 1 || value > 30000) {
      throw const FormatException('Invalid directory timeout');
    }
  }

  static void request(DirectoryTaskRequest value) {
    HttpTaskValidation.identity(value.submission);
    HttpTaskValidation.identity(value.packageDigest);
    timeout(value.timeoutMs);
    bool text(String v) =>
        v.isNotEmpty && !v.contains('\u0000') && utf8.encode(v).length <= 256;
    if (!text(value.packageId) ||
        !text(value.handler) ||
        (value.nativePicker
            ? value.selectedPath.isNotEmpty
            : value.selectedPath.isEmpty) ||
        value.selectedPath.length > 4096 ||
        value.selectedPath.contains('\u0000') ||
        value.registryRevision < BigInt.zero ||
        value.registryRevision > _u64 ||
        value.maxEntries < 0 ||
        value.maxEntries > 1024 ||
        value.maxMetadataBytes < BigInt.zero ||
        value.maxMetadataBytes > BigInt.from(1024 * 1024)) {
      throw const FormatException('Invalid selected directory request');
    }
  }
}
