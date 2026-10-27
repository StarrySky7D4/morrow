import 'dart:typed_data';
import 'blob_upload_models.dart';
import 'file_task_native.dart' show FileTaskExchange;
import 'generated/host.capnp.dart' as host;
import 'io_task_codec_native.dart';
import 'io_task_models.dart';

/// One exchange per explicit command. No retry and no result reconstruction.
final class NativeBlobUploadClient implements BlobUploadBackend {
  NativeBlobUploadClient(this._exchange);
  final FileTaskExchange _exchange;
  static bool _same(Uint8List? a, Uint8List b) =>
      a != null &&
      a.length == b.length &&
      Iterable<int>.generate(b.length).every((i) => a[i] == b[i]);

  @override
  Future<void> approveBlobUpload(
    String packageId,
    Uint8List digest,
    BigInt revision,
    bool approved,
  ) {
    BlobUploadValidation.selection(packageId, digest, revision);
    final exact = HttpTaskCodec.identity(digest);
    return _exchange(
      host.Action.pluginApproveBlobUpload,
      clearReply: true,
      configure: (r) {
        r.id = packageId;
        r.sha256 = exact;
        r.revisionBigInt = revision;
        r.blobUploadApproved = approved;
      },
      decode: (_) {},
    );
  }

  @override
  Future<BlobUploadSnapshot> startBlobUpload(BlobUploadRequest value) {
    BlobUploadValidation.request(value);
    return _exchange(
      host.Action.blobUploadStart,
      clearReply: true,
      configure: (r) {
        final out = r.initBlobUploadStart();
        out.submission = value.submission;
        out.packageId = value.packageId;
        out.packageDigest = value.packageDigest;
        out.registryRevisionBigInt = value.registryRevision;
        out.nominationRef = value.nominationRef;
        out.ownerDomain = value.ownerDomain;
        out.transferEpoch = value.transferEpoch;
        out.objectRef = value.objectRef;
        out.operationId = value.operationId;
        out.byteLengthBigInt = value.byteLength;
        out.wholeSha256 = value.wholeSha256;
        out.timeoutMs = value.timeoutMs;
        out.handler = value.handler;
        out.inputType = value.inputType;
        out.outputType = value.outputType;
        out.input = value.input;
      },
      decode: (r) {
        final reply = decodeReply(r, allowResult: false);
        if (!_same(reply.snapshot.task.submission, value.submission)) {
          throw const FormatException('Upload submission changed');
        }
        return reply.snapshot;
      },
    );
  }

  Future<BlobUploadReply> _command(
    host.Action action,
    Uint8List key, {
    BlobUploadObservation? expected,
  }) {
    final exact = HttpTaskCodec.identity(key);
    return _exchange(
      action,
      clearReply: true,
      configure: (r) {
        r.ioKey = exact;
        if (expected != null) r.blobUploadObservation = expected.index + 1;
      },
      decode: (r) {
        final value = decodeReply(
          r,
          allowResult: action == host.Action.blobUploadRead,
        );
        if (!_same(value.snapshot.task.key, exact)) {
          throw const FormatException('Upload task changed');
        }
        return value;
      },
    );
  }

  @override
  Future<BlobUploadSnapshot> statusBlobUpload(Uint8List key) async =>
      (await _command(host.Action.blobUploadStatus, key)).snapshot;
  @override
  Future<BlobUploadReply> readBlobUpload(Uint8List key) =>
      _command(host.Action.blobUploadRead, key);
  @override
  Future<BlobUploadSnapshot> inspectBlobUpload(Uint8List key) async =>
      (await _command(host.Action.blobUploadInspect, key)).snapshot;
  @override
  Future<BlobUploadSnapshot> confirmBlobUpload(
    Uint8List key,
    BlobUploadObservation expected,
  ) async => (await _command(
    host.Action.blobUploadConfirm,
    key,
    expected: expected,
  )).snapshot;

  static BlobUploadReply decodeReply(
    host.ResponseReader reply, {
    required bool allowResult,
  }) {
    final task = HttpTaskCodec.snapshot(reply.ioState);
    final state = reply.blobUploadState, row = reply.blobUploadResult;
    if (task.key == null ||
        task.submission == null ||
        state == null ||
        row == null ||
        state.observed > 3 ||
        (state.ready && state.delivered) ||
        ((state.ready || state.delivered || state.observed != 0) &&
            task.exit == null) ||
        (state.recoveryRequired &&
            task.storage != IoStoragePhase.recoveryRequired)) {
      throw const FormatException('Inconsistent upload state');
    }
    final snapshot = BlobUploadSnapshot(
      task: task,
      ready: state.ready,
      delivered: state.delivered,
      recoveryRequired: state.recoveryRequired,
      observed: state.observed == 0
          ? null
          : BlobUploadObservation.values[state.observed - 1],
    );
    final wire = row.wireBytesBigInt, work = row.nativeWorkBytesBigInt;
    if (row.kind > 2 ||
        (!allowResult && row.kind != 0) ||
        row.phase > 3 ||
        row.error > 10 ||
        row.executionFault > 9 ||
        row.calls > 1024 ||
        row.hostCalls > 1024 ||
        wire > BigInt.from(64 * 1024 * 1024) ||
        work > BigInt.from(128 * 1024 * 1024) ||
        (row.kind == 0 &&
            (row.phase != 0 ||
                row.error != 0 ||
                row.unknown ||
                row.calls != 0 ||
                wire != BigInt.zero ||
                work != BigInt.zero ||
                row.hasExecution)) ||
        (row.kind == 1 &&
            (row.phase == 0 ||
                row.error != 0 ||
                !row.hasExecution ||
                row.executionFault != 0 ||
                row.exitCode != 0)) ||
        (row.kind == 2 && (row.phase != 0 || row.error == 0)) ||
        (!row.hasExecution &&
            (row.executionFault != 0 ||
                row.exitCode != 0 ||
                row.hostCalls != 0)) ||
        (row.executionFault != 0 && row.exitCode != 0) ||
        (row.kind != 0 && (!state.delivered || state.ready)) ||
        (row.unknown && !state.recoveryRequired)) {
      throw const FormatException('Inconsistent upload result');
    }
    return BlobUploadReply(
      snapshot,
      row.kind == 0
          ? null
          : BlobUploadResult(
              phase: row.phase == 0
                  ? null
                  : BlobUploadObservation.values[row.phase - 1],
              error: row.error == 0
                  ? null
                  : BlobUploadError.values[row.error - 1],
              unknown: row.unknown,
              calls: row.calls,
              wireBytes: wire,
              nativeWorkBytes: work,
              hasExecution: row.hasExecution,
              executionFault: IoExecutionFault.values[row.executionFault],
              exitCode: row.exitCode,
              hostCalls: row.hostCalls,
            ),
    );
  }
}
