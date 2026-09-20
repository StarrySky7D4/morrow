import 'dart:typed_data';

/// Persisted policy metadata; saving it never creates an active network grant.
class EndpointPolicy {
  const EndpointPolicy({
    required this.packageId,
    required this.packageDigest,
    required this.origin,
    required this.profile,
    required this.methods,
    required this.credentialReference,
    required this.rootCertificate,
    required this.maxRequestBytes,
    required this.maxResponseBytes,
    required this.maxHeaderBytes,
    required this.maxConcurrent,
    required this.timeoutMs,
    required this.maxFrameBytes,
  });
  final String packageId, origin;
  final Uint8List packageDigest, credentialReference, rootCertificate;

  /// 1: public HTTPS, 2: local HTTP, 3: local HTTPS.
  final int profile;
  final List<String> methods;
  final int maxRequestBytes, maxResponseBytes, maxHeaderBytes;
  final int maxConcurrent, timeoutMs, maxFrameBytes;
}

class StoredEndpoint {
  const StoredEndpoint({
    required this.reference,
    required this.revision,
    required this.createdMs,
    required this.expiresMs,
    required this.disabled,
    required this.policy,
  });
  final Uint8List reference;
  final BigInt revision, createdMs, expiresMs;
  final bool disabled;
  final EndpointPolicy policy;
}

class EndpointPage {
  const EndpointPage({
    required this.entries,
    required this.snapshot,
    this.next,
  });
  final List<StoredEndpoint> entries;
  final Uint8List snapshot;
  final Uint8List? next;
}

abstract interface class WorkbenchEndpointControl {
  Future<EndpointPage> endpointPage({Uint8List? after, Uint8List? snapshot});
  Future<StoredEndpoint> saveEndpoint({
    required Uint8List reference,
    required BigInt expectedRevision,
    required BigInt registryRevision,
    required int lifetimeDays,
    required EndpointPolicy policy,
  });
  Future<StoredEndpoint> disableEndpoint(StoredEndpoint expected);
}
