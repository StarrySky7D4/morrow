import 'dart:async';
import 'workbench_backend.dart';

typedef QueryConditions = ({
  int contentGeneration,
  String section,
  String filter,
  String text,
  String sort,
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
    invalidate();
    _backend = backend;
    _conditions = conditions;
    _operation = newQueryOperationId();
    _deferred = deferred;
    phase = QueryPhase.loading;
    if (!deferred) _schedule();
  }

  void invalidate() {
    _serial++;
    _timer?.cancel();
    _timer = null;
    _conditions = null;
    _operation = null;
    failure = null;
    _due = false;
    ids = const [];
    phase = QueryPhase.idle;
  }

  void retry() {
    if (_disposed || phase != QueryPhase.failed || _backend == null) return;
    _serial++;
    if (failure?.terminal ?? false) _operation = newQueryOperationId();
    failure = null;
    ids = const [];
    phase = QueryPhase.loading;
    _schedule();
    onChanged();
  }

  void _schedule() {
    _timer = Timer(debounce, () {
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
        conditions.section,
        conditions.filter,
        conditions.text,
        conditions.sort,
        operation: operation,
      );
      if (!_disposed && serial == _serial) {
        ids = List.unmodifiable(result);
        phase = QueryPhase.ready;
        onChanged();
      }
    } catch (error) {
      if (!_disposed && serial == _serial) {
        failure = error is QueryFailure ? error : null;
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
