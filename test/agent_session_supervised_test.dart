// SOURCE ONLY. Real Dart channel/controller with an ordinary Python protocol
// fixture. No real supervisor, Job, protected library, native host or authority.
import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/agent_session_codec_native.dart';
import 'package:morrow_studio/plugins/agent_task_discovery.dart';
import 'package:morrow_studio/plugins/agent_session_control.dart';
import 'package:morrow_studio/plugins/agent_session_native.dart';
import 'package:morrow_studio/plugins/generated/host.capnp.dart' as host;
import 'package:morrow_studio/plugins/workbench_channel_supervised.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';
import 'package:morrow_studio/plugins/workbench_supervision.dart';
import 'agent_session_workbench_routing_helper.dart';


// Read the actual generated Rust None reply. Only the exact request ID and
// SHA256 of this actual request are replaced; every other canonical byte stays.
final class _RustDiscoveryNone {
  _RustDiscoveryNone(this.id, this.request, this.reply);
  final Uint8List id, request, reply;
  static Uint8List hex(String text) => Uint8List.fromList([
    for (var i = 0; i < text.length; i += 2)
      int.parse(text.substring(i, i + 2), radix: 16),
  ]);
  static _RustDiscoveryNone load() {
    final root = Platform.environment['MORROW_AGENT_DISCOVERY_FIXTURES'];
    if (root == null || !Directory(root).isAbsolute) {
      throw StateError('Explicit original Rust discovery fixture directory required');
    }
    final vectors = jsonDecode(File('$root/vectors.json').readAsStringSync()) as Map<String, dynamic>;
    expect(vectors['schema'], 'STAGE31_AGENT_DISCOVERY_SYNTHETIC_INTEROP_V1');
    expect(vectors['schemaSha256'], '9357ac5a01dc00c22551f0d3a399fa1a2498780283f7e700b0b966bbc449ddef');
    final id = hex(vectors['requestIdHex'] as String);
    final request = hex(vectors['requestHex'] as String);
    final row = (vectors['replies'] as List).cast<Map<String, dynamic>>().singleWhere((r) => r['kind'] == 'none');
    expect(row['taskHex'], '');
    final reply = hex(row['replyHex'] as String);
    expect(request, orderedEquals(File('$root/request.bin').readAsBytesSync()));
    expect(reply, orderedEquals(File('$root/none.reply.bin').readAsBytesSync()));
    expect(AgentTaskDiscoveryCodec().encode(id), orderedEquals(request));
    expect(AgentTaskDiscoveryCodec().decode(id, request, reply).kind, AgentTaskKind.none);
    return _RustDiscoveryNone(id, request, reply);
  }
  Uint8List correlate(Uint8List current) {
    final currentId = Uint8List.sublistView(current, current.length - 32);
    expect(AgentTaskDiscoveryCodec().encode(currentId), orderedEquals(current));
    final output = Uint8List.fromList(reply);
    for (final pair in [
      (id, currentId),
      (AgentSessionCodec.hash(request), AgentSessionCodec.hash(current)),
    ]) {
      final positions = <int>[];
      for (var i = 0; i <= output.length - 32; i++) {
        if (List.generate(32, (j) => output[i + j] == pair.$1[j]).every((v) => v)) positions.add(i);
      }
      expect(positions, hasLength(1));
      output.setRange(positions.single, positions.single + 32, pair.$2);
    }
    return output;
  }
}

final class _SupervisorFixture {
  _SupervisorFixture(this.directory, this.process, this.channel, this.script);
  final Directory directory;
  final Process process;
  final SupervisedWorkbenchChannel channel;
  final SessionPipeScript script;
  RustWorkbench? owner;
  late Future<void> _server;
  bool _stop = false, _closed = false;
  Object? _serverError;
  bool outerBad = false, drift = false, gate = false, wrongHash = false;
  bool holdLocale = false, holdPrivate = false;
  bool oversizedDiscovery = false;
  final discoveryFrames = <Uint8List>[];
  final legacyActions = <host.Action?>[];
  _RustDiscoveryNone? discoveryNone;
  final localeSeen = Completer<void>(), privateSeen = Completer<void>();
  final releaseLocale = Completer<void>(), releasePrivate = Completer<void>();

  static Future<_SupervisorFixture> open(
    String python,
    Directory parent,
    SessionPipeScript script, {
    bool connect = true,
  }) async {
    if (!File(python).isAbsolute || !await File(python).exists()) {
      throw ArgumentError('ROOT must supply a fixed absolute ordinary Python');
    }
    if (!parent.isAbsolute || !await parent.exists()) {
      throw ArgumentError('ROOT must supply an independent retained evidence directory');
    }
    final dir = await parent.createTemp('supervised-session-synthetic-');
    final source = await File('test/supervised_session_fake.py').readAsBytes();
    await File('${dir.path}/fake_supervisor.py').writeAsBytes(
      source, mode: FileMode.writeOnly, flush: true,
    );
    final process = await Process.start(
      python, ['-I', '-S', '-B', '${dir.path}/fake_supervisor.py'],
      workingDirectory: dir.path,
      includeParentEnvironment: false,
      environment: {
        for (final k in ['SYSTEMROOT', 'WINDIR'])
          if (Platform.environment[k] != null) k: Platform.environment[k]!,
        for (final k in ['TEMP', 'TMP', 'HOME', 'USERPROFILE', 'APPDATA', 'LOCALAPPDATA'])
          k: dir.path,
      },
    );
    final channel = SupervisedWorkbenchChannel(
      process, hostSha256: 'c' * 64, supervisorSha256: 'e' * 64,
      expectedProfile: 'a' * 64, packageSha256: 'd' * 64,
    );
    final f = _SupervisorFixture(dir, process, channel, script);
    f._server = f._serve();
    try {
      if (connect) f.owner = await RustWorkbench.connect(channel);
      await until(() => channel.supportsPrivateControl);
      return f;
    } catch (_) {
      await f.close(expectedUnconfirmed: true);
      rethrow;
    }
  }

  static Future<void> until(bool Function() ready) async {
    final watch = Stopwatch()..start();
    while (!ready()) {
      if (watch.elapsed > const Duration(seconds: 10)) {
        throw TimeoutException('Synthetic observation deadline');
      }
      await Future<void>.delayed(const Duration(milliseconds: 2));
    }
  }

  Future<void> _serve() async {
    for (var i = 0; !_stop; i++) {
      final input = File('${directory.path}/request.$i.bin');
      while (!await input.exists()) {
        if (_stop) return;
        await Future<void>.delayed(const Duration(milliseconds: 2));
      }
      try {
        final bytes = await input.readAsBytes();
        if (bytes.isEmpty || bytes.length > 128 * 1024) {
          throw StateError('Synthetic request bound');
        }
        final private = bytes.length >= AgentSessionCodec.prefix.length &&
            AgentSessionCodec.prefix.asMap().entries.every((e) => bytes[e.key] == e.value);
        final discovery = bytes.length >= 8 &&
            AgentTaskDiscoveryCodec.prefix.asMap().entries.every((e) => bytes[e.key] == e.value);
        if (!private && !discovery) {
          legacyActions.add(RustWorkbench.readMessage(bytes).getRoot(host.requestFactory).action);
        }
        if (private && holdPrivate) {
          if (!privateSeen.isCompleted) privateSeen.complete();
          await releasePrivate.future;
        }
        if (!private && !discovery && holdLocale &&
            RustWorkbench.readMessage(bytes).getRoot(host.requestFactory).action == host.Action.readUiLocale) {
          if (!localeSeen.isCompleted) localeSeen.complete();
          await releaseLocale.future;
        }
        final PipeReply reply;
        if (discovery) {
          discoveryFrames.add(Uint8List.fromList(bytes));
          final canonical = (discoveryNone ??= _RustDiscoveryNone.load()).correlate(bytes);
          // Deliberately invalid oversized data probes the fixed 16KiB cap.
          reply = PipeReply(oversizedDiscovery
              ? (Uint8List(16 * 1024 + 1)..setRange(0, canonical.length, canonical))
              : canonical);
        } else {
          reply = script.respond(bytes);
        }
        if (private && wrongHash) {
          final digest = AgentSessionCodec.hash(bytes);
          var at = -1;
          for (var x = 8; x <= reply.bytes.length - digest.length; x++) {
            if (digest.asMap().entries.every((e) => reply.bytes[x + e.key] == e.value)) {
              at = x; break;
            }
          }
          if (at < 0) throw StateError('Expected exact synthetic request hash');
          reply.bytes[at] ^= 1;
        }
        await File('${directory.path}/reply.$i.bin').writeAsBytes(
          reply.bytes, mode: FileMode.writeOnly, flush: true,
        );
        final temporary = File('${directory.path}/reply.$i.tmp');
        await temporary.writeAsString(jsonEncode({
          'drop': reply.drop, 'outer_bad': private && outerBad,
          'drift': private && drift, 'gate': private && gate,
        }), mode: FileMode.writeOnly, flush: true);
        await temporary.rename('${directory.path}/reply.$i.json');
      } catch (e) {
        _serverError ??= e;
        await File('${directory.path}/reply.$i.bin').writeAsBytes([]);
        await File('${directory.path}/reply.$i.json').writeAsString('{"drop":true}');
      }
    }
  }

  Future<void> status(Map<String, Object> values) async {
    final temporary = File('${directory.path}/status-command.tmp');
    await temporary.writeAsString(jsonEncode(values), flush: true);
    await temporary.rename('${directory.path}/status-command.json');
    await until(() => File('${directory.path}/status-command-observed').existsSync());
  }

  Future<Map<String, dynamic>> close({bool expectedUnconfirmed = false}) async {
    if (_closed) throw StateError('Fixture close only once');
    _closed = true;
    if (!releaseLocale.isCompleted) releaseLocale.complete();
    if (!releasePrivate.isCompleted) releasePrivate.complete();
    script.business.businessReady = true;
    Object? closeError;
    try {
      if (owner != null) {
        await owner!.close();
      } else {
        await channel.closeInput();
        await channel.exitCode;
      }
    } catch (error) {
      closeError = error;
    } finally {
      _stop = true;
      await _server;
    }
    final childExit = await process.exitCode;
    String? detail(Object? error) {
      if (error == null) return null;
      final value = error.toString();
      return value.length <= 16384
          ? value
          : '${value.substring(0, 16384)} [diagnostic truncated]';
    }
    final diagnostics = File('${directory.path}/close-diagnostics.json');
    await diagnostics.writeAsString(jsonEncode({
      'ordinary_synthetic_only': true,
      'actual_child_exit': childExit,
      'expected_child_exit': 0,
      'expected_unconfirmed': expectedUnconfirmed,
      'close_error_type': closeError?.runtimeType.toString(),
      'close_error_detail': detail(closeError),
      'server_error_type': _serverError?.runtimeType.toString(),
      'server_error_detail': detail(_serverError),
      'python_child_error': '${directory.path}/child-error',
      'python_reader_error': '${directory.path}/reader-error',
      'retained': true,
    }), mode: FileMode.writeOnly, flush: true);
    expect(
      childExit, 0,
      reason: 'Original synthetic fixture diagnostics retained at ${diagnostics.path}',
    );
    await until(() => File('${directory.path}/drain-complete').existsSync());
    expect(_serverError, isNull);
    expect(File('${directory.path}/child-error').existsSync(), false);
    if (expectedUnconfirmed) {
      expect(closeError, isA<WorkbenchSupervisionUnconfirmed>());
    } else if (closeError != null) {
      Error.throwWithStackTrace(closeError, StackTrace.current);
    }
    final result = jsonDecode(await File('${directory.path}/transport-summary.json').readAsString()) as Map<String, dynamic>;
    await File('${directory.path}/fixture-verdict.json').writeAsString(jsonEncode({
      'ordinary_synthetic_only': true, 'expected_unconfirmed': expectedUnconfirmed,
      'close_error': closeError?.runtimeType.toString(), 'retained': true,
    }), mode: FileMode.writeOnly);
    return result;
  }
}

void main() {
  final python = Platform.environment['MORROW_TEST_PYTHON'];
  final evidence = Platform.environment['MORROW_SUPERVISED_SESSION_EVIDENCE'];
  final skip = python == null || evidence == null
      ? 'ROOT supplies fixed ordinary Python and retained synthetic evidence root' : false;
  Future<_SupervisorFixture> start(SessionPipeScript script, {bool connect = true}) =>
      _SupervisorFixture.open(python!, Directory(evidence!), script, connect: connect);
  final discoverySkip = skip != false ? skip :
      Platform.environment['MORROW_AGENT_DISCOVERY_FIXTURES'] == null
          ? 'ROOT supplies actual Rust discovery fixture directory' : false;
  final unknown = isA<AgentSessionFailure>().having(
    (e) => e.kind, 'uncertain result', AgentSessionFailureKind.unknown,
  );
  final decoderError = isA<FormatException>().having(
    (e) => e.message, 'fixed codec', 'Private session receipt mismatch',
  );

  test('supervised original fixed codec small chain retains Pending and consumes terminal once', () async {
    final f = await start(SessionPipeScript());
    try {
      final c = f.owner!.agentSessionControl();
      expect(c.supportsAgentSessions, true);
      expect(identical(c, f.owner!.agentSessionControl()), true);
      expect((await c.attachSession(pipeTask)).status, AgentSessionStatus.attached);
      expect((await c.openSession(pipeTask, pipeAttachment)).status, AgentSessionStatus.submitted);
      expect((await c.readSession(pipeTask, pipeAttachment, pipeToken)).status, AgentSessionStatus.pending);
      expect((await c.readSession(pipeTask, pipeAttachment, pipeToken)).status, AgentSessionStatus.opened);
      final before = f.script.privateFrames.length;
      await expectLater(c.readSession(pipeTask, pipeAttachment, pipeToken), throwsA(unknown));
      expect(f.script.privateFrames.length, before);
      expect(f.script.privateOps, [1, 2, 5, 5]);
    } finally {
      final result = await f.close();
      expect(result['graceful_close_requested'], true);
      expect(result['synthetic_protocol_released'], true);
    }
  }, skip: skip, timeout: const Timeout(Duration(seconds: 30)));

  test('supervised kind1 reassembles 300KiB transport bytes across original bounded chunks', () async {
    final f = await start(SessionPipeScript()..largePrivate = true, connect: false);
    final bytes = <int>[]; final done = Completer<void>();
    final sub = f.channel.output.listen((chunk) {
      bytes.addAll(chunk);
      if (bytes.length == 300 * 1024 + 4 && !done.isCompleted) done.complete();
    }, onError: (Object e) { if (!done.isCompleted) done.completeError(e); });
    try {
      final request = AgentSessionCodec().encode(AgentSessionWireRequest(
        operation: AgentSessionOperation.attach, id: List.filled(32, 7),
        taskKey: pipeTask, attachment: BigInt.zero,
      ));
      await f.channel.send(request);
      await done.future.timeout(const Duration(seconds: 5));
      expect(ByteData.sublistView(Uint8List.fromList(bytes)).getUint32(0, Endian.little), 300 * 1024);
      expect(bytes.skip(4).every((x) => x == 0), true);
      // Byte transport only: the padding is explicitly NOT a valid PS30 Reply.
    } finally {
      final result = await f.close();
      expect(result['max_kind1_payload'], lessThanOrEqualTo(16384));
      expect(result['kind1_chunks'], greaterThan(18));
      await sub.cancel();
    }
  }, skip: skip, timeout: const Timeout(Duration(seconds: 30)));

  test('supervised 300KiB malformed inner Reply reaches fixed decoder and seals without replay', () async {
    final f = await start(SessionPipeScript()..largePrivate = true);
    try {
      final c = f.owner!.agentSessionControl();
      await expectLater(c.attachSession(pipeTask), throwsA(decoderError));
      expect(c.sessionOutcomeUnknown, true);
      await expectLater(c.attachSession(pipeTask), throwsA(unknown));
      expect(f.script.privateOps, [1]);
      expect(await f.owner!.readUiLocale(), 'en');
      f.script.largeLegacy = true;
      await expectLater(f.owner!.readUiLocale(), throwsA(isA<FormatException>()));
    } finally { await f.close(); }
  }, skip: skip, timeout: const Timeout(Duration(seconds: 30)));

  test('supervised original outer128KiB is rejected rather than enlarged for private pending', () async {
    final f = await start(SessionPipeScript()); f.outerBad = true;
    try {
      final c = f.owner!.agentSessionControl();
      await expectLater(c.attachSession(pipeTask), throwsA(isA<FormatException>().having(
        (e) => e.message, 'original outer budget', 'Supervisor frame budget',
      )));
      expect(c.sessionOutcomeUnknown, true);
      expect(f.script.privateOps, [1]);
    } finally { await f.close(expectedUnconfirmed: true); }
  }, skip: skip, timeout: const Timeout(Duration(seconds: 30)));

  test('supervised inner384KiB plus one is refused by original current private cap', () async {
    final f = await start(SessionPipeScript()..tooLargePrivate = true);
    try {
      final c = f.owner!.agentSessionControl();
      await expectLater(c.attachSession(pipeTask), throwsA(isA<FormatException>()));
      expect(c.sessionOutcomeUnknown, true);
      expect(f.script.privateOps, [1]);
    } finally { await f.close(); }
  }, skip: skip, timeout: const Timeout(Duration(seconds: 30)));

  test('supervised exact request SHA mismatch cannot become success or automatic reattach', () async {
    final f = await start(SessionPipeScript()); f.wrongHash = true;
    try {
      final c = f.owner!.agentSessionControl();
      await expectLater(c.attachSession(pipeTask), throwsA(decoderError));
      expect(c.sessionOutcomeUnknown, true);
      await expectLater(c.attachSession(pipeTask), throwsA(unknown));
      expect(f.script.privateOps, [1]);
    } finally { await f.close(); }
  }, skip: skip, timeout: const Timeout(Duration(seconds: 30)));

  test('supervised identity drift while pending fails original owner and never replays', () async {
    final f = await start(SessionPipeScript()); f.drift = true;
    try {
      final c = f.owner!.agentSessionControl();
      await expectLater(c.attachSession(pipeTask), throwsA(isA<FormatException>()));
      expect(c.sessionOutcomeUnknown, true);
      expect(c.supportsAgentSessions, false);
      await expectLater(c.attachSession(pipeTask), throwsA(isA<AgentSessionFailure>()));
      expect(f.script.privateOps, [1]);
    } finally { await f.close(expectedUnconfirmed: true); }
  }, skip: skip, timeout: const Timeout(Duration(seconds: 30)));

  test('supervised gate close before correlated reply decode prevents final delivery', () async {
    final f = await start(SessionPipeScript()); f.gate = true;
    try {
      final c = f.owner!.agentSessionControl();
      await expectLater(c.attachSession(pipeTask), throwsA(unknown));
      expect(c.sessionOutcomeUnknown, true);
      expect(c.supportsAgentSessions, false);
      expect(f.script.privateOps, [1]);
    } finally { await f.close(); }
  }, skip: skip, timeout: const Timeout(Duration(seconds: 30)));

  test('supervised admission is checked again when the original shared queue actually runs', () async {
    final f = await start(SessionPipeScript()); f.holdLocale = true;
    Future<String>? legacy; Future<AgentSessionResult>? request;
    try {
      final c = f.owner!.agentSessionControl();
      legacy = f.owner!.readUiLocale(); legacy.ignore();
      await f.localeSeen.future.timeout(const Duration(seconds: 5));
      request = c.attachSession(pipeTask); request.ignore();
      await f.status({'gate_closed': true});
      await _SupervisorFixture.until(() => !c.supportsAgentSessions);
      // Presentation can be changed by a listener; admission uses only the
      // independently validated original state, which remains gate closed.
      f.channel.supervision.value = WorkbenchSupervisionState({
        ...f.channel.supervision.value!.record,
        'phase': 'Running', 'gate_closed': false,
      });
      expect(f.channel.supervision.value!.record['gate_closed'], false);
      expect(f.channel.supportsPrivateControl, false);
      expect(c.supportsAgentSessions, false);
      f.releaseLocale.complete();
      expect(await legacy, 'en');
      await expectLater(request, throwsA(unknown));
      expect(c.sessionOutcomeUnknown, true);
      expect(f.script.privateFrames, isEmpty);
    } finally { await f.close(); }
  }, skip: skip, timeout: const Timeout(Duration(seconds: 30)));


  test('supervised public read-only discovery shares original queue and returns actual Rust None without session dispatch', () async {
    final f = await start(SessionPipeScript()); f.holdLocale = true;
    Future<String>? legacy; Future<AgentTaskObservation>? observation;
    try {
      final session = f.owner!.agentSessionControl();
      final discovery = f.owner!.agentTaskDiscovery();
      expect(identical(discovery, f.owner!.agentTaskDiscovery()), true);
      expect(discovery.supportsAgentTaskDiscovery, true);
      expect(session.sessionOutcomeUnknown, false);
      final actionsBefore = List<host.Action?>.from(f.legacyActions);
      final businessBefore = List<host.Action>.from(f.script.business.actions);
      legacy = f.owner!.readUiLocale(); legacy.ignore();
      await f.localeSeen.future.timeout(const Duration(seconds: 5));
      observation = discovery.observeAgentTask(); observation.ignore();
      await Future<void>.delayed(Duration.zero);
      expect(f.discoveryFrames, isEmpty); // The original queue is still held.
      f.releaseLocale.complete();
      expect(await legacy, 'en');
      final result = await observation;
      expect(result.kind, AgentTaskKind.none);
      expect(result.taskKey, isNull);
      expect(session.sessionOutcomeUnknown, false);
      expect(f.legacyActions, [...actionsBefore, host.Action.readUiLocale]);
      expect(f.script.business.actions, businessBefore);
      expect(f.script.business.adminFrames, isEmpty);
      expect(f.script.privateFrames, isEmpty); // No PS30 Attach/Open/Start.
      await Future<void>.delayed(Duration.zero);
      expect(f.discoveryFrames, hasLength(1)); // No automatic polling/resend.
    } finally { await f.close(); }
  }, skip: discoverySkip, timeout: const Timeout(Duration(seconds: 30)));

  test('supervised public discovery rejects inner16KiB plus one and never resends', () async {
    final f = await start(SessionPipeScript()); f.oversizedDiscovery = true;
    try {
      final session = f.owner!.agentSessionControl();
      final discovery = f.owner!.agentTaskDiscovery();
      final actionsBefore = List<host.Action?>.from(f.legacyActions);
      await expectLater(discovery.observeAgentTask(), throwsA(isA<FormatException>()));
      expect(discovery.supportsAgentTaskDiscovery, false);
      await expectLater(discovery.observeAgentTask(), throwsStateError);
      await Future<void>.delayed(Duration.zero);
      expect(f.discoveryFrames, hasLength(1));
      expect(f.legacyActions, actionsBefore);
      expect(f.script.privateFrames, isEmpty);
      expect(f.script.business.adminFrames, isEmpty);
      expect(session.sessionOutcomeUnknown, false); // No session request existed.
    } finally { await f.close(); }
  }, skip: discoverySkip, timeout: const Timeout(Duration(seconds: 30)));


  test('supervised forged presentation Released cannot replace absent original release proof', () async {
    final f = await start(SessionPipeScript());
    final omit = File('${f.directory.path}/omit-release-proof');
    final allow = File('${f.directory.path}/allow-exit-without-proof');
    try {
      expect(await omit.exists(), false);
      await omit.writeAsString('ordinary synthetic omission only', flush: true);
      await f.channel.closeInput();
      await _SupervisorFixture.until(() => f.channel.supervision.value?.phase == 'Closing');
      // The Python peer is held after its actual Closing observation. This
      // public-only forgery must never become original validated owner proof.
      f.channel.supervision.value = WorkbenchSupervisionState({
        ...f.channel.supervision.value!.record,
        'phase': 'Released', 'owner_phase': 'Released',
        'normal_shutdown': true, 'durable_proof': true, 'child_exit_code': 0,
        for (final key in ['resource_reclaimed', 'tree_empty', 'child_exit',
          'stdout_eof', 'stderr_eof', 'control_reaped', 'gate_closed']) key: true,
      });
      expect(f.channel.supervision.value!.released, true);
      expect(f.channel.supportsPrivateControl, false);
      expect(await allow.exists(), false);
      await allow.writeAsString('ordinary exit0 without Released frame', flush: true);
      final result = await f.close(expectedUnconfirmed: true);
      expect(result['graceful_close_requested'], true);
      expect(result['release_proof_deliberately_omitted'], true);
      expect(result['synthetic_protocol_released'], false);
    } finally {
      // Retain all files and unblock only this ordinary fixture if an earlier
      // assertion fails; never fabricate Released or kill/retry any process.
      if (!await allow.exists()) {
        await allow.writeAsString('release synthetic hold after assertion', flush: true);
      }
      if (!f._closed) await f.close(expectedUnconfirmed: true);
    }
  }, skip: skip, timeout: const Timeout(Duration(seconds: 30)));

  test('supervised lost terminal Read seals original token while heartbeat and graceful close remain ordinary', () async {
    final f = await start(SessionPipeScript()..dropRead = true);
    try {
      final c = f.owner!.agentSessionControl();
      f.holdPrivate = true;
      final attach = c.attachSession(pipeTask);
      await f.privateSeen.future.timeout(const Duration(seconds: 5));
      await Future<void>.delayed(const Duration(milliseconds: 650));
      f.releasePrivate.complete(); f.holdPrivate = false;
      await attach;
      await c.openSession(pipeTask, pipeAttachment);
      await expectLater(c.readSession(pipeTask, pipeAttachment, pipeToken), throwsA(anything));
      await _SupervisorFixture.until(() => !c.supportsAgentSessions);
      expect(c.sessionOutcomeUnknown, true);
      final before = f.script.privateFrames.length;
      await expectLater(c.readSession(pipeTask, pipeAttachment, pipeToken), throwsA(isA<AgentSessionFailure>()));
      expect(f.script.privateFrames.length, before);
      expect(f.script.privateOps, [1, 2, 5]);
    } finally {
      final result = await f.close(expectedUnconfirmed: true);
      expect(result['heartbeats'], greaterThanOrEqualTo(2));
      expect(result['synthetic_protocol_released'], false);
    }
  }, skip: skip, timeout: const Timeout(Duration(seconds: 30)));
}
