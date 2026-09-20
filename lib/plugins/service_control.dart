import 'dart:convert';
import 'dart:typed_data';

Uint8List _owned(Uint8List value) =>
    Uint8List.fromList(value).asUnmodifiableView();

/// Desired service policy, never a running listener or restored authorization.
class ServiceContentScope {
  const ServiceContentScope({
    required this.kind,
    required this.cardId,
    this.attachmentId = '',
  });
  final int kind;
  final String cardId, attachmentId;
}

class ServicePrincipal {
  ServicePrincipal({
    required this.id,
    required Uint8List authenticationReference,
    required List<ServiceContentScope> scopes,
  }) : authenticationReference = _owned(authenticationReference),
       scopes = List.unmodifiable(scopes);
  final String id;
  final Uint8List authenticationReference;
  final List<ServiceContentScope> scopes;
}

class ServiceConfigUpdate {
  ServiceConfigUpdate({
    required this.id,
    required this.expectedRevision,
    required this.registryRevision,
    required this.packageId,
    required Uint8List packageDigest,
    required this.service,
    required this.handler,
    required this.retentionMs,
    required List<ServicePrincipal> principals,
  }) : packageDigest = _owned(packageDigest),
       principals = List.unmodifiable(principals);
  final String id, packageId, service, handler;
  final BigInt expectedRevision, registryRevision, retentionMs;
  final Uint8List packageDigest;
  final List<ServicePrincipal> principals;
}

class StoredServiceConfig {
  StoredServiceConfig({
    required this.id,
    required this.revision,
    required Uint8List namespace,
    required this.retentionMs,
    required this.service,
    required this.handler,
    required Uint8List packageDigest,
    required this.disabled,
    required List<ServicePrincipal> principals,
    required List<Uint8List> approvalReferences,
    required Uint8List digest,
  }) : namespace = _owned(namespace),
       packageDigest = _owned(packageDigest),
       principals = List.unmodifiable(principals),
       approvalReferences = List.unmodifiable(approvalReferences.map(_owned)),
       digest = _owned(digest);
  final String id, service, handler;
  final BigInt revision, retentionMs;
  final Uint8List namespace, packageDigest, digest;
  final bool disabled;
  final List<ServicePrincipal> principals;
  final List<Uint8List> approvalReferences;
}

class ServiceConfigPage {
  ServiceConfigPage({
    required List<StoredServiceConfig> configs,
    required Uint8List snapshot,
    this.next,
  }) : configs = List.unmodifiable(configs),
       snapshot = _owned(snapshot);
  final List<StoredServiceConfig> configs;
  final Uint8List snapshot;
  final String? next;
}

class ServicePublication {
  ServicePublication({
    required this.configId,
    required Uint8List configDigest,
    required this.listenAddress,
    required this.tlsRequired,
    required this.method,
    required this.path,
    this.queryPath = '',
  }) : configDigest = _owned(configDigest);
  final String configId, listenAddress, method, path, queryPath;
  final Uint8List configDigest;
  final bool tlsRequired;
}

class ServicePublicationUpdate {
  ServicePublicationUpdate({
    required Uint8List reference,
    required this.expectedRevision,
    required this.configRevision,
    required this.registryRevision,
    required this.packageId,
    required this.lifetimeDays,
    required this.policy,
  }) : reference = _owned(reference);
  final Uint8List reference;
  final BigInt expectedRevision, configRevision, registryRevision;
  final String packageId;
  final int lifetimeDays;
  final ServicePublication policy;
}

/// Redacted metadata only; authentication hashes and raw tokens are absent.
class StoredServiceAuthority {
  StoredServiceAuthority({
    required Uint8List reference,
    required this.revision,
    required this.createdMs,
    required this.expiresMs,
    required this.disabled,
    required this.kind,
    this.principalId = '',
    this.publication,
  }) : reference = _owned(reference);
  final Uint8List reference;
  final BigInt revision, createdMs, expiresMs;
  final bool disabled;

  /// 1 authentication, 2 publication. Other values fail decoding.
  final int kind;
  final String principalId;
  final ServicePublication? publication;
}

class ServiceAuthorityPage {
  ServiceAuthorityPage({
    required List<StoredServiceAuthority> records,
    required Uint8List snapshot,
    Uint8List? next,
  }) : records = List.unmodifiable(records),
       snapshot = _owned(snapshot),
       next = next == null ? null : _owned(next);
  final List<StoredServiceAuthority> records;
  final Uint8List snapshot;
  final Uint8List? next;
}

/// A one-time secret with explicit ownership. Do not turn it into a cached String.
/// Disposing clears this owned allocation, including views retained by the caller.
class IssuedToken {
  IssuedToken(Uint8List value) : bytes = _copy(value);
  final Uint8List bytes;
  bool _disposed = false;
  bool get isDisposed => _disposed;
  static Uint8List _copy(Uint8List value) {
    if (value.length != 64 ||
        !value.every((b) => b >= 48 && b <= 57 || b >= 97 && b <= 102)) {
      throw const FormatException('Invalid issued service token');
    }
    return Uint8List.fromList(value);
  }

  void dispose() {
    bytes.fillRange(0, bytes.length, 0);
    _disposed = true;
  }

  @override
  String toString() => 'IssuedToken(redacted)';
}

class IssuedServiceAuthentication {
  const IssuedServiceAuthentication({
    required this.authority,
    required this.token,
  });
  final StoredServiceAuthority authority;
  final IssuedToken token;
  void dispose() => token.dispose();
}

abstract interface class WorkbenchServiceControl {
  Future<ServiceConfigPage> serviceConfigPage({
    String? after,
    Uint8List? snapshot,
  });
  Future<StoredServiceConfig> saveServiceConfig(ServiceConfigUpdate update);
  Future<StoredServiceConfig> disableServiceConfig(
    String id,
    BigInt expectedRevision,
  );
  Future<ServiceAuthorityPage> serviceAuthorityPage({
    Uint8List? after,
    Uint8List? snapshot,
  });
  Future<IssuedServiceAuthentication> issueServiceAuthentication({
    required Uint8List reference,
    required BigInt expectedRevision,
    required String principalId,
    required int lifetimeDays,
  });
  Future<StoredServiceAuthority> disableServiceAuthority(
    Uint8List reference,
    BigInt expectedRevision,
  );
  Future<StoredServiceAuthority> saveServicePublication(
    ServicePublicationUpdate update,
  );
}

/// Bounded platform-neutral validation. The host still resolves original Store
/// references, selected package identity, approvals, time and exact address policy.
abstract final class ServiceValidation {
  static final maxRevision = (BigInt.one << 63) - BigInt.one;
  static final maxUint64 = (BigInt.one << 64) - BigInt.one;
  static final maxLifetimeMs = BigInt.from(2592000000);
  static void identity(String value) {
    if (value.isEmpty ||
        utf8.encode(value).length > 256 ||
        value.runes.any(
          (v) =>
              v < 32 || v >= 127 && v <= 159 || v == 47 || v == 92 || v == 58,
        )) {
      throw const FormatException('Invalid service identity');
    }
  }

  static void principalId(String value) {
    if (!RegExp(r'^[A-Za-z0-9._-]{1,128}$').hasMatch(value)) {
      throw const FormatException('Invalid service principal');
    }
  }

  static void digest(Uint8List value) {
    if (value.length != 32 || value.every((b) => b == 0)) {
      throw const FormatException('Invalid service reference');
    }
  }

  static void unsigned(BigInt value) {
    if (value < BigInt.zero || value > maxUint64) {
      throw const FormatException('Invalid service integer');
    }
  }

  static void revision(
    BigInt value, {
    bool zero = false,
    bool updating = false,
  }) {
    if (value < (zero ? BigInt.zero : BigInt.one) ||
        value > maxRevision ||
        updating && value == maxRevision) {
      throw const FormatException('Invalid service revision');
    }
  }

  static void days(int value) {
    if (value < 1 || value > 30) {
      throw const FormatException('Invalid service lifetime');
    }
  }

  static void referenceRevision(
    Uint8List reference,
    BigInt value, {
    bool create = false,
  }) {
    revision(value, zero: create);
    if (create && reference.isEmpty && value == BigInt.zero) return;
    digest(reference);
    if (value == BigInt.zero) {
      throw const FormatException('Invalid service replacement');
    }
  }

  static int compareBytes(List<int> a, List<int> b) {
    for (var i = 0; i < a.length && i < b.length; i++) {
      if (a[i] != b[i]) return a[i].compareTo(b[i]);
    }
    return a.length.compareTo(b.length);
  }

  static int _textOrder(String a, String b) =>
      compareBytes(utf8.encode(a), utf8.encode(b));
  static void principals(List<ServicePrincipal> values) {
    if (values.length > 64) {
      throw const FormatException('Too many service principals');
    }
    String? last;
    var count = 0;
    for (final p in values) {
      identity(p.id);
      digest(p.authenticationReference);
      if (last != null && _textOrder(last, p.id) >= 0) {
        throw const FormatException('Service principal order');
      }
      last = p.id;
      count += p.scopes.length;
      if (count > 128) throw const FormatException('Too many service scopes');
      ServiceContentScope? previous;
      for (final s in p.scopes) {
        if (s.kind < 1 || s.kind > 7) {
          throw const FormatException('Invalid service scope');
        }
        identity(s.cardId);
        if (s.kind == 4) {
          identity(s.attachmentId);
        } else if (s.attachmentId.isNotEmpty) {
          throw const FormatException('Invalid attachment scope');
        }
        if (previous != null) {
          var order = previous.kind.compareTo(s.kind);
          if (order == 0) order = _textOrder(previous.cardId, s.cardId);
          if (order == 0) {
            order = _textOrder(previous.attachmentId, s.attachmentId);
          }
          if (order >= 0) throw const FormatException('Service scope order');
        }
        previous = s;
      }
    }
  }

  static void retention(BigInt value) {
    if (value <= BigInt.zero || value > maxLifetimeMs) {
      throw const FormatException('Invalid service retention');
    }
  }

  static void configUpdate(ServiceConfigUpdate value) {
    revision(value.expectedRevision, zero: true, updating: true);
    if (value.id.isEmpty) {
      if (value.expectedRevision != BigInt.zero) {
        throw const FormatException('Invalid new configuration');
      }
    } else {
      identity(value.id);
      revision(value.expectedRevision, updating: true);
    }
    unsigned(value.registryRevision);
    identity(value.packageId);
    digest(value.packageDigest);
    identity(value.service);
    identity(value.handler);
    retention(value.retentionMs);
    principals(value.principals);
    if (value.principals.isEmpty) {
      throw const FormatException('Service needs a principal');
    }
  }

  static void storedConfig(StoredServiceConfig value) {
    identity(value.id);
    revision(value.revision);
    digest(value.namespace);
    retention(value.retentionMs);
    identity(value.service);
    identity(value.handler);
    digest(value.packageDigest);
    digest(value.digest);
    principals(value.principals);
    if (value.approvalReferences.length > 64) {
      throw const FormatException('Too many service approvals');
    }
    Uint8List? last;
    for (final ref in value.approvalReferences) {
      digest(ref);
      if (last != null && compareBytes(last, ref) >= 0) {
        throw const FormatException('Service approval order');
      }
      last = ref;
    }
  }

  static void _path(String value) {
    if (!value.startsWith('/') ||
        value.startsWith('//') ||
        value.length > 8192 ||
        !RegExp(r"^[/A-Za-z0-9\-._~!\$&'()+,;=:@%]+$").hasMatch(value) ||
        RegExp(r'%(?![A-Fa-f0-9]{2})').hasMatch(value)) {
      throw const FormatException('Invalid service path');
    }
  }

  static void publication(ServicePublication value) {
    identity(value.configId);
    digest(value.configDigest);
    final address = value.listenAddress;
    if (address.length > 64 ||
        !RegExp(
          r'^(?:[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+|\[[0-9a-f:.]+(?:%[0-9]{1,10})?\]):[0-9]{1,5}$',
        ).hasMatch(address)) {
      throw const FormatException('Invalid service listen address');
    }
    final colon = address.lastIndexOf(':');
    // Rust SocketAddr persists numeric IPv6 scope IDs rather than URI zones.
    // Validate the canonical u32 separately, then let Uri validate the IP only.
    // A zero scope is omitted by SocketAddr::to_string(), never stored as %0.
    final scopeStart = address.indexOf('%');
    var syntaxAddress = address;
    if (scopeStart >= 0) {
      final scopeEnd = address.indexOf(']', scopeStart);
      final rawScope = address.substring(scopeStart + 1, scopeEnd);
      final scope = BigInt.parse(rawScope);
      if (scope <= BigInt.zero ||
          scope > BigInt.from(0xffffffff) ||
          scope.toString() != rawScope) {
        throw const FormatException('Invalid service IPv6 scope');
      }
      syntaxAddress =
          address.substring(0, scopeStart) + address.substring(scopeEnd);
    }
    // Pure Dart validation keeps the model available in Web builds.
    Uri.parse('http://$syntaxAddress');
    final port = int.parse(address.substring(colon + 1));
    if (port > 65535 || port.toString() != address.substring(colon + 1)) {
      throw const FormatException('Invalid service port');
    }
    final host = address.substring(0, colon);
    final syntaxHost = syntaxAddress.substring(
      0,
      syntaxAddress.lastIndexOf(':'),
    );
    var loopback = syntaxHost == '[::1]';
    if (!host.startsWith('[')) {
      final parts = host.split('.');
      for (final p in parts) {
        final n = int.parse(p);
        if (n > 255 || n.toString() != p) {
          throw const FormatException('Invalid service address');
        }
      }
      loopback = parts.first == '127';
    }
    if (!loopback && !value.tlsRequired) {
      throw const FormatException('Service TLS required');
    }
    if (!RegExp(r'^[A-Z-]{1,32}$').hasMatch(value.method)) {
      throw const FormatException('Invalid service method');
    }
    _path(value.path);
    if (value.queryPath.isNotEmpty) {
      _path(value.queryPath);
      if (value.path == value.queryPath) {
        throw const FormatException('Service paths must differ');
      }
    }
  }

  static void publicationUpdate(ServicePublicationUpdate value) {
    digest(value.reference);
    revision(value.expectedRevision, zero: true, updating: true);
    revision(value.configRevision);
    unsigned(value.registryRevision);
    identity(value.packageId);
    days(value.lifetimeDays);
    publication(value.policy);
  }

  static void authority(StoredServiceAuthority value) {
    digest(value.reference);
    revision(value.revision);
    unsigned(value.createdMs);
    unsigned(value.expiresMs);
    final duration = value.expiresMs - value.createdMs;
    if (value.createdMs == BigInt.zero ||
        duration <= BigInt.zero ||
        duration > maxLifetimeMs) {
      throw const FormatException('Invalid service authority lifetime');
    }
    if (value.kind == 1) {
      principalId(value.principalId);
      if (value.publication != null) {
        throw const FormatException('Mixed service authority');
      }
    } else if (value.kind == 2) {
      if (value.principalId.isNotEmpty || value.publication == null) {
        throw const FormatException('Mixed service authority');
      }
      publication(value.publication!);
    } else {
      throw const FormatException('Unknown service authority kind');
    }
  }
}
