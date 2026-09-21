import 'dart:async';
import 'dart:typed_data';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_i18n/morrow_i18n.dart';
import 'package:morrow_studio/plugins/service_tls_picker.dart';
import 'package:morrow_studio/plugins/service_run_control.dart';

class Backend implements WorkbenchServiceTlsControl {
  Completer<ServiceTlsSelection>? pending;
  @override
  Future<ServiceTlsSelection> inspectServiceTls({
    required String certificatePath,
    required String privateKeyPath,
  }) async => pending == null
      ? selection(certificatePath, privateKeyPath)
      : pending!.future;
}

ServiceTlsSelection selection(String cert, String key) => ServiceTlsSelection(
  certificatePath: cert,
  privateKeyPath: key,
  certificateSha256: Uint8List.fromList(List.filled(32, 17)),
);

void main() {
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
      await tester.pumpWidget(host('zh'));
      await tester.pumpAndSettle();
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
