import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';
import 'package:crypto/crypto.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/service_run_control.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';
import 'service_run_real_native_fixture.dart';

final _available =
    RealServiceFixture.available &&
    Platform.environment.containsKey('MORROW_TLS_FIXTURE');
Future<void> _generate(Directory directory, {int? before, int? after}) async {
  final result = await Process.run(
    Platform.environment['MORROW_TLS_FIXTURE']!,
    [
      directory.path,
      if (before != null) '$before',
      if (after != null) '$after',
    ],
  );
  expect(result.exitCode, 0, reason: '${result.stderr}');
}

Future<String> _post(RealServiceFixture fixture, String certificate) async {
  final address = Uri.parse('https://${fixture.address}');
  final context = SecurityContext(withTrustedRoots: false)
    ..setTrustedCertificates(certificate);
  final socket = await SecureSocket.connect(
    address.host,
    address.port,
    context: context,
    timeout: const Duration(seconds: 5),
  );
  final request = Uint8List.fromList([
    ...utf8.encode(
      'POST /api HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer ',
    ),
    ...fixture.issued!.token.bytes,
    ...utf8.encode(
      '\r\nIdempotency-Key: application-service-before\r\nContent-Length: 6\r\nConnection: close\r\n\r\nbefore',
    ),
  ]);
  try {
    socket.add(request);
    await socket.flush();
    return utf8.decode(
      await socket
          .fold<List<int>>([], (all, part) => all..addAll(part))
          .timeout(const Duration(seconds: 10)),
    );
  } finally {
    request.fillRange(0, request.length, 0);
    socket.destroy();
  }
}

void main() {
  for (final rotate in [false, true]) {
    test(
      'saved TLS identity ${rotate ? "rotation" : "disable"} uses real owner and closes listener',
      () async {
        final f = await RealServiceFixture.open(tlsRequired: true);
        try {
          final pem = Directory('${f.directory.path}/tls');
          await _generate(pem);
          final cert = '${pem.path}/certificate.pem',
              key = '${pem.path}/private-key.pem';
          final trust = '${f.directory.path}/trust.pem';
          await File(cert).copy(trust);
          final inspected = await f.backend.inspectServiceTls(
            certificatePath: cert,
            privateKeyPath: key,
          );
          final saved = await f.backend.saveTlsIdentity(
            selection: inspected,
            reference: Uint8List(0),
            expectedRevision: BigInt.zero,
          );
          await File(cert).delete();
          await File(key).delete();
          await f.session.start(f.request(protectedTls: saved.choice));
          await f.observe(ServiceRunPhase.running);
          expect(await _post(f, trust), startsWith('HTTP/1.1 202 '));
          final before = await f.backend.tlsIdentityPage();
          expect(
            before.identities.single.choice.reference,
            saved.choice.reference,
          );
          await _generate(pem);
          final next = await f.backend.inspectServiceTls(
            certificatePath: cert,
            privateKeyPath: key,
          );
          await f.backend.saveTlsIdentity(
            selection: next,
            reference: Uint8List(0),
            expectedRevision: BigInt.zero,
          );
          expect(await _post(f, trust), startsWith('HTTP/1.1 202 '));
          final updated = rotate
              ? await f.backend.saveTlsIdentity(
                  selection: next,
                  reference: saved.choice.reference,
                  expectedRevision: saved.choice.revision,
                )
              : await f.backend.disableTlsIdentity(saved);
          expect(updated.choice.revision, BigInt.two);
          expect(updated.disabled, !rotate);
          await f.observe(ServiceRunPhase.exited);
          await f.session.acknowledge();
          await expectLater(
            f.backend.tlsIdentityPage(snapshot: before.snapshot),
            throwsA(isA<StateError>()),
          );
          await expectLater(
            f.backend.startServiceRun(
              f.request(submission: 32, protectedTls: saved.choice),
            ),
            throwsA(isA<ServiceRunStartFailure>()),
          );
          if (rotate) {
            await f.session.start(
              f.request(submission: 33, protectedTls: updated.choice),
            );
            await f.observe(ServiceRunPhase.running);
            await expectLater(
              _post(f, trust),
              throwsA(isA<HandshakeException>()),
            );
            expect(await _post(f, cert), startsWith('HTTP/1.1 202 '));
            await f.session.stop();
            await f.observe(ServiceRunPhase.exited);
            await f.session.acknowledge();
          }
          await f.backend.close();
          final reopened = await RustWorkbench.open(
            executable: Platform.environment['MORROW_WORKBENCH_HOST']!,
            package: Platform.environment['MORROW_WORKBENCH_PACKAGE']!,
            directory: f.directory,
            managed: true,
          );
          try {
            final row = (await reopened.tlsIdentityPage()).identities
                .singleWhere((row) => row.choice.key == saved.choice.key);
            expect(row.choice.revision, BigInt.two);
            expect(row.disabled, !rotate);
          } finally {
            await reopened.close();
          }
        } finally {
          await f.close(observeBeforeClose: false);
        }
      },
      skip: !_available,
      timeout: const Timeout(Duration(minutes: 2)),
    );
  }
  test(
    'real host refuses expired/future PEM even with fabricated valid metadata',
    () async {
      final fixture = await RealServiceFixture.open(tlsRequired: true);
      try {
        final pem = Directory('${fixture.directory.path}/tls');
        final now = DateTime.now().millisecondsSinceEpoch ~/ 1000;
        for (final bounds in [(now - 100, now - 10), (now + 100, now + 200)]) {
          await _generate(pem, before: bounds.$1, after: bounds.$2);
          final selected = await fixture.backend.inspectServiceTls(
            certificatePath: '${pem.path}/certificate.pem',
            privateKeyPath: '${pem.path}/private-key.pem',
          );
          expect(selected.validity!.notBeforeSeconds, bounds.$1);
          expect(selected.validity!.notAfterSeconds, bounds.$2);
          expect(selected.validity!.validAt(DateTime.now()), isFalse);
          final fabricated = ServiceTlsSelection(
            certificatePath: selected.certificatePath,
            privateKeyPath: selected.privateKeyPath,
            certificateSha256: selected.certificateSha256,
            validity: const ServiceTlsValidity(
              notBeforeSeconds: 0,
              notAfterSeconds: 253402300799,
            ),
          );
          await expectLater(
            fixture.backend.startServiceRun(fixture.request(tls: fabricated)),
            throwsA(isA<ServiceRunStartFailure>()),
          );
          expect((await fixture.backend.ioStatus()).key, isNull);
        }
        // Rejections leave the original owner and exact submission available.
        await _generate(pem);
        final selected = await fixture.backend.inspectServiceTls(
          certificatePath: '${pem.path}/certificate.pem',
          privateKeyPath: '${pem.path}/private-key.pem',
        );
        await fixture.session.start(fixture.request(tls: selected));
        await fixture.observe(ServiceRunPhase.running);
        expect(
          await _post(fixture, selected.certificatePath),
          startsWith('HTTP/1.1 202 '),
        );
      } finally {
        await fixture.close();
      }
    },
    skip: !_available,
    timeout: const Timeout(Duration(minutes: 2)),
  );

  test(
    'real private TLS inspection, original owner start, HTTPS and same-library reopen',
    () async {
      final fixture = await RealServiceFixture.open(tlsRequired: true);
      try {
        final pem = Directory('${fixture.directory.path}/tls');
        await _generate(pem);
        final cert = '${pem.path}/certificate.pem',
            key = '${pem.path}/private-key.pem';
        final selected = await fixture.backend.inspectServiceTls(
          certificatePath: cert,
          privateKeyPath: key,
        );
        expect(selected.validity, isNotNull);
        expect(selected.validity!.validAt(DateTime.now()), isTrue);
        expect(
          selected.certificateSha256,
          orderedEquals(sha256.convert(await File(cert).readAsBytes()).bytes),
        );
        // The same exact start identity remains available after a digest rejection.
        final stale = ServiceTlsSelection(
          certificatePath: cert,
          privateKeyPath: key,
          certificateSha256: Uint8List.fromList(List.filled(32, 9)),
        );
        await expectLater(
          fixture.backend.startServiceRun(fixture.request(tls: stale)),
          throwsA(isA<ServiceRunStartFailure>()),
        );
        expect((await fixture.backend.ioStatus()).key, isNull);
        await fixture.session.start(fixture.request(tls: selected));
        await fixture.observe(ServiceRunPhase.running);
        final output = await _post(fixture, cert);
        expect(output, startsWith('HTTP/1.1 202 '));
        expect(output, endsWith('executed-before'));
        await fixture.backend.saveUiLocale('en');
        // Ordinary inspector also uses the existing owner lane while TLS runs.
        final rechecked = await fixture.backend.inspectServiceTls(
          certificatePath: cert,
          privateKeyPath: key,
        );
        expect(
          rechecked.certificateSha256,
          orderedEquals(selected.certificateSha256),
        );
        await fixture.session.stop();
        await fixture.observe(ServiceRunPhase.exited);
        await fixture.session.acknowledge();
        await fixture.backend.close();
        final reopened = await RustWorkbench.open(
          executable: Platform.environment['MORROW_WORKBENCH_HOST']!,
          package: Platform.environment['MORROW_WORKBENCH_PACKAGE']!,
          directory: fixture.directory,
          managed: true,
        );
        try {
          expect(await reopened.readUiLocale(), 'en');
        } finally {
          await reopened.close();
        }
      } finally {
        await fixture.close(observeBeforeClose: false);
      }
    },
    skip: !_available,
    timeout: const Timeout(Duration(minutes: 2)),
  );

  test(
    'selected certificate changed after inspection is refused by real host',
    () async {
      final fixture = await RealServiceFixture.open(tlsRequired: true);
      try {
        final pem = Directory('${fixture.directory.path}/tls');
        await _generate(pem);
        final cert = '${pem.path}/certificate.pem',
            key = '${pem.path}/private-key.pem';
        final selected = await fixture.backend.inspectServiceTls(
          certificatePath: cert,
          privateKeyPath: key,
        );
        await _generate(pem);
        await expectLater(
          fixture.backend.startServiceRun(fixture.request(tls: selected)),
          throwsA(isA<ServiceRunStartFailure>()),
        );
        expect((await fixture.backend.ioStatus()).key, isNull);
        final fresh = await fixture.backend.inspectServiceTls(
          certificatePath: cert,
          privateKeyPath: key,
        );
        expect(
          fresh.certificateSha256,
          isNot(orderedEquals(selected.certificateSha256)),
        );
        await fixture.session.start(fixture.request(tls: fresh));
        await fixture.observe(ServiceRunPhase.running);
        expect(await _post(fixture, cert), endsWith('executed-before'));
      } finally {
        await fixture.close();
      }
    },
    skip: !_available,
    timeout: const Timeout(Duration(minutes: 2)),
  );
}
