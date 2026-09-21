// Actual high-level business write, with its consumed owner-command reply lost
// at the real stdio boundary. The underlying commit is never mocked.
import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/generated/host.capnp.dart' as host;
import 'package:morrow_studio/plugins/generated/identity.dart' as contract;
import 'package:morrow_studio/plugins/host_request.dart';
import 'package:morrow_studio/plugins/service_run_codec_native.dart';
import 'package:morrow_studio/plugins/service_run_control.dart';
import 'package:morrow_studio/plugins/service_tls_identity_codec_native.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';

import 'service_run_real_native_fixture.dart';
import 'service_run_reply_loss_real_native_test.dart' show openProxy, trace;

Uint8List identity(int byte) => Uint8List.fromList(List.filled(32, byte));
String hex(List<int> bytes) =>
    bytes.map((b) => b.toRadixString(16).padLeft(2, '0')).join();

Future<Uint8List> frame(
  host.Action action, {
  void Function(host.RequestBuilder)? configure,
}) async {
  late Uint8List result;
  await sendHostRequest(
    action,
    configure: configure,
    send: (bytes) async {
      result = Uint8List.fromList(bytes);
    },
  );
  return result;
}

(int, String) localeReceipt(Uint8List bytes) {
  final reply = RustWorkbench.readMessage(bytes).getRoot(host.responseFactory);
  expect(reply.version, 1);
  expect(reply.digest, contract.hostDigest);
  expect(reply.error ?? '', isEmpty);
  return (reply.revision, utf8.decode(reply.payload!));
}

Future<(int, String)> queryLocale(
  RealServiceFixture f,
  Uint8List task,
  int id,
) async {
  final input = await frame(host.Action.readUiLocale);
  try {
    var command = await f.backend.submitServiceCommand(
      task,
      identity(id),
      input,
    );
    final elapsed = Stopwatch()..start();
    while (command.delivery == OwnerCommandDelivery.pending) {
      if (elapsed.elapsed > const Duration(seconds: 10)) {
        fail('owner read timed out');
      }
      await Future<void>.delayed(const Duration(milliseconds: 15));
      command = await f.backend.serviceCommandStatus(task, command.key);
    }
    expect(command.delivery, OwnerCommandDelivery.ready);
    final read = await f.backend.readServiceCommand(task, command.key);
    try {
      expect(read.snapshot.terminal, OwnerCommandTerminal.none);
      return localeReceipt(read.payload!);
    } finally {
      read.dispose();
    }
  } finally {
    input.fillRange(0, input.length, 0);
  }
}

Future<void> armCommandRead(RealServiceFixture f, Uint8List task) async {
  Future<Uint8List> template(int byte) => frame(
    host.Action.commandRead,
    configure: (r) {
      r.ioKey = task;
      r.commandKey = identity(byte);
    },
  );
  final a = await template(0), b = await template(255);
  expect(a.length, b.length);
  final mask = [for (var i = 0; i < a.length; i++) a[i] == b[i] ? 255 : 0];
  // Only the unpredictable command key is ignored. The action, framing,
  // contract and actual service task identity are still compared exactly.
  expect(mask.where((value) => value == 0), hasLength(32));
  await File(
    '${f.directory.path}/armed.mask.json',
  ).writeAsString(jsonEncode({'template': hex(a), 'mask': hex(mask)}));
}

Future<(int, String)> readReopenedStore(RealServiceFixture f) async {
  final process =
      await Process.start(Platform.environment['MORROW_WORKBENCH_HOST']!, [
        '--managed',
        '${f.directory.path}/managed-store',
        Platform.environment['MORROW_WORKBENCH_PACKAGE']!,
      ]);
  final output = process.stdout.fold<List<int>>(
    [],
    (all, bytes) => all..addAll(bytes),
  );
  final errors = process.stderr.transform(utf8.decoder).join();
  final request = await frame(host.Action.readUiLocale);
  process.stdin.add(
    (ByteData(
      4,
    )..setUint32(0, request.length, Endian.little)).buffer.asUint8List(),
  );
  process.stdin.add(request);
  await process.stdin.close();
  final code = await process.exitCode;
  final bytes = Uint8List.fromList(await output);
  expect(code, 0, reason: await errors);
  expect(bytes.length, greaterThan(4));
  final size = ByteData.sublistView(bytes).getUint32(0, Endian.little);
  expect(bytes.length, size + 4);
  return localeReceipt(Uint8List.sublistView(bytes, 4));
}

void main() {
  final skip =
      !RealServiceFixture.available ||
      !Platform.environment.containsKey('MORROW_CLOSE_TEST_PYTHON');
  for (final mode in ['malformed', 'eof']) {
    test(
      'protected TLS save retains Unknown after $mode reply loss without duplicate identity',
      () async {
        final f = await openProxy(mode);
        try {
          final pem = Directory('${f.directory.path}/tls');
          final generated = await Process.run(
            Platform.environment['MORROW_TLS_FIXTURE']!,
            [pem.path],
          );
          expect(generated.exitCode, 0, reason: '${generated.stderr}');
          final selection = await f.backend.inspectServiceTls(
            certificatePath: '${pem.path}/certificate.pem',
            privateKeyPath: '${pem.path}/private-key.pem',
          );
          await f.session.start(f.request());
          final task = (await f.observe(ServiceRunPhase.running)).task.key!;
          await armCommandRead(f, task);
          ServiceCommandFailure? lost;
          try {
            await f.backend.saveTlsIdentity(
              selection: selection,
              reference: Uint8List(0),
              expectedRevision: BigInt.zero,
            );
          } on ServiceCommandFailure catch (error) {
            lost = error;
          }
          expect(lost, isNotNull);
          expect(lost!.outcomeUnknown, isTrue);
          expect(lost.task, task);
          expect(lost.command, hasLength(32));
          await File('${f.directory.path}/armed.mask.json').delete();
          final original = await File(
            '${f.directory.path}/receipt.bin',
          ).readAsBytes();
          final read = ServiceRunCodec.read(
            RustWorkbench.readMessage(original).getRoot(host.responseFactory),
            expectedKey: lost.command!,
          );
          late String savedKey;
          try {
            expect(read.snapshot.submission, lost.submission);
            expect(read.snapshot.terminal, OwnerCommandTerminal.none);
            final saved = ServiceTlsIdentityCodec.saved(
              RustWorkbench.readMessage(
                read.payload!,
              ).getRoot(host.responseFactory),
              reference: Uint8List(0),
              revision: BigInt.one,
              certificateSha256: selection.certificateSha256,
              disabled: false,
            );
            savedKey = saved.choice.key;
          } finally {
            read.dispose();
            original.fillRange(0, original.length, 0);
          }
          if (mode == 'malformed') {
            final state = await f.backend.serviceCommandBySubmission(
              task,
              lost.submission,
            );
            expect(state.key, lost.command);
            expect(state.delivery, OwnerCommandDelivery.consumed);
            expect(
              (await f.backend.tlsIdentityPage()).identities.single.choice.key,
              savedKey,
            );
            await f.session.stop();
            await f.observe(ServiceRunPhase.exited);
            await f.session.acknowledge();
          }
          await f.backend.close();
          final events = await trace(f);
          expect(events.where((e) => e['event'] == 'matched'), hasLength(1));
          final reopened = await RustWorkbench.open(
            executable: Platform.environment['MORROW_WORKBENCH_HOST']!,
            package: Platform.environment['MORROW_WORKBENCH_PACKAGE']!,
            directory: Directory('${f.directory.path}/managed-store'),
            managed: true,
          );
          try {
            final row = (await reopened.tlsIdentityPage()).identities.single;
            expect(row.choice.key, savedKey);
            expect(row.choice.revision, BigInt.one);
            expect(row.disabled, isFalse);
          } finally {
            await reopened.close();
          }
        } finally {
          await f.close(observeBeforeClose: false);
        }
      },
      skip: skip || !Platform.environment.containsKey('MORROW_TLS_FIXTURE'),
      timeout: const Timeout(Duration(minutes: 2)),
    );
    test(
      'committed ordinary business write survives $mode reply loss without duplicate commit',
      () async {
        final f = await openProxy(mode);
        try {
          await f.session.start(f.request());
          final live = await f.observe(ServiceRunPhase.running);
          final task = live.task.key!;
          final before = await queryLocale(f, task, 101);
          expect(before.$2, 'zh');
          await armCommandRead(f, task);
          ServiceCommandFailure? failure;
          try {
            await f.backend.saveUiLocale('en');
          } on ServiceCommandFailure catch (error) {
            failure = error;
          }
          expect(failure, isNotNull);
          final lost = failure!;
          expect(lost.outcomeUnknown, isTrue);
          expect(lost.task, task);
          expect(lost.submission, hasLength(32));
          expect(lost.command, hasLength(32));
          await File('${f.directory.path}/armed.mask.json').delete();
          final original = await File(
            '${f.directory.path}/receipt.bin',
          ).readAsBytes();
          final reply = RustWorkbench.readMessage(
            original,
          ).getRoot(host.responseFactory);
          expect(reply.version, 1);
          expect(reply.digest, contract.hostDigest);
          expect(reply.error ?? '', isEmpty);
          final read = ServiceRunCodec.read(reply, expectedKey: lost.command!);
          try {
            expect(read.snapshot.submission, lost.submission);
            expect(read.snapshot.started, isTrue);
            expect(read.snapshot.delivery, OwnerCommandDelivery.consumed);
            expect(read.snapshot.terminal, OwnerCommandTerminal.none);
            expect(localeReceipt(read.payload!), (before.$1 + 1, 'en'));
          } finally {
            read.dispose();
            original.fillRange(0, original.length, 0);
          }
          if (mode == 'malformed') {
            final status = await f.backend.serviceCommandBySubmission(
              task,
              lost.submission,
            );
            expect(status.key, lost.command);
            expect(status.delivery, OwnerCommandDelivery.consumed);
            final reread = await f.backend.readServiceCommand(
              task,
              lost.command!,
            );
            try {
              expect(reread.snapshot.delivery, OwnerCommandDelivery.consumed);
              expect(reread.payload, isNull);
            } finally {
              reread.dispose();
            }
            expect(await queryLocale(f, task, 102), (before.$1 + 1, 'en'));
            // A later explicit user save confirms the original pending operation.
            // It may send a new command envelope but cannot create a second write.
            await f.backend.saveUiLocale('en');
            expect(await queryLocale(f, task, 103), (before.$1 + 1, 'en'));
            await f.session.stop();
            await f.observe(ServiceRunPhase.exited);
            await f.session.acknowledge();
          }
          await f.backend.close();
          final events = await trace(f);
          expect(events.where((e) => e['event'] == 'matched'), hasLength(1));
          expect(events.where((e) => e['event'] == 'injected'), hasLength(1));
          expect(
            events.singleWhere((e) => e['event'] == 'child_exit')['code'],
            0,
          );
          // A new native process reads the SAME managed store after actual exit.
          // No new save or recovery replay is issued: the revision remains +1.
          expect(await readReopenedStore(f), (before.$1 + 1, 'en'));
        } finally {
          // EOF leaves the old observation stale; do not wait for it to refresh
          // through a dead pipe. close() below still waits for real native exit.
          await f.close(observeBeforeClose: false);
        }
      },
      skip: skip,
      timeout: const Timeout(Duration(minutes: 2)),
    );
  }
}
