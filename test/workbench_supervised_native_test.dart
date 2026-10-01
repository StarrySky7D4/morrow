import 'dart:convert';
import 'dart:io';
import 'dart:ffi';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/main.dart';
import 'package:morrow_studio/plugins/workbench_backend.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';
import 'package:morrow_studio/plugins/workbench_channel_supervised.dart';
import 'package:morrow_studio/plugins/workbench_supervision.dart';
import 'package:morrow_studio/plugins/session_coordinator.dart';

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
final _closeHandle = _kernel
    .lookupFunction<Int32 Function(Pointer<Void>), int Function(Pointer<Void>)>(
      'CloseHandle',
    );

class _HeldProcess {
  _HeldProcess(this.pid) : handle = _openProcess(0x00101001, 0, pid) {
    if (handle == nullptr || _processId(handle) != pid) { throw StateError('Owned process handle identity mismatch'); }
  }
  final int pid;
  final Pointer<Void> handle;
  void crash() {
    if (_terminate(handle, 72) == 0) { throw StateError('Owned process fault failed'); }
  }

  Future<Map<String, Object>> exited() async {
    final watch = Stopwatch()..start();
    while (_wait(handle, 0) != 0) {
      if (watch.elapsed > const Duration(seconds: 7)) { throw StateError('Owned process cleanup deadline exceeded'); }
      await Future<void>.delayed(const Duration(milliseconds: 10));
    }
    if (_processId(handle) != pid) { throw StateError('Held handle identity drift'); }
    return {
      'retained_pid': pid,
      'wait_signaled': true,
      'get_process_id_matches': true,
      'elapsed_ms': watch.elapsedMilliseconds,
    };
  }

  void dispose() => _closeHandle(handle);
}

void main() {
  final executable = Platform.environment['MORROW_WORKBENCH_HOST'];
  final supervisor = Platform.environment['MORROW_WORKBENCH_SUPERVISOR'];
  final package = Platform.environment['MORROW_WORKBENCH_PACKAGE'];
  final evidence = Platform.environment['MORROW_PRODUCT_SUPERVISOR_EVIDENCE'];
  final enabled =
      Platform.isWindows &&
      executable != null &&
      supervisor != null &&
      package != null &&
      evidence != null;
  Future<RustWorkbench> open(Directory directory) => RustWorkbench.open(
    executable: executable!,
    supervisorExecutable: supervisor!,
    package: package!,
    directory: directory,
  );
  test(
    'real Flutter client and Wasmi workbench preserve writes across proven normal exit/reopen',
    () async {
      final directory = Directory('$evidence/normal');
      expect(directory.existsSync(), isFalse);
      await directory.create(recursive: true);
      var backend = await open(directory);
      final channel = backend.channel as SupervisedWorkbenchChannel;
      final generation = channel.supervision.value!.record['generation'];
      try {
        expect(await backend.load(), isEmpty);
        await backend.apply(
          PluginAction.create,
          Idea(
            'supervised workbench',
            'real Wasmi write',
            '灵感',
            Idea.icons.first,
            const Color(0xff8866aa),
            id: 'supervised-real-card',
          ),
        );
        await backend.saveUiLocale('en');
        expect(await backend.readUiLocale(), 'en');
      } finally {
        await backend.close();
      }
      expect(channel.supervision.value!.released, isTrue);
      expect(await backend.process.exitCode, 0);
      backend = await open(directory);
      final second = backend.channel as SupervisedWorkbenchChannel;
      try {
        expect(second.supervision.value!.record['generation'], generation + 1);
        expect(
          (await backend.load()).map((v) => v.id),
          contains('supervised-real-card'),
        );
        expect(await backend.readUiLocale(), 'en');
      } finally {
        await backend.close();
      }
      await File('$evidence/normal/result.json').writeAsString(
        jsonEncode({
          'status': 'passed',
          'first': channel.supervision.value!.record,
          'second': second.supervision.value!.record,
          'real_flutter_client': true,
          'actual_wasmi_guest': true,
          'replayed': false,
        }),
      );
    },
    skip: !enabled,
    timeout: const Timeout(Duration(minutes: 2)),
  );
  test(
    'actual supervisor crash keeps library owner retained and rejects product reopen',
    () async {
      final directory = Directory('$evidence/supervisor-crash');
      expect(directory.existsSync(), isFalse);
      await directory.create(recursive: true);
      final backend = await open(directory);
      final channel = backend.channel as SupervisedWorkbenchChannel;
      final initial = Map<String, dynamic>.of(
        channel.supervision.value!.record,
      );
      final session = SessionCoordinator();
      addTearDown(session.dispose);
      await session.run(() async {
        session.attach(
          process: backend.process,
          library: directory.path,
          exited: channel.exitCode,
          transportExited: backend.process.exitCode,
          close: backend.close,
        );
        session.active();
      });
      final hostWatch = _HeldProcess(initial['child_pid'] as int);
      addTearDown(hostWatch.dispose);
      expect(backend.process.kill(), isTrue);
      final hostReaped = await hostWatch.exited();
      await expectLater(
        backend.close(),
        throwsA(isA<WorkbenchSupervisionUnconfirmed>()),
      );
      await Future<void>.delayed(const Duration(milliseconds: 30));
      expect(session.mayRecover, isFalse);
      expect(session.mayCloseWindow, isTrue);
      await expectLater(
        open(directory),
        throwsA(isA<WorkbenchSupervisionUnconfirmed>()),
      );
      await File('$evidence/supervisor-crash/result.json').writeAsString(
        jsonEncode({
          'status': 'passed',
          'initial': initial,
          'actual_supervisor_exit': await backend.process.exitCode,
          'host_handle_proof': hostReaped,
          'may_recover': session.mayRecover,
          'unknown': true,
          'automatic_replay': false,
          'reopen_refused': true,
          'owner_record_preserved': true,
        }),
      );
    },
    skip: !enabled,
    timeout: const Timeout(Duration(minutes: 2)),
  );
  test(
    'actual business host crash has bounded resources and retains unknown owner',
    () async {
      final directory = Directory('$evidence/host-crash');
      expect(directory.existsSync(), isFalse);
      await directory.create(recursive: true);
      final backend = await open(directory);
      final channel = backend.channel as SupervisedWorkbenchChannel;
      final initial = Map<String, dynamic>.of(
        channel.supervision.value!.record,
      );
      final hostWatch = _HeldProcess(initial['child_pid'] as int);
      final supervisorWatch = _HeldProcess(backend.process.pid);
      addTearDown(hostWatch.dispose);
      addTearDown(supervisorWatch.dispose);
      final cleanupWatch = Stopwatch()..start();
      hostWatch.crash();
      await expectLater(
        backend.close(),
        throwsA(isA<WorkbenchSupervisionUnconfirmed>()),
      );
      final hostProof = await hostWatch.exited();
      final supervisorProof = await supervisorWatch.exited();
      expect(cleanupWatch.elapsed, lessThan(const Duration(seconds: 7)));
      expect(channel.supervision.value!.released, isFalse);
      expect(channel.supervision.value!.record['resource_reclaimed'], isTrue);
      expect(channel.supervision.value!.record['owner_phase'], 'Retained');
      await expectLater(
        open(directory),
        throwsA(isA<WorkbenchSupervisionUnconfirmed>()),
      );
      await File('$evidence/host-crash/result.json').writeAsString(
        jsonEncode({
          'status': 'passed',
          'initial': initial,
          'final': channel.supervision.value!.record,
          'host_handle_proof': hostProof,
          'supervisor_handle_proof': supervisorProof,
          'fault_to_cleanup_ms': cleanupWatch.elapsedMilliseconds,
          'unknown': true,
          'reopen_refused': true,
          'automatic_replay': false,
        }),
      );
    },
    skip: !enabled,
    timeout: const Timeout(Duration(minutes: 2)),
  );

  test(
    'real product concurrent owner refuses same library and permits independent library',
    () async {
      final first = Directory('$evidence/concurrent-a');
      final second = Directory('$evidence/concurrent-b');
      expect(first.existsSync(), isFalse);
      expect(second.existsSync(), isFalse);
      await first.create(recursive: true);
      await second.create(recursive: true);
      final a = await open(first);
      final b = await open(second);
      try {
        await expectLater(
          open(first),
          throwsA(isA<WorkbenchSupervisionUnconfirmed>()),
        );
        expect(await a.load(), isEmpty);
        expect(await b.load(), isEmpty);
      } finally {
        await a.close();
        await b.close();
      }
      await File('$evidence/concurrent-result.json').writeAsString(
        jsonEncode({
          'status': 'passed',
          'same_profile_refused': true,
          'independent_profiles_actual_overlap': true,
          'first': (a.channel as SupervisedWorkbenchChannel)
              .supervision
              .value!
              .record,
          'second': (b.channel as SupervisedWorkbenchChannel)
              .supervision
              .value!
              .record,
        }),
      );
    },
    skip: !enabled,
    timeout: const Timeout(Duration(minutes: 2)),
  );
}
