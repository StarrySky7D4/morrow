import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_i18n/morrow_i18n.dart';
import 'package:morrow_studio/plugins/plugin_library.dart';
import 'package:morrow_studio/plugins/service_control.dart';
import 'package:morrow_studio/plugins/service_manager.dart';
import 'package:morrow_studio/plugins/service_session.dart';

import 'service_manager_fakes.dart';

PluginLibraryEntry servicePlugin() => PluginLibraryEntry(
  id: 'example.package',
  name: 'Service package',
  version: '1',
  digest: serviceKey(3),
  enabled: false,
  builtin: false,
  available: true,
  declared: [],
  approved: [],
  dependencies: [],
  handlers: [],
  issue: '',
  ioHandlers: ['invoke'],
  declaredIo: ['http-listen', 'http-publish'],
  approvedIo: ['http-listen', 'http-publish'],
);
String refHex(List<int> value) =>
    value.map((v) => v.toRadixString(16).padLeft(2, '0')).join();
Widget host(
  ServiceFakeBackend backend, {
  String locale = 'en',
  int revision = 1,
}) => MaterialApp(
  locale: Locale(locale),
  localizationsDelegates: AppLocalizations.localizationsDelegates,
  supportedLocales: AppLocalizations.supportedLocales,
  home: Scaffold(
    body: SingleChildScrollView(
      child: Padding(
        padding: const EdgeInsets.all(12),
        child: ServiceManager(
          backend: backend,
          plugins: [servicePlugin()],
          registryRevision: BigInt.from(revision),
          ink: Colors.black,
          muted: Colors.grey,
          line: Colors.grey,
          radius: BorderRadius.circular(12),
        ),
      ),
    ),
  ),
);
Future<void> click(WidgetTester tester, String key) async {
  final target = find.byKey(ValueKey(key));
  await tester.ensureVisible(target);
  await tester.tap(target);
  await tester.pumpAndSettle();
}

Future<void> text(WidgetTester tester, String key, String value) async {
  final target = find.byKey(ValueKey(key));
  await tester.ensureVisible(target);
  await tester.enterText(target, value);
}

OutlinedButton button(WidgetTester tester, String key) =>
    tester.widget(find.byKey(ValueKey(key)));

void main() {
  testWidgets(
    'pending and failed clipboard copy does not duplicate or retain token',
    (tester) async {
      final backend = ServiceFakeBackend();
      final gate = Completer<void>();
      var copies = 0;
      TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
          .setMockMethodCallHandler(SystemChannels.platform, (call) async {
            if (call.method == 'Clipboard.setData') {
              copies++;
              await gate.future;
            }
            return null;
          });
      addTearDown(
        () => TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
            .setMockMethodCallHandler(SystemChannels.platform, null),
      );
      await tester.pumpWidget(host(backend));
      await tester.pumpAndSettle();
      await click(tester, 'service-new-auth');
      await text(tester, 'service-auth-principal', 'alice');
      await text(tester, 'service-auth-days', '1');
      await click(tester, 'service-auth-save');
      final issued = ServiceSession.forBackend(backend).issued!;
      await click(tester, 'service-token-copy');
      expect(button(tester, 'service-token-copy').onPressed, isNull);
      expect(copies, 1);
      gate.completeError(PlatformException(code: 'denied'));
      await tester.pumpAndSettle();
      expect(issued.token.bytes, everyElement(0));
      expect(find.byKey(const ValueKey('service-token')), findsNothing);
      expect(copies, 1);
    },
  );
  testWidgets(
    'locale and catalog updates keep draft scopes until explicit rebind',
    (tester) async {
      final backend = ServiceFakeBackend()..authorities = [serviceAuthority()];
      await tester.pumpWidget(host(backend));
      await tester.pumpAndSettle();
      await click(tester, 'service-new-config');
      await text(tester, 'service-service', 'draft.service');
      await text(tester, 'service-retention-ms', 'invalid-number');
      await click(tester, 'service-principal-${refHex(serviceKey(2))}');
      await click(tester, 'service-scope-add-alice');
      tester
          .widget<DropdownButtonFormField<int>>(
            find.byKey(const ValueKey('service-scope-kind-alice-0')),
          )
          .onChanged!(4);
      await tester.pumpAndSettle();
      await text(tester, 'service-scope-card-alice-0', 'draft-card');
      await text(tester, 'service-scope-attachment-alice-0', 'draft-file');
      await click(tester, 'service-config-save');
      expect(backend.writes, 0);
      await tester.pumpWidget(host(backend, locale: 'zh', revision: 2));
      await tester.pumpAndSettle();
      expect(
        tester
            .widget<TextField>(
              find.byKey(const ValueKey('service-retention-ms')),
            )
            .controller!
            .text,
        'invalid-number',
      );
      expect(
        tester
            .widget<TextField>(
              find.byKey(const ValueKey('service-scope-attachment-alice-0')),
            )
            .controller!
            .text,
        'draft-file',
      );
      expect(button(tester, 'service-config-save').onPressed, isNull);
      await text(tester, 'service-retention-ms', '1000');
      await click(tester, 'service-config-rebind');
      await click(tester, 'service-config-save');
      expect(backend.saves, hasLength(1));
      final saved = backend.saves.single;
      expect(saved.registryRevision, BigInt.two);
      expect(saved.principals.single.scopes.single.kind, 4);
      expect(saved.principals.single.scopes.single.attachmentId, 'draft-file');
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets(
    'lost authentication receipt survives unmount and requires fresh explicit acknowledgement',
    (tester) async {
      final gate = Completer<IssuedServiceAuthentication>();
      final backend = ServiceFakeBackend()..onIssue = (_) => gate.future;
      await tester.pumpWidget(host(backend));
      await tester.pumpAndSettle();
      await click(tester, 'service-new-auth');
      await text(tester, 'service-auth-principal', 'alice');
      await click(tester, 'service-auth-save');
      expect(backend.issues, hasLength(1));
      await tester.pumpWidget(const SizedBox.shrink());
      gate.completeError(StateError('lost receipt'));
      await tester.pumpAndSettle();
      await tester.pumpWidget(host(backend));
      await tester.pumpAndSettle();
      final session = ServiceSession.forBackend(backend);
      expect(session.uncertain, isTrue);
      expect(session.canWrite, isFalse);
      await click(tester, 'service-new-auth');
      expect(button(tester, 'service-auth-save').onPressed, isNull);
      await click(tester, 'service-refresh');
      expect(session.uncertain, isTrue);
      await click(tester, 'service-acknowledge-uncertain');
      expect(session.canWrite, isTrue);
      expect(backend.issues, hasLength(1));
    },
  );

  testWidgets(
    'switching backend wipes late one-time receipt and never shows it in new workspace',
    (tester) async {
      final gate = Completer<IssuedServiceAuthentication>();
      final old = ServiceFakeBackend()..onIssue = (_) => gate.future;
      final next = ServiceFakeBackend();
      await tester.pumpWidget(host(old));
      await tester.pumpAndSettle();
      await click(tester, 'service-new-auth');
      await text(tester, 'service-auth-principal', 'alice');
      await text(tester, 'service-auth-days', '1');
      await click(tester, 'service-auth-save');
      await tester.pumpWidget(host(next));
      await tester.pumpAndSettle();
      final receipt = serviceIssued();
      gate.complete(receipt);
      await tester.pumpAndSettle();
      expect(receipt.token.isDisposed, isTrue);
      expect(receipt.token.bytes, everyElement(0));
      expect(find.byKey(const ValueKey('service-token')), findsNothing);
      expect(next.writes, 0);
    },
  );

  testWidgets(
    'explicit copy clears owned token and locale change does not reissue',
    (tester) async {
      final backend = ServiceFakeBackend();
      var copies = 0;
      TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
          .setMockMethodCallHandler(SystemChannels.platform, (call) async {
            if (call.method == 'Clipboard.setData') copies++;
            return null;
          });
      addTearDown(
        () => TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
            .setMockMethodCallHandler(SystemChannels.platform, null),
      );
      await tester.pumpWidget(host(backend));
      await tester.pumpAndSettle();
      await click(tester, 'service-new-auth');
      await text(tester, 'service-auth-principal', 'alice');
      await text(tester, 'service-auth-days', '1');
      await click(tester, 'service-auth-save');
      final issued = ServiceSession.forBackend(backend).issued!;
      expect(copies, 0);
      await tester.pumpWidget(host(backend, locale: 'zh'));
      await tester.pumpAndSettle();
      expect(backend.issues, hasLength(1));
      await click(tester, 'service-token-copy');
      expect(copies, 1);
      expect(issued.token.bytes, everyElement(0));
      expect(find.byKey(const ValueKey('service-token')), findsNothing);
    },
  );

  testWidgets(
    'narrow historical metadata and large timestamps render and disabled records stay inspectable',
    (tester) async {
      tester.view.physicalSize = const Size(360, 850);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      final max = BigInt.parse('18446744073700000000');
      final backend = ServiceFakeBackend()
        ..configs = [serviceConfig(disabled: true)]
        ..authorities = [
          serviceAuthority(
            disabled: true,
            createdMs: max,
            expiresMs: max + BigInt.from(1000),
          ),
        ];
      await tester.pumpWidget(host(backend));
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull);
      expect(
        button(tester, 'service-config-disable-service-001').onPressed,
        isNull,
      );
      await click(tester, 'service-config-edit-service-001');
      expect(
        find.byKey(const ValueKey('service-scope-card-alice-0')),
        findsOneWidget,
      );
      expect(
        tester
            .widget<TextField>(
              find.byKey(const ValueKey('service-scope-card-alice-0')),
            )
            .controller!
            .text,
        'card',
      );
      expect(button(tester, 'service-config-save').onPressed, isNull);
      expect(tester.takeException(), isNull);
    },
  );
}
