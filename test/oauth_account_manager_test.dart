import 'dart:typed_data';

import 'package:material_ui/material_ui.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/oauth_accounts.dart';
import 'package:morrow_studio/plugins/oauth_account_manager.dart';
import 'package:morrow_studio/plugins/plugin_library.dart';

import 'endpoint_manager_test.dart' as endpoints;

Uint8List ref(int n) => Uint8List.fromList(List.filled(32, n));
final service = OAuthService(
  ref(3),
  'Work account',
  Uri.parse('https://id.example'),
  Uri.parse('https://id.example/authorize'),
  Uri.parse('https://api.example.com'),
  'cards.read',
  Uri.parse('https://id.example/token'),
  'workbench',
);
final foreign = OAuthService(
  ref(4),
  'Other service',
  Uri.parse('https://other.example'),
  Uri.parse('https://other.example/authorize'),
  Uri.parse('https://other-api.example'),
  'cards.write',
  Uri.parse('https://other.example/token'),
  'other-workbench',
);
Uri signInUrl() => service.authorize.replace(
  queryParameters: {
    'client_id': 'workbench',
    'response_type': 'code',
    'scope': 'cards.read',
    'redirect_uri': 'http://127.0.0.1:4242/morrow/oauth/callback',
    'state': 'a' * 43,
    'code_challenge': 'b' * 43,
    'code_challenge_method': 'S256',
  },
);

class Accounts extends endpoints.FakeEndpoints
    implements WorkbenchOAuthControl {
  @override
  bool get supportsOAuthAccounts => true;
  final calls = <OAuthCommand>[];
  List<OAuthEntry> rows = [];
  bool loseLogin = false;
  String? browserOverride;
  @override
  Future<OAuthResult> accountControl(
    PluginLibraryEntry entry,
    BigInt revision,
    OAuthCommand cmd,
  ) async {
    calls.add(cmd);
    switch (cmd.action) {
      case OAuthAction.services:
        return OAuthResult(services: [service, foreign]);
      case OAuthAction.accounts:
        return OAuthResult(accounts: rows, snapshot: ref(9));
      case OAuthAction.login:
        if (loseLogin) throw StateError('connection ended');
        return OAuthResult(
          key: cmd.key,
          account: ref(5),
          phase: OAuthPhase.waiting,
          browser: browserOverride ?? signInUrl().toString(),
        );
      case OAuthAction.cancel:
        return OAuthResult(
          key: cmd.key,
          account: ref(5),
          phase: OAuthPhase.cancelled,
        );
      case OAuthAction.refresh:
      case OAuthAction.poll:
        rows = [
          OAuthEntry(
            ref(5),
            BigInt.one,
            OAuthState.available,
            DateTime(2099).millisecondsSinceEpoch,
          ),
        ];
        return OAuthResult(
          key: cmd.key,
          account: ref(5),
          phase: OAuthPhase.done,
          accounts: rows,
        );
      case OAuthAction.logout:
        rows = [OAuthEntry(ref(5), BigInt.two, OAuthState.signedOut, 0)];
        return OAuthResult(accounts: rows);
    }
  }
}

PluginLibraryEntry plugin() => PluginLibraryEntry(
  id: 'test.account',
  name: 'Example plugin',
  version: '1',
  digest: ref(2),
  enabled: true,
  builtin: false,
  available: true,
  declared: [],
  approved: [],
  dependencies: [],
  handlers: [],
  issue: '',
  declaredIo: ['http-request', 'credential-use'],
  approvedIo: ['http-request', 'credential-use'],
);
Future<void> tap(WidgetTester t, String label, {bool last = false}) async {
  final target = last ? find.text(label).last : find.text(label).first;
  await t.ensureVisible(target);
  await t.tap(target);
  await t.pumpAndSettle();
}

Future<void> mount(WidgetTester t, Accounts a, List<Uri> opened) async {
  await t.pumpWidget(
    MaterialApp(
      home: Scaffold(
        body: SingleChildScrollView(
          child: OAuthAccountManager(
            backend: a,
            entries: [plugin()],
            revision: BigInt.one,
            openBrowser: (u) async {
              opened.add(u);
              return true;
            },
          ),
        ),
      ),
    ),
  );
  await tap(t, 'Example plugin');
  await tap(t, 'Work account');
}

void main() {
  testWidgets(
    'review before system browser, keep original submission on poll, select account to sign out',
    (t) async {
      final a = Accounts(), opened = <Uri>[];
      await mount(t, a, opened);
      await tap(t, 'Sign in');
      expect(find.textContaining('cards.read'), findsOneWidget);
      expect(find.textContaining('https://id.example/token'), findsOneWidget);
      expect(find.textContaining('Client ID workbench'), findsOneWidget);
      await tap(t, 'Cancel');
      expect(a.calls.where((v) => v.action == OAuthAction.login), isEmpty);
      expect(opened, isEmpty);
      await tap(t, 'Sign in');
      await tap(t, 'Open browser');
      expect(opened, [signInUrl()]);
      final start = a.calls.singleWhere((v) => v.action == OAuthAction.login);
      expect(start.days, 7);
      await tap(t, 'Check status');
      expect(opened.length, 1);
      expect(
        a.calls.singleWhere((v) => v.action == OAuthAction.poll).key,
        orderedEquals(start.key!),
      );
      expect(find.text('Access available'), findsOneWidget);
      await tap(t, 'Refresh access');
      expect(
        a.calls.singleWhere((v) => v.action == OAuthAction.refresh).account,
        orderedEquals(ref(5)),
      );
      await tap(t, 'Sign out');
      await tap(t, 'Cancel');
      expect(a.calls.where((v) => v.action == OAuthAction.logout), isEmpty);
      await tap(t, 'Sign out');
      await tap(t, 'Sign out', last: true);
      expect(
        a.calls.singleWhere((v) => v.action == OAuthAction.logout).revision,
        BigInt.one,
      );
      expect(find.text('Signed out'), findsOneWidget);
    },
  );
  testWidgets(
    'lost initial reply preserves single submission and cancellation never opens a browser',
    (t) async {
      final a = Accounts()..loseLogin = true, opened = <Uri>[];
      await mount(t, a, opened);
      await tap(t, 'Sign in');
      await tap(t, 'Open browser');
      expect(opened, isEmpty);
      final original = a.calls.singleWhere(
        (v) => v.action == OAuthAction.login,
      );
      await tap(t, 'Cancel request');
      expect(
        a.calls.singleWhere((v) => v.action == OAuthAction.cancel).key,
        orderedEquals(original.key!),
      );
      expect(opened, isEmpty);
      expect(find.text('Cancelled.'), findsOneWidget);
    },
  );
  testWidgets(
    'an unexpected destination from a start reply cannot open the external browser',
    (t) async {
      final a = Accounts()
            ..browserOverride = signInUrl()
                .replace(host: 'other.example')
                .toString(),
          opened = <Uri>[];
      await mount(t, a, opened);
      await tap(t, 'Sign in');
      await tap(t, 'Open browser');
      expect(opened, isEmpty);
      expect(find.text('Cancel request'), findsOneWidget);
    },
  );
  testWidgets(
    'endpoint editor loads only matching origin accounts and saves the chosen reference',
    (t) async {
      final a = Accounts()
        ..rows = [
          OAuthEntry(
            ref(5),
            BigInt.one,
            OAuthState.available,
            DateTime(2099).millisecondsSinceEpoch,
          ),
        ];
      await t.pumpWidget(endpoints.page(a));
      await t.pumpAndSettle();
      await endpoints.click(t, 'endpoint-new');
      await endpoints.enter(t, 'origin', 'https://api.example.com');
      await tap(t, 'Load service accounts for this origin');
      final discovery = a.calls.where((v) => v.action == OAuthAction.accounts);
      expect(discovery.length, 1);
      expect(discovery.single.service, orderedEquals(service.key));
      await endpoints.choose<String>(
        t,
        'endpoint-credential-',
        accountHex(ref(5)),
      );
      await endpoints.click(t, 'endpoint-save');
      expect(a.saves.single.policy.credentialReference, orderedEquals(ref(5)));
      expect(
        a.calls.where(
          (v) =>
              v.action == OAuthAction.login || v.action == OAuthAction.refresh,
        ),
        isEmpty,
      );
    },
  );
  test(
    'reviewed browser URL has a single issuer path and a local callback',
    () {
      final original = signInUrl();
      expect(service.browser(original.toString()), original);
      for (final uri in [
        original.replace(scheme: 'http'),
        original.replace(path: '/other'),
        original.replace(fragment: 'x'),
        original.replace(query: original.query + '&scope=excess'),
        original.replace(
          queryParameters: {
            ...original.queryParameters,
            'client_id': 'another-client',
          },
        ),
        original.replace(
          queryParameters: {...original.queryParameters, 'extra': 'unreviewed'},
        ),
        original.replace(
          queryParameters: {
            ...original.queryParameters,
            'redirect_uri': 'https://other.example/callback',
          },
        ),
        original.replace(
          queryParameters: {
            ...original.queryParameters,
            'code_challenge_method': 'plain',
          },
        ),
      ]) {
        expect(() => service.browser(uri.toString()), throwsFormatException);
      }
    },
  );
}
