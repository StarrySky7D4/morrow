import 'dart:typed_data';

import 'generated/host.capnp.dart' as host;
import 'service_control.dart';

/// Private native transport only. The platform-neutral models never import it.
abstract final class ServiceCodec {
  static Uint8List _data(Uint8List? value) => value ?? Uint8List(0);
  static void _noToken(host.ResponseReader value) {
    final token = value.issuedToken;
    if (token == null) return;
    try {
      if (token.isNotEmpty) {
        throw const FormatException('Unexpected service token');
      }
    } finally {
      token.fillRange(0, token.length, 0);
    }
  }

  static void _cursor(Uint8List? after, Uint8List? snapshot) {
    if (after != null) ServiceValidation.digest(after);
    if (snapshot != null) ServiceValidation.digest(snapshot);
    if (after != null && snapshot == null) {
      throw const FormatException('Service snapshot required');
    }
  }

  static void writeConfigPage(
    host.RequestBuilder out, {
    String? after,
    Uint8List? snapshot,
  }) {
    if (after != null) ServiceValidation.identity(after);
    if (snapshot != null) ServiceValidation.digest(snapshot);
    if (after != null && snapshot == null) {
      throw const FormatException('Service snapshot required');
    }
    if (after != null) out.cursor = after;
    if (snapshot != null) out.serviceSnapshot = snapshot;
  }

  static void writeAuthorityPage(
    host.RequestBuilder out, {
    Uint8List? after,
    Uint8List? snapshot,
  }) {
    _cursor(after, snapshot);
    if (after != null) out.serviceCursor = after;
    if (snapshot != null) out.serviceSnapshot = snapshot;
  }

  static void _writePrincipal(
    ServicePrincipal value,
    host.ServicePrincipalBuilder out,
  ) {
    out.id = value.id;
    out.authenticationReference = value.authenticationReference;
    final scopes = out.initScopes(value.scopes.length);
    for (var i = 0; i < value.scopes.length; i++) {
      final s = value.scopes[i], target = scopes[i];
      target.kind = s.kind;
      target.cardId = s.cardId;
      target.attachmentId = s.attachmentId;
    }
  }

  static void writeConfig(
    ServiceConfigUpdate value,
    host.ServiceConfigUpdateBuilder out,
  ) {
    ServiceValidation.configUpdate(value);
    out.id = value.id;
    out.expectedRevisionBigInt = value.expectedRevision;
    out.registryRevisionBigInt = value.registryRevision;
    out.packageId = value.packageId;
    out.packageDigest = value.packageDigest;
    out.service = value.service;
    out.handler = value.handler;
    out.retentionMsBigInt = value.retentionMs;
    final principals = out.initPrincipals(value.principals.length);
    for (var i = 0; i < value.principals.length; i++) {
      _writePrincipal(value.principals[i], principals[i]);
    }
  }

  static void writeConfigDisable(
    String id,
    BigInt revision,
    host.RequestBuilder out,
  ) {
    ServiceValidation.identity(id);
    ServiceValidation.revision(revision);
    out.id = id;
    out.revisionBigInt = revision;
  }

  static void writeAuthentication(
    host.RequestBuilder out, {
    required Uint8List reference,
    required BigInt expectedRevision,
    required String principalId,
    required int lifetimeDays,
  }) {
    ServiceValidation.referenceRevision(
      reference,
      expectedRevision,
      create: true,
    );
    ServiceValidation.revision(expectedRevision, zero: true, updating: true);
    ServiceValidation.principalId(principalId);
    ServiceValidation.days(lifetimeDays);
    out.serviceReference = reference;
    out.revisionBigInt = expectedRevision;
    out.principalId = principalId;
    out.serviceDays = lifetimeDays;
  }

  static void writeAuthorityDisable(
    Uint8List reference,
    BigInt revision,
    host.RequestBuilder out,
  ) {
    ServiceValidation.referenceRevision(reference, revision);
    out.serviceReference = reference;
    out.revisionBigInt = revision;
  }

  static void writePublication(
    ServicePublicationUpdate value,
    host.ServicePublicationUpdateBuilder out,
  ) {
    ServiceValidation.publicationUpdate(value);
    out.reference = value.reference;
    out.expectedRevisionBigInt = value.expectedRevision;
    out.configRevisionBigInt = value.configRevision;
    out.registryRevisionBigInt = value.registryRevision;
    out.packageId = value.packageId;
    out.lifetimeDays = value.lifetimeDays;
    final p = out.initPolicy(), v = value.policy;
    p.configId = v.configId;
    p.configDigest = v.configDigest;
    p.listenAddress = v.listenAddress;
    p.tlsRequired = v.tlsRequired;
    p.method = v.method;
    p.path = v.path;
    p.queryPath = v.queryPath;
  }

  static List<ServicePrincipal> _principals(
    Iterable<host.ServicePrincipalReader>? rows,
    int length,
  ) {
    if (length > 64) throw const FormatException('Too many service principals');
    var total = 0;
    final values = <ServicePrincipal>[];
    for (final row in rows ?? <host.ServicePrincipalReader>[]) {
      final scopes = row.scopes;
      total += scopes?.length ?? 0;
      if (total > 128) throw const FormatException('Too many service scopes');
      values.add(
        ServicePrincipal(
          id: row.id ?? '',
          authenticationReference: _data(row.authenticationReference),
          scopes: [
            for (final s in scopes ?? <host.ServiceContentScopeReader>[])
              ServiceContentScope(
                kind: s.kind,
                cardId: s.cardId ?? '',
                attachmentId: s.attachmentId ?? '',
              ),
          ],
        ),
      );
    }
    ServiceValidation.principals(values);
    return values;
  }

  static StoredServiceConfig config(host.ServiceConfigInfoReader row) {
    final principals = row.principals, refs = row.approvalReferences;
    if ((refs?.length ?? 0) > 64) {
      throw const FormatException('Too many service approvals');
    }
    final value = StoredServiceConfig(
      id: row.id ?? '',
      revision: row.revisionBigInt,
      namespace: _data(row.namespace),
      retentionMs: row.retentionMsBigInt,
      service: row.service ?? '',
      handler: row.handler ?? '',
      packageDigest: _data(row.packageDigest),
      disabled: row.disabled,
      principals: _principals(principals, principals?.length ?? 0),
      approvalReferences: [
        for (final ref in refs ?? <Uint8List?>[]) _data(ref),
      ],
      digest: _data(row.digest),
    );
    ServiceValidation.storedConfig(value);
    return value;
  }

  static ServicePublication publication(host.ServicePublicationReader row) {
    final value = ServicePublication(
      configId: row.configId ?? '',
      configDigest: _data(row.configDigest),
      listenAddress: row.listenAddress ?? '',
      tlsRequired: row.tlsRequired,
      method: row.method ?? '',
      path: row.path ?? '',
      queryPath: row.queryPath ?? '',
    );
    ServiceValidation.publication(value);
    return value;
  }

  static StoredServiceAuthority authority(host.ServiceAuthorityInfoReader row) {
    final policy = row.publication;
    final value = StoredServiceAuthority(
      reference: _data(row.reference),
      revision: row.revisionBigInt,
      createdMs: row.createdMsBigInt,
      expiresMs: row.expiresMsBigInt,
      disabled: row.disabled,
      kind: row.kind,
      principalId: row.principalId ?? '',
      publication: policy == null ? null : publication(policy),
    );
    ServiceValidation.authority(value);
    return value;
  }

  static ServiceConfigPage configPage(host.ResponseReader response) {
    _noToken(response);
    final rows = response.serviceConfigs;
    final snapshot = _data(response.serviceSnapshot),
        next = response.cursor ?? '';
    ServiceValidation.digest(snapshot);
    if (rows == null ||
        rows.length > 1 ||
        (response.serviceAuthorities?.length ?? 0) != 0 ||
        _data(response.serviceCursor).isNotEmpty) {
      throw const FormatException('Invalid service configuration page');
    }
    final values = [for (final row in rows) config(row)];
    if (next.isNotEmpty && (values.length != 1 || next != values.single.id)) {
      throw const FormatException('Invalid service configuration cursor');
    }
    return ServiceConfigPage(
      configs: values,
      snapshot: snapshot,
      next: next.isEmpty ? null : next,
    );
  }

  static StoredServiceConfig configResult(host.ResponseReader response) {
    _noToken(response);
    final rows = response.serviceConfigs;
    if (rows == null ||
        rows.length != 1 ||
        (response.serviceAuthorities?.length ?? 0) != 0) {
      throw const FormatException('Missing service configuration result');
    }
    return config(rows.single);
  }

  static ServiceAuthorityPage authorityPage(host.ResponseReader response) {
    _noToken(response);
    final rows = response.serviceAuthorities;
    final snapshot = _data(response.serviceSnapshot),
        next = _data(response.serviceCursor);
    ServiceValidation.digest(snapshot);
    if (rows == null ||
        rows.length > 2 ||
        (response.serviceConfigs?.length ?? 0) != 0 ||
        (response.cursor ?? '').isNotEmpty) {
      throw const FormatException('Invalid service authority page');
    }
    final values = [for (final row in rows) authority(row)];
    for (var i = 1; i < values.length; i++) {
      if (ServiceValidation.compareBytes(
            values[i - 1].reference,
            values[i].reference,
          ) >=
          0) {
        throw const FormatException('Service authority order');
      }
    }
    if (next.isNotEmpty) {
      ServiceValidation.digest(next);
      if (values.isEmpty ||
          ServiceValidation.compareBytes(next, values.last.reference) != 0) {
        throw const FormatException('Invalid service authority cursor');
      }
    }
    return ServiceAuthorityPage(
      records: values,
      snapshot: snapshot,
      next: next.isEmpty ? null : next,
    );
  }

  static StoredServiceAuthority _authorityResult(host.ResponseReader response) {
    final rows = response.serviceAuthorities;
    if (rows == null ||
        rows.length != 1 ||
        (response.serviceConfigs?.length ?? 0) != 0) {
      throw const FormatException('Missing service authority result');
    }
    return authority(rows.single);
  }

  static StoredServiceAuthority authorityResult(host.ResponseReader response) {
    _noToken(response);
    return _authorityResult(response);
  }

  static StoredServiceAuthority publicationResult(
    host.ResponseReader response,
  ) {
    final value = authorityResult(response);
    if (value.kind != 2) {
      throw const FormatException('Invalid service publication result');
    }
    return value;
  }

  static IssuedServiceAuthentication issuedAuthentication(
    host.ResponseReader response,
  ) {
    // getDataField allocates a copy in capnproto_dart. Clear that temporary on
    // success and every validation failure; the owner also wipes the source frame.
    final raw = _data(response.issuedToken);
    try {
      final value = _authorityResult(response);
      if (value.kind != 1 || value.disabled) {
        throw const FormatException('Invalid issued authentication');
      }
      final token = IssuedToken(raw);
      return IssuedServiceAuthentication(authority: value, token: token);
    } finally {
      raw.fillRange(0, raw.length, 0);
    }
  }
}
