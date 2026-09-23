import 'dart:async';

import 'editor_draft_import.dart';
import 'editor_draft_import_codec.dart';

enum _ImportAction { begin, complete }

/// Retains one immutable import operation across lost replies. A caller can
/// reconstruct it after restart from host-inspected, already journaled intent.
/// All writes are explicit. Inspection is read-only and never resolves an
/// uncertain Pending or absent result by inventing a replacement operation.
final class EditorDraftImportSession {
  EditorDraftImportSession({
    required this.control,
    required this.request,
    required this.isCurrent,
  }) {
    EditorDraftImportCodec.validateRequest(request);
  }

  EditorDraftImportSession.restoreDecision({
    required this.control,
    required EditorDraftImportDecision decision,
    required this.isCurrent,
  }) : request = decision.request {
    EditorDraftImportCodec.validateRequest(request);
    EditorDraftImportCodec.validateAbandon(decision.intent);
    _lastDecision = decision;
    if (decision.status == EditorDraftImportDecisionStatus.pending ||
        decision.status == EditorDraftImportDecisionStatus.conflict) {
      _abandonIntent = decision.intent;
    }
  }
  final EditorDraftImportControl control;
  final EditorDraftImportRequest request;
  final bool Function() isCurrent;

  EditorDraftImportSnapshot? _confirmed;
  EditorDraftImportSnapshot? _inspection;
  EditorDraftImportFailure? _failure;
  EditorDraftImportAbandon? _abandonIntent;
  _ImportAction? _pendingAction;
  Future<EditorDraftImportSnapshot>? _inFlight;
  EditorDraftImportDecision? _lastDecision;
  bool _importUnknown = false;
  bool _decisionUnknown = false;
  bool _disposed = false;

  EditorDraftImportSnapshot? get confirmed => _confirmed;
  EditorDraftImportSnapshot? get lastInspection => _inspection;
  EditorDraftImportFailure? get lastFailure => _failure;
  EditorDraftImportAbandon? get pendingAbandon => _abandonIntent;
  EditorDraftImportDecision? get lastDecision => _lastDecision;
  bool get unknown => _importUnknown || _decisionUnknown;
  bool get busy => _inFlight != null;
  bool get disposed => _disposed;

  bool get _fresh {
    try {
      return !_disposed && isCurrent();
    } catch (_) {
      return false;
    }
  }

  void _check() {
    if (_disposed) throw StateError('Draft import session disposed');
    if (!_fresh) throw StateError('Draft import workspace changed');
  }

  Future<EditorDraftImportSnapshot> _job(
    Object intent,
    Future<EditorDraftImportSnapshot> Function() call, {
    required void Function(EditorDraftImportSnapshot) accept,
    bool resolvesUnknown = true,
    bool decisionOutcome = false,
  }) {
    _check();
    if (_inFlight != null) {
      throw StateError('Draft import operation is still in flight');
    }
    final result = Future.sync(() async {
      try {
        final snapshot = await call();
        if (!_fresh) {
          throw StateError('Draft import workspace changed during request');
        }
        accept(snapshot);
        if (resolvesUnknown) {
          if (decisionOutcome) {
            _decisionUnknown = false;
          } else {
            _importUnknown = false;
          }
          if (!unknown) _failure = null;
        }
        return snapshot;
      } catch (error, stack) {
        if (!resolvesUnknown) {
          Error.throwWithStackTrace(error, stack);
        }
        final failure =
            error is EditorDraftImportFailure && identical(error.intent, intent)
            ? error
            : EditorDraftImportFailure(
                intent: intent,
                outcomeUnknown: true,
                cause: error,
              );
        // A reply from a closed or replaced workspace cannot alter its state.
        if (_fresh && resolvesUnknown) {
          _failure = failure;
          if (decisionOutcome) {
            _decisionUnknown = _decisionUnknown || failure.outcomeUnknown;
          } else {
            _importUnknown = _importUnknown || failure.outcomeUnknown;
          }
        }
        Error.throwWithStackTrace(failure, stack);
      }
    });
    _inFlight = result;
    result.then<void>(
      (_) {
        _inFlight = null;
      },
      onError: (Object _, StackTrace _) {
        _inFlight = null;
      },
    );
    return result;
  }

  void _context(
    EditorDraftImportSnapshot snapshot,
    BigInt generation,
    String operation,
  ) {
    if (snapshot.cardId != request.cardId ||
        snapshot.draftId != request.draftId ||
        snapshot.operation != operation ||
        snapshot.importOperation != request.operation ||
        snapshot.expectedGeneration != generation ||
        (snapshot.record != null &&
            (snapshot.record!.currentGeneration != snapshot.currentGeneration ||
                snapshot.record!.mainActive != snapshot.mainActive ||
                snapshot.record!.stagingRevision !=
                    snapshot.stagingRevision))) {
      throw const FormatException('Draft import session context mismatch');
    }
  }

  Future<EditorDraftImportSnapshot> _sendImport(
    _ImportAction action, {
    String selectedPath = '',
    required bool retry,
  }) {
    _check();
    if (_abandonIntent != null || (!retry && unknown)) {
      throw StateError('Resolve the original draft import operation first');
    }
    if (_inFlight != null) {
      throw StateError('Draft import operation is still in flight');
    }
    _pendingAction = action;
    return _job(
      request,
      () => action == _ImportAction.begin
          ? control.begin(request)
          : control.complete(request, selectedPath: selectedPath),
      accept: (snapshot) {
        _context(snapshot, request.expectedGeneration, request.operation);
        if (snapshot.kind != EditorDraftImportResultKind.record ||
            snapshot.record == null ||
            !EditorDraftImportCodec.sameRequest(
              snapshot.record!.request,
              request,
            )) {
          throw const FormatException(
            'Draft import receipt changed original request',
          );
        }
        _confirmed = snapshot;
        _pendingAction = null;
      },
    );
  }

  Future<EditorDraftImportSnapshot> begin() =>
      _sendImport(_ImportAction.begin, retry: false);

  /// [selectedPath] is transport-only and is never saved in the journal. An
  /// empty path can confirm an already retained byte owner after a lost reply.
  Future<EditorDraftImportSnapshot> complete({String selectedPath = ''}) =>
      _sendImport(
        _ImportAction.complete,
        selectedPath: selectedPath,
        retry: false,
      );

  /// Retries the same request and action. The caller must explicitly select a
  /// source path again if bytes were never retained by the host.
  Future<EditorDraftImportSnapshot> retryOriginal({String selectedPath = ''}) {
    _check();
    final action = _pendingAction;
    if (action == null) throw StateError('No original draft import to retry');
    return _sendImport(action, selectedPath: selectedPath, retry: true);
  }

  Future<EditorDraftImportSnapshot> inspect() => _job(
    request,
    () => control.inspect(request.cardId, request.draftId, request.operation),
    accept: (snapshot) {
      _context(snapshot, BigInt.zero, request.operation);
      if (snapshot.kind != EditorDraftImportResultKind.record &&
          snapshot.kind != EditorDraftImportResultKind.absent) {
        throw const FormatException('Invalid draft import inspection');
      }
      if (snapshot.record != null &&
          !EditorDraftImportCodec.sameRequest(
            snapshot.record!.request,
            request,
          )) {
        throw const FormatException('Foreign draft import inspection');
      }
      _inspection = snapshot;
    },
    resolvesUnknown: false,
  );

  EditorDraftImportDecision _decision(
    EditorDraftImportSnapshot snapshot,
    EditorDraftImportAbandon intent,
  ) {
    _context(snapshot, intent.currentGeneration, intent.operation);
    final decision = snapshot.decision;
    if (snapshot.kind != EditorDraftImportResultKind.decision ||
        decision == null ||
        !EditorDraftImportCodec.sameRequest(decision.request, request) ||
        decision.operation != intent.operation ||
        decision.expectedGeneration != intent.currentGeneration ||
        decision.currentGeneration != snapshot.currentGeneration ||
        decision.mainActive != snapshot.mainActive ||
        decision.stagingRevision != snapshot.stagingRevision) {
      throw const FormatException('Draft import decision changed its intent');
    }
    return decision;
  }

  void _checkDecisionIntent(EditorDraftImportAbandon intent) {
    EditorDraftImportCodec.validateAbandon(intent);
    if (intent.cardId != request.cardId ||
        intent.draftId != request.draftId ||
        intent.importOperation != request.operation) {
      throw const FormatException('Draft import decision identity mismatch');
    }
  }

  /// Records only the decision. It never sends the abandonment commit.
  Future<EditorDraftImportSnapshot> prepareDecision(
    EditorDraftImportAbandon intent,
  ) {
    _check();
    _checkDecisionIntent(intent);
    if (_lastDecision?.operation == intent.operation &&
        (_lastDecision!.status == EditorDraftImportDecisionStatus.committed ||
            _lastDecision!.status ==
                EditorDraftImportDecisionStatus.cancelled)) {
      throw StateError('Decision operation is already final');
    }
    if (_abandonIntent != null &&
        (_abandonIntent!.operation != intent.operation ||
            _abandonIntent!.currentGeneration != intent.currentGeneration)) {
      throw StateError('Original abandon operation remains unresolved');
    }
    if (_inFlight != null) {
      throw StateError('Draft import operation is still in flight');
    }
    _abandonIntent = intent;
    return _job(
      intent,
      () => control.prepareDecision(intent),
      decisionOutcome: true,
      accept: (snapshot) {
        final decision = _decision(snapshot, intent);
        _lastDecision = decision;
        if (decision.status == EditorDraftImportDecisionStatus.committed ||
            decision.status == EditorDraftImportDecisionStatus.cancelled) {
          _abandonIntent = null;
          if (decision.status == EditorDraftImportDecisionStatus.committed) {
            _pendingAction = null;
            _importUnknown = false;
          }
        }
      },
    );
  }

  /// Read-only recovery. An absent response never clears an unknown outcome.
  Future<EditorDraftImportSnapshot> inspectDecision(
    EditorDraftImportAbandon intent,
  ) => _job(
    intent,
    () {
      _checkDecisionIntent(intent);
      return control.inspectDecision(intent);
    },
    resolvesUnknown: false,
    accept: (snapshot) {
      _context(snapshot, intent.currentGeneration, intent.operation);
      if (snapshot.kind == EditorDraftImportResultKind.absent) return;
      final decision = _decision(snapshot, intent);
      _lastDecision = decision;
      _decisionUnknown = false;
      if (decision.status == EditorDraftImportDecisionStatus.committed ||
          decision.status == EditorDraftImportDecisionStatus.cancelled) {
        _abandonIntent = null;
        if (decision.status == EditorDraftImportDecisionStatus.committed) {
          _pendingAction = null;
          _importUnknown = false;
        }
      } else {
        _abandonIntent = intent;
      }
    },
  );

  Future<EditorDraftImportDecisionPage> listDecisions({
    String cursor = '',
    int limit = 32,
  }) async {
    _check();
    if (_inFlight != null) {
      throw StateError('Draft import operation is still in flight');
    }
    final page = await control.listDecisions(
      request.cardId,
      request.draftId,
      cursor: cursor,
      limit: limit,
    );
    if (!_fresh ||
        page.cardId != request.cardId ||
        page.draftId != request.draftId ||
        page.requestCursor != cursor ||
        page.requestLimit != limit) {
      throw const FormatException('Draft import decision page changed scope');
    }
    return page;
  }

  Future<EditorDraftImportSnapshot> cancelDecision(
    EditorDraftImportAbandon intent,
  ) {
    _check();
    _checkDecisionIntent(intent);
    if (_abandonIntent?.operation != intent.operation ||
        _abandonIntent?.currentGeneration != intent.currentGeneration) {
      throw StateError('No matching pending abandon decision to cancel');
    }
    return _job(
      intent,
      () => control.cancelDecision(intent),
      decisionOutcome: true,
      accept: (snapshot) {
        final decision = _decision(snapshot, intent);
        if (decision.status != EditorDraftImportDecisionStatus.cancelled) {
          throw const FormatException('Draft import decision not cancelled');
        }
        _lastDecision = decision;
        _abandonIntent = null;
      },
    );
  }

  Future<EditorDraftImportSnapshot> abandon(EditorDraftImportAbandon intent) {
    _check();
    _checkDecisionIntent(intent);
    if (intent.cardId != request.cardId ||
        intent.draftId != request.draftId ||
        intent.importOperation != request.operation ||
        intent.operation == request.operation) {
      throw const FormatException('Draft import abandon identity mismatch');
    }
    if (_lastDecision?.operation == intent.operation &&
        (_lastDecision!.status == EditorDraftImportDecisionStatus.committed ||
            _lastDecision!.status ==
                EditorDraftImportDecisionStatus.cancelled)) {
      throw StateError('Decision operation is already final');
    }
    if (_abandonIntent != null &&
        (_abandonIntent!.operation != intent.operation ||
            _abandonIntent!.currentGeneration != intent.currentGeneration)) {
      throw StateError('Original abandon operation remains unresolved');
    }
    if (_inFlight != null) {
      throw StateError('Draft import operation is still in flight');
    }
    _abandonIntent = intent;
    return _job(
      intent,
      () => control.abandon(intent),
      accept: (snapshot) {
        _context(snapshot, intent.currentGeneration, intent.operation);
        if (snapshot.kind != EditorDraftImportResultKind.record ||
            snapshot.record?.phase != EditorDraftImportPhase.retired ||
            snapshot.record == null ||
            !EditorDraftImportCodec.sameRequest(
              snapshot.record!.request,
              request,
            )) {
          throw const FormatException('Invalid draft import abandon receipt');
        }
        _confirmed = snapshot;
        _lastDecision = null;
        _importUnknown = false;
        _abandonIntent = null;
        _pendingAction = null;
      },
      decisionOutcome: true,
    );
  }

  Future<EditorDraftImportSnapshot> retryAbandon() {
    _check();
    final intent = _abandonIntent;
    if (intent == null) throw StateError('No abandon operation to retry');
    return abandon(intent);
  }

  /// The view may detach without destroying ownership of the frozen request.
  void detachUI() {}

  void dispose() {
    _disposed = true;
  }
}
