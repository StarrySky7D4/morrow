import 'dart:typed_data';
import 'generated/host.capnp.dart' as host;
import 'service_run_control.dart';

/// Copies redacted metadata before the original response frame is erased.
abstract final class ServiceTlsIdentityCodec {
  static bool same(List<int> a, List<int> b) {
    if (a.length != b.length) return false;
    for (var i = 0; i < a.length; i++) {
      if (a[i] != b[i]) return false;
    }
    return true;
  }

  static void _metadataOnly(host.ResponseReader response) {
    final token = response.issuedToken;
    final unexpectedToken = token?.isNotEmpty == true;
    token?.fillRange(0, token.length, 0);
    if (unexpectedToken ||
        response.serviceTls != null ||
        response.payload?.isNotEmpty == true) {
      throw const FormatException('Unexpected TLS identity material');
    }
  }

  static ServiceTlsIdentityInfo _row(host.TlsIdentityInfoReader row) {
    final value = row.choice;
    if (value == null) throw const FormatException('Missing TLS identity');
    final choice = ServiceTlsIdentityChoice(
      reference: value.reference ?? Uint8List(0),
      revision: value.revisionBigInt,
      certificateSha256: value.certificateSha256 ?? Uint8List(0),
    );
    ServiceRunValidation.tlsIdentity(choice);
    return ServiceTlsIdentityInfo(choice: choice, disabled: row.disabled);
  }

  static ServiceTlsIdentityPage page(
    host.ResponseReader response, {
    Uint8List? after,
    Uint8List? expectedSnapshot,
  }) {
    _metadataOnly(response);
    final snapshot = ServiceRunValidation.identity(
      response.serviceSnapshot ?? Uint8List(0),
    );
    if (expectedSnapshot != null && !same(snapshot, expectedSnapshot)) {
      throw const FormatException('TLS identity snapshot changed');
    }
    final rows = response.tlsIdentities;
    if (rows == null || rows.length > 16) {
      throw const FormatException('Invalid TLS identity page');
    }
    final identities = <ServiceTlsIdentityInfo>[];
    var previous = after == null
        ? ''
        : after.map((b) => b.toRadixString(16).padLeft(2, '0')).join();
    for (final row in rows) {
      final info = _row(row);
      if (info.choice.key.compareTo(previous) <= 0) {
        throw const FormatException('Invalid TLS identity order');
      }
      previous = info.choice.key;
      identities.add(info);
    }
    final cursor = response.serviceCursor;
    final next = cursor == null || cursor.isEmpty
        ? null
        : ServiceRunValidation.identity(cursor);
    if (next != null &&
        (identities.length != 16 ||
            !same(next, identities.last.choice.reference))) {
      throw const FormatException('Invalid TLS identity cursor');
    }
    return ServiceTlsIdentityPage(
      identities: identities,
      snapshot: snapshot,
      next: next,
    );
  }

  static ServiceTlsIdentityInfo saved(
    host.ResponseReader response, {
    required Uint8List reference,
    required BigInt revision,
    required Uint8List certificateSha256,
    required bool disabled,
  }) {
    _metadataOnly(response);
    final rows = response.tlsIdentities;
    if (rows == null || rows.length != 1) {
      throw const FormatException('Missing TLS identity receipt');
    }
    final result = _row(rows[0]);
    if ((reference.isNotEmpty && !same(reference, result.choice.reference)) ||
        result.choice.revision != revision ||
        result.disabled != disabled ||
        !same(certificateSha256, result.choice.certificateSha256)) {
      throw const FormatException('TLS identity receipt mismatch');
    }
    return result;
  }
}
