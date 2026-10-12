// Real original product carrier only, explicitly selected and compiled in.
// This file is SOURCE ONLY until separately authorized; no helper/fake owner.
import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';
import 'package:crypto/crypto.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';
import 'package:morrow_studio/plugins/workbench_supervision.dart';
import 'package:morrow_studio/plugins/qualification_session_contract.dart';
import 'package:morrow_studio/plugins/agent_session_control.dart';
import 'package:morrow_studio/plugins/agent_session_controller.dart';
import 'package:morrow_studio/plugins/agent_session_models.dart';
import 'package:morrow_studio/plugins/agent_session_workspace.dart';
import 'package:morrow_studio/plugins/agent_task_discovery.dart';

const _selected = bool.fromEnvironment('MORROW_C28_SESSION_QUALIFICATION_RUN', defaultValue:false);

void main() {
  // Disabled discovery registers one inert skipped test before Platform, IO,
  // Process or any FFI access. Importing definitions selects no native owner.
  if (!qualificationSessionCompiled || !_selected) {
    test('real selected original supervised Agent session qualification', () {},
      skip:'Explicit compile and native test selection required');
    return;
  }
  final inputEnvironment=Platform.environment;
  if (!Platform.isWindows || !['MORROW_C28_SESSION_PLAN','MORROW_C28_SESSION_PLAN_BYTES',
      'MORROW_C28_SESSION_PLAN_SHA256','MORROW_C28_SESSION_EVIDENCE']
      .every((key)=>inputEnvironment[key]?.isNotEmpty == true)) {
    test('real selected original supervised Agent session qualification', () {},
      skip:'Explicit Windows plan pins and evidence scope required');
    return;
  }
  test('real selected original supervised Agent session qualification', () async {
    if (!Platform.isWindows) throw StateError('Reviewed Windows carrier only');
    final env=Platform.environment;
    String requiredEnv(String name) => env[name] ?? (throw StateError('Missing explicit $name'));
    final config=QualificationSessionConfiguration(
      planPath:requiredEnv('MORROW_C28_SESSION_PLAN'),
      planBytes:int.parse(requiredEnv('MORROW_C28_SESSION_PLAN_BYTES')),
      planSha256:requiredEnv('MORROW_C28_SESSION_PLAN_SHA256'));
    config.requireCompiled();
    final evidence=qualificationWindowsPath(requiredEnv('MORROW_C28_SESSION_EVIDENCE'));
    await _plainEvidenceDirectory(evidence);
    if (!(await Directory(evidence).list(followLinks:false).isEmpty)) {
      throw StateError('Evidence directory must be explicitly prepared and empty');
    }
    final raw=await _readPinnedPlan(config);
    final plan=QualificationSessionPlan.decode(config,raw);
    RustWorkbench? backend;
    QualificationSessionControl? qualification;
    Object? primary, closeFailure, evidenceFailure;
    StackTrace? primaryStack;
    Map<String,dynamic>? openedIdentity, terminalIdentity;
    final observations=<Map<String,Object?>>[];
    final startupSnapshots=<Map<String,Object?>>[];
    var startupSnapshotAttempts=0;
    void saveStartupFailure(WorkbenchStartupObservation observation) {
      // Observation only: never authorizes cleanup, admission, or acceptance.
      final index=startupSnapshotAttempts++;
      try {
        if(index>1) throw StateError('Startup snapshot attempt budget');
        final value=observation.toJson();
        final expectedStage=index==0 ? 'before_original_close' : 'after_original_close';
        if(value['stage']!=expectedStage || value['incomplete']!=true ||
            value['accepted']!=false) {
          throw StateError('Diagnostic snapshot is never a release receipt');
        }
        final name='startup-incomplete-$index.json';
        final bytes=utf8.encode(jsonEncode(value));
        _saveNewSync('$evidence/$name',bytes);
        startupSnapshots.add(Map<String,Object?>.unmodifiable({
          'path':name,'bytes':bytes.length,'sha256':sha256.convert(bytes).toString(),
          'incomplete':true,'accepted':false,
        }));
      } catch(error) {
        evidenceFailure ??= error;
        rethrow; // The original open preserves its primary failure and close path.
      }
    }
    var sequenceCompleted=false, normalCloseConfirmed=false;
    try {
      backend=await RustWorkbench.open(executable:plan.host.path, package:plan.package.path,
        directory:Directory(plan.profileRoot),supervisorExecutable:plan.supervisor.path,
        qualification:config,onQualificationStartupFailure:saveStartupFailure);
      qualification=backend.qualificationSession;
      openedIdentity=qualification.originalIdentity;
      final workspace=backend.agentSessions; // Existing original product facade.
      BigInt? openedGeneration;
      void step(String name) {
        qualification!.checkBusinessBudget();
        final view=workspace.view;
        if (openedGeneration != null && view.session?.phase != AgentSessionPhase.detached) {
          expect(view.session?.generation,openedGeneration);
        }
        observations.add({'step':name,'phase':view.session?.phase.name,
          'submission':view.session?.submission?.toString(),
          'generation':view.session?.generation.toString(),
          'outcome_unknown':view.outcomeUnknown,'last_status':view.lastResult?.status.name});
      }
      final none=await workspace.observe();
      expect(none.kind,AgentTaskKind.none);
      final started=await qualification.start();
      expect(started.storage,QualificationSessionStorage.running);
      final actualKey=List<int>.unmodifiable(qualification.taskKey!);
      final running=await workspace.observe();
      expect(running.kind,AgentTaskKind.runningAgent);
      expect(running.taskKey,actualKey);
      step('actual-running');
      expect((await workspace.attach()).status,AgentSessionStatus.attached);
      step('attached');
      await _complete(workspace,qualification,workspace.open,AgentSessionStatus.opened);
      expect(workspace.view.session?.phase,AgentSessionPhase.opened);
      expect(workspace.view.session!.generation,greaterThan(BigInt.zero));
      openedGeneration=workspace.view.session!.generation;
      step('opened');
      workspace.setDraft(AgentSessionDraft.sessionId,plan.sessionId);
      workspace.setDraft(AgentSessionDraft.eventId,'${plan.sessionId}-event');
      workspace.setDraft(AgentSessionDraft.eventBody,'c28 reviewed session ${plan.runId}');
      await _complete(workspace,qualification,workspace.createSession,AgentSessionStatus.reply);
      final created=workspace.view.selectedInfo!;
      expect(created.sessionId,plan.sessionId);
      expect(created.parent,isNull); expect(created.parentTail,BigInt.zero);
      expect(created.revision,BigInt.one); expect(created.epoch,BigInt.zero);
      expect(created.tail,BigInt.zero); step('created');
      await _complete(workspace,qualification,workspace.listSessions,AgentSessionStatus.reply);
      expect(workspace.view.sessions.map((s)=>s.sessionId).toList(),[plan.sessionId]);
      expect(workspace.view.sessions.single.revision,BigInt.one);
      step('listed');
      await _complete(workspace,qualification,workspace.admitWriter,AgentSessionStatus.writerAdmitted);
      final admitted=workspace.view.lastResult!.data;
      expect(admitted,isA<AgentSessionWriterAdmitted>());
      expect((admitted as AgentSessionWriterAdmitted).sessionId,plan.sessionId);
      expect(admitted.slot,greaterThan(BigInt.zero));
      step('writer-admitted');
      await _complete(workspace,qualification,workspace.openWriter,AgentSessionStatus.reply);
      expect(workspace.view.selectedInfo!.epoch,BigInt.one);
      expect(workspace.view.selectedInfo!.revision,BigInt.from(2)); step('writer-opened');
      await _complete(workspace,qualification,workspace.append,AgentSessionStatus.reply);
      expect(workspace.view.selectedInfo!.tail,BigInt.one);
      expect(workspace.view.selectedInfo!.revision,BigInt.from(3)); step('appended');
      await _complete(workspace,qualification,workspace.snapshotWriter,AgentSessionStatus.reply);
      final snapshot=workspace.view.snapshot!;
      expect(snapshot.info.sessionId,plan.sessionId);
      expect(snapshot.info.epoch,BigInt.one); expect(snapshot.info.tail,BigInt.one);
      expect(snapshot.info.revision,BigInt.from(3)); expect(snapshot.events.length,1);
      expect(snapshot.gap,isFalse); expect(snapshot.info.floor,BigInt.one);
      expect(snapshot.events.single.sequence,BigInt.one);
      expect(snapshot.events.single.event.id,'${plan.sessionId}-event');
      expect(snapshot.events.single.event.body,utf8.encode('c28 reviewed session ${plan.runId}'));
      step('snapshot-validated');
      expect((await workspace.closeWriter()).status,AgentSessionStatus.observed);
      step('writer-closed');
      expect((await workspace.detach()).status,AgentSessionStatus.detached);
      step('detached');
      var reclaim=await qualification.stop();
      var polls=0;
      while (reclaim.storage != QualificationSessionStorage.reclaimed) {
        if (++polls>1000 || reclaim.storage != QualificationSessionStorage.stopping) {
          throw StateError('Original join did not reach healthy Reclaimed');
        }
        await Future<void>.delayed(const Duration(milliseconds:10));
        reclaim=await qualification.poll(); // Observe same actual key, no Start replay.
      }
      expect(reclaim.healthyReclaimed,isTrue); expect(qualification.taskKey,actualKey);
      final retained=await workspace.observe();
      expect(retained.kind,AgentTaskKind.retainedAgentExit); expect(retained.taskKey,actualKey);
      final ack=await qualification.acknowledge();
      expect(ack.storage,QualificationSessionStorage.local);
      expect(qualification.taskKey,isNull); expect(qualification.acknowledgedTask,actualKey);
      expect((await workspace.observe()).kind,AgentTaskKind.none);
      sequenceCompleted=true;
    } catch (error,stack) {
      primary=error; primaryStack=stack;
      qualification?.evidencePersistenceFailed(error); // Permanent fence; no replay/ACK fallback.
    } finally {
      if (backend != null) {
        try {
          await backend.close(); // Existing original graceful close/EOF path only.
          terminalIdentity=qualification?.originalIdentity;
          normalCloseConfirmed=terminalIdentity?['phase']=='Released' &&
            terminalIdentity?['owner_phase']=='Released' &&
            terminalIdentity?['normal_shutdown']==true && terminalIdentity?['durable_proof']==true &&
            terminalIdentity?['child_exit_code']==0 &&
            ['resource_reclaimed','tree_empty','child_exit','stdout_eof','stderr_eof',
             'control_reaped','gate_closed'].every((key)=>terminalIdentity?[key]==true);
          if (!normalCloseConfirmed) throw StateError('Original close remains unconfirmed');
        } catch (error) { closeFailure=error; }
      }
    }
    final trace=qualification?.trace ?? const <Map<String,Object?>>[];
    try {
      await _saveNew('$evidence/trace.json',utf8.encode(jsonEncode(trace)));
      final traceBytes=await File('$evidence/trace.json').readAsBytes();
      await _saveNew('$evidence/receipt.json',utf8.encode(jsonEncode({
        'schema':'C28_DART_ORIGINAL_PRODUCT_SESSION_OBSERVATION_V1',
        'sequence_completed':sequenceCompleted,'normal_close_confirmed':normalCloseConfirmed,
        'accepted_scoped_sequence':primary==null && closeFailure==null && sequenceCompleted && normalCloseConfirmed,
        'plan_sha256':config.planSha256,'plan_bytes':config.planBytes,
        'contract_sha256':qualificationSessionContractSha256,
        'opened_original_identity':openedIdentity,'terminal_original_identity':terminalIdentity,
        'actual_task_key':qualification?.taskKey,'acknowledged_historical_key':qualification?.acknowledgedTask,
        'trace_bytes':traceBytes.length,'trace_sha256':sha256.convert(traceBytes).toString(),
        'observations':observations,'primary_failure':primary?.toString(),'close_failure':closeFailure?.toString(),
        'startup_incomplete_snapshots':startupSnapshots,
        'SDK26_G04':'OPEN','release_eligible':false,'VM':'NOT_RUN',
      })));
    } catch (error) {
      evidenceFailure=error; qualification?.evidencePersistenceFailed(error);
    }
    if (primary != null) Error.throwWithStackTrace(primary,primaryStack!);
    if (closeFailure != null) throw closeFailure;
    final failedEvidence = evidenceFailure;
    if (failedEvidence != null) throw failedEvidence;
    expect(sequenceCompleted && normalCloseConfirmed,isTrue);
  },timeout:const Timeout(Duration(minutes:3)));
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

Future<void> _complete(AgentSessionWorkspace workspace,QualificationSessionControl qualification,
    Future<AgentSessionResult> Function() submit,AgentSessionStatus expected) async {
  qualification.checkBusinessBudget();
  final submitted=await submit(); // Exact one effect, never repeat submission.
  if (submitted.status!=AgentSessionStatus.submitted) {
    throw StateError('Original operation did not return exact pending token');
  }
  final token=submitted.submission;
  for(var n=0;n<1000;n++) {
    qualification.checkBusinessBudget();
    if (workspace.view.session?.submission!=token) throw StateError('Original pending token drift');
    final completion=await workspace.read();
    if(completion.status==AgentSessionStatus.pending) {
      if(completion.submission!=token) throw StateError('Pending token drift');
      await Future<void>.delayed(const Duration(milliseconds:10));
      continue;
    }
    if(completion.status!=expected || workspace.view.session?.submission!=null ||
        completion.deliveryUnknown || workspace.view.outcomeUnknown) {
      throw StateError('Original consumed completion not validated');
    }
    return;
  }
  throw StateError('Bounded Pending observation exhausted; no resubmit');
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
