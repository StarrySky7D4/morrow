import 'package:flutter/foundation.dart';

import 'io_task_control.dart';
import 'service_run_control.dart';

enum ServiceRunNotice {
  statusFailed,
  startUnknown,
  startRejected,
  controlUnknown,
  identityChanged,
  invalid,
}

class ServiceRunAttemptRecord {
  const ServiceRunAttemptRecord({
    required this.request,
    required this.outcomeUnknown,
    required this.abandoned,
    this.startFailureDetail,
  });
  final ServiceRunRequest request;
  final bool outcomeUnknown, abandoned;
  final String? startFailureDetail;
}

class _IdentityChanged implements Exception {}

/// Backend-owned observation and explicit controls, without timers or retries.
/// Pages remove their listeners when unmounted; only the backend owner disposes
/// this session. In-flight attempts therefore survive a page being replaced.
class ServiceRunSession extends ChangeNotifier {
  ServiceRunSession(this.runBackend, this.ioBackend);
  static final _sessions = Expando<Expando<ServiceRunSession>>(
    'Service run backends',
  );
  factory ServiceRunSession.forBackend(
    WorkbenchServiceRunControl runBackend,
    WorkbenchIoTaskControl ioBackend,
  ) {
    final pair = _sessions[runBackend] ??= Expando<ServiceRunSession>(
      'Service run IO backend',
    );
    return pair[ioBackend] ??= ServiceRunSession(runBackend, ioBackend);
  }

  final WorkbenchServiceRunControl runBackend;
  final WorkbenchIoTaskControl ioBackend;
  ServiceRunSnapshot? service;
  IoTaskSnapshot? task;
  ServiceRunRequest? attempt;
  bool busy = false, trusted = false, startUnknown = false;
  ServiceRunNotice? notice;
  String? startFailureDetail;
  final List<ServiceRunAttemptRecord> _history = [];
  List<ServiceRunAttemptRecord> get history => List.unmodifiable(_history);
  bool _disposed = false;

  bool get _ready => !_disposed && trusted && !busy;
  bool get _local =>
      task?.storage == IoStoragePhase.local &&
      task?.key == null &&
      task?.exit == null;
  bool get _owned =>
      service != null &&
      task?.key != null &&
      listEquals(service!.task.key, task!.key);
  bool get canStart => _ready && _local && !startUnknown && attempt == null;
  bool get canStop =>
      _ready &&
      _owned &&
      !startUnknown &&
      task!.exit == null &&
      task!.storage == IoStoragePhase.running &&
      (service!.phase == ServiceRunPhase.starting ||
          service!.phase == ServiceRunPhase.running);
  bool get canRepair =>
      _ready &&
      _owned &&
      !startUnknown &&
      service!.phase == ServiceRunPhase.exited &&
      task!.exit != null &&
      task!.storage == IoStoragePhase.recoveryRequired;
  bool get canAcknowledge =>
      _ready &&
      _owned &&
      !startUnknown &&
      service!.phase == ServiceRunPhase.exited &&
      task!.exit != null &&
      task!.storage == IoStoragePhase.reclaimed;
  bool get canAbandon => _ready && _local && startUnknown && attempt != null;
  bool get shouldPoll =>
      _ready &&
      _owned &&
      (task!.storage == IoStoragePhase.running ||
          task!.storage == IoStoragePhase.stopping);

  void _changed() {
    if (!_disposed) notifyListeners();
  }

  void _archive({required bool unknown, bool abandoned = false}) {
    final value = attempt;
    if (value == null) return;
    _history.add(
      ServiceRunAttemptRecord(
        request: value,
        outcomeUnknown: unknown,
        abandoned: abandoned,
        startFailureDetail: startFailureDetail,
      ),
    );
    if (_history.length > 5) _history.removeAt(0);
    attempt = null;
  }

  void _validateTask(IoTaskSnapshot value) {
    final key = value.key;
    if (key != null) ServiceRunValidation.identity(key);
    if (value.submission != null) {
      ServiceRunValidation.identity(value.submission!);
    }
    if (key == null &&
            (value.storage != IoStoragePhase.local ||
                value.exit != null ||
                value.submission != null ||
                value.delivery != IoDeliveryPhase.absent) ||
        key != null && value.storage == IoStoragePhase.local ||
        value.exit == null &&
            (value.storage == IoStoragePhase.reclaimed ||
                value.storage == IoStoragePhase.recoveryRequired) ||
        value.exit != null &&
            (value.storage == IoStoragePhase.running ||
                value.storage == IoStoragePhase.stopping)) {
      throw const FormatException('Inconsistent task state');
    }
  }

  void _validateService(
    ServiceRunSnapshot value, {
    Uint8List? key,
    Uint8List? submission,
  }) {
    _validateTask(value.task);
    ServiceRunValidation.identity(value.submission);
    if (value.task.key == null ||
        !listEquals(value.task.submission, value.submission) ||
        key != null && !listEquals(value.task.key, key) ||
        submission != null && !listEquals(value.submission, submission)) {
      throw _IdentityChanged();
    }
    if (value.phase == ServiceRunPhase.exited && value.task.exit == null ||
        value.phase != ServiceRunPhase.exited && value.task.exit != null) {
      throw const FormatException('Service exit is not confirmed');
    }
  }

  /// Two read-only calls bind the visible service to the currently retained task.
  /// A short HTTP task fails the keyed service query and never gets controls.
  Future<void> _inspect({Uint8List? expectedKey}) async {
    if (_disposed) return;
    final current = await ioBackend.ioStatus();
    if (_disposed) return;
    _validateTask(current);
    task = current;
    if (current.key == null) {
      service = null;
      if (!startUnknown) _archive(unknown: false);
      if (expectedKey != null) throw _IdentityChanged();
      return;
    }
    final previous = expectedKey ?? service?.task.key;
    if (previous != null && !listEquals(previous, current.key)) {
      throw _IdentityChanged();
    }
    final value = await runBackend.serviceRunStatus(key: current.key);
    if (_disposed) return;
    _validateService(
      value,
      key: current.key,
      submission: attempt?.submission ?? service?.submission,
    );
    if (current.submission != null &&
        !listEquals(current.submission, value.submission)) {
      throw _IdentityChanged();
    }
    service = value;
    task = value.task;
    // The retained native task and original submission positively identify the
    // lost start receipt. A later empty task alone cannot resolve that outcome.
    startUnknown = false;
  }

  Future<void> refresh() async {
    if (_disposed || busy) return;
    busy = true;
    trusted = false;
    _changed();
    try {
      await _inspect();
      trusted = true;
      notice = startUnknown
          ? ServiceRunNotice.startUnknown
          : startFailureDetail != null
          ? ServiceRunNotice.startRejected
          : null;
    } catch (error) {
      notice = error is _IdentityChanged
          ? ServiceRunNotice.identityChanged
          : ServiceRunNotice.statusFailed;
    } finally {
      busy = false;
      _changed();
    }
  }

  Future<void> start(ServiceRunRequest request) async {
    if (!canStart) return;
    try {
      ServiceRunValidation.request(request);
      if (_history.any(
        (e) => listEquals(e.request.submission, request.submission),
      )) {
        throw const FormatException('Recent attempt already used');
      }
    } catch (_) {
      notice = ServiceRunNotice.invalid;
      _changed();
      return;
    }
    busy = true;
    trusted = false;
    notice = null;
    _changed();
    var sent = false;
    startFailureDetail = null;
    try {
      // Refresh the local task immediately before the single explicit admission.
      await _inspect();
      if (_disposed) return;
      if (!_local) throw _IdentityChanged();
      attempt = request;
      sent = true;
      final value = await runBackend.startServiceRun(request);
      _validateService(value, submission: request.submission);
      service = value;
      task = value.task;
      startUnknown = false;
      trusted = true;
    } catch (error) {
      startUnknown = sent;
      if (sent && error is ServiceRunStartFailure) {
        // A valid error is not evidence that no worker/cleanup task exists.
        // Keep the submitted identity while observing the native owner's state.
        startFailureDetail = String.fromCharCodes(
          error.message.runes.take(1024),
        );
        try {
          await _inspect();
          if (_disposed) return;
          if (_local) {
            _archive(unknown: false);
            startUnknown = false;
          }
          trusted = true;
          notice = ServiceRunNotice.startRejected;
        } catch (inspectionError) {
          notice = inspectionError is _IdentityChanged
              ? ServiceRunNotice.identityChanged
              : ServiceRunNotice.startUnknown;
        }
      } else {
        notice = error is _IdentityChanged
            ? ServiceRunNotice.identityChanged
            : sent
            ? ServiceRunNotice.startUnknown
            : ServiceRunNotice.statusFailed;
      }
    } finally {
      busy = false;
      _changed();
    }
  }

  Future<void> _control(int operation) async {
    final allowed = switch (operation) {
      0 => canStop,
      1 => canRepair,
      _ => canAcknowledge,
    };
    if (!allowed) return;
    final key = service!.task.key!;
    busy = true;
    trusted = false;
    notice = null;
    _changed();
    var sent = false;
    try {
      await _inspect(expectedKey: key);
      if (_disposed) return;
      final valid = switch (operation) {
        0 =>
          task!.exit == null &&
              task!.storage == IoStoragePhase.running &&
              (service!.phase == ServiceRunPhase.running ||
                  service!.phase == ServiceRunPhase.starting),
        1 =>
          service!.phase == ServiceRunPhase.exited &&
              task!.exit != null &&
              task!.storage == IoStoragePhase.recoveryRequired,
        _ =>
          service!.phase == ServiceRunPhase.exited &&
              task!.exit != null &&
              task!.storage == IoStoragePhase.reclaimed,
      };
      if (!valid) throw const FormatException('Control no longer applicable');
      sent = true;
      final receipt = await switch (operation) {
        0 => ioBackend.cancelIo(key),
        1 => ioBackend.repairIo(key),
        _ => ioBackend.acknowledgeIo(key),
      };
      _validateTask(receipt);
      if (operation == 2) {
        if (receipt.key != null || receipt.storage != IoStoragePhase.local) {
          throw _IdentityChanged();
        }
        task = receipt;
        service = null;
        _archive(unknown: false);
        startFailureDetail = null;
      } else {
        if (!listEquals(receipt.key, key)) throw _IdentityChanged();
        task = receipt;
        await _inspect(expectedKey: key);
      }
      trusted = true;
    } catch (error) {
      notice = error is _IdentityChanged
          ? ServiceRunNotice.identityChanged
          : sent
          ? ServiceRunNotice.controlUnknown
          : ServiceRunNotice.statusFailed;
    } finally {
      busy = false;
      _changed();
    }
  }

  Future<void> stop() => _control(0);
  Future<void> repair() => _control(1);
  Future<void> acknowledge() => _control(2);

  /// Explicitly forgets the active proposal after a fresh empty local status.
  /// The finite history still labels it unknown; this is never rollback proof.
  void abandonAttempt() {
    if (!canAbandon) return;
    _archive(unknown: true, abandoned: true);
    startFailureDetail = null;
    startUnknown = false;
    notice = null;
    _changed();
  }

  @override
  void dispose() {
    _disposed = true;
    super.dispose();
  }
}
