import 'dart:async';
import 'dart:typed_data';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/directory_task_models.dart';
import 'package:morrow_studio/plugins/directory_task_session.dart';
import 'package:morrow_studio/plugins/io_task_models.dart';
import 'directory_task_test_support.dart';

Future<DirectoryTaskSession> start(DirectoryFake backend) async {
  final session = DirectoryTaskSession.forBackend(backend, backend);
  await session.refresh();
  await session.start(directoryRequest());
  return session;
}

void main() {
  test(
    'completion is distinct from actual reclamation and original acknowledgement',
    () async {
      final b = DirectoryFake();
      final s = await start(b);
      await s.execute();
      expect(s.phase, DirectoryTaskPhase.completed);
      expect(s.result!.succeeded, isTrue);
      expect([b.starts, b.runs, b.reads, b.guestReads], [1, 1, 1, 1]);
      expect(s.canStart, isFalse);
      expect(s.canAcknowledge, isFalse);
      b.state = directoryState(b.request, storage: IoStoragePhase.reclaimed);
      await s.refresh();
      expect(s.canAcknowledge, isTrue);
      await s.acknowledge();
      expect(s.canStart, isTrue);
      expect(s.history.single.phase, DirectoryTaskPhase.completed);
    },
  );
  test('lost start recovers only the exact original submission', () async {
    final b = DirectoryFake()..loseStart = true;
    final s = await start(b);
    expect(s.phase, DirectoryTaskPhase.unknown);
    expect(s.notice, DirectoryTaskNotice.startUnknown);
    await s.refresh();
    expect(s.phase, DirectoryTaskPhase.capturing);
    await s.execute();
    expect(s.phase, DirectoryTaskPhase.completed);
    expect(b.starts, 1);
    expect(b.runs, 1);
  });
  test(
    'lost guest command remains unknown after status and later cancellation',
    () async {
      final b = DirectoryFake()..loseRun = true;
      final s = await start(b);
      await s.execute();
      expect(s.notice, DirectoryTaskNotice.commandUnknown);
      await s.refresh();
      await s.execute();
      await s.advance();
      expect(b.runs, 1);
      expect(b.guestReads, 0);
      expect(s.phase, DirectoryTaskPhase.unknown);
      await s.cancel();
      expect(s.phase, DirectoryTaskPhase.unknown);
      b.state = directoryState(b.request, storage: IoStoragePhase.reclaimed);
      await s.refresh();
      await s.acknowledge();
      expect(s.history.single.phase, DirectoryTaskPhase.unknown);
      expect(s.history.single.notice, DirectoryTaskNotice.commandUnknown);
    },
  );
  test(
    'consumed guest reply is never read again after delivery loss',
    () async {
      final b = DirectoryFake()..loseRead = true;
      final s = await start(b);
      await s.execute();
      expect(s.notice, DirectoryTaskNotice.readUnknown);
      await s.refresh();
      await s.execute();
      await s.advance();
      expect(b.guestReads, 1);
      expect(b.runs, 1);
      expect(s.phase, DirectoryTaskPhase.unknown);
    },
  );
  test(
    'cancellation invalidates an in-flight read without releasing the retained owner',
    () async {
      final b = DirectoryFake();
      final s = await start(b);
      await s.advance();
      await s.advance();
      await s.refresh();
      final delayed = Completer<DirectoryTaskRead>();
      b.delayedGuest = () => delayed.future;
      final pending = s.advance();
      await Future<void>.delayed(Duration.zero);
      await s.cancel();
      delayed.complete(
        DirectoryTaskRead(
          snapshot: directoryState(b.request),
          result: DirectoryTaskGuest(
            frame: Uint8List.fromList([1]),
            unknown: false,
            executionFault: 0,
            exitCode: 0,
            hostCalls: 4,
          ),
        ),
      );
      await pending;
      expect(s.phase, DirectoryTaskPhase.cancelled);
      expect(s.result, isNull);
      expect(s.ownsTask, isTrue);
      expect(s.canStart, isFalse);
      expect(b.cancels, 1);
    },
  );
  test(
    'foreign submission cannot recover a lost start or acquire its task',
    () async {
      final b = DirectoryFake()..loseStart = true;
      final s = await start(b);
      b.state = IoTaskSnapshot(
        key: directoryKey,
        submission: Uint8List.fromList(List.filled(32, 99)),
        storage: IoStoragePhase.running,
        delivery: IoDeliveryPhase.ready,
        exit: null,
      );
      await s.refresh();
      expect(s.ownsTask, isFalse);
      expect(s.canExecute, isFalse);
      expect(s.canCancel, isFalse);
      expect(s.phase, DirectoryTaskPhase.unknown);
      expect(b.starts, 1);
    },
  );
  test(
    'lost acknowledgement is observed once and archives the original result',
    () async {
      final b = DirectoryFake()..loseAck = true;
      final s = await start(b);
      await s.execute();
      b.state = directoryState(b.request, storage: IoStoragePhase.reclaimed);
      await s.refresh();
      await s.acknowledge();
      expect(s.attempt, isNotNull);
      expect(s.history, isEmpty);
      await s.refresh();
      expect(s.canStart, isTrue);
      expect(s.history.single.phase, DirectoryTaskPhase.completed);
      expect(b.acks, 1);
    },
  );
  test('empty successful-exit report is not a successful directory result', () {
    expect(
      DirectoryTaskGuest(
        frame: Uint8List(0),
        unknown: false,
        executionFault: 0,
        exitCode: 0,
        hostCalls: 0,
      ).succeeded,
      isFalse,
    );
  });
}
