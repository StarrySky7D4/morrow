// Deterministic UI test peers adapted from the pure session fixtures.
// They are not runtime evidence and use Flutter test assertions directly.
import 'dart:async';
import 'dart:typed_data';
import 'package:crypto/crypto.dart' as crypto;
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/blob_plan_models.dart';
import 'package:morrow_studio/plugins/blob_upload_models.dart';
import 'package:morrow_studio/plugins/io_task_models.dart';

Uint8List id(int n) => Uint8List.fromList(List.filled(32, n));

final class Original implements BlobUploadBackend, WorkbenchIoTaskControl {
  final calls = <String>[];
  bool active = false, ready = false, delivered = false, recovery = false;
  bool loseStart = false,
      loseRead = false,
      wrongSubmission = false,
      loseCancel = false,
      loseStatus = false,
      loseAck = false;
  Completer<void>? startWait, readWait;
  BlobUploadObservation? observation;
  IoTaskSnapshot get io => IoTaskSnapshot(
    key: active ? id(3) : null,
    submission: active ? id(wrongSubmission ? 33 : 1) : null,
    storage: !active
        ? IoStoragePhase.local
        : recovery
        ? IoStoragePhase.recoveryRequired
        : ready || delivered
        ? IoStoragePhase.reclaimed
        : IoStoragePhase.running,
    delivery: IoDeliveryPhase.absent,
    exit: ready || delivered
        ? const IoTaskExit(
            execution: IoJobError.none,
            disconnect: IoJobError.none,
            maintenance: IoJobError.none,
          )
        : null,
  );
  BlobUploadSnapshot get snapshot => BlobUploadSnapshot(
    task: io,
    ready: ready && !delivered,
    delivered: delivered,
    recoveryRequired: recovery,
    observed: observation,
  );
  BlobUploadResult get result => BlobUploadResult(
    phase: recovery ? null : BlobUploadObservation.staged,
    error: recovery ? BlobUploadError.cancelled : null,
    unknown: recovery,
    calls: 7,
    wireBytes: BigInt.from(150017),
    nativeWorkBytes: BigInt.from(750000),
    hasExecution: true,
    executionFault: IoExecutionFault.none,
    exitCode: 0,
    hostCalls: 9,
  );
  @override
  Future<void> approveBlobUpload(
    String packageId,
    Uint8List digest,
    BigInt revision,
    bool approved,
  ) async {
    calls.add('approve');
  }

  @override
  Future<BlobUploadSnapshot> startBlobUpload(BlobUploadRequest request) async {
    calls.add('start');
    active = true;
    if (startWait != null) await startWait!.future;
    if (loseStart) throw StateError('lost START reply');
    return snapshot;
  }

  @override
  Future<BlobUploadSnapshot> statusBlobUpload(Uint8List key) async {
    calls.add('status');
    if (loseStatus) throw StateError('lost status');
    expect(key, id(3));
    return snapshot;
  }

  @override
  Future<BlobUploadReply> readBlobUpload(Uint8List key) async {
    calls.add('read');
    expect(key, id(3));
    final outcome = result;
    delivered = true;
    if (readWait != null) await readWait!.future;
    if (loseRead) throw StateError('lost READ reply');
    return BlobUploadReply(snapshot, outcome);
  }

  @override
  Future<BlobUploadSnapshot> inspectBlobUpload(Uint8List key) async {
    calls.add('inspect');
    expect(key, id(3));
    observation = BlobUploadObservation.staged;
    return snapshot;
  }

  @override
  Future<BlobUploadSnapshot> confirmBlobUpload(
    Uint8List key,
    BlobUploadObservation expected,
  ) async {
    calls.add('confirm');
    expect(key, id(3));
    expect(expected, observation);
    recovery = false;
    return snapshot;
  }

  @override
  Future<IoTaskSnapshot> ioStatus() async {
    calls.add('ioStatus');
    return io;
  }

  @override
  Future<IoTaskSnapshot> pollIo(Uint8List key) async => io;
  @override
  Future<IoTaskSnapshot> cancelIo(Uint8List key) async {
    calls.add('cancel');
    expect(key, id(3));
    recovery = true;
    if (loseCancel) throw StateError('lost cancellation reply');
    return io;
  }

  @override
  Future<IoTaskSnapshot> repairIo(Uint8List key) async {
    calls.add('repair');
    return io;
  }

  @override
  Future<IoTaskSnapshot> acknowledgeIo(Uint8List key) async {
    calls.add('acknowledge');
    expect(key, id(3));
    expect(delivered, isTrue);
    expect(recovery, isFalse);
    active = false;
    ready = false;
    delivered = false;
    if (loseAck) throw StateError('lost acknowledgement');
    return io;
  }

  @override
  Future<IoTaskRead> readIo(Uint8List key) =>
      throw UnsupportedError('not an HTTP result');
  @override
  Future<IoTaskSnapshot> startHttp(HttpTaskRequest request) =>
      throw UnsupportedError('not HTTP');
}

final class Plans implements BlobPlanBackend {
  Plans(this.io);
  final Original io;
  final calls = <String>[];
  BlobPlanRequest? original;
  bool created = false,
      joined = false,
      delivered = false,
      consumed = false,
      cancelled = false;
  bool loseStart = false,
      rejectStart = false,
      loseRead = false,
      loseApprove = false,
      loseAck = false;
  bool wrongHash = false, wrongIdentity = false, failAdmission = false;
  Completer<void>? startWait, approvalWait, readWait;
  BlobPlanSnapshot get state => BlobPlanSnapshot(
    key: id(wrongIdentity ? 17 : 7),
    submission: original!.submission,
    running: !joined,
    originalWorkerJoined: joined,
    ready: joined && !delivered,
    delivered: delivered,
    consumed: consumed,
    error: cancelled && !consumed ? BlobPlanError.cancelled : null,
    uploadKey: consumed && !failAdmission ? id(3) : null,
    uploadSubmission: consumed ? id(1) : null,
    submissionFailed: consumed && failAdmission,
  );
  BlobPlanView get view => BlobPlanView(
    packageId: original!.packageId,
    packageDigest: original!.packageDigest,
    registryRevision: original!.registryRevision,
    plannerHandler: original!.handler,
    uploadHandler: 'blob.upload',
    planningInputSha256: wrongHash
        ? id(99)
        : Uint8List.fromList(crypto.sha256.convert(original!.input).bytes),
    label: 'Sample',
    byteLength: BigInt.from(150017),
    wholeSha256: id(5),
    proposalSha256: id(6),
  );
  @override
  Future<BlobPlanSnapshot> startBlobPlan(BlobPlanRequest request) async {
    calls.add('start');
    original = request;
    if (rejectStart) throw StateError('no admission');
    created = true;
    await startWait?.future;
    if (loseStart) throw StateError('lost start');
    return state;
  }

  @override
  Future<BlobPlanSnapshot> statusBlobPlan(Uint8List key) async {
    calls.add('status');
    expect(key, id(7));
    return state;
  }

  @override
  Future<BlobPlanSnapshot?> discoverBlobPlan(Uint8List submission) async {
    calls.add('discover');
    expect(submission, original!.submission);
    return created ? state : null;
  }

  @override
  Future<BlobPlanReply> readBlobPlan(Uint8List key) async {
    calls.add('read');
    expect(key, id(7));
    if (!joined || delivered) return BlobPlanReply(state, null);
    delivered = true;
    await readWait?.future;
    if (loseRead) throw StateError('lost proposal');
    return BlobPlanReply(
      state,
      BlobPlanResult(
        view: cancelled ? null : view,
        error: cancelled ? BlobPlanError.cancelled : null,
        hasExecution: true,
        executionFault: IoExecutionFault.none,
        exitCode: 0,
        hostCalls: 0,
        pluginFailureCode: null,
      ),
    );
  }

  @override
  Future<BlobPlanSnapshot> approveBlobPlan(
    Uint8List key,
    BlobPlanView reviewed,
  ) async {
    calls.add('approve');
    expect(key, id(7));
    expect(reviewed.proposalSha256, id(6));
    expect(consumed, isFalse);
    expect(delivered, isTrue);
    consumed = true;
    io.active = !failAdmission;
    await approvalWait?.future;
    if (loseApprove || failAdmission) {
      throw StateError('admission reply unavailable');
    }
    return state;
  }

  @override
  Future<BlobPlanSnapshot> cancelBlobPlan(Uint8List key) async {
    calls.add('cancel');
    expect(key, id(7));
    expect(consumed, isFalse);
    cancelled = true;
    return state;
  }

  @override
  Future<void> acknowledgeBlobPlan(Uint8List key) async {
    calls.add('ack');
    expect(key, id(7));
    expect(joined && delivered, isTrue);
    expect(io.active, isFalse);
    created = false;
    if (loseAck) throw StateError('lost acknowledgement');
  }
}
