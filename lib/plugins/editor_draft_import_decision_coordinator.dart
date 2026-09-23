import 'editor_draft_import.dart';
import 'editor_draft_import_codec.dart';

/// The requested action has a confirmed host receipt, but the subsequent
/// read-only refresh failed. The caller can keep the receipt and refresh later.
final class EditorDraftImportDecisionRefreshFailure implements Exception {
  const EditorDraftImportDecisionRefreshFailure({
    required this.confirmed,
    required this.cause,
  });

  final EditorDraftImportSnapshot confirmed;
  final Object cause;

  @override
  String toString() =>
      'Draft import action confirmed; recovery list refresh failed';
}

/// Coordinates explicit recovery actions for one workspace. Discovery only
/// reads host journals; no business card is submitted by this controller.
final class EditorDraftImportDecisionCoordinator {
  EditorDraftImportDecisionCoordinator(
    this._control, {
    required this.isCurrent,
  });

  final EditorDraftImportControl _control;
  final bool Function() isCurrent;
  List<EditorDraftImportDecision> _decisions = const [];
  bool _busy = false;
  bool _disposed = false;

  List<EditorDraftImportDecision> get decisions => _decisions;
  bool get busy => _busy;

  void dispose() {
    _disposed = true;
  }

  Future<List<EditorDraftImportDecision>> refresh() async {
    _enter();
    try {
      return await _refresh();
    } finally {
      _busy = false;
    }
  }

  Future<EditorDraftImportSnapshot> retry(EditorDraftImportDecision observed) =>
      _act(observed, cancel: false);

  Future<EditorDraftImportSnapshot> cancel(
    EditorDraftImportDecision observed,
  ) => _act(observed, cancel: true);

  Future<EditorDraftImportSnapshot> _act(
    EditorDraftImportDecision observed, {
    required bool cancel,
  }) async {
    _enter();
    var submitted = false;
    try {
      final intent = observed.intent;
      final inspected = await _control.inspectDecision(intent);
      _requireCurrent();
      final fresh = inspected.decision;
      if (inspected.kind != EditorDraftImportResultKind.decision ||
          fresh == null ||
          fresh.operation != observed.operation ||
          fresh.expectedGeneration != observed.expectedGeneration ||
          !EditorDraftImportCodec.sameRequest(
            fresh.request,
            observed.request,
          )) {
        throw StateError('Original draft import decision changed');
      }
      if (cancel) {
        if (fresh.status == EditorDraftImportDecisionStatus.committed) {
          throw StateError('Committed draft import cannot be cancelled');
        }
      } else if (fresh.status == EditorDraftImportDecisionStatus.cancelled ||
          fresh.status == EditorDraftImportDecisionStatus.conflict) {
        throw StateError('Draft import decision cannot be retried');
      }
      _requireCurrent();
      submitted = true;
      final result = cancel
          ? await _control.cancelDecision(intent)
          : await _control.abandon(intent);
      try {
        _requireCurrent();
        await _refresh();
      } catch (error) {
        throw EditorDraftImportDecisionRefreshFailure(
          confirmed: result,
          cause: error,
        );
      }
      return result;
    } catch (error) {
      if (error is EditorDraftImportFailure ||
          error is EditorDraftImportDecisionRefreshFailure) {
        rethrow;
      }
      if (submitted) {
        throw EditorDraftImportFailure(
          intent: observed.intent,
          outcomeUnknown: true,
          cause: error,
        );
      }
      rethrow;
    } finally {
      _busy = false;
    }
  }

  Future<List<EditorDraftImportDecision>> _refresh() async {
    _requireCurrent();
    final next = await _control.discoverDecisions();
    _requireCurrent();
    _decisions = List<EditorDraftImportDecision>.unmodifiable(next);
    return _decisions;
  }

  void _enter() {
    if (_busy) throw StateError('Draft import recovery is already running');
    _requireCurrent();
    _busy = true;
  }

  void _requireCurrent() {
    if (_disposed || !isCurrent()) {
      throw StateError('Draft import workspace changed');
    }
  }
}
