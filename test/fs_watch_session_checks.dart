import 'dart:typed_data';
import 'dart:async';
import 'package:morrow_studio/plugins/fs_watch_models.dart';
import 'package:morrow_studio/plugins/fs_watch_native.dart';
import 'package:morrow_studio/plugins/fs_watch_session.dart';
import 'package:morrow_studio/plugins/io_task_models.dart';
import 'fs_watch_wire_checks.dart' show response, id, request;

int checks = 0;
void check(bool value, String label) {
  if (!value) throw StateError(label);
  checks++;
}

IoTaskSnapshot local() => IoTaskSnapshot(
  storage: IoStoragePhase.local,
  delivery: IoDeliveryPhase.absent,
  exit: null,
);

final class Fake implements FsWatchBackend, WorkbenchIoTaskControl {
  final calls = <String, int>{};
  FsWatchRequest? lastRequest;
  String? fail;
  Completer<void>? startBarrier;
  bool spawn = false, repaired = false, unknown = false, wrongIdentity = false;
  void count(String command) {
    calls.update(command, (n) => n + 1, ifAbsent: () => 1);
    if (fail == command) throw StateError('original $command response lost');
  }

  FsWatchReply reply({bool finished = false, bool receipt = false}) {
    final r = response(finished: finished, receipt: receipt && !spawn);
    if (lastRequest != null) r.ioState.submission = lastRequest!.submission;
    if (wrongIdentity) r.ioState.key = id(99);
    if (spawn && finished) {
      r.ioState.storage = repaired ? 3 : 4;
      r.ioState.execution = IoJobError.spawn.index;
      r.ioState.disconnect = repaired ? 0 : IoJobError.disconnect.index;
      r.ioState.maintenance = repaired ? 0 : IoJobError.unavailable.index;
      r.fsWatchState.phase = 0;
      if (receipt) {
        r.fsWatchState.ready = false;
        r.fsWatchState.delivered = true;
        r.fsWatchResult.present = true;
        r.fsWatchResult.error = FsWatchError.spawn.index;
        r.fsWatchResult.detail = 'Spawn';
        r.fsWatchResult.nativeRetirementComplete = true;
      }
    } else if (unknown && receipt) {
      r.fsWatchResult.success = false;
      r.fsWatchResult.error = FsWatchError.unknown.index;
      r.fsWatchResult.detail = 'Unknown';
      r.fsWatchResult.unknown = true;
      r.fsWatchResult.nativeRetirementComplete = false;
      r.fsWatchResult.nativeClosed = 1;
      r.fsWatchResult.initDeliveries(0);
    }
    return NativeFsWatchClient.decode(r.asReader(), allowResult: receipt);
  }

  @override
  Future<void> approveFsWatch(String a, Uint8List b, BigInt c, bool d) async {
    count('approve');
  }

  @override
  Future<FsWatchSnapshot> startFsWatch(FsWatchRequest r) async {
    lastRequest = r;
    count('start');
    if (startBarrier != null) await startBarrier!.future;
    return reply().snapshot;
  }

  @override
  Future<FsWatchSnapshot> pollFsWatch(Uint8List key) async {
    count('poll');
    return reply(finished: true).snapshot;
  }

  @override
  Future<FsWatchReply> readFsWatch(Uint8List key) async {
    count('read');
    return reply(finished: true, receipt: true);
  }

  @override
  Future<IoTaskSnapshot> ioStatus() async {
    count('status');
    return local();
  }

  @override
  Future<IoTaskSnapshot> cancelIo(Uint8List key) async {
    count('cancel');
    return reply().snapshot.task;
  }

  @override
  Future<IoTaskSnapshot> repairIo(Uint8List key) async {
    count('repair');
    repaired = true;
    return reply(finished: true).snapshot.task;
  }

  @override
  Future<IoTaskSnapshot> acknowledgeIo(Uint8List key) async {
    count('ack');
    return local();
  }

  @override
  Future<IoTaskSnapshot> pollIo(Uint8List key) async =>
      throw StateError('Generic poll forbidden in watch flow');
  @override
  Future<IoTaskRead> readIo(Uint8List key) async =>
      throw StateError('Generic read forbidden in watch flow');
  @override
  Future<IoTaskSnapshot> startHttp(HttpTaskRequest r) async =>
      throw StateError('HTTP dispatch forbidden in watch flow');
}

Future<FsWatchSession> running(Fake backend) async {
  final s = FsWatchSession.forBackend(backend, backend);
  await s.initialize();
  check(s.canStart, 'initial local owner permits admission');
  await s.start(request());
  return s;
}

Future<void> noFurtherCalls(FsWatchSession s, Fake b) async {
  final before = Map.of(b.calls);
  await s.poll();
  await s.initialize();
  check(
    s.phase == FsWatchSessionPhase.unknown &&
        !s.canStart &&
        !s.canRead &&
        !s.canCancel &&
        !s.canRepair &&
        !s.canAcknowledge &&
        !s.canBeginNew,
    'Unknown seals all actions',
  );
  check(
    before.toString() == b.calls.toString(),
    'Unknown makes no new backend call',
  );
}

Future<void> main() async {
  final b = Fake();
  final s = await running(b);
  check(
    identical(s, FsWatchSession.forBackend(b, b)),
    'page changes retain the same owner session',
  );
  void listener() {}
  s.addListener(listener);
  s.removeListener(listener);
  check(
    s.canPoll && s.canCancel && !s.canAcknowledge,
    'removing a page listener neither cancels nor acknowledges',
  );
  await s.poll();
  check(
    s.canRead && !s.canAcknowledge && s.result == null,
    'ready does not authorize acknowledgement',
  );
  await s.read();
  check(
    s.canAcknowledge && !s.canRead && s.result!.success,
    'one original receipt reviewed',
  );
  final original = s.result;
  await s.acknowledge();
  check(
    s.phase == FsWatchSessionPhase.acknowledged && s.canBeginNew && !s.canStart,
    'explicit acknowledgement seals original attempt',
  );
  check(
    b.calls['start'] == 1 && b.calls['read'] == 1 && b.calls['ack'] == 1,
    'commands issued exactly once',
  );
  s.beginNew();
  check(
    s.canStart &&
        s.history.length == 1 &&
        identical(s.history.single.result, original),
    'explicit new observation retains prior evidence',
  );

  for (final lost in ['start', 'poll', 'read', 'cancel', 'ack']) {
    final f = Fake();
    final t = FsWatchSession.forBackend(f, f);
    await t.initialize();
    if (lost == 'start') f.fail = lost;
    await t.start(request());
    if (lost == 'poll') {
      f.fail = lost;
      await t.poll();
    }
    if (lost == 'read' || lost == 'ack') {
      await t.poll();
      if (lost == 'read') f.fail = lost;
      await t.read();
      if (lost == 'ack') {
        f.fail = lost;
        await t.acknowledge();
      }
    }
    if (lost == 'cancel') {
      f.fail = lost;
      await t.cancel();
    }
    await noFurtherCalls(t, f);
    check(f.calls[lost] == 1, 'lost $lost is not replayed');
  }
  final u = Fake()..unknown = true;
  final us = await running(u);
  await us.poll();
  await us.read();
  check(
    us.result!.unknown &&
        us.result!.deliveries.isEmpty &&
        us.snapshot!.frames == 2,
    'actual Unknown receipt and historical progress are retained',
  );
  await noFurtherCalls(us, u);

  final foreign = Fake();
  final fs = await running(foreign);
  foreign.wrongIdentity = true;
  await fs.poll();
  await noFurtherCalls(fs, foreign);

  final refused = Fake()..spawn = true;
  final rs = await running(refused);
  await rs.poll();
  await rs.read();
  check(
    rs.canRepair && !rs.canAcknowledge && !rs.result!.workerJoined,
    'returned spawn failure needs original cleanup',
  );
  await rs.repair();
  check(
    rs.canAcknowledge &&
        rs.result!.disconnect == null &&
        rs.task!.exit!.disconnect == IoJobError.none,
    'fresh cleanup status does not rewrite historical spawn result',
  );
  await rs.acknowledge();
  check(rs.canBeginNew, 'known spawn refusal can finish after actual cleanup');
  final lostRepair = Fake()..spawn = true;
  final lr = await running(lostRepair);
  await lr.poll();
  await lr.read();
  lostRepair.fail = 'repair';
  await lr.repair();
  await noFurtherCalls(lr, lostRepair);
  check(lostRepair.calls['repair'] == 1, 'lost cleanup is never replayed');

  final cancel = Fake();
  final cs = await running(cancel);
  await cs.cancel();
  check(
    !cs.canCancel && cs.canPoll && cancel.calls['cancel'] == 1,
    'cancel request does not imply actual close',
  );
  await cs.poll();
  check(
    cs.canRead && !cs.canAcknowledge,
    'cancel still requires original receipt review',
  );
  final hidden = Fake();
  final hs = await running(hidden);
  await hs.withdrawView();
  await hs.withdrawView();
  check(
    hidden.calls['cancel'] == 1 && hs.canPoll && !hs.canAcknowledge,
    'view withdrawal requests one original stop and retains the owner',
  );
  final pending = Fake()..startBarrier = Completer<void>();
  final ps = FsWatchSession.forBackend(pending, pending);
  await ps.initialize();
  final started = ps.start(request());
  await Future<void>.delayed(Duration.zero);
  await ps.withdrawView();
  check(
    pending.calls['cancel'] == null && ps.busy,
    'withdrawn view does not invent a task key during pending start',
  );
  pending.startBarrier!.complete();
  await started;
  check(
    pending.calls['start'] == 1 && pending.calls['cancel'] == 1 && ps.canPoll,
    'late original start reply receives one original stop',
  );
  print(
    'STAGED_WATCH_SESSION_CHECKS_PASS checks=$checks native_calls=0 UI=false',
  );
}
