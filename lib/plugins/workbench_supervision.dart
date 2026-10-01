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
