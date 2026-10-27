import 'dart:async';
import 'dart:typed_data';
import 'package:crypto/crypto.dart' as crypto;
import 'package:test/test.dart';
import '../../../lib/plugins/blob_plan_models.dart';
import '../../../lib/plugins/blob_plan_session.dart';
import '../../../lib/plugins/blob_upload_session.dart';
import '../../../lib/plugins/io_task_models.dart';
import 'blob_plan_client_test.dart' show id, request;
import 'blob_upload_session_test.dart' show Original;

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
    if (loseApprove || failAdmission)
      throw StateError('admission reply unavailable');
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

Future<(Plans, BlobPlanSession, Object, Original)> open({
  bool prepare = false,
}) async {
  final io = Original(),
      p = Plans(io),
      s = BlobPlanSession.forBackend(p, io),
      v = s.attachView();
  await s.refresh(v);
  if (prepare) {
    await s.start(v, request(submission: id(6)));
    p.joined = true;
    await s.refresh(v);
    await s.read(v);
  }
  return (p, s, v, io);
}

void main() {
  test(
    'retains one original start across missing reply and view replacement',
    () async {
      final (p, s, v, io) = await open();
      p.loseStart = true;
      expect(await s.start(v, request(submission: id(6))), isFalse);
      expect(await s.start(v, request()), isFalse);
      expect(await s.refresh(v), isTrue);
      expect(s.snapshot!.key, id(7));
      expect(p.calls.where((a) => a == 'start').length, 1);
      await s.detachView(v);
      final next = s.attachView();
      expect(identical(s, BlobPlanSession.forBackend(p, io)), isTrue);
      expect(await s.refresh(next), isTrue);
      expect(s.canApprove, isFalse);
      expect(p.calls.where((a) => a == 'cancel').length, 1);
    },
  );
  test('absent discovery never authorizes replay of a failed start', () async {
    final (p, s, v, _) = await open();
    p.rejectStart = true;
    await s.start(v, request());
    expect(await s.refresh(v), isFalse);
    expect(s.request, isNotNull);
    expect(s.canStart, isFalse);
    expect(await s.start(v, request()), isFalse);
    expect(p.calls.where((a) => a == 'start').length, 1);
  });
  test(
    'wrong original key is rejected without replacing retained identity',
    () async {
      final (p, s, v, _) = await open();
      await s.start(v, request());
      p.wrongIdentity = true;
      expect(await s.refresh(v), isFalse);
      expect(s.snapshot!.key, id(7));
    },
  );
  test('proposal input hash must belong to the frozen request', () async {
    final (p, s, v, _) = await open();
    await s.start(v, request());
    p.joined = true;
    p.wrongHash = true;
    await s.refresh(v);
    expect(await s.read(v), isFalse);
    expect(s.deliveryLost, isTrue);
    expect(s.canApprove, isFalse);
    expect(s.result, isNull);
  });
  test(
    'lost proposal delivery cannot become approval or repeat the read',
    () async {
      final (p, s, v, _) = await open();
      await s.start(v, request());
      p.joined = true;
      p.loseRead = true;
      await s.refresh(v);
      expect(await s.read(v), isFalse);
      await s.refresh(v);
      expect(await s.read(v), isFalse);
      expect(await s.approve(v), isFalse);
      expect(p.calls.where((a) => a == 'read').length, 1);
      expect(await s.acknowledge(v), isTrue);
      expect(s.history.single.result, isNull);
    },
  );
  test(
    'lost approval finds and adopts only the original upload; never starts another',
    () async {
      final (p, s, v, io) = await open(prepare: true);
      p.loseApprove = true;
      expect(s.canApprove, isTrue);
      expect(await s.approve(v), isFalse);
      expect(await s.approve(v), isFalse);
      expect(await s.refresh(v), isTrue);
      final found = s.snapshot!;
      expect(found.consumed, isTrue);
      expect(found.uploadKey, id(3));
      final uploads = BlobUploadSession.forBackend(io, io),
          uv = uploads.attachView();
      expect(
        await uploads.adoptOriginal(
          uv,
          found.uploadKey!,
          found.uploadSubmission!,
        ),
        isTrue,
      );
      expect(uploads.attempt, isNull);
      expect(uploads.canStart, isFalse);
      expect(io.calls.contains('start'), isFalse);
      expect(p.calls.where((a) => a == 'approve').length, 1);
      expect(await s.acknowledge(v), isFalse);
      io.ready = true;
      await uploads.refresh(uv);
      expect(await uploads.read(uv), isTrue);
      expect(await uploads.acknowledge(uv), isTrue);
      expect(await s.acknowledge(v), isTrue);
      expect(s.history.single.snapshot!.uploadKey, id(3));
      expect(s.canStart, isTrue);
    },
  );
  test(
    'failed upload admission remains consumed and can only be acknowledged',
    () async {
      final (p, s, v, _) = await open(prepare: true);
      p.failAdmission = true;
      expect(await s.approve(v), isFalse);
      await s.refresh(v);
      expect(s.snapshot!.submissionFailed, isTrue);
      expect(s.snapshot!.uploadKey, isNull);
      expect(await s.approve(v), isFalse);
      expect(await s.acknowledge(v), isTrue);
    },
  );
  test(
    'closing view during start requests one original cancellation',
    () async {
      final (p, s, v, _) = await open();
      p.startWait = Completer<void>();
      final pending = s.start(v, request());
      await s.detachView(v);
      p.startWait!.complete();
      await pending;
      expect(p.calls.where((a) => a == 'cancel').length, 1);
      expect(s.canApprove, isFalse);
    },
  );
  test(
    'closing view during lost approval discovers then cancels only original upload',
    () async {
      final (p, s, v, io) = await open(prepare: true);
      p.approvalWait = Completer<void>();
      p.loseApprove = true;
      final pending = s.approve(v);
      await s.detachView(v);
      p.approvalWait!.complete();
      await pending;
      expect(p.calls.where((a) => a == 'approve').length, 1);
      expect(p.calls.contains('cancel'), isFalse);
      expect(io.calls.where((a) => a == 'cancel').length, 1);
      expect(io.calls.contains('start'), isFalse);
    },
  );
  test('replacement view never receives the detached view proposal', () async {
    final (p, s, v, _) = await open();
    await s.start(v, request());
    p.joined = true;
    await s.refresh(v);
    p.readWait = Completer<void>();
    final pending = s.read(v);
    await s.detachView(v);
    final next = s.attachView();
    p.readWait!.complete();
    await pending;
    expect(s.result, isNull);
    expect(s.deliveryLost, isTrue);
    expect(await s.approve(next), isFalse);
  });
  test(
    'lost plan acknowledgement stays unresolved and is never repeated',
    () async {
      final (p, s, v, _) = await open(prepare: true);
      p.loseAck = true;
      expect(await s.acknowledge(v), isFalse);
      expect(s.acknowledgementUncertain, isTrue);
      expect(await s.refresh(v), isFalse);
      expect(await s.acknowledge(v), isFalse);
      expect(s.canStart, isFalse);
      expect(s.history, isEmpty);
      expect(p.calls.where((a) => a == 'ack').length, 1);
    },
  );
}
