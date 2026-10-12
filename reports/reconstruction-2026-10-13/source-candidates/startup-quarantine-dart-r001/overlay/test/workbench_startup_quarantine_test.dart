import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/workbench_channel_supervised.dart';
import 'package:morrow_studio/plugins/workbench_supervision.dart';

// Memory transport only: no Process.start/run/kill, filesystem, native API or
// real owner. Status delivery goes through the actual channel's private receive.
final class _MemoryProcess implements Process {
  _MemoryProcess() {
    input.stream.listen(written.addAll);
    _stdin = IOSink(input.sink);
  }
  final input = StreamController<List<int>>();
  final out = StreamController<List<int>>(sync: true);
  final err = StreamController<List<int>>(sync: true);
  final exited = Completer<int>();
  final written = <int>[];
  late final IOSink _stdin;
  @override
  int get pid => 4242;
  @override
  IOSink get stdin => _stdin;
  @override
  Stream<List<int>> get stdout => out.stream;
  @override
  Stream<List<int>> get stderr => err.stream;
  @override
  Future<int> get exitCode => exited.future;
  @override
  bool kill([ProcessSignal signal = ProcessSignal.sigterm]) =>
      throw StateError('Process control forbidden in memory fixture');
}

String get _hash => 'a' * 64;
Map<String, dynamic> _state({String phase = 'Running'}) => {
  'version': 2, 'phase': phase, 'owner_phase': 'Retained',
  'generation': 1, 'child_pid': 100, 'supervisor_pid': 4242,
  'supervisor_creation_filetime': 1, 'child_creation_filetime': 2,
  'supervisor_sha256': _hash, 'host_sha256': _hash,
  'package_sha256': _hash, 'profile': _hash, 'incarnation': _hash,
  'recovery_history_head': '', 'abnormal_resource_closure_protocol': 1,
  'protocol_ack': 'not_applicable', 'business_outcome': 'Unknown',
  'normal_shutdown': false, 'gate_closed': phase != 'Running',
};

Map<String, dynamic> _terminal() {
  final s = _state(phase: 'ClosingUnconfirmed');
  const flags = [
    'resource_reclaimed', 'gate_closed', 'business_gate_revoked', 'tree_empty',
    'child_exit', 'stdout_eof', 'stderr_eof', 'control_reaped',
  ];
  final joins = <String, dynamic>{'version': 1,
    'stdout_reader_joined': true, 'stderr_reader_joined': true,
    'controller_reader_joined': true};
  s.addAll({'durable_proof': true, 'child_exit_code': 2,
    for (final key in flags) key: true,
    'abnormal_resource_closure_proof': Map<String, dynamic>.of(joins)});
  s['abnormal_resource_closure_prepared'] = <String, dynamic>{
    'version': 1, 'kind': 'AbnormalResourceClosurePreparedV1',
    'identity': <String, dynamic>{
      for (final key in workbenchQuarantineIdentityFields) key: s[key],
    },
    'committed_record_sha256': 'b' * 64, 'child_exit_code': 2,
    for (final key in flags) key: true,
    'reader_joins': joins, 'durable_abnormal_record_confirmed': true,
    'normal_shutdown': false, 'owner_phase': 'Retained',
    'business_outcome': 'Unknown', 'protocol_ack': 'not_applicable',
    'accepted': false, 'supervisor_exit_confirmed': false,
  };
  return s;
}

Uint8List _frame(Map<String, dynamic> state) {
  final payload = utf8.encode(jsonEncode(state));
  final header = ByteData(4)..setUint32(0, payload.length + 1, Endian.little);
  return Uint8List.fromList([...header.buffer.asUint8List(), 2, ...payload]);
}

final class _Fixture {
  _Fixture({bool qualification = true}) {
    channel = SupervisedWorkbenchChannel(process,
      hostSha256: _hash, supervisorSha256: _hash,
      expectedProfile: _hash, packageSha256: _hash,
      qualificationStartupObservationEnabled: qualification,
      registerQualificationStartupFailure: qualification ?
          (mark) => markFailed = mark : null,
    );
    channel.output.listen((_) {}, onError: (Object _) {});
    channel.diagnostics.listen((_) {}, onError: (Object _) {});
  }
  final process = _MemoryProcess();
  late final SupervisedWorkbenchChannel channel;
  late final void Function() markFailed;
  void emit(Map<String, dynamic> state) => process.out.add(_frame(state));
  WorkbenchStartupObservation snapshot() => channel.qualificationStartupObservation(
    stage: 'before_original_close', primaryFailure: StateError('primary startup'),
  );
  List<({int kind, int payloadSize})> get commands {
    final b = process.written;
    final result = <({int kind, int payloadSize})>[];
    var offset = 0;
    while (offset + 4 <= b.length) {
      final n = ByteData.sublistView(Uint8List.fromList(b.sublist(offset, offset + 4)))
          .getUint32(0, Endian.little);
      if (offset + 4 + n > b.length) break;
      result.add((kind: b[offset + 4], payloadSize: n - 1));
      offset += 4 + n;
    }
    return result;
  }
  Future<void> closing() async {
    emit(_state());
    markFailed();
    emit(_state(phase: 'ClosingUnconfirmed'));
    await _tick();
  }
  Future<void> finish({int code = 2}) async {
    if (!process.exited.isCompleted) process.exited.complete(code);
    if (!process.out.isClosed) await process.out.close();
    if (!process.err.isClosed) await process.err.close();
    await channel.exitCode.then<void>((_) {}, onError: (Object _) {});
    await _tick();
  }
}

Future<void> _tick() => Future<void>.delayed(Duration.zero);

void main() {
  test('only marked validated abnormal status sends one empty kind4', () async {
    final f = _Fixture();
    f.emit(_state());
    f.markFailed();
    f.markFailed();
    await _tick();
    expect(f.commands.where((c) => c.kind == 4), isEmpty);
    expect(f.channel.supportsPrivateControl, false);
    await expectLater(f.channel.send(Uint8List.fromList([1])), throwsA(isA<WorkbenchSupervisionUnconfirmed>()));
    f.emit(_state(phase: 'ClosingUnconfirmed'));
    f.emit(_state(phase: 'ClosingUnconfirmed'));
    await f.channel.closeInput();
    await _tick();
    expect(f.commands.where((c) => c.kind == 4).toList(), [(kind: 4, payloadSize: 0)]);
    expect(f.commands.where((c) => c.kind == 3), isEmpty);
    f.emit(_terminal());
    await f.finish();
    final receipt = f.snapshot().toJson()['failed_startup_quarantine'] as Map;
    expect(receipt['kind'], 'failed-startup-quarantined');
    expect(receipt['accepted'], false);
    expect(receipt['business_outcome'], 'Unknown');
    expect(receipt['owner_phase'], 'Retained');
    await expectLater(f.channel.exitCode, throwsA(isA<WorkbenchSupervisionUnconfirmed>()));
  });

  test('public notifier cannot authorize control or manufacture receipt', () async {
    final f = _Fixture();
    f.channel.supervision.value = WorkbenchSupervisionState(_terminal());
    f.markFailed();
    await _tick();
    expect(f.commands.where((c) => c.kind == 4), isEmpty);
    await f.finish();
    expect(f.snapshot().failedStartupQuarantine, isNull);
  });

  test('nested public proof mutations cannot alter private prepared copy', () async {
    final f = _Fixture();
    await f.closing();
    f.emit(_terminal());
    final exposed = f.channel.supervision.value!.record['abnormal_resource_closure_prepared'] as Map;
    (exposed['identity'] as Map)['child_pid'] = 999;
    (exposed['reader_joins'] as Map)['stdout_reader_joined'] = false;
    f.channel.supervision.value = WorkbenchSupervisionState(_state());
    await f.finish();
    final receipt = f.snapshot().failedStartupQuarantine!;
    final prepared = receipt['prepared'] as Map;
    expect((prepared['identity'] as Map)['child_pid'], 100);
    expect((prepared['reader_joins'] as Map)['stdout_reader_joined'], true);
    expect(() => (prepared['identity'] as Map)['child_pid'] = 999, throwsUnsupportedError);
  });

  test('every original identity field including history rejects changed prepared identity', () async {
    for (final key in workbenchQuarantineIdentityFields) {
      final f = _Fixture();
      await f.closing();
      final s = _terminal();
      final p = s['abnormal_resource_closure_prepared'] as Map;
      (p['identity'] as Map)[key] = key == 'recovery_history_head' ? 'c' * 64 : null;
      f.emit(s);
      await f.finish();
      expect(f.snapshot().failedStartupQuarantine, isNull, reason: key);
    }
  });

  test('wrong history on abnormal status rejects before kind4', () async {
    final f = _Fixture();
    f.emit(_state());
    f.markFailed();
    f.emit(_state(phase: 'ClosingUnconfirmed')..['recovery_history_head'] = 'c' * 64);
    await f.finish();
    expect(f.commands.where((c) => c.kind == 4), isEmpty);
    expect(f.snapshot().failedStartupQuarantine, isNull);
  });

  test('floating protocol capability cannot authorize kind4', () async {
    final f = _Fixture();
    f.emit(_state()..['abnormal_resource_closure_protocol'] = 1.0);
    f.markFailed();
    f.emit(_state(phase: 'ClosingUnconfirmed'));
    await f.finish();
    expect(f.commands.where((c) => c.kind == 4), isEmpty);
    expect(f.snapshot().failedStartupQuarantine, isNull);
  });

  test('unmarked extension cannot repurpose normal or nonfailure channel', () async {
    final f = _Fixture();
    f.emit(_state());
    f.emit(_terminal());
    await f.finish();
    expect(f.commands.where((c) => c.kind == 4), isEmpty);
    expect(f.snapshot().failedStartupQuarantine, isNull);
  });

  test('fake extension flags digest reader joins and child code fail closed', () async {
    final corrupt = <void Function(Map<String, dynamic>)>[
      (s) => (s['abnormal_resource_closure_prepared'] as Map)['version'] = 2,
      (s) => (s['abnormal_resource_closure_prepared'] as Map)['version'] = 1.0,
      (s) => (s['abnormal_resource_closure_prepared'] as Map)['kind'] = 'Released',
      (s) => (s['abnormal_resource_closure_prepared'] as Map)['accepted'] = true,
      (s) => (s['abnormal_resource_closure_prepared'] as Map)['supervisor_exit_confirmed'] = true,
      (s) => (s['abnormal_resource_closure_prepared'] as Map)['resource_reclaimed'] = false,
      (s) => (s['abnormal_resource_closure_prepared'] as Map)['committed_record_sha256'] = '',
      (s) => (s['abnormal_resource_closure_prepared'] as Map)['child_exit_code'] = '2',
      (s) => ((s['abnormal_resource_closure_prepared'] as Map)['reader_joins'] as Map)['controller_reader_joined'] = false,
      (s) => s['durable_proof'] = false,
      (s) => s['tree_empty'] = false,
      (s) => s['abnormal_resource_closure_protocol'] = 2,
      (s) => s['child_exit_code'] = 9,
      (s) => (s['abnormal_resource_closure_prepared'] as Map)['unexpected'] = true,
    ];
    for (final change in corrupt) {
      final f = _Fixture();
      await f.closing();
      final s = _terminal();
      change(s);
      f.emit(s);
      await f.finish();
      expect(f.snapshot().failedStartupQuarantine, isNull);
    }
  });

  test('missing actual EOF never creates quarantine receipt', () async {
    final f = _Fixture();
    await f.closing();
    f.emit(_terminal());
    f.process.exited.complete(2);
    await f.process.out.close();
    await _tick();
    expect(f.snapshot().failedStartupQuarantine, isNull);
    await f.finish();
    expect(f.snapshot().failedStartupQuarantine, isNotNull);
  });

  test('wrong actual supervisor exit cannot accept prepared extension', () async {
    for (final code in [0, 1, 3]) {
      final f = _Fixture();
      await f.closing();
      f.emit(_terminal());
      await f.finish(code: code);
      expect(f.snapshot().failedStartupQuarantine, isNull, reason: '$code');
    }
  });

  test('duplicate terminal invalidates quarantine before actual closure', () async {
    final f = _Fixture();
    await f.closing();
    f.emit(_terminal());
    f.emit(_terminal());
    await f.finish();
    expect(f.snapshot().failedStartupQuarantine, isNull);
    expect(f.commands.where((c) => c.kind == 4).length, 1);
  });

  test('transport error retains Unknown without quarantine retry', () async {
    final f = _Fixture();
    await f.closing();
    f.process.out.addError(StateError('original transport error'));
    f.emit(_terminal());
    f.markFailed();
    await f.finish();
    expect(f.snapshot().failedStartupQuarantine, isNull);
    expect(f.commands.where((c) => c.kind == 4).length, 1);
  });

  test('normal close stays kind3 Released zero without quarantine receipt', () async {
    final f = _Fixture();
    f.emit(_state());
    await f.channel.closeInput();
    f.markFailed(); // An already requested normal close cannot be repurposed.
    final s = _terminal()
      ..remove('abnormal_resource_closure_prepared')
      ..['phase'] = 'Released'
      ..['owner_phase'] = 'Released'
      ..['normal_shutdown'] = true
      ..['child_exit_code'] = 0;
    f.emit(s);
    await f.finish(code: 0);
    expect(await f.channel.exitCode, 0);
    expect(f.commands.where((c) => c.kind == 3).length, 1);
    expect(f.commands.where((c) => c.kind == 4), isEmpty);
    expect(f.snapshot().failedStartupQuarantine, isNull);
  });

  test('legacy nonqualification normal release remains unchanged', () async {
    final f = _Fixture(qualification: false);
    final initial = _state()
      ..remove('recovery_history_head')
      ..remove('abnormal_resource_closure_protocol');
    f.emit(initial);
    await f.channel.closeInput();
    final released = Map<String, dynamic>.of(initial)
      ..['phase'] = 'Released'
      ..['owner_phase'] = 'Released'
      ..['normal_shutdown'] = true
      ..['durable_proof'] = true
      ..['child_exit_code'] = 0;
    for (final key in ['resource_reclaimed', 'tree_empty', 'child_exit',
        'stdout_eof', 'stderr_eof', 'control_reaped', 'gate_closed']) {
      released[key] = true;
    }
    f.emit(released);
    await f.finish(code: 0);
    expect(await f.channel.exitCode, 0);
    expect(f.commands.where((c) => c.kind == 3).length, 1);
    expect(f.commands.where((c) => c.kind == 4), isEmpty);
  });

  test('failure observer keeps primary and actual failed close alongside separate receipt', () async {
    final f = _Fixture();
    await f.closing();
    final primary = StateError('original startup');
    WorkbenchStartupObservation? after;
    final observed = observeWorkbenchStartupFailure(
      primaryFailure: primary,
      capture: f.channel.qualificationStartupObservation,
      observe: (snapshot) {
        if (snapshot.stage == 'before_original_close') throw StateError('observer failed');
        after = snapshot;
      },
      closeOriginal: () async {
        await f.channel.closeInput();
        await f.channel.exitCode;
      },
    );
    f.emit(_terminal());
    await f.finish();
    final errors = await observed;
    expect(errors.beforeObserverFailed, true);
    expect(after!.primaryFailure.text, primary.toString());
    expect(after!.secondaryCloseFailure!.text, contains('without verified normal'));
    expect(after!.observerFailure!.text, contains('observer failed'));
    expect(after!.failedStartupQuarantine, isNotNull);
    expect(after!.toJson()['accepted'], false);
  });
}
