import 'dart:typed_data';
import 'dart:convert';

import 'io_task_models.dart';
import 'service_control.dart' show ServiceValidation;
import 'service_tls_identity_models.dart';
export 'service_tls_identity_models.dart';

Uint8List _owned(Uint8List bytes) =>
    Uint8List.fromList(bytes).asUnmodifiableView();

class ServiceTlsSelection {
  ServiceTlsSelection({
    required this.certificatePath,
    required this.privateKeyPath,
    required Uint8List certificateSha256,
    this.validity,
  }) : certificateSha256 = _owned(certificateSha256);
  final String certificatePath, privateKeyPath;
  final Uint8List certificateSha256;
  final ServiceTlsValidity? validity;
}

class ServiceTlsValidity {
  const ServiceTlsValidity({
    required this.notBeforeSeconds,
    required this.notAfterSeconds,
  });
  final int notBeforeSeconds, notAfterSeconds;
  bool validAt(DateTime time) {
    final now = time.millisecondsSinceEpoch;
    return now >= notBeforeSeconds * 1000 && now < (notAfterSeconds + 1) * 1000;
  }
}

abstract interface class WorkbenchServiceTlsControl {
  Future<ServiceTlsSelection> inspectServiceTls({
    required String certificatePath,
    required String privateKeyPath,
  });
}

/// Original-owner administration. A lost reply must be reconciled, never retried automatically.
abstract interface class WorkbenchTlsIdentityControl {
  Future<ServiceTlsIdentityPage> tlsIdentityPage({
    Uint8List? after,
    Uint8List? snapshot,
  });
  Future<ServiceTlsIdentityInfo> saveTlsIdentity({
    required ServiceTlsSelection selection,
    required Uint8List reference,
    required BigInt expectedRevision,
  });
  Future<ServiceTlsIdentityInfo> disableTlsIdentity(
    ServiceTlsIdentityInfo expected,
  );
}

class ServiceEndpointSelection {
  ServiceEndpointSelection({
    required Uint8List reference,
    required this.revision,
  }) : reference = _owned(reference);
  final Uint8List reference;
  final BigInt revision;
  String get key =>
      reference.map((b) => b.toRadixString(16).padLeft(2, '0')).join();
}

/// A fresh explicit service attempt, bounded by the original package and stored
/// approvals. Query the current service and compare this submission to recover
/// a lost start receipt; never automatically resubmit a start request.
class ServiceRunRequest {
  ServiceRunRequest({
    required Uint8List submission,
    required this.configId,
    required Uint8List configDigest,
    required this.configRevision,
    required Uint8List publication,
    required this.publicationRevision,
    required this.packageId,
    required Uint8List packageDigest,
    required this.registryRevision,
    required this.lifetimeMs,
    required this.maxJobs,
    required this.maxBytes,
    required this.maxJobBytes,
    required this.maxTotalBytes,
    this.maxCalls = 16,
    this.maxRequestBytes = 65536,
    this.maxResponseBytes = 65536,
    this.maxHeaderBytes = 16384,
    this.maxConcurrent = 1,
    this.timeoutMs = 10000,
    List<ServiceEndpointSelection> outbound = const [],
    ServiceTlsSelection? tls,
    ServiceTlsIdentityChoice? protectedTls,
  }) : submission = _owned(submission),
       configDigest = _owned(configDigest),
       publication = _owned(publication),
       packageDigest = _owned(packageDigest),
       protectedTls = protectedTls == null
           ? null
           : ServiceTlsIdentityChoice(
               reference: protectedTls.reference,
               revision: protectedTls.revision,
               certificateSha256: protectedTls.certificateSha256,
             ),
       tls = tls == null
           ? null
           : ServiceTlsSelection(
               certificatePath: tls.certificatePath,
               privateKeyPath: tls.privateKeyPath,
               certificateSha256: tls.certificateSha256,
               validity: tls.validity,
             ),
       outbound = List.unmodifiable(
         outbound.map(
           (e) => ServiceEndpointSelection(
             reference: e.reference,
             revision: e.revision,
           ),
         ),
       );
  final List<ServiceEndpointSelection> outbound;
  final ServiceTlsSelection? tls;
  final ServiceTlsIdentityChoice? protectedTls;
  final Uint8List submission, configDigest, publication, packageDigest;
  final String configId, packageId;
  final BigInt configRevision, publicationRevision, registryRevision;
  final BigInt maxJobs, maxBytes, maxJobBytes, maxTotalBytes;
  final int lifetimeMs, maxCalls, maxRequestBytes, maxResponseBytes;
  final int maxHeaderBytes, maxConcurrent, timeoutMs;
}

enum ServiceRunPhase { starting, running, stopping, exited }

/// A valid host error response to a start request, as distinct from a lost or
/// malformed response. The host may retain a cleanup task: callers must inspect
/// its original identity before deciding which operation is available next.
class ServiceRunStartFailure implements Exception {
  const ServiceRunStartFailure(this.message);
  final String message;
  @override
  String toString() => message;
}

enum ServiceNetworkOutcome {
  pending,
  succeeded,
  invalid,
  denied,
  limit,
  cancelled,
  timeout,
  transport,
  closed,
}

enum OwnerCommandDelivery { pending, ready, consumed }

enum OwnerCommandTerminal {
  none,
  busy,
  closed,
  limit,
  cancelled,
  unknown,
  consumed,
}

class ServiceRunSnapshot {
  ServiceRunSnapshot({
    required this.task,
    required Uint8List submission,
    required this.phase,
    required this.address,
    required this.bind,
    required this.listener,
    required this.supervision,
  }) : submission = _owned(submission);
  final IoTaskSnapshot task;
  final Uint8List submission;
  final ServiceRunPhase phase;
  final String? address;
  final ServiceNetworkOutcome bind, listener, supervision;
}

class OwnerCommandSnapshot {
  OwnerCommandSnapshot({
    required Uint8List key,
    required Uint8List submission,
    required this.delivery,
    required this.started,
    required this.terminal,
  }) : key = _owned(key),
       submission = _owned(submission);
  final Uint8List key, submission;
  final OwnerCommandDelivery delivery;
  final bool started;
  final OwnerCommandTerminal terminal;
}

/// Owns the possibly secret-bearing nested response frame. The caller must
/// dispose this result and protect any further copies or decoded secret values.
/// No immutable String or unmodifiable payload copy is created here.
class OwnerCommandRead {
  OwnerCommandRead({required this.snapshot, Uint8List? payload})
    : _payload = payload == null ? null : Uint8List.fromList(payload);
  final OwnerCommandSnapshot snapshot;
  Uint8List? _payload;
  bool _disposed = false;
  Uint8List? get payload => _payload;
  bool get disposed => _disposed;

  /// Transfers this buffer's lifetime to a caller that must retain a native
  /// reader backed by it. The receiver now owns erasure and further copies.
  Uint8List? takePayload() {
    final bytes = _payload;
    _payload = null;
    _disposed = true;
    return bytes;
  }

  void dispose() {
    final bytes = _payload;
    if (bytes != null) bytes.fillRange(0, bytes.length, 0);
    _payload = null;
    _disposed = true;
  }
}

/// A routed business command did not deliver a usable response. Retained
/// identities permit read-only reconciliation, never automatic resubmission.
class ServiceCommandFailure implements Exception {
  ServiceCommandFailure({
    required this.message,
    required Uint8List task,
    required Uint8List submission,
    Uint8List? command,
    required this.outcomeUnknown,
    this.terminal,
  }) : task = _owned(task),
       submission = _owned(submission),
       command = command == null ? null : _owned(command);
  final String message;
  final Uint8List task, submission;
  final Uint8List? command;
  final bool outcomeUnknown;
  final OwnerCommandTerminal? terminal;
  @override
  String toString() => message;
}

abstract interface class WorkbenchServiceRunControl {
  Future<ServiceRunSnapshot> startServiceRun(ServiceRunRequest request);
  Future<ServiceRunSnapshot> serviceRunStatus({Uint8List? key});
  Future<OwnerCommandSnapshot> submitServiceCommand(
    Uint8List task,
    Uint8List submission,
    Uint8List frame,
  );
  Future<OwnerCommandSnapshot> serviceCommandStatus(
    Uint8List task,
    Uint8List key,
  );

  /// Recovers a lost acknowledgement without submitting or replaying a command.
  Future<OwnerCommandSnapshot> serviceCommandBySubmission(
    Uint8List task,
    Uint8List submission,
  );
  Future<OwnerCommandRead> readServiceCommand(Uint8List task, Uint8List key);
  Future<OwnerCommandSnapshot> cancelServiceCommand(
    Uint8List task,
    Uint8List key,
  );
}

abstract final class ServiceRunValidation {
  static final maxTlsRevision = (BigInt.one << 63) - BigInt.one;
  static void tlsIdentity(ServiceTlsIdentityChoice choice) {
    identity(choice.reference);
    identity(choice.certificateSha256);
    _bigRange(choice.revision, maxTlsRevision);
  }

  static void tlsMutation(
    Uint8List reference,
    BigInt revision, {
    bool create = false,
  }) {
    if (create && reference.isEmpty && revision == BigInt.zero) return;
    identity(reference);
    _bigRange(revision, maxTlsRevision);
  }

  static void tlsPath(String path) {
    if (path.isEmpty ||
        utf8.encode(path).length > 4096 ||
        path.runes.any((r) => r < 32 || r >= 127 && r <= 159)) {
      throw const FormatException('Invalid selected TLS path');
    }
  }

  static void tlsSelection(ServiceTlsSelection value) {
    tlsPath(value.certificatePath);
    tlsPath(value.privateKeyPath);
    identity(value.certificateSha256);
    if (value.validity case final ServiceTlsValidity validity) {
      if (validity.notBeforeSeconds < -62135596800 ||
          validity.notAfterSeconds > 253402300799 ||
          validity.notBeforeSeconds > validity.notAfterSeconds) {
        throw const FormatException('Invalid TLS certificate validity');
      }
    }
  }

  static Uint8List identity(Uint8List value) {
    ServiceValidation.digest(value);
    return _owned(value);
  }

  static void _range(int value, int maximum) {
    if (value < 1 || value > maximum) {
      throw const FormatException('Invalid service run bound');
    }
  }

  static void _bigRange(BigInt value, BigInt maximum) {
    ServiceValidation.unsigned(value);
    if (value < BigInt.one || value > maximum) {
      throw const FormatException('Invalid service run integer');
    }
  }

  static void request(ServiceRunRequest value) {
    if (value.tls != null && value.protectedTls != null) {
      throw const FormatException('Ambiguous TLS identity choice');
    }
    if (value.protectedTls case final ServiceTlsIdentityChoice choice) {
      tlsIdentity(choice);
    }
    if (value.tls case final ServiceTlsSelection selected) {
      tlsSelection(selected);
    }
    if (value.outbound.length > 8) {
      throw const FormatException('Too many service outbound endpoints');
    }
    final seen = <String>{};
    for (final endpoint in value.outbound) {
      ServiceValidation.digest(endpoint.reference);
      _bigRange(endpoint.revision, ServiceValidation.maxUint64);
      if (!seen.add(endpoint.key)) {
        throw const FormatException('Duplicate service outbound endpoint');
      }
    }
    for (final identity in [
      value.submission,
      value.configDigest,
      value.publication,
      value.packageDigest,
    ]) {
      ServiceValidation.digest(identity);
    }
    ServiceValidation.identity(value.configId);
    ServiceValidation.identity(value.packageId);
    for (final revision in [
      value.configRevision,
      value.publicationRevision,
      value.registryRevision,
    ]) {
      _bigRange(revision, ServiceValidation.maxUint64);
    }
    _range(value.lifetimeMs, 3600000);
    _range(value.timeoutMs, 30000);
    if (value.timeoutMs > value.lifetimeMs) {
      throw const FormatException('Service timeout exceeds lifetime');
    }
    _range(value.maxCalls, 1024);
    _range(value.maxRequestBytes, 64 * 1024 * 1024);
    _range(value.maxResponseBytes, 64 * 1024 * 1024);
    _range(value.maxHeaderBytes, 64 * 1024);
    _range(value.maxConcurrent, 128);
    _bigRange(value.maxJobs, BigInt.from(1000000));
    _bigRange(value.maxBytes, BigInt.from(64 * 1024 * 1024));
    _bigRange(value.maxJobBytes, BigInt.from(16 * 1024 * 1024));
    _bigRange(value.maxTotalBytes, BigInt.from(64 * 1024 * 1024));
    if (value.maxJobBytes > value.maxTotalBytes) {
      throw const FormatException('Service job exceeds total budget');
    }
  }

  static void command(Uint8List frame) {
    if (frame.length < 8 || frame.length > 64 * 1024) {
      throw const FormatException('Invalid nested command size');
    }
  }
}
