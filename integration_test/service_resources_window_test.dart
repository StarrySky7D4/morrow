// Windows app + original Rust owner + actual Rust/Wasm resource discovery.
// Framework input and Flutter render capture do not certify OS input capture.
import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:morrow_studio/desktop_frame.dart';
import 'package:morrow_studio/main.dart';
import 'package:morrow_studio/plugins/endpoint_control.dart';
import 'package:morrow_studio/plugins/io_task_models.dart';
import 'package:morrow_studio/plugins/service_run_control.dart';
import 'package:morrow_studio/plugins/service_run_manager.dart';
import 'package:morrow_studio/plugins/studio_storage.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';
import 'package:morrow_studio/window_effects.dart';
import 'package:window_manager/window_manager.dart';

import '../test/service_run_real_native_fixture.dart';
import 'live_ui_helpers.dart';

Finder keyed(String key) => find.byKey(ValueKey(key));
String hex(List<int> value) =>
    value.map((b) => b.toRadixString(16).padLeft(2, '0')).join();

Future<(int, List<int>)> invoke(
  RealServiceFixture fixture,
  Uint8List input,
  String key,
) async {
  final client = HttpClient()..connectionTimeout = const Duration(seconds: 3);
  try {
    final request = await client.postUrl(
      Uri.parse('http://${fixture.address}/api'),
    );
    request.headers.set(
      'authorization',
      'Bearer ${utf8.decode(fixture.issued!.token.bytes)}',
    );
    request.headers.set('idempotency-key', key);
    request.headers.set('morrow-service-resources-v1', 'forged');
    request.contentLength = input.length;
    request.add(input);
    final response = await request.close().timeout(const Duration(seconds: 35));
    final bytes = await response.fold<List<int>>(
      [],
      (all, part) => all..addAll(part),
    );
    return (response.statusCode, bytes);
  } on SocketException {
    // Stopping the listener may close an in-flight response before headers.
    return (0, const <int>[]);
  } on HttpException {
    return (0, const <int>[]);
  } finally {
    client.close(force: true);
  }
}

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  for (final action in ['revoke', 'stop']) {
    testWidgets(
      'resource discovery and $action during real outbound HTTP',
      (tester) async {
        expect(RealServiceFixture.available, isTrue);
        final package =
            Platform.environment['MORROW_SERVICE_RESOURCES_PACKAGE'];
        final inputPath =
            Platform.environment['MORROW_SERVICE_RESOURCES_INPUT'];
        final outputPath = Platform.environment['MORROW_WINDOW_TEST_OUTPUT'];
        expect(package, isNotNull);
        expect(inputPath, isNotNull);
        expect(outputPath, isNotNull);
        final output = Directory('$outputPath/$action').absolute;
        final input = await File(inputPath!).readAsBytes();
        final waitingInput = await File('$inputPath.wait').readAsBytes();
        final server = await HttpServer.bind(InternetAddress.loopbackIPv4, 0);
        final release = Completer<void>();
        var calls = 0;
        final received = <(String, String)>[];
        final handlers = <Future<void>>[];
        final subscription = server.listen((request) {
          handlers.add(() async {
            received.add((request.method, request.uri.path));
            calls++;
            if (calls == 2) await release.future;
            try {
              request.response.write('resource-window-result');
              await request.response.close();
            } on SocketException {
              /* Cancellation may close the real transport. */
            }
          }());
        });
        final fixture = await RealServiceFixture.open(resourcePackage: package);
        final root = GlobalKey();
        try {
          await fixture.backend.saveUiLocale('en');
          final endpoint = await fixture.backend.saveEndpoint(
            reference: Uint8List(0),
            expectedRevision: BigInt.zero,
            registryRevision: fixture.registry,
            lifetimeDays: 1,
            policy: EndpointPolicy(
              packageId: fixture.plugin.id,
              packageDigest: fixture.plugin.digest,
              origin: 'http://127.0.0.1:${server.port}',
              profile: 2,
              methods: ['GET'],
              credentialReference: Uint8List(0),
              rootCertificate: Uint8List(0),
              maxRequestBytes: 65536,
              maxResponseBytes: 65536,
              maxHeaderBytes: 16384,
              maxConcurrent: 2,
              timeoutMs: 30000,
              maxFrameBytes: 131072,
            ),
          );
          final storage = await RustStudioStorage.open(fixture.backend);
          await initializeDesktopFrame();
          await windowManager.setSize(const Size(800, 820));
          await tester.pumpWidget(
            RepaintBoundary(
              key: root,
              child: MorrowApp(
                storage: storage,
                workbench: fixture.backend,
                initialLocale: const Locale('en'),
                nativeBackground: DesktopBackground(),
              ),
            ),
          );
          await tapVisible(tester, keyed('appearance-toggle'));
          await waitForUi(
            tester,
            () => find.byType(ServiceRunManager).evaluate().length == 1,
            reason: 'service panel',
          );
          final selection = find.descendant(
            of: find.byType(ServiceRunManager),
            matching: find.byType(DropdownButtonFormField<String>),
          );
          await waitForUi(
            tester,
            () =>
                tester
                    .widget<DropdownButton<String>>(
                      find.descendant(
                        of: selection,
                        matching: find.byType(DropdownButton<String>),
                      ),
                    )
                    .items
                    ?.length ==
                1,
            reason: 'approved service metadata',
          );
          await tapVisible(tester, selection);
          await tapVisible(
            tester,
            find.textContaining('${fixture.plugin.name} ·').last,
          );
          final choice = keyed(
            'service-run-endpoint-${hex(endpoint.reference)}',
          );
          await tapVisible(tester, choice);
          expect(tester.widget<CheckboxListTile>(choice).value, isTrue);
          await tester.ensureVisible(keyed('service-run-lifetime'));
          await tester.enterText(keyed('service-run-lifetime'), '120000');
          await tapVisible(tester, keyed('service-run-start'));
          await waitForUi(
            tester,
            () => fixture.session.service?.phase == ServiceRunPhase.running,
            reason: 'real listener',
          );
          final identity = fixture.session.task!.key!.toList();
          expect(
            fixture.session.attempt!.outbound.single.reference,
            endpoint.reference,
          );
          final first = await invoke(fixture, input, 'window-first');
          expect(first.$1, 202);
          expect(
            utf8.decode(first.$2, allowMalformed: true),
            contains('resource-window-result'),
          );
          expect((await invoke(fixture, input, 'window-first')).$1, 202);
          expect(calls, 1, reason: 'same service request must not resend');
          await tester.ensureVisible(keyed('service-run-identity'));
          await saveBoundaryPng(
            tester,
            root,
            '${output.path}/01-discovered.png',
          );
          // Mutate an unselected endpoint on the same original owner while
          // the selected service and its cached response remain live.
          final unrelated = await fixture.backend.saveEndpoint(
            reference: Uint8List(0),
            expectedRevision: BigInt.zero,
            registryRevision: fixture.registry,
            lifetimeDays: 1,
            policy: endpoint.policy,
          );
          await fixture.backend.disableEndpoint(unrelated);
          expect(fixture.session.task!.key, identity);
          expect(fixture.session.service?.phase, ServiceRunPhase.running);
          expect((await invoke(fixture, input, 'window-first')).$1, 202);
          expect(
            calls,
            1,
            reason: 'unrelated writes preserve the cached reply',
          );
          var settled = false;
          final waiting = invoke(fixture, waitingInput, 'window-wait').then((
            value,
          ) {
            settled = true;
            return value;
          });
          await waitForUi(
            tester,
            () => calls == 2,
            reason: 'real outbound server waiting',
          );
          expect(settled, isFalse);
          expect(received, [('GET', '/value'), ('GET', '/value')]);
          if (action == 'revoke') {
            debugPrint('WINDOW revoke: disable UI');
            await tapVisible(
              tester,
              keyed('endpoint-disable-${hex(endpoint.reference)}'),
            );
            await waitForUi(
              tester,
              () => fixture.session.canAcknowledge,
              // The immutable service scope depends on this selected endpoint.
              // Observe actual reclaim before any local read.
              reason: 'approval mutation automatically retires the service',
            );
            expect(fixture.session.task!.key, identity);
            final disabled = (await fixture.backend.endpointPage()).entries
                .singleWhere(
                  (entry) => hex(entry.reference) == hex(endpoint.reference),
                );
            expect(disabled.reference, endpoint.reference);
            expect(disabled.disabled, isTrue);
            expect(disabled.revision, endpoint.revision + BigInt.one);
            debugPrint('WINDOW revoke: stored revision confirmed');
            final denied = await waiting;
            expect(denied.$1, isNot(202));
            expect(
              utf8.decode(denied.$2, allowMalformed: true),
              isNot(contains('resource-window-result')),
            );
            expect(
              (await invoke(fixture, waitingInput, 'window-wait')).$1,
              isNot(202),
            );
            expect(calls, 2);
          }
          if (action == 'stop') {
            await tapVisible(tester, keyed('service-run-stop'));
            debugPrint('WINDOW stop: stop requested');
          }
          await waitForUi(
            tester,
            () => fixture.session.canAcknowledge,
            reason: 'original owner and listener reclaimed',
          );
          final denied = await waiting;
          expect(denied.$1, isNot(202));
          expect(calls, 2);
          expect(fixture.session.task!.key, identity);
          expect(fixture.session.task!.storage, IoStoragePhase.reclaimed);
          expect(fixture.session.task!.exit!.maintenance, IoJobError.none);
          expect(
            release.isCompleted,
            isFalse,
            reason:
                'stop must cancel/join transport without waiting for remote reply',
          );
          release.complete();
          await tester.ensureVisible(keyed('service-run-exit'));
          await saveBoundaryPng(
            tester,
            root,
            '${output.path}/02-reclaimed.png',
          );
          await tapVisible(tester, keyed('service-run-acknowledge'));
          await waitForUi(
            tester,
            () => fixture.session.task?.storage == IoStoragePhase.local,
            reason: 'explicit acknowledgement',
          );
          await tester.pumpWidget(const SizedBox());
          await fixture.backend.close();
          debugPrint('WINDOW $action: original backend closed');
          final reopened = await RustWorkbench.open(
            executable: Platform.environment['MORROW_WORKBENCH_HOST']!,
            package: Platform.environment['MORROW_WORKBENCH_PACKAGE']!,
            directory: fixture.directory,
            managed: true,
          );
          try {
            final stored = (await reopened.endpointPage()).entries.singleWhere(
              (entry) => hex(entry.reference) == hex(endpoint.reference),
            );
            expect(stored.reference, endpoint.reference);
            expect(stored.disabled, action == 'revoke');
            expect(await reopened.readUiLocale(), 'en');
          } finally {
            await reopened.close();
          }
          await output.create(recursive: true);
          await File('${output.path}/result.json').writeAsString(
            jsonEncode({
              'passed': true,
              'action': action,
              'requests': calls,
              'unrelatedMutationPreservedService': true,
              'input': 'Windows Flutter framework',
              'guest':
                  'actual Rust/Wasm core codec; not public SDK qualification',
              'store': 'original reopened',
            }),
          );
        } finally {
          if (!release.isCompleted) release.complete();
          await tester.pumpWidget(const SizedBox());
          await fixture.close(observeBeforeClose: false);
          await server.close(force: true);
          await subscription.cancel();
          await Future.wait(handlers);
        }
      },
      timeout: const Timeout(Duration(minutes: 3)),
    );
  }
}
