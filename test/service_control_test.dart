import 'dart:convert';
import 'dart:typed_data';

import 'package:capnproto_dart/capnproto_dart.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/generated/host.capnp.dart' as host;
import 'package:morrow_studio/plugins/host_request.dart';
import 'package:morrow_studio/plugins/service_codec_native.dart';
import 'package:morrow_studio/plugins/service_control.dart';

Uint8List key(int value) => Uint8List.fromList(List.filled(32, value));
ServicePrincipal principal({
  String id = 'alice',
  List<ServiceContentScope>? scopes,
  Uint8List? reference,
}) => ServicePrincipal(
  id: id,
  authenticationReference: reference ?? key(2),
  scopes:
      scopes ??
      [
        const ServiceContentScope(
          kind: 4,
          cardId: 'card',
          attachmentId: 'attachment',
        ),
      ],
);
ServiceConfigUpdate update({
  String id = '',
  BigInt? expected,
  BigInt? registry,
  BigInt? retention,
  List<ServicePrincipal>? principals,
}) => ServiceConfigUpdate(
  id: id,
  expectedRevision: expected ?? BigInt.zero,
  registryRevision: registry ?? BigInt.one,
  packageId: 'package.test',
  packageDigest: key(3),
  service: 'test.service',
  handler: 'invoke',
  retentionMs: retention ?? BigInt.from(1000),
  principals: principals ?? [principal()],
);
ServicePublication publication({
  String address = '127.0.0.1:8080',
  bool tls = false,
  String path = '/api',
  String query = '/history',
  String method = 'POST',
}) => ServicePublication(
  configId: 'service-test',
  configDigest: key(4),
  listenAddress: address,
  tlsRequired: tls,
  method: method,
  path: path,
  queryPath: query,
);
ServicePublicationUpdate publicationUpdate({
  BigInt? revision,
  BigInt? configRevision,
  BigInt? registry,
  int days = 1,
  ServicePublication? policy,
}) => ServicePublicationUpdate(
  reference: key(5),
  expectedRevision: revision ?? BigInt.zero,
  configRevision: configRevision ?? BigInt.one,
  registryRevision: registry ?? BigInt.one,
  packageId: 'package.test',
  lifetimeDays: days,
  policy: policy ?? publication(),
);
host.ResponseBuilder response() =>
    MessageBuilder().initRoot(host.responseFactory);
void fillConfig(
  host.ServiceConfigInfoBuilder out, {
  int principalCount = 1,
  int scopeCount = 1,
  int refs = 1,
}) {
  out.id = 'service-test';
  out.revision = 1;
  out.namespace = key(1);
  out.retentionMs = 1000;
  out.service = 'test.service';
  out.handler = 'invoke';
  out.packageDigest = key(3);
  out.digest = key(4);
  final principals = out.initPrincipals(principalCount);
  for (var i = 0; i < principalCount; i++) {
    final p = principals[i];
    p.id = 'principal-${i.toString().padLeft(3, '0')}';
    p.authenticationReference = key(2);
    final scopes = p.initScopes(scopeCount);
    for (var j = 0; j < scopeCount; j++) {
      scopes[j].kind = 7;
      scopes[j].cardId = 'card-${j.toString().padLeft(3, '0')}';
    }
  }
  final approvals = out.initApprovalReferences(refs);
  for (var i = 0; i < refs; i++) {
    approvals[i] = key(i + 1);
  }
}

void fillPublication(host.ServicePublicationBuilder out) {
  out.configId = 'service-test';
  out.configDigest = key(4);
  out.listenAddress = '127.0.0.1:8080';
  out.method = 'POST';
  out.path = '/api';
  out.queryPath = '/history';
}

void fillAuthority(
  host.ServiceAuthorityInfoBuilder out, {
  int kind = 1,
  int identity = 1,
}) {
  out.reference = key(identity);
  out.revision = 1;
  out.createdMs = 10;
  out.expiresMs = 1000;
  out.kind = kind;
  if (kind == 1) {
    out.principalId = 'alice';
  }
  if (kind == 2) {
    fillPublication(out.initPublication());
  }
}

void main() {
  test(
    'all seven service actions preserve exact revisions and original policy',
    () async {
      final exact = (BigInt.one << 53) + BigInt.one;
      final uint64 = (BigInt.one << 64) - BigInt.one;
      final actions = <host.Action, void Function(host.RequestBuilder)>{
        host.Action.serviceConfigPage: (r) => ServiceCodec.writeConfigPage(
          r,
          after: 'service-test',
          snapshot: key(1),
        ),
        host.Action.serviceConfigSave: (r) => ServiceCodec.writeConfig(
          update(id: 'service-test', expected: exact, registry: uint64),
          r.initServiceConfig(),
        ),
        host.Action.serviceConfigDisable: (r) =>
            ServiceCodec.writeConfigDisable('service-test', exact, r),
        host.Action.serviceAuthorityPage: (r) =>
            ServiceCodec.writeAuthorityPage(r, after: key(2), snapshot: key(1)),
        host.Action.serviceAuthenticationIssue: (r) =>
            ServiceCodec.writeAuthentication(
              r,
              reference: key(2),
              expectedRevision: exact,
              principalId: 'alice',
              lifetimeDays: 30,
            ),
        host.Action.serviceAuthorityDisable: (r) =>
            ServiceCodec.writeAuthorityDisable(key(2), exact, r),
        host.Action.servicePublicationSave: (r) =>
            ServiceCodec.writePublication(
              publicationUpdate(
                revision: exact,
                configRevision: exact,
                registry: uint64,
              ),
              r.initServicePublication(),
            ),
      };
      var sends = 0;
      for (final entry in actions.entries) {
        await sendHostRequest(
          entry.key,
          configure: entry.value,
          send: (bytes) async {
            sends++;
            final r = MessageReader.deserialize(
              bytes,
            ).getRoot(host.requestFactory);
            expect(r.action, entry.key);
            switch (entry.key) {
              case host.Action.serviceConfigPage:
                expect(r.cursor, 'service-test');
                expect(r.serviceSnapshot, key(1));
              case host.Action.serviceConfigSave:
                final p = r.serviceConfig!;
                expect(p.expectedRevisionBigInt, exact);
                expect(p.registryRevisionBigInt, uint64);
                expect(p.packageId, 'package.test');
                expect(p.packageDigest, key(3));
                expect(p.retentionMs, 1000);
                expect(p.principals!.single.id, 'alice');
                expect(
                  p.principals!.single.scopes!.single.attachmentId,
                  'attachment',
                );
              case host.Action.serviceConfigDisable:
                expect(r.id, 'service-test');
                expect(r.revisionBigInt, exact);
              case host.Action.serviceAuthorityPage:
                expect(r.serviceCursor, key(2));
                expect(r.serviceSnapshot, key(1));
              case host.Action.serviceAuthenticationIssue:
                expect(r.principalId, 'alice');
                expect(r.serviceDays, 30);
                expect(r.serviceReference, key(2));
                expect(r.revisionBigInt, exact);
              case host.Action.serviceAuthorityDisable:
                expect(r.serviceReference, key(2));
                expect(r.revisionBigInt, exact);
              case host.Action.servicePublicationSave:
                final p = r.servicePublication!;
                expect(p.reference, key(5));
                expect(p.expectedRevisionBigInt, exact);
                expect(p.configRevisionBigInt, exact);
                expect(p.registryRevisionBigInt, uint64);
                expect(p.policy!.configDigest, key(4));
                expect(p.policy!.queryPath, '/history');
              default:
                fail('Unexpected action');
            }
          },
        );
      }
      expect(sends, 7);
    },
  );

  test(
    'invalid scalar and identity input fails before transport send',
    () async {
      final bad = <ServiceConfigUpdate>[
        update(expected: BigInt.one),
        update(id: 'service-test'),
        update(id: '../outside', expected: BigInt.one),
        update(expected: BigInt.from(-1)),
        update(id: 'service-test', expected: ServiceValidation.maxRevision),
        update(registry: BigInt.one << 64),
        update(registry: BigInt.from(-1)),
        update(retention: BigInt.zero),
        update(retention: ServiceValidation.maxLifetimeMs + BigInt.one),
        update(principals: []),
      ];
      for (final value in bad) {
        var sends = 0;
        await expectLater(
          sendHostRequest(
            host.Action.serviceConfigSave,
            configure: (r) =>
                ServiceCodec.writeConfig(value, r.initServiceConfig()),
            send: (_) async {
              sends++;
            },
          ),
          throwsFormatException,
        );
        expect(sends, 0);
      }
      for (final value in ['', 'x/y', 'x\\y', 'x:y', 'x\u0085y', 'é' * 129]) {
        expect(() => ServiceValidation.identity(value), throwsFormatException);
      }
      ServiceValidation.identity('é' * 128);
    },
  );

  test('principals and scopes enforce total bounds and canonical order', () {
    final scopes = List.generate(
      128,
      (i) => ServiceContentScope(
        kind: 7,
        cardId: 'card-${i.toString().padLeft(3, '0')}',
      ),
    );
    ServiceValidation.principals([principal(scopes: scopes)]);
    for (final values in <List<ServicePrincipal>>[
      List.generate(
        65,
        (i) => principal(id: 'p-${i.toString().padLeft(3, '0')}', scopes: []),
      ),
      [principal(scopes: scopes), principal(id: 'bob')],
      [principal(), principal()],
      [
        principal(scopes: [const ServiceContentScope(kind: 8, cardId: 'card')]),
      ],
      [
        principal(scopes: [const ServiceContentScope(kind: 4, cardId: 'card')]),
      ],
      [
        principal(
          scopes: [
            const ServiceContentScope(
              kind: 1,
              cardId: 'card',
              attachmentId: 'invalid',
            ),
          ],
        ),
      ],
      [
        principal(
          scopes: [
            const ServiceContentScope(kind: 7, cardId: 'z'),
            const ServiceContentScope(kind: 7, cardId: 'a'),
          ],
        ),
      ],
      [principal(reference: Uint8List(32))],
    ]) {
      expect(() => ServiceValidation.principals(values), throwsFormatException);
    }
  });

  test('models own immutable references and lists before queue admission', () {
    final authRef = key(2),
        digest = key(3),
        scopeList = <ServiceContentScope>[];
    final p = principal(reference: authRef, scopes: scopeList);
    final principals = [p];
    final config = ServiceConfigUpdate(
      id: '',
      expectedRevision: BigInt.zero,
      registryRevision: BigInt.zero,
      packageId: 'package',
      packageDigest: digest,
      service: 'svc',
      handler: 'run',
      retentionMs: BigInt.one,
      principals: principals,
    );
    authRef.fillRange(0, 32, 0);
    digest.fillRange(0, 32, 0);
    principals.clear();
    scopeList.add(const ServiceContentScope(kind: 7, cardId: 'card'));
    expect(config.packageDigest, key(3));
    expect(config.principals.single.authenticationReference, key(2));
    expect(p.scopes, isEmpty);
    expect(() => config.packageDigest[0] = 0, throwsUnsupportedError);
    expect(() => config.principals.clear(), throwsUnsupportedError);
    expect(() => p.scopes.clear(), throwsUnsupportedError);
    final approvals = [key(5)];
    final row = response();
    fillConfig(row.initServiceConfigs(1)[0]);
    final stored = ServiceCodec.configResult(row.asReader());
    expect(
      () => stored.approvalReferences.single[0] = 0,
      throwsUnsupportedError,
    );
    expect(
      () => stored.approvalReferences.add(approvals.single),
      throwsUnsupportedError,
    );
  });

  test(
    'authentication create rotation and lifetime cannot truncate or change type',
    () {
      final r = MessageBuilder().initRoot(host.requestFactory);
      ServiceCodec.writeAuthentication(
        r,
        reference: Uint8List(0),
        expectedRevision: BigInt.zero,
        principalId: 'alice',
        lifetimeDays: 30,
      );
      for (final days in [0, 31, 1 << 32]) {
        expect(
          () => ServiceCodec.writeAuthentication(
            r,
            reference: Uint8List(0),
            expectedRevision: BigInt.zero,
            principalId: 'alice',
            lifetimeDays: days,
          ),
          throwsFormatException,
        );
      }
      for (final id in ['', 'alice bob', 'alice/other', 'é', 'a' * 129]) {
        expect(
          () => ServiceCodec.writeAuthentication(
            r,
            reference: Uint8List(0),
            expectedRevision: BigInt.zero,
            principalId: id,
            lifetimeDays: 1,
          ),
          throwsFormatException,
        );
      }
      expect(
        () => ServiceCodec.writeAuthentication(
          r,
          reference: key(1),
          expectedRevision: BigInt.zero,
          principalId: 'alice',
          lifetimeDays: 1,
        ),
        throwsFormatException,
      );
      expect(
        () => ServiceCodec.writeAuthentication(
          r,
          reference: Uint8List(0),
          expectedRevision: BigInt.one,
          principalId: 'alice',
          lifetimeDays: 1,
        ),
        throwsFormatException,
      );
      expect(
        () => ServiceCodec.writeAuthorityDisable(key(1), BigInt.one << 63, r),
        throwsFormatException,
      );
    },
  );

  test(
    'historical IPv6 numeric scopes survive listing and disable receipts',
    () {
      for (final address in ['[fe80::1%3]:443', '[fe80::1%4294967295]:443']) {
        final r = response();
        final a = r.initServiceAuthorities(1)[0];
        fillAuthority(a, kind: 2);
        a.disabled = true;
        final p = a.initPublication();
        fillPublication(p);
        p.listenAddress = address;
        p.tlsRequired = true;
        r.serviceSnapshot = key(7);
        final page = ServiceCodec.authorityPage(r.asReader());
        expect(page.records.single.publication!.listenAddress, address);
        expect(page.records.single.disabled, isTrue);
        final disabled = ServiceCodec.authorityResult(r.asReader());
        expect(disabled.publication!.listenAddress, address);
        expect(disabled.disabled, isTrue);
      }
      ServiceValidation.publication(publication(address: '[::1%3]:8080'));
      for (final address in [
        '[fe80::1%0]:443',
        '[fe80::1%03]:443',
        '[fe80::1%-1]:443',
        '[fe80::1%4294967296]:443',
        '[fe80::1%eth0]:443',
        '[fe80::1%]:443',
        '[fe80::1%%3]:443',
        '[fe80::1%3%4]:443',
        '[fe80:::1%3]:443',
        '127.0.0.1%3:443',
      ]) {
        expect(
          () => ServiceValidation.publication(
            publication(address: address, tls: true),
          ),
          throwsFormatException,
          reason: address,
        );
      }
      expect(
        () => ServiceValidation.publication(
          publication(address: '[fe80::1%3]:443'),
        ),
        throwsFormatException,
      );
    },
  );

  test('publication validates fixed paths TLS and exact revision domains', () {
    ServiceValidation.publication(publication(address: '[::1]:8080'));
    ServiceValidation.publication(
      publication(address: '0.0.0.0:443', tls: true),
    );
    for (final value in [
      publication(address: '0.0.0.0:8080'),
      publication(address: '127.0.0.1:65536'),
      publication(address: '127.0.0.1:08080'),
      publication(address: '127.0.0.999:80'),
      publication(address: 'example.com:80'),
      publication(path: '//other'),
      publication(path: '/api?query=1'),
      publication(path: '/api#hash'),
      publication(path: '/%GG'),
      publication(path: '/{id}'),
      publication(query: '/api'),
      publication(method: 'get'),
    ]) {
      expect(() => ServiceValidation.publication(value), throwsFormatException);
    }
    for (final value in [
      publicationUpdate(configRevision: BigInt.zero),
      publicationUpdate(revision: ServiceValidation.maxRevision),
      publicationUpdate(registry: BigInt.one << 64),
      publicationUpdate(days: 31),
    ]) {
      expect(
        () => ServiceValidation.publicationUpdate(value),
        throwsFormatException,
      );
    }
    ServiceValidation.publicationUpdate(
      publicationUpdate(),
    ); // Reserved ref + revision zero is intentional.
  });

  test(
    'configuration decode rejects oversized counts before iterating nested data',
    () {
      for (final counts in [(65, 0, 0), (2, 65, 0), (0, 0, 65)]) {
        final r = response();
        fillConfig(
          r.initServiceConfigs(1)[0],
          principalCount: counts.$1,
          scopeCount: counts.$2,
          refs: counts.$3,
        );
        expect(
          () => ServiceCodec.configResult(r.asReader()),
          throwsFormatException,
        );
      }
      final r = response();
      final c = r.initServiceConfigs(1)[0];
      fillConfig(c, refs: 64);
      c.revisionBigInt = ServiceValidation.maxRevision;
      expect(
        ServiceCodec.configResult(r.asReader()).revision,
        ServiceValidation.maxRevision,
      );
      c.revision = -1;
      expect(
        () => ServiceCodec.configResult(r.asReader()),
        throwsFormatException,
      );
    },
  );

  test('pages preserve historical metadata with exact known continuation', () {
    final configs = response();
    fillConfig(configs.initServiceConfigs(1)[0]);
    configs.serviceSnapshot = key(7);
    configs.cursor = 'service-test';
    final page = ServiceCodec.configPage(configs.asReader());
    expect(page.next, 'service-test');
    configs.cursor = 'another';
    expect(
      () => ServiceCodec.configPage(configs.asReader()),
      throwsFormatException,
    );
    configs.initServiceConfigs(2);
    expect(
      () => ServiceCodec.configPage(configs.asReader()),
      throwsFormatException,
    );
    final r = response();
    final values = r.initServiceAuthorities(2);
    fillAuthority(values[0], identity: 1);
    fillAuthority(values[1], kind: 2, identity: 2);
    values[0].disabled = true;
    r.serviceSnapshot = key(7);
    r.serviceCursor = key(2);
    final authorities = ServiceCodec.authorityPage(r.asReader());
    expect(authorities.records.first.disabled, isTrue);
    expect(authorities.records.last.publication!.path, '/api');
    expect(authorities.next, key(2));
    r.serviceCursor = key(3);
    expect(
      () => ServiceCodec.authorityPage(r.asReader()),
      throwsFormatException,
    );
    r.serviceCursor = key(2);
    values[0].reference = key(2);
    expect(
      () => ServiceCodec.authorityPage(r.asReader()),
      throwsFormatException,
    );
    r.initServiceAuthorities(3);
    expect(
      () => ServiceCodec.authorityPage(r.asReader()),
      throwsFormatException,
    );
  });

  test(
    'authority decode rejects mixed type invalid time and unsigned overflow',
    () {
      for (var i = 0; i < 7; i++) {
        final r = response();
        final a = r.initServiceAuthorities(1)[0];
        fillAuthority(a);
        switch (i) {
          case 0:
            a.kind = 3;
          case 1:
            a.expiresMs = 10;
          case 2:
            a.createdMs = 0;
          case 3:
            a.expiresMs = 2592000011;
          case 4:
            a.revision = -1;
          case 5:
            fillPublication(a.initPublication());
          case 6:
            a.kind = 2;
        }
        expect(
          () => ServiceCodec.authorityResult(r.asReader()),
          throwsFormatException,
          reason: 'case $i',
        );
      }
      final r = response();
      final a = r.initServiceAuthorities(1)[0];
      fillAuthority(a);
      a.createdMs = -1000;
      a.expiresMs = -1;
      expect(
        ServiceCodec.authorityResult(r.asReader()).expiresMs,
        ServiceValidation.maxUint64,
      );
    },
  );

  test(
    'issued token ownership survives frame wipe then dispose clears all views',
    () {
      final message = MessageBuilder();
      final r = message.initRoot(host.responseFactory);
      fillAuthority(r.initServiceAuthorities(1)[0]);
      r.issuedToken = Uint8List.fromList(utf8.encode('a' * 64));
      final frame = message.serialize();
      final reader = MessageReader.deserialize(
        frame,
      ).getRoot(host.responseFactory);
      final issued = ServiceCodec.issuedAuthentication(reader);
      final view = Uint8List.sublistView(issued.token.bytes);
      frame.fillRange(0, frame.length, 0);
      expect(issued.authority.reference, key(1));
      expect(view, everyElement(97));
      expect(issued.token.toString(), isNot(contains('a' * 64)));
      issued.dispose();
      expect(view, everyElement(0));
      expect(issued.token.isDisposed, isTrue);
      issued.dispose();
      expect(view, everyElement(0));
    },
  );

  test('only issue can expose a token and invalid issue never returns one', () {
    for (final decode in <Object Function(host.ResponseReader)>[
      ServiceCodec.configPage,
      ServiceCodec.configResult,
      ServiceCodec.authorityPage,
      ServiceCodec.authorityResult,
      ServiceCodec.publicationResult,
    ]) {
      final r = response();
      r.issuedToken = Uint8List.fromList(utf8.encode('a' * 64));
      expect(() => decode(r.asReader()), throwsFormatException);
    }
    for (var i = 0; i < 5; i++) {
      final r = response();
      final a = r.initServiceAuthorities(1)[0];
      fillAuthority(a, kind: i == 3 ? 2 : 1);
      r.issuedToken = Uint8List.fromList(
        utf8.encode(
          i == 0
              ? 'a' * 63
              : i == 1
              ? 'A' * 64
              : i == 2
              ? 'g' * 64
              : 'a' * 64,
        ),
      );
      if (i == 4) a.disabled = true;
      expect(
        () => ServiceCodec.issuedAuthentication(r.asReader()),
        throwsFormatException,
      );
    }
  });

  test(
    'continuation requires snapshot and invalid cursors never enter wire',
    () {
      final r = MessageBuilder().initRoot(host.requestFactory);
      expect(
        () => ServiceCodec.writeConfigPage(r, after: 'service-test'),
        throwsFormatException,
      );
      expect(
        () => ServiceCodec.writeAuthorityPage(r, after: key(1)),
        throwsFormatException,
      );
      expect(
        () => ServiceCodec.writeAuthorityPage(r, snapshot: Uint8List(32)),
        throwsFormatException,
      );
      expect(
        () => ServiceCodec.writeConfigPage(r, after: '', snapshot: key(1)),
        throwsFormatException,
      );
    },
  );
}
