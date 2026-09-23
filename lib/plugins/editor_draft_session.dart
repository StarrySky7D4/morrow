import 'dart:async';

import 'package:flutter/foundation.dart';

import 'editor_draft.dart';

/// One complete local editor value. A journal acknowledgement never replaces
/// this value: it only changes which earlier snapshot is known durable.
final class EditorDraftSnapshot {
  EditorDraftSnapshot({
    required this.values,
    required List<EditorDraftAssetSelection> assets,
  }) : assets = List.unmodifiable(assets);

  final EditorDraftValues values;
  final List<EditorDraftAssetSelection> assets;
}

bool _sameText(EditorDraftTextValue a, EditorDraftTextValue b) =>
    a.text == b.text &&
    a.selectionBase == b.selectionBase &&
    a.selectionExtent == b.selectionExtent &&
    a.affinity == b.affinity &&
    a.directional == b.directional &&
    a.composingStart == b.composingStart &&
    a.composingEnd == b.composingEnd;

bool _sameSelection(EditorDraftAssetSelection a, EditorDraftAssetSelection b) {
  if (a.origin != b.origin ||
      a.assetId != b.assetId ||
      a.aliases.length != b.aliases.length) {
    return false;
  }
  for (var i = 0; i < a.aliases.length; i++) {
    if (a.aliases[i] != b.aliases[i]) return false;
  }
  return true;
}

bool _sameSnapshot(EditorDraftSnapshot a, EditorDraftSnapshot b) {
  final x = a.values;
  final y = b.values;
  if (!_sameText(x.title, y.title) ||
      !_sameText(x.description, y.description) ||
      !_sameText(x.hypothesis, y.hypothesis) ||
      !_sameText(x.conclusion, y.conclusion) ||
      !_sameText(x.todos, y.todos) ||
      x.category != y.category ||
      x.stage != y.stage ||
      a.assets.length != b.assets.length) {
    return false;
  }
  for (var i = 0; i < a.assets.length; i++) {
    if (!_sameSelection(a.assets[i], b.assets[i])) return false;
  }
  return true;
}

bool _sameBytes(List<int> a, List<int> b) {
  if (a.length != b.length) return false;
  for (var i = 0; i < a.length; i++) {
    if (a[i] != b[i]) return false;
  }
  return true;
}

EditorDraftSnapshot _requestSnapshot(EditorDraftWriteRequest request) =>
    EditorDraftSnapshot(values: request.values, assets: request.assets);

bool _sameRequest(EditorDraftWriteRequest a, EditorDraftWriteRequest b) =>
    a.cardId == b.cardId &&
    a.draftId == b.draftId &&
    a.operation == b.operation &&
    a.expectedGeneration == b.expectedGeneration &&
    a.sourceRevision == b.sourceRevision &&
    a.sourceKind == b.sourceKind &&
    a.predecessorOperation == b.predecessorOperation &&
    _sameBytes(a.predecessorDigest, b.predecessorDigest) &&
    _sameSnapshot(_requestSnapshot(a), _requestSnapshot(b));

/// Coordinates durable draft generations. No method here submits a business
/// edit, revives a capture scope, or grants access to a plugin.
final class EditorDraftSession extends ChangeNotifier {
  EditorDraftSession.newSession({
    required this._control,
    required this.cardId,
    required this.draftId,
    required this.sourceRevision,
    this.sourceKind = EditorDraftSourceKind.existingCard,
    required EditorDraftSnapshot initialSnapshot,
    required this._operationFactory,
    required this._isCurrent,
    this._predecessorOperation = '',
    List<int> predecessorDigest = const [],
    this._debounce = const Duration(milliseconds: 500),
  }) : _predecessorDigest = List.unmodifiable(predecessorDigest),
       _current = initialSnapshot,
       _confirmedSnapshot = initialSnapshot,
       _journalGeneration = BigInt.zero {
    _checkConstruction();
  }

  /// Restores only host-verified draft data. It performs no write and supplies
  /// no fresh permission, capture ticket, or attachment source path.
  EditorDraftSession.restore({
    required this._control,
    required EditorDraftRecord record,
    required this._operationFactory,
    required this._isCurrent,
    this._debounce = const Duration(milliseconds: 500),
  }) : cardId = record.request.cardId,
       draftId = record.request.draftId,
       sourceRevision = record.request.sourceRevision,
       sourceKind = record.request.sourceKind,
       _predecessorOperation = record.request.predecessorOperation,
       _predecessorDigest = List.unmodifiable(record.request.predecessorDigest),
       _current = _requestSnapshot(record.request),
       _confirmedSnapshot = _requestSnapshot(record.request),
       _journalGeneration = record.generation {
    _checkConstruction();
    if (sourceKind == EditorDraftSourceKind.newCard &&
        (record.sourceFormat != 0 ||
            record.sourceRevision != BigInt.zero ||
            record.sourceSha256.isNotEmpty ||
            record.predecessorRevision != BigInt.zero ||
            record.predecessorSha256.isNotEmpty)) {
      throw const FormatException('Invalid new-card draft source record');
    }
    _confirmed = record;
    _conflicted =
        !record.active ||
        !record.currentActive ||
        record.currentGeneration != record.generation;
  }

  final EditorDraftControl _control;
  final String Function() _operationFactory;
  final bool Function() _isCurrent;
  final Duration? _debounce;
  final String _predecessorOperation;
  final List<int> _predecessorDigest;
  final String cardId, draftId;
  final BigInt sourceRevision;
  final EditorDraftSourceKind sourceKind;

  EditorDraftSnapshot _current;
  EditorDraftSnapshot _confirmedSnapshot;
  BigInt _journalGeneration;
  EditorDraftRecord? _confirmed;
  EditorDraftWriteRequest? _pending;
  EditorDraftSnapshot? _frozenSnapshot;
  int? _frozenLocalGeneration;
  EditorDraftSaveFailure? _lastFailure;
  Future<EditorDraftRecord>? _inFlight;
  Timer? _timer;
  int _localGeneration = 0;
  bool _unknown = false;
  bool _conflicted = false;
  bool _disposed = false;
  bool _autoSavePaused = false;

  EditorDraftSnapshot get current => _current;
  int get localGeneration => _localGeneration;
  EditorDraftRecord? get confirmed => _confirmed;
  EditorDraftWriteRequest? get pendingRequest => _pending;
  EditorDraftSaveFailure? get lastFailure => _lastFailure;
  bool get dirty =>
      (sourceKind == EditorDraftSourceKind.newCard && _confirmed == null) ||
      !_sameSnapshot(_current, _confirmedSnapshot);
  bool get saving => _inFlight != null;
  bool get unknown => _unknown;
  bool get conflicted => _conflicted;
  bool get disposed => _disposed;
  bool get autoSavePaused => _autoSavePaused;
  BigInt get journalGeneration => _journalGeneration;

  void _checkConstruction() {
    if (cardId.isEmpty ||
        draftId.isEmpty ||
        (sourceKind == EditorDraftSourceKind.existingCard
            ? sourceRevision <= BigInt.zero
            : sourceRevision != BigInt.zero ||
                  _predecessorOperation.isNotEmpty ||
                  _predecessorDigest.isNotEmpty) ||
        _debounce?.isNegative == true ||
        (_predecessorOperation.isEmpty != _predecessorDigest.isEmpty) ||
        (_predecessorDigest.isNotEmpty && _predecessorDigest.length != 32)) {
      throw const FormatException('Invalid editor draft session identity');
    }
  }

  bool get _fresh {
    try {
      return _isCurrent();
    } catch (_) {
      return false;
    }
  }

  void _notify() {
    if (!_disposed && _fresh) notifyListeners();
  }

  void _cancelTimer() {
    _timer?.cancel();
    _timer = null;
  }

  void _scheduleAuto() {
    _cancelTimer();
    if (_debounce == null ||
        _autoSavePaused ||
        _disposed ||
        !_fresh ||
        _conflicted ||
        _unknown ||
        saving ||
        !dirty) {
      return;
    }
    _timer = Timer(_debounce, () {
      _timer = null;
      if (_disposed ||
          _autoSavePaused ||
          !_fresh ||
          _conflicted ||
          _unknown ||
          saving ||
          !dirty) {
        return;
      }
      // A timer has no caller awaiting its error. Keep the failure in state.
      unawaited(flush().then<void>((_) {}, onError: (Object _) {}));
    });
  }

  /// Stops future debounce saves without cancelling a request already sent.
  /// The caller retains this session and may still call explicit save methods.
  void pauseAutoSave() {
    if (_disposed) throw StateError('Editor draft session is disposed');
    if (!_fresh) throw StateError('Editor draft workspace changed');
    if (_autoSavePaused) return;
    _autoSavePaused = true;
    _cancelTimer();
    _notify();
  }

  /// Restores the configured debounce policy. Unknown and conflicted saves
  /// remain blocked; this never retries a frozen operation.
  void resumeAutoSave() {
    if (_disposed) throw StateError('Editor draft session is disposed');
    if (!_fresh) throw StateError('Editor draft workspace changed');
    if (!_autoSavePaused) return;
    _autoSavePaused = false;
    _scheduleAuto();
    _notify();
  }

  /// A structurally equal observation is a no-op, including for autosave.
  void observe(EditorDraftSnapshot snapshot) {
    if (_disposed) throw StateError('Editor draft session is disposed');
    if (!_fresh) throw StateError('Editor draft workspace changed');
    if (_sameSnapshot(_current, snapshot)) return;
    _current = snapshot;
    _localGeneration++;
    if (!_unknown && !saving && _lastFailure?.outcomeUnknown == false) {
      _lastFailure = null;
    }
    _scheduleAuto();
    _notify();
  }

  EditorDraftWriteRequest _freeze() {
    final request = EditorDraftWriteRequest(
      cardId: cardId,
      draftId: draftId,
      operation: _operationFactory(),
      expectedGeneration: _journalGeneration,
      sourceRevision: sourceRevision,
      sourceKind: sourceKind,
      predecessorOperation: _predecessorOperation,
      predecessorDigest: _predecessorDigest,
      values: _current.values,
      assets: _current.assets,
    );
    _pending = request;
    _frozenSnapshot = _current;
    _frozenLocalGeneration = _localGeneration;
    return request;
  }

  Future<EditorDraftRecord> _send(
    EditorDraftWriteRequest request, {
    required bool retry,
  }) async {
    final wasUnknown = _unknown;
    final frozenLocalGeneration = _frozenLocalGeneration;
    var scheduleNext = false;
    try {
      final record = await Future.sync(() => _control.save(request));
      if (_disposed || !_fresh) {
        throw StateError('Editor draft workspace changed during save');
      }
      if (!_sameRequest(record.request, request) ||
          !record.active ||
          record.generation != request.expectedGeneration + BigInt.one ||
          record.currentGeneration < record.generation) {
        throw StateError('Editor draft receipt differs from original request');
      }
      if (!record.currentActive ||
          record.currentGeneration != record.generation) {
        _conflicted = true;
        throw StateError('Editor draft journal advanced during save');
      }
      _confirmed = record;
      _confirmedSnapshot = _frozenSnapshot!;
      _journalGeneration = record.generation;
      _pending = null;
      _frozenSnapshot = null;
      _frozenLocalGeneration = null;
      _lastFailure = null;
      _unknown = false;
      _notify();
      // Only the first acknowledged save may schedule S2. Reconciliation of
      // an unknown S1 stops after its exact original receipt.
      scheduleNext = !retry;
      return record;
    } catch (error) {
      final failure =
          error is EditorDraftSaveFailure &&
              identical(error.request, request) &&
              (!wasUnknown || error.outcomeUnknown)
          ? error
          : EditorDraftSaveFailure(
              request: request,
              outcomeUnknown:
                  wasUnknown ||
                  error is! EditorDraftSaveFailure ||
                  error.outcomeUnknown ||
                  !identical(error.request, request),
              cause: error,
            );
      _lastFailure = failure;
      _unknown = failure.outcomeUnknown;
      if (!_unknown) {
        _pending = null;
        _frozenSnapshot = null;
        _frozenLocalGeneration = null;
      }
      _notify();
      throw failure;
    } finally {
      _inFlight = null;
      // A rejected S1 must not retry itself. A different local snapshot
      // observed while S1 was in flight may start a new debounce instead.
      // Check after notifying listeners, which may synchronously observe S2.
      if (!scheduleNext &&
          !retry &&
          !_unknown &&
          _lastFailure?.outcomeUnknown == false &&
          frozenLocalGeneration != null &&
          _localGeneration != frozenLocalGeneration &&
          dirty) {
        scheduleNext = true;
      }
      if (scheduleNext && !_conflicted && !_disposed) _scheduleAuto();
      _notify();
    }
  }

  /// After an exact confirmed save, selected assets are pinned by that draft
  /// generation. This changes only the local origin for a future generation;
  /// it does not rewrite the acknowledged request or submit anything.
  /// Callers must update their attachment catalog before capturing the view.
  void adoptConfirmedAssetPins(EditorDraftRecord record) {
    if (_disposed) throw StateError('Editor draft session is disposed');
    if (!_fresh) throw StateError('Editor draft workspace changed');
    if (!identical(_confirmed, record) ||
        _pending != null ||
        _inFlight != null ||
        _unknown ||
        _conflicted ||
        !record.active ||
        !record.currentActive ||
        record.currentGeneration != record.generation) {
      throw StateError('Editor draft pins are not confirmed');
    }
    final selected = record.request.assets;
    if (record.assets.length != selected.length) {
      throw const FormatException('Editor draft pin list changed');
    }
    for (var i = 0; i < selected.length; i++) {
      if (!_sameSelection(record.assets[i].selection, selected[i])) {
        throw const FormatException('Editor draft pin identity changed');
      }
    }

    EditorDraftSnapshot adopt(EditorDraftSnapshot snapshot) {
      final assets = <EditorDraftAssetSelection>[];
      for (final asset in snapshot.assets) {
        final pinned = selected.any(
          (original) =>
              original.origin == asset.origin &&
              original.assetId == asset.assetId,
        );
        assets.add(
          pinned && asset.origin != EditorDraftAssetOrigin.previousDraft
              ? EditorDraftAssetSelection(
                  origin: EditorDraftAssetOrigin.previousDraft,
                  assetId: asset.assetId,
                  aliases: asset.aliases,
                )
              : asset,
        );
      }
      return EditorDraftSnapshot(values: snapshot.values, assets: assets);
    }

    // Compute both replacements before changing either field. Repeating this
    // call is a no-op because the original origins are no longer present.
    final confirmed = adopt(_confirmedSnapshot);
    final current = adopt(_current);
    _confirmedSnapshot = confirmed;
    _current = current;
  }

  Future<EditorDraftRecord> _startSave() {
    final request = _freeze();
    _inFlight = _send(request, retry: false);
    _notify();
    return _inFlight!;
  }

  /// Explicitly creates the first durable journal from the complete current
  /// snapshot, even when an existing card has not been edited. Once a journal
  /// exists this returns its confirmed generation; newer local input remains
  /// dirty and can be saved separately. It never retries an unknown request.
  Future<EditorDraftRecord> ensureJournal() async {
    if (_disposed) throw StateError('Editor draft session is disposed');
    if (!_fresh) throw StateError('Editor draft workspace changed');
    if (_inFlight case final flight?) return flight;
    if (_conflicted) throw StateError('Editor draft journal changed');
    if (_unknown) {
      throw _lastFailure ?? StateError('Editor draft outcome is unknown');
    }
    if (_confirmed case final record?) return record;
    _cancelTimer();
    return _startSave();
  }

  /// Quiesces debounce and attempts to make the latest visible snapshot
  /// durable. It waits for at most one existing save, then sends at most one
  /// newer generation. A timeout does not cancel a sent request or prove
  /// rollback. If input changes during that final save, callers must retry
  /// after stabilizing their editor; no extra generation is sent here.
  Future<EditorDraftRecord?> flushLatestVisible({
    required Duration timeout,
  }) async {
    if (timeout <= Duration.zero) {
      throw ArgumentError.value(timeout, 'timeout', 'Must be positive');
    }
    pauseAutoSave();
    final clock = Stopwatch()..start();
    Duration remaining() {
      final left = timeout - clock.elapsed;
      if (left <= Duration.zero) {
        throw TimeoutException('Editor draft handoff wait timed out', timeout);
      }
      return left;
    }

    if (_inFlight case final flight?) {
      await flight.timeout(remaining());
    }
    if (_disposed) throw StateError('Editor draft session is disposed');
    if (!_fresh) throw StateError('Editor draft workspace changed');
    if (_conflicted) throw StateError('Editor draft journal changed');
    if (_unknown) {
      throw _lastFailure ?? StateError('Editor draft outcome is unknown');
    }
    final visibleGeneration = _localGeneration;
    if (dirty) {
      final budget = remaining();
      await flush().timeout(budget);
    }
    if (_disposed) throw StateError('Editor draft session is disposed');
    if (!_fresh) throw StateError('Editor draft workspace changed');
    if (_localGeneration != visibleGeneration || dirty) {
      throw StateError('Editor draft changed during handoff save');
    }
    return _confirmed;
  }

  /// Save the current snapshot once. If another generation is already in
  /// flight, this awaits that same operation and does not submit newer edits.
  Future<EditorDraftRecord?> flush() async {
    if (_disposed) throw StateError('Editor draft session is disposed');
    if (!_fresh) throw StateError('Editor draft workspace changed');
    _cancelTimer();
    if (_inFlight case final flight?) return flight;
    if (_conflicted) throw StateError('Editor draft journal changed');
    if (_unknown) {
      throw _lastFailure ?? StateError('Editor draft outcome is unknown');
    }
    if (!dirty) return _confirmed;
    return _startSave();
  }

  /// Resend only the frozen operation after an unknown outcome. Newer local
  /// input remains in [current] and requires a later explicit save/observe.
  Future<EditorDraftRecord> retryPending() async {
    if (_disposed) throw StateError('Editor draft session is disposed');
    if (!_fresh) throw StateError('Editor draft workspace changed');
    _cancelTimer();
    if (_inFlight case final flight?) return flight;
    if (_conflicted) throw StateError('Editor draft journal changed');
    final request = _pending;
    if (!_unknown || request == null) {
      throw StateError('No unknown editor draft operation to retry');
    }
    _inFlight = _send(request, retry: true);
    _notify();
    return _inFlight!;
  }

  /// View owners remove their own listeners. Detaching a view does not mute
  /// other observers or stop durable saves owned by this session.
  void detachUI() {}

  void attachUI() {
    if (_disposed) throw StateError('Editor draft session is disposed');
    _notify();
  }

  @override
  void dispose() {
    if (_disposed) return;
    _disposed = true;
    _cancelTimer();
    super.dispose();
  }
}
