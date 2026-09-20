import 'dart:io';
import 'dart:typed_data';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_i18n/morrow_i18n.dart';
import 'package:morrow_studio/plugins/credential_manager.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';
import 'external_plugin_native_test.dart' show removeTestDirectory;
import 'plugin_tools_native_test.dart' show pumpHost;

void main() {
  final executable = Platform.environment['MORROW_WORKBENCH_HOST'];
  final package = Platform.environment['MORROW_WORKBENCH_PACKAGE'];
  final unavailable =
      !Platform.isWindows || executable == null || package == null;
  test(
    'native credentials persist redacted metadata, replace with CAS and disable across process restarts',
    () async {
      final directory = await Directory.systemTemp.createTemp(
        'morrow-external-credentials-',
      );
      RustWorkbench? backend;
      Future<RustWorkbench> open() => RustWorkbench.open(
        executable: executable!,
        package: package!,
        directory: directory,
        managed: true,
      );
      try {
        backend = await open();
        final saved = await backend.saveCredential(
          reference: Uint8List(0),
          expectedRevision: BigInt.zero,
          headerName: 'authorization',
          headerValue: 'Bearer native-test-secret',
          lifetimeDays: 7,
        );
        expect(saved.reference, hasLength(32));
        expect(saved.revision, BigInt.one);
        await expectLater(
          backend.saveCredential(
            reference: saved.reference,
            expectedRevision: (BigInt.one << 64) + BigInt.one,
            headerName: 'x-api-key',
            headerValue: 'must-not-wrap',
            lifetimeDays: 7,
          ),
          throwsA(isA<FormatException>()),
        );
        await expectLater(
          backend.saveCredential(
            reference: saved.reference,
            expectedRevision: BigInt.one,
            headerName: 'x-api-key',
            headerValue: 'must-not-wrap',
            lifetimeDays: 0x100000001,
          ),
          throwsA(isA<FormatException>()),
        );
        await backend.close();
        backend = await open();
        var page = await backend.credentialPage();
        expect(page.entries.single.reference, saved.reference);
        expect(page.entries.single.disabled, isFalse);
        final updated = await backend.saveCredential(
          reference: saved.reference,
          expectedRevision: BigInt.one,
          headerName: 'x-api-key',
          headerValue: 'new-native-test-secret',
          lifetimeDays: 1,
        );
        expect(updated.revision, BigInt.two);
        await expectLater(
          backend.disableCredential(saved),
          throwsA(isA<StateError>()),
        );
        await backend.disableCredential(updated);
        await backend.close();
        backend = await open();
        page = await backend.credentialPage();
        expect(page.entries.single.disabled, isTrue);
        expect(page.entries.single.revision, BigInt.from(3));
      } finally {
        await backend?.close();
        await removeTestDirectory(directory);
      }
    },
    skip: unavailable,
    timeout: const Timeout(Duration(minutes: 3)),
  );

  testWidgets(
    'real credential form saves protected input and disables the same record',
    (tester) async {
      RustWorkbench? backend;
      Directory? directory;
      try {
        await tester.runAsync(() async {
          directory = await Directory.systemTemp.createTemp(
            'morrow-external-credential-form-',
          );
          backend = await RustWorkbench.open(
            executable: executable!,
            package: package!,
            directory: directory!,
            managed: true,
          );
        });
        await tester.pumpWidget(
          MaterialApp(
            locale: const Locale('en'),
            localizationsDelegates: AppLocalizations.localizationsDelegates,
            supportedLocales: AppLocalizations.supportedLocales,
            home: Scaffold(
              body: SingleChildScrollView(
                child: CredentialManager(
                  backend: backend!,
                  ink: Colors.black,
                  muted: Colors.grey,
                  line: Colors.grey,
                  radius: BorderRadius.circular(12),
                ),
              ),
            ),
          ),
        );
        final create = find.byKey(const ValueKey('credential-new'));
        await pumpHost(
          tester,
          () => tester.widget<OutlinedButton>(create).onPressed != null,
        );
        await tester.tap(create);
        await tester.pumpAndSettle();
        final secret = find.byKey(const ValueKey('credential-secret'));
        expect(tester.widget<TextField>(secret).obscureText, isTrue);
        await tester.enterText(secret, 'Bearer form-only-test-secret');
        final save = find.byKey(const ValueKey('credential-save'));
        await tester.ensureVisible(save);
        await tester.tap(save);
        await tester.pump();
        if (secret.evaluate().isNotEmpty) {
          expect(tester.widget<TextField>(secret).controller!.text, isEmpty);
        }
        await pumpHost(tester, () => secret.evaluate().isEmpty);
        CredentialPage? page;
        await tester.runAsync(
          () async => page = await backend!.credentialPage(),
        );
        expect(page!.entries.single.disabled, isFalse);
        final reference = page!.entries.single.reference
            .map((b) => b.toRadixString(16).padLeft(2, '0'))
            .join();
        final disable = find.byKey(ValueKey('credential-disable-$reference'));
        await tester.ensureVisible(disable);
        await tester.tap(disable);
        await tester.pump();
        await pumpHost(
          tester,
          () => tester.widget<OutlinedButton>(create).onPressed != null,
        );
        await tester.runAsync(
          () async => page = await backend!.credentialPage(),
        );
        expect(page!.entries.single.disabled, isTrue);
        expect(find.textContaining('form-only-test-secret'), findsNothing);
      } finally {
        await tester.pumpWidget(const SizedBox.shrink());
        await tester.runAsync(() async {
          await backend?.close();
          if (directory != null) await removeTestDirectory(directory!);
        });
      }
    },
    skip: unavailable,
    timeout: const Timeout(Duration(minutes: 3)),
  );
}
