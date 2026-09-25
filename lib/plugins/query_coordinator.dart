import 'dart:async';
import 'workbench_backend.dart';
import 'workbench_ids.dart';

typedef QueryConditions = ({
  int contentGeneration,
  WorkbenchPage section,
  WorkbenchFilter filter,
  String text,
  WorkbenchSort sort,
});

enum QueryPhase { idle, loading, ready, failed }

/// Coalesces pending work; an already issued host request is never cancelled.
class QueryCoordinator {
  QueryCoordinator({
    required this.onChanged,
    this.debounce = const Duration(milliseconds: 200),
  });
  final void Function() onChanged;
  final Duration debounce;
  WorkbenchBackend? _backend;
  QueryConditions? _conditions;
  String? _operation;
  QueryFailure? failure;
  Timer? _timer;
  int _serial = 0;
  bool _disposed = false, _running = false, _due = false, _deferred = false;
  QueryPhase phase = QueryPhase.idle;
  List<String> ids = const [];

  /// Same-view content refresh keeps the last confirmed membership visible.
  /// New search/page/access conditions never reuse those rows.
  bool refreshing = false;
  // Only completed operation references, never a second authority for results.
  // Returning to a view re-delivers via the host, which checks current access
  // and the original observation. No fabricated query execution or audit event.
  final _completed = <QueryConditions, String>{};
  static const _maxCompletedViews = 8;

  /// Synchronous selection is safe during build: only asynchronous completion notifies.
  void select(
    WorkbenchBackend? backend,
    QueryConditions conditions, {
    bool deferred = false,
  }) {
    if (_disposed) return;
    if (backend == null || !backend.writable) {
      if (_backend != null || phase != QueryPhase.idle) invalidate();
      _backend = null;
      return;
    }
    if (identical(_backend, backend) &&
        _conditions == conditions &&
        _deferred == deferred) {
      return;
    }
    final previousConditions = _conditions;
    final sameView =
        identical(_backend, backend) &&
        previousConditions != null &&
        previousConditions.section == conditions.section &&
        previousConditions.filter == conditions.filter &&
        previousConditions.text == conditions.text &&
        previousConditions.sort == conditions.sort;
    final retain =
        sameView && !deferred && (phase == QueryPhase.ready || refreshing);
    final previousIds = ids;
    final typing =
        previousConditions != null &&
        previousConditions.text != conditions.text;
    if (!identical(_backend, backend) ||
        _conditions?.contentGeneration != conditions.contentGeneration) {
      _completed.clear();
    }
    _resetSelection();
    _backend = backend;
    _conditions = conditions;
    final previous = _completed.remove(conditions);
    if (previous != null) _completed[conditions] = previous;
    _operation = previous ?? newQueryOperationId();
    _deferred = deferred;
    phase = QueryPhase.loading;
    refreshing = retain;
    if (retain) ids = previousIds;
    // Only text input needs debounce. Tabs, sorting and confirmed edits should
    // reach the host immediately, while the serialized lane still coalesces.
    if (!deferred) _schedule(typing ? debounce : Duration.zero);
  }

  void releaseCompletedViews() => _completed.clear();

  void invalidate() {
    _completed.clear();
    _resetSelection();
  }

  void _resetSelection() {
    _serial++;
    _timer?.cancel();
    _timer = null;
    _conditions = null;
    _operation = null;
    failure = null;
    _due = false;
    ids = const [];
    refreshing = false;
    phase = QueryPhase.idle;
  }

  void retry() {
    if (_disposed || phase != QueryPhase.failed || _backend == null) return;
    _serial++;
    if (failure?.terminal ?? false) _operation = newQueryOperationId();
    failure = null;
    ids = const [];
    phase = QueryPhase.loading;
    _schedule(Duration.zero);
    onChanged();
  }

  void _schedule(Duration delay) {
    _timer = Timer(delay, () {
      _timer = null;
      _due = true;
      _drain();
    });
  }

  Future<void> _drain() async {
    if (_disposed || _running || !_due) return;
    final backend = _backend;
    final conditions = _conditions;
    final operation = _operation;
    if (backend == null || conditions == null || operation == null) return;
    _due = false;
    _running = true;
    final serial = _serial;
    try {
      final result = await backend.query(
        WorkbenchV1.section(conditions.section),
        WorkbenchV1.filter(conditions.filter),
        conditions.text,
        WorkbenchV1.sort(conditions.sort),
        operation: operation,
      );
      if (!_disposed && serial == _serial) {
        ids = List.unmodifiable(result);
        _completed.remove(conditions);
        _completed[conditions] = operation;
        while (_completed.length > _maxCompletedViews) {
          _completed.remove(_completed.keys.first);
        }
        phase = QueryPhase.ready;
        refreshing = false;
        onChanged();
      }
    } catch (error) {
      if (!_disposed && serial == _serial) {
        failure = error is QueryFailure ? error : null;
        ids = const [];
        refreshing = false;
        if (failure?.terminal ?? false) _completed.remove(conditions);
        phase = QueryPhase.failed;
        onChanged();
      }
    } finally {
      _running = false;
      if (!_disposed) _drain();
    }
  }

  void dispose() {
    _disposed = true;
    invalidate();
    _backend = null;
  }
}
