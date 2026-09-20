import 'dart:async';
import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/io_task_control.dart';
import 'package:morrow_studio/plugins/service_run_control.dart';
import 'package:morrow_studio/plugins/service_run_session.dart';

Uint8List id(int n) => Uint8List.fromList(List.filled(32, n));
ServiceRunRequest request([int n = 1]) => ServiceRunRequest(
  submission: id(n),
  configId: 'config',
  configDigest: id(2),
  configRevision: BigInt.one,
  publication: id(3),
  publicationRevision: BigInt.one,
  packageId: 'package',
  packageDigest: id(4),
  registryRevision: BigInt.one,
  lifetimeMs: 30000,
  maxJobs: BigInt.from(8),
  maxBytes: BigInt.from(65536),
  maxJobBytes: BigInt.from(65536),
  maxTotalBytes: BigInt.from(65536),
);
IoTaskSnapshot local() => IoTaskSnapshot(
  storage: IoStoragePhase.local,
  delivery: IoDeliveryPhase.absent,
  exit: null,
);
ServiceRunSnapshot run({
  int key = 10,
  int submission = 1,
  ServiceRunPhase phase = ServiceRunPhase.running,
  IoStoragePhase? storage,
}) => ServiceRunSnapshot(
  task: IoTaskSnapshot(
    key: id(key),
    submission: id(submission),
    storage:
        storage ??
        (phase == ServiceRunPhase.exited
            ? IoStoragePhase.reclaimed
            : phase == ServiceRunPhase.stopping
            ? IoStoragePhase.stopping
            : IoStoragePhase.running),
    delivery: IoDeliveryPhase.absent,
    exit: phase == ServiceRunPhase.exited
        ? const IoTaskExit(
            execution: IoJobError.none,
            disconnect: IoJobError.none,
            maintenance: IoJobError.none,
          )
        : null,
  ),
  submission: id(submission),
  phase: phase,
  address: '127.0.0.1:1234',
  bind: ServiceNetworkOutcome.succeeded,
  listener: ServiceNetworkOutcome.pending,
  supervision: ServiceNetworkOutcome.pending,
);

class FakeBackend
    implements WorkbenchServiceRunControl, WorkbenchIoTaskControl {
  IoTaskSnapshot current = local();
  ServiceRunSnapshot? service;
  int starts = 0, stops = 0, repairs = 0, acknowledgements = 0;
  bool failStatus = false, failControl = false;
  Completer<IoTaskSnapshot>? pendingStatus;
  Future<ServiceRunSnapshot> Function(ServiceRunRequest)? onStart;
  void Function()? beforeStatus;
  final List<Uint8List?> queriedKeys = [];
  void set(ServiceRunSnapshot value) {
    service = value;
    current = value.task;
  }

  @override
  Future<IoTaskSnapshot> ioStatus() async {
    beforeStatus?.call();
    if (failStatus) throw StateError('status failed');
    if (pendingStatus != null) return pendingStatus!.future;
    return current;
  }

  @override
  Future<ServiceRunSnapshot> serviceRunStatus({Uint8List? key}) async {
    queriedKeys.add(key);
    if (service == null) throw StateError('not a service');
    return service!;
  }

  @override
  Future<ServiceRunSnapshot> startServiceRun(ServiceRunRequest value) async {
    starts++;
    if (onStart != null) return onStart!(value);
    final valueRun = run(submission: value.submission.first);
    set(valueRun);
    return valueRun;
  }

  @override
  Future<IoTaskSnapshot> cancelIo(Uint8List key) async {
    stops++;
    expect(key, current.key);
    if (failControl) throw StateError('lost stop');
    set(
      run(
        key: key.first,
        submission: service!.submission.first,
        phase: ServiceRunPhase.stopping,
      ),
    );
    return current;
  }

  @override
  Future<IoTaskSnapshot> repairIo(Uint8List key) async {
    repairs++;
    expect(key, current.key);
    if (failControl) throw StateError('lost repair');
    set(
      run(
        key: key.first,
        submission: service!.submission.first,
        phase: ServiceRunPhase.exited,
      ),
    );
    return current;
  }

  @override
  Future<IoTaskSnapshot> acknowledgeIo(Uint8List key) async {
    acknowledgements++;
    expect(key, current.key);
    if (failControl) throw StateError('lost acknowledgement');
    service = null;
    current = local();
    return current;
  }

  @override
  dynamic noSuchMethod(Invocation invocation) => super.noSuchMethod(invocation);
}

void main() {
  test(
    'backend disposal during preflight prevents later start and stop mutations',
    () async {
      for (final stopping in [false, true]) {
        final backend = FakeBackend();
        if (stopping) backend.set(run());
        final session = ServiceRunSession(backend, backend);
        await session.refresh();
        final pending = backend.pendingStatus = Completer<IoTaskSnapshot>();
        final operation = stopping ? session.stop() : session.start(request());
        session.dispose();
        pending.complete(backend.current);
        await operation;
        expect(backend.starts + backend.stops, 0);
        expect(
          session.canStart || session.canStop || session.shouldPoll,
          isFalse,
        );
      }
    },
  );

  test(
    'an exited label without actual worker exit cannot enable acknowledgement',
    () async {
      final backend = FakeBackend();
      final live = run();
      backend.set(
        ServiceRunSnapshot(
          task: live.task,
          submission: live.submission,
          phase: ServiceRunPhase.exited,
          address: live.address,
          bind: live.bind,
          listener: live.listener,
          supervision: live.supervision,
        ),
      );
      final session = ServiceRunSession(backend, backend);
      await session.refresh();
      expect(session.trusted, isFalse);
      expect(session.canAcknowledge, isFalse);
      await session.acknowledge();
      expect(backend.acknowledgements, 0);
    },
  );

  test(
    'a mismatched start receipt retains the original uncertain attempt',
    () async {
      final backend = FakeBackend();
      final session = ServiceRunSession(backend, backend);
      await session.refresh();
      backend.onStart = (_) async {
        final wrong = run(submission: 9);
        backend.set(wrong);
        return wrong;
      };
      await session.start(request());
      expect(session.notice, ServiceRunNotice.identityChanged);
      expect(session.startUnknown, isTrue);
      expect(session.attempt!.submission, id(1));
      expect(session.canStart || session.canStop, isFalse);
      await session.start(request(2));
      expect(backend.starts, 1);
      await session.refresh();
      expect(session.notice, ServiceRunNotice.identityChanged);
    },
  );

  test(
    'backend-pair session retains an in-flight start across page listener replacement',
    () async {
      final backend = FakeBackend();
      final session = ServiceRunSession.forBackend(backend, backend);
      expect(session.canStart, isFalse);
      await session.refresh();
      expect(session.canStart, isTrue);
      final receipt = Completer<ServiceRunSnapshot>();
      backend.onStart = (_) => receipt.future;
      void listener() {}
      session.addListener(listener);
      final operation = session.start(request());
      await Future<void>.delayed(Duration.zero);
      expect(session.busy, isTrue);
      expect(backend.starts, 1);
      session.removeListener(listener);
      final reopened = ServiceRunSession.forBackend(backend, backend);
      expect(identical(reopened, session), isTrue);
      expect(reopened.attempt!.submission, id(1));
      await reopened.start(request(2));
      expect(backend.starts, 1);
      final value = run();
      backend.set(value);
      receipt.complete(value);
      await operation;
      expect(reopened.canStop, isTrue);
      expect(reopened.startUnknown, isFalse);
      expect(
        identical(
          ServiceRunSession.forBackend(backend, FakeBackend()),
          session,
        ),
        isFalse,
      );
    },
  );

  test(
    'lost start receipt is reconciled by original submission without resubmission',
    () async {
      final backend = FakeBackend();
      final session = ServiceRunSession(backend, backend);
      await session.refresh();
      backend.onStart = (_) async {
        backend.set(run());
        throw StateError('lost start');
      };
      await session.start(request());
      expect(session.startUnknown, isTrue);
      expect(session.trusted, isFalse);
      expect(session.notice, ServiceRunNotice.startUnknown);
      await session.start(request());
      expect(backend.starts, 1);
      await session.refresh();
      expect(session.startUnknown, isFalse);
      expect(session.canStop, isTrue);
      expect(backend.queriedKeys.single, id(10));
      expect(backend.starts, 1);
    },
  );

  test(
    'unknown start never adopts a service with a different submission',
    () async {
      final backend = FakeBackend();
      final session = ServiceRunSession(backend, backend);
      await session.refresh();
      backend.onStart = (_) async {
        throw StateError('lost');
      };
      await session.start(request());
      backend.set(run(submission: 9));
      await session.refresh();
      expect(session.notice, ServiceRunNotice.identityChanged);
      expect(session.startUnknown, isTrue);
      expect(session.canStop, isFalse);
      expect(session.canAbandon, isFalse);
      await session.stop();
      expect(backend.stops, 0);
      expect(session.attempt!.submission, id(1));
    },
  );

  test(
    'fresh empty task permits explicit abandon but preserves unknown evidence',
    () async {
      final backend = FakeBackend();
      final session = ServiceRunSession(backend, backend);
      await session.refresh();
      backend.onStart = (_) async {
        throw StateError('lost');
      };
      await session.start(request());
      expect(session.canAbandon, isFalse);
      session.abandonAttempt();
      expect(session.attempt, isNotNull);
      await session.refresh();
      expect(session.canAbandon, isTrue);
      expect(session.canStart, isFalse);
      session.abandonAttempt();
      expect(session.history.single.outcomeUnknown, isTrue);
      expect(session.history.single.abandoned, isTrue);
      expect(session.canStart, isTrue);
      await session.start(request());
      expect(session.notice, ServiceRunNotice.invalid);
      expect(backend.starts, 1);
      for (var n = 2; n <= 7; n++) {
        await session.start(request(n));
        await session.refresh();
        session.abandonAttempt();
      }
      expect(session.history, hasLength(5));
      expect(session.history.first.request.submission, id(3));
      expect(session.canStart, isTrue);
      expect(backend.starts, 7);
    },
  );

  test(
    'foreign short HTTP task is not presented or controlled as a service',
    () async {
      final backend = FakeBackend();
      backend.current = IoTaskSnapshot(
        key: id(20),
        submission: id(21),
        storage: IoStoragePhase.running,
        delivery: IoDeliveryPhase.ready,
        exit: null,
      );
      final session = ServiceRunSession(backend, backend);
      await session.refresh();
      expect(session.task!.key, id(20));
      expect(session.service, isNull);
      expect(session.trusted, isFalse);
      expect(
        session.canStart ||
            session.canStop ||
            session.canRepair ||
            session.canAcknowledge,
        isFalse,
      );
      await session.stop();
      await session.repair();
      await session.acknowledge();
      expect(backend.stops + backend.repairs + backend.acknowledgements, 0);
      expect(backend.queriedKeys.single, id(20));
    },
  );

  test(
    'stop rechecks current identity before sending and cannot stop a replacement task',
    () async {
      final backend = FakeBackend()..set(run());
      final session = ServiceRunSession(backend, backend);
      await session.refresh();
      expect(session.canStop, isTrue);
      backend.set(run(key: 11, submission: 2));
      await session.stop();
      expect(backend.stops, 0);
      expect(session.notice, ServiceRunNotice.identityChanged);
      expect(session.trusted, isFalse);
      expect(session.task!.key, id(11));
    },
  );

  test(
    'stop repair and acknowledgement require actual task lifecycle evidence',
    () async {
      final backend = FakeBackend()..set(run());
      final session = ServiceRunSession(backend, backend);
      await session.refresh();
      await session.acknowledge();
      expect(backend.acknowledgements, 0);
      await session.stop();
      expect(backend.stops, 1);
      expect(session.canStop, isFalse);
      expect(session.shouldPoll, isTrue);
      expect(session.canAcknowledge, isFalse);
      await session.acknowledge();
      expect(backend.acknowledgements, 0);
      backend.set(
        run(
          phase: ServiceRunPhase.exited,
          storage: IoStoragePhase.recoveryRequired,
        ),
      );
      await session.refresh();
      expect(session.canRepair, isTrue);
      expect(session.canAcknowledge, isFalse);
      await session.repair();
      expect(backend.repairs, 1);
      expect(session.canAcknowledge, isTrue);
      expect(session.canRepair, isFalse);
      await session.acknowledge();
      expect(backend.acknowledgements, 1);
      expect(session.task!.storage, IoStoragePhase.local);
      expect(session.canStart, isTrue);
    },
  );

  test(
    'control and status failures disable controls until explicit refresh',
    () async {
      final backend = FakeBackend()..set(run());
      final session = ServiceRunSession(backend, backend);
      await session.refresh();
      backend.failControl = true;
      await session.stop();
      expect(session.notice, ServiceRunNotice.controlUnknown);
      expect(session.trusted, isFalse);
      expect(session.shouldPoll, isFalse);
      await session.stop();
      expect(backend.stops, 1);
      backend.failStatus = true;
      await session.refresh();
      expect(session.notice, ServiceRunNotice.statusFailed);
      expect(session.canStop, isFalse);
      backend.failStatus = false;
      backend.failControl = false;
      await session.refresh();
      expect(session.canStop, isTrue);
    },
  );

  test(
    'start preflight detects a task created since the last local observation',
    () async {
      final backend = FakeBackend();
      final session = ServiceRunSession(backend, backend);
      await session.refresh();
      backend.set(run(submission: 8));
      await session.start(request());
      expect(backend.starts, 0);
      expect(session.trusted, isFalse);
      expect(session.startUnknown, isFalse);
    },
  );
}
