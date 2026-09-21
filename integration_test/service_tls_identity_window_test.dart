// Actual Windows app and Rust host; deterministic picker results and framework
// input do not certify the OS file dialog or physical keyboard/mouse handling.
import 'dart:convert';
import 'dart:io';
import 'package:file_selector_platform_interface/file_selector_platform_interface.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:morrow_studio/desktop_frame.dart';
import 'package:morrow_studio/main.dart';
import 'package:morrow_studio/plugins/service_run_control.dart';
import 'package:morrow_studio/plugins/service_run_manager.dart';
import 'package:morrow_studio/plugins/service_tls_picker.dart';
import 'package:morrow_studio/plugins/service_tls_identity_session.dart';
import 'package:morrow_studio/plugins/studio_storage.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';
import 'package:morrow_studio/window_effects.dart';
import 'package:window_manager/window_manager.dart';
import '../test/service_run_real_native_fixture.dart';
import 'live_ui_helpers.dart';

class _PemPicker extends FileSelectorPlatform {
  _PemPicker(this.paths);
  final List<String> paths;
  var count = 0;
  @override
  Future<XFile?> openFile({
    List<XTypeGroup>? acceptedTypeGroups,
    String? initialDirectory,
    String? confirmButtonText,
  }) async {
    expect(acceptedTypeGroups!.single.extensions, contains('pem'));
    return XFile(paths[count++]);
  }
}

Finder keyed(String key) => find.byKey(ValueKey(key));

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  testWidgets(
    'Windows saved TLS identity selection rotation disable and original-store recovery',
    (tester) async {
      expect(RealServiceFixture.available, isTrue);
      final output = Directory(
        Platform.environment['MORROW_WINDOW_TEST_OUTPUT']!,
      ).absolute;
      final fixture = await RealServiceFixture.open(tlsRequired: true);
      final originalPicker = FileSelectorPlatform.instance;
      final boundary = GlobalKey();
      try {
        final pem = Directory('${fixture.directory.path}/tls');
        final generated = await Process.run(
          Platform.environment['MORROW_TLS_FIXTURE']!,
          [pem.path],
        );
        expect(generated.exitCode, 0, reason: '${generated.stderr}');
        final cert = '${pem.path}/certificate.pem',
            key = '${pem.path}/private-key.pem';
        final picker = _PemPicker([cert, key]);
        FileSelectorPlatform.instance = picker;
        await fixture.backend.saveUiLocale('en');
        final storage = await RustStudioStorage.open(fixture.backend);
        await initializeDesktopFrame();
        await windowManager.setSize(const Size(800, 820));
        await tester.pumpWidget(
          RepaintBoundary(
            key: boundary,
            child: MorrowApp(
              storage: storage,
              workbench: fixture.backend,
              initialLocale: const Locale('en'),
              nativeBackground: DesktopBackground(),
            ),
          ),
        );
        await tapVisible(tester, keyed('appearance-toggle'));
        await openIoSettings(tester);
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
          reason: 'TLS publication',
        );
        await tapVisible(tester, selection);
        await tapVisible(
          tester,
          find.textContaining('${fixture.plugin.name} ·').last,
        );
        expect(
          tester.widget<OutlinedButton>(keyed('service-run-start')).onPressed,
          isNull,
        );
        await tapVisible(tester, keyed('service-tls-certificate'));
        await tapVisible(tester, keyed('service-tls-private-key'));
        expect(
          tester.widget<OutlinedButton>(keyed('service-run-start')).onPressed,
          isNull,
        );
        await tapVisible(tester, keyed('service-tls-inspect'));
        await waitForUi(
          tester,
          () => keyed('service-tls-fingerprint').evaluate().isNotEmpty,
          reason: 'real PEM inspection',
        );
        expect(fixture.session.service, isNull);
        expect(
          find.textContaining(
            'Shared certificate-chain validity (UTC): 1975-01-01T00:00:00.000Z through 4096-01-01T00:00:00.000Z',
          ),
          findsOneWidget,
        );
        await tester.ensureVisible(find.byType(ServiceTlsPicker));
        await saveBoundaryPng(
          tester,
          boundary,
          '${output.path}/01-checked.png',
        );
        final identities = TlsIdentitySession.forBackend(fixture.backend);
        await tapVisible(tester, keyed('tls-identities-save'));
        await waitForUi(
          tester,
          () =>
              identities.trusted &&
              !identities.busy &&
              identities.records.length == 1,
          reason: 'saved identity receipt',
        );
        final first = identities.records.single.choice;
        await tapVisible(tester, keyed('tls-identities-select-${first.key}'));
        final trust = '${fixture.directory.path}/old-trust.pem';
        await File(cert).copy(trust);
        await File(cert).delete();
        await File(key).delete();
        await tester.ensureVisible(keyed('tls-identities-selection'));
        await saveBoundaryPng(
          tester,
          boundary,
          '${output.path}/02-saved-selection.png',
        );
        await tester.ensureVisible(keyed('service-run-lifetime'));
        await tester.enterText(keyed('service-run-lifetime'), '120000');
        await tapVisible(tester, keyed('service-run-start'));
        await waitForUi(
          tester,
          () => fixture.session.service?.phase == ServiceRunPhase.running,
          reason: 'TLS listener',
        );
        expect(picker.count, 2);
        expect(fixture.session.attempt!.tls, isNull);
        expect(
          fixture.session.attempt!.protectedTls!.reference,
          first.reference,
        );
        final response = await fixture.post(tlsCertificate: trust);
        expect(response, startsWith('HTTP/1.1 202 '));
        expect(response, endsWith('executed-before'));
        await leaveIoSettings(tester);
        await tapVisible(tester, keyed('language-picker'));
        await tapVisible(tester, find.text('简体中文').last);
        await waitForUi(
          tester,
          () => storage.read()['uiLocale'] == 'zh',
          reason: 'Chinese locale persisted',
        );
        await openIoSettings(tester);
        final regenerated = await Process.run(
          Platform.environment['MORROW_TLS_FIXTURE']!,
          [pem.path],
        );
        expect(regenerated.exitCode, 0, reason: '${regenerated.stderr}');
        await tapVisible(tester, keyed('service-tls-inspect'));
        await waitForUi(
          tester,
          () => identities.draft.accepted,
          reason: 'replacement PEM inspection',
        );
        await tapVisible(tester, keyed('tls-identities-replace-${first.key}'));
        await waitForUi(
          tester,
          () =>
              fixture.session.service?.phase == ServiceRunPhase.exited &&
              !identities.busy,
          reason: 'rotation stopped original listener',
        );
        expect(identities.selected!.revision, BigInt.one);
        expect(identities.canUse, isFalse);
        expect(keyed('tls-identities-stale'), findsOneWidget);
        expect(identities.receipt!.choice.revision, BigInt.two);
        expect(identities.uncertain, isFalse);
        await tapVisible(tester, keyed('service-run-acknowledge'));
        await tapVisible(tester, keyed('tls-identities-refresh'));
        await waitForUi(
          tester,
          () =>
              identities.trusted &&
              !identities.busy &&
              identities.records.single.choice.revision == BigInt.two,
          reason: 'refresh identity after original owner returns',
        );
        await tapVisible(tester, keyed('tls-identities-select-${first.key}'));
        expect(identities.selected!.revision, BigInt.two);
        await tapVisible(tester, keyed('service-run-start'));
        await waitForUi(
          tester,
          () => fixture.session.service?.phase == ServiceRunPhase.running,
          reason: 'explicit new identity start',
        );
        expect(fixture.session.attempt!.protectedTls!.revision, BigInt.two);
        await expectLater(
          fixture.post(tlsCertificate: trust),
          throwsA(isA<HandshakeException>()),
        );
        expect(
          await fixture.post(tlsCertificate: cert),
          startsWith('HTTP/1.1 202 '),
        );
        await tester.ensureVisible(keyed('tls-identities-selection'));
        await saveBoundaryPng(
          tester,
          boundary,
          '${output.path}/03-rotated-chinese.png',
        );
        await tapVisible(tester, keyed('tls-identities-disable-${first.key}'));
        await waitForUi(
          tester,
          () => fixture.session.service?.phase == ServiceRunPhase.exited,
          reason: 'actual owner reclaimed',
        );
        await tapVisible(tester, keyed('service-run-acknowledge'));
        await tester.pumpWidget(const SizedBox.shrink());
        await fixture.backend.close();
        final reopened = await RustWorkbench.open(
          executable: Platform.environment['MORROW_WORKBENCH_HOST']!,
          package: Platform.environment['MORROW_WORKBENCH_PACKAGE']!,
          directory: fixture.directory,
          managed: true,
        );
        try {
          expect(await reopened.readUiLocale(), 'zh');
          final row = (await reopened.tlsIdentityPage()).identities.single;
          expect(row.choice.revision, BigInt.from(3));
          expect(row.disabled, isTrue);
        } finally {
          await reopened.close();
        }
        await File('${output.path}/result.json').writeAsString(
          jsonEncode({
            'passed': true,
            'httpsStatus': 202,
            'savedIdentity': true,
            'rotationStoppedService': true,
            'explicitRestart': true,
            'oldCertificateRejected': true,
            'disableStoppedService': true,
            'finalRevision': 3,
            'pickerCalls': picker.count,
            'input': 'Flutter framework',
            'picker': 'deterministic local PEM selection',
            'store': 'original reopened',
          }),
        );
      } finally {
        FileSelectorPlatform.instance = originalPicker;
        await tester.pumpWidget(const SizedBox.shrink());
        await fixture.close(observeBeforeClose: false);
      }
    },
  );
}
