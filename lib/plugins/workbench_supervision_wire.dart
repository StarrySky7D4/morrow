import 'dart:convert' show utf8;
import 'dart:typed_data';
import 'package:capnproto_dart/capnproto_dart.dart';
import 'generated/workbench_supervision.capnp.dart' as wire;
import 'generated/workbench_supervision_identity.dart';

// Local UI projections only. No JSON conversion or dual durable representation.
const _closedFields = <String>[
  'resource_reclaimed',
  'gate_closed',
  'tree_empty',
  'child_exit',
  'stdout_eof',
  'stderr_eof',
  'control_reaped',
];
const _identityFields = <String>[
  'profile',
  'generation',
  'incarnation',
  'host_sha256',
  'package_sha256',
  'supervisor_sha256',
  'supervisor_pid',
  'supervisor_creation_filetime',
  'child_pid',
  'child_creation_filetime',
  'recovery_history_head',
];
Never _bad(String text) =>
    throw FormatException('Supervisor binary protocol: $text');
void _shape(StructReader? r, int data, int pointers) {
  if (r == null || r.raw.dataWords != data || r.raw.ptrWords != pointers) {
    _bad('missing or changed structure');
  }
}

String _digest(Uint8List? value, {bool empty = false}) {
  if (value == null) {
    _bad('missing digest');
  }
  if (value.length != 32 && !(empty && value.isEmpty)) _bad('digest length');
  return value.map((v) => v.toRadixString(16).padLeft(2, '0')).join();
}

int _exact(BigInt value, {bool zero = false}) {
  final n = value.toInt();
  if (BigInt.from(n) != value || n < 0 || (!zero && n == 0)) {
    _bad('integer cannot be represented exactly on this runtime');
  }
  return n;
}

bool? _truth(wire.ObservedTruth? truth) => switch (truth) {
  wire.ObservedTruth.unspecified => null,
  wire.ObservedTruth.no => false,
  wire.ObservedTruth.yes => true,
  _ => _bad('unknown observation'),
};
Map<String, dynamic> _identity(wire.IdentityReader? r) {
  _shape(r, wire.identityFactory.dataWords, wire.identityFactory.ptrWords);
  return {
    'profile': _digest(r!.profile),
    'generation': _exact(r.generationBigInt),
    'incarnation': _digest(r.incarnation),
    'host_sha256': _digest(r.hostSha256),
    'package_sha256': _digest(r.packageSha256),
    'supervisor_sha256': _digest(r.supervisorSha256),
    'supervisor_pid': r.supervisorPid,
    'supervisor_creation_filetime': _exact(r.supervisorCreationFiletimeBigInt),
    'child_pid': r.childPid,
    'child_creation_filetime': _exact(
      r.childCreationFiletimeBigInt,
      zero: true,
    ),
    'recovery_history_head': _digest(r.recoveryHistoryHead, empty: true),
  };
}

Map<String, dynamic> _facts(wire.ResourceFactsReader? r) {
  _shape(
    r,
    wire.resourceFactsFactory.dataWords,
    wire.resourceFactsFactory.ptrWords,
  );
  final exit = r!.childExitCode;
  _shape(
    exit,
    wire.exitObservationFactory.dataWords,
    wire.exitObservationFactory.ptrWords,
  );
  final int? code = switch (exit!.which) {
    0 => null,
    1 => exit.code,
    _ => _bad('exit discriminant'),
  };
  return {
    'resource_reclaimed': r.resourceReclaimed,
    'gate_closed': r.gateClosed,
    'tree_empty': r.treeEmpty,
    'child_exit': r.childExit,
    'stdout_eof': r.stdoutEof,
    'stderr_eof': r.stderrEof,
    'control_reaped': r.controlReaped,
    'business_gate_revoked': r.businessGateRevoked,
    'normal_shutdown': _truth(r.normalShutdown),
    'child_exit_code': code,
  };
}

Map<String, dynamic> _joins(wire.ReaderJoinsReader r) {
  _shape(
    r,
    wire.readerJoinsFactory.dataWords,
    wire.readerJoinsFactory.ptrWords,
  );
  if (r.version != 1) _bad('reader join version');
  return Map<String, dynamic>.unmodifiable({
    'version': 1,
    'stdout_reader_joined': r.stdoutReaderJoined,
    'stderr_reader_joined': r.stderrReaderJoined,
    'controller_reader_joined': r.controllerReaderJoined,
  });
}

void _unknown(wire.BusinessOutcome? b, wire.ProtocolAck? a) {
  if (b != wire.BusinessOutcome.unknown ||
      a != wire.ProtocolAck.notApplicable) {
    _bad('unsupported outcome or acknowledgement');
  }
}

bool _closed(Map<String, dynamic> r) =>
    _closedFields.every((k) => r[k] == true);
bool _abnormal(Map<String, dynamic> r) =>
    _closed(r) &&
    r['business_gate_revoked'] == true &&
    r['normal_shutdown'] == false &&
    r['child_exit_code'] is int;
Map<String, dynamic> _owner(wire.OwnerRecordReader? r) {
  _shape(
    r,
    wire.ownerRecordFactory.dataWords,
    wire.ownerRecordFactory.ptrWords,
  );
  if (r!.modelVersion != 2) _bad('owner model version');
  _unknown(r.businessOutcome, r.protocolAck);
  final phase = switch (r.phase) {
    wire.OwnerPhase.pending => 'Pending',
    wire.OwnerPhase.active => 'Active',
    wire.OwnerPhase.closing => 'Closing',
    wire.OwnerPhase.released => 'Released',
    wire.OwnerPhase.recovered => 'Recovered',
    _ => _bad('owner phase'),
  };
  final value = <String, dynamic>{
    'version': 2,
    ..._identity(r.identity),
    'phase': phase,
    ..._facts(r.resources),
    'business_outcome': 'Unknown',
    'protocol_ack': 'not_applicable',
  };
  if (value['supervisor_pid'] == 0 ||
      (phase != 'Pending' &&
          (value['child_pid'] == 0 || value['child_creation_filetime'] == 0))) {
    _bad('process identity');
  }
  if (r.readerJoins != null) {
    value['abnormal_resource_closure_proof'] = _joins(r.readerJoins!);
  }
  if (phase == 'Released' &&
      (!_closed(value) ||
          value['normal_shutdown'] != true ||
          value['child_exit_code'] != 0)) {
    _bad('normal release lacks original proof');
  }
  if (phase == 'Recovered') {
    final b = r.recovery;
    _shape(
      b,
      wire.recoveryBindingFactory.dataWords,
      wire.recoveryBindingFactory.ptrWords,
    );
    if (!_abnormal(value) || b!.version != 1) {
      _bad('incomplete abnormal recovery');
    }
    value['recovery'] = Map<String, dynamic>.unmodifiable({
      'version': 1,
      'original_digest': _digest(b.originalDigest),
      'receipt_digest': _digest(b.receiptDigest),
      'history_head': _digest(b.historyHead),
    });
  } else if (r.recovery != null) {
    _bad('recovery on unrecovered owner');
  }
  return value;
}

wire.EnvelopeReader _envelope(List<int> input) {
  if (input.length < 8 ||
      input.length > 65536 ||
      input.any((v) => v < 0 || v > 255)) {
    _bad('packet bound');
  }
  final bytes = Uint8List.fromList(input),
      view = ByteData.sublistView(Uint8List.fromList(input));
  final count = view.getUint32(0, Endian.little) + 1;
  if (count > 64) _bad('segment count');
  var total = ((count + 2) ~/ 2) * 8;
  if (total > bytes.length) _bad('segment header');
  for (var i = 0; i < count; i++) {
    total += view.getUint32(4 + i * 4, Endian.little) * 8;
    if (total > bytes.length) _bad('segment bounds');
  }
  if (total != bytes.length) _bad('trailing packet bytes');
  final message = MessageReader.deserialize(
    bytes,
    const MessageReaderOptions(
      traversalLimitInWords: 16384,
      nestingLimit: 16,
      maxSegments: 64,
    ),
  );
  final r = message.getRoot(wire.envelopeFactory);
  _shape(r, wire.envelopeFactory.dataWords, wire.envelopeFactory.ptrWords);
  if (r.wireVersion != 1 ||
      _digest(r.schemaSha256) != supervisionSchemaSha256) {
    _bad('version or compiled schema');
  }
  return r;
}

Map<String, dynamic> decodeWorkbenchSupervisionStatus(List<int> bytes) {
  final envelope = _envelope(bytes);
  if (envelope.which != 0) _bad('expected status');
  final s = envelope.status;
  _shape(s, wire.statusFactory.dataWords, wire.statusFactory.ptrWords);
  final value = _owner(s!.record), ownerPhase = value['phase'];
  final phase = switch (s.phase) {
    wire.StatusPhase.running => 'Running',
    wire.StatusPhase.closing => 'Closing',
    wire.StatusPhase.released => 'Released',
    wire.StatusPhase.closingUnconfirmed => 'ClosingUnconfirmed',
    _ => _bad('status phase'),
  };
  final disposition = switch (s.ownerDisposition) {
    wire.OwnerDisposition.active => 'Active',
    wire.OwnerDisposition.closing => 'Closing',
    wire.OwnerDisposition.released => 'Released',
    wire.OwnerDisposition.retained => 'Retained',
    _ => _bad('owner disposition'),
  };
  final binding = {
    'Running': ['Active', 'Active'],
    'Closing': ['Closing', 'Closing'],
    'Released': ['Released', 'Released'],
    'ClosingUnconfirmed': ['Closing', 'Retained'],
  }[phase]!;
  final unconfirmedNormalCommit =
      phase == 'ClosingUnconfirmed' &&
      ownerPhase == 'Released' &&
      !s.durableProof;
  if ((!unconfirmedNormalCommit && ownerPhase != binding[0]) ||
      disposition != binding[1] ||
      value['child_pid'] == 0 ||
      value['child_creation_filetime'] == 0 ||
      ![0, 1].contains(s.abnormalResourceClosureProtocol) ||
      (phase == 'Released' && !s.durableProof)) {
    _bad('status owner binding');
  }
  value.addAll({
    'phase': phase,
    'owner_phase': disposition,
    'durable_proof': s.durableProof,
    'abnormal_resource_closure_protocol': s.abnormalResourceClosureProtocol,
  });
  final p = s.prepared;
  if (p != null) {
    _shape(
      p,
      wire.abnormalClosurePreparedFactory.dataWords,
      wire.abnormalClosurePreparedFactory.ptrWords,
    );
    _unknown(p.businessOutcome, p.protocolAck);
    final id = _identity(p.identity),
        facts = _facts(p.resources),
        joins = p.readerJoins;
    if (joins == null) _bad('prepared reader joins');
    final j = _joins(joins);
    if (p.version != 1 ||
        phase != 'ClosingUnconfirmed' ||
        s.abnormalResourceClosureProtocol != 1 ||
        !s.durableProof ||
        !_abnormal(value) ||
        !p.durableAbnormalRecordConfirmed ||
        p.ownerDisposition != wire.OwnerDisposition.retained ||
        p.accepted ||
        p.supervisorExitConfirmed ||
        !_identityFields.every((k) => id[k] == value[k]) ||
        !facts.keys.every((k) => facts[k] == value[k]) ||
        !j.keys.every((k) => j[k] == (k == 'version' ? 1 : true)) ||
        value['abnormal_resource_closure_proof'] is! Map ||
        !j.keys.every(
          (k) => value['abnormal_resource_closure_proof'][k] == j[k],
        )) {
      _bad('prepared proof binding');
    }
    value['abnormal_resource_closure_prepared'] =
        Map<String, dynamic>.unmodifiable({
          'version': 1,
          'kind': 'AbnormalResourceClosurePreparedV1',
          'identity': Map<String, dynamic>.unmodifiable(id),
          'committed_record_sha256': _digest(p.committedRecordSha256),
          ...facts,
          'reader_joins': j,
          'durable_abnormal_record_confirmed': true,
          'owner_phase': 'Retained',
          'business_outcome': 'Unknown',
          'protocol_ack': 'not_applicable',
          'accepted': false,
          'supervisor_exit_confirmed': false,
        });
  }
  return Map<String, dynamic>.unmodifiable(value);
}

Map<String, dynamic> decodeWorkbenchOwnerManagement(
  List<int> bytes, {
  required bool preview,
}) {
  final e = _envelope(bytes);
  if (e.which == 3) {
    final f = e.failure;
    _shape(
      f,
      wire.managementFailureFactory.dataWords,
      wire.managementFailureFactory.ptrWords,
    );
    if (f!.eligible ||
        f.recovered ||
        f.businessOutcome != wire.BusinessOutcome.unknown ||
        (f.reason ?? '').isEmpty ||
        utf8.encode(f.reason!).length > 512 ||
        f.detail == null ||
        utf8.encode(f.detail!).length > 8192) {
      _bad('failure payload');
    }
    return Map<String, dynamic>.unmodifiable({
      'version': 1,
      'eligible': false,
      'recovered': false,
      'reason': f.reason,
      'detail': f.detail,
      'business_outcome': 'Unknown',
    });
  }
  if (e.which != (preview ? 1 : 2)) _bad('management response kind');
  if (preview) {
    final p = e.preview;
    _shape(
      p,
      wire.recoveryPreviewFactory.dataWords,
      wire.recoveryPreviewFactory.ptrWords,
    );
    final record = _owner(p!.record),
        profile = _digest(p.profile),
        reason = p.reason;
    if (p.businessOutcome != wire.BusinessOutcome.unknown ||
        profile != record['profile'] ||
        reason == null ||
        reason.isEmpty ||
        utf8.encode(reason).length > 512 ||
        p.eligible != (reason == 'eligible_resource_only') ||
        p.resourceRecoveryConfirmed !=
            (reason == 'already_resource_recovered') ||
        (p.eligible && (record['phase'] != 'Closing' || !_abnormal(record))) ||
        (p.resourceRecoveryConfirmed && record['phase'] != 'Recovered')) {
      _bad('preview binding');
    }
    final recovered = p.recoveryOriginalDigest;
    if (!p.resourceRecoveryConfirmed &&
        recovered != null &&
        recovered.isNotEmpty) {
      _bad('unexpected recovery digest');
    }
    return Map<String, dynamic>.unmodifiable({
      'version': 1,
      'profile': profile,
      'preview_token': _digest(p.previewToken),
      'original_digest': _digest(p.originalDigest),
      'history_head': _digest(p.historyHead, empty: true),
      'eligible': p.eligible,
      'reason': reason,
      'resource_recovery_confirmed': p.resourceRecoveryConfirmed,
      'recovery_original_digest': p.resourceRecoveryConfirmed
          ? _digest(recovered)
          : null,
      'record': Map<String, dynamic>.unmodifiable(record),
      'business_outcome': 'Unknown',
    });
  }
  final r = e.recovery;
  _shape(
    r,
    wire.recoveryResultFactory.dataWords,
    wire.recoveryResultFactory.ptrWords,
  );
  final record = _owner(r!.record),
      profile = _digest(r.profile),
      digest = _digest(r.originalDigest),
      head = _digest(r.historyHead);
  if (!r.recovered ||
      r.normalShutdown != wire.ObservedTruth.no ||
      r.businessOutcome != wire.BusinessOutcome.unknown ||
      record['phase'] != 'Recovered' ||
      profile != record['profile'] ||
      record['recovery']['original_digest'] != digest ||
      record['recovery']['history_head'] != head) {
    _bad('recovery response binding');
  }
  return Map<String, dynamic>.unmodifiable({
    'version': 1,
    'profile': profile,
    'request_token': _digest(r.requestToken),
    'original_digest': digest,
    'history_head': head,
    'recovered': true,
    'normal_shutdown': false,
    'record': Map<String, dynamic>.unmodifiable(record),
    'business_outcome': 'Unknown',
  });
}
