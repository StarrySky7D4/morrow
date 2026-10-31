// Synthetic binary wire fixtures only. No kernel or durable proof is created.
import 'dart:typed_data';
import 'package:capnproto_dart/capnproto_dart.dart';
import 'package:morrow_studio/plugins/generated/workbench_supervision.capnp.dart'
    as wire;
import 'package:morrow_studio/plugins/generated/workbench_supervision_identity.dart';

Uint8List? _hex(String? value) => value == null
    ? null
    : Uint8List.fromList([
        for (var i = 0; i < value.length; i += 2)
          int.parse(value.substring(i, i + 2), radix: 16),
      ]);
wire.BusinessOutcome _outcome(Object? v) => v == 'Unknown'
    ? wire.BusinessOutcome.unknown
    : wire.BusinessOutcome.unspecified;
wire.ProtocolAck _ack(Object? v) => v == 'not_applicable'
    ? wire.ProtocolAck.notApplicable
    : wire.ProtocolAck.unspecified;
wire.OwnerDisposition _disposition(Object? v) => switch (v) {
  'Active' => wire.OwnerDisposition.active,
  'Closing' => wire.OwnerDisposition.closing,
  'Released' => wire.OwnerDisposition.released,
  'Retained' => wire.OwnerDisposition.retained,
  _ => wire.OwnerDisposition.unspecified,
};
void _identity(wire.IdentityBuilder b, Map r) {
  b.profile = _hex(r['profile']);
  b.generationBigInt = BigInt.from(r['generation'] ?? 0);
  b.incarnation = _hex(r['incarnation']);
  b.hostSha256 = _hex(r['host_sha256']);
  b.packageSha256 = _hex(r['package_sha256']);
  b.supervisorSha256 = _hex(r['supervisor_sha256']);
  b.supervisorPid = r['supervisor_pid'] ?? 0;
  b.supervisorCreationFiletimeBigInt = BigInt.from(
    r['supervisor_creation_filetime'] ?? 0,
  );
  b.childPid = r['child_pid'] ?? 0;
  b.childCreationFiletimeBigInt = BigInt.from(
    r['child_creation_filetime'] ?? 0,
  );
  b.recoveryHistoryHead = _hex(r['recovery_history_head'] ?? '');
}

void _facts(wire.ResourceFactsBuilder b, Map r) {
  b.resourceReclaimed = r['resource_reclaimed'] == true;
  b.gateClosed = r['gate_closed'] == true;
  b.treeEmpty = r['tree_empty'] == true;
  b.childExit = r['child_exit'] == true;
  b.stdoutEof = r['stdout_eof'] == true;
  b.stderrEof = r['stderr_eof'] == true;
  b.controlReaped = r['control_reaped'] == true;
  b.businessGateRevoked = r['business_gate_revoked'] == true;
  b.normalShutdown = switch (r['normal_shutdown']) {
    true => wire.ObservedTruth.yes,
    false => wire.ObservedTruth.no,
    _ => wire.ObservedTruth.unspecified,
  };
  final exit = b.initChildExitCode();
  if (r['child_exit_code'] == null) {
    exit.selectAbsent();
  } else {
    exit.code = r['child_exit_code'];
  }
}

void _joins(wire.ReaderJoinsBuilder b, Map r) {
  b.version = r['version'];
  b.stdoutReaderJoined = r['stdout_reader_joined'] == true;
  b.stderrReaderJoined = r['stderr_reader_joined'] == true;
  b.controllerReaderJoined = r['controller_reader_joined'] == true;
}

Uint8List encodeSupervisionFixture(Map<String, dynamic> r) {
  final message = MessageBuilder(), e = message.initRoot(wire.envelopeFactory);
  e.wireVersion = r['wire_version'] ?? 1;
  e.schemaSha256 = _hex(supervisionSchemaSha256);
  final prepared = r['abnormal_resource_closure_prepared'] as Map?;
  if (prepared != null &&
      prepared['kind'] != 'AbnormalResourceClosurePreparedV1') {
    // A different typed message cannot masquerade as the prepared status.
    e.initFailure();
    return message.serialize();
  }
  final s = e.initStatus();
  s.phase = switch (r['phase']) {
    'Running' => wire.StatusPhase.running,
    'Closing' => wire.StatusPhase.closing,
    'Released' => wire.StatusPhase.released,
    'ClosingUnconfirmed' => wire.StatusPhase.closingUnconfirmed,
    _ => wire.StatusPhase.unspecified,
  };
  s.ownerDisposition = _disposition(r['owner_phase']);
  s.durableProof = r['durable_proof'] == true;
  s.abnormalResourceClosureProtocol =
      r['abnormal_resource_closure_protocol'] ?? 0;
  final owner = s.initRecord();
  owner.modelVersion = r['version'];
  owner.phase = switch (r['wire_owner_phase'] ?? r['phase']) {
    'Running' => wire.OwnerPhase.active,
    'Closing' || 'ClosingUnconfirmed' => wire.OwnerPhase.closing,
    'Released' => wire.OwnerPhase.released,
    _ => wire.OwnerPhase.unspecified,
  };
  _identity(owner.initIdentity(), r);
  _facts(owner.initResources(), r);
  owner.businessOutcome = _outcome(r['business_outcome']);
  owner.protocolAck = _ack(r['protocol_ack']);
  if (r['abnormal_resource_closure_proof'] case final Map joins) {
    _joins(owner.initReaderJoins(), joins);
  }
  if (prepared != null) {
    final p = s.initPrepared();
    p.version = prepared['version'];
    _identity(p.initIdentity(), prepared['identity']);
    p.committedRecordSha256 = _hex(prepared['committed_record_sha256']);
    _facts(p.initResources(), prepared);
    _joins(p.initReaderJoins(), prepared['reader_joins']);
    p.durableAbnormalRecordConfirmed =
        prepared['durable_abnormal_record_confirmed'] == true;
    p.ownerDisposition = _disposition(prepared['owner_phase']);
    p.businessOutcome = _outcome(prepared['business_outcome']);
    p.protocolAck = _ack(prepared['protocol_ack']);
    p.accepted = prepared['accepted'] == true;
    p.supervisorExitConfirmed = prepared['supervisor_exit_confirmed'] == true;
  }
  return message.serialize();
}
