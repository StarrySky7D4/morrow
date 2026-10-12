import 'dart:async';
import 'dart:convert';
import 'agent_session_control.dart';
import 'agent_session_controller.dart';
import 'agent_session_models.dart';
import 'agent_task_discovery.dart';

/// A product capability on one original Workbench. It is not a task factory.
abstract interface class WorkbenchAgentSessionSupport {
  AgentSessionWorkspace get agentSessions;
}

enum AgentSessionDraft {
  sessionId,
  parent,
  parentTail,
  after,
  limit,
  eventId,
  eventBody,
}

final class AgentSessionWorkspaceView {
  const AgentSessionWorkspaceView({
    required this.closed,
    required this.discoveryBusy,
    required this.actionBusy,
    required this.observation,
    required this.session,
    required this.sessions,
    required this.selectedInfo,
    required this.snapshot,
    required this.drafts,
    required this.failure,
    required this.lastResult,
    required this.canObserve,
    required this.canAttach,
    required this.canOpen,
    required this.canSubmit,
    required this.canRead,
    required this.canStatus,
    required this.canAdmitWriter,
    required this.canOpenWriter,
    required this.canAppend,
    required this.canCloseWriter,
    required this.canDetach,
    required this.outcomeUnknown,
    required this.attachedTaskKey,
  });
  final bool closed, discoveryBusy, actionBusy;
  final AgentTaskObservation? observation;
  final AgentSessionView? session;
  final List<AgentSessionInfo> sessions;
  final AgentSessionInfo? selectedInfo;
  final AgentSessionSnapshot? snapshot;
  final Map<AgentSessionDraft, String> drafts;
  final Object? failure;
  final AgentSessionResult? lastResult;
  final bool canObserve, canAttach, canOpen, canSubmit, canRead, canStatus;
  final bool canAdmitWriter,
      canOpenWriter,
      canAppend,
      canCloseWriter,
      canDetach;
  final bool outcomeUnknown;
  final List<int>? attachedTaskKey;
}

/// Owned by the original Workbench, never by a temporary settings route.
/// Every transport action is explicit. Closing a view does not close this
/// workspace; disposing the owner seals presentation, without remote cleanup.
final class AgentSessionWorkspace {
  AgentSessionWorkspace({
    required AgentSessionControl control,
    required AgentTaskDiscovery discovery,
  }) :
       // Preserve the portable public named control/discovery constructor API.
       // ignore: prefer_initializing_formals
       _control = control,
       // ignore: prefer_initializing_formals
       _discovery = discovery;

  final AgentSessionControl _control;
  final AgentTaskDiscovery _discovery;
  final _changes = StreamController<AgentSessionWorkspaceView>.broadcast();
  Stream<AgentSessionWorkspaceView> get changes => _changes.stream;
  AgentSessionController? _controller;
  StreamSubscription<AgentSessionView>? _subscription;
  AgentTaskObservation? _observation;
  List<AgentSessionInfo> _sessions = const [];
  final _observedInfos = <String, AgentSessionInfo>{};
  AgentSessionInfo? _selectedInfo;
  AgentSessionSnapshot? _snapshot;
  AgentSessionResult? _lastResult;
  Object? _failure;
  bool _closed = false, _discoveryBusy = false, _actionBusy = false;
  bool _attachInvoked = false, _writerReady = false;
  BigInt? _writerEpoch;
  int _epoch = 0;
  final _drafts = <AgentSessionDraft, String>{
    AgentSessionDraft.sessionId: '',
    AgentSessionDraft.parent: '',
    AgentSessionDraft.parentTail: '0',
    AgentSessionDraft.after: '0',
    AgentSessionDraft.limit: '16',
    AgentSessionDraft.eventId: '',
    AgentSessionDraft.eventBody: '',
  };

  bool get closed => _closed;
  bool get _busy =>
      _discoveryBusy || _actionBusy || (_controller?.view.busy ?? false);
  bool get _attached {
    final s = _controller?.view;
    return !_closed &&
        !_busy &&
        _control.supportsAgentSessions &&
        s != null &&
        s.phase != AgentSessionPhase.detached &&
        s.phase != AgentSessionPhase.disposed &&
        s.phase != AgentSessionPhase.staleBackend;
  }

  bool get _effectReady =>
      _attached &&
      !_controller!.view.outcomeUnknown &&
      _controller!.view.submission == null;
  bool get _submitReady =>
      _effectReady &&
      _controller!.view.leaseLive &&
      _controller!.view.generation != BigInt.zero &&
      _controller!.view.phase == AgentSessionPhase.opened;

  AgentSessionWorkspaceView get view {
    final s = _controller?.view;
    final writerId = s?.writerSessionId;
    return AgentSessionWorkspaceView(
      closed: _closed,
      discoveryBusy: _discoveryBusy,
      actionBusy: _actionBusy,
      observation: _observation,
      session: s,
      sessions: List.unmodifiable(_sessions),
      selectedInfo: _selectedInfo,
      snapshot: _snapshot,
      drafts: Map.unmodifiable(_drafts),
      failure: _failure,
      lastResult: _lastResult,
      canObserve: !_closed && !_busy && _discovery.supportsAgentTaskDiscovery,
      canAttach:
          !_closed &&
          !_busy &&
          !_attachInvoked &&
          _control.supportsAgentSessions &&
          !_control.sessionOutcomeUnknown &&
          _observation?.kind == AgentTaskKind.runningAgent,
      canOpen:
          _effectReady &&
          s!.phase == AgentSessionPhase.attached &&
          s.generation == BigInt.zero,
      canSubmit: _submitReady,
      canRead: _attached && (_controller?.canRead ?? false),
      canStatus: _attached,
      canAdmitWriter: _submitReady && writerId == null,
      canOpenWriter:
          _submitReady &&
          writerId != null &&
          !_writerReady &&
          _observedInfos[writerId] != null &&
          !_observedInfos[writerId]!.archived,
      canAppend: _submitReady && writerId != null && _writerReady,
      canCloseWriter: _attached && (_controller?.canCloseWriter ?? false),
      canDetach: _attached,
      outcomeUnknown:
          (s?.outcomeUnknown ?? false) || _control.sessionOutcomeUnknown,
      attachedTaskKey: _controller?.taskKey,
    );
  }

  void _emit() {
    if (!_closed) _changes.add(view);
  }

  void _checkOpen() {
    if (_closed) {
      throw const AgentSessionFailure(AgentSessionFailureKind.disposed);
    }
  }

  void _require(bool allowed) {
    _checkOpen();
    if (!allowed) {
      throw const AgentSessionFailure(AgentSessionFailureKind.invalid);
    }
  }

  void setDraft(AgentSessionDraft field, String value) {
    _checkOpen();
    // Presentation drafts are bounded, but never silently trimmed or rewritten.
    if (value.length > 65536) {
      throw const FormatException('Draft exceeds limit');
    }
    _drafts[field] = value;
    _emit();
  }

  BigInt _number(AgentSessionDraft field) {
    final text = _drafts[field]!;
    if (!RegExp(r'^[0-9]+$').hasMatch(text)) {
      throw const FormatException('Expected unsigned decimal counter');
    }
    return AgentSessionValidation.u64(BigInt.parse(text));
  }

  Future<AgentTaskObservation> observe() async {
    _require(view.canObserve);
    final epoch = _epoch;
    _discoveryBusy = true;
    _failure = null;
    _emit();
    try {
      final observation = await _discovery.observeAgentTask();
      if (_closed || epoch != _epoch) {
        throw const AgentSessionFailure(AgentSessionFailureKind.staleBackend);
      }
      _observation = observation;
      // Existing controller/pending/drafts belong to their original task.
      // A different observed slot does not adopt or replace that controller.
      return observation;
    } catch (error) {
      if (!_closed && epoch == _epoch) _failure = error;
      rethrow;
    } finally {
      if (!_closed && epoch == _epoch) {
        _discoveryBusy = false;
        _emit();
      }
    }
  }

  Future<AgentSessionResult> _run(
    Future<AgentSessionResult> Function() action,
  ) async {
    _checkOpen();
    if (_busy) throw const AgentSessionFailure(AgentSessionFailureKind.busy);
    final epoch = _epoch;
    _actionBusy = true;
    _failure = null;
    _emit();
    try {
      final result = await action();
      if (_closed || epoch != _epoch) {
        throw const AgentSessionFailure(AgentSessionFailureKind.staleBackend);
      }
      _lastResult = result;
      if (result.operation == AgentSessionOperation.read) {
        if (result.status != AgentSessionStatus.pending &&
            result.status != AgentSessionStatus.unknown) {
          _acceptData(result.data);
        }
      }
      if (result.operation == AgentSessionOperation.closeWriter ||
          result.operation == AgentSessionOperation.detach) {
        _writerReady = false;
        _writerEpoch = null;
      }
      return result;
    } catch (error) {
      if (!_closed && epoch == _epoch) _failure = error;
      rethrow;
    } finally {
      if (!_closed && epoch == _epoch) {
        _actionBusy = false;
        _emit();
      }
    }
  }

  void _acceptData(AgentSessionData? data) {
    if (data is AgentSessionInfos) {
      _sessions = data.sessions;
      for (final info in data.sessions) {
        _recordInfo(info);
      }
    } else if (data is AgentSessionInfo) {
      _recordInfo(data);
      _selectedInfo = data;
      _snapshot = null;
      if (_controller!.view.writerSessionId == data.sessionId &&
          data.epoch > BigInt.zero) {
        _writerReady = !data.archived;
        _writerEpoch = data.epoch;
      }
    } else if (data is AgentSessionSnapshot) {
      _recordInfo(data.info);
      _selectedInfo = data.info;
      _snapshot = data;
    }
  }

  void _recordInfo(AgentSessionInfo info) {
    _observedInfos[info.sessionId] = info;
    if (_controller!.view.writerSessionId == info.sessionId &&
        (info.archived || _writerEpoch != info.epoch)) {
      _writerReady = false;
    }
  }

  void selectSession(String id) {
    _checkOpen();
    final info = _observedInfos[id];
    if (info == null) {
      throw const AgentSessionFailure(AgentSessionFailureKind.invalid);
    }
    if (_selectedInfo?.sessionId != id) _snapshot = null;
    _selectedInfo = info;
    _drafts[AgentSessionDraft.sessionId] = id;
    _emit();
  }

  Future<AgentSessionResult> attach() {
    _require(view.canAttach);
    final key = _observation!.taskKey!;
    _attachInvoked = true;
    final controller = _controller = AgentSessionController(_control, key);
    _subscription = controller.changes.listen((_) => _emit());
    return _run(controller.attach);
  }

  Future<AgentSessionResult> open() {
    _require(view.canOpen);
    return _run(_controller!.open);
  }

  Future<AgentSessionResult> listSessions() {
    _require(view.canSubmit);
    return _run(_controller!.listSessions);
  }

  Future<AgentSessionResult> createSession() {
    _require(view.canSubmit);
    final id = _drafts[AgentSessionDraft.sessionId]!;
    final parent = _drafts[AgentSessionDraft.parent]!;
    final tail = _number(AgentSessionDraft.parentTail);
    // Validate locally before registering a possibly effectful submission.
    AgentSessionCreate(
      id,
      parent: parent.isEmpty ? null : parent,
      parentTail: tail,
    );
    return _run(
      () => _controller!.createSession(
        id,
        parent: parent.isEmpty ? null : parent,
        parentTail: tail,
      ),
    );
  }

  Future<AgentSessionResult> snapshotSession() {
    _require(view.canSubmit);
    final id = _drafts[AgentSessionDraft.sessionId]!,
        after = _number(AgentSessionDraft.after);
    final limit = _number(AgentSessionDraft.limit);
    if (limit < BigInt.one || limit > BigInt.from(16)) {
      throw const FormatException('Invalid snapshot limit');
    }
    AgentSessionSnapshotRequest(id, after: after, limit: limit.toInt());
    return _run(
      () =>
          _controller!.snapshotSession(id, after: after, limit: limit.toInt()),
    );
  }

  Future<AgentSessionResult> admitWriter() {
    _require(view.canAdmitWriter);
    final id = AgentSessionValidation.identity(
      _drafts[AgentSessionDraft.sessionId]!,
    );
    return _run(() => _controller!.admitWriter(id));
  }

  Future<AgentSessionResult> openWriter() {
    _require(view.canOpenWriter);
    final id = _controller!.view.writerSessionId!;
    final epoch = _observedInfos[id]!.epoch;
    return _run(() => _controller!.openWriter(epoch));
  }

  Future<AgentSessionResult> append() {
    _require(view.canAppend);
    final event = AgentSessionEvent(
      _drafts[AgentSessionDraft.eventId]!,
      utf8.encode(_drafts[AgentSessionDraft.eventBody]!),
    );
    return _run(() => _controller!.append([event]));
  }

  Future<AgentSessionResult> snapshotWriter() {
    _require(view.canSubmit && view.session!.writerSessionId != null);
    final after = _number(AgentSessionDraft.after),
        limit = _number(AgentSessionDraft.limit);
    if (limit < BigInt.one || limit > BigInt.from(16)) {
      throw const FormatException('Invalid snapshot limit');
    }
    return _run(
      () => _controller!.snapshotWriter(after: after, limit: limit.toInt()),
    );
  }

  Future<AgentSessionResult> status() {
    _require(view.canStatus);
    return _run(_controller!.status);
  }

  Future<AgentSessionResult> read() {
    _require(view.canRead);
    return _run(_controller!.read);
  }

  Future<AgentSessionResult> markLost() {
    _require(view.canStatus);
    return _run(_controller!.markLost);
  }

  Future<AgentSessionResult> closeWriter() {
    _require(view.canCloseWriter);
    return _run(_controller!.closeWriter);
  }

  Future<AgentSessionResult> detach() {
    _require(view.canDetach);
    return _run(_controller!.detach);
  }

  /// ROOT calls this only when this original Workbench retires. Synchronous
  /// sealing precedes asynchronous local stream disposal; no backend call.
  void dispose() {
    if (_closed) return;
    _closed = true;
    _epoch++;
    _changes.add(view);
    final subscription = _subscription;
    if (subscription != null) unawaited(subscription.cancel());
    final controller = _controller;
    if (controller != null) unawaited(controller.dispose());
    unawaited(_changes.close());
  }
}
