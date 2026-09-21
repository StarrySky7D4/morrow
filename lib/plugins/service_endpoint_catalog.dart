import 'package:flutter/foundation.dart';
import 'endpoint_control.dart';
import 'io_task_control.dart';

String _key(List<int> bytes) =>
    bytes.map((b) => b.toRadixString(16).padLeft(2, '0')).join();

/// Complete bounded snapshot; partial/changed pages are never eligible choices.
Future<List<StoredEndpoint>> loadServiceEndpoints(
  WorkbenchEndpointControl backend,
) async {
  final entries = <StoredEndpoint>[];
  Uint8List? cursor, snapshot;
  for (var index = 0; index < 512; index++) {
    final page = await backend.endpointPage(
      after: cursor == null ? null : Uint8List.fromList(cursor),
      snapshot: snapshot == null ? null : Uint8List.fromList(snapshot),
    );
    if (page.snapshot.length != 32 ||
        (snapshot != null && !listEquals(snapshot, page.snapshot)) ||
        page.entries.length > 2 ||
        entries.length + page.entries.length > 512) {
      throw const FormatException('Inconsistent service endpoint page');
    }
    snapshot ??= Uint8List.fromList(page.snapshot);
    for (final raw in page.entries) {
      final entry = _checkedEndpoint(raw);
      if ((cursor != null &&
              _key(entry.reference).compareTo(_key(cursor)) <= 0) ||
          (entries.isNotEmpty &&
              _key(entry.reference).compareTo(_key(entries.last.reference)) <=
                  0)) {
        throw const FormatException('Invalid service endpoint order');
      }
      entries.add(entry);
    }
    final next = page.next;
    if (next == null) return List.unmodifiable(entries);
    HttpTaskValidation.identity(next);
    if ((cursor != null && _key(next).compareTo(_key(cursor)) <= 0) ||
        (entries.isNotEmpty &&
            _key(next).compareTo(_key(entries.last.reference)) < 0)) {
      throw const FormatException('Invalid service endpoint cursor');
    }
    cursor = Uint8List.fromList(next);
  }
  throw const FormatException('Service endpoint page bound');
}

StoredEndpoint _checkedEndpoint(StoredEndpoint e) {
  HttpTaskValidation.identity(e.reference);
  HttpTaskValidation.identity(e.policy.packageDigest);
  if (e.revision <= BigInt.zero ||
      e.revision >= (BigInt.one << 64) ||
      e.createdMs <= BigInt.zero ||
      e.expiresMs <= e.createdMs ||
      e.expiresMs > BigInt.from(253402300799999) ||
      e.policy.packageId.isEmpty ||
      e.policy.methods.isEmpty ||
      e.policy.methods.length > 16 ||
      e.policy.methods.toSet().length != e.policy.methods.length ||
      e.policy.methods.any((m) => !RegExp(r'^[A-Z]{1,16}$').hasMatch(m))) {
    throw const FormatException('Invalid endpoint metadata');
  }
  if (e.policy.credentialReference.isNotEmpty) {
    HttpTaskValidation.identity(e.policy.credentialReference);
  }
  final p = e.policy;
  return StoredEndpoint(
    reference: Uint8List.fromList(e.reference),
    revision: e.revision,
    createdMs: e.createdMs,
    expiresMs: e.expiresMs,
    disabled: e.disabled,
    policy: EndpointPolicy(
      packageId: p.packageId,
      packageDigest: Uint8List.fromList(p.packageDigest),
      origin: p.origin,
      profile: p.profile,
      methods: List.of(p.methods),
      credentialReference: Uint8List.fromList(p.credentialReference),
      rootCertificate: Uint8List.fromList(p.rootCertificate),
      maxRequestBytes: p.maxRequestBytes,
      maxResponseBytes: p.maxResponseBytes,
      maxHeaderBytes: p.maxHeaderBytes,
      maxConcurrent: p.maxConcurrent,
      timeoutMs: p.timeoutMs,
      maxFrameBytes: p.maxFrameBytes,
    ),
  );
}
