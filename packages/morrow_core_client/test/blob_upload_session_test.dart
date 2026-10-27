import 'dart:async';
import 'dart:typed_data';
import 'package:test/test.dart';
import '../../../lib/plugins/blob_upload_models.dart';
import '../../../lib/plugins/blob_upload_session.dart';
import '../../../lib/plugins/io_task_models.dart';
import 'blob_upload_client_test.dart' show id, request;

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

Future<(Original, BlobUploadSession, Object)> open() async {
  final backend = Original(),
      session = BlobUploadSession.forBackend(Original(), Original());
  // Separate pairs must not alias each other's retained original session.
  final value = BlobUploadSession.forBackend(backend, backend);
  expect(identical(value, session), isFalse);
  final view = value.attachView();
  await value.refresh(view);
  return (backend, value, view);
}

void main() {
  test(
    'missing acknowledgement cannot be repeated across view replacement',
    () async {
      final (b, s, v) = await open();
      await s.start(v, request());
      b.ready = true;
      await s.refresh(v);
      await s.read(v);
      b.loseAck = true;
      expect(await s.acknowledge(v), isFalse);
      expect(s.acknowledgementUncertain, isTrue);
      expect(await s.acknowledge(v), isFalse);
      await s.detachView(v);
      final next = s.attachView();
      await s.refresh(next);
      expect(s.acknowledgementUncertain, isTrue);
      expect(s.canStart, isFalse);
      expect(await s.acknowledge(next), isFalse);
      expect(b.calls.where((a) => a == 'acknowledge').length, 1);
      expect(b.calls.contains('cancel'), isFalse);
      expect(s.history, isEmpty);
    },
  );
  test(
    'observing view can leave cancellation to original planning owner',
    () async {
      final (b, s, v) = await open();
      b.active = true;
      await s.adoptOriginal(v, id(3), id(1));
      expect(s.originalKey, id(3));
      expect(s.submission, id(1));
      expect(() => s.originalKey![0] = 1, throwsUnsupportedError);
      await s.detachView(v, cancelOriginal: false);
      expect(b.calls.contains('cancel'), isFalse);
      final next = s.attachView();
      await s.detachView(next);
      expect(b.calls.where((a) => a == 'cancel').length, 1);
    },
  );
  test(
    'adoption observes an original host upload without reconstructing or starting input',
    () async {
      final (b, s, v) = await open();
      b.active = true;
      expect(await s.adoptOriginal(v, id(3), id(1)), isTrue);
      expect(s.attempt, isNull);
      expect(s.canStart, isFalse);
      expect(b.calls.contains('start'), isFalse);
      expect(await s.adoptOriginal(v, id(4), id(1)), isFalse);
      expect(await s.adoptOriginal(v, id(3), id(8)), isFalse);
      b.ready = true;
      await s.refresh(v);
      expect(await s.read(v), isTrue);
      expect(await s.acknowledge(v), isTrue);
      expect(s.canStart, isTrue);
    },
  );
  test(
    'lost adoption observation retains original key and can cancel it on detach',
    () async {
      final (b, s, v) = await open();
      b.active = true;
      b.loseStatus = true;
      expect(await s.adoptOriginal(v, id(3), id(1)), isFalse);
      expect(s.canStart, isFalse);
      await s.detachView(v);
      expect(b.calls.where((a) => a == 'cancel').length, 1);
      expect(b.calls.contains('start'), isFalse);
      b.loseStatus = false;
      final next = s.attachView();
      expect(await s.refresh(next), isTrue);
      expect(s.snapshot!.task.key, id(3));
    },
  );

  test(
    'lost START reply is found by original submission without a second upload',
    () async {
      final (b, s, v) = await open();
      b.loseStart = true;
      expect(await s.start(v, request()), isFalse);
      expect(s.attempt, isNotNull);
      expect(s.canStart, isFalse);
      expect(await s.start(v, request()), isFalse);
      expect(await s.refresh(v), isTrue);
      expect(s.snapshot!.task.key, id(3));
      expect(b.calls.where((x) => x == 'start').length, 1);
    },
  );
  test(
    'another submission cannot be adopted after missing START reply',
    () async {
      final (b, s, v) = await open();
      b.loseStart = true;
      await s.start(v, request());
      b.wrongSubmission = true;
      expect(await s.refresh(v), isFalse);
      expect(s.snapshot, isNull);
      expect(s.canStart, isFalse);
      expect(b.calls.contains('status'), isFalse);
    },
  );
  test(
    'original backend session survives detach, reattach and lost cancel reply',
    () async {
      final (b, s, v) = await open();
      await s.start(v, request());
      b.loseCancel = true;
      await s.detachView(v);
      expect(identical(BlobUploadSession.forBackend(b, b), s), isTrue);
      final second = s.attachView();
      await s.refresh(second);
      await s.cancel(second);
      expect(b.calls.where((x) => x == 'cancel').length, 1);
      expect(b.calls.where((x) => x == 'start').length, 1);
    },
  );
  test('detach during START cancels the returned original key once', () async {
    final (b, s, v) = await open();
    b.startWait = Completer<void>();
    final pending = s.start(v, request());
    await s.detachView(v);
    expect(b.calls.contains('cancel'), isFalse);
    b.startWait!.complete();
    await pending;
    expect(b.calls.where((x) => x == 'cancel').length, 1);
    expect(s.canStart, isFalse);
  });
  test(
    'detach during READ suppresses unread success and requires explicit history',
    () async {
      final (b, s, v) = await open();
      await s.start(v, request());
      b.ready = true;
      await s.refresh(v);
      b.readWait = Completer<void>();
      final pending = s.read(v);
      await s.detachView(v);
      b.readWait!.complete();
      await pending;
      expect(s.result, isNull);
      expect(s.deliveryLost, isTrue);
      expect(s.canAcknowledge, isFalse);
      expect(b.calls.contains('cancel'), isTrue);
      final next = s.attachView();
      await s.refresh(next);
      await s.inspect(next);
      await s.confirm(next, BlobUploadObservation.staged);
      expect(s.canAcknowledge, isTrue);
      await s.acknowledge(next);
      expect(s.history.single.result, isNull);
      expect(s.canStart, isTrue);
    },
  );
  test(
    'lost READ never repeats consumption; observation does not invent success',
    () async {
      final (b, s, v) = await open();
      await s.start(v, request());
      b.ready = true;
      await s.refresh(v);
      b.loseRead = true;
      expect(await s.read(v), isFalse);
      await s.refresh(v);
      expect(await s.read(v), isFalse);
      expect(s.result, isNull);
      expect(s.canAcknowledge, isFalse);
      await s.inspect(v);
      expect(s.canAcknowledge, isFalse);
      await s.confirm(v, BlobUploadObservation.staged);
      expect(s.canAcknowledge, isTrue);
      await s.acknowledge(v);
      expect(s.history.single.result, isNull);
      expect(b.calls.where((x) => x == 'read').length, 1);
    },
  );
  test(
    'generic repair cannot clear original Unknown and wrong review cannot confirm',
    () async {
      final (b, s, v) = await open();
      await s.start(v, request());
      b.ready = true;
      await s.cancel(v);
      await s.refresh(v);
      await s.read(v);
      expect(s.result!.unknown, isTrue);
      await s.repair(v);
      expect(s.canAcknowledge, isFalse);
      expect(await s.confirm(v, BlobUploadObservation.staged), isFalse);
      await s.inspect(v);
      expect(await s.confirm(v, BlobUploadObservation.absent), isFalse);
      expect(b.calls.contains('confirm'), isFalse);
      await s.confirm(v, BlobUploadObservation.staged);
      await s.acknowledge(v);
      expect(s.history.single.result!.unknown, isTrue);
    },
  );
  test(
    'normal original success needs one delivery and actual join before acknowledgement',
    () async {
      final (b, s, v) = await open();
      await s.start(v, request());
      expect(await s.acknowledge(v), isFalse);
      expect(await s.read(v), isFalse);
      b.ready = true;
      await s.refresh(v);
      await s.read(v);
      expect(s.result!.phase, BlobUploadObservation.staged);
      expect(s.canAcknowledge, isTrue);
      await s.acknowledge(v);
      expect(s.history.length, 1);
      expect(s.canStart, isTrue);
      expect(b.calls.where((x) => x == 'read').length, 1);
    },
  );
}
