import 'dart:async';
import 'dart:typed_data';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_i18n/morrow_i18n.dart';
import 'package:morrow_studio/plugins/service_tls_picker.dart';
import 'package:morrow_studio/plugins/service_run_control.dart';

class Backend implements WorkbenchServiceTlsControl {
  Completer<ServiceTlsSelection>? pending;
  ServiceTlsValidity validity = const ServiceTlsValidity(
    notBeforeSeconds: 1,
    notAfterSeconds: 253402300799,
  );
  @override
  Future<ServiceTlsSelection> inspectServiceTls({
    required String certificatePath,
    required String privateKeyPath,
  }) async => pending == null
      ? ServiceTlsSelection(
          certificatePath: certificatePath,
          privateKeyPath: privateKeyPath,
          certificateSha256: Uint8List.fromList(List.filled(32, 17)),
          validity: validity,
        )
      : pending!.future;
}

ServiceTlsSelection selection(String cert, String key) => ServiceTlsSelection(
  certificatePath: cert,
  privateKeyPath: key,
  certificateSha256: Uint8List.fromList(List.filled(32, 17)),
  validity: const ServiceTlsValidity(
    notBeforeSeconds: 1,
    notAfterSeconds: 253402300799,
  ),
);

void main() {
  testWidgets(
    'future and expired checks require explicit reinspection without automatic reactivation',
    (tester) async {
      final backend = Backend()
        ..validity = const ServiceTlsValidity(
          notBeforeSeconds: 1000,
          notAfterSeconds: 1500,
        );
      var now = DateTime.fromMillisecondsSinceEpoch(900000, isUtc: true);
      final events = <ServiceTlsSelection?>[];
      var file = 0;
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: ServiceTlsPicker(
              backend: backend,
              enabled: true,
              now: () => now,
              chooseFile: () async => file++ == 0 ? '/cert.pem' : '/key.pem',
              onChanged: events.add,
              ink: Colors.black,
              muted: Colors.grey,
              line: Colors.grey,
              radius: BorderRadius.circular(12),
            ),
          ),
        ),
      );
      Future<void> tap(String key) async {
        await tester.tap(find.byKey(ValueKey('service-tls-$key')));
        await tester.pumpAndSettle();
      }

      await tap('certificate');
      await tap('private-key');
      await tap('inspect');
      expect(events.last, isNull);
      expect(
        find.byKey(const ValueKey('service-tls-outside-validity')),
        findsOneWidget,
      );
      now = DateTime.fromMillisecondsSinceEpoch(1200000, isUtc: true);
      await tester.pump(const Duration(seconds: 1));
      expect(events.last, isNull);
      await tap('inspect');
      expect(events.last, isNotNull);
      now = DateTime.fromMillisecondsSinceEpoch(1501000, isUtc: true);
      await tester.pump(const Duration(seconds: 1));
      expect(events.last, isNull);
      now = DateTime.fromMillisecondsSinceEpoch(1200000, isUtc: true);
      await tester.pump(const Duration(seconds: 1));
      expect(events.last, isNull);
      await tester.pumpWidget(const SizedBox.shrink());
    },
  );
  testWidgets(
    'selection requires inspection and replacing a path clears its fingerprint',
    (tester) async {
      final backend = Backend(), events = <ServiceTlsSelection?>[];
      final paths = ['/cert.pem', '/key.pem', '/other.pem'];
      var next = 0;
      Widget host(String locale) => MaterialApp(
        locale: Locale(locale),
        localizationsDelegates: AppLocalizations.localizationsDelegates,
        supportedLocales: AppLocalizations.supportedLocales,
        home: Scaffold(
          body: ServiceTlsPicker(
            backend: backend,
            enabled: true,
            chooseFile: () async => paths[next++],
            onChanged: events.add,
            ink: Colors.black,
            muted: Colors.grey,
            line: Colors.grey,
            radius: BorderRadius.circular(12),
          ),
        ),
      );
      Future<void> tap(String key) async {
        await tester.tap(find.byKey(ValueKey('service-tls-$key')));
        await tester.pumpAndSettle();
      }

      await tester.pumpWidget(host('en'));
      await tester.pumpAndSettle();
      await tap('certificate');
      await tap('private-key');
      expect(events.whereType<ServiceTlsSelection>(), isEmpty);
      await tap('inspect');
      expect(events.last!.certificatePath, '/cert.pem');
      expect(
        find.textContaining(
          '1970-01-01T00:00:01.000Z through 9999-12-31T23:59:59.000Z',
        ),
        findsOneWidget,
      );
      await tester.pumpWidget(host('zh'));
      await tester.pumpAndSettle();
      expect(find.textContaining('1970-01-01T00:00:01.000Z'), findsOneWidget);
      final validityText = tester
          .widget<Text>(find.textContaining('证书链共同有效区间'))
          .data!;
      expect(
        validityText.indexOf('1970'),
        lessThan(validityText.indexOf('9999')),
      );
      expect(
        find.byKey(const ValueKey('service-tls-fingerprint')),
        findsOneWidget,
      );
      expect(find.text('/key.pem'), findsOneWidget);
      await tap('certificate');
      expect(events.last, isNull);
      expect(
        find.byKey(const ValueKey('service-tls-fingerprint')),
        findsNothing,
      );
      await tap('inspect');
      expect(events.last!.certificatePath, '/other.pem');
      await tester.pumpWidget(const SizedBox.shrink());
    },
  );

  testWidgets(
    'inspection completed after disposal does not publish an old selection',
    (tester) async {
      final backend = Backend()..pending = Completer<ServiceTlsSelection>();
      final events = <ServiceTlsSelection?>[];
      var next = 0;
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: ServiceTlsPicker(
              backend: backend,
              enabled: true,
              chooseFile: () async => next++ == 0 ? '/cert.pem' : '/key.pem',
              onChanged: events.add,
              ink: Colors.black,
              muted: Colors.grey,
              line: Colors.grey,
              radius: BorderRadius.circular(12),
            ),
          ),
        ),
      );
      for (final key in ['certificate', 'private-key', 'inspect']) {
        await tester.tap(find.byKey(ValueKey('service-tls-$key')));
        await tester.pumpAndSettle();
      }
      await tester.pumpWidget(const SizedBox.shrink());
      backend.pending!.complete(selection('/cert.pem', '/key.pem'));
      await tester.pumpAndSettle();
      expect(events.whereType<ServiceTlsSelection>(), isEmpty);
      expect(tester.takeException(), isNull);
    },
  );
}
