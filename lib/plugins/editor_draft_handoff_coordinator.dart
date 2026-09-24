import 'package:flutter/foundation.dart';

import 'editor_draft.dart';
import 'editor_draft_codec.dart';

enum EditorDraftHandoffAction { complete, retire, cancel }

/// Holds recovery evidence for one workspace. Reads never commit a child or
/// retire a parent; each write is one explicit user action.
final class EditorDraftHandoffCoordinator extends ChangeNotifier {
  factory EditorDraftHandoffCoordinator(
    EditorDraftHandoffProposalControl control, {
    required bool Function() isCurrent,
    required bool Function() canWrite,
    Future<void> Function(
      EditorDraftHandoffProposalRecord,
      EditorDraftHandoffAction,
    )?
    beforeAction,
  }) => EditorDraftHandoffCoordinator._(
    control,
    isCurrent,
    canWrite,
    beforeAction,
  );

  EditorDraftHandoffCoordinator._(
    this._control,
    this._isCurrent,
    this._canWrite,
    this._beforeAction,
  );

  final EditorDraftHandoffProposalControl _control;
  final bool Function() _isCurrent;
  final bool Function() _canWrite;
  final Future<void> Function(
    EditorDraftHandoffProposalRecord,
    EditorDraftHandoffAction,
  )?
  _beforeAction;

  List<EditorDraftHandoffProposalSummary> _summaries = const [];
  EditorDraftHandoffProposalRecord? _selected;
  EditorDraftHandoffProposalRecord? _lastConfirmed;
  Object? _lastError;
  (String, String, String)? _uncertainIdentity;
  List<int>? _uncertainProposal;
  bool _uncertain = false;
  bool _busy = false;
  bool _disposed = false;
  bool _detached = false;

  List<EditorDraftHandoffProposalSummary> get summaries => _summaries;
  EditorDraftHandoffProposalRecord? get selected => _selected;
  EditorDraftHandoffProposalRecord? get lastConfirmed => _lastConfirmed;
  Object? get lastError => _lastError;
  bool get uncertain => _uncertain;
  bool get busy => _busy;

  @override
  void dispose() {
    _disposed = true;
    super.dispose();
  }

  Future<List<EditorDraftHandoffProposalSummary>> refresh() =>
      _perform(() async {
        final results = await _control.discover();
        _requireCurrent();
        _summaries = List.unmodifiable(results);
        _publish();
        return _summaries;
      });

  Future<EditorDraftHandoffProposalRecord?> inspect(
    EditorDraftHandoffProposalSummary summary,
  ) => _perform(() async {
    _identity(summary);
    final record = await _control.inspect(
      cardId: summary.cardId,
      parentDraftId: summary.parentDraftId,
      childOperation: summary.childOperation,
    );
    _requireCurrent();
    if (record != null) {
      _requireRecord(record);
      if (!_sameIdentityAndIntent(summary, record.summary)) {
        throw StateError('Original draft handoff proposal identity changed');
      }
      if (_uncertainIdentity == _key(summary)) {
        if (!listEquals(
          _uncertainProposal,
          EditorDraftCodec.encodeHandoffProposal(record.proposal),
        )) {
          throw StateError('Original uncertain draft handoff proposal changed');
        }
        _uncertain = false;
        _uncertainIdentity = null;
        _uncertainProposal = null;
      }
    }
    _selected = record;
    if (record != null) _replaceSummary(record.summary);
    _publish();
    return record;
  });

  Future<EditorDraftHandoffProposalRecord> act(
    EditorDraftHandoffProposalRecord observed,
    EditorDraftHandoffAction action,
  ) => _perform(() async {
    _requireWritable();
    _requireRecord(observed);
    final summary = observed.summary;
    if (_uncertain) {
      throw StateError(
        'Inspect the original draft handoff operation before another action',
      );
    }
    final beforeAction = _beforeAction;
    if (beforeAction != null) {
      await beforeAction(observed, action);
      _requireWritable();
    }
    final fresh = await _control.inspect(
      cardId: summary.cardId,
      parentDraftId: summary.parentDraftId,
      childOperation: summary.childOperation,
    );
    _requireWritable();
    if (fresh == null) {
      throw StateError('Original draft handoff proposal disappeared');
    }
    _requireRecord(fresh);
    if (!_sameSummary(summary, fresh.summary) ||
        !listEquals(
          EditorDraftCodec.encodeHandoffProposal(observed.proposal),
          EditorDraftCodec.encodeHandoffProposal(fresh.proposal),
        )) {
      throw StateError('Original draft handoff proposal changed');
    }
    if (!_allowed(action, fresh.summary.status)) {
      throw StateError('Draft handoff action is not available in this state');
    }
    _requireWritable();
    var submitted = false;
    var confirmed = false;
    try {
      submitted = true;
      final result = switch (action) {
        EditorDraftHandoffAction.complete => await _control.complete(
          cardId: summary.cardId,
          parentDraftId: summary.parentDraftId,
          childOperation: summary.childOperation,
        ),
        EditorDraftHandoffAction.retire => await _control.retire(
          cardId: summary.cardId,
          parentDraftId: summary.parentDraftId,
          childOperation: summary.childOperation,
        ),
        EditorDraftHandoffAction.cancel => await _control.cancel(
          cardId: summary.cardId,
          parentDraftId: summary.parentDraftId,
          childOperation: summary.childOperation,
        ),
      };
      _requireRecord(result);
      if (!_sameIdentityAndIntent(summary, result.summary) ||
          !listEquals(
            EditorDraftCodec.encodeHandoffProposal(observed.proposal),
            EditorDraftCodec.encodeHandoffProposal(result.proposal),
          )) {
        throw StateError('Draft handoff action returned a changed proposal');
      }
      if (!_resultAllowed(action, result.summary.status)) {
        throw StateError('Draft handoff action returned an invalid state');
      }
      // A valid receipt remains available even if the workspace switches
      // before this coordinator can publish it.
      _lastConfirmed = result;
      confirmed = true;
      _requireCurrent();
      _selected = result;
      _replaceSummary(result.summary);
      _publish();
      return result;
    } catch (error) {
      if (submitted &&
          !confirmed &&
          (error is! EditorDraftHandoffProposalFailure ||
              error.outcomeUnknown)) {
        _uncertain = true;
        _uncertainIdentity = _key(summary);
        _uncertainProposal = EditorDraftCodec.encodeHandoffProposal(
          observed.proposal,
        );
      }
      rethrow;
    }
  });

  bool _allowed(
    EditorDraftHandoffAction action,
    EditorDraftHandoffProposalStatus status,
  ) => switch (action) {
    EditorDraftHandoffAction.complete =>
      status == EditorDraftHandoffProposalStatus.pending,
    EditorDraftHandoffAction.retire =>
      status == EditorDraftHandoffProposalStatus.childCommitted,
    EditorDraftHandoffAction.cancel =>
      status == EditorDraftHandoffProposalStatus.pending ||
          status == EditorDraftHandoffProposalStatus.conflict,
  };

  bool _resultAllowed(
    EditorDraftHandoffAction action,
    EditorDraftHandoffProposalStatus status,
  ) => switch (action) {
    EditorDraftHandoffAction.complete =>
      status == EditorDraftHandoffProposalStatus.childCommitted ||
          status == EditorDraftHandoffProposalStatus.parentRetired,
    EditorDraftHandoffAction.retire =>
      status == EditorDraftHandoffProposalStatus.parentRetired,
    EditorDraftHandoffAction.cancel =>
      status == EditorDraftHandoffProposalStatus.cancelled,
  };

  void _identity(EditorDraftHandoffProposalSummary summary) {
    EditorDraftCodec.validateHandoffProposalIdentity(
      summary.cardId,
      summary.parentDraftId,
      summary.childOperation,
    );
  }

  void _requireRecord(EditorDraftHandoffProposalRecord record) {
    final summary = record.summary;
    _identity(summary);
    final proposal = record.proposal;
    EditorDraftCodec.encodeHandoffProposal(proposal);
    if (summary.cardId != proposal.handoff.request.cardId ||
        summary.parentDraftId != proposal.handoff.parentLink.parentDraftId ||
        summary.childDraftId != proposal.handoff.request.draftId ||
        summary.childOperation != proposal.handoff.request.operation ||
        summary.retirementOperation != proposal.retirementOperation) {
      throw StateError('Draft handoff record identity changed');
    }
  }

  (String, String, String) _key(EditorDraftHandoffProposalSummary summary) =>
      (summary.cardId, summary.parentDraftId, summary.childOperation);

  bool _sameIdentityAndIntent(
    EditorDraftHandoffProposalSummary a,
    EditorDraftHandoffProposalSummary b,
  ) =>
      a.cardId == b.cardId &&
      a.parentDraftId == b.parentDraftId &&
      a.childDraftId == b.childDraftId &&
      a.childOperation == b.childOperation &&
      a.retirementOperation == b.retirementOperation &&
      a.cursor == b.cursor;

  bool _sameSummary(
    EditorDraftHandoffProposalSummary a,
    EditorDraftHandoffProposalSummary b,
  ) =>
      _sameIdentityAndIntent(a, b) &&
      a.revision == b.revision &&
      a.status == b.status &&
      a.parentGeneration == b.parentGeneration &&
      a.parentActive == b.parentActive &&
      a.childGeneration == b.childGeneration &&
      a.childActive == b.childActive;

  void _replaceSummary(EditorDraftHandoffProposalSummary updated) {
    if (_summaries.isEmpty) return;
    var changed = false;
    final next = <EditorDraftHandoffProposalSummary>[
      for (final item in _summaries)
        if (_key(item) == _key(updated)) updated else item,
    ];
    for (var i = 0; i < _summaries.length; i++) {
      if (!identical(next[i], _summaries[i])) {
        changed = true;
        break;
      }
    }
    if (changed) _summaries = List.unmodifiable(next);
  }

  Future<T> _perform<T>(Future<T> Function() work) async {
    try {
      _enter();
    } catch (error) {
      _lastError = error;
      rethrow;
    }
    try {
      // A listener notified by _enter may have switched workspaces.
      _requireCurrent();
      final result = await work();
      _requireCurrent();
      _lastError = null;
      _publish();
      return result;
    } catch (error) {
      _lastError = error;
      _publish();
      rethrow;
    } finally {
      _busy = false;
      _publish();
    }
  }

  void _enter() {
    _requireCurrent();
    if (_busy) {
      throw StateError('Draft handoff recovery is already running');
    }
    _busy = true;
    try {
      _publish();
      _requireCurrent();
    } catch (_) {
      _busy = false;
      rethrow;
    }
  }

  void _requireWritable() {
    _requireCurrent();
    if (!_canWrite()) {
      throw StateError('Draft handoff write permission changed');
    }
  }

  void _requireCurrent() {
    if (_disposed || _detached || !_isCurrent()) {
      _detached = true;
      throw StateError('Draft handoff workspace changed');
    }
  }

  void _publish() {
    if (_disposed || _detached) return;
    try {
      if (!_isCurrent()) {
        _detached = true;
        return;
      }
    } catch (_) {
      // Notification is secondary to the action's original result or error.
      _detached = true;
      return;
    }
    notifyListeners();
  }
}
