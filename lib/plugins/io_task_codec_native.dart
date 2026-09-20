import 'dart:typed_data';

import 'generated/host.capnp.dart' as host;
import 'io_task_models.dart';

/// Codec for the private trusted native channel, excluded from Web builds.
abstract final class HttpTaskCodec {
  static Uint8List identity(Uint8List bytes) =>
      HttpTaskValidation.identity(bytes);

  static void validateRequest(HttpTaskRequest request) =>
      HttpTaskValidation.validateRequest(request);

  static Uint8List? _optionalIdentity(Uint8List? bytes) =>
      bytes == null || bytes.isEmpty ? null : identity(bytes);
  static T _enum<T>(List<T> values, int code) {
    if (code < 0 || code >= values.length) {
      throw const FormatException('Unknown IO state');
    }
    return values[code];
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
      HttpTaskValidation.validateHeaders(values, request: false);
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
