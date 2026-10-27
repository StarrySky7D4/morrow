import 'dart:typed_data';
import 'blob_plan_models.dart';
import 'file_task_native.dart' show FileTaskExchange;
import 'generated/host.capnp.dart' as host;
import 'io_task_codec_native.dart';
import 'io_task_models.dart';

/// Every method performs one exchange. Lost replies never repeat a decision.
final class NativeBlobPlanClient implements BlobPlanBackend {
  NativeBlobPlanClient(this._exchange);
  final FileTaskExchange _exchange;
  static bool _same(Uint8List? a, Uint8List b) =>
      a != null &&
      a.length == b.length &&
      Iterable<int>.generate(b.length).every((i) => a[i] == b[i]);
  @override
  Future<BlobPlanSnapshot> startBlobPlan(BlobPlanRequest value) {
    BlobPlanValidation.request(value);
    return _exchange(
      host.Action.blobPlanStart,
      clearReply: true,
      configure: (r) {
        final out = r.initBlobPlanStart();
        out.submission = value.submission;
        out.packageId = value.packageId;
        out.packageDigest = value.packageDigest;
        out.registryRevisionBigInt = value.registryRevision;
        out.handler = value.handler;
        out.inputType = value.inputType;
        out.outputType = BlobPlanRequest.outputType;
        out.input = value.input;
        out.timeoutMs = value.timeoutMs;
      },
      decode: (r) {
        final state = decodeReply(r, allowResult: false).snapshot;
        if (state == null ||
            !_same(state.submission, value.submission) ||
            state.consumed ||
            state.delivered) {
          throw const FormatException('Original planning submission changed');
        }
        return state;
      },
    );
  }

  Future<BlobPlanReply> _command(
    host.Action action,
    Uint8List key, {
    BlobPlanView? review,
  }) {
    final exact = HttpTaskCodec.identity(key);
    if (review != null) BlobPlanValidation.view(review);
    return _exchange(
      action,
      clearReply: true,
      configure: (r) {
        r.blobPlanKey = exact;
        if (review != null) {
          final out = r.initBlobPlanApproval();
          out.packageDigest = review.packageDigest;
          out.registryRevisionBigInt = review.registryRevision;
          out.proposalSha256 = review.proposalSha256;
        }
      },
      decode: (r) {
        final value = decodeReply(
          r,
          allowResult: action == host.Action.blobPlanRead,
        );
        if (action == host.Action.blobPlanAcknowledge) {
          if (value.snapshot != null)
            throw const FormatException('Planning acknowledgement incomplete');
        } else if (value.snapshot == null ||
            !_same(value.snapshot!.key, exact)) {
          throw const FormatException('Original planning key changed');
        }
        if (action == host.Action.blobPlanApprove &&
            (!value.snapshot!.consumed ||
                value.snapshot!.uploadKey == null ||
                value.snapshot!.submissionFailed)) {
          throw const FormatException('Original upload admission unresolved');
        }
        return value;
      },
    );
  }

  @override
  Future<BlobPlanSnapshot> statusBlobPlan(Uint8List key) async =>
      (await _command(host.Action.blobPlanStatus, key)).snapshot!;
  @override
  Future<BlobPlanReply> readBlobPlan(Uint8List key) =>
      _command(host.Action.blobPlanRead, key);
  @override
  Future<BlobPlanSnapshot> cancelBlobPlan(Uint8List key) async =>
      (await _command(host.Action.blobPlanCancel, key)).snapshot!;
  @override
  Future<void> acknowledgeBlobPlan(Uint8List key) async {
    await _command(host.Action.blobPlanAcknowledge, key);
  }

  @override
  Future<BlobPlanSnapshot> approveBlobPlan(
    Uint8List key,
    BlobPlanView reviewed,
  ) async => (await _command(
    host.Action.blobPlanApprove,
    key,
    review: reviewed,
  )).snapshot!;
  @override
  Future<BlobPlanSnapshot?> discoverBlobPlan(Uint8List submission) {
    final exact = HttpTaskCodec.identity(submission);
    return _exchange(
      host.Action.blobPlanDiscover,
      clearReply: true,
      configure: (r) => r.blobPlanSubmission = exact,
      decode: (r) {
        final state = decodeReply(r, allowResult: false).snapshot;
        if (state != null && !_same(state.submission, exact)) {
          throw const FormatException('Original planning discovery changed');
        }
        return state;
      },
    );
  }

  static BlobPlanReply decodeReply(
    host.ResponseReader response, {
    required bool allowResult,
  }) {
    final row = response.blobPlanState, result = response.blobPlanResult;
    if (row == null || result == null)
      throw const FormatException('Missing planning envelope');
    Uint8List? optional(Uint8List? b) =>
        b == null || b.isEmpty ? null : HttpTaskCodec.identity(b);
    final key = optional(row.key), submission = optional(row.submission);
    final uploadKey = optional(row.uploadKey),
        uploadSubmission = optional(row.uploadSubmission);
    if (row.error > 10 ||
        (row.ready && row.delivered) ||
        (row.running &&
            (row.originalWorkerJoined ||
                row.ready ||
                row.delivered ||
                row.consumed)) ||
        (row.found &&
            (key == null ||
                submission == null ||
                (!row.running && !row.ready && !row.delivered))) ||
        (!row.found &&
            (key != null ||
                submission != null ||
                row.running ||
                row.originalWorkerJoined ||
                row.ready ||
                row.delivered ||
                row.consumed ||
                row.error != 0 ||
                uploadKey != null ||
                uploadSubmission != null ||
                row.submissionFailed)) ||
        (row.consumed &&
            (!row.originalWorkerJoined ||
                !row.delivered ||
                uploadSubmission == null ||
                row.error != 0)) ||
        (!row.consumed &&
            (uploadKey != null ||
                uploadSubmission != null ||
                row.submissionFailed)) ||
        (row.submissionFailed && uploadKey != null) ||
        (row.consumed && (uploadKey == null) != row.submissionFailed)) {
      throw const FormatException('Inconsistent original planning state');
    }
    final snapshot = !row.found
        ? null
        : BlobPlanSnapshot(
            key: key!,
            submission: submission!,
            running: row.running,
            originalWorkerJoined: row.originalWorkerJoined,
            ready: row.ready,
            delivered: row.delivered,
            consumed: row.consumed,
            error: row.error == 0 ? null : BlobPlanError.values[row.error - 1],
            uploadKey: uploadKey,
            uploadSubmission: uploadSubmission,
            submissionFailed: row.submissionFailed,
          );
    if (result.kind > 2 ||
        result.error > 10 ||
        result.executionFault > 9 ||
        result.hostCalls > 1024 ||
        result.pluginFailureCode > 3 ||
        (!allowResult && result.kind != 0) ||
        (result.kind != 0 &&
            (!row.found || !row.delivered || row.ready || row.consumed)) ||
        (result.kind == 0 &&
            (result.error != 0 ||
                result.view != null ||
                result.hasExecution ||
                result.hasPluginFailure)) ||
        (result.kind == 1 &&
            (result.error != 0 ||
                result.view == null ||
                !row.originalWorkerJoined ||
                !result.hasExecution ||
                result.executionFault != 0 ||
                result.exitCode != 0 ||
                result.hasPluginFailure)) ||
        (result.kind == 2 && (result.error == 0 || result.view != null)) ||
        (!result.hasExecution &&
            (result.executionFault != 0 ||
                result.exitCode != 0 ||
                result.hostCalls != 0)) ||
        (result.executionFault != 0 && result.exitCode != 0) ||
        (!result.hasPluginFailure && result.pluginFailureCode != 0) ||
        (result.hasPluginFailure &&
            (!result.hasExecution || result.kind != 2))) {
      throw const FormatException('Inconsistent original planning result');
    }
    if (result.kind == 0) return BlobPlanReply(snapshot, null);
    BlobPlanView? view;
    final v = result.view;
    if (v != null) {
      view = BlobPlanView(
        packageId: v.packageId ?? '',
        packageDigest: v.packageDigest ?? Uint8List(0),
        registryRevision: v.registryRevisionBigInt,
        plannerHandler: v.plannerHandler ?? '',
        uploadHandler: v.uploadHandler ?? '',
        planningInputSha256: v.planningInputSha256 ?? Uint8List(0),
        label: v.label ?? '',
        byteLength: v.byteLengthBigInt,
        wholeSha256: v.wholeSha256 ?? Uint8List(0),
        proposalSha256: v.proposalSha256 ?? Uint8List(0),
      );
      BlobPlanValidation.view(view);
    }
    return BlobPlanReply(
      snapshot,
      BlobPlanResult(
        view: view,
        error: result.error == 0
            ? null
            : BlobPlanError.values[result.error - 1],
        hasExecution: result.hasExecution,
        executionFault: IoExecutionFault.values[result.executionFault],
        exitCode: result.exitCode,
        hostCalls: result.hostCalls,
        pluginFailureCode: result.hasPluginFailure
            ? result.pluginFailureCode
            : null,
      ),
    );
  }
}
