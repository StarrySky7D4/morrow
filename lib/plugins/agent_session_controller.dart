import 'dart:async';
import 'dart:typed_data';
import 'agent_session_control.dart';
import 'agent_session_models.dart';

enum AgentSessionPhase {
  detached,
  attached,
  pending,
  opened,
  unknown,
  staleBackend,
  disposed,
}

final class AgentSessionView {
  const AgentSessionView({
    required this.phase,
    required this.busy,
    required this.outcomeUnknown,
    required this.leaseLive,
    required this.submission,
    required this.generation,
    required this.lastData,
    required this.writerSessionId,
  });
  final AgentSessionPhase phase;
  final bool busy, outcomeUnknown, leaseLive;
  final BigInt? submission;
  final BigInt generation;
  final AgentSessionData? lastData;
  final String? writerSessionId;
}

/// Nonblocking presentation controller. Every transport action is explicit;
/// no timers, polling loops, automatic resubmission or owner adoption.
/// dispose() stops presentation only: it is not a remote detach/join proof.
final class AgentSessionController {
  AgentSessionController(AgentSessionControl backend, List<int> taskKey)
    : _backend = backend,
      taskKey = AgentSessionValidation.bytes(taskKey, 32, nonzero32: true);
  AgentSessionControl _backend;
  final Uint8List taskKey;
  final _changes = StreamController<AgentSessionView>.broadcast();
  Stream<AgentSessionView> get changes => _changes.stream;
  AgentSessionPhase _phase = AgentSessionPhase.detached;
  bool _busy = false, _unknown = false, _leaseLive = false, _disposed = false;
  bool _everAttached = false, _readUncertain = false;
  int _epoch = 0;
  BigInt _attachment = BigInt.zero, _generation = BigInt.zero;
  BigInt? _submission;
  AgentSessionCommand? _command;
  AgentSessionOperation? _pendingOperation;
  AgentSessionData? _lastData;
  AgentSessionWriterAdmitted? _writer;
  AgentSessionInfo? _writerInfo;
  AgentSessionView get view => AgentSessionView(
    phase: _phase,
    busy: _busy,
    outcomeUnknown: _unknown || _backend.sessionOutcomeUnknown,
    leaseLive: _leaseLive,
    submission: _submission,
    generation: _generation,
    lastData: _lastData,
    writerSessionId: _writer?.sessionId,
  );

  // Presentation affordances only. Each explicit operation still checks its
  // original preflight; these getters neither contact the backend nor grant rights.
  bool get _canObserveAttached =>
      !_disposed &&
      _phase != AgentSessionPhase.staleBackend &&
      !_busy &&
      _backend.supportsAgentSessions &&
      _attachment != BigInt.zero &&
      _phase != AgentSessionPhase.detached;

  // Unknown permits draining a known, not-yet-consumed original token. A lost
  // Read remains sealed by _readUncertain and can never be retried here.
  bool get canRead =>
      _canObserveAttached && _submission != null && !_readUncertain;
  bool get canCloseWriter => _canObserveAttached && _writer != null;
  AgentSessionOperation? get pendingOperation => _pendingOperation;

  void _changed() {
    if (!_disposed) _changes.add(view);
  }

  void _check({bool effect = false, bool attached = true}) {
    if (_disposed) {
      throw const AgentSessionFailure(AgentSessionFailureKind.disposed);
    }
    if (_phase == AgentSessionPhase.staleBackend) {
      throw const AgentSessionFailure(AgentSessionFailureKind.staleBackend);
    }
    if (_busy) throw const AgentSessionFailure(AgentSessionFailureKind.busy);
    if (!_backend.supportsAgentSessions) {
      throw const AgentSessionFailure(AgentSessionFailureKind.unavailable);
    }
    if (attached &&
        (_attachment == BigInt.zero || _phase == AgentSessionPhase.detached)) {
      throw const AgentSessionFailure(AgentSessionFailureKind.invalid);
    }
    if (effect && (_unknown || _backend.sessionOutcomeUnknown)) {
      throw const AgentSessionFailure(AgentSessionFailureKind.unknown);
    }
  }

  /// Called from the original widget's didUpdateWidget/owner change. Old
  /// completion is discarded. Construct a separately reviewed controller for
  /// a new owner; this object cannot attach it to the old task automatically.
  void replaceBackend(AgentSessionControl backend) {
    if (identical(_backend, backend)) return;
    _epoch++;
    _backend = backend;
    _busy = false;
    _unknown = true;
    _leaseLive = false;
    _phase = AgentSessionPhase.staleBackend;
    _changed();
  }

  Future<AgentSessionResult> _perform(
    AgentSessionOperation operation,
    Future<AgentSessionResult> Function(AgentSessionControl) action, {
    bool effect = false,
    bool attaching = false,
  }) async {
    _check(effect: effect, attached: !attaching);
    final backend = _backend, epoch = _epoch, previousUnknown = _unknown;
    var definiteRejection = false;
    _busy = true;
    if (effect) _unknown = true; // Before the possible effect and every await.
    _changed();
    try {
      final result = await action(backend);
      if (_disposed || epoch != _epoch || !identical(backend, _backend)) {
        throw const AgentSessionFailure(AgentSessionFailureKind.staleBackend);
      }
      if (result.operation != operation ||
          !AgentSessionValidation.same(result.taskKey, taskKey) ||
          (attaching
              ? result.status == AgentSessionStatus.attached &&
                    result.attachment == BigInt.zero
              : result.attachment != _attachment)) {
        throw const FormatException('Session reply binding changed');
      }
      final definiteFailure = switch (result.status) {
        AgentSessionStatus.unsupported ||
        AgentSessionStatus.unavailable ||
        AgentSessionStatus.busy ||
        AgentSessionStatus.invalid ||
        AgentSessionStatus.limit => true,
        _ => false,
      };
      if (definiteFailure) {
        _unknown =
            previousUnknown ||
            result.deliveryUnknown ||
            backend.sessionOutcomeUnknown;
        // Only this correlated, bound rejection proves no effect. Exception
        // types thrown by action(backend) cannot supply that proof.
        definiteRejection = true;
        if (!result.leaseLive) _leaseLive = false;
        if (_unknown) _phase = AgentSessionPhase.unknown;
        throw AgentSessionFailure(
          result.status == AgentSessionStatus.busy
              ? AgentSessionFailureKind.busy
              : result.status == AgentSessionStatus.unsupported ||
                    result.status == AgentSessionStatus.unavailable
              ? AgentSessionFailureKind.unavailable
              : AgentSessionFailureKind.invalid,
        );
      }
      if (_generation != BigInt.zero &&
          result.endpointGeneration != _generation &&
          result.status != AgentSessionStatus.unknown &&
          operation != AgentSessionOperation.detach) {
        throw const FormatException('Original endpoint generation changed');
      }
      _accept(operation, result);
      _unknown =
          previousUnknown ||
          operation == AgentSessionOperation.markLost ||
          result.deliveryUnknown ||
          backend.sessionOutcomeUnknown ||
          result.status == AgentSessionStatus.unknown ||
          (result.data is AgentSessionRejected &&
              (result.data as AgentSessionRejected).uncertain);
      if (_unknown && _phase != AgentSessionPhase.detached) {
        _phase = AgentSessionPhase.unknown;
      }
      return result;
    } catch (_) {
      if (epoch == _epoch && !_disposed) {
        // The backend may have consumed a Read or revoked a writer before
        // throwing any typed failure. Never infer non-delivery from its kind.
        if (!definiteRejection) _unknown = true;
        if (_unknown) _phase = AgentSessionPhase.unknown;
      }
      rethrow;
    } finally {
      if (epoch == _epoch && !_disposed) {
        _busy = false;
        _changed();
      }
    }
  }

  void _accept(AgentSessionOperation operation, AgentSessionResult result) {
    final status = result.status;
    if (status == AgentSessionStatus.unknown) return;
    if (operation == AgentSessionOperation.attach &&
        status == AgentSessionStatus.attached) {
      _attachment = result.attachment;
      _everAttached = true;
      _phase = AgentSessionPhase.attached;
    } else if ((operation == AgentSessionOperation.open ||
            operation == AgentSessionOperation.submit ||
            operation == AgentSessionOperation.admitWriter) &&
        status == AgentSessionStatus.submitted) {
      if (result.submission == BigInt.zero ||
          _submission != null ||
          result.data != null) {
        throw const FormatException('Invalid session submission receipt');
      }
      _submission = result.submission;
      _pendingOperation = operation;
      _readUncertain = false;
      _phase = AgentSessionPhase.pending;
    } else if (operation == AgentSessionOperation.status &&
        status == AgentSessionStatus.observed) {
      if (result.data != null ||
          result.submission != (_submission ?? BigInt.zero)) {
        throw const FormatException(
          'Status cannot consume or adopt a submission',
        );
      }
    } else if (operation == AgentSessionOperation.read) {
      if (result.submission != _submission) {
        throw const FormatException('Read token changed');
      }
      if (status == AgentSessionStatus.pending) {
        if (result.data != null) {
          throw const FormatException('Pending reply has data');
        }
        _readUncertain =
            false; // Explicit Pending proves no terminal consumption.
      } else {
        if (_pendingOperation == AgentSessionOperation.open) {
          if (status != AgentSessionStatus.opened ||
              result.data != null ||
              result.endpointGeneration == BigInt.zero) {
            throw const FormatException('Invalid open completion');
          }
          _generation = result.endpointGeneration;
        } else if (_pendingOperation == AgentSessionOperation.admitWriter) {
          if (status != AgentSessionStatus.writerAdmitted ||
              result.data is! AgentSessionWriterAdmitted ||
              (result.data as AgentSessionWriterAdmitted).sessionId !=
                  (_command as AgentSessionAdmitWriter).sessionId) {
            throw const FormatException(
              'Invalid exact-session writer admission',
            );
          }
          _writer = result.data as AgentSessionWriterAdmitted;
        } else {
          if (status != AgentSessionStatus.reply || result.data == null) {
            throw const FormatException('Missing session completion');
          }
          _validateData(_command!, result.data!);
        }
        _lastData = result.data;
        _submission = null;
        _pendingOperation = null;
        _command = null;
        _phase = AgentSessionPhase.opened;
      }
    } else if (operation == AgentSessionOperation.markLost &&
        status == AgentSessionStatus.observed) {
      _unknown = true;
    } else if (operation == AgentSessionOperation.closeWriter &&
        status == AgentSessionStatus.observed) {
      _writer = null;
      _writerInfo = null;
    } else if (operation == AgentSessionOperation.detach &&
        status == AgentSessionStatus.detached) {
      _phase = AgentSessionPhase.detached;
      _writer = null;
      _writerInfo = null;
      _leaseLive = false;
    } else {
      throw const FormatException('Unexpected session operation result');
    }
    if (operation != AgentSessionOperation.detach) {
      _leaseLive = result.leaseLive;
    }
  }

  void _validateData(AgentSessionCommand command, AgentSessionData data) {
    if (data is AgentSessionRejected) {
      return; // R2 rejection remains a rejection.
    }
    if (command is AgentSessionList && data is AgentSessionInfos) return;
    if (command is AgentSessionCreate &&
        data is AgentSessionInfo &&
        data.sessionId == command.sessionId &&
        data.parent == command.parent &&
        data.parentTail == command.parentTail &&
        data.epoch == BigInt.zero &&
        data.tail == BigInt.zero &&
        data.floor == BigInt.one &&
        !data.archived &&
        data.checkpointTail == BigInt.zero &&
        (command.parent == null
            ? !data.checkpointSealed
            : data.checkpointSealed &&
                  AgentSessionValidation.same(
                    data.checkpointSha256,
                    data.parentCheckpointSha256,
                  ))) {
      return;
    }
    if (command is AgentSessionSnapshotRequest &&
        data is AgentSessionSnapshot &&
        data.info.sessionId == command.sessionId &&
        data.events.length <= command.limit &&
        data.events.every((e) => e.sequence > command.after)) {
      final start = (command.after + BigInt.one) > data.info.floor
          ? command.after + BigInt.one
          : data.info.floor;
      final available = start > data.info.tail
          ? BigInt.zero
          : data.info.tail - start + BigInt.one;
      final count = available < BigInt.from(command.limit)
          ? available.toInt()
          : command.limit;
      if (command.after > data.info.tail ||
          data.gap != (command.after + BigInt.one < data.info.floor) ||
          data.events.length != count ||
          data.events.isNotEmpty && data.events.first.sequence != start) {
        throw const FormatException(
          'Snapshot range does not match the pending request',
        );
      }
      if (command.writerSlot != null) {
        _writerInfo = _writerInfo?.epoch == data.info.epoch ? data.info : null;
      }
      return;
    }
    if (command is AgentSessionOpenWriter &&
        data is AgentSessionInfo &&
        data.sessionId == command.sessionId &&
        data.epoch == command.expectedEpoch + BigInt.one &&
        !data.archived) {
      _writerInfo = data;
      return;
    }
    if (command is AgentSessionAppend &&
        data is AgentSessionInfo &&
        data.sessionId == command.sessionId &&
        data.epoch == command.epoch &&
        data.tail ==
            command.expectedTail + BigInt.from(command.events.length) &&
        !data.archived) {
      _writerInfo = data;
      return;
    }
    throw const FormatException(
      'Session data does not match the pending command',
    );
  }

  Future<AgentSessionResult> attach() {
    _check(effect: true, attached: false);
    if (_everAttached) {
      throw const AgentSessionFailure(AgentSessionFailureKind.invalid);
    }
    return _perform(
      AgentSessionOperation.attach,
      (b) => b.attachSession(taskKey),
      effect: true,
      attaching: true,
    );
  }

  Future<AgentSessionResult> open() {
    _requireNoPending();
    if (_generation != BigInt.zero) {
      throw const AgentSessionFailure(AgentSessionFailureKind.invalid);
    }
    return _perform(
      AgentSessionOperation.open,
      (b) => b.openSession(taskKey, _attachment),
      effect: true,
    );
  }

  void _requireNoPending() {
    _check(effect: true);
    if (_submission != null) {
      throw const AgentSessionFailure(AgentSessionFailureKind.busy);
    }
  }

  Future<AgentSessionResult> _submit(AgentSessionCommand command) {
    _requireNoPending();
    if (_generation == BigInt.zero || !_leaseLive) {
      throw const AgentSessionFailure(AgentSessionFailureKind.unavailable);
    }
    _command = command;
    return _perform(
      AgentSessionOperation.submit,
      (b) => b.submitSession(taskKey, _attachment, _generation, command),
      effect: true,
    );
  }

  Future<AgentSessionResult> createSession(
    String id, {
    String? parent,
    BigInt? parentTail,
  }) => _submit(AgentSessionCreate(id, parent: parent, parentTail: parentTail));
  Future<AgentSessionResult> listSessions() =>
      _submit(const AgentSessionList());
  Future<AgentSessionResult> snapshotSession(
    String id, {
    required BigInt after,
    int limit = 16,
  }) => _submit(AgentSessionSnapshotRequest(id, after: after, limit: limit));
  Future<AgentSessionResult> admitWriter(String id) {
    _requireNoPending();
    if (_writer != null || _generation == BigInt.zero || !_leaseLive) {
      throw const AgentSessionFailure(AgentSessionFailureKind.invalid);
    }
    _command = AgentSessionAdmitWriter(id);
    return _perform(
      AgentSessionOperation.admitWriter,
      (b) => b.admitSessionWriter(taskKey, _attachment, id),
      effect: true,
    );
  }

  AgentSessionWriterAdmitted _requireWriter() {
    final writer = _writer;
    if (writer == null) {
      throw const AgentSessionFailure(AgentSessionFailureKind.invalid);
    }
    return writer;
  }

  Future<AgentSessionResult> openWriter(BigInt expectedEpoch) {
    final writer = _requireWriter();
    return _submit(
      AgentSessionOpenWriter(writer.sessionId, writer.slot, expectedEpoch),
    );
  }

  Future<AgentSessionResult> append(List<AgentSessionEvent> events) {
    final writer = _requireWriter(), info = _writerInfo;
    if (info == null || info.archived) {
      throw const AgentSessionFailure(AgentSessionFailureKind.invalid);
    }
    return _submit(
      AgentSessionAppend(
        writer.sessionId,
        writer.slot,
        info.epoch,
        info.tail,
        events,
      ),
    );
  }

  Future<AgentSessionResult> snapshotWriter({
    required BigInt after,
    int limit = 16,
  }) {
    final writer = _requireWriter();
    return _submit(
      AgentSessionSnapshotRequest(
        writer.sessionId,
        after: after,
        limit: limit,
        writerSlot: writer.slot,
      ),
    );
  }

  Future<AgentSessionResult> closeWriter() {
    final writer = _requireWriter();
    return _perform(
      AgentSessionOperation.closeWriter,
      (b) => b.closeSessionWriter(taskKey, _attachment, writer.slot),
    );
  }

  Future<AgentSessionResult> status() => _perform(
    AgentSessionOperation.status,
    (b) => b.sessionStatus(taskKey, _attachment),
  );
  Future<AgentSessionResult> read() {
    _check();
    final token = _submission;
    if (token == null || _readUncertain) {
      throw const AgentSessionFailure(AgentSessionFailureKind.unknown);
    }
    _readUncertain =
        true; // A lost terminal read cannot be consumed a second time.
    return _perform(
      AgentSessionOperation.read,
      (b) => b.readSession(taskKey, _attachment, token),
    );
  }

  Future<AgentSessionResult> markLost() => _perform(
    AgentSessionOperation.markLost,
    (b) => b.markSessionLost(taskKey, _attachment),
  );
  Future<AgentSessionResult> detach() => _perform(
    AgentSessionOperation.detach,
    (b) => b.detachSession(taskKey, _attachment),
  );
  Future<void> dispose() async {
    if (_disposed) return;
    _disposed = true;
    _epoch++;
    _phase = AgentSessionPhase.disposed;
    await _changes.close();
  }
}
