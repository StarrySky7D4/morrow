import 'dart:typed_data';

import 'generated/host.capnp.dart' as host;
import 'io_task_codec_native.dart';
import 'io_task_models.dart';
import 'service_run_control.dart';

/// Native scheduler codec. It never decodes nested secret-bearing business data.
abstract final class ServiceRunCodec {
  static void writeTls(
    ServiceTlsSelection value,
    host.ServiceTlsSelectionBuilder out,
  ) {
    ServiceRunValidation.tlsSelection(value);
    out.certificatePath = value.certificatePath;
    out.privateKeyPath = value.privateKeyPath;
    out.certificateSha256 = value.certificateSha256;
  }

  static ServiceTlsSelection tlsSelection(
    host.ResponseReader response, {
    required String certificatePath,
    required String privateKeyPath,
  }) {
    _noToken(response);
    final row = response.serviceTls;
    if (row == null ||
        row.certificatePath != certificatePath ||
        row.privateKeyPath != privateKeyPath) {
      throw const FormatException('TLS inspection selection mismatch');
    }
    final selected = ServiceTlsSelection(
      certificatePath: certificatePath,
      privateKeyPath: privateKeyPath,
      certificateSha256: row.certificateSha256 ?? Uint8List(0),
      validity: row.validity == null
          ? throw const FormatException('Missing TLS certificate validity')
          : ServiceTlsValidity(
              notBeforeSeconds: row.validity!.notBeforeSeconds,
              notAfterSeconds: row.validity!.notAfterSeconds,
            ),
    );
    ServiceRunValidation.tlsSelection(selected);
    return selected;
  }

  static int responseMaxBytes(host.Action action) =>
      action == host.Action.commandRead ? 256 * 1024 : 128 * 1024;
  static int _wire(BigInt value) => value.toSigned(64).toInt();
  static T _enum<T>(List<T> values, int code) {
    if (code < 0 || code >= values.length) {
      throw const FormatException('Unknown service runtime state');
    }
    return values[code];
  }

  static bool _same(Uint8List a, Uint8List b) {
    if (a.length != b.length) return false;
    for (var i = 0; i < a.length; i++) {
      if (a[i] != b[i]) return false;
    }
    return true;
  }

  static void _noToken(host.ResponseReader response) {
    final token = response.issuedToken;
    if (token == null) return;
    try {
      if (token.isNotEmpty) {
        throw const FormatException('Unexpected scheduler token');
      }
    } finally {
      token.fillRange(0, token.length, 0);
    }
  }

  static void writeRequest(
    ServiceRunRequest value,
    host.ServiceRunStartBuilder out,
  ) {
    ServiceRunValidation.request(value);
    out.submission = value.submission;
    out.configId = value.configId;
    out.configDigest = value.configDigest;
    out.configRevision = _wire(value.configRevision);
    out.publication = value.publication;
    out.publicationRevision = _wire(value.publicationRevision);
    out.packageId = value.packageId;
    out.packageDigest = value.packageDigest;
    out.registryRevision = _wire(value.registryRevision);
    out.lifetimeMs = value.lifetimeMs;
    out.maxJobs = _wire(value.maxJobs);
    out.maxBytes = _wire(value.maxBytes);
    out.maxCalls = value.maxCalls;
    out.maxJobBytes = _wire(value.maxJobBytes);
    out.maxTotalBytes = _wire(value.maxTotalBytes);
    out.maxRequestBytes = value.maxRequestBytes;
    out.maxResponseBytes = value.maxResponseBytes;
    out.maxHeaderBytes = value.maxHeaderBytes;
    out.maxConcurrent = value.maxConcurrent;
    out.timeoutMs = value.timeoutMs;
    if (value.tls case final ServiceTlsSelection selected) {
      writeTls(selected, out.initTls());
    }
    if (value.protectedTls case final ServiceTlsIdentityChoice selected) {
      final choice = out.initProtectedTls();
      choice.reference = selected.reference;
      choice.revision = _wire(selected.revision);
      choice.certificateSha256 = selected.certificateSha256;
    }
    final outbound = out.initOutbound(value.outbound.length);
    for (var i = 0; i < value.outbound.length; i++) {
      outbound[i].reference = value.outbound[i].reference;
      outbound[i].revision = _wire(value.outbound[i].revision);
    }
  }

  static ServiceRunSnapshot snapshot(
    host.ResponseReader response, {
    Uint8List? expectedKey,
    Uint8List? expectedSubmission,
  }) {
    _noToken(response);
    final row = response.serviceRun;
    if (row == null) throw const FormatException('Missing service run state');
    final task = HttpTaskCodec.snapshot(row.task);
    if (task.key == null || task.submission == null) {
      throw const FormatException('Missing service run identity');
    }
    final submission = ServiceRunValidation.identity(
      row.submission ?? Uint8List(0),
    );
    if (!_same(task.submission!, submission) ||
        expectedKey != null && !_same(task.key!, expectedKey) ||
        expectedSubmission != null && !_same(submission, expectedSubmission)) {
      throw const FormatException('Service run identity mismatch');
    }
    final phase = _enum(ServiceRunPhase.values, row.phase);
    final bind = _enum(ServiceNetworkOutcome.values, row.bind);
    final listener = _enum(ServiceNetworkOutcome.values, row.listener);
    final supervision = _enum(ServiceNetworkOutcome.values, row.supervision);
    final address = row.address ?? '';
    if (address.length > 256 ||
        address.codeUnits.any((v) => v < 32) ||
        phase == ServiceRunPhase.running &&
            (bind != ServiceNetworkOutcome.succeeded ||
                address.isEmpty ||
                task.exit != null ||
                (task.storage != IoStoragePhase.running &&
                    task.storage != IoStoragePhase.stopping)) ||
        phase == ServiceRunPhase.exited && task.exit == null) {
      throw const FormatException('Inconsistent service run state');
    }
    return ServiceRunSnapshot(
      task: task,
      submission: submission,
      phase: phase,
      address: address.isEmpty ? null : address,
      bind: bind,
      listener: listener,
      supervision: supervision,
    );
  }

  static OwnerCommandSnapshot command(
    host.ResponseReader response, {
    Uint8List? expectedKey,
    Uint8List? expectedSubmission,
  }) {
    _noToken(response);
    final row = response.ownerCommand;
    if (row == null) throw const FormatException('Missing owner command state');
    final key = ServiceRunValidation.identity(row.key ?? Uint8List(0));
    final submission = ServiceRunValidation.identity(
      row.submission ?? Uint8List(0),
    );
    if (expectedKey != null && !_same(key, expectedKey) ||
        expectedSubmission != null && !_same(submission, expectedSubmission)) {
      throw const FormatException('Owner command identity mismatch');
    }
    final delivery = _enum(OwnerCommandDelivery.values, row.delivery);
    final terminal = _enum(OwnerCommandTerminal.values, row.terminal);
    if (terminal != OwnerCommandTerminal.none &&
            delivery != OwnerCommandDelivery.consumed ||
        terminal == OwnerCommandTerminal.unknown && !row.started ||
        terminal == OwnerCommandTerminal.cancelled && row.started ||
        delivery == OwnerCommandDelivery.consumed &&
            terminal == OwnerCommandTerminal.none &&
            !row.started) {
      throw const FormatException('Inconsistent owner command state');
    }
    return OwnerCommandSnapshot(
      key: key,
      submission: submission,
      delivery: delivery,
      started: row.started,
      terminal: terminal,
    );
  }

  static OwnerCommandRead read(
    host.ResponseReader response, {
    required Uint8List expectedKey,
  }) {
    final payload = response.payload;
    try {
      final state = command(response, expectedKey: expectedKey);
      if (payload != null &&
          (payload.length > 128 * 1024 ||
              payload.isNotEmpty &&
                  (state.delivery != OwnerCommandDelivery.consumed ||
                      !state.started ||
                      state.terminal != OwnerCommandTerminal.none))) {
        throw const FormatException('Unexpected owner command payload');
      }
      return OwnerCommandRead(
        snapshot: state,
        payload: payload == null || payload.isEmpty ? null : payload,
      );
    } finally {
      // capnproto_dart data access allocates a mutable copy, separate from the
      // received frame. Erase it on success and all decoder rejection paths.
      payload?.fillRange(0, payload.length, 0);
    }
  }
}
