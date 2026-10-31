import 'dart:typed_data';
import 'file_task_native.dart' show FileTaskExchange;
import 'fs_watch_models.dart';
import 'generated/host.capnp.dart' as host;
import 'io_task_codec_native.dart';
import 'io_task_models.dart';

/// One private Cap'n Proto exchange per command; no inferred retry or renewal.
final class NativeFsWatchClient implements FsWatchBackend {
  NativeFsWatchClient(this._exchange);
  final FileTaskExchange _exchange;
  @override
  Future<void> approveFsWatch(
    String packageId,
    Uint8List digest,
    BigInt revision,
    bool approved,
  ) {
    FsWatchValidation.selection(packageId, digest, revision);
    final exact = HttpTaskValidation.identity(digest);
    return _exchange(
      host.Action.pluginApproveFsWatch,
      clearReply: true,
      configure: (r) {
        r.id = packageId;
        r.sha256 = exact;
        r.revisionBigInt = revision;
        r.fsWatchApproved = approved;
      },
      decode: (_) {},
    );
  }

  @override
  Future<FsWatchSnapshot> startFsWatch(FsWatchRequest value) => _exchange(
    host.Action.fsWatchStart,
    clearReply: true,
    configure: (r) {
      final out = r.initFsWatchStart();
      out.submission = value.submission;
      out.packageId = value.packageId;
      out.packageDigest = value.digest;
      out.registryRevisionBigInt = value.revision;
      out.handler = value.handler;
      final units = value.path.codeUnits;
      final bytes = ByteData(units.length * 2);
      for (var i = 0; i < units.length; i++) {
        bytes.setUint16(i * 2, units[i], Endian.little);
      }
      out.nominatedPathUtf16Le = bytes.buffer.asUint8List();
      out.maxFrames = value.limits.frames;
      out.maxWireBytesBigInt = value.limits.wireBytes;
      out.windowMs = value.limits.windowMs;
    },
    decode: (r) {
      final reply = decode(r, allowResult: false);
      if (reply.snapshot.task.key == null ||
          !fsWatchSame(reply.snapshot.task.submission, value.submission)) {
        throw const FormatException('Original observation submission changed');
      }
      return reply.snapshot;
    },
  );
  Future<FsWatchReply> _command(host.Action action, Uint8List key, bool read) {
    final exact = HttpTaskValidation.identity(key);
    return _exchange(
      action,
      clearReply: true,
      configure: (r) => r.ioKey = exact,
      decode: (r) {
        final result = decode(r, allowResult: read);
        if (!fsWatchSame(result.snapshot.task.key, exact)) {
          throw const FormatException('Original observation identity changed');
        }
        return result;
      },
    );
  }

  @override
  Future<FsWatchSnapshot> pollFsWatch(Uint8List key) async =>
      (await _command(host.Action.fsWatchStatus, key, false)).snapshot;
  @override
  Future<FsWatchReply> readFsWatch(Uint8List key) =>
      _command(host.Action.fsWatchRead, key, true);

  static T _enum<T>(List<T> values, int code) {
    if (code < 0 || code >= values.length)
      throw const FormatException('Unknown observation code');
    return values[code];
  }

  static FsWatchReply decode(
    host.ResponseReader reply, {
    required bool allowResult,
  }) {
    final task = HttpTaskCodec.snapshot(reply.ioState);
    final state = reply.fsWatchState;
    if (state == null ||
        task.key == null ||
        task.submission == null ||
        state.frames > 64 ||
        (state.ready && state.delivered) ||
        ((state.ready || state.delivered) && task.exit == null)) {
      throw const FormatException('Invalid original observation state');
    }
    final snapshot = FsWatchSnapshot(
      task: task,
      ready: state.ready,
      delivered: state.delivered,
      phase: _enum(FsWatchPhase.values, state.phase),
      frames: state.frames,
    );
    final row = reply.fsWatchResult;
    if (row == null) return FsWatchReply(snapshot, null);
    if (!row.present) {
      if (row.success ||
          row.error != 0 ||
          (row.detail?.isNotEmpty ?? false) ||
          row.unknown ||
          row.originalWorkerJoined ||
          row.nativeRetirementComplete ||
          row.hasDisconnect ||
          row.disconnect != 0 ||
          row.hasMaintenance ||
          row.maintenance != 0 ||
          row.resources != 0 ||
          row.jobs != 0 ||
          row.chargedBytesBigInt != BigInt.zero ||
          (row.deliveries?.isNotEmpty ?? false) ||
          row.hasSnapshot ||
          row.pickerStarted ||
          row.pickerClosed ||
          row.ancestorObjects != 0 ||
          row.ancestorClosed != 0 ||
          row.hasNative ||
          row.nativeHandles != 0 ||
          row.nativeClosed != 0 ||
          row.cancelRequested) {
        throw const FormatException('Observation data without a result');
      }
      return FsWatchReply(snapshot, null);
    }
    final detail = row.detail ?? '';
    if (!allowResult ||
        !state.delivered ||
        state.ready ||
        detail.length > 1024 ||
        row.success != (row.error == 0) ||
        (row.success &&
            (row.unknown || !row.originalWorkerJoined || !row.hasNative)) ||
        (!row.originalWorkerJoined &&
            (row.error != FsWatchError.spawn.index || row.hasSnapshot)) ||
        (!row.nativeRetirementComplete && !row.unknown) ||
        (row.success && detail.isNotEmpty) ||
        (!row.success && detail.isEmpty) ||
        (!row.hasDisconnect && row.disconnect != 0) ||
        (!row.hasMaintenance && row.maintenance != 0) ||
        row.ancestorObjects > 33 ||
        row.ancestorClosed > row.ancestorObjects ||
        row.nativeHandles > 2 ||
        row.nativeClosed > row.nativeHandles ||
        (!row.hasNative &&
            (row.nativeHandles != 0 ||
                row.nativeClosed != 0 ||
                row.cancelRequested)) ||
        (!row.hasSnapshot &&
            (row.pickerStarted ||
                row.pickerClosed ||
                row.ancestorObjects != 0 ||
                row.ancestorClosed != 0 ||
                row.hasNative)) ||
        (row.pickerClosed && !row.pickerStarted)) {
      throw const FormatException('Invalid original observation result');
    }
    if (row.nativeRetirementComplete &&
        (row.nativeClosed != row.nativeHandles ||
            row.ancestorClosed != row.ancestorObjects ||
            (row.pickerStarted && !row.pickerClosed))) {
      throw const FormatException('Incomplete native close evidence');
    }
    final deliveries = <FsWatchDelivery>[];
    final frames = row.deliveries;
    if (frames != null) {
      if (frames.length > 64)
        throw const FormatException('Observation receipt limit');
      for (var i = 0; i < frames.length; i++) {
        final f = frames[i];
        if (f.sequenceBigInt != BigInt.from(i + 1) ||
            f.digest == null ||
            (f.executionFault != 0 && f.exitCode != 0) ||
            (i != 0 && f.fuelRemainingBigInt > deliveries.last.fuelRemaining)) {
          throw const FormatException('Original observation sequence changed');
        }
        deliveries.add(
          FsWatchDelivery(
            sequence: f.sequenceBigInt,
            digest: f.digest!,
            executionFault: _enum(IoExecutionFault.values, f.executionFault),
            exitCode: f.exitCode,
            hostCalls: f.hostCalls,
            fuelRemaining: f.fuelRemainingBigInt,
          ),
        );
      }
    }
    if ((!row.unknown && deliveries.length != state.frames) ||
        deliveries.length > state.frames) {
      throw const FormatException('Original observation receipt count changed');
    }
    return FsWatchReply(
      snapshot,
      FsWatchResult(
        success: row.success,
        error: _enum(FsWatchError.values, row.error),
        detail: detail,
        unknown: row.unknown,
        workerJoined: row.originalWorkerJoined,
        nativeRetirementComplete: row.nativeRetirementComplete,
        disconnect: row.hasDisconnect
            ? _enum(IoJobError.values, row.disconnect)
            : null,
        maintenance: row.hasMaintenance
            ? _enum(IoJobError.values, row.maintenance)
            : null,
        resources: row.resources,
        jobs: row.jobs,
        chargedBytes: row.chargedBytesBigInt,
        deliveries: deliveries,
        hasSnapshot: row.hasSnapshot,
        pickerStarted: row.pickerStarted,
        pickerClosed: row.pickerClosed,
        ancestorObjects: row.ancestorObjects,
        ancestorClosed: row.ancestorClosed,
        hasNative: row.hasNative,
        nativeHandles: row.nativeHandles,
        nativeClosed: row.nativeClosed,
        cancelRequested: row.cancelRequested,
      ),
    );
  }
}
