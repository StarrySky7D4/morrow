import 'dart:convert';
import 'dart:io';
import 'dart:ffi';
import 'package:ffi/ffi.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/workbench_owner_management.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';
import 'package:morrow_studio/plugins/workbench_channel_supervised.dart';
import 'package:morrow_studio/plugins/workbench_supervision.dart';

// Retain real kernel process handles before injecting an owned-process fault.
final _kernel = DynamicLibrary.open('kernel32.dll');
final _openProcess = _kernel
    .lookupFunction<
      Pointer<Void> Function(Uint32, Int32, Uint32),
      Pointer<Void> Function(int, int, int)
    >('OpenProcess');
final _processId = _kernel
    .lookupFunction<
      Uint32 Function(Pointer<Void>),
      int Function(Pointer<Void>)
    >('GetProcessId');
final _terminate = _kernel
    .lookupFunction<
      Int32 Function(Pointer<Void>, Uint32),
      int Function(Pointer<Void>, int)
    >('TerminateProcess');
final _wait = _kernel
    .lookupFunction<
      Uint32 Function(Pointer<Void>, Uint32),
      int Function(Pointer<Void>, int)
    >('WaitForSingleObject');
final _processTimes = _kernel
    .lookupFunction<
      Int32 Function(
        Pointer<Void>,
        Pointer<Uint64>,
        Pointer<Uint64>,
        Pointer<Uint64>,
        Pointer<Uint64>,
      ),
      int Function(
        Pointer<Void>,
        Pointer<Uint64>,
        Pointer<Uint64>,
        Pointer<Uint64>,
        Pointer<Uint64>,
      )
    >('GetProcessTimes');
final _exitCode = _kernel
    .lookupFunction<
      Int32 Function(Pointer<Void>, Pointer<Uint32>),
      int Function(Pointer<Void>, Pointer<Uint32>)
    >('GetExitCodeProcess');
final _closeHandle = _kernel
    .lookupFunction<Int32 Function(Pointer<Void>), int Function(Pointer<Void>)>(
      'CloseHandle',
    );

class _HeldProcess {
  _HeldProcess(this.pid, {int? expectedCreation})
    : handle = _openProcess(0x00101001, 0, pid) {
    if (handle == nullptr || _processId(handle) != pid) {
      if (handle != nullptr) _closeHandle(handle);
      throw StateError('Owned process handle identity mismatch');
    }
    final times = calloc<Uint64>(4);
    try {
      if (_processTimes(handle, times, times + 1, times + 2, times + 3) == 0) {
        throw StateError('Owned process creation query failed');
      }
      creationFiletime = times.value;
      if (expectedCreation != null && expectedCreation != creationFiletime) {
        throw StateError('Owned process creation identity mismatch');
      }
    } catch (_) {
      _closeHandle(handle);
      rethrow;
    } finally {
      calloc.free(times);
    }
  }
  final int pid;
  final Pointer<Void> handle;
  late final int creationFiletime;
  void crash() {
    if (_terminate(handle, 72) == 0) {
      throw StateError('Owned process fault failed');
    }
  }

  Future<Map<String, Object>> exited() async {
    final watch = Stopwatch()..start();
    while (_wait(handle, 0) != 0) {
      if (watch.elapsed > const Duration(seconds: 7)) {
        throw StateError('Owned process cleanup deadline exceeded');
      }
      await Future<void>.delayed(const Duration(milliseconds: 10));
    }
    if (_processId(handle) != pid) {
      throw StateError('Held handle identity drift');
    }
    final code = calloc<Uint32>();
    final int exitCode;
    try {
      if (_exitCode(handle, code) == 0) {
        throw StateError('Held process exit query failed');
      }
      exitCode = code.value;
    } finally {
      calloc.free(code);
    }
    return {
      'retained_pid': pid,
      'creation_filetime': creationFiletime,
      'exit_code': exitCode,
      'wait_signaled': true,
      'get_process_id_matches': true,
      'elapsed_ms': watch.elapsedMilliseconds,
    };
  }

  void dispose() {
    // A failed assertion must still reclaim only this test's retained process.
    try {
      if (_wait(handle, 0) == 258) {
        crash();
        if (_wait(handle, 7000) != 0) {
          throw StateError('Owned teardown cleanup deadline exceeded');
        }
      }
    } finally {
      _closeHandle(handle);
    }
  }
}

void main() {
  final host = Platform.environment['MORROW_WORKBENCH_HOST'] ?? '';
  final supervisor = Platform.environment['MORROW_WORKBENCH_SUPERVISOR'] ?? '';
  final package = Platform.environment['MORROW_WORKBENCH_PACKAGE'] ?? '';
  final evidence =
      Platform.environment['MORROW_PRODUCT_SUPERVISOR_EVIDENCE'] ?? '';
  final enabled =
      Platform.isWindows &&
      host.isNotEmpty &&
      supervisor.isNotEmpty &&
      package.isNotEmpty &&
      evidence.isNotEmpty;
  Future<RustWorkbench> open(Directory dir) => RustWorkbench.open(
    executable: host,
    supervisorExecutable: supervisor,
    package: package,
    directory: dir,
  );
  WorkbenchOwnerManager manager(Directory dir) => WorkbenchOwnerManager(
    supervisor: supervisor,
    host: host,
    package: package,
    directory: dir,
  );
  Future<void> save(String name, Map<String, dynamic> data) async =>
      File('$evidence/$name.json').writeAsString('${jsonEncode(data)}\n');
  test(
    'actual live owner refuses transient recovery without changing resources or identity',
    () async {
      final dir = Directory('$evidence/live-owner-library');
      await dir.create();
      final backend = await open(dir);
      final channel = backend.channel as SupervisedWorkbenchChannel;
      final record = channel.supervision.value!.record;
      final child = _HeldProcess(
        record['child_pid'] as int,
        expectedCreation: record['child_creation_filetime'] as int,
      );
      final sup = _HeldProcess(
        backend.process.pid,
        expectedCreation: record['supervisor_creation_filetime'] as int,
      );
      addTearDown(child.dispose);
      addTearDown(sup.dispose);
      final control = manager(dir);
      final preview = await control.preview();
      expect(preview.eligible, isFalse);
      expect(preview.reason, 'original_resource_proof_missing');
      final refusal = await Process.run(supervisor, [
        '--owner-recover',
        dir.path,
        host,
        package,
        preview.token,
        '--acknowledge-unknown',
      ]);
      expect(refusal.exitCode, 2);
      final after = await control.preview();
      expect(after.value['original_digest'], preview.value['original_digest']);
      expect(after.record['generation'], record['generation']);
      expect(after.record['incarnation'], record['incarnation']);
      expect(_wait(child.handle, 0), 258);
      expect(_wait(sup.handle, 0), 258);
      await backend.close();
      expect(await channel.exitCode, 0);
      await save('live-owner-refused', {
        'preview': preview.value,
        'after': after.value,
        'refusal_exit': refusal.exitCode,
        'child': await child.exited(),
        'supervisor': await sup.exited(),
        'live_before_close': true,
        'automatic_replay': false,
      });
    },
    skip: !enabled,
    timeout: const Timeout(Duration(seconds: 50)),
  );
  test(
    'actual host crash complete proof permits explicit recovery and preserves Unknown',
    () async {
      final dir = Directory('$evidence/host-crash-library');
      await dir.create();
      final backend = await open(dir);
      final channel = backend.channel as SupervisedWorkbenchChannel;
      final status = channel.supervision.value!;
      final child = _HeldProcess(
        status.record['child_pid'] as int,
        expectedCreation: status.record['child_creation_filetime'] as int,
      );
      final sup = _HeldProcess(
        backend.process.pid,
        expectedCreation:
            channel.supervision.value!.record['supervisor_creation_filetime']
                as int,
      );
      addTearDown(child.dispose);
      addTearDown(sup.dispose);
      final unconfirmed = expectLater(
        channel.exitCode,
        throwsA(isA<WorkbenchSupervisionUnconfirmed>()),
      );
      child.crash();
      final childProof = await child.exited();
      final supProof = await sup.exited();
      await unconfirmed;
      expect(await backend.process.exitCode, 2);
      final control = manager(dir);
      final preview = await control.preview();
      expect(preview.eligible, isTrue);
      final recovered = await control.recover(preview);
      expect(recovered.record['normal_shutdown'], isFalse);
      expect(recovered.record['business_outcome'], 'Unknown');
      expect(recovered.record['generation'], 1);
      expect(recovered.record['phase'], 'Recovered');
      final direct = await Process.run(host, [
        '${dir.path}/workbench.db',
        package,
      ]);
      expect(direct.exitCode, isNot(0));
      final oldToken = await Process.run(supervisor, [
        '--owner-recover',
        dir.path,
        host,
        package,
        preview.token,
        '--acknowledge-unknown',
      ]);
      expect(oldToken.exitCode, 2);
      final next = await open(dir);
      final nextStatus =
          (next.channel as SupervisedWorkbenchChannel).supervision.value!;
      expect(nextStatus.record['generation'], 2);
      expect(
        nextStatus.record['incarnation'],
        isNot(preview.record['incarnation']),
      );
      await next.close();
      expect(await next.channel.exitCode, 0);
      await save('host-crash-explicit-recovery', {
        'preview': preview.value,
        'recovery': recovered.value,
        'child': childProof,
        'supervisor': supProof,
        'old_exit_code': 2,
        'direct_host_refused': direct.exitCode,
        'stale_preview_refused': oldToken.exitCode,
        'next_session': nextStatus.record,
        'next_verified_exit': 0,
      });
    },
    skip: !enabled,
    timeout: const Timeout(Duration(seconds: 50)),
  );
  test(
    'actual supervisor crash before proof refuses recovery and leaves original owner unchanged',
    () async {
      final dir = Directory('$evidence/supervisor-crash-library');
      await dir.create();
      final backend = await open(dir);
      final channel = backend.channel as SupervisedWorkbenchChannel;
      final child = _HeldProcess(
        channel.supervision.value!.record['child_pid'] as int,
        expectedCreation:
            channel.supervision.value!.record['child_creation_filetime'] as int,
      );
      final sup = _HeldProcess(
        backend.process.pid,
        expectedCreation:
            channel.supervision.value!.record['supervisor_creation_filetime']
                as int,
      );
      addTearDown(child.dispose);
      addTearDown(sup.dispose);
      final unconfirmed = expectLater(
        channel.exitCode,
        throwsA(isA<WorkbenchSupervisionUnconfirmed>()),
      );
      sup.crash();
      final supProof = await sup.exited();
      final childProof = await child.exited();
      await unconfirmed;
      expect(await backend.process.exitCode, 72);
      final control = manager(dir);
      final preview = await control.preview();
      expect(preview.eligible, isFalse);
      expect(preview.reason, 'original_resource_proof_missing');
      final refused = await Process.run(supervisor, [
        '--owner-recover',
        dir.path,
        host,
        package,
        preview.token,
        '--acknowledge-unknown',
      ]);
      expect(refused.exitCode, 2);
      await expectLater(open(dir), throwsA(anything));
      final after = await control.preview();
      expect(after.value['original_digest'], preview.value['original_digest']);
      await save('supervisor-crash-unprovable', {
        'preview': preview.value,
        'after': after.value,
        'supervisor': supProof,
        'child': childProof,
        'recover_refused': refused.exitCode,
        'original_digest_unchanged': true,
      });
    },
    skip: !enabled,
    timeout: const Timeout(Duration(seconds: 50)),
  );
  test(
    'two real transient managers share one CAS recovery; same library still has one next owner',
    () async {
      final dir = Directory('$evidence/concurrent-recovery-library');
      await dir.create();
      final backend = await open(dir);
      final channel = backend.channel as SupervisedWorkbenchChannel;
      final child = _HeldProcess(
        channel.supervision.value!.record['child_pid'] as int,
        expectedCreation:
            channel.supervision.value!.record['child_creation_filetime'] as int,
      );
      final sup = _HeldProcess(
        backend.process.pid,
        expectedCreation:
            channel.supervision.value!.record['supervisor_creation_filetime']
                as int,
      );
      addTearDown(child.dispose);
      addTearDown(sup.dispose);
      final unconfirmed = expectLater(
        channel.exitCode,
        throwsA(isA<WorkbenchSupervisionUnconfirmed>()),
      );
      child.crash();
      final childProof = await child.exited();
      final supProof = await sup.exited();
      await unconfirmed;
      final preview = await manager(dir).preview();
      expect(preview.eligible, isTrue);
      final args = [
        '--owner-recover',
        dir.path,
        host,
        package,
        preview.token,
        '--acknowledge-unknown',
      ];
      final results = await Future.wait([
        Process.run(supervisor, args),
        Process.run(supervisor, args),
      ]);
      expect(results.map((r) => r.exitCode).where((c) => c == 0).length, 1);
      expect(results.map((r) => r.exitCode).where((c) => c == 2).length, 1);
      final next = await open(dir);
      await expectLater(open(dir), throwsA(anything));
      final nextRecord = (next.channel as SupervisedWorkbenchChannel)
          .supervision
          .value!
          .record;
      expect(nextRecord['generation'], 2);
      await next.close();
      expect(await next.channel.exitCode, 0);
      await save('concurrent-managers', {
        'preview': preview.value,
        'manager_results': [
          for (final r in results)
            {'exit_code': r.exitCode, 'stdout': r.stdout, 'stderr': r.stderr},
        ],
        'child': childProof,
        'supervisor': supProof,
        'next_session': nextRecord,
        'duplicate_next_owner_refused': true,
        'next_verified_exit': 0,
      });
    },
    skip: !enabled,
    timeout: const Timeout(Duration(seconds: 50)),
  );
}
