part of 'workbench_native.dart';

final class _QualificationPair {
  _QualificationPair(List<int> request, List<int> reply)
      : request = sha256.convert(request).bytes,
        reply = sha256.convert(reply).bytes;
  final List<int> request, reply;
}

final class _QualificationSelection {
  _QualificationSelection(this.plan, this.canonicalProfile);
  final QualificationSessionPlan plan;
  final String canonicalProfile;
  bool openCompleted = false;
  Object? failure;
  _QualificationPair? startup, none, retained;
  Uint8List? retainedKey;
  AgentTaskKind? latestKind;
  final _trace = <Map<String, Object?>>[];
  int _traceBytes = 0;
  void fence(Object error) { failure ??= error; }
  void recordStartup(Uint8List request, Uint8List reply) {
    if (startup != null || openCompleted || request.length > 131072 || reply.length > 131072) {
      throw StateError('Qualification startup marker bounds');
    }
    _append('startup', request, reply);
    startup = _QualificationPair(request, reply);
  }
  void record(String kind, Uint8List request, Uint8List reply, Object? decoded) {
    _append(kind, request, reply);
    if (kind == 'AD31') {
      if (decoded is! AgentTaskObservation) throw StateError('Actual AD31 decode missing');
      latestKind = decoded.kind;
      if (decoded.kind == AgentTaskKind.none) none = _QualificationPair(request, reply);
      if (decoded.kind == AgentTaskKind.retainedAgentExit) {
        retained = _QualificationPair(request, reply);
        retainedKey = Uint8List.fromList(decoded.taskKey!).asUnmodifiableView();
      }
    }
  }
  void _append(String kind, Uint8List request, Uint8List reply) {
    final bytes = request.length + reply.length;
    requireTraceCapacity(request.length,reply.length);
    _traceBytes += bytes;
    _trace.add(Map.unmodifiable({
      'kind': kind, 'request': base64Encode(request), 'reply': base64Encode(reply),
      'request_bytes': request.length, 'reply_bytes': reply.length,
      'request_sha256': sha256.convert(request).toString(),
      'reply_sha256': sha256.convert(reply).toString(),
    }));
  }
  void requireTraceCapacity(int requestBytes,int replyBytes) {
    if (_trace.length >= 4096 || _traceBytes + requestBytes + replyBytes > 8 * 1024 * 1024) {
      throw StateError('Qualification bounded evidence exhausted');
    }
  }
  List<Map<String, Object?>> get trace => List.unmodifiable(_trace);
}

String _qualificationCanonical(String value) {
  var result = value.replaceAll('\\', '/').toLowerCase();
  if (result.startsWith('//?/')) result = result.substring(4);
  if (RegExp(r'^[a-z]:/$').hasMatch(result)) return result;
  return qualificationWindowsPath(result);
}
Future<void> _qualificationPlainAncestors(String path, {required bool directory}) async {
  if (!(directory && RegExp(r'^[A-Za-z]:/$').hasMatch(path))) qualificationWindowsPath(path);
  final drive = path.substring(0,3);
  if (await FileSystemEntity.type(drive,followLinks:false) != FileSystemEntityType.directory ||
      _qualificationCanonical(await Directory(drive).resolveSymbolicLinks()) != drive.toLowerCase()) {
    throw StateError('Qualification plain drive root required');
  }
  if (path.length == 3 && directory) return;
  final parts = path.substring(3).split('/');
  var current = path.substring(0, 3);
  for (var i = 0; i < parts.length; i++) {
    current = '$current${i == 0 ? '' : '/'}${parts[i]}';
    final expected = i == parts.length - 1 && !directory
        ? FileSystemEntityType.file : FileSystemEntityType.directory;
    if (await FileSystemEntity.type(current, followLinks: false) != expected) {
      throw StateError('Qualification requires plain existing ancestors');
    }
    final resolved = expected == FileSystemEntityType.directory
        ? await Directory(current).resolveSymbolicLinks()
        : await File(current).resolveSymbolicLinks();
    if (_qualificationCanonical(resolved) != _qualificationCanonical(current)) {
      throw StateError('Qualification ancestor alias/reparse rejected');
    }
  }
}
Future<Uint8List> _qualificationPinnedRead(String path, int bytes, String digest) async {
  await _qualificationPlainAncestors(path, directory: false);
  final file = File(path), before = await File(path).stat();
  if (before.type != FileSystemEntityType.file || before.size != bytes) {
    throw StateError('Qualification input size/type');
  }
  final handle = await file.open(mode: FileMode.read);
  try {
    if (await handle.length() != bytes) throw StateError('Qualification input length drift');
    final raw = await handle.read(bytes + 1);
    final after = await file.stat();
    if (raw.length != bytes || await handle.length() != bytes || after.size != bytes ||
        before.modified != after.modified || sha256.convert(raw).toString() != digest) {
      throw StateError('Qualification input pin/drift');
    }
    await _qualificationPlainAncestors(path, directory: false);
    return raw;
  } finally { await handle.close(); }
}
Future<_QualificationSelection> _qualificationPreflight(
    QualificationSessionConfiguration configuration, {required String executable,
    required String package, required Directory directory, String? supervisorExecutable,
    required bool managed, required void Function(RustWorkbench)? onStarted}) async {
  configuration.requireCompiled();
  if (!Platform.isWindows || supervisorExecutable == null || managed || onStarted != null) {
    throw StateError('Qualification requires original supervised startup without early callback');
  }
  final raw = await _qualificationPinnedRead(configuration.planPath,
      configuration.planBytes, configuration.planSha256);
  final plan = QualificationSessionPlan.decode(configuration, raw);
  if (_qualificationCanonical(executable) != _qualificationCanonical(plan.host.path) ||
      _qualificationCanonical(supervisorExecutable) != _qualificationCanonical(plan.supervisor.path) ||
      _qualificationCanonical(package) != _qualificationCanonical(plan.package.path) ||
      _qualificationCanonical(directory.path) != _qualificationCanonical(plan.profileRoot)) {
    throw StateError('Qualification launch arguments differ from exact plan');
  }
  final slash = plan.profileRoot.lastIndexOf('/');
  final parent = plan.profileRoot.substring(0, slash == 2 ? 3 : slash);
  await _qualificationPlainAncestors(parent, directory: true);
  if (await FileSystemEntity.type(plan.profileRoot, followLinks: false) != FileSystemEntityType.notFound) {
    throw StateError('Qualification profile already exists; no adoption');
  }
  final canonicalParent = _qualificationCanonical(await Directory(parent).resolveSymbolicLinks());
  final expected = '$canonicalParent${canonicalParent.endsWith('/') ? '' : '/'}${plan.profileRoot.split('/').last}';
  final artifactIdentities = <String>{};
  for (final pin in [plan.host, plan.supervisor, plan.package, plan.fixture]) {
    if (_qualificationCanonical(pin.path).startsWith('$expected/')) {
      throw StateError('Qualification inputs cannot be inside fresh profile');
    }
    await _qualificationPinnedRead(pin.path, pin.bytes, pin.sha256);
    final canonical = _qualificationCanonical(await File(pin.path).resolveSymbolicLinks());
    if (!artifactIdentities.add(canonical)) {
      throw StateError('Qualification distinct artifact roles alias one canonical file');
    }
  }
  if (await FileSystemEntity.type(plan.profileRoot, followLinks: false) != FileSystemEntityType.notFound) {
    throw StateError('Qualification profile appeared before dispatch');
  }
  // No mkdir here. Supervisor performs exclusive CreateNew before Owner::claim.
  return _QualificationSelection(plan, expected);
}

/// Narrow selected capability. No public constructor, channel, owner or codec
/// injection. All four operations use this Workbench's existing queue/pipe.
final class QualificationSessionControl {
  QualificationSessionControl._(this._backend, this._selection);
  final RustWorkbench _backend;
  final _QualificationSelection _selection;
  final QualificationSessionAttemptState _state = QualificationSessionAttemptState();
  final _ids = <String>{};
  final _random = Random.secure();
  Stopwatch? _business, _join;
  bool _busy = false;
  int _polls = 0;
  QualificationSessionReply? _last;
  List<int>? get taskKey => _state.taskKey;
  List<int>? get acknowledgedTask => _state.acknowledgedTask;
  Object? get failure => _selection.failure;
  List<Map<String, Object?>> get trace => _selection.trace;
  QualificationSessionPlan get plan => _selection.plan;
  Map<String, dynamic> get originalIdentity =>
      (_backend.channel as SupervisedWorkbenchChannel).qualificationValidatedIdentity;
  void evidencePersistenceFailed(Object error) => _selection.fence(error);
  void checkBusinessBudget() {
    if (failure != null || _business == null || _business!.elapsedMilliseconds > 40000) {
      final error = StateError('Qualification business permanently fenced/deadline');
      _selection.fence(error); throw error;
    }
  }
  Duration _remainingBusiness() {
    checkBusinessBudget();
    return Duration(milliseconds: 40000 - _business!.elapsedMilliseconds);
  }
  Duration _remainingJoin() {
    if (_join == null || _join!.elapsedMilliseconds >= 20000) {
      throw StateError('Qualification bounded original join expired');
    }
    return Duration(milliseconds: 20000 - _join!.elapsedMilliseconds);
  }
  Future<QualificationSessionReply> start() => _invoke(QualificationSessionOperation.start);
  Future<QualificationSessionReply> stop() => _invoke(QualificationSessionOperation.stop);
  Future<QualificationSessionReply> poll() => _invoke(QualificationSessionOperation.poll);
  Future<QualificationSessionReply> acknowledge() => _invoke(QualificationSessionOperation.acknowledge);

  Future<QualificationSessionReply> _invoke(QualificationSessionOperation operation) async {
    if (_busy) {
      final error = StateError('Qualification operation already pending');
      _selection.fence(error); throw error;
    }
    _busy = true;
    try {
      // Permanent effect latches precede every fallible preflight/dispatch.
      _state.reserve(operation);
      final starting = operation == QualificationSessionOperation.start;
      final ack = operation == QualificationSessionOperation.acknowledge;
      if ((starting || ack) && failure != null) throw failure!;
      if (!_selection.openCompleted || !_backend._supportsPrivateControl) {
        throw StateError('Qualification original owner gate unavailable');
      }
      final identity = originalIdentity;
      if (identity['generation'] != 1 || identity['phase'] != 'Running' ||
          identity['gate_closed'] != false) {
        throw StateError('Qualification original identity gate');
      }
      _QualificationPair? barrier;
      if (starting) {
        _state.requireStartBarrier(originalOpenCompleted:_selection.openCompleted,
          actualNoneDecoded:_selection.latestKind == AgentTaskKind.none && _selection.none != null,
          startupRequestDigest:_selection.startup?.request,
          startupReplyDigest:_selection.startup?.reply);
        barrier = _selection.startup;
        _business = Stopwatch()..start();
      } else if (operation == QualificationSessionOperation.stop) {
        _join = Stopwatch()..start();
      } else if (operation == QualificationSessionOperation.poll) {
        if (++_polls > 1000 || _join == null || _join!.elapsedMilliseconds > 20000) {
          throw StateError('Qualification bounded original join exhausted');
        }
      } else if (ack) {
        final view = _backend.agentSessions.view;
        if (_last?.storage != QualificationSessionStorage.reclaimed ||
            _last?.healthyReclaimed != true || view.session?.phase != AgentSessionPhase.detached ||
            view.session?.submission != null || view.outcomeUnknown ||
            _selection.latestKind != AgentTaskKind.retainedAgentExit ||
            _selection.retainedKey == null || taskKey == null ||
            !RustWorkbench._same(_selection.retainedKey, taskKey!)) {
          throw StateError('Qualification ACK requires actual healthy join/detach/same-key retained');
        }
        barrier = _selection.retained;
      }
      if (_ids.length >= 1024) throw StateError('Qualification request ID bound');
      final id = Uint8List.fromList(List.generate(32, (_) => _random.nextInt(256)));
      if (id.every((b) => b == 0) || !_ids.add(qualificationHexBytes(id))) {
        throw StateError('Qualification request ID collision; no retry');
      }
      final request = QualificationSessionRequest(operation: operation, id: id,
        planDigest: plan.planDigest, nonce: plan.nonce,
        task: starting ? Uint8List(32) : (taskKey ?? (throw StateError('Actual task unknown'))),
        barrierRequest: barrier?.request ?? Uint8List(32),
        barrierReply: barrier?.reply ?? Uint8List(32));
      final bytes = request.encode();
      _selection.requireTraceCapacity(188,256);
      Uint8List? actualReply;
      final reply = await _backend._exchangePrivate(bytes, (raw) {
        final decoded = QualificationSessionReply.decode(request, bytes, raw,
          incarnation: qualificationHex(identity['incarnation'] as String, 32),
          profile: qualificationHex(identity['profile'] as String, 32));
        // Fully correlated original identity/key validated first. Retain known
        // effects before any postgate, trace allocation or semantic rejection.
        _state.retainValidated(decoded, operation);
        _last = decoded;
        actualReply = Uint8List.fromList(raw);
        return decoded;
      }, requestLimit: 188, replyLimit: 256, qualificationRetainBeforeGate: true,
          qualificationBudget: starting ? _remainingBusiness :
              (ack ? null : _remainingJoin));
      _selection.record('qualification-${operation.name}', bytes,
          actualReply!, reply);
      // byte254 observes only an unstarted external prepared-context debt.
      // Successful original Start consumes that Option into its actual worker.
      if (reply.fenced) {
        throw StateError('Qualification original context fenced');
      }
      if (starting && reply.storage != QualificationSessionStorage.running) {
        throw StateError('Qualification actual Start did not reach Running');
      }
      return reply;
    } catch (error) { _selection.fence(error); rethrow; }
    finally { _busy = false; }
  }
}
