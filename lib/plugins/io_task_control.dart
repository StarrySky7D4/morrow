import 'dart:convert';
import 'dart:typed_data';

import 'generated/host.capnp.dart' as host;

Uint8List _owned(List<int> bytes) =>
    Uint8List.fromList(bytes).asUnmodifiableView();

class HttpTaskHeader {
  HttpTaskHeader({required this.name, required Uint8List value})
    : value = _owned(value);
  final String name;

  /// Binary header data; never decoded as UTF-8 or treated as a credential.
  final Uint8List value;
}

/// An explicit attempt identity, selected approval and origin-relative request.
/// Construct a fresh submission only for a new user-authorized attempt; never
/// automatically replay an attempt whose acknowledgement was lost.
class HttpTaskRequest {
  HttpTaskRequest({
    required Uint8List submission,
    required Uint8List endpoint,
    required this.endpointRevision,
    required Uint8List packageDigest,
    required this.registryRevision,
    required this.method,
    required this.target,
    required List<HttpTaskHeader> headers,
    required Uint8List body,
    required this.timeoutMs,
  }) : submission = _owned(submission),
       endpoint = _owned(endpoint),
       packageDigest = _owned(packageDigest),
       headers = List.unmodifiable(headers),
       body = _owned(body);
  final Uint8List submission, endpoint, packageDigest, body;
  final BigInt endpointRevision, registryRevision;
  final String method, target;
  final List<HttpTaskHeader> headers;
  final int timeoutMs;
}

enum IoStoragePhase {
  local,
  running,
  stopping,
  reclaimed,
  recoveryRequired,
  unavailable,
}

enum IoDeliveryPhase { absent, pending, ready, consumed, unavailable }

enum IoJobError {
  none,
  invalidOptions,
  busy,
  closed,
  unavailable,
  consumed,
  readBound,
  limit,
  spawn,
  disconnect,
}

enum IoExecutionFault {
  none,
  taskProtocol,
  deadline,
  packageBinding,
  inactiveConnection,
  invalidModule,
  unsupportedAbi,
  limits,
  cancelled,
  trap,
}

enum IoHttpStatus {
  invalid,
  accepted,
  pending,
  completed,
  denied,
  revoked,
  expired,
  unsupported,
  quota,
  notFound,
  conflict,
  cancelled,
  outcomeUnknown,
  evidenceUnavailable,
  failed,
}

class IoTaskExit {
  const IoTaskExit({
    required this.execution,
    required this.disconnect,
    required this.maintenance,
  });
  final IoJobError execution, disconnect, maintenance;
}

class IoTaskSnapshot {
  IoTaskSnapshot({
    Uint8List? key,
    Uint8List? submission,
    required this.storage,
    required this.delivery,
    required this.exit,
  }) : key = key == null ? null : _owned(key),
       submission = submission == null ? null : _owned(submission);
  final Uint8List? key, submission;
  final IoStoragePhase storage;
  final IoDeliveryPhase delivery;

  /// Present only after actual worker exit, never inferred from ready delivery.
  final IoTaskExit? exit;
}

class HttpTaskOutcome {
  HttpTaskOutcome({
    required this.status,
    required this.httpStatus,
    required List<HttpTaskHeader> headers,
    required Uint8List body,
  }) : headers = List.unmodifiable(headers),
       body = _owned(body);
  final IoHttpStatus status;

  /// Nonzero only for completed transport; a 4xx/5xx response is still completed.
  final int httpStatus;
  final List<HttpTaskHeader> headers;
  final Uint8List body;
}

class IoTaskResult {
  const IoTaskResult({
    required this.cancelled,
    required this.unknown,
    required this.calls,
    required this.chargedBytes,
    required this.executionFault,
    required this.exitCode,
    required this.http,
  });
  final bool cancelled, unknown;
  final BigInt calls, chargedBytes;
  final IoExecutionFault executionFault;
  final int exitCode;
  final HttpTaskOutcome? http;
}

class IoTaskRead {
  const IoTaskRead({required this.snapshot, required this.result});
  final IoTaskSnapshot snapshot;
  final IoTaskResult? result;
}

abstract interface class WorkbenchIoTaskControl {
  Future<IoTaskSnapshot> startHttp(HttpTaskRequest request);
  Future<IoTaskSnapshot> ioStatus();
  Future<IoTaskSnapshot> pollIo(Uint8List key);
  Future<IoTaskRead> readIo(Uint8List key);
  Future<IoTaskSnapshot> cancelIo(Uint8List key);
  Future<IoTaskSnapshot> repairIo(Uint8List key);
  Future<IoTaskSnapshot> acknowledgeIo(Uint8List key);
}

/// Codec for the private trusted application channel, not a guest SDK.
abstract final class HttpTaskCodec {
  static final _maxUint64 = (BigInt.one << 64) - BigInt.one;
  static const _deniedHeaders = {
    'host',
    'authorization',
    'cookie',
    'connection',
    'content-length',
    'transfer-encoding',
    'upgrade',
    'te',
    'trailer',
    'expect',
    'proxy-authorization',
    'proxy-connection',
    'keep-alive',
  };

  static Uint8List identity(Uint8List bytes) {
    if (bytes.length != 32 || bytes.every((v) => v == 0)) {
      throw const FormatException('Invalid IO identity');
    }
    return _owned(bytes);
  }

  static Uint8List? _optionalIdentity(Uint8List? bytes) =>
      bytes == null || bytes.isEmpty ? null : identity(bytes);
  static void _revision(BigInt value, {bool allowZero = false}) {
    if (value < (allowZero ? BigInt.zero : BigInt.one) || value > _maxUint64) {
      throw const FormatException('Invalid IO revision');
    }
  }

  static T _enum<T>(List<T> values, int code) {
    if (code < 0 || code >= values.length) {
      throw const FormatException('Unknown IO state');
    }
    return values[code];
  }

  static void _headers(List<HttpTaskHeader> headers, {required bool request}) {
    if (headers.length > 64) {
      throw const FormatException('Too many HTTP headers');
    }
    var total = 0;
    for (final header in headers) {
      if (!RegExp(
            r"^[A-Za-z0-9!#$%&'*+.^_`|~-]{1,128}$",
          ).hasMatch(header.name) ||
          header.value.length > 8192 ||
          header.value.any((v) => (v < 0x20 && v != 9) || v == 0x7f)) {
        throw const FormatException('Invalid HTTP header');
      }
      final name = header.name.toLowerCase();
      if (request &&
          (_deniedHeaders.contains(name) || name.startsWith('proxy-'))) {
        throw const FormatException('HTTP header belongs to the runtime');
      }
      total += header.name.length + header.value.length;
      if (total > 16384) {
        throw const FormatException('HTTP headers exceed limit');
      }
    }
  }

  static void validateRequest(HttpTaskRequest request) {
    identity(request.submission);
    identity(request.endpoint);
    identity(request.packageDigest);
    _revision(request.endpointRevision);
    _revision(request.registryRevision, allowZero: true);
    final target = utf8.encode(request.target);
    if (!const [
          'GET',
          'HEAD',
          'POST',
          'PUT',
          'PATCH',
          'DELETE',
          'OPTIONS',
        ].contains(request.method) ||
        target.isEmpty ||
        target.length > 2048 ||
        !request.target.startsWith('/') ||
        request.target.startsWith('//') ||
        request.target.contains('\\') ||
        request.target.contains('://') ||
        target.any((v) => v < 0x21 || v == 0x7f || v == 0x23) ||
        request.body.length > 65536 ||
        request.timeoutMs < 1 ||
        request.timeoutMs > 30000) {
      throw const FormatException('Invalid HTTP task request');
    }
    _headers(request.headers, request: true);
  }

  static void writeRequest(HttpTaskRequest request, host.HttpStartBuilder out) {
    // Validate before any narrowing conversion or field write can truncate input.
    validateRequest(request);
    out.submission = request.submission;
    out.endpoint = request.endpoint;
    out.endpointRevision = request.endpointRevision.toSigned(64).toInt();
    out.packageDigest = request.packageDigest;
    out.registryRevision = request.registryRevision.toSigned(64).toInt();
    out.method = request.method;
    out.target = request.target;
    out.timeoutMs = request.timeoutMs;
    out.body = request.body;
    final headers = out.initHeaders(request.headers.length);
    for (var i = 0; i < request.headers.length; i++) {
      headers[i].name = request.headers[i].name;
      headers[i].value = request.headers[i].value;
    }
  }

  static IoTaskSnapshot snapshot(host.IoStateReader? row) {
    if (row == null) throw const FormatException('Missing IO state');
    final key = _optionalIdentity(row.key);
    final submission = _optionalIdentity(row.submission);
    final storage = _enum(IoStoragePhase.values, row.storage);
    final delivery = _enum(IoDeliveryPhase.values, row.delivery);
    final execution = _enum(IoJobError.values, row.execution);
    final disconnect = _enum(IoJobError.values, row.disconnect);
    final maintenance = _enum(IoJobError.values, row.maintenance);
    if ((!row.hasExit &&
            (execution != IoJobError.none ||
                disconnect != IoJobError.none ||
                maintenance != IoJobError.none)) ||
        (key == null &&
            (delivery != IoDeliveryPhase.absent ||
                row.hasExit ||
                storage == IoStoragePhase.running ||
                storage == IoStoragePhase.stopping ||
                storage == IoStoragePhase.reclaimed))) {
      throw const FormatException('Inconsistent IO state');
    }
    return IoTaskSnapshot(
      key: key,
      submission: submission,
      storage: storage,
      delivery: delivery,
      exit: row.hasExit
          ? IoTaskExit(
              execution: execution,
              disconnect: disconnect,
              maintenance: maintenance,
            )
          : null,
    );
  }

  static IoTaskResult? result(host.IoResultReader? row) {
    if (row == null) return null;
    final headers = row.headers;
    final body = row.body ?? Uint8List(0);
    if (headers != null && headers.length > 64 || body.length > 65536) {
      throw const FormatException('HTTP result exceeds limit');
    }
    if (!row.present) {
      if (row.cancelled ||
          row.unknown ||
          row.calls != 0 ||
          row.chargedBytes != 0 ||
          row.executionFault != 0 ||
          row.exitCode != 0 ||
          row.hasHttp ||
          row.status != 0 ||
          row.httpStatus != 0 ||
          (headers?.length ?? 0) != 0 ||
          body.isNotEmpty) {
        throw const FormatException('Inconsistent absent IO result');
      }
      return null;
    }
    final fault = _enum(IoExecutionFault.values, row.executionFault);
    if (fault != IoExecutionFault.none && row.exitCode != 0) {
      throw const FormatException('Inconsistent IO execution result');
    }
    HttpTaskOutcome? http;
    if (row.hasHttp) {
      final status = _enum(IoHttpStatus.values, row.status);
      final values = <HttpTaskHeader>[
        for (final h in headers ?? <host.HttpHeaderReader>[])
          HttpTaskHeader(name: h.name ?? '', value: h.value ?? Uint8List(0)),
      ];
      _headers(values, request: false);
      if (status == IoHttpStatus.invalid ||
          (status == IoHttpStatus.completed
              ? row.httpStatus < 100 || row.httpStatus > 599
              : row.httpStatus != 0 || values.isNotEmpty || body.isNotEmpty)) {
        throw const FormatException('Inconsistent HTTP result');
      }
      http = HttpTaskOutcome(
        status: status,
        httpStatus: row.httpStatus,
        headers: values,
        body: body,
      );
    } else if (row.status != 0 ||
        row.httpStatus != 0 ||
        (headers?.length ?? 0) != 0 ||
        body.isNotEmpty) {
      throw const FormatException('HTTP data without HTTP result');
    }
    return IoTaskResult(
      cancelled: row.cancelled,
      unknown: row.unknown,
      calls: BigInt.from(row.calls),
      chargedBytes: BigInt.from(row.chargedBytes).toUnsigned(64),
      executionFault: fault,
      exitCode: row.exitCode,
      http: http,
    );
  }
}
