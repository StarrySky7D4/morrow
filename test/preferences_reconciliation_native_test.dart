import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';
import 'package:capnproto_dart/capnproto_dart.dart';
import 'package:crypto/crypto.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/generated/host.capnp.dart' as host;
import 'package:morrow_studio/plugins/generated/identity.dart' as contract;
import 'package:morrow_studio/plugins/host_request.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';
import 'package:morrow_studio/plugins/studio_storage.dart';
import 'package:morrow_studio/plugins/preferences_save_failure.dart';

void main() {
  final executable = Platform.environment['MORROW_WORKBENCH_HOST'],
      package = Platform.environment['MORROW_WORKBENCH_PACKAGE'],
      python = Platform.environment['MORROW_CLOSE_TEST_PYTHON'];
  for (final (throughStorage, restart, lostReceipt) in [
    (false, false, false),
    (true, false, false),
    (false, true, false),
    (true, true, false),
    (false, true, true),
    (true, true, true),
  ]) {
    test(
      'committed S1 reconciles before S2 (storage: $throughStorage, restart: $restart, receipt lost: $lostReceipt)',
      () async {
        final directory = await Directory.systemTemp.createTemp(
          'morrow-prefs-reconcile-',
        );
        RustWorkbench? backend;
        try {
          await File(
            'test/fixtures/service_reply_proxy.py',
          ).copy('${directory.path}/workbench.db');
          await File('${directory.path}/proxy.json').writeAsString(
            jsonEncode({
              'host': executable,
              'package': package,
              'store': '${directory.path}/store',
              'mode': 'replace',
              'trace_requests': true,
            }),
          );
          final failure = MessageBuilder();
          final reply = failure.initRoot(host.responseFactory);
          reply.version = 1;
          reply.digest = Uint8List.fromList(contract.hostDigest);
          reply.error = 'injected post-commit read failure';
          await File(
            '${directory.path}/replacement.bin',
          ).writeAsBytes(failure.serialize());
          backend = await RustWorkbench.open(
            executable: python!,
            package: 'proxy',
            directory: directory,
          );
          await backend.saveUiLocale('fr');
          var storage = throughStorage
              ? await RustStudioStorage.open(backend)
              : null;
          final baseline =
              storage?.read() ??
              <String, dynamic>{'ideas': [], 'uiLocale': 'fr'};
          Future<Uint8List> save(String theme) async {
            final data = {...baseline, 'theme': theme};
            if (storage != null) {
              await storage.write(data);
              return encodePreferences(storage.read());
            }
            return backend!.savePreferences(encodePreferences(data));
          }

          await sendHostRequest(
            lostReceipt
                ? host.Action.finishPreferences
                : host.Action.readPreferences,
            // Fresh fixture: no earlier upload/download issued a transfer token.
            configure: lostReceipt ? (r) => r.transfer = 'upload-1' : null,
            send: (bytes) async {
              await File(
                '${directory.path}/armed.sha256',
              ).writeAsString(sha256.convert(bytes).toString());
            },
          );
          final first = save('dark');
          final second = save('white'); // Equal to the old confirmed snapshot.
          await Future.wait([
            expectLater(
              first,
              throwsA(
                isA<PreferencesSaveFailure>().having(
                  (e) => e.effect,
                  'effect',
                  lostReceipt
                      ? PreferencesEffect.unknown
                      : PreferencesEffect.locallyCommitted,
                ),
              ),
            ),
            expectLater(second, throwsStateError),
          ]);
          final original = backend.pendingPreferencesOperation;
          expect(original, isNotNull);
          if (restart) {
            await backend.close();
            await File('${directory.path}/armed.sha256').delete();
            backend = await RustWorkbench.open(
              executable: python,
              package: 'proxy',
              directory: directory,
            );
            // Reopening only restores the proposal and reads the current snapshot.
            // It must not issue a second begin/finish submission.
            await backend.readPreferences();
            expect(backend.pendingPreferencesOperation, original);
            storage = throughStorage
                ? await RustStudioStorage.open(backend)
                : null;
          }
          // Language committed independently; the local newer proposal was not sent.
          expect(await backend.readUiLocale(), 'fr');
          expect(
            decodePreferences((await backend.readPreferences())!)['theme'],
            'dark',
          );
          if (restart && throughStorage) {
            final observed = await storage!.inspectSaveRecovery();
            expect(observed!.operation, original);
            expect(observed.committed, isTrue);
            await storage.resolveSaveRecovery(observed, abandon: false);
            expect(backend.pendingPreferencesOperation, isNull);
            expect(storage.read()['theme'], 'dark');
            // The caller's queued/newer white draft remains separate.
            expect(baseline['theme'] ?? 'white', 'white');
          }
          final result = await save('white'); // explicit retry
          expect(decodePreferences(result)['theme'], 'white');
          expect(backend.pendingPreferencesOperation, isNull);
          final requests =
              (await File('${directory.path}/trace.jsonl').readAsLines())
                  .map((line) => jsonDecode(line) as Map)
                  .where((e) => e['event'] == 'request')
                  .map((e) {
                    final hex = e['hex'] as String;
                    final bytes = Uint8List.fromList([
                      for (var i = 0; i < hex.length; i += 2)
                        int.parse(hex.substring(i, i + 2), radix: 16),
                    ]);
                    return RustWorkbench.readMessage(
                      bytes,
                    ).getRoot(host.requestFactory);
                  })
                  .where((r) => r.action == host.Action.beginPreferences)
                  .map((r) => r.operation)
                  .toList();
          expect(requests.length, 2);
          expect(requests[0], original);
          expect(requests[1], isNot(original));
        } finally {
          try {
            await backend?.close();
          } catch (_) {}
          // This is the unique temporary directory created by this test.
          await directory.delete(recursive: true);
        }
      },
      skip:
          executable == null ||
          package == null ||
          python == null ||
          !Platform.isWindows,
    );
  }
}
