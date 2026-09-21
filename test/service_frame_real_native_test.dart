import 'dart:io';
import 'package:morrow_studio/plugins/host_request.dart';
import 'package:morrow_studio/plugins/generated/host.capnp.dart' as host;
import 'package:morrow_studio/plugins/service_codec_native.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/service_control.dart';
import 'package:morrow_studio/plugins/service_run_control.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';
import 'service_run_real_native_fixture.dart';

void main() {
  test(
    'large service configuration crosses segmented original-owner lane and reopens intact',
    () async {
      final fixture = await RealServiceFixture.open();
      try {
        await fixture.session.start(fixture.request(lifetime: 60000));
        final running = await fixture.observe(ServiceRunPhase.running);
        final principal = fixture.config.principals.single;
        final scopes = List.generate(
          128,
          (i) => ServiceContentScope(
            kind: 4,
            cardId: 'card-${i.toString().padLeft(3, '0')}-${'c' * 240}',
            attachmentId: 'asset-${i.toString().padLeft(3, '0')}-${'a' * 240}',
          ),
        );
        final update = ServiceConfigUpdate(
          id: '',
          expectedRevision: BigInt.zero,
          registryRevision: fixture.registry,
          packageId: fixture.plugin.id,
          packageDigest: fixture.plugin.digest,
          service: fixture.config.service,
          handler: fixture.config.handler,
          retentionMs: BigInt.from(30000),
          principals: [
            ServicePrincipal(
              id: principal.id,
              authenticationReference: principal.authenticationReference,
              scopes: scopes,
            ),
          ],
        );
        await sendHostRequest(
          host.Action.serviceConfigSave,
          configure: (r) =>
              ServiceCodec.writeConfig(update, r.initServiceConfig()),
          clearAfterSend: true,
          send: (bytes) async {
            expect(bytes.length, greaterThan(65536));
            expect(bytes.length, lessThanOrEqualTo(128 * 1024));
            print(
              'verified complete configuration request: ${bytes.length} bytes',
            );
          },
        );
        final saved = await fixture.backend.saveServiceConfig(update);
        expect(saved.revision, BigInt.one);
        expect(saved.principals.single.scopes.length, 128);
        expect(
          saved.principals.single.scopes.last.attachmentId,
          scopes.last.attachmentId,
        );
        final current = await fixture.backend.serviceRunStatus(
          key: running.task.key,
        );
        expect(current.task.key, running.task.key);
        expect(current.phase, ServiceRunPhase.running);
        expect(await fixture.post(), endsWith('executed-before'));
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
          var page = await reopened.serviceConfigPage();
          final all = [...page.configs];
          while (page.next != null) {
            page = await reopened.serviceConfigPage(
              after: page.next!,
              snapshot: page.snapshot,
            );
            all.addAll(page.configs);
          }
          final stored = all.singleWhere((c) => c.id == saved.id);
          expect(stored.revision, BigInt.one);
          expect(stored.digest, saved.digest);
          expect(stored.namespace, saved.namespace);
          expect(
            stored.principals.single.authenticationReference,
            principal.authenticationReference,
          );
          expect(
            stored.principals.single.scopes.map((s) => s.attachmentId),
            scopes.map((s) => s.attachmentId),
          );
          expect(
            stored.principals.single.scopes.map((s) => s.kind),
            scopes.map((s) => s.kind),
          );
          expect(
            stored.principals.single.scopes.map((s) => s.cardId),
            scopes.map((s) => s.cardId),
          );
        } finally {
          await reopened.close();
        }
      } finally {
        await fixture.close(observeBeforeClose: false);
      }
    },
    skip: !RealServiceFixture.available,
    timeout: const Timeout(Duration(minutes: 2)),
  );
}
