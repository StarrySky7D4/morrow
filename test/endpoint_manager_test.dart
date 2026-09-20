import 'dart:async';
import 'dart:convert';
import 'dart:typed_data';

import 'package:file_selector/file_selector.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_i18n/morrow_i18n.dart';
import 'package:morrow_studio/plugins/credential_manager.dart';
import 'package:morrow_studio/plugins/endpoint_control.dart';
import 'package:morrow_studio/plugins/endpoint_manager.dart';
import 'package:morrow_studio/plugins/plugin_library.dart';

Uint8List ref(int n) => Uint8List.fromList(List.filled(32, n));
String hex(int n) => n.toRadixString(16).padLeft(2, '0') * 32;
// A single self-signed P-256 test CA encoded as binary DER, without a private key.
Uint8List certificate() => base64Decode(
  'MIIBSzCB8qADAgECAgEBMAoGCCqGSM49BAMCMCUxIzAhBgNVBAMMGk1vcnJvdyBlbmRwb2ludCBVSSB0ZXN0IENBMB4XDTI2MDEwMTAwMDAwMFoXDTM2MDEwMTAwMDAwMFowJTEjMCEGA1UEAwwaTW9ycm93IGVuZHBvaW50IFVJIHRlc3QgQ0EwWTATBgcqhkjOPQIBBggqhkjOPQMBBwNCAATmjRYaiIzQjOKe4ysMPT5lqjydXrHEOfIwWybmrKwjHD625SMzP6+AslcOAbk+p4MyDVHY31u650qNo6IfrQdOoxMwETAPBgNVHRMBAf8EBTADAQH/MAoGCCqGSM49BAMCA0gAMEUCIQD3657kDqpE9rtHgvRKck6ylKF2qV4BkNi9WGvStJPf6wIgcSa04ZdokbQY44fjxB7yjLrXOK1XLhY9AQuMq7GsFf8=',
);
PluginLibraryEntry plugin({
  int digest = 1,
  bool available = true,
  List<String> declared = const ['http-request', 'credential-use'],
  List<String> approved = const ['http-request', 'credential-use'],
}) => PluginLibraryEntry(
  id: 'test.endpoint',
  name: 'Endpoint test package',
  version: '1',
  digest: ref(digest),
  enabled: false,
  builtin: false,
  available: available,
  declared: [],
  approved: [],
  dependencies: [],
  handlers: [],
  issue: '',
  declaredIo: declared,
  approvedIo: approved,
);
EndpointPolicy policy({int digest = 1, bool advanced = false}) =>
    EndpointPolicy(
      packageId: 'test.endpoint',
      packageDigest: ref(digest),
      origin: advanced ? 'https://localhost:4443' : 'https://api.example.com',
      profile: advanced ? 3 : 1,
      methods: advanced ? ['GET', 'POST'] : ['GET'],
      credentialReference: advanced ? ref(8) : Uint8List(0),
      rootCertificate: advanced ? certificate() : Uint8List(0),
      maxRequestBytes: advanced ? 1234 : 65536,
      maxResponseBytes: advanced ? 2345 : 65536,
      maxHeaderBytes: advanced ? 456 : 16384,
      maxConcurrent: advanced ? 3 : 1,
      timeoutMs: advanced ? 12345 : 10000,
      maxFrameBytes: advanced ? 4567 : 131072,
    );
StoredEndpoint endpoint(
  int n, {
  int revision = 1,
  bool disabled = false,
  EndpointPolicy? data,
}) => StoredEndpoint(
  reference: ref(n),
  revision: BigInt.from(revision),
  createdMs: BigInt.from(DateTime(2026, 1).millisecondsSinceEpoch),
  expiresMs: BigInt.from(DateTime(2099, 1).millisecondsSinceEpoch),
  disabled: disabled,
  policy: data ?? policy(),
);
StoredCredential credential(
  int n, {
  bool disabled = false,
  DateTime? expires,
}) => StoredCredential(
  reference: ref(n),
  revision: BigInt.one,
  createdMs: BigInt.from(DateTime(2026, 1).millisecondsSinceEpoch),
  expiresMs: BigInt.from((expires ?? DateTime(2099, 1)).millisecondsSinceEpoch),
  disabled: disabled,
);

class SaveCall {
  SaveCall(
    this.reference,
    this.expectedRevision,
    this.registryRevision,
    this.days,
    this.policy,
  );
  final Uint8List reference;
  final BigInt expectedRevision, registryRevision;
  final int days;
  final EndpointPolicy policy;
}

class FakeEndpoints implements WorkbenchEndpointControl {
  List<StoredEndpoint> entries = [];
  final saves = <SaveCall>[];
  final disables = <StoredEndpoint>[];
  int reads = 0;
  Future<EndpointPage> Function(Uint8List?, Uint8List?)? page;
  Completer<StoredEndpoint>? saveGate, disableGate;
  Object? saveError;
  @override
  Future<EndpointPage> endpointPage({
    Uint8List? after,
    Uint8List? snapshot,
  }) async {
    reads++;
    if (page != null) return page!(after, snapshot);
    return EndpointPage(entries: entries, snapshot: ref(9));
  }

  @override
  Future<StoredEndpoint> saveEndpoint({
    required Uint8List reference,
    required BigInt expectedRevision,
    required BigInt registryRevision,
    required int lifetimeDays,
    required EndpointPolicy policy,
  }) async {
    saves.add(
      SaveCall(
        Uint8List.fromList(reference),
        expectedRevision,
        registryRevision,
        lifetimeDays,
        policy,
      ),
    );
    if (saveError != null) throw saveError!;
    if (saveGate != null) return saveGate!.future;
    final saved = endpoint(
      reference.isEmpty ? 4 + saves.length : reference.first,
      revision: expectedRevision.toInt() + 1,
      data: policy,
    );
    entries = [
      ...entries.where((e) => e.reference.first != saved.reference.first),
      saved,
    ];
    return saved;
  }

  @override
  Future<StoredEndpoint> disableEndpoint(StoredEndpoint expected) async {
    disables.add(expected);
    if (disableGate != null) return disableGate!.future;
    final saved = endpoint(
      expected.reference.first,
      revision: expected.revision.toInt() + 1,
      disabled: true,
      data: expected.policy,
    );
    entries = entries
        .map((e) => e.reference.first == expected.reference.first ? saved : e)
        .toList();
    return saved;
  }
}

class FakeCredentials implements WorkbenchCredentialControl {
  List<StoredCredential> entries = [credential(8)];
  @override
  Future<CredentialPage> credentialPage({
    Uint8List? after,
    Uint8List? snapshot,
  }) async => CredentialPage(entries: entries, snapshot: ref(9));
  @override
  Future<StoredCredential> saveCredential({
    required Uint8List reference,
    required BigInt expectedRevision,
    required String headerName,
    required String headerValue,
    required int lifetimeDays,
  }) => throw UnsupportedError('Endpoint UI must not write credentials');
  @override
  Future<StoredCredential> disableCredential(StoredCredential expected) =>
      throw UnsupportedError('Endpoint UI must not disable credentials');
}

Widget page(
  FakeEndpoints backend, {
  List<PluginLibraryEntry>? plugins,
  int revision = 7,
  FakeCredentials? credentials,
  Future<XFile?> Function()? pickCertificate,
  String locale = 'en',
}) => MaterialApp(
  locale: Locale(locale),
  localizationsDelegates: AppLocalizations.localizationsDelegates,
  supportedLocales: AppLocalizations.supportedLocales,
  home: Scaffold(
    body: SingleChildScrollView(
      child: EndpointManager(
        backend: backend,
        plugins: plugins ?? [plugin()],
        registryRevision: BigInt.from(revision),
        credentialBackend: credentials,
        pickCertificate: pickCertificate,
        ink: Colors.black,
        muted: Colors.grey,
        line: Colors.grey,
        radius: BorderRadius.circular(12),
      ),
    ),
  ),
);
Future<void> click(WidgetTester tester, String key) async {
  final target = find.byKey(ValueKey(key));
  await tester.ensureVisible(target);
  await tester.pumpAndSettle();
  await tester.tap(target);
  await tester.pumpAndSettle();
}

Future<void> enter(WidgetTester tester, String key, String value) async {
  final target = find.byKey(ValueKey('endpoint-$key'));
  await tester.ensureVisible(target);
  await tester.enterText(target, value);
  await tester.pump();
}

Finder dropdown(String prefix) => find.byWidgetPredicate(
  (w) =>
      w.key is ValueKey<String> &&
      (w.key! as ValueKey<String>).value.startsWith(prefix),
);
Future<void> choose<T>(WidgetTester tester, String prefix, T value) async {
  tester.widget<DropdownButtonFormField<T>>(dropdown(prefix)).onChanged!(value);
  await tester.pumpAndSettle();
}

String field(WidgetTester tester, String key) => tester
    .widget<TextField>(find.byKey(ValueKey('endpoint-$key')))
    .controller!
    .text;
void main() {
  testWidgets(
    'public HTTPS accepts a DER root; HTTP exposes no certificate picker',
    (tester) async {
      final backend = FakeEndpoints();
      await tester.pumpWidget(
        page(
          backend,
          pickCertificate: () async =>
              XFile.fromData(certificate(), name: 'root.der', path: 'root.der'),
        ),
      );
      await tester.pumpAndSettle();
      await click(tester, 'endpoint-new');
      await enter(tester, 'origin', 'https://api.example.com');
      await click(tester, 'endpoint-certificate');
      await click(tester, 'endpoint-save');
      expect(backend.saves.single.policy.profile, 1);
      expect(
        backend.saves.single.policy.rootCertificate,
        orderedEquals(certificate()),
      );
      await click(tester, 'endpoint-new');
      await choose(tester, 'endpoint-profile-', 2);
      expect(find.byKey(const ValueKey('endpoint-certificate')), findsNothing);
      await enter(tester, 'origin', 'http://127.0.0.2:8123');
      await click(tester, 'endpoint-save');
      expect(backend.saves.last.policy.profile, 2);
      expect(backend.saves.last.policy.rootCertificate, isEmpty);
      expect(
        find.text('Endpoint approval saved. No network connection was made.'),
        findsOneWidget,
      );
    },
  );

  testWidgets(
    'empty shared-table pages advance with a pinned snapshot and never write on refresh',
    (tester) async {
      final backend = FakeEndpoints();
      backend.page = (after, snapshot) async {
        if (after == null) {
          expect(snapshot, isNull);
          return EndpointPage(entries: [], snapshot: ref(9), next: ref(2));
        }
        expect(after, orderedEquals(ref(2)));
        expect(snapshot, orderedEquals(ref(9)));
        return EndpointPage(entries: [endpoint(3)], snapshot: ref(9));
      };
      await tester.pumpWidget(page(backend));
      await tester.pumpAndSettle();
      expect(backend.reads, 2);
      expect(find.byKey(ValueKey('endpoint-row-${hex(3)}')), findsOneWidget);
      await click(tester, 'endpoint-refresh');
      expect(backend.reads, 4);
      expect(backend.saves, isEmpty);
      expect(backend.disables, isEmpty);
    },
  );
  for (final broken in ['snapshot', 'order', 'cursor', 'page-size', 'bound']) {
    testWidgets('rejects inconsistent endpoint pagination: $broken', (
      tester,
    ) async {
      final backend = FakeEndpoints();
      backend.page = (after, snapshot) async {
        if (broken == 'bound') {
          final cursor = Uint8List(32)
            ..buffer.asByteData().setUint32(28, backend.reads);
          return EndpointPage(entries: [], snapshot: ref(9), next: cursor);
        }
        if (broken == 'page-size') {
          return EndpointPage(
            entries: [endpoint(1), endpoint(2), endpoint(3)],
            snapshot: ref(9),
          );
        }
        if (after == null) {
          return EndpointPage(
            entries: [endpoint(2)],
            snapshot: ref(9),
            next: ref(2),
          );
        }
        return EndpointPage(
          entries: broken == 'order' ? [endpoint(1)] : [],
          snapshot: ref(broken == 'snapshot' ? 8 : 9),
          next: broken == 'cursor' ? ref(2) : null,
        );
      };
      await tester.pumpWidget(page(backend));
      await tester.pumpAndSettle();
      expect(
        find.text(
          'Endpoint approvals could not be read consistently. Refresh status to try again.',
        ),
        findsOneWidget,
      );
      expect(
        tester
            .widget<OutlinedButton>(find.byKey(const ValueKey('endpoint-new')))
            .onPressed,
        isNull,
      );
      expect(backend.reads, lessThanOrEqualTo(512));
      expect(backend.saves, isEmpty);
    });
  }
  testWidgets(
    'explicit create transmits every policy field, current digest, reference and requested duration',
    (tester) async {
      final backend = FakeEndpoints(), credentials = FakeCredentials();
      // A one-day credential is displayed; the requested 30 days must not be silently changed.
      credentials.entries = [
        credential(8, expires: DateTime.now().add(const Duration(days: 1))),
      ];
      await tester.pumpWidget(
        page(
          backend,
          credentials: credentials,
          pickCertificate: () async =>
              XFile.fromData(certificate(), name: 'root.der', path: 'root.der'),
        ),
      );
      await tester.pumpAndSettle();
      expect(backend.saves, isEmpty);
      await click(tester, 'endpoint-new');
      await enter(tester, 'origin', 'https://localhost:4443');
      await choose(tester, 'endpoint-profile-', 3);
      await click(tester, 'endpoint-method-POST');
      await choose(tester, 'endpoint-credential-', hex(8));
      await enter(tester, 'days', '30');
      for (final e in {
        'request': '1234',
        'response': '2345',
        'headers': '456',
        'concurrent': '3',
        'timeout': '12345',
        'frame': '4567',
      }.entries) {
        await enter(tester, e.key, e.value);
      }
      await click(tester, 'endpoint-certificate');
      expect(find.textContaining('DER trust root selected'), findsOneWidget);
      await click(tester, 'endpoint-save');
      final call = backend.saves.single, p = call.policy;
      expect(call.reference, isEmpty);
      expect(call.expectedRevision, BigInt.zero);
      expect(call.registryRevision, BigInt.from(7));
      expect(call.days, 30);
      expect(p.packageId, 'test.endpoint');
      expect(p.packageDigest, orderedEquals(ref(1)));
      expect(p.origin, 'https://localhost:4443');
      expect(p.profile, 3);
      expect(p.methods, ['GET', 'POST']);
      expect(p.credentialReference, orderedEquals(ref(8)));
      expect(p.rootCertificate, orderedEquals(certificate()));
      expect(
        [
          p.maxRequestBytes,
          p.maxResponseBytes,
          p.maxHeaderBytes,
          p.maxConcurrent,
          p.timeoutMs,
          p.maxFrameBytes,
        ],
        [1234, 2345, 456, 3, 12345, 4567],
      );
      expect(
        find.text('Endpoint approval saved. No network connection was made.'),
        findsOneWidget,
      );
    },
  );
  testWidgets(
    'replacement pins package ID, preserves existing policy and explicitly binds the current upgraded digest',
    (tester) async {
      final backend = FakeEndpoints()
        ..entries = [endpoint(3, revision: 9, data: policy(advanced: true))];
      await tester.pumpWidget(
        page(
          backend,
          plugins: [plugin(digest: 2)],
          credentials: FakeCredentials(),
        ),
      );
      await tester.pumpAndSettle();
      expect(backend.saves, isEmpty);
      await click(tester, 'endpoint-replace-${hex(3)}');
      expect(
        tester
            .widget<DropdownButtonFormField<String>>(
              dropdown('endpoint-package-'),
            )
            .onChanged,
        isNull,
      );
      expect(field(tester, 'request'), '1234');
      expect(field(tester, 'response'), '2345');
      expect(field(tester, 'headers'), '456');
      expect(field(tester, 'concurrent'), '3');
      expect(field(tester, 'timeout'), '12345');
      expect(field(tester, 'frame'), '4567');
      await click(tester, 'endpoint-save');
      final call = backend.saves.single;
      expect(call.reference, orderedEquals(ref(3)));
      expect(call.expectedRevision, BigInt.from(9));
      expect(call.policy.packageDigest, orderedEquals(ref(2)));
      expect(call.policy.rootCertificate, orderedEquals(certificate()));
      expect(call.policy.credentialReference, orderedEquals(ref(8)));
      expect(call.policy.profile, 3);
    },
  );
  testWidgets('missing packages can still be disabled but cannot be replaced', (
    tester,
  ) async {
    final backend = FakeEndpoints()..entries = [endpoint(3)];
    await tester.pumpWidget(page(backend, plugins: []));
    await tester.pumpAndSettle();
    expect(
      tester
          .widget<OutlinedButton>(
            find.byKey(ValueKey('endpoint-replace-${hex(3)}')),
          )
          .onPressed,
      isNull,
    );
    await click(tester, 'endpoint-disable-${hex(3)}');
    expect(backend.disables.single.reference, orderedEquals(ref(3)));
    expect(find.text('Endpoint approval disabled.'), findsOneWidget);
    expect(
      tester
          .widget<OutlinedButton>(
            find.byKey(ValueKey('endpoint-disable-${hex(3)}')),
          )
          .onPressed,
      isNull,
    );
  });
  for (final kind in ['unavailable', 'undeclared', 'unapproved']) {
    testWidgets('creation excludes packages with $kind HTTP permission', (
      tester,
    ) async {
      final p = plugin(
        available: kind != 'unavailable',
        declared: kind == 'undeclared' ? [] : ['http-request'],
        approved: kind == 'unapproved' ? [] : ['http-request'],
      );
      final backend = FakeEndpoints();
      await tester.pumpWidget(page(backend, plugins: [p]));
      await tester.pumpAndSettle();
      expect(
        tester
            .widget<OutlinedButton>(find.byKey(const ValueKey('endpoint-new')))
            .onPressed,
        isNull,
      );
      expect(backend.saves, isEmpty);
    });
  }
  testWidgets(
    'credential permission is required and disabled or expired credentials cannot be selected',
    (tester) async {
      final backend = FakeEndpoints(), credentials = FakeCredentials();
      credentials.entries = [
        credential(6, disabled: true),
        credential(7, expires: DateTime(2026, 2)),
        credential(8),
      ];
      await tester.pumpWidget(
        page(
          backend,
          plugins: [
            plugin(approved: ['http-request']),
          ],
          credentials: credentials,
        ),
      );
      await tester.pumpAndSettle();
      await click(tester, 'endpoint-new');
      final widget = tester.widget<DropdownButton<String>>(
        find.descendant(
          of: dropdown('endpoint-credential-'),
          matching: find.byType(DropdownButton<String>),
        ),
      );
      expect(widget.items!.map((i) => i.value), ['', hex(8)]);
      await choose(tester, 'endpoint-credential-', hex(8));
      await enter(tester, 'origin', 'https://api.example.com');
      await click(tester, 'endpoint-save');
      expect(backend.saves, isEmpty);
      expect(find.textContaining('Check the package, origin'), findsOneWidget);
    },
  );
  for (final bad in ['pem', 'extension', 'oversize', 'trailing', 'truncated']) {
    testWidgets(
      'rejects $bad certificate without sending policy or leaking paths',
      (tester) async {
        var bytes = certificate();
        var name = 'root.cer';
        if (bad == 'pem') {
          bytes = Uint8List.fromList(
            utf8.encode('-----BEGIN CERTIFICATE-----secret'),
          );
        }
        if (bad == 'extension') name = 'root.pem';
        if (bad == 'oversize') bytes = Uint8List(32769);
        if (bad == 'trailing') bytes = Uint8List.fromList([...bytes, ...bytes]);
        if (bad == 'truncated') {
          bytes = Uint8List.sublistView(bytes, 0, bytes.length - 1);
        }
        final backend = FakeEndpoints();
        await tester.pumpWidget(
          page(
            backend,
            pickCertificate: () async =>
                XFile.fromData(bytes, name: name, path: name),
          ),
        );
        await tester.pumpAndSettle();
        await click(tester, 'endpoint-new');
        await click(tester, 'endpoint-certificate');
        expect(
          find.text(
            'Choose one valid binary DER certificate (.der or .cer) no larger than 32 KiB.',
          ),
          findsOneWidget,
        );
        expect(find.textContaining('DER trust root selected'), findsNothing);
        expect(backend.saves, isEmpty);
        expect(find.textContaining('BEGIN CERTIFICATE'), findsNothing);
      },
    );
  }
  testWidgets('A to B to A ignores late load, including its finally block', (
    tester,
  ) async {
    final a = FakeEndpoints(),
        b = FakeEndpoints(),
        gate = Completer<EndpointPage>();
    a.page = (after, snapshot) => a.reads == 1
        ? gate.future
        : Future.value(EndpointPage(entries: [endpoint(2)], snapshot: ref(9)));
    await tester.pumpWidget(page(a));
    await tester.pump();
    await tester.pumpWidget(page(b));
    await tester.pumpAndSettle();
    await tester.pumpWidget(page(a));
    await tester.pumpAndSettle();
    await click(tester, 'endpoint-new');
    await enter(tester, 'origin', 'https://current.example.com');
    gate.complete(EndpointPage(entries: [endpoint(7)], snapshot: ref(9)));
    await tester.pumpAndSettle();
    expect(find.byKey(ValueKey('endpoint-row-${hex(7)}')), findsNothing);
    expect(find.byKey(ValueKey('endpoint-row-${hex(2)}')), findsOneWidget);
    expect(field(tester, 'origin'), 'https://current.example.com');
  });
  for (final change in ['backend', 'digest', 'registry', 'close']) {
    testWidgets(
      'late save is isolated after $change and does not overwrite a new form',
      (tester) async {
        final a = FakeEndpoints()..saveGate = Completer<StoredEndpoint>(),
            b = FakeEndpoints();
        await tester.pumpWidget(page(a));
        await tester.pumpAndSettle();
        await click(tester, 'endpoint-new');
        await enter(tester, 'origin', 'https://old.example.com');
        await click(tester, 'endpoint-save');
        final oldCall = a.saves.single;
        if (change == 'backend') {
          await tester.pumpWidget(page(b));
          await tester.pumpAndSettle();
          await tester.pumpWidget(page(a));
          await tester.pumpAndSettle();
        } else if (change == 'digest') {
          await tester.pumpWidget(page(a, plugins: [plugin(digest: 2)]));
          await tester.pumpAndSettle();
        } else if (change == 'registry') {
          await tester.pumpWidget(page(a, revision: 8));
          await tester.pumpAndSettle();
        } else {
          await click(tester, 'endpoint-close');
          await click(tester, 'endpoint-refresh');
        }
        await click(tester, 'endpoint-new');
        await enter(tester, 'origin', 'https://new.example.com');
        a.saveGate!.complete(endpoint(5, data: oldCall.policy));
        await tester.pumpAndSettle();
        expect(field(tester, 'origin'), 'https://new.example.com');
        expect(find.byKey(ValueKey('endpoint-row-${hex(5)}')), findsNothing);
        expect(
          find.text('Endpoint approval saved. No network connection was made.'),
          findsNothing,
        );
        expect(a.saves, hasLength(1));
      },
    );
  }
  testWidgets('late disable cannot change the new directory session', (
    tester,
  ) async {
    final backend = FakeEndpoints()
      ..entries = [endpoint(3)]
      ..disableGate = Completer<StoredEndpoint>();
    await tester.pumpWidget(page(backend));
    await tester.pumpAndSettle();
    await click(tester, 'endpoint-disable-${hex(3)}');
    await tester.pumpWidget(page(backend, revision: 8));
    await tester.pumpAndSettle();
    await click(tester, 'endpoint-new');
    backend.disableGate!.complete(endpoint(3, revision: 2, disabled: true));
    await tester.pumpAndSettle();
    expect(find.byKey(const ValueKey('endpoint-origin')), findsOneWidget);
    expect(find.text('Endpoint approval disabled.'), findsNothing);
    expect(
      tester
          .widget<OutlinedButton>(
            find.byKey(ValueKey('endpoint-disable-${hex(3)}')),
          )
          .onPressed,
      isNotNull,
    );
  });
  testWidgets('late certificate picker cannot modify a reopened form', (
    tester,
  ) async {
    final backend = FakeEndpoints(), gate = Completer<XFile?>();
    await tester.pumpWidget(page(backend, pickCertificate: () => gate.future));
    await tester.pumpAndSettle();
    await click(tester, 'endpoint-new');
    await click(tester, 'endpoint-certificate');
    await click(tester, 'endpoint-close');
    await click(tester, 'endpoint-new');
    gate.complete(
      XFile.fromData(certificate(), name: 'root.der', path: 'root.der'),
    );
    await tester.pumpAndSettle();
    expect(find.textContaining('DER trust root selected'), findsNothing);
  });
  testWidgets(
    'unknown write result requires a read and never replays a request',
    (tester) async {
      final backend = FakeEndpoints()
        ..saveError = StateError('secret at C:/private/credential');
      await tester.pumpWidget(page(backend));
      await tester.pumpAndSettle();
      await click(tester, 'endpoint-new');
      await enter(tester, 'origin', 'https://api.example.com');
      await click(tester, 'endpoint-save');
      expect(
        find.textContaining('The result could not be confirmed.'),
        findsOneWidget,
      );
      expect(find.textContaining('C:/private'), findsNothing);
      expect(
        tester
            .widget<OutlinedButton>(find.byKey(const ValueKey('endpoint-new')))
            .onPressed,
        isNull,
      );
      await click(tester, 'endpoint-refresh');
      expect(backend.saves, hasLength(1));
    },
  );
  for (final locale in ['en', 'zh']) {
    testWidgets(
      'complete $locale editor and actions fit a 320 pixel viewport',
      (tester) async {
        tester.view.physicalSize = const Size(320, 720);
        tester.view.devicePixelRatio = 1;
        addTearDown(tester.view.resetPhysicalSize);
        addTearDown(tester.view.resetDevicePixelRatio);
        final backend = FakeEndpoints()..entries = [endpoint(3)];
        await tester.pumpWidget(page(backend, locale: locale));
        await tester.pumpAndSettle();
        await click(tester, 'endpoint-new');
        await tester.ensureVisible(find.byKey(const ValueKey('endpoint-save')));
        await tester.pumpAndSettle();
        expect(tester.takeException(), isNull);
      },
    );
  }
}
