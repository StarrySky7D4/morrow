import 'dart:typed_data';

import 'mutation_recovery_session.dart';
import 'mutation_task_models.dart';

/// Bounded presentation state for one recovery session. Retained plans and
/// checkpoints are observations, not approval or permission to mutate a file.
/// The caller must check its current catalog and native authority again.
final class MutationRecoveryViewState {
  MutationRecoveryViewState(this.session) {
    session.addListener(_onSession);
    _sync();
  }

  static final _states = Expando<MutationRecoveryViewState>(
    'mutation recovery views',
  );

  factory MutationRecoveryViewState.forSession(
    MutationRecoverySession session,
  ) => _states[session] ??= MutationRecoveryViewState(session);

  final MutationRecoverySession session;
  final Set<void Function()> _listeners = {};
  MutationResult? _seenPage;
  MutationResult? _seenReconciliation;
  MutationResult? _page;
  MutationDiscoverRequest? _pageRequest;
  Uint8List? _selectedPlan;
  MutationDiscoverRequest? _selectedRequest;
  MutationResult? _lastReconciliation;

  /// The most recently delivered discovery page, including an empty page.
  MutationResult? get page => _page;

  /// The exact request whose scope produced [page].
  MutationDiscoverRequest? get pageRequest => _pageRequest;

  /// Host-session continuation from [page], if that page is nonterminal.
  Uint8List? get checkpoint => _page?.checkpoint;

  /// The original scope of [selectedPlan], retained across later pages.
  MutationDiscoverRequest? get selectedRequest => _selectedRequest;
  Uint8List? get selectedPlan => _selectedPlan;

  /// The most recently delivered reconciliation result, if any.
  MutationResult? get lastReconciliation => _lastReconciliation;

  void addListener(void Function() listener) => _listeners.add(listener);
  void removeListener(void Function() listener) => _listeners.remove(listener);

  void _notify() {
    for (final listener in List<void Function()>.of(_listeners)) {
      try {
        listener();
      } catch (_) {
        // A presentation callback cannot reclassify a successful host action.
      }
    }
  }

  void _onSession() {
    _sync();
    _notify();
  }

  void _sync() {
    final deliveredPage = session.page;
    if (deliveredPage != null && !identical(deliveredPage, _seenPage)) {
      final request = session.discoveryRequest;
      if (deliveredPage.kind == MutationResultKind.plans && request != null) {
        // MutationRecoverySession accepted and bounded this result before it
        // notified us. Its model owns immutable copies of every byte field.
        _page = deliveredPage;
        _pageRequest = request;
      }
    }
    _seenPage = deliveredPage;

    final deliveredReconciliation = session.reconciliation;
    if (deliveredReconciliation != null &&
        !identical(deliveredReconciliation, _seenReconciliation) &&
        deliveredReconciliation.kind == MutationResultKind.reconciled) {
      _lastReconciliation = deliveredReconciliation;
    }
    _seenReconciliation = deliveredReconciliation;
  }

  /// Selects exactly one opaque plan from the retained page. This selection
  /// stays bound to that page's original request even when a new scope arrives.
  void selectPlan(int index) {
    final plans = _page?.plans;
    final request = _pageRequest;
    if (plans == null || request == null) {
      throw StateError('No delivered discovery page');
    }
    RangeError.checkValidIndex(index, plans);
    final plan = plans[index];
    MutationTaskValidation.plan(plan);
    _selectedPlan = Uint8List.fromList(plan).asUnmodifiableView();
    _selectedRequest = request;
    _notify();
  }

  void clearSelection() {
    if (_selectedPlan == null && _selectedRequest == null) return;
    _selectedPlan = null;
    _selectedRequest = null;
    _notify();
  }
}
