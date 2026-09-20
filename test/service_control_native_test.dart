import 'dart:io';
import 'dart:typed_data';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/service_control.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';
import 'external_plugin_native_test.dart'
    show entireCatalog, removeTestDirectory;

void main() {
  final exe = Platform.environment['MORROW_WORKBENCH_HOST'];
  final builtin = Platform.environment['MORROW_WORKBENCH_PACKAGE'];
  final fixture = Platform.environment['MORROW_SERVICE_ADMIN_PACKAGE'];
  final unavailable =
      !Platform.isWindows || exe == null || builtin == null || fixture == null;
  test(
    'private service administration persists policies, returns tokens once and never starts a listener',
    () async {
      final dir = await Directory.systemTemp.createTemp(
        'morrow-external-service-admin-',
      );
      RustWorkbench? backend;
      final occupied = await ServerSocket.bind(InternetAddress.loopbackIPv4, 0);
      Future<RustWorkbench> open() => RustWorkbench.open(
        executable: exe!,
        package: builtin!,
        directory: dir,
        managed: true,
      );
      try {
        backend = await open();
        final candidate = (await backend.inspectPlugin(
          fixture!,
        )).entries.single;
        await backend.importPlugin(
          fixture,
          candidate.digest,
          (await backend.pluginPage()).revision,
        );
        var catalog = await entireCatalog(backend);
        var entry = catalog.entries.singleWhere((e) => e.id == candidate.id);
        await backend.configureExternalIo(entry, catalog.revision, [
          'http-listen',
          'http-publish',
        ]);
        catalog = await entireCatalog(backend);
        entry = catalog.entries.singleWhere((e) => e.id == candidate.id);
        expect(entry.enabled, isFalse);
        final issued = await backend.issueServiceAuthentication(
          reference: Uint8List(0),
          expectedRevision: BigInt.zero,
          principalId: 'native-client',
          lifetimeDays: 1,
        );
        final originalToken = Uint8List.fromList(issued.token.bytes);
        expect(originalToken.length, 64);
        expect(issued.token.bytes.any((v) => v != 0), isTrue);
        final auth = issued.authority;
        issued.dispose();
        expect(issued.token.bytes, everyElement(0));
        final principals = [
          ServicePrincipal(
            id: 'native-client',
            authenticationReference: auth.reference,
            scopes: [],
          ),
        ];
        ServiceConfigUpdate configRequest({StoredServiceConfig? previous}) =>
            ServiceConfigUpdate(
              id: previous?.id ?? '',
              expectedRevision: previous?.revision ?? BigInt.zero,
              registryRevision: catalog.revision,
              packageId: entry.id,
              packageDigest: entry.digest,
              service: 'native.service',
              handler: 'morrow.service.admin.test.v1',
              retentionMs: BigInt.from(previous == null ? 86400000 : 172800000),
              principals: principals,
            );
        final config = await backend.saveServiceConfig(configRequest());
        expect(config.revision, BigInt.one);
        expect(
          config.principals.single.authenticationReference,
          auth.reference,
        );
        expect(config.approvalReferences, hasLength(1));
        ServicePublicationUpdate publication(
          StoredServiceConfig expected,
          BigInt revision,
        ) => ServicePublicationUpdate(
          reference: expected.approvalReferences.single,
          expectedRevision: revision,
          configRevision: expected.revision,
          registryRevision: catalog.revision,
          packageId: entry.id,
          lifetimeDays: 1,
          policy: ServicePublication(
            configId: expected.id,
            configDigest: expected.digest,
            listenAddress: '127.0.0.1:${occupied.port}',
            tlsRequired: false,
            method: 'POST',
            path: '/invoke',
            queryPath: '/result',
          ),
        );
        final approval = await backend.saveServicePublication(
          publication(config, BigInt.zero),
        );
        expect(approval.expiresMs, auth.expiresMs);
        expect(approval.publication!.configDigest, config.digest);
        final rows = (await backend.serviceAuthorityPage()).records;
        expect(rows, hasLength(2));
        expect(
          rows.singleWhere((r) => r.kind == 1).principalId,
          'native-client',
        );
        expect(
          (await backend.serviceConfigPage()).configs.single.namespace,
          config.namespace,
        );
        await backend.close();
        backend = await open();
        expect(
          (await backend.serviceConfigPage()).configs.single.digest,
          config.digest,
        );
        expect(
          (await entireCatalog(
            backend,
          )).entries.singleWhere((e) => e.id == entry.id).enabled,
          isFalse,
        );
        final replacement = await backend.saveServiceConfig(
          configRequest(previous: config),
        );
        expect(replacement.namespace, config.namespace);
        expect(
          replacement.approvalReferences.single,
          config.approvalReferences.single,
        );
        expect(replacement.digest, isNot(config.digest));
        await expectLater(
          backend.saveServicePublication(
            publication(config, approval.revision),
          ),
          throwsStateError,
        );
        final current = await backend.saveServicePublication(
          publication(replacement, approval.revision),
        );
        expect(current.revision, BigInt.two);
        final rotated = await backend.issueServiceAuthentication(
          reference: auth.reference,
          expectedRevision: auth.revision,
          principalId: auth.principalId,
          lifetimeDays: 2,
        );
        expect(rotated.token.bytes, isNot(originalToken));
        expect(rotated.authority.reference, auth.reference);
        expect(rotated.authority.revision, BigInt.two);
        originalToken.fillRange(0, originalToken.length, 0);
        rotated.dispose();
        expect(
          (await backend.disableServiceAuthority(
            current.reference,
            current.revision,
          )).disabled,
          isTrue,
        );
        expect(
          (await backend.disableServiceAuthority(
            auth.reference,
            BigInt.two,
          )).disabled,
          isTrue,
        );
        expect(
          (await backend.disableServiceConfig(
            replacement.id,
            replacement.revision,
          )).disabled,
          isTrue,
        );
        await backend.close();
        backend = await open();
        expect(
          (await backend.serviceConfigPage()).configs.single.disabled,
          isTrue,
        );
        expect(
          (await backend.serviceAuthorityPage()).records.every(
            (r) => r.disabled,
          ),
          isTrue,
        );
      } finally {
        await backend?.close();
        await occupied.close();
        await removeTestDirectory(dir);
      }
    },
    skip: unavailable,
    timeout: const Timeout(Duration(minutes: 2)),
  );
}
