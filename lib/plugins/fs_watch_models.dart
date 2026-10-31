import 'dart:typed_data';
import 'dart:convert';
import 'io_task_models.dart';

bool fsWatchSame(List<int>? a, List<int>? b) =>
    a != null &&
    b != null &&
    a.length == b.length &&
    Iterable<int>.generate(a.length).every((i) => a[i] == b[i]);

final class FsWatchLimits {
  FsWatchLimits({
    required this.frames,
    required this.wireBytes,
    required this.windowMs,
  }) {
    if (frames < 1 ||
        frames > 64 ||
        wireBytes < BigInt.one ||
        wireBytes > BigInt.from(4 * 1024 * 1024) ||
        windowMs < 1 ||
        windowMs > 30000) {
      throw const FormatException('Invalid bounded directory observation');
    }
  }
  final int frames, windowMs;
  final BigInt wireBytes;
}

/// Owned host nomination. A guest cannot replace it and a result cannot renew it.
final class FsWatchRequest {
  FsWatchRequest({
    required Uint8List submission,
    required this.packageId,
    required Uint8List digest,
    required this.revision,
    required this.handler,
    required this.path,
    required this.limits,
  }) : submission = HttpTaskValidation.identity(submission),
       digest = HttpTaskValidation.identity(digest) {
    FsWatchValidation.selection(packageId, this.digest, revision);
    FsWatchValidation.handler(handler);
    if (path.codeUnits.length < 3 ||
        path.codeUnits.length > 4096 ||
        path.codeUnits.contains(0) ||
        !RegExp(r'^[A-Za-z]:\\').hasMatch(path)) {
      throw const FormatException('Invalid original directory nomination');
    }
  }
  final Uint8List submission, digest;
  final String packageId, handler, path;
  final BigInt revision;
  final FsWatchLimits limits;
}

abstract final class FsWatchValidation {
  static void handler(String value) {
    if (value.isEmpty ||
        utf8.encode(value).length > 256 ||
        value.codeUnits.any((u) => u < 0x20 || (u >= 0x7f && u <= 0x9f)) ||
        RegExp(r'[/\\:]').hasMatch(value)) {
      throw const FormatException('Invalid observation handler');
    }
  }

  static void selection(String packageId, Uint8List digest, BigInt revision) {
    HttpTaskValidation.identity(digest);
    if (packageId.isEmpty ||
        packageId.length > 256 ||
        !RegExp(r'^[A-Za-z0-9_.-]+$').hasMatch(packageId) ||
        revision < BigInt.zero ||
        revision > (BigInt.one << 64) - BigInt.one) {
      throw const FormatException('Invalid observation package selection');
    }
  }
}

enum FsWatchPhase { absent, starting, observing, finishing, finished }

enum FsWatchError {
  none,
  approval,
  native,
  guest,
  owner,
  protocol,
  terminalObservation,
  cancelled,
  expired,
  spawn,
  unknown,
  consumed,
}

final class FsWatchSnapshot {
  const FsWatchSnapshot({
    required this.task,
    required this.ready,
    required this.delivered,
    required this.phase,
    required this.frames,
  });
  final IoTaskSnapshot task;
  final bool ready, delivered;
  final FsWatchPhase phase;
  final int frames;
}

final class FsWatchDelivery {
  FsWatchDelivery({
    required this.sequence,
    required Uint8List digest,
    required this.executionFault,
    required this.exitCode,
    required this.hostCalls,
    required this.fuelRemaining,
  }) : digest = HttpTaskValidation.identity(digest);
  final BigInt sequence, fuelRemaining;
  final Uint8List digest;
  final IoExecutionFault executionFault;
  final int exitCode, hostCalls;
}

final class FsWatchResult {
  FsWatchResult({
    required this.success,
    required this.error,
    required this.detail,
    required this.unknown,
    required this.workerJoined,
    required this.nativeRetirementComplete,
    required this.disconnect,
    required this.maintenance,
    required this.resources,
    required this.jobs,
    required this.chargedBytes,
    required List<FsWatchDelivery> deliveries,
    required this.hasSnapshot,
    required this.pickerStarted,
    required this.pickerClosed,
    required this.ancestorObjects,
    required this.ancestorClosed,
    required this.hasNative,
    required this.nativeHandles,
    required this.nativeClosed,
    required this.cancelRequested,
  }) : deliveries = List.unmodifiable(deliveries);
  final bool success, unknown, workerJoined, nativeRetirementComplete;
  final FsWatchError error;
  final String detail;
  final IoJobError? disconnect, maintenance;
  final int resources, jobs;
  final BigInt chargedBytes;
  final List<FsWatchDelivery> deliveries;
  final bool hasSnapshot,
      pickerStarted,
      pickerClosed,
      hasNative,
      cancelRequested;
  final int ancestorObjects, ancestorClosed, nativeHandles, nativeClosed;
  bool get knownClosed =>
      !unknown &&
      nativeRetirementComplete &&
      disconnect == IoJobError.none &&
      maintenance == IoJobError.none &&
      resources == 0 &&
      jobs == 0;
}

final class FsWatchReply {
  const FsWatchReply(this.snapshot, this.result);
  final FsWatchSnapshot snapshot;
  final FsWatchResult? result;
}

abstract interface class FsWatchBackend {
  Future<void> approveFsWatch(
    String packageId,
    Uint8List digest,
    BigInt revision,
    bool approved,
  );
  Future<FsWatchSnapshot> startFsWatch(FsWatchRequest request);
  Future<FsWatchSnapshot> pollFsWatch(Uint8List key);
  Future<FsWatchReply> readFsWatch(Uint8List key);
}

abstract interface class FsWatchPlatformCapabilities {
  bool get supportsFsWatch;
}
