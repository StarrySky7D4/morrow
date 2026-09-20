import 'dart:io';
import 'dart:typed_data';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/endpoint_control.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';
import 'external_plugin_native_test.dart'
    show entireCatalog, removeTestDirectory;

void main() {
  final executable = Platform.environment['MORROW_WORKBENCH_HOST'];
  final builtin = Platform.environment['MORROW_WORKBENCH_PACKAGE'];
  final fixture = Platform.environment['MORROW_IO_CONTROL_FIXTURE'];
  final unavailable =
      !Platform.isWindows ||
      executable == null ||
      builtin == null ||
      fixture == null;
  test(
    'real native endpoint policies survive restart, reject stale changes, and never open a connection',
    () async {
      final directory = await Directory.systemTemp.createTemp(
        'morrow-external-endpoints-',
      );
      final server = await ServerSocket.bind(InternetAddress.loopbackIPv4, 0);
      var connections = 0;
      final listening = server.listen((socket) {
        connections++;
        socket.destroy();
      });
      RustWorkbench? backend;
      Future<RustWorkbench> open() => RustWorkbench.open(
        executable: executable!,
        package: builtin!,
        directory: directory,
        managed: true,
      );
      Future<List<StoredEndpoint>> all(RustWorkbench b) async {
        final entries = <StoredEndpoint>[];
        Uint8List? after, snapshot;
        final seen = <String>{};
        for (var i = 0; i < 512; i++) {
          final page = await b.endpointPage(after: after, snapshot: snapshot);
          expect(page.entries.length, lessThanOrEqualTo(2));
          snapshot ??= page.snapshot;
          expect(page.snapshot, snapshot);
          entries.addAll(page.entries);
          if (page.next == null) return entries;
          expect(seen.add(page.next!.join(',')), isTrue);
          after = page.next;
        }
        throw StateError('endpoint pagination did not terminate');
      }

      try {
        backend = await open();
        final preview = await backend.inspectPlugin(fixture!);
        final candidate = preview.entries.single;
        await backend.importPlugin(fixture, candidate.digest, preview.revision);
        var catalog = await entireCatalog(backend);
        var entry = catalog.entries.singleWhere((e) => e.id == candidate.id);
        await backend.configureExternalIo(entry, catalog.revision, [
          'http-request',
          'credential-use',
        ]);
        catalog = await entireCatalog(backend);
        entry = catalog.entries.singleWhere((e) => e.id == candidate.id);
        expect(entry.enabled, isFalse);
        final credential = await backend.saveCredential(
          reference: Uint8List(0),
          expectedRevision: BigInt.zero,
          headerName: 'authorization',
          headerValue: 'Bearer synthetic-endpoint-test',
          lifetimeDays: 3,
        );
        final policy = EndpointPolicy(
          packageId: entry.id,
          packageDigest: entry.digest,
          origin: 'http://127.0.0.1:${server.port}',
          profile: 2,
          methods: ['GET', 'POST'],
          credentialReference: credential.reference,
          rootCertificate: Uint8List(0),
          maxRequestBytes: 32768,
          maxResponseBytes: 49152,
          maxHeaderBytes: 8192,
          maxConcurrent: 2,
          timeoutMs: 9000,
          maxFrameBytes: 98304,
        );
        final saved = await backend.saveEndpoint(
          reference: Uint8List(0),
          expectedRevision: BigInt.zero,
          registryRevision: catalog.revision,
          lifetimeDays: 1,
          policy: policy,
        );
        expect(saved.policy.packageId, entry.id);
        expect(saved.policy.packageDigest, entry.digest);
        expect(saved.policy.origin, policy.origin);
        expect(saved.policy.profile, 2);
        expect(saved.policy.methods, ['GET', 'POST']);
        expect(saved.policy.credentialReference, credential.reference);
        expect(saved.policy.maxRequestBytes, 32768);
        expect(saved.policy.maxResponseBytes, 49152);
        expect(saved.policy.maxHeaderBytes, 8192);
        expect(saved.policy.maxConcurrent, 2);
        expect(saved.policy.timeoutMs, 9000);
        expect(saved.policy.maxFrameBytes, 98304);
        expect(saved.expiresMs - saved.createdMs, BigInt.from(86400000));
        for (var i = 0; i < 2; i++) {
          await backend.saveEndpoint(
            reference: Uint8List(0),
            expectedRevision: BigInt.zero,
            registryRevision: catalog.revision,
            lifetimeDays: 1,
            policy: policy,
          );
        }
        expect(await all(backend), hasLength(3));
        final snapshot = await backend.endpointPage();
        final updated = await backend.saveEndpoint(
          reference: saved.reference,
          expectedRevision: saved.revision,
          registryRevision: catalog.revision,
          lifetimeDays: 1,
          policy: policy,
        );
        expect(updated.revision, BigInt.two);
        await expectLater(
          backend.disableEndpoint(saved),
          throwsA(isA<StateError>()),
        );
        await expectLater(
          backend.endpointPage(
            after: snapshot.next,
            snapshot: snapshot.snapshot,
          ),
          throwsA(isA<StateError>()),
        );
        await expectLater(
          backend.saveEndpoint(
            reference: credential.reference,
            expectedRevision: credential.revision,
            registryRevision: catalog.revision,
            lifetimeDays: 1,
            policy: policy,
          ),
          throwsA(isA<StateError>()),
        );
        await expectLater(
          backend.saveEndpoint(
            reference: saved.reference,
            expectedRevision: updated.revision,
            registryRevision: (BigInt.one << 64) + catalog.revision,
            lifetimeDays: 1,
            policy: policy,
          ),
          throwsA(isA<FormatException>()),
        );
        await backend.close();
        backend = await open();
        final restored = (await all(backend)).singleWhere(
          (e) => e.reference.join(',') == saved.reference.join(','),
        );
        expect(restored.revision, BigInt.two);
        expect(restored.policy.credentialReference, credential.reference);
        expect(restored.policy.maxFrameBytes, policy.maxFrameBytes);
        catalog = await entireCatalog(backend);
        entry = catalog.entries.singleWhere((e) => e.id == candidate.id);
        expect(entry.enabled, isFalse);
        await backend.removeExternal(entry, catalog.revision);
        final disabled = await backend.disableEndpoint(restored);
        expect(disabled.disabled, isTrue);
        expect(disabled.revision, BigInt.from(3));
        await backend.close();
        backend = await open();
        expect(
          (await all(backend))
              .singleWhere(
                (e) => e.reference.join(',') == saved.reference.join(','),
              )
              .disabled,
          isTrue,
        );
        expect(connections, 0);
      } finally {
        await backend?.close();
        await listening.cancel();
        await server.close();
        await removeTestDirectory(directory);
      }
    },
    skip: unavailable,
    timeout: const Timeout(Duration(minutes: 3)),
  );
}
