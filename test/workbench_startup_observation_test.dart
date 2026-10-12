import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/workbench_channel_supervised.dart';
import 'package:morrow_studio/plugins/workbench_supervision.dart';

// All streams, exit completion and stdin here are memory-only. No OS process,
// Process.start/run, native API, file operation or kill is used by these tests.
final class _MemoryProcess implements Process {
  _MemoryProcess() {
    input.stream.listen((_) {});
    _stdin = IOSink(input.sink);
  }
  final input = StreamController<List<int>>();
  final out = StreamController<List<int>>(sync: true);
  final err = StreamController<List<int>>(sync: true);
  final exited = Completer<int>();
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
      throw StateError('No process control exists in this memory fixture');
}

String get _hash => List.filled(64, 'a').join();
Map<String, dynamic> _state({String phase = 'Running', int generation = 1}) => {
  'version': 2, 'phase': phase, 'owner_phase': 'Retained',
  'generation': generation, 'child_pid': 100, 'supervisor_pid': 4242,
  'supervisor_creation_filetime': 1, 'child_creation_filetime': 2,
  'supervisor_sha256': _hash, 'host_sha256': _hash,
  'package_sha256': _hash, 'profile': _hash, 'incarnation': _hash,
  'protocol_ack': 'not_applicable', 'business_outcome': 'Unknown',
  'gate_closed': phase != 'Running',
};
Uint8List _frame(Map<String, dynamic> state) {
  final payload = utf8.encode(jsonEncode(state));
  final header = ByteData(4)..setUint32(0, payload.length + 1, Endian.little);
  return Uint8List.fromList([...header.buffer.asUint8List(), 2, ...payload]);
}
SupervisedWorkbenchChannel _channel(_MemoryProcess process, {bool observationEnabled = true}) {
  final channel = SupervisedWorkbenchChannel(process,
    hostSha256: _hash, supervisorSha256: _hash,
    expectedProfile: _hash, packageSha256: _hash,
    qualificationStartupObservationEnabled: observationEnabled,
  );
  channel.output.listen((_) {}, onError: (Object _) {});
  channel.diagnostics.listen((_) {}, onError: (Object _) {});
  return channel;
}
Future<void> _finish(_MemoryProcess process, SupervisedWorkbenchChannel channel) async {
  if (!process.exited.isCompleted) {
    process.exited.complete(0);
  }
  if (!process.out.isClosed) {
    await process.out.close();
  }
  if (!process.err.isClosed) {
    await process.err.close();
  }
  await channel.exitCode.then<void>((_) {}, onError: (Object _) {});
}
WorkbenchStartupObservation _capture({
  required String stage, required Object primaryFailure,
  Object? secondaryCloseFailure, Object? observerFailure,
}) => WorkbenchStartupObservation(
  stage: stage, primaryFailure: primaryFailure,
  initialValidatedState: null, latestValidatedState: null,
  diagnosticBytes: const [], diagnosticBytesObserved: 0,
  actualExitCode: null, stdoutEof: false, stderrEof: false,
  secondaryCloseFailure: secondaryCloseFailure, observerFailure: observerFailure,
);

void main() {
  test('observation precedes a pending original close without acceptance', () async {
    final close = Completer<void>();
    final events = <String>[];
    final seen = <WorkbenchStartupObservation>[];
    final delivery = observeWorkbenchStartupFailure(
      primaryFailure: StateError('original startup'), capture: _capture,
      observe: (value) { events.add(value.stage); seen.add(value); },
      closeOriginal: () { events.add('close requested'); return close.future; },
    );
    await Future<void>.delayed(Duration.zero);
    expect(events, ['before_original_close', 'close requested']);
    expect(seen.single.toJson()['accepted'], false);
    expect(seen.single.toJson()['original_close_settled'], false);
    close.complete();
    await delivery;
    expect(events.last, 'after_original_close');
    expect(seen.last.toJson()['incomplete'], true);
    expect(seen.last.toJson()['accepted'], false);
  });

  test('observer failure cannot skip close or replace primary and secondary', () async {
    final primary = StateError('primary startup');
    var closes = 0;
    WorkbenchStartupObservation? after;
    final delivery = await observeWorkbenchStartupFailure(
      primaryFailure: primary, capture: _capture,
      observe: (value) {
        if (value.stage == 'before_original_close') {
          throw StateError('write failed');
        }
        after = value;
      },
      closeOriginal: () async { closes++; throw StateError('secondary close'); },
    );
    expect(closes, 1);
    expect(delivery.beforeObserverFailed, true);
    expect(delivery.afterObserverFailed, false);
    expect(after!.primaryFailure.text, primary.toString());
    expect(after!.secondaryCloseFailure!.text, contains('secondary close'));
    expect(after!.observerFailure!.text, contains('write failed'));
    expect(after!.toJson()['accepted'], false);
  });

  test('second observer failure stays separate from primary and close', () async {
    var observations = 0, closes = 0;
    final delivery = await observeWorkbenchStartupFailure(
      primaryFailure: StateError('primary'), capture: _capture,
      observe: (_) {
        if (++observations == 2) {
          throw StateError('second write');
        }
      },
      closeOriginal: () async { closes++; },
    );
    expect(closes, 1);
    expect(delivery.beforeObserverFailed, false);
    expect(delivery.afterObserverFailed, true);
  });

  test('private validated state ignores public notifier writes', () async {
    final process = _MemoryProcess();
    final original = _channel(process);
    addTearDown(() => _finish(process, original));
    original.supervision.value = WorkbenchSupervisionState(_state());
    expect(original.qualificationStartupObservation(
      stage: 'before_original_close', primaryFailure: StateError('x'),
    ).initialValidatedState, null);
    process.out.add(_frame(_state()));
    process.out.add(_frame(_state(phase: 'ClosingUnconfirmed')));
    original.supervision.value = WorkbenchSupervisionState(_state(generation: 999));
    final snapshot = original.qualificationStartupObservation(
      stage: 'before_original_close', primaryFailure: StateError('x'),
    );
    expect(snapshot.initialValidatedState!['phase'], 'Running');
    expect(snapshot.latestValidatedState!['phase'], 'ClosingUnconfirmed');
    expect(snapshot.latestValidatedState!['generation'], 1);
    expect(original.supportsPrivateControl, false);
  });

  test('ordinary channel cannot export qualification startup observation', () async {
    final process = _MemoryProcess();
    final channel = _channel(process, observationEnabled: false);
    addTearDown(() => _finish(process, channel));
    process.out.add(_frame(_state()));
    expect(channel.supportsPrivateControl, true);
    expect(() => channel.qualificationStartupObservation(
      stage: 'before_original_close', primaryFailure: StateError('x'),
    ), throwsStateError);
  });

  test('foreign initial frame is excluded from startup observation', () async {
    final process = _MemoryProcess(); final channel = _channel(process);
    addTearDown(() => _finish(process, channel));
    process.out.add(_frame(_state()..['supervisor_pid'] = 999));
    final snapshot = channel.qualificationStartupObservation(
      stage: 'before_original_close', primaryFailure: StateError('x'),
    );
    expect(snapshot.initialValidatedState, null);
    expect(snapshot.latestValidatedState, null);
    expect(channel.supportsPrivateControl, false);
  });

  test('identity continuity failure preserves only last validated state', () async {
    final process = _MemoryProcess(); final channel = _channel(process);
    addTearDown(() => _finish(process, channel));
    process.out.add(_frame(_state()));
    process.out.add(_frame(_state(phase: 'ClosingUnconfirmed', generation: 2)));
    final snapshot = channel.qualificationStartupObservation(
      stage: 'before_original_close', primaryFailure: StateError('x'),
    );
    expect(snapshot.initialValidatedState!['generation'], 1);
    expect(snapshot.latestValidatedState!['generation'], 1);
    expect(snapshot.latestValidatedState!['phase'], 'Running');
    expect(channel.supportsPrivateControl, false);
  });

  test('diagnostic prefix counts original bytes and snapshots are immutable', () async {
    final process = _MemoryProcess(); final channel = _channel(process);
    addTearDown(() => _finish(process, channel));
    final prefix = List<int>.generate(10000, (index) => index % 256);
    process.err.add(prefix);
    final snapshot = channel.qualificationStartupObservation(
      stage: 'before_original_close', primaryFailure: StateError('x'),
    );
    process.err.add([99]); prefix[0] = 99;
    expect(snapshot.diagnosticBytes, List<int>.generate(8192, (index) => index % 256));
    expect(snapshot.diagnosticBytesObserved, 10000);
    expect(snapshot.toJson()['diagnostic_bytes_truncated'], true);
    expect(() => snapshot.diagnosticBytes[0] = 5, throwsUnsupportedError);
    expect(() => snapshot.toJson()['accepted'] = true, throwsUnsupportedError);
  });

  test('exit and EOF facts remain independent and never imply release', () async {
    final process = _MemoryProcess(); final channel = _channel(process);
    addTearDown(() => _finish(process, channel));
    process.exited.complete(2);
    await Future<void>.delayed(Duration.zero);
    final first = channel.qualificationStartupObservation(
      stage: 'before_original_close', primaryFailure: StateError('x'),
    );
    expect(first.toJson()['actual_exit_known'], true);
    expect(first.actualExitCode, 2);
    expect(first.stdoutEof, false); expect(first.stderrEof, false);
    await process.out.close(); await process.err.close();
    final last = channel.qualificationStartupObservation(
      stage: 'after_original_close', primaryFailure: StateError('x'),
    );
    expect(last.stdoutEof, true); expect(last.stderrEof, true);
    expect(last.toJson()['accepted'], false);
  });

  test('failure text and JSON snapshots remain bounded', () {
    final record = _state()..['owner_phase'] = List.filled(100000, 'x').join();
    final huge = List.filled(10000, '\u0000😀').join();
    final snapshot = WorkbenchStartupObservation(
      stage: 'after_original_close', primaryFailure: huge,
      initialValidatedState: WorkbenchSupervisionState(record),
      latestValidatedState: WorkbenchSupervisionState(record),
      diagnosticBytes: List.filled(8192, 255), diagnosticBytesObserved: 999999,
      actualExitCode: null, stdoutEof: false, stderrEof: false,
      secondaryCloseFailure: huge, observerFailure: huge,
    );
    record['generation'] = 2;
    expect(snapshot.initialValidatedState!['generation'], 1);
    expect(snapshot.initialValidatedState!.containsKey('owner_phase'), false);
    expect(() => snapshot.initialValidatedState!['generation'] = 3, throwsUnsupportedError);
    expect(utf8.encode(snapshot.primaryFailure.text).length, lessThanOrEqualTo(2048));
    expect(snapshot.primaryFailure.truncated, true);
    expect(utf8.encode(jsonEncode(snapshot.toJson())).length, lessThan(128 * 1024));
  });

  test('snapshot failure still attempts original close', () async {
    var closes = 0;
    final delivery = await observeWorkbenchStartupFailure(
      primaryFailure: StateError('primary'),
      capture: ({required stage, required primaryFailure,
          secondaryCloseFailure, observerFailure}) => throw StateError('capture failed'),
      observe: (_) => fail('No snapshot was produced'),
      closeOriginal: () async { closes++; },
    );
    expect(closes, 1);
    expect(delivery.beforeObserverFailed, true);
    expect(delivery.afterObserverFailed, true);
  });
}
