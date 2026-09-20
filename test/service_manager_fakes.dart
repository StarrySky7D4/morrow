import 'dart:typed_data';

import 'package:morrow_studio/plugins/service_control.dart';

Uint8List serviceKey(int value) {
  final bytes = Uint8List(32);
  ByteData.sublistView(bytes).setUint32(0, value, Endian.big);
  return bytes;
}

ServicePrincipal servicePrincipal({String id = 'alice', int reference = 2}) =>
    ServicePrincipal(
      id: id,
      authenticationReference: serviceKey(reference),
      scopes: const [ServiceContentScope(kind: 7, cardId: 'card')],
    );

StoredServiceConfig serviceConfig({
  String id = 'service-001',
  BigInt? revision,
  bool disabled = false,
  int identity = 1,
  String service = 'example.service',
  String handler = 'invoke',
  List<ServicePrincipal>? principals,
}) => StoredServiceConfig(
  id: id,
  revision: revision ?? BigInt.one,
  namespace: serviceKey(identity),
  retentionMs: BigInt.from(1000),
  service: service,
  handler: handler,
  packageDigest: serviceKey(3),
  disabled: disabled,
  principals: principals ?? [servicePrincipal()],
  approvalReferences: [serviceKey(identity + 10000)],
  digest: serviceKey(identity + 20000),
);

ServiceConfigUpdate serviceUpdate({
  String id = '',
  BigInt? expectedRevision,
  BigInt? registryRevision,
  String service = 'example.service',
  String handler = 'invoke',
  List<ServicePrincipal>? principals,
}) => ServiceConfigUpdate(
  id: id,
  expectedRevision: expectedRevision ?? BigInt.zero,
  registryRevision: registryRevision ?? BigInt.one,
  packageId: 'example.package',
  packageDigest: serviceKey(3),
  service: service,
  handler: handler,
  retentionMs: BigInt.from(1000),
  principals: principals ?? [servicePrincipal()],
);

ServicePublication servicePublication({String configId = 'service-001'}) =>
    ServicePublication(
      configId: configId,
      configDigest: serviceKey(20001),
      listenAddress: '127.0.0.1:8080',
      tlsRequired: false,
      method: 'POST',
      path: '/api',
      queryPath: '/history',
    );

ServicePublicationUpdate servicePublicationUpdate({BigInt? expectedRevision}) =>
    ServicePublicationUpdate(
      reference: serviceKey(10001),
      expectedRevision: expectedRevision ?? BigInt.zero,
      configRevision: BigInt.one,
      registryRevision: BigInt.one,
      packageId: 'example.package',
      lifetimeDays: 1,
      policy: servicePublication(),
    );

StoredServiceAuthority serviceAuthority({
  int identity = 2,
  BigInt? revision,
  bool disabled = false,
  String principalId = 'alice',
  ServicePublication? publication,
  BigInt? createdMs,
  BigInt? expiresMs,
}) {
  final created =
      createdMs ?? BigInt.from(DateTime.now().millisecondsSinceEpoch);
  return StoredServiceAuthority(
    reference: serviceKey(identity),
    revision: revision ?? BigInt.one,
    createdMs: created,
    expiresMs: expiresMs ?? created + BigInt.from(86400000),
    disabled: disabled,
    kind: publication == null ? 1 : 2,
    principalId: publication == null ? principalId : '',
    publication: publication,
  );
}

IssuedServiceAuthentication serviceIssued({
  StoredServiceAuthority? authority,
  int tokenByte = 97,
}) => IssuedServiceAuthentication(
  authority: authority ?? serviceAuthority(identity: 1000),
  token: IssuedToken(Uint8List.fromList(List.filled(64, tokenByte))),
);

typedef ConfigPageCall = ({String? after, Uint8List? snapshot});
typedef AuthorityPageCall = ({Uint8List? after, Uint8List? snapshot});
typedef IssueCall = ({
  Uint8List reference,
  BigInt expectedRevision,
  String principalId,
  int lifetimeDays,
});

/// Records explicit requests and exposes gates/faults without implicit retries.
class ServiceFakeBackend implements WorkbenchServiceControl {
  List<StoredServiceConfig> configs = [];
  List<StoredServiceAuthority> authorities = [];
  final configPages = <ConfigPageCall>[];
  final authorityPages = <AuthorityPageCall>[];
  final saves = <ServiceConfigUpdate>[];
  final configDisables = <({String id, BigInt revision})>[];
  final issues = <IssueCall>[];
  final authorityDisables = <({Uint8List reference, BigInt revision})>[];
  final publications = <ServicePublicationUpdate>[];
  Future<ServiceConfigPage> Function(String? after, Uint8List? snapshot)?
  onConfigPage;
  Future<ServiceAuthorityPage> Function(Uint8List? after, Uint8List? snapshot)?
  onAuthorityPage;
  Future<StoredServiceConfig> Function(ServiceConfigUpdate update)? onSave;
  Future<StoredServiceConfig> Function(String id, BigInt revision)?
  onDisableConfig;
  Future<IssuedServiceAuthentication> Function(IssueCall call)? onIssue;
  Future<StoredServiceAuthority> Function(Uint8List reference, BigInt revision)?
  onDisableAuthority;
  Future<StoredServiceAuthority> Function(ServicePublicationUpdate update)?
  onPublication;
  int generation = 500;
  int get writes =>
      saves.length +
      configDisables.length +
      issues.length +
      authorityDisables.length +
      publications.length;

  bool _same(Uint8List a, Uint8List b) =>
      ServiceValidation.compareBytes(a, b) == 0;
  void _snapshot(Uint8List? expected) {
    if (expected != null && !_same(expected, serviceKey(generation))) {
      throw StateError('Snapshot changed');
    }
  }

  @override
  Future<ServiceConfigPage> serviceConfigPage({
    String? after,
    Uint8List? snapshot,
  }) async {
    configPages.add((after: after, snapshot: snapshot));
    if (onConfigPage != null) return onConfigPage!(after, snapshot);
    _snapshot(snapshot);
    final sorted = [...configs]..sort((a, b) => a.id.compareTo(b.id));
    final index = after == null
        ? 0
        : sorted.indexWhere((c) => c.id == after) + 1;
    if (after != null && index == 0) throw StateError('Unknown cursor');
    final values = sorted.skip(index).take(1).toList();
    return ServiceConfigPage(
      configs: values,
      snapshot: serviceKey(generation),
      next: index + values.length < sorted.length ? values.last.id : null,
    );
  }

  @override
  Future<ServiceAuthorityPage> serviceAuthorityPage({
    Uint8List? after,
    Uint8List? snapshot,
  }) async {
    authorityPages.add((after: after, snapshot: snapshot));
    if (onAuthorityPage != null) return onAuthorityPage!(after, snapshot);
    _snapshot(snapshot);
    final sorted = [
      ...authorities,
    ]..sort((a, b) => ServiceValidation.compareBytes(a.reference, b.reference));
    final index = after == null
        ? 0
        : sorted.indexWhere((a) => _same(a.reference, after)) + 1;
    if (after != null && index == 0) throw StateError('Unknown cursor');
    final values = sorted.skip(index).take(2).toList();
    return ServiceAuthorityPage(
      records: values,
      snapshot: serviceKey(generation),
      next: index + values.length < sorted.length
          ? values.last.reference
          : null,
    );
  }

  @override
  Future<StoredServiceConfig> saveServiceConfig(
    ServiceConfigUpdate update,
  ) async {
    saves.add(update);
    if (onSave != null) return onSave!(update);
    final old = configs.where((c) => c.id == update.id).firstOrNull;
    final value = StoredServiceConfig(
      id: update.id.isEmpty ? 'service-new' : update.id,
      revision: update.expectedRevision + BigInt.one,
      namespace: old?.namespace ?? serviceKey(999),
      retentionMs: update.retentionMs,
      service: update.service,
      handler: update.handler,
      packageDigest: update.packageDigest,
      disabled: false,
      principals: update.principals,
      approvalReferences: old?.approvalReferences ?? [serviceKey(10999)],
      digest: serviceKey(++generation),
    );
    configs = [...configs.where((c) => c.id != value.id), value];
    return value;
  }

  @override
  Future<StoredServiceConfig> disableServiceConfig(
    String id,
    BigInt expectedRevision,
  ) async {
    configDisables.add((id: id, revision: expectedRevision));
    if (onDisableConfig != null) return onDisableConfig!(id, expectedRevision);
    final old = configs.firstWhere((c) => c.id == id);
    if (old.disabled) return old;
    final value = StoredServiceConfig(
      id: id,
      revision: expectedRevision + BigInt.one,
      namespace: old.namespace,
      retentionMs: old.retentionMs,
      service: old.service,
      handler: old.handler,
      packageDigest: old.packageDigest,
      disabled: true,
      principals: old.principals,
      approvalReferences: old.approvalReferences,
      digest: serviceKey(++generation),
    );
    configs = [...configs.where((c) => c.id != id), value];
    return value;
  }

  @override
  Future<IssuedServiceAuthentication> issueServiceAuthentication({
    required Uint8List reference,
    required BigInt expectedRevision,
    required String principalId,
    required int lifetimeDays,
  }) async {
    final call = (
      reference: reference,
      expectedRevision: expectedRevision,
      principalId: principalId,
      lifetimeDays: lifetimeDays,
    );
    issues.add(call);
    if (onIssue != null) return onIssue!(call);
    final now = BigInt.from(DateTime.now().millisecondsSinceEpoch);
    final value = StoredServiceAuthority(
      reference: reference.isEmpty ? serviceKey(1000 + generation) : reference,
      revision: expectedRevision + BigInt.one,
      createdMs: now,
      expiresMs: now + BigInt.from(lifetimeDays * 86400000),
      disabled: false,
      kind: 1,
      principalId: principalId,
    );
    authorities = [
      ...authorities.where((a) => !_same(a.reference, value.reference)),
      value,
    ];
    generation++;
    return serviceIssued(authority: value);
  }

  @override
  Future<StoredServiceAuthority> disableServiceAuthority(
    Uint8List reference,
    BigInt expectedRevision,
  ) async {
    authorityDisables.add((reference: reference, revision: expectedRevision));
    if (onDisableAuthority != null) {
      return onDisableAuthority!(reference, expectedRevision);
    }
    final old = authorities.firstWhere((a) => _same(a.reference, reference));
    if (old.disabled) return old;
    final value = StoredServiceAuthority(
      reference: reference,
      revision: expectedRevision + BigInt.one,
      createdMs: old.createdMs,
      expiresMs: old.expiresMs,
      disabled: true,
      kind: old.kind,
      principalId: old.principalId,
      publication: old.publication,
    );
    authorities = [
      ...authorities.where((a) => !_same(a.reference, reference)),
      value,
    ];
    generation++;
    return value;
  }

  @override
  Future<StoredServiceAuthority> saveServicePublication(
    ServicePublicationUpdate update,
  ) async {
    publications.add(update);
    if (onPublication != null) return onPublication!(update);
    final now = BigInt.from(DateTime.now().millisecondsSinceEpoch);
    final value = StoredServiceAuthority(
      reference: update.reference,
      revision: update.expectedRevision + BigInt.one,
      createdMs: now,
      expiresMs: now + BigInt.from(update.lifetimeDays * 86400000),
      disabled: false,
      kind: 2,
      publication: update.policy,
    );
    authorities = [
      ...authorities.where((a) => !_same(a.reference, value.reference)),
      value,
    ];
    generation++;
    return value;
  }
}
