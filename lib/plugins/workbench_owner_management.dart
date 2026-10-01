import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'package:crypto/crypto.dart';

/// A transient invocation of the existing trusted supervisor. No guest/replay.
final class WorkbenchOwnerManager {
  WorkbenchOwnerManager({
    required this.supervisor,
    required this.host,
    required this.package,
    required this.directory,
  });
  final String supervisor, host, package;
  final Directory directory;
  WorkbenchOwnerPreview? _unconfirmedRecovery;
  Future<String> get profile async {
    var path = (await directory.resolveSymbolicLinks())
        .replaceAll('\\', '/')
        .toLowerCase();
    if (path.startsWith('//?/unc/')) {
      path = '//${path.substring(8)}';
    } else if (path.startsWith('//?/')) {
      path = path.substring(4);
    }
    return sha256.convert(utf8.encode(path)).toString();
  }

  Future<Map<String, dynamic>> _invoke(List<String> args) async {
    if (!Platform.isWindows) {
      throw StateError('Owner management requires Windows');
    }
    final process = await Process.start(supervisor, args);
    Future<List<int>> collect(Stream<List<int>> stream) async {
      final bytes = <int>[];
      await for (final part in stream) {
        if (bytes.length + part.length > 65536) {
          throw const FormatException('Owner management output budget');
        }
        bytes.addAll(part);
      }
      return bytes;
    }

    try {
      final results = await Future.wait<Object>([
        process.exitCode,
        collect(process.stdout),
        collect(process.stderr),
      ]).timeout(const Duration(seconds: 10));
      final value = jsonDecode(utf8.decode(results[1] as List<int>));
      if (value is! Map<String, dynamic> ||
          value['version'] != 1 ||
          value['business_outcome'] != 'Unknown') {
        throw const FormatException('Invalid owner management result');
      }
      if (results[0] != 0) {
        throw WorkbenchOwnerManagementUnconfirmed(
          value['reason'] as String? ?? 'recovery_result_unconfirmed',
          detail: '${value['detail'] ?? value['reason']}',
        );
      }
      if (value['profile'] != await profile) {
        throw const FormatException('Owner management profile mismatch');
      }
      final record = value['record'];
      if (record is! Map<String, dynamic> ||
          record['profile'] != value['profile'] ||
          record['business_outcome'] != 'Unknown') {
        throw const FormatException('Invalid original owner');
      }
      for (final key in ['original_digest', 'preview_token', 'request_token']) {
        if (value[key] != null &&
            (value[key] is! String ||
                !RegExp(r'^[a-f0-9]{64}$').hasMatch(value[key] as String))) {
          throw const FormatException('Invalid owner recovery binding');
        }
      }
      if (record['version'] == 2) {
        for (final item in [
          (host, 'host_sha256'),
          (package, 'package_sha256'),
          (supervisor, 'supervisor_sha256'),
        ]) {
          if (record[item.$2] !=
              sha256.convert(await File(item.$1).readAsBytes()).toString()) {
            // Artifact drift is a legitimate refusal, never an eligible preview.
            if (value['eligible'] == true ||
                value['recovered'] == true ||
                value['resource_recovery_confirmed'] == true ||
                record['phase'] == 'Recovered') {
              throw const WorkbenchOwnerManagementUnconfirmed(
                'artifact_binding_mismatch',
              );
            }
          }
        }
      }
      return Map.unmodifiable(value);
    } catch (_) {
      // Only this transient command is killed. A commit may have happened: re-preview,
      // never infer success, launch a new owner or retry recovery automatically.
      process.kill();
      rethrow;
    }
  }

  Future<WorkbenchOwnerPreview> preview() async {
    final preview = WorkbenchOwnerPreview(
      await _invoke(['--owner-preview', directory.path, host, package]),
    );
    final confirmation = preview.confirmedRecovery;
    if (confirmation != null && _unconfirmedRecovery != null) {
      final pending = _unconfirmedRecovery!;
      if (preview.value['recovery_original_digest'] !=
              pending.value['original_digest'] ||
          !ownerIdentityFields.every(
            (key) => preview.record[key] == pending.record[key],
          )) {
        throw const WorkbenchOwnerManagementUnconfirmed('stale_owner_preview');
      }
    }
    return preview;
  }

  Future<WorkbenchOwnerRecovery> recover(WorkbenchOwnerPreview preview) async {
    if (!preview.eligible || preview.record['profile'] != await profile) {
      throw const WorkbenchOwnerManagementUnconfirmed('stale_owner_preview');
    }
    _unconfirmedRecovery = preview;
    final WorkbenchOwnerRecovery result;
    try {
      result = WorkbenchOwnerRecovery(
        await _invoke([
          '--owner-recover',
          directory.path,
          host,
          package,
          preview.token,
          '--acknowledge-unknown',
        ]),
      );
    } on WorkbenchOwnerManagementUnconfirmed {
      rethrow;
    } catch (error) {
      throw WorkbenchOwnerManagementUnconfirmed(
        'recovery_result_unconfirmed',
        detail: error.toString(),
      );
    }
    if (!result.hasOriginalResourceProof ||
        result.record['phase'] != 'Recovered' ||
        result.value['request_token'] != preview.token ||
        result.value['original_digest'] != preview.value['original_digest'] ||
        result.record['normal_shutdown'] != false ||
        result.record['business_gate_revoked'] != true ||
        !ownerResourceProof.every((k) => result.record[k] == true)) {
      throw const WorkbenchOwnerManagementUnconfirmed(
        'recovery_result_unconfirmed',
        detail: 'Resource recovery lacks durable original proof',
      );
    }
    for (final key in ownerIdentityFields) {
      if (result.record[key] != preview.record[key]) {
        throw const WorkbenchOwnerManagementUnconfirmed(
          'recovery_result_unconfirmed',
          detail: 'Recovery changed original identity',
        );
      }
    }
    _unconfirmedRecovery = null;
    return result;
  }
}

final class WorkbenchOwnerManagementUnconfirmed implements Exception {
  const WorkbenchOwnerManagementUnconfirmed(this.reason, {this.detail});
  final String reason;
  final String? detail;
  @override
  String toString() =>
      'Owner recovery remains unconfirmed: $reason${detail == null ? '' : ': $detail'}';
}

const ownerIdentityFields = [
  'profile',
  'generation',
  'incarnation',
  'supervisor_pid',
  'supervisor_creation_filetime',
  'child_pid',
  'child_creation_filetime',
  'host_sha256',
  'package_sha256',
  'supervisor_sha256',
];

bool _digest(Object? value) =>
    value is String && RegExp(r'^[a-f0-9]{64}$').hasMatch(value);

const ownerResourceProof = [
  'resource_reclaimed',
  'gate_closed',
  'tree_empty',
  'child_exit',
  'stdout_eof',
  'stderr_eof',
  'control_reaped',
];

final class WorkbenchOwnerPreview {
  WorkbenchOwnerPreview(this.value);
  final Map<String, dynamic> value;
  Map<String, dynamic> get record => value['record'] as Map<String, dynamic>;
  bool get eligible => value['eligible'] == true;
  String get token => value['preview_token'] as String;
  String get reason => value['reason'] as String;

  /// Only the trusted CLI's read-only, history-validated confirmation qualifies.
  /// A phase label alone never clears the UI owner after an uncertain commit.
  WorkbenchOwnerRecovery? get confirmedRecovery {
    if (reason != 'already_resource_recovered') return null;
    final metadata = record['recovery'];
    if (value['resource_recovery_confirmed'] != true ||
        eligible ||
        value['version'] != 1 ||
        value['business_outcome'] != 'Unknown' ||
        value['profile'] != record['profile'] ||
        !_digest(value['preview_token']) ||
        !_digest(value['original_digest']) ||
        metadata is! Map<String, dynamic> ||
        metadata['version'] != 1 ||
        !_digest(metadata['original_digest']) ||
        !_digest(metadata['receipt_digest']) ||
        !_digest(metadata['history_head']) ||
        value['history_head'] != metadata['history_head'] ||
        value['recovery_original_digest'] != metadata['original_digest']) {
      throw const WorkbenchOwnerManagementUnconfirmed(
        'recovery_result_unconfirmed',
      );
    }
    final proof = WorkbenchOwnerRecovery(
      Map.unmodifiable({...value, 'recovered': true}),
    );
    if (!proof.hasOriginalResourceProof) {
      throw const WorkbenchOwnerManagementUnconfirmed(
        'original_resource_proof_missing',
      );
    }
    return proof;
  }
}

final class WorkbenchOwnerRecovery {
  WorkbenchOwnerRecovery(this.value);
  final Map<String, dynamic> value;
  Map<String, dynamic> get record => value['record'] as Map<String, dynamic>;
  bool get recovered => value['recovered'] == true;
  bool get hasOriginalResourceProof =>
      recovered &&
      record['version'] == 2 &&
      record['phase'] == 'Recovered' &&
      record['business_outcome'] == 'Unknown' &&
      record['normal_shutdown'] == false &&
      record['business_gate_revoked'] == true &&
      record['protocol_ack'] == 'not_applicable' &&
      record['child_exit_code'] is int &&
      ownerResourceProof.every((key) => record[key] == true) &&
      [
        'profile',
        'incarnation',
        'host_sha256',
        'package_sha256',
        'supervisor_sha256',
      ].every((key) => _digest(record[key])) &&
      [
        'generation',
        'supervisor_pid',
        'child_pid',
        'supervisor_creation_filetime',
        'child_creation_filetime',
      ].every((key) => record[key] is int && (record[key] as int) > 0);
}
