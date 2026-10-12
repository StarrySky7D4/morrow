import 'dart:convert';

final class WorkbenchSupervisionState {
  WorkbenchSupervisionState(this.record);
  final Map<String, dynamic> record;
  String get phase => record['phase'] as String;
  String get ownerPhase => record['owner_phase'] as String? ?? 'Retained';
  bool get released =>
      phase == 'Released' &&
      ownerPhase == 'Released' &&
      record['normal_shutdown'] == true &&
      record['durable_proof'] == true &&
      record['child_exit_code'] == 0 &&
      [
        'resource_reclaimed',
        'tree_empty',
        'child_exit',
        'stdout_eof',
        'stderr_eof',
        'control_reaped',
        'gate_closed',
      ].every((k) => record[k] == true);
  bool get uncertain => phase == 'ClosingUnconfirmed';
  static WorkbenchSupervisionState decode(List<int> bytes) {
    final value = jsonDecode(utf8.decode(bytes));
    if (value is! Map<String, dynamic> ||
        ![1, 2].contains(value['version']) ||
        ![
          'Running',
          'Closing',
          'Released',
          'ClosingUnconfirmed',
        ].contains(value['phase']) ||
        value['generation'] is! int ||
        (value['generation'] as int) < 1 ||
        value['child_pid'] is! int ||
        (value['child_pid'] as int) < 1 ||
        value['supervisor_pid'] is! int ||
        (value['supervisor_pid'] as int) < 1 ||
        value['protocol_ack'] != 'not_applicable' ||
        value['business_outcome'] != 'Unknown') {
      throw const FormatException('Invalid workbench supervisor status');
    }
    for (final key in [
      'profile',
      'incarnation',
      'host_sha256',
      'package_sha256',
    ]) {
      if (value[key] is! String ||
          !RegExp(r'^[a-f0-9]{64}$').hasMatch(value[key] as String)) {
        throw const FormatException('Invalid supervisor identity');
      }
    }
    if (value['version'] == 2) {
      if (value['supervisor_creation_filetime'] is! int ||
          (value['supervisor_creation_filetime'] as int) <= 0 ||
          value['child_creation_filetime'] is! int ||
          (value['child_creation_filetime'] as int) <= 0 ||
          value['supervisor_sha256'] is! String ||
          !RegExp(
            r'^[a-f0-9]{64}$',
          ).hasMatch(value['supervisor_sha256'] as String)) {
        throw const FormatException(
          'Missing supervisor process creation binding',
        );
      }
    }
    final state = WorkbenchSupervisionState(Map.unmodifiable(value));
    if (state.phase == 'Released' && !state.released) {
      throw const FormatException(
        'Released status lacks durable resource proof',
      );
    }
    return state;
  }
}

final class WorkbenchSupervisionUnconfirmed implements Exception {
  const WorkbenchSupervisionUnconfirmed(this.detail);
  final String detail;
  @override
  String toString() =>
      'Content service outcome is Unknown; original owner retained. $detail';
}

/// Incomplete startup evidence only. No field grants admission or owner release.
final class WorkbenchStartupObservation {
  WorkbenchStartupObservation({
    required this.stage,
    required Object primaryFailure,
    required WorkbenchSupervisionState? initialValidatedState,
    required WorkbenchSupervisionState? latestValidatedState,
    required List<int> diagnosticBytes,
    required this.diagnosticBytesObserved,
    required this.actualExitCode,
    required this.stdoutEof,
    required this.stderrEof,
    Object? secondaryCloseFailure,
    Object? observerFailure,
  }) : initialValidatedState = _copyState(initialValidatedState),
       latestValidatedState = _copyState(latestValidatedState),
       diagnosticBytes = List<int>.unmodifiable(diagnosticBytes),
       primaryFailure = _boundedFailure(primaryFailure),
       secondaryCloseFailure = secondaryCloseFailure == null
           ? null : _boundedFailure(secondaryCloseFailure),
       observerFailure = observerFailure == null
           ? null : _boundedFailure(observerFailure) {
    if (!['before_original_close', 'after_original_close'].contains(stage) ||
        diagnosticBytes.length > diagnosticByteLimit ||
        diagnosticBytesObserved < diagnosticBytes.length ||
        diagnosticBytes.any((byte) => byte < 0 || byte > 255) ||
        (stage == 'before_original_close' && secondaryCloseFailure != null)) {
      throw ArgumentError('Invalid incomplete startup observation');
    }
  }

  static const diagnosticByteLimit = 8192;
  static const failureTextByteLimit = 2048;
  final String stage;
  final Map<String, Object?>? initialValidatedState, latestValidatedState;
  final List<int> diagnosticBytes;
  final int diagnosticBytesObserved;
  final int? actualExitCode;
  final bool stdoutEof, stderrEof;
  final ({String text, bool truncated}) primaryFailure;
  final ({String text, bool truncated})? secondaryCloseFailure, observerFailure;

  // Fixed primitive fields only; unknown/nested frame content is not exported.
  static Map<String, Object?>? _copyState(WorkbenchSupervisionState? state) {
    if (state == null) {
      return null;
    }
    final result = <String, Object?>{};
    for (final key in const [
      'version', 'phase', 'owner_phase', 'profile', 'generation', 'incarnation',
      'child_pid', 'supervisor_pid', 'supervisor_creation_filetime',
      'child_creation_filetime', 'supervisor_sha256', 'host_sha256',
      'package_sha256', 'normal_shutdown', 'durable_proof', 'child_exit_code',
      'resource_reclaimed', 'tree_empty', 'child_exit', 'stdout_eof',
      'stderr_eof', 'control_reaped', 'gate_closed', 'protocol_ack',
      'business_outcome',
    ]) {
      final value = state.record[key];
      if (value == null || value is int || value is bool ||
          (value is String && value.length <= 128)) {
        result[key] = value;
      }
    }
    return Map<String, Object?>.unmodifiable(result);
  }

  static ({String text, bool truncated}) _boundedFailure(Object error) {
    String raw;
    try {
      raw = error.toString();
    } catch (_) {
      raw = 'Failure text unavailable';
    }
    final prefix = StringBuffer();
    var bytes = 0;
    for (final rune in raw.runes) {
      final width = utf8.encode(String.fromCharCode(rune)).length;
      if (bytes + width > failureTextByteLimit) {
        return (text: prefix.toString(), truncated: true);
      }
      prefix.writeCharCode(rune);
      bytes += width;
    }
    return (text: prefix.toString(), truncated: false);
  }

  Map<String, Object?> toJson() => Map<String, Object?>.unmodifiable({
    'version': 1,
    'kind': 'startup_incomplete',
    'snapshot': true,
    'incomplete': true,
    'accepted': false,
    'stage': stage,
    'primary_failure': primaryFailure.text,
    'primary_failure_truncated': primaryFailure.truncated,
    'initial_validated_state': initialValidatedState,
    'latest_validated_state': latestValidatedState,
    'diagnostic_bytes': diagnosticBytes,
    'diagnostic_bytes_observed': diagnosticBytesObserved,
    'diagnostic_bytes_stored': diagnosticBytes.length,
    'diagnostic_bytes_truncated': diagnosticBytesObserved > diagnosticBytes.length,
    'diagnostic_byte_limit': diagnosticByteLimit,
    'actual_exit_known': actualExitCode != null,
    'actual_exit_code': actualExitCode,
    'stdout_eof': stdoutEof,
    'stderr_eof': stderrEof,
    // A settled close may have failed; this is never a release claim.
    'original_close_settled': stage == 'after_original_close',
    'secondary_close_failure': secondaryCloseFailure?.text,
    'secondary_close_failure_truncated': secondaryCloseFailure?.truncated,
    'observer_failure': observerFailure?.text,
    'observer_failure_truncated': observerFailure?.truncated,
  });
}

typedef WorkbenchStartupObservationCapture = WorkbenchStartupObservation Function({
  required String stage,
  required Object primaryFailure,
  Object? secondaryCloseFailure,
  Object? observerFailure,
});

/// Callback failures are evidence failures, never cleanup permission. If the
/// original close stays pending, the first callback has already been attempted.
/// Its own failure remains local; persistence is not guaranteed by this helper.
Future<({bool beforeObserverFailed, bool afterObserverFailed})>
    observeWorkbenchStartupFailure({
  required Object primaryFailure,
  required WorkbenchStartupObservationCapture capture,
  required void Function(WorkbenchStartupObservation) observe,
  required Future<void> Function() closeOriginal,
}) async {
  Object? beforeObserverFailure;
  try {
    observe(capture(
      stage: 'before_original_close', primaryFailure: primaryFailure,
    ));
  } catch (error) {
    beforeObserverFailure = error;
  }
  Object? closeFailure;
  try {
    await closeOriginal();
  } catch (error) {
    closeFailure = error;
  }
  var afterObserverFailed = false;
  try {
    observe(capture(
      stage: 'after_original_close', primaryFailure: primaryFailure,
      secondaryCloseFailure: closeFailure,
      observerFailure: beforeObserverFailure,
    ));
  } catch (_) {
    afterObserverFailed = true;
  }
  return (
    beforeObserverFailed: beforeObserverFailure != null,
    afterObserverFailed: afterObserverFailed,
  );
}
