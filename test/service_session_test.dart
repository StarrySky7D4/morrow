import 'dart:async';
import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/service_control.dart';
import 'package:morrow_studio/plugins/service_session.dart';

import 'service_manager_fakes.dart';

Future<ServiceSession> ready(ServiceFakeBackend backend) async {
  final session = ServiceSession(backend)..attach();
  await session.refresh();
  expect(session.trusted, isTrue);
  return session;
}

Future<bool> issue(
  ServiceSession session, {
  Uint8List? reference,
  BigInt? revision,
}) => session.issueAuthentication(
  reference: reference ?? Uint8List(0),
  expectedRevision: revision ?? BigInt.zero,
  principalId: 'alice',
  lifetimeDays: 1,
);

void main() {
  test(
    'backend identity owns independent sessions across detach and remount',
    () async {
      final a = ServiceFakeBackend(), b = ServiceFakeBackend();
      final first = ServiceSession.forBackend(a),
          second = ServiceSession.forBackend(b);
      expect(identical(first, ServiceSession.forBackend(a)), isTrue);
      expect(identical(first, second), isFalse);
      first.attach();
      second.attach();
      await first.refresh();
      await second.refresh();
      a.onSave = (_) async => throw StateError('Reply lost after dispatch');
      expect(await first.saveConfig(serviceUpdate()), isFalse);
      expect(first.uncertain, isTrue);
      expect(second.uncertain, isFalse);
      first.detach();
      final remounted = ServiceSession.forBackend(a)..attach();
      expect(remounted.uncertain, isTrue);
      expect(remounted.canWrite, isFalse);
      expect(a.saves, hasLength(1));
      expect(b.writes, 0);
      first.detach();
      second.detach();
    },
  );

  test(
    'refresh reads all bounded pages with exact snapshots and immutable views',
    () async {
      final backend = ServiceFakeBackend()
        ..configs = List.generate(
          3,
          (i) => serviceConfig(id: 'service-${i + 1}', identity: i + 1),
        )
        ..authorities = List.generate(
          5,
          (i) => serviceAuthority(identity: i + 1),
        );
      final session = await ready(backend);
      expect(session.configs.map((c) => c.id), [
        'service-1',
        'service-2',
        'service-3',
      ]);
      expect(session.authorities, hasLength(5));
      expect(backend.configPages.map((p) => p.after), [
        null,
        'service-1',
        'service-2',
      ]);
      expect(backend.authorityPages.map((p) => p.after), [
        null,
        serviceKey(2),
        serviceKey(4),
      ]);
      for (final call in backend.configPages.skip(1)) {
        expect(call.snapshot, serviceKey(500));
      }
      for (final call in backend.authorityPages.skip(1)) {
        expect(call.snapshot, serviceKey(500));
      }
      expect(() => session.configs.clear(), throwsUnsupportedError);
      expect(() => session.authorities.clear(), throwsUnsupportedError);
      expect(backend.writes, 0);
      expect(session.canWrite, isTrue);
      session.detach();
    },
  );

  test(
    'read failures never publish partial replacements or restore trust',
    () async {
      for (var caseId = 0; caseId < 6; caseId++) {
        final backend = ServiceFakeBackend()
          ..configs = [serviceConfig()]
          ..authorities = [serviceAuthority()];
        final session = await ready(backend);
        final oldConfigs = [...session.configs],
            oldAuthorities = [...session.authorities];
        var reads = 0;
        backend.onConfigPage = (after, snapshot) async {
          reads++;
          if (caseId == 0) throw StateError('Busy');
          final c = serviceConfig(
            id: 'replacement-${reads.toString().padLeft(3, '0')}',
            identity: reads + 10,
          );
          if (caseId == 1) {
            return ServiceConfigPage(
              configs: [c, c],
              snapshot: serviceKey(501),
            );
          }
          if (caseId == 2) {
            return ServiceConfigPage(
              configs: [c],
              snapshot: serviceKey(501),
              next: 'unknown-cursor',
            );
          }
          if (caseId == 3) {
            return ServiceConfigPage(
              configs: [serviceConfig(id: 'same')],
              snapshot: serviceKey(501),
              next: 'same',
            );
          }
          if (caseId == 4) {
            return ServiceConfigPage(
              configs: [c],
              snapshot: serviceKey(reads == 1 ? 501 : 502),
              next: reads == 1 ? c.id : null,
            );
          }
          return ServiceConfigPage(configs: [c], snapshot: serviceKey(501));
        };
        backend.onAuthorityPage = (_, _) async =>
            throw StateError('Authority page failed');
        await session.refresh();
        expect(session.trusted, isFalse, reason: 'case $caseId');
        expect(session.canWrite, isFalse);
        expect(session.busy, isFalse);
        expect(session.configs, oldConfigs);
        expect(session.authorities, oldAuthorities);
        expect(session.notice, ServiceNotice.loadFailed);
        expect(reads, lessThanOrEqualTo(2));
        expect(backend.writes, 0);
        session.detach();
      }
    },
  );

  test(
    'authority snapshot drift ordering loops and per-page overflow fail closed',
    () async {
      for (var caseId = 0; caseId < 5; caseId++) {
        final backend = ServiceFakeBackend();
        final session = await ready(backend);
        var reads = 0;
        backend.onAuthorityPage = (after, snapshot) async {
          reads++;
          if (caseId == 0) {
            return ServiceAuthorityPage(
              records: List.generate(
                3,
                (i) => serviceAuthority(identity: i + 1),
              ),
              snapshot: serviceKey(1),
            );
          }
          if (caseId == 1) {
            return ServiceAuthorityPage(
              records: [
                serviceAuthority(identity: 2),
                serviceAuthority(identity: 1),
              ],
              snapshot: serviceKey(1),
            );
          }
          if (caseId == 2) {
            return ServiceAuthorityPage(
              records: [serviceAuthority(identity: 1)],
              snapshot: serviceKey(1),
              next: serviceKey(2),
            );
          }
          if (caseId == 3) {
            return ServiceAuthorityPage(
              records: [serviceAuthority(identity: 1)],
              snapshot: serviceKey(1),
              next: serviceKey(1),
            );
          }
          final identity = reads;
          return ServiceAuthorityPage(
            records: [serviceAuthority(identity: identity)],
            snapshot: serviceKey(reads),
            next: reads == 1 ? serviceKey(identity) : null,
          );
        };
        await session.refresh();
        expect(session.trusted, isFalse, reason: 'case $caseId');
        expect(session.authorities, isEmpty);
        expect(reads, lessThanOrEqualTo(2));
        expect(session.busy, isFalse);
        session.detach();
      }
    },
  );

  test(
    'full table ceilings stop endless streams without partial publication',
    () async {
      for (final configOverflow in [true, false]) {
        final backend = ServiceFakeBackend();
        final session = await ready(backend);
        var count = 0;
        if (configOverflow) {
          backend.onConfigPage = (_, _) async {
            final index = ++count;
            final c = serviceConfig(
              id: 'node-${index.toString().padLeft(4, '0')}',
              identity: index,
            );
            return ServiceConfigPage(
              configs: [c],
              snapshot: serviceKey(1),
              next: c.id,
            );
          };
        } else {
          backend.onAuthorityPage = (_, _) async {
            final index = ++count * 2;
            return ServiceAuthorityPage(
              records: [
                serviceAuthority(identity: index - 1),
                serviceAuthority(identity: index),
              ],
              snapshot: serviceKey(1),
              next: serviceKey(index),
            );
          };
        }
        await session.refresh();
        expect(session.trusted, isFalse);
        expect(session.configs, isEmpty);
        expect(session.authorities, isEmpty);
        expect(count, lessThanOrEqualTo(configOverflow ? 129 : 257));
        expect(session.busy, isFalse);
        session.detach();
      }
    },
  );

  test(
    'invalid writes fail locally without uncertainty or backend calls',
    () async {
      final backend = ServiceFakeBackend();
      final session = await ready(backend);
      expect(await session.saveConfig(serviceUpdate(principals: [])), isFalse);
      expect(session.notice, ServiceNotice.invalid);
      expect(
        await session.issueAuthentication(
          reference: Uint8List(0),
          expectedRevision: BigInt.zero,
          principalId: 'bad id',
          lifetimeDays: 1,
        ),
        isFalse,
      );
      expect(
        await session.issueAuthentication(
          reference: serviceKey(1),
          expectedRevision: BigInt.zero,
          principalId: 'alice',
          lifetimeDays: 1,
        ),
        isFalse,
      );
      expect(
        await session.disableConfig(serviceConfig(revision: BigInt.zero)),
        isFalse,
      );
      expect(
        await session.disableAuthority(serviceAuthority(revision: BigInt.zero)),
        isFalse,
      );
      expect(
        await session.savePublication(
          servicePublicationUpdate(
            expectedRevision: ServiceValidation.maxRevision,
          ),
        ),
        isFalse,
      );
      expect(backend.writes, 0);
      expect(session.uncertain, isFalse);
      expect(session.canWrite, isTrue);
      session.detach();
    },
  );

  test(
    'pending write rejects duplicate clicks and concurrent refresh admission',
    () async {
      final backend = ServiceFakeBackend();
      final session = await ready(backend);
      final gate = Completer<StoredServiceConfig>();
      backend.onSave = (_) => gate.future;
      final first = session.saveConfig(serviceUpdate());
      expect(session.busy, isTrue);
      expect(session.canWrite, isFalse);
      expect(await session.saveConfig(serviceUpdate()), isFalse);
      expect(await issue(session), isFalse);
      final reads = backend.configPages.length;
      await session.refresh();
      expect(backend.configPages.length, reads);
      final saved = serviceConfig(id: 'service-new');
      backend.configs = [saved];
      gate.complete(saved);
      expect(await first, isTrue);
      expect(backend.saves, hasLength(1));
      expect(backend.issues, isEmpty);
      expect(session.busy, isFalse);
      expect(session.uncertain, isFalse);
      expect(session.configs.single.id, 'service-new');
      session.detach();
    },
  );

  test(
    'lost receipt persists across reads and needs explicit trusted acknowledgement',
    () async {
      final backend = ServiceFakeBackend();
      final session = await ready(backend);
      backend.onSave = (_) async => throw StateError('Lost reply');
      expect(await session.saveConfig(serviceUpdate()), isFalse);
      expect(session.notice, ServiceNotice.writeUnknown);
      expect(session.uncertain, isTrue);
      session.detach();
      session.attach();
      await session.refresh();
      expect(session.uncertain, isTrue);
      expect(session.canWrite, isFalse);
      expect(await session.saveConfig(serviceUpdate()), isFalse);
      expect(backend.saves, hasLength(1));
      backend.onConfigPage = (_, _) async => throw StateError('Busy');
      await session.refresh();
      session.acknowledgeUncertain();
      expect(session.uncertain, isTrue);
      backend.onConfigPage = null;
      await session.refresh();
      session.acknowledgeUncertain();
      expect(session.uncertain, isFalse);
      expect(session.canWrite, isTrue);
      expect(backend.saves, hasLength(1));
      session.detach();
    },
  );

  test(
    'mismatched configuration receipts cannot become a confirmed write',
    () async {
      for (var caseId = 0; caseId < 5; caseId++) {
        final old = serviceConfig();
        final backend = ServiceFakeBackend()..configs = [old];
        final session = await ready(backend);
        backend.onSave = (_) async => serviceConfig(
          id: caseId == 0 ? 'different' : old.id,
          revision: BigInt.from(caseId == 1 ? 1 : 2),
          identity: caseId == 2 ? 2 : 1,
          disabled: caseId == 3,
          service: caseId == 4 ? 'different.service' : old.service,
        );
        expect(
          await session.saveConfig(
            serviceUpdate(id: old.id, expectedRevision: old.revision),
          ),
          isFalse,
          reason: 'case $caseId',
        );
        expect(session.uncertain, isTrue);
        expect(session.canWrite, isFalse);
        expect(backend.saves, hasLength(1));
        session.detach();
      }
    },
  );

  test(
    'mismatched token receipts are cleared and block automatic replay',
    () async {
      for (var caseId = 0; caseId < 3; caseId++) {
        final backend = ServiceFakeBackend()
          ..authorities = [serviceAuthority()];
        final session = await ready(backend);
        final bad = serviceIssued(
          authority: serviceAuthority(
            identity: caseId == 0 ? 99 : 2,
            revision: BigInt.from(caseId == 1 ? 1 : 2),
            principalId: caseId == 2 ? 'bob' : 'alice',
          ),
        );
        backend.onIssue = (_) async => bad;
        expect(
          await issue(session, reference: serviceKey(2), revision: BigInt.one),
          isFalse,
        );
        expect(session.uncertain, isTrue);
        expect(session.issued, isNull);
        expect(bad.token.isDisposed, isTrue);
        expect(bad.token.bytes, everyElement(0));
        expect(await issue(session), isFalse);
        expect(backend.issues, hasLength(1));
        session.detach();
      }
    },
  );

  test(
    'late issued secret with no viewer is wiped and cannot leak to another backend',
    () async {
      final a = ServiceFakeBackend(), b = ServiceFakeBackend();
      final sessionA = await ready(a), sessionB = await ready(b);
      final gate = Completer<IssuedServiceAuthentication>();
      a.onIssue = (_) => gate.future;
      final pending = issue(sessionA);
      sessionA.detach();
      final token = serviceIssued();
      gate.complete(token);
      await pending;
      expect(token.token.bytes, everyElement(0));
      expect(token.token.isDisposed, isTrue);
      expect(sessionA.issued, isNull);
      expect(sessionB.issued, isNull);
      expect(b.writes, 0);
      sessionA.attach();
      expect(sessionA.issued, isNull);
      expect(a.issues, hasLength(1));
      sessionA.detach();
      sessionB.detach();
    },
  );

  test(
    'last viewer detach and clearToken wipe owned bytes without writes',
    () async {
      final backend = ServiceFakeBackend();
      final session = await ready(backend);
      session.attach();
      expect(await issue(session), isTrue);
      final first = session.issued!;
      session.detach();
      expect(first.token.isDisposed, isFalse);
      expect(session.issued, same(first));
      session.detach();
      expect(first.token.bytes, everyElement(0));
      expect(session.issued, isNull);
      session.attach();
      expect(await issue(session), isTrue);
      final second = session.issued!;
      session.clearToken();
      expect(second.token.isDisposed, isTrue);
      expect(second.token.bytes, everyElement(0));
      expect(session.issued, isNull);
      expect(backend.issues, hasLength(2));
      expect(backend.writes, 2);
      session.detach();
    },
  );

  test(
    'confirmed token survives refresh failure without being marked unknown',
    () async {
      final backend = ServiceFakeBackend();
      final session = await ready(backend);
      final issued = serviceIssued();
      backend.onIssue = (_) async {
        backend.onConfigPage = (_, _) async =>
            throw StateError('Read unavailable');
        return issued;
      };
      expect(await issue(session), isTrue);
      expect(session.issued, same(issued));
      expect(issued.token.isDisposed, isFalse);
      expect(session.uncertain, isFalse);
      expect(session.trusted, isFalse);
      expect(session.canWrite, isFalse);
      expect(backend.issues, hasLength(1));
      backend.onConfigPage = null;
      await session.refresh();
      expect(session.issued, same(issued));
      expect(backend.issues, hasLength(1));
      session.detach();
      expect(issued.token.isDisposed, isTrue);
    },
  );

  test(
    'exact maximum tables are complete without an extra continuation call',
    () async {
      final backend = ServiceFakeBackend()
        ..configs = List.generate(
          128,
          (i) => serviceConfig(
            id: 'node-${i.toString().padLeft(4, '0')}',
            identity: i + 1,
          ),
        )
        ..authorities = List.generate(
          512,
          (i) => serviceAuthority(identity: i + 1),
        );
      final session = await ready(backend);
      expect(session.configs, hasLength(128));
      expect(session.authorities, hasLength(512));
      expect(backend.configPages, hasLength(128));
      expect(backend.authorityPages, hasLength(256));
      expect(backend.writes, 0);
      session.detach();
    },
  );

  test(
    'remount before late issue reply cannot receive prior presentation secret',
    () async {
      final backend = ServiceFakeBackend();
      final session = await ready(backend);
      final gate = Completer<IssuedServiceAuthentication>();
      backend.onIssue = (_) => gate.future;
      final pending = issue(session);
      session.detach();
      session.attach();
      final issued = serviceIssued();
      gate.complete(issued);
      expect(await pending, isTrue);
      expect(session.issued, isNull);
      expect(issued.token.isDisposed, isTrue);
      expect(issued.token.bytes, everyElement(0));
      expect(session.notice, ServiceNotice.tokenDiscarded);
      expect(session.uncertain, isFalse);
      expect(backend.issues, hasLength(1));
      session.detach();
    },
  );

  test(
    'busy metadata inspection cannot prematurely acknowledge uncertain write',
    () async {
      final backend = ServiceFakeBackend();
      final session = await ready(backend);
      backend.onSave = (_) async => throw StateError('Lost reply');
      await session.saveConfig(serviceUpdate());
      final gate = Completer<ServiceConfigPage>();
      backend.onConfigPage = (_, _) => gate.future;
      final read = session.refresh();
      session.acknowledgeUncertain();
      expect(session.uncertain, isTrue);
      expect(session.canWrite, isFalse);
      gate.complete(ServiceConfigPage(configs: [], snapshot: serviceKey(500)));
      await read;
      expect(session.uncertain, isTrue);
      session.acknowledgeUncertain();
      expect(session.uncertain, isFalse);
      expect(backend.saves, hasLength(1));
      session.detach();
    },
  );

  test(
    'disable and publication reject acknowledged changes to another identity',
    () async {
      for (var caseId = 0; caseId < 3; caseId++) {
        final old = serviceConfig(), auth = serviceAuthority();
        final backend = ServiceFakeBackend()
          ..configs = [old]
          ..authorities = [auth];
        final session = await ready(backend);
        late bool success;
        if (caseId == 0) {
          backend.onDisableConfig = (_, _) async =>
              serviceConfig(id: 'wrong', revision: BigInt.two, disabled: true);
          success = await session.disableConfig(old);
        } else if (caseId == 1) {
          backend.onDisableAuthority = (_, _) async => serviceAuthority(
            identity: 99,
            revision: BigInt.two,
            disabled: true,
            createdMs: auth.createdMs,
            expiresMs: auth.expiresMs,
          );
          success = await session.disableAuthority(auth);
        } else {
          backend.onPublication = (_) async => serviceAuthority(
            identity: 10001,
            publication: servicePublication(configId: 'wrong'),
          );
          success = await session.savePublication(servicePublicationUpdate());
        }
        expect(success, isFalse, reason: 'case $caseId');
        expect(session.uncertain, isTrue);
        expect(session.canWrite, isFalse);
        expect(backend.writes, 1);
        session.detach();
      }
    },
  );

  test(
    'already disabled config preserves maximum revision on repeat',
    () async {
      final old = serviceConfig(
        disabled: true,
        revision: ServiceValidation.maxRevision,
      );
      final backend = ServiceFakeBackend()..configs = [old];
      final session = await ready(backend);
      expect(await session.disableConfig(old), isTrue);
      expect(session.configs.single.revision, old.revision);
      expect(session.configs.single.disabled, isTrue);
      expect(backend.configDisables, hasLength(1));
      expect(session.uncertain, isFalse);
      session.detach();
    },
  );

  test(
    'already disabled authority preserves maximum revision on repeat',
    () async {
      final old = serviceAuthority(
        disabled: true,
        revision: ServiceValidation.maxRevision,
      );
      final backend = ServiceFakeBackend()..authorities = [old];
      final session = await ready(backend);
      expect(await session.disableAuthority(old), isTrue);
      expect(session.authorities.single.revision, old.revision);
      expect(session.authorities.single.disabled, isTrue);
      expect(backend.authorityDisables, hasLength(1));
      expect(session.uncertain, isFalse);
      session.detach();
    },
  );

  test(
    'disable and publication keep exact identity revision and read-only refresh',
    () async {
      final config = serviceConfig(), auth = serviceAuthority();
      final backend = ServiceFakeBackend()
        ..configs = [config]
        ..authorities = [auth];
      final session = await ready(backend);
      expect(await session.savePublication(servicePublicationUpdate()), isTrue);
      expect(backend.publications, hasLength(1));
      expect(session.authorities.where((a) => a.kind == 2), hasLength(1));
      expect(await session.disableAuthority(auth), isTrue);
      expect(backend.authorityDisables.single.reference, auth.reference);
      expect(backend.authorityDisables.single.revision, auth.revision);
      expect(await session.disableConfig(config), isTrue);
      expect(backend.configDisables.single.id, config.id);
      expect(backend.configDisables.single.revision, config.revision);
      expect(session.configs.single.disabled, isTrue);
      expect(session.uncertain, isFalse);
      expect(backend.writes, 3);
      session.detach();
    },
  );
}
