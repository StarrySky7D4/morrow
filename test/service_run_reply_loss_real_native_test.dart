// Fault injection is at the actual stdio boundary after the Rust host replies.
// No synthetic service state or alternate production transport is installed.
import 'dart:convert';
import 'dart:io';

import 'package:crypto/crypto.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/generated/host.capnp.dart' as host;
import 'package:morrow_studio/plugins/host_request.dart';
import 'package:morrow_studio/plugins/io_task_models.dart';
import 'package:morrow_studio/plugins/service_run_codec_native.dart';
import 'package:morrow_studio/plugins/service_run_control.dart';
import 'package:morrow_studio/plugins/service_run_session.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';

import 'service_run_real_native_fixture.dart';

Future<RealServiceFixture> openProxy(String mode) => RealServiceFixture.open(
  openBackend: (dir) async {
    await File(
      'test/fixtures/service_reply_proxy.py',
    ).copy('${dir.path}/workbench.db');
    await File('${dir.path}/proxy.json').writeAsString(
      jsonEncode({
        'host': Platform.environment['MORROW_WORKBENCH_HOST']!,
        'package': Platform.environment['MORROW_WORKBENCH_PACKAGE']!,
        'store': '${dir.path}/managed-store',
        'mode': mode,
      }),
    );
    return RustWorkbench.open(
      executable: Platform.environment['MORROW_CLOSE_TEST_PYTHON']!,
      package: 'proxy-only',
      directory: dir,
    );
  },
);

Future<void> arm(RealServiceFixture f, ServiceRunRequest request) =>
    sendHostRequest(
      host.Action.serviceRunStart,
      configure: (r) =>
          ServiceRunCodec.writeRequest(request, r.initServiceRun()),
      send: (bytes) async {
        await File(
          '${f.directory.path}/armed.sha256',
        ).writeAsString(sha256.convert(bytes).toString());
      },
    );

Future<ServiceRunSnapshot> actualReceipt(RealServiceFixture f) async {
  final bytes = await File('${f.directory.path}/receipt.bin').readAsBytes();
  try {
    final reader = RustWorkbench.readMessage(
      bytes,
    ).getRoot(host.responseFactory);
    expect(reader.error ?? '', isEmpty);
    return ServiceRunCodec.snapshot(
      reader,
      expectedSubmission: f.request().submission,
    );
  } finally {
    bytes.fillRange(0, bytes.length, 0);
  }
}

Future<List<Map<String, dynamic>>> trace(RealServiceFixture f) async =>
    (await File('${f.directory.path}/trace.jsonl').readAsLines())
        .map((line) => jsonDecode(line) as Map<String, dynamic>)
        .toList();

void main() {
  final skip =
      !RealServiceFixture.available ||
      !Platform.environment.containsKey('MORROW_CLOSE_TEST_PYTHON');
  test(
    'damaged real start reply reconciles original live task without resubmit',
    () async {
      final f = await openProxy('malformed');
      try {
        final request = f.request();
        await arm(f, request);
        await f.session.start(request);
        expect(f.session.startUnknown, isTrue);
        expect(f.session.notice, ServiceRunNotice.startUnknown);
        expect(f.session.trusted, isFalse);
        final original = await actualReceipt(f);
        expect(original.task.key, hasLength(32));
        expect(original.submission, request.submission);
        await expectLater(f.backend.readUiLocale(), throwsStateError);
        await f.session.start(request);
        final running = await f.observe(ServiceRunPhase.running);
        expect(running.task.key, original.task.key);
        expect(running.submission, original.submission);
        expect(f.session.startUnknown, isFalse);
        final response = await f.post();
        expect(response, startsWith('HTTP/1.1 202 '));
        expect(response, endsWith('executed-before'));
        await f.backend.saveUiLocale('en');
        expect(await f.backend.readUiLocale(), 'en');
        await f.session.stop();
        final exited = await f.observe(ServiceRunPhase.exited);
        expect(exited.task.key, original.task.key);
        expect(exited.task.storage, IoStoragePhase.reclaimed);
        expect(exited.task.exit, isNotNull);
        await f.session.acknowledge();
        expect(f.session.canStart, isTrue);
        final events = await trace(f);
        expect(events.where((e) => e['event'] == 'matched'), hasLength(1));
        expect(events.where((e) => e['event'] == 'injected'), hasLength(1));
      } finally {
        await f.close();
      }
    },
    skip: skip,
    timeout: const Timeout(Duration(minutes: 2)),
  );

  test(
    'real start reply lost to EOF retains Unknown and waits for native exit',
    () async {
      final f = await openProxy('eof');
      try {
        final request = f.request();
        await arm(f, request);
        await f.session.start(request);
        final original = await actualReceipt(f);
        expect(original.submission, request.submission);
        expect(original.task.key, hasLength(32));
        expect(f.session.startUnknown, isTrue);
        expect(f.session.attempt!.submission, original.submission);
        await f.session.refresh();
        expect(f.session.trusted, isFalse);
        expect(f.session.canStart || f.session.canAbandon, isFalse);
        await f.session.start(request);
        await f.backend.close();
        final events = await trace(f);
        expect(events.where((e) => e['event'] == 'matched'), hasLength(1));
        expect(events.where((e) => e['event'] == 'injected'), hasLength(1));
        expect(
          events.singleWhere((e) => e['event'] == 'child_exit')['code'],
          0,
        );
        await expectLater(f.post(), throwsA(isA<SocketException>()));
        final reopened = await RustWorkbench.open(
          executable: Platform.environment['MORROW_WORKBENCH_HOST']!,
          package: Platform.environment['MORROW_WORKBENCH_PACKAGE']!,
          directory: Directory('${f.directory.path}/managed-store'),
          managed: true,
        );
        try {
          expect(await reopened.readUiLocale(), 'zh');
          final state = await reopened.ioStatus();
          expect(state.storage, IoStoragePhase.local);
          expect(state.key, isNull);
          await reopened.saveUiLocale('en');
          expect(await reopened.readUiLocale(), 'en');
          expect(f.session.startUnknown, isTrue);
        } finally {
          await reopened.close();
        }
      } finally {
        await f.close();
      }
    },
    skip: skip,
    timeout: const Timeout(Duration(minutes: 2)),
  );
}
