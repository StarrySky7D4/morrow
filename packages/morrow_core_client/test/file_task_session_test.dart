import 'dart:async';
import 'dart:typed_data';
import 'package:crypto/crypto.dart';
import 'package:test/test.dart';
import '../../../lib/plugins/file_task_models.dart';
import '../../../lib/plugins/file_task_session.dart';
import '../../../lib/plugins/io_task_models.dart';

Uint8List id(int value) => Uint8List.fromList(List.filled(32, value));
FileTaskRequest request(int value, {int max = 100000}) => FileTaskRequest(
  submission: id(value),
  packageId: 'org.example.file',
  packageDigest: id(4),
  registryRevision: BigInt.one,
  handler: 'file.read-selected',
  selectedPath: '/chosen',
  maxBytes: BigInt.from(max),
  timeoutMs: 1000,
);

class Backend implements FileTaskBackend, WorkbenchIoTaskControl {
  Backend(this.bytes);
  final Uint8List bytes;
  FileTaskResult? pending;
  Uint8List? key, submission;
  IoStoragePhase storage = IoStoragePhase.local;
  IoTaskExit? exit;
  bool loseStart = false,
      loseChunk = false,
      loseRead = false,
      loseFinish = false,
      loseAck = false;
  bool alwaysPending = false, recovery = false, joinOnFinishedRead = false;
  int starts = 0, chunks = 0, reads = 0, finishes = 0, cancels = 0, acks = 0;
  Completer<void>? heldRead;
  FileTaskResult Function(FileTaskResult)? mutate;
  IoTaskSnapshot state() => IoTaskSnapshot(
    key: key,
    submission: submission,
    storage: storage,
    delivery: pending == null
        ? IoDeliveryPhase.absent
        : alwaysPending
        ? IoDeliveryPhase.pending
        : IoDeliveryPhase.ready,
    exit: exit,
  );
  void reclaimed() {
    storage = recovery
        ? IoStoragePhase.recoveryRequired
        : IoStoragePhase.reclaimed;
    exit = IoTaskExit(
      execution: IoJobError.none,
      disconnect: IoJobError.none,
      maintenance: recovery ? IoJobError.busy : IoJobError.none,
    );
  }

  @override
  Future<IoTaskSnapshot> startFile(FileTaskRequest r) async {
    starts++;
    key = id(2);
    submission = r.submission;
    storage = IoStoragePhase.running;
    pending = FileTaskCaptured(
      length: BigInt.from(bytes.length),
      sha256: Uint8List.fromList(sha256.convert(bytes).bytes),
    );
    if (loseStart) throw StateError('lost start');
    return state();
  }

  @override
  Future<IoTaskSnapshot> ioStatus() async => state();
  @override
  Future<IoTaskSnapshot> pollIo(Uint8List key) async {
    if (this.key == null) throw StateError('stale task');
    if (storage == IoStoragePhase.stopping) reclaimed();
    return state();
  }

  @override
  Future<IoTaskSnapshot> requestFileChunk(
    Uint8List key,
    BigInt offset,
    int limit,
  ) async {
    chunks++;
    if (pending != null) throw StateError('unconsumed result');
    final start = offset.toInt(),
        end = (offset.toInt() + limit).clamp(0, bytes.length);
    pending = FileTaskChunk(
      offset: offset,
      bytes: Uint8List.sublistView(bytes, start, end),
      eof: end == bytes.length,
    );
    if (loseChunk) throw StateError('lost command acknowledgement');
    return state();
  }

  @override
  Future<FileTaskRead> readFile(Uint8List key) async {
    reads++;
    final result = pending;
    pending = null;
    await heldRead?.future;
    if (loseRead) throw StateError('lost consumed result');
    if (result is FileTaskFinished) {
      storage = IoStoragePhase.stopping;
      if (joinOnFinishedRead) reclaimed();
    }
    return FileTaskRead(
      snapshot: state(),
      result: result == null ? null : mutate?.call(result) ?? result,
    );
  }

  @override
  Future<IoTaskSnapshot> finishFile(Uint8List key) async {
    finishes++;
    pending = FileTaskFinished();
    if (loseFinish) throw StateError('lost Finish ack');
    return state();
  }

  @override
  Future<IoTaskSnapshot> cancelIo(Uint8List key) async {
    cancels++;
    pending = null;
    storage = IoStoragePhase.stopping;
    return state();
  }

  @override
  Future<IoTaskSnapshot> repairIo(Uint8List key) async {
    recovery = false;
    reclaimed();
    return state();
  }

  @override
  Future<IoTaskSnapshot> acknowledgeIo(Uint8List key) async {
    acks++;
    this.key = null;
    exit = null;
    storage = IoStoragePhase.local;
    if (loseAck) throw StateError('lost ack');
    return state();
  }

  @override
  dynamic noSuchMethod(Invocation invocation) => super.noSuchMethod(invocation);
}

FileTaskSession session(Backend b) =>
    FileTaskSession(b, b, pollInterval: Duration.zero);
Future<FileTaskSession> started(Backend b, {int max = 100000}) async {
  final s = session(b);
  await s.refresh();
  await s.start(request(1, max: max));
  return s;
}

void main() {
  test(
    'sequential drain independently verifies bytes with bounded preview and waits for actual join',
    () async {
      final b = Backend(
        Uint8List.fromList(List.generate(90007, (i) => i % 251)),
      );
      final s = await started(b);
      await s.verify();
      expect(s.phase, FileTaskPhase.verified);
      expect(s.received, BigInt.from(90007));
      expect(s.digest, sha256.convert(b.bytes).toString());
      expect(s.preview.length, 4096);
      expect(b.chunks, 2);
      expect(b.finishes, 1);
      expect(b.reads, 4);
      expect(s.canAcknowledge, isFalse);
      expect(s.canStart, isFalse);
      await s.refresh();
      expect(s.canAcknowledge, isTrue);
      await s.acknowledge();
      expect(s.canStart, isTrue);
      expect(s.preview, isEmpty);
      expect(s.history.single.digest, sha256.convert(b.bytes).toString());
    },
  );
  for (final maintenanceFailed in [false, true]) {
    test('Finished and worker exit in one reply preserve verified status '
        '(maintenance failed: $maintenanceFailed)', () async {
      final b = Backend(Uint8List.fromList([1, 2, 3]))
        ..joinOnFinishedRead = true
        ..recovery = maintenanceFailed;
      final s = await started(b);
      await s.verify();
      expect(s.phase, FileTaskPhase.verified);
      expect(s.notice, isNull);
      expect(s.digest, sha256.convert(b.bytes).toString());
      expect(s.canRepair, maintenanceFailed);
      expect(s.canAcknowledge, !maintenanceFailed);
      expect(b.finishes, 1);
      expect(b.cancels, 0);
    });
  }
  test(
    'empty file uses the empty digest and Finish without requesting a chunk',
    () async {
      final b = Backend(Uint8List(0));
      final s = await started(b, max: 0);
      await s.verify();
      expect(s.phase, FileTaskPhase.verified);
      expect(b.chunks, 0);
      expect(b.finishes, 1);
    },
  );
  for (final failure in [
    'offset',
    'early-eof',
    'missing-eof',
    'oversize',
    'empty',
    'hash',
    'kind',
    'length',
  ]) {
    test(
      'rejects $failure and stops instead of delivering a verified result',
      () async {
        final b = Backend(
          Uint8List.fromList(List.generate(90007, (i) => i % 251)),
        );
        b.mutate = (r) {
          if (r is FileTaskCaptured && failure == 'length')
            return FileTaskCaptured(
              length: BigInt.from(100001),
              sha256: r.sha256,
            );
          if (r is FileTaskChunk) {
            if (failure == 'kind') return FileTaskFinished();
            return FileTaskChunk(
              offset: failure == 'offset' ? r.offset + BigInt.one : r.offset,
              bytes: failure == 'oversize'
                  ? Uint8List(65537)
                  : failure == 'empty'
                  ? Uint8List(0)
                  : failure == 'hash'
                  ? Uint8List.fromList(r.bytes.map((v) => v ^ 1).toList())
                  : r.bytes,
              eof: failure == 'early-eof'
                  ? true
                  : failure == 'missing-eof'
                  ? false
                  : r.eof,
            );
          }
          return r;
        };
        final s = await started(b);
        await s.verify();
        expect(s.phase, FileTaskPhase.failed);
        expect(s.notice, FileTaskNotice.integrity);
        expect(s.digest, isNull);
        expect(b.finishes, 0);
        expect(b.cancels, 1);
      },
    );
  }
  test(
    'lost start recovers only exact submission and never opens a second file',
    () async {
      final b = Backend(Uint8List.fromList([1, 2, 3]))..loseStart = true;
      final s = await started(b);
      expect(s.phase, FileTaskPhase.unknown);
      await s.refresh();
      expect(s.phase, FileTaskPhase.capturing);
      await s.verify();
      expect(s.phase, FileTaskPhase.verified);
      expect(b.starts, 1);
    },
  );
  test(
    'foreign task does not recover, consume, cancel or replace an uncertain attempt',
    () async {
      final b = Backend(Uint8List(2))..loseStart = true;
      final s = await started(b);
      b.submission = id(9);
      await s.refresh();
      await s.verify();
      await s.cancel();
      expect(s.phase, FileTaskPhase.unknown);
      expect(s.ownsTask, isFalse);
      expect(b.reads, 0);
      expect(b.cancels, 0);
      expect(s.canStart, isFalse);
      b.key = null;
      b.storage = IoStoragePhase.local;
      await s.refresh();
      expect(s.canAbandon, isTrue);
      s.abandon();
      expect(s.canStart, isTrue);
      expect(s.history.single.phase, FileTaskPhase.unknown);
    },
  );
  for (final failure in ['chunk', 'read', 'finish']) {
    test(
      'lost $failure reply stays unknown after status and is never replayed',
      () async {
        final b = Backend(Uint8List.fromList([1, 2, 3]));
        b.loseChunk = failure == 'chunk';
        b.loseRead = failure == 'read';
        b.loseFinish = failure == 'finish';
        final s = await started(b);
        await s.verify();
        expect(s.phase, FileTaskPhase.unknown);
        final counts = [b.starts, b.chunks, b.reads, b.finishes];
        await s.refresh();
        await s.verify();
        await s.advance();
        expect([b.starts, b.chunks, b.reads, b.finishes], counts);
        expect(s.canCancel, isTrue);
        await s.cancel();
        expect(b.cancels, 1);
      },
    );
  }
  test(
    'cancel while once-only chunk delivery is in flight suppresses late bytes',
    () async {
      final b = Backend(Uint8List.fromList([1, 2, 3]));
      final s = await started(b);
      await s.advance();
      await s.advance();
      b.heldRead = Completer<void>();
      final pending = s.advance();
      await Future<void>.delayed(Duration.zero);
      expect(s.canCancel, isTrue);
      await s.cancel();
      b.heldRead!.complete();
      await pending;
      expect(s.phase, FileTaskPhase.cancelled);
      expect(s.received, BigInt.zero);
      expect(s.preview, isEmpty);
    },
  );
  test(
    'unexpected worker exit cannot be presented as verified or restart an attempt',
    () async {
      final b = Backend(Uint8List(3));
      final s = await started(b);
      b.reclaimed();
      await s.refresh();
      expect(s.phase, FileTaskPhase.failed);
      await s.verify();
      expect(b.reads, 0);
      expect(b.starts, 1);
    },
  );
  test(
    'repair requires exit, and digest success does not hide failed maintenance',
    () async {
      final b = Backend(Uint8List(3))..recovery = true;
      final s = await started(b);
      await s.verify();
      await s.refresh();
      expect(s.phase, FileTaskPhase.verified);
      expect(s.canRepair, isTrue);
      expect(s.canAcknowledge, isFalse);
      await s.repair();
      expect(s.canAcknowledge, isTrue);
      await s.acknowledge();
      expect(s.canStart, isTrue);
    },
  );
  test(
    'lost acknowledgement is inspected using status without acknowledging again',
    () async {
      final b = Backend(Uint8List(0))..loseAck = true;
      final s = await started(b);
      await s.verify();
      await s.refresh();
      await s.acknowledge();
      expect(s.canStart, isFalse);
      await s.refresh();
      expect(s.canStart, isTrue);
      expect(b.acks, 1);
      expect(s.history.length, 1);
    },
  );
  test(
    'observation time is bounded and pending data is not blindly consumed',
    () async {
      final b = Backend(Uint8List(3))..alwaysPending = true;
      final s = FileTaskSession(
        b,
        b,
        pollInterval: const Duration(milliseconds: 1),
        observationLimit: const Duration(milliseconds: 5),
      );
      await s.refresh();
      await s.start(request(1));
      await s.verify();
      expect(s.phase, FileTaskPhase.failed);
      expect(b.reads, 0);
      expect(b.cancels, 1);
    },
  );
  test(
    'session and consumed progress survive view listener detachment',
    () async {
      final b = Backend(Uint8List(3));
      final s = FileTaskSession.forBackend(b, b);
      var count = 0;
      void listener() {
        count++;
      }

      s.addListener(listener);
      await s.refresh();
      await s.start(request(1));
      s.removeListener(listener);
      final before = count;
      await s.verify();
      expect(count, before);
      expect(identical(s, FileTaskSession.forBackend(b, b)), isTrue);
      expect(s.phase, FileTaskPhase.verified);
      expect(b.starts, 1);
    },
  );
}
