import 'dart:async';
import 'dart:typed_data';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/directory_task_models.dart';
import 'package:morrow_studio/plugins/directory_task_session.dart';
import 'package:morrow_studio/plugins/io_task_models.dart';
import 'directory_task_test_support.dart';
import 'directory_native_picker_test_support.dart';

class DelayedCapture extends DirectoryFake {
  final captureReply = Completer<DirectoryTaskRead>();
  @override
  Future<DirectoryTaskRead> readDirectory(Uint8List key) {
    reads++;
    return captureReply.future;
  }
}

void main() {
  test('view loss before START reply cancels its exact owner once', () async {
    final b = PendingNativeDirectory();
    final s = DirectoryTaskSession(b, b);
    await s.refresh();
    final start = s.start(nativeDirectoryRequest());
    await s.revokeNativePickerView();
    await s.revokeNativePickerView();
    expect([b.starts, b.cancels, b.runs, b.statusReads], [1, 0, 0, 1]);
    b.startReply.complete(b.state);
    await start;
    await s.execute();
    await s.revokeNativePickerView();
    expect([b.starts, b.cancels, b.runs, b.statusReads], [1, 1, 0, 1]);
    expect(s.ownsTask, isTrue);
    expect(s.canStart, isFalse);
    expect(s.phase, DirectoryTaskPhase.cancelled);
  });

  test('lost START after view loss remains Unknown without a probe', () async {
    final b = PendingNativeDirectory();
    final s = DirectoryTaskSession(b, b);
    await s.refresh();
    final start = s.start(nativeDirectoryRequest());
    await s.revokeNativePickerView();
    b.startReply.completeError(StateError('reply lost'));
    await start;
    await s.revokeNativePickerView();
    await s.execute();
    expect([b.starts, b.cancels, b.runs, b.statusReads], [1, 0, 0, 1]);
    expect(s.phase, DirectoryTaskPhase.unknown);
    expect(s.attempt, isNotNull);
    expect(s.ownsTask, isFalse);
    expect(s.canStart, isFalse);
  });

  test(
    'explicit status recovery cannot revive revoked native admission',
    () async {
      final b = PendingNativeDirectory();
      final s = DirectoryTaskSession(b, b);
      await s.refresh();
      final start = s.start(nativeDirectoryRequest());
      await s.revokeNativePickerView();
      b.startReply.completeError(StateError('reply lost'));
      await start;
      await s.refresh();
      await s.execute();
      expect(s.ownsTask, isTrue);
      expect(s.phase, DirectoryTaskPhase.unknown);
      expect(s.canExecute, isFalse);
      expect([b.starts, b.runs, b.cancels], [1, 0, 0]);
    },
  );

  test(
    'late captured delivery after view loss cannot invoke the plugin',
    () async {
      final b = DelayedCapture();
      final s = DirectoryTaskSession(b, b);
      await s.refresh();
      await s.start(nativeDirectoryRequest());
      final read = s.advance();
      await s.revokeNativePickerView();
      b.captureReply.complete(
        DirectoryTaskRead(
          snapshot: directoryState(b.request),
          result: DirectoryTaskCaptured(1, BigInt.one),
        ),
      );
      await read;
      await s.execute();
      expect(s.captured, isNull);
      expect(s.ownsTask, isTrue);
      expect(s.phase, DirectoryTaskPhase.cancelled);
      expect([b.reads, b.cancels, b.runs], [1, 1, 0]);
    },
  );

  test(
    'view revocation does not claim cancellation of an unknown delivery',
    () async {
      final b = DirectoryFake()..loseRead = true;
      final s = DirectoryTaskSession(b, b);
      await s.refresh();
      await s.start(nativeDirectoryRequest());
      await s.execute();
      await s.revokeNativePickerView();
      expect(s.phase, DirectoryTaskPhase.unknown);
      expect(s.ownsTask, isTrue);
      expect([b.starts, b.runs, b.guestReads, b.cancels], [1, 1, 1, 0]);
    },
  );

  test('completed native execution is retained when its view leaves', () async {
    final b = DirectoryFake();
    final s = DirectoryTaskSession(b, b);
    await s.refresh();
    await s.start(nativeDirectoryRequest());
    await s.execute();
    await s.revokeNativePickerView();
    expect(s.phase, DirectoryTaskPhase.completed);
    expect(s.ownsTask, isTrue);
    expect(b.cancels, 0);
    expect(s.canAcknowledge, isFalse);
  });

  test(
    'checked acknowledgement clears revocation for a new explicit task',
    () async {
      final b = DirectoryFake();
      final s = DirectoryTaskSession(b, b);
      await s.refresh();
      await s.start(nativeDirectoryRequest());
      await s.revokeNativePickerView();
      b.state = directoryState(b.request, storage: IoStoragePhase.reclaimed);
      await s.refresh();
      await s.acknowledge();
      await s.start(nativeDirectoryRequest());
      await s.execute();
      expect([b.starts, b.cancels, b.runs, b.acks], [2, 1, 1, 1]);
      expect(s.phase, DirectoryTaskPhase.completed);
      expect(s.history.single.phase, DirectoryTaskPhase.cancelled);
    },
  );
}
