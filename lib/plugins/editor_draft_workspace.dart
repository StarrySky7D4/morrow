import 'editor_draft.dart';
import 'editor_draft_session.dart';

/// A view lease owns only attachment to the view. Releasing it never abandons
/// a draft, cancels an operation or disposes the workspace's only live S2.
final class EditorDraftViewLease {
  EditorDraftViewLease._(this._workspace, this.session, this._capture);
  final EditorDraftWorkspace _workspace;
  final EditorDraftSession session;
  final bool Function()? _capture;
  bool _released = false;
  bool get released => _released;
  void release() {
    if (_released) return;
    _released = true;
    _workspace._release(this);
  }
}

/// A point-in-time local close preparation, not a new persistence authority.
/// Later input invalidates it; it must never be used after a workspace switch.
final class EditorDraftClosePreparation {
  EditorDraftClosePreparation._(
    this._workspace,
    this._epoch,
    this._generations,
    this.records,
  );
  final EditorDraftWorkspace _workspace;
  final int _epoch;
  final Map<EditorDraftSession, int> _generations;
  final List<EditorDraftRecord> records;
}

/// Keeps sessions independent of replaceable editor widgets. Host journals
/// remain authoritative across process restarts; this owner protects live S2
/// and unknown operations while views detach within the current workspace.
final class EditorDraftWorkspace {
  EditorDraftWorkspace({required this.isCurrent, this.capacity = 16}) {
    if (capacity < 1 || capacity > 16) {
      throw ArgumentError.value(capacity, 'capacity');
    }
  }
  final bool Function() isCurrent;
  final int capacity;
  final _sessions = <(String, String), EditorDraftSession>{};
  final _leases = <EditorDraftViewLease>{};
  final _pausedBefore = <EditorDraftSession, bool>{};
  bool _closing = false;
  bool _attaching = false;
  bool _disposed = false;
  int _epoch = 0;

  bool get preparingClose => _closing;
  bool get disposed => _disposed;
  int get attachedViews => _leases.length;
  List<EditorDraftSession> get sessions => List.unmodifiable(_sessions.values);

  bool hasView(EditorDraftSession session) =>
      _leases.any((lease) => identical(lease.session, session));

  /// Releases only a transferred, host-retired parent cache. The child remains
  /// retained even when it receives further input during S1 acknowledgement.
  void releaseRetiredParent(
    EditorDraftSession parent,
    EditorDraftSession child,
    EditorDraftHandoffProposalRecord retired, {
    required int expectedLocalGeneration,
  }) {
    _check();
    final summary = retired.summary;
    final link = retired.proposal.handoff.parentLink;
    if (_closing ||
        hasView(parent) ||
        parent.saving ||
        parent.unknown ||
        parent.captureBlocked ||
        parent.localGeneration != expectedLocalGeneration ||
        !identical(find(parent.cardId, parent.draftId), parent) ||
        !identical(find(child.cardId, child.draftId), child) ||
        child.disposed ||
        child.unknown ||
        child.conflicted ||
        child.confirmed == null ||
        summary.status != EditorDraftHandoffProposalStatus.parentRetired ||
        summary.parentActive ||
        !summary.childActive ||
        summary.cardId != parent.cardId ||
        child.cardId != parent.cardId ||
        summary.parentDraftId != parent.draftId ||
        summary.childDraftId != child.draftId ||
        link.parentGeneration != parent.confirmed?.generation ||
        link.parentSaveOperation != parent.confirmed?.request.operation ||
        child.confirmed!.parentLink?.childOperation != link.childOperation) {
      throw StateError('Original parent retirement cannot release this cache');
    }
    _sessions.remove((parent.cardId, parent.draftId));
    parent.dispose();
  }

  /// Release exactly the original local reservation after host retirement.
  /// This never evicts an active, dirty, attached or unknown draft to make room.
  void releaseRetiredDraft(
    EditorDraftSession session,
    EditorDraftRetirementReservation reservation,
    EditorDraftRecord retired,
  ) {
    _check();
    final key = (session.cardId, session.draftId);
    if (_closing || _attaching || hasView(session) ||
        !identical(_sessions[key], session) ||
        !session.ownsRetirement(reservation)) {
      throw StateError('Original draft cannot leave this workspace');
    }
    session.completeRetirement(reservation, retired);
    _sessions.remove(key);
  }

  void _check() {
    if (_disposed || !isCurrent()) {
      throw StateError('Draft workspace is unavailable');
    }
  }

  EditorDraftSession? find(String cardId, String draftId) {
    _check();
    return _sessions[(cardId, draftId)];
  }

  /// Retain a workbench-owned session before its first view is constructed.
  /// A failed or cancelled route must not orphan an already admitted owner.
  void retain(EditorDraftSession session) {
    _check();
    if (_closing || _attaching || session.disposed || session.retirementFrozen) {
      throw StateError('Draft workspace cannot retain a session');
    }
    final key = (session.cardId, session.draftId);
    final existing = _sessions[key];
    if (existing != null && !identical(existing, session)) {
      throw StateError('A live draft already owns this scope');
    }
    if (existing == null && _sessions.length >= capacity) {
      throw StateError('Draft workspace session capacity reached');
    }
    _sessions[key] = session;
  }

  /// Adopts exactly one session per scope. A new widget must reuse this session
  /// rather than replace it with a stale record read while a save was pending.
  EditorDraftViewLease attach(
    EditorDraftSession session, {
    bool Function()? capture,
  }) {
    _check();
    if (_closing || _attaching || session.disposed || session.retirementFrozen) {
      throw StateError('Draft workspace cannot attach');
    }
    final key = (session.cardId, session.draftId);
    final existing = _sessions[key];
    if (existing != null && !identical(existing, session)) {
      throw StateError('A live draft already owns this scope');
    }
    if (_leases.any((lease) => identical(lease.session, session))) {
      throw StateError('A draft already has an active view');
    }
    if (existing == null && _sessions.length >= capacity) {
      throw StateError('Draft workspace session capacity reached');
    }
    final lease = EditorDraftViewLease._(this, session, capture);
    _sessions[key] = session;
    _leases.add(lease);
    _attaching = true;
    try {
      // Publish the scope before notifying synchronous session listeners.
      session.attachUI();
      _check();
      if (_closing || session.disposed) {
        throw StateError('Draft workspace changed during attach');
      }
      return lease;
    } catch (_) {
      lease._released = true;
      _leases.remove(lease);
      if (existing == null) _sessions.remove(key);
      rethrow;
    } finally {
      _attaching = false;
    }
  }

  void _release(EditorDraftViewLease lease) {
    if (!_leases.remove(lease)) return;
    lease.session.detachUI();
  }

  /// Capture the real attached views before close, including metadata that
  /// does not notify a text controller. A failed or throwing capture keeps
  /// its session dirty; an older successful receipt cannot authorize EOF.
  void captureViews() {
    _check();
    for (final lease in List<EditorDraftViewLease>.of(_leases)) {
      if (lease.released || lease._capture == null) continue;
      try {
        if (!lease._capture()) {
          throw StateError('Draft view could not capture its complete state');
        }
      } catch (error) {
        if (!lease.session.captureBlocked) {
          lease.session.markCaptureIncomplete(error);
        }
        rethrow;
      }
      _check();
    }
  }

  /// Eviction is explicit and only releases a reconstructible view/session
  /// cache. It never discards the host journal or an unconfirmed local proposal.
  bool evictDurable(String cardId, String draftId) {
    _check();
    if (_closing || _attaching) return false;
    final key = (cardId, draftId);
    final session = _sessions[key];
    if (session == null) return true;
    final confirmed = session.confirmed;
    if (session.retirementFrozen ||
        _leases.any((lease) => identical(lease.session, session)) ||
        session.saving ||
        session.dirty ||
        session.unknown ||
        session.conflicted ||
        confirmed == null ||
        !confirmed.active ||
        !confirmed.currentActive ||
        confirmed.currentGeneration != confirmed.generation) {
      return false;
    }
    _sessions.remove(key);
    session.dispose();
    return true;
  }

  /// Call after each visible binding has captured all fields and resolved asset
  /// metadata. This method never closes views or retries an unknown operation.
  Future<EditorDraftClosePreparation> prepareClose({
    required Duration timeout,
  }) async {
    _check();
    if (_closing || _attaching) {
      throw StateError('Draft close is already being prepared');
    }
    if (timeout <= Duration.zero) throw ArgumentError.value(timeout, 'timeout');
    _closing = true;
    final epoch = ++_epoch;
    final timer = Stopwatch()..start();
    final records = <EditorDraftRecord>[];
    final generations = <EditorDraftSession, int>{};
    final sessions = List<EditorDraftSession>.of(_sessions.values);
    try {
      for (final session in sessions) {
        _check();
        _pausedBefore[session] = session.autoSavePaused;
        session.pauseAutoSave();
      }
      captureViews();
      for (final session in sessions) {
        _check();
        final remaining = timeout - timer.elapsed;
        if (remaining <= Duration.zero) {
          throw StateError('Draft close deadline elapsed');
        }
        final record = await session.flushLatestVisible(timeout: remaining);
        _check();
        if (_epoch != epoch || !_closing) {
          throw StateError('Draft close preparation changed');
        }
        if (record != null) records.add(record);
        generations[session] = session.localGeneration;
      }
      captureViews();
      _validate(generations);
      return EditorDraftClosePreparation._(
        this,
        epoch,
        Map.unmodifiable(generations),
        List.unmodifiable(records),
      );
    } catch (error, stack) {
      // Retain every session, including live S2 and uncertain requests. Restore
      // timers only while this is still the same workspace, and never replace
      // the original failure with a restoration failure.
      try {
        _restoreAutoPolicy(preserveFailure: true);
      } finally {
        _closing = false;
      }
      Error.throwWithStackTrace(error, stack);
    } finally {
      timer.stop();
    }
  }

  void _validate(Map<EditorDraftSession, int> generations) {
    _check();
    if (generations.length != _sessions.length) {
      throw StateError('Draft set changed during close');
    }
    for (final session in _sessions.values) {
      if (session.disposed ||
          session.dirty ||
          session.saving ||
          session.unknown ||
          session.conflicted ||
          generations[session] != session.localGeneration) {
        throw StateError('Latest draft is not confirmed for close');
      }
    }
  }

  /// Resume editing after a prepared close is cancelled. No journal mutation.
  void cancelClose(EditorDraftClosePreparation preparation) {
    _checkPreparation(preparation);
    ++_epoch;
    try {
      _restoreAutoPolicy();
    } finally {
      _closing = false;
    }
  }

  bool get _stillCurrent {
    if (_disposed) return false;
    try {
      return isCurrent();
    } catch (_) {
      return false;
    }
  }

  void _restoreAutoPolicy({bool preserveFailure = false}) {
    final previous = Map<EditorDraftSession, bool>.from(_pausedBefore);
    _pausedBefore.clear();
    if (!_stillCurrent) return;
    Object? firstError;
    StackTrace? firstStack;
    for (final entry in previous.entries) {
      // A preceding resume may synchronously switch the workspace.
      if (!_stillCurrent) break;
      if (entry.value || entry.key.disposed) continue;
      try {
        entry.key.resumeAutoSave();
      } catch (error, stack) {
        firstError ??= error;
        firstStack ??= stack;
      }
    }
    if (!preserveFailure && firstError != null) {
      Error.throwWithStackTrace(firstError, firstStack!);
    }
  }

  void _checkPreparation(EditorDraftClosePreparation preparation) {
    _check();
    if (!identical(preparation._workspace, this) ||
        !_closing ||
        preparation._epoch != _epoch) {
      throw StateError('Stale draft close preparation');
    }
  }

  /// Release view bindings/leases first. Recheck local generations immediately
  /// before releasing sessions, so input after prepare cannot be silently lost.
  /// If validation fails, retain the preparation and call cancelClose to resume
  /// editing, or release the remaining views and retry this exact preparation.
  void finishClose(EditorDraftClosePreparation preparation) {
    _checkPreparation(preparation);
    _validate(preparation._generations);
    if (_leases.isNotEmpty) throw StateError('Draft views are still attached');
    for (final session in _sessions.values) {
      session.dispose();
    }
    _sessions.clear();
    _pausedBefore.clear();
    _closing = false;
    _disposed = true;
    ++_epoch;
  }
}
