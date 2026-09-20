import 'dart:typed_data';

import 'io_task_models.dart';
import 'service_control.dart' show ServiceValidation;

Uint8List _owned(Uint8List bytes) =>
    Uint8List.fromList(bytes).asUnmodifiableView();

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
  }) : submission = _owned(submission),
       configDigest = _owned(configDigest),
       publication = _owned(publication),
       packageDigest = _owned(packageDigest);
  final Uint8List submission, configDigest, publication, packageDigest;
  final String configId, packageId;
  final BigInt configRevision, publicationRevision, registryRevision;
  final BigInt maxJobs, maxBytes, maxJobBytes, maxTotalBytes;
  final int lifetimeMs, maxCalls, maxRequestBytes, maxResponseBytes;
  final int maxHeaderBytes, maxConcurrent, timeoutMs;
}

enum ServiceRunPhase { starting, running, stopping, exited }

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
  void dispose() {
    final bytes = _payload;
    if (bytes != null) bytes.fillRange(0, bytes.length, 0);
    _payload = null;
    _disposed = true;
  }
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
