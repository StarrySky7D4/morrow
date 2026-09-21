import 'dart:async';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_i18n/morrow_i18n.dart';
import 'package:morrow_studio/plugins/service_run_control.dart';
import 'package:morrow_studio/plugins/service_tls_identity_panel.dart';
import 'package:morrow_studio/plugins/service_tls_identity_session.dart';
import 'tls_identity_fakes.dart';

Finder keyed(String key) => find.byKey(ValueKey(key));
Future<void> tap(WidgetTester tester, String key) async {
  await tester.ensureVisible(keyed(key));
  await tester.tap(keyed(key));
  await tester.pumpAndSettle();
}

Widget app(
  Backend b, {
  String locale = 'en',
  Future<String?> Function()? choose,
  void Function(ServiceTlsSelection?, ServiceTlsIdentityChoice?)? changed,
}) => MaterialApp(
  locale: Locale(locale),
  localizationsDelegates: AppLocalizations.localizationsDelegates,
  supportedLocales: AppLocalizations.supportedLocales,
  home: Scaffold(
    body: SingleChildScrollView(
      child: ServiceTlsIdentityPanel(
        backend: b,
        inspector: b,
        selectionEnabled: true,
        onChanged: changed ?? (_, _) {},
        chooseFile: choose,
        ink: Colors.black,
        muted: Colors.grey,
        line: Colors.grey,
        radius: BorderRadius.circular(12),
      ),
    ),
  ),
);
void main() {
  testWidgets(
    'narrow bilingual remount retains file draft and explicit saved selection',
    (tester) async {
      await tester.binding.setSurfaceSize(const Size(350, 760));
      addTearDown(() => tester.binding.setSurfaceSize(null));
      final b = Backend();
      var picks = 0;
      Future<String?> choose() async => picks++ == 0 ? '/cert.pem' : '/key.pem';
      ServiceTlsIdentityChoice? selected;
      void changed(
        ServiceTlsSelection? file,
        ServiceTlsIdentityChoice? saved,
      ) => selected = saved;
      await tester.pumpWidget(app(b, choose: choose, changed: changed));
      await tester.pumpAndSettle();
      for (final key in ['certificate', 'private-key', 'inspect']) {
        await tap(tester, 'service-tls-$key');
      }
      await tap(tester, 'tls-identities-save');
      expect(b.saves, 1);
      expect(selected, isNull);
      await tap(tester, 'tls-identities-select-${b.rows.single.choice.key}');
      expect(selected!.revision, BigInt.one);
      await tester.pumpWidget(const SizedBox.shrink());
      await tester.pumpWidget(
        app(b, locale: 'zh', choose: choose, changed: changed),
      );
      await tester.pumpAndSettle();
      expect(find.text('已保存的 TLS 身份'), findsOneWidget);
      expect(keyed('service-tls-certificate-path'), findsOneWidget);
      expect(find.text('/cert.pem'), findsOneWidget);
      expect(selected!.revision, BigInt.one);
      expect(picks, 2);
      b.rows = [row(1, revision: 2)];
      await tap(tester, 'tls-identities-refresh');
      expect(selected, isNull);
      expect(keyed('tls-identities-stale'), findsOneWidget);
      expect(TlsIdentitySession.forBackend(b).selected!.revision, BigInt.one);
      await tap(tester, 'tls-identities-select-${b.rows.single.choice.key}');
      expect(selected!.revision, BigInt.two);
      expect(b.saves, 1);
      expect(tester.takeException(), isNull);
      await tester.pumpWidget(const SizedBox.shrink());
    },
  );
  testWidgets(
    'pending write and Unknown survive remount and require explicit acknowledgement',
    (tester) async {
      final b = Backend()..pending = Completer<ServiceTlsIdentityInfo>();
      final s = TlsIdentitySession.forBackend(b);
      await s.refresh();
      s.draft.certificate = '/cert';
      s.draft.privateKey = '/key';
      s.draft.checked = await b.inspectServiceTls(
        certificatePath: '/cert',
        privateKeyPath: '/key',
      );
      s.draft.accepted = true;
      await tester.pumpWidget(app(b));
      await tester.pumpAndSettle();
      await tester.ensureVisible(keyed('tls-identities-save'));
      await tester.tap(keyed('tls-identities-save'));
      await tester.pump();
      expect(b.saves, 1);
      await tester.pumpWidget(const SizedBox.shrink());
      await tester.pumpWidget(app(b, locale: 'zh'));
      await tester.pump();
      expect(
        tester.widget<OutlinedButton>(keyed('tls-identities-save')).onPressed,
        isNull,
      );
      b.rows = [row(1)];
      b.pending!.completeError(
        ServiceCommandFailure(
          message: 'lost',
          task: id(10),
          submission: id(11),
          command: id(12),
          outcomeUnknown: true,
        ),
      );
      await tester.pumpAndSettle();
      expect(keyed('tls-identities-unknown'), findsOneWidget);
      expect(
        tester
            .widget<OutlinedButton>(keyed('tls-identities-acknowledge'))
            .onPressed,
        isNull,
      );
      await tap(tester, 'tls-identities-refresh');
      expect(s.uncertain, isTrue);
      await tap(tester, 'tls-identities-acknowledge');
      expect(s.uncertain, isFalse);
      expect(b.saves, 1);
      expect(s.failure!.command, id(12));
      expect(find.text('/cert'), findsOneWidget);
      expect(tester.takeException(), isNull);
      await tester.pumpWidget(const SizedBox.shrink());
    },
  );
  testWidgets(
    'different backend never inherits previous identity or file draft',
    (tester) async {
      final a = Backend()..rows = [row(1)], b = Backend();
      await tester.pumpWidget(app(a));
      await tester.pumpAndSettle();
      await tap(tester, 'tls-identities-select-${a.rows.single.choice.key}');
      final old = TlsIdentitySession.forBackend(a);
      old.draft.certificate = '/old-cert';
      await tester.pumpWidget(app(b));
      await tester.pumpAndSettle();
      expect(keyed('tls-identities-selection'), findsNothing);
      expect(find.text('/old-cert'), findsNothing);
      expect(TlsIdentitySession.forBackend(b).selected, isNull);
      await tester.pumpWidget(app(a));
      await tester.pumpAndSettle();
      expect(find.text('/old-cert'), findsOneWidget);
      expect(
        TlsIdentitySession.forBackend(a).selected!.key,
        a.rows.single.choice.key,
      );
      expect(tester.takeException(), isNull);
      await tester.pumpWidget(const SizedBox.shrink());
    },
  );
}
