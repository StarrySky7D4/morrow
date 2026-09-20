// Real SQLite write contention in a new test library. No production fault flag,
// database mutation, replacement owner, or synthetic lifecycle response is used.
import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/io_task_models.dart';
import 'package:morrow_studio/plugins/service_run_control.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';

import 'service_run_real_native_fixture.dart';

const helper = 'test/fixtures/sqlite_seal_contention.py';
String get python => Platform.environment['MORROW_CLOSE_TEST_PYTHON']!;

Future<Map<String, dynamic>> storeSnapshot(String database) async {
  final result = await Process.run(python, [helper, 'snapshot', database]);
  expect(result.exitCode, 0, reason: '${result.stderr}');
  return jsonDecode(result.stdout as String) as Map<String, dynamic>;
}

class WriterLock {
  WriterLock(this.process, this.errors, this.snapshot);
  final Process process;
  final Future<String> errors;
  final Map<String, dynamic> snapshot;
  bool released = false;

  static Future<WriterLock> acquire(String database) async {
    final process = await Process.start(python, [helper, 'lock', database]);
    final errors = process.stderr.transform(utf8.decoder).join();
    try {
      final line = await process.stdout
          .transform(utf8.decoder)
          .transform(const LineSplitter())
          .first
          .timeout(const Duration(seconds: 10));
      return WriterLock(
        process,
        errors,
        jsonDecode(line) as Map<String, dynamic>,
      );
    } catch (_) {
      await process.stdin.close();
      await process.exitCode;
      await errors;
      rethrow;
    }
  }

  Future<void> release() async {
    if (released) return;
    released = true;
    // The helper responds to a line without requiring EOF. Wait for actual
    // process exit before closing stdin so the release contract is exercised.
    process.stdin.writeln('release');
    await process.stdin.flush();
    final code = await process.exitCode;
    await process.stdin.close();
    expect(code, 0, reason: await errors);
  }
}

void main() {
  final skip =
      !RealServiceFixture.available ||
      !Platform.environment.containsKey('MORROW_CLOSE_TEST_PYTHON');
  test(
    'real seal contention preserves original owner and needs explicit repair',
    () async {
      final f = await RealServiceFixture.open();
      WriterLock? lock;
      try {
        final database = '${f.directory.path}/workbench.db';
        await f.session.start(f.request());
        final live = await f.observe(ServiceRunPhase.running);
        final key = live.task.key!;
        final response = await f.post();
        expect(response, startsWith('HTTP/1.1 202 '));
        expect(response, endsWith('executed-before'));
        lock = await WriterLock.acquire(database);
        final before = lock.snapshot;
        expect(before['pending'], greaterThan(0));
        await f.session.stop();
        final failed = await f.observe(ServiceRunPhase.exited);
        expect(failed.task.key, key);
        expect(failed.submission, live.submission);
        expect(failed.task.storage, IoStoragePhase.recoveryRequired);
        final historicalExit = failed.task.exit!;
        expect(historicalExit.maintenance, isNot(IoJobError.none));
        expect(f.session.canRepair, isTrue);
        expect(f.session.canAcknowledge || f.session.canStart, isFalse);
        expect(await f.backend.readUiLocale(), 'zh');
        await expectLater(
          f.backend.saveUiLocale('en'),
          throwsA(
            isA<StateError>().having(
              (error) => error.message,
              "reason",
              contains("需要恢复"),
            ),
          ),
        );
        await expectLater(
          f.backend.acknowledgeIo(key),
          throwsA(
            isA<StateError>().having(
              (error) => error.message,
              "reason",
              contains("需要恢复"),
            ),
          ),
        );
        await expectLater(
          f.backend.startServiceRun(f.request(submission: 32)),
          throwsA(isA<ServiceRunStartFailure>()),
        );
        expect(await storeSnapshot(database), before);
        await expectLater(
          f.post(second: true),
          throwsA(isA<SocketException>()),
        );
        // A repair with the same real lock still held must not claim recovery.
        await f.session.repair();
        expect(f.session.trusted, isFalse);
        await f.session.refresh();
        expect(f.session.task!.storage, IoStoragePhase.recoveryRequired);
        expect(f.session.task!.key, key);
        expect(await storeSnapshot(database), before);
        await lock.release();
        // Releasing the contention and querying do not silently seal the queue.
        await f.session.refresh();
        expect(f.session.task!.storage, IoStoragePhase.recoveryRequired);
        expect(await storeSnapshot(database), before);
        await f.session.repair();
        expect(f.session.trusted, isTrue);
        expect(f.session.task!.storage, IoStoragePhase.reclaimed);
        expect(f.session.task!.key, key);
        expect(f.session.task!.exit!.maintenance, historicalExit.maintenance);
        expect(f.session.task!.exit!.execution, historicalExit.execution);
        expect(f.session.task!.exit!.disconnect, historicalExit.disconnect);
        expect(f.session.canAcknowledge, isTrue);
        final after = await storeSnapshot(database);
        expect(after['identity'], before['identity']);
        expect(after['operations'], before['operations']);
        expect(after['pending'], 0);
        final oldSeals = before['sealed'] as List;
        final newSeals = after['sealed'] as List;
        expect(newSeals.length, greaterThan(oldSeals.length));
        expect(newSeals.take(oldSeals.length), oldSeals);
        expect(await f.backend.readUiLocale(), 'zh');
        await expectLater(
          f.post(second: true),
          throwsA(isA<SocketException>()),
        );
        await f.session.acknowledge();
        expect(f.session.task!.storage, IoStoragePhase.local);
        // The explicitly retried previously rejected write is now permitted.
        await f.backend.saveUiLocale('en');
        expect(await f.backend.readUiLocale(), 'en');
        await f.backend.close();
        final reopened = await RustWorkbench.open(
          executable: Platform.environment['MORROW_WORKBENCH_HOST']!,
          package: Platform.environment['MORROW_WORKBENCH_PACKAGE']!,
          directory: f.directory,
          managed: true,
        );
        try {
          expect(await reopened.readUiLocale(), 'en');
          expect((await reopened.ioStatus()).storage, IoStoragePhase.local);
          expect(
            (await storeSnapshot(database))['identity'],
            before['identity'],
          );
        } finally {
          await reopened.close();
        }
      } finally {
        await lock?.release();
        await f.close(observeBeforeClose: false);
      }
    },
    skip: skip,
    timeout: const Timeout(Duration(minutes: 2)),
  );
}
