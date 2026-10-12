// Real pinned native resources; disabled until three explicit defines select it.
// A successful test would prove abnormal resource quarantine, not business success.
import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';
import 'package:crypto/crypto.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';
import 'package:morrow_studio/plugins/workbench_supervision.dart';
import 'package:morrow_studio/plugins/qualification_session_contract.dart';

const _selected = bool.fromEnvironment('MORROW_C28_SESSION_QUALIFICATION_RUN',
  defaultValue: false);
const _rpcSelected = bool.fromEnvironment('MORROW_C28_RPC_FAULT_FIXTURE_TEST', defaultValue: false);
const _rpcError = 'C28 qualified first PageVersioned RPC fixture failure';
const _name = 'real first PageVersioned RPC error preserves original catch and quarantines resources';

void main() {
  if (!qualificationSessionCompiled || !_selected ||
      !_rpcSelected) {
    test(_name, () {}, skip: 'Explicit compiled real RPC fault fixture selection required');
    return;
  }
  final env = Platform.environment;
  if (!Platform.isWindows || ![
    'MORROW_C28_SESSION_PLAN', 'MORROW_C28_SESSION_PLAN_BYTES',
    'MORROW_C28_SESSION_PLAN_SHA256', 'MORROW_C28_SESSION_EVIDENCE',
  ].every((key) => env[key]?.isNotEmpty == true)) {
    test(_name, () {}, skip: 'Explicit Windows plan pins and evidence required');
    return;
  }
  test(_name, () async {
    String requiredEnv(String key) => env[key] ?? (throw StateError('Missing $key'));
    final config = QualificationSessionConfiguration(
      planPath: requiredEnv('MORROW_C28_SESSION_PLAN'),
      planBytes: int.parse(requiredEnv('MORROW_C28_SESSION_PLAN_BYTES')),
      planSha256: requiredEnv('MORROW_C28_SESSION_PLAN_SHA256'));
    config.requireCompiled();
    final evidence = qualificationWindowsPath(requiredEnv('MORROW_C28_SESSION_EVIDENCE'));
    await _plainEvidenceDirectory(evidence);
    if (!(await Directory(evidence).list(followLinks: false).isEmpty)) {
      throw StateError('Fresh empty evidence required; no adoption');
    }
    final plan = QualificationSessionPlan.decode(config, await _readPinnedPlan(config));
    final snapshots = <WorkbenchStartupObservation>[];
    final snapshotPins = <Map<String, Object?>>[];
    Object? persistenceFailure;
    var attempts = 0;
    void observe(WorkbenchStartupObservation observation) {
      final index = attempts++;
      try {
        if (index > 1 || observation.stage !=
            (index == 0 ? 'before_original_close' : 'after_original_close')) {
          throw StateError('Exact two ordered startup snapshots required');
        }
        final raw = utf8.encode(jsonEncode(observation.toJson()));
        final name = 'startup-incomplete-$index.json';
        _saveNewSync('$evidence/$name', raw);
        snapshots.add(observation);
        snapshotPins.add(Map<String, Object?>.unmodifiable({
          'path': name, 'bytes': raw.length, 'sha256': sha256.convert(raw).toString(),
        }));
      } catch (error) { persistenceFailure ??= error; rethrow; }
    }
    RustWorkbench? returned;
    Object? primary, unexpectedClose;
    try {
      returned = await RustWorkbench.open(executable: plan.host.path,
        package: plan.package.path, directory: Directory(plan.profileRoot),
        supervisorExecutable: plan.supervisor.path, qualification: config,
        onQualificationStartupFailure: observe);
    } catch (error) { primary = error; }
    // If the fixture were wrongly disabled, close its original returned owner.
    if (returned != null) {
      try { await returned.close(); } catch (error) { unexpectedClose = error; }
    }
    final after = snapshots.length == 2 ? snapshots[1] : null;
    final receipt = after?.failedStartupQuarantine;
    await _saveNew('$evidence/failed-startup-receipt.json', utf8.encode(jsonEncode({
      'schema': 'C28_REAL_STARTUP_RPC_FAULT_QUARANTINE_OBSERVATION_V1',
      'fixture': 'first-page-versioned-rpc-error-v1',
      'primary_is_exact_rpc_fault': primary is StateError && primary.message == _rpcError,
      'primary_error_message': primary is StateError && primary.message == _rpcError ? _rpcError : null,
      'open_returned': returned != null, 'snapshot_attempts': attempts,
      'snapshot_pins': snapshotPins, 'failed_startup_quarantine': receipt,
      'actual_supervisor_exit_code': after?.actualExitCode,
      'stdout_eof': after?.stdoutEof, 'stderr_eof': after?.stderrEof,
      'original_close_settled': after?.stage == 'after_original_close',
      'business_outcome': 'Unknown', 'owner_phase': 'Retained',
      'normal_shutdown': false, 'accepted': false,
      'SDK26_G04': 'OPEN', 'release_eligible': false,
    })));
    final failedPersistence = persistenceFailure;
    if (failedPersistence != null) throw failedPersistence;
    expect(returned, isNull); expect(unexpectedClose, isNull);
    expect(primary, isA<StateError>());
    expect((primary as StateError).message, _rpcError);
    expect(attempts, 2); expect(snapshots.length, 2);
    expect(snapshots[0].initialValidatedState?['phase'], 'Running');
    expect(after!.actualExitCode, 2);
    expect(after.stdoutEof && after.stderrEof, isTrue);
    expect(after.secondaryCloseFailure, isNotNull);
    expect(after.observerFailure, isNull);
    expect(receipt, isNotNull);
    expect(receipt!['kind'], 'failed-startup-quarantined');
    expect(receipt['actual_supervisor_exit_code'], 2);
    for (final key in ['supervisor_exit_confirmed', 'stdout_eof', 'stderr_eof']) {
      expect(receipt[key], isTrue);
    }
    expect(receipt['business_outcome'], 'Unknown');
    expect(receipt['owner_phase'], 'Retained');
    expect(receipt['normal_shutdown'], isFalse); expect(receipt['accepted'], isFalse);
    final prepared = receipt['prepared'] as Map;
    final identity = prepared['identity'] as Map;
    expect(prepared['kind'], 'AbnormalResourceClosurePreparedV1');
    expect(prepared['durable_abnormal_record_confirmed'], isTrue);
    expect(prepared['normal_shutdown'], isFalse); expect(prepared['accepted'], isFalse);
    expect(prepared['supervisor_exit_confirmed'], isFalse);
    expect(prepared['owner_phase'], 'Retained'); expect(prepared['business_outcome'], 'Unknown');
    expect(prepared['protocol_ack'], 'not_applicable');
    expect(prepared['committed_record_sha256'], matches(RegExp(r'^[0-9a-f]{64}$')));
    for (final key in ['profile','generation','incarnation','host_sha256',
      'package_sha256','supervisor_sha256','child_pid','child_creation_filetime',
      'supervisor_pid','supervisor_creation_filetime']) {
      expect(identity[key], snapshots[0].initialValidatedState![key]);
    }
    expect(identity['host_sha256'], plan.host.sha256);
    expect(identity['supervisor_sha256'], plan.supervisor.sha256);
    expect(identity['package_sha256'], plan.package.sha256);
    expect(identity['child_pid'], isA<int>()); expect(identity['supervisor_pid'], isA<int>());
    expect(identity['child_pid'], isNot(identity['supervisor_pid']));
    expect(identity['recovery_history_head'], isA<String>());
    for (final key in ['resource_reclaimed','gate_closed','business_gate_revoked',
      'tree_empty','child_exit','stdout_eof','stderr_eof','control_reaped']) {
      expect(prepared[key], isTrue);
    }
    final joins = prepared['reader_joins'] as Map;
    expect(joins.length, 4); expect(joins['version'], 1);
    for (final key in ['stdout_reader_joined','stderr_reader_joined','controller_reader_joined']) {
      expect(joins[key], isTrue);
    }
  }, timeout: const Timeout(Duration(minutes: 3)));
}

Future<Uint8List> _readPinnedPlan(QualificationSessionConfiguration config) async {
  final file=File(config.planPath);
  await _plainEvidenceDirectory(config.planPath.substring(0,config.planPath.lastIndexOf('/')));
  if (await FileSystemEntity.type(config.planPath,followLinks:false)!=FileSystemEntityType.file) {
    throw StateError('Plain selected plan file required');
  }
  final before=await file.stat();
  if (before.type!=FileSystemEntityType.file || before.size!=config.planBytes) {
    throw StateError('Bounded selected plan size');
  }
  final handle=await file.open(mode:FileMode.read);
  try {
    if (await handle.length()!=config.planBytes) throw StateError('Selected plan handle size');
    final raw=await handle.read(config.planBytes+1);
    final after=await file.stat();
    if (raw.length!=config.planBytes || await handle.length()!=config.planBytes ||
        after.type!=FileSystemEntityType.file || after.size!=before.size ||
        after.modified!=before.modified || after.changed!=before.changed ||
        await FileSystemEntity.type(config.planPath,followLinks:false)!=FileSystemEntityType.file ||
        sha256.convert(raw).toString()!=config.planSha256) {
      throw StateError('Selected plan changed or external pin mismatch');
    }
    await _plainEvidenceDirectory(config.planPath.substring(0,config.planPath.lastIndexOf('/')));
    return raw;
  } finally { await handle.close(); }
}

Future<void> _plainEvidenceDirectory(String path) async {
  var current=path.substring(0,3);
  final parts=path.substring(3).split('/');
  for(var i=0;i<parts.length;i++) {
    current='$current${i==0?'':'/'}${parts[i]}';
    if(await FileSystemEntity.type(current,followLinks:false)!=FileSystemEntityType.directory) {
      throw StateError('Plain existing evidence directory required');
    }
    var resolved=(await Directory(current).resolveSymbolicLinks()).replaceAll('\\','/').toLowerCase();
    if(resolved.startsWith('//?/')) resolved=resolved.substring(4);
    if(resolved!=current.toLowerCase()) throw StateError('Evidence reparse/alias rejected');
  }
}
Future<void> _saveNew(String path,List<int> raw) async {
  if(raw.length>12*1024*1024) throw StateError('Bounded evidence bytes');
  await _plainEvidenceDirectory(path.substring(0,path.lastIndexOf('/')));
  final file=File(path);
  await file.create(exclusive:true); // CreateNew; preserve partials and never overwrite.
  final handle=await file.open(mode:FileMode.writeOnly);
  try { await handle.writeFrom(raw); await handle.flush(); }
  finally { await handle.close(); }
  final reopened=await file.readAsBytes();
  if(reopened.length!=raw.length || sha256.convert(reopened).toString()!=sha256.convert(raw).toString()) {
    throw StateError('Full evidence reopen byte/SHA mismatch');
  }
}


// Synchronous, finite evidence delivery before original close can await forever.
// This writes diagnostic snapshots only; the final trace/receipt path is unchanged.
void _saveNewSync(String path,List<int> raw) {
  if(raw.length>128*1024) throw StateError('Startup snapshot byte budget');
  final parent=path.substring(0,path.lastIndexOf('/'));
  var current=parent.substring(0,3);
  final parts=parent.substring(3).split('/');
  for(var index=0;index<parts.length;index++) {
    current='$current${index==0?'':'/'}${parts[index]}';
    if(FileSystemEntity.typeSync(current,followLinks:false)!=FileSystemEntityType.directory) {
      throw StateError('Plain existing startup evidence directory required');
    }
    var resolved=Directory(current).resolveSymbolicLinksSync().replaceAll('\\','/').toLowerCase();
    if(resolved.startsWith('//?/')) resolved=resolved.substring(4);
    if(resolved!=current.toLowerCase()) throw StateError('Startup evidence alias rejected');
  }
  final file=File(path)..createSync(exclusive:true);
  final handle=file.openSync(mode:FileMode.writeOnly);
  try { handle.writeFromSync(raw); handle.flushSync(); }
  finally { handle.closeSync(); }
  if(FileSystemEntity.typeSync(path,followLinks:false)!=FileSystemEntityType.file) {
    throw StateError('Startup evidence plain readback');
  }
  final before=file.statSync();
  if(before.size!=raw.length) throw StateError('Startup evidence readback size');
  final reader=file.openSync(mode:FileMode.read);
  late Uint8List reopened;
  try {
    if(reader.lengthSync()!=raw.length) throw StateError('Startup evidence held size');
    reopened=reader.readSync(raw.length+1); // Bounded even if the path changes or grows.
    if(reader.lengthSync()!=raw.length) throw StateError('Startup evidence held growth');
  } finally { reader.closeSync(); }
  final after=file.statSync();
  if(FileSystemEntity.typeSync(path,followLinks:false)!=FileSystemEntityType.file ||
      before.size!=after.size || before.modified!=after.modified || before.changed!=after.changed ||
      reopened.length!=raw.length || sha256.convert(reopened).toString()!=sha256.convert(raw).toString()) {
    throw StateError('Startup evidence readback SHA or metadata');
  }
}
