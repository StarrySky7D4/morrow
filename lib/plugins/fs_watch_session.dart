import 'dart:typed_data';
import 'fs_watch_models.dart';
import 'io_task_models.dart';

enum FsWatchSessionPhase {
  idle,
  running,
  reviewReady,
  reviewed,
  acknowledged,
  unknown,
}

final class FsWatchRecord {
  const FsWatchRecord(this.request, this.result);
  final FsWatchRequest request;
  final FsWatchResult result;
}

/// One original observation, retained with its backend pair across page changes.
/// Transport loss seals this attempt; status cannot recreate a consumed receipt.
final class FsWatchSession {
  FsWatchSession(this.watch, this.io);
  static final _sessions = Expando<Expando<FsWatchSession>>(
    'directory watch sessions',
  );
  factory FsWatchSession.forBackend(
    FsWatchBackend watch,
    WorkbenchIoTaskControl io,
  ) {
    final pair = _sessions[watch] ??= Expando<FsWatchSession>(
      'directory watch IO',
    );
    return pair[io] ??= FsWatchSession(watch, io);
  }
  final FsWatchBackend watch;
  final WorkbenchIoTaskControl io;
  final _listeners = <void Function()>{};
  void addListener(void Function() f) => _listeners.add(f);
  void removeListener(void Function() f) => _listeners.remove(f);
  void _notify() {
    for (final f in List.of(_listeners)) {
      f();
    }
  }

  FsWatchRequest? _attempt;
  FsWatchRequest? get attempt => _attempt;
  FsWatchSnapshot? _snapshot;
  FsWatchSnapshot? get snapshot => _snapshot;
  IoTaskSnapshot? _task;
  IoTaskSnapshot? get task => _task;
  FsWatchResult? _result;
  FsWatchResult? get result => _result;
  final _history = <FsWatchRecord>[];
  List<FsWatchRecord> get history => List.unmodifiable(_history);
  Uint8List? _key;
  bool _busy = false,
      _trusted = false,
      _readAttempted = false,
      _cancelSent = false;
  bool _viewWithdrawn = false;
  bool get busy => _busy;
  FsWatchSessionPhase _phase = FsWatchSessionPhase.idle;
  FsWatchSessionPhase get phase => _phase;
  bool get _sealed =>
      _phase == FsWatchSessionPhase.unknown ||
      _phase == FsWatchSessionPhase.acknowledged;
  bool get ownsTask => _key != null && fsWatchSame(_key, _task?.key);
  bool get canStart =>
      !_busy &&
      !_sealed &&
      _trusted &&
      !_viewWithdrawn &&
      _attempt == null &&
      _task?.key == null &&
      _task?.exit == null &&
      _task?.storage == IoStoragePhase.local;
  bool get canPoll =>
      !_busy &&
      !_sealed &&
      _trusted &&
      ownsTask &&
      _result == null &&
      !_readAttempted;
  bool get canRead =>
      canPoll && _snapshot?.ready == true && _task?.exit != null;
  bool get canCancel =>
      !_busy &&
      !_sealed &&
      _trusted &&
      ownsTask &&
      !_cancelSent &&
      _task?.exit == null;
  bool get _retirementProven {
    final r = _result;
    return r != null &&
        !r.unknown &&
        r.nativeRetirementComplete &&
        (r.workerJoined || (r.error == FsWatchError.spawn && !r.hasSnapshot)) &&
        r.resources == 0 &&
        r.jobs == 0 &&
        r.ancestorClosed == r.ancestorObjects &&
        r.nativeClosed == r.nativeHandles &&
        (!r.pickerStarted || r.pickerClosed);
  }

  bool get canRepair =>
      !_busy &&
      !_sealed &&
      _trusted &&
      ownsTask &&
      _retirementProven &&
      _task?.exit != null &&
      _task?.storage == IoStoragePhase.recoveryRequired;
  bool get canAcknowledge =>
      !_busy &&
      !_sealed &&
      _trusted &&
      ownsTask &&
      _retirementProven &&
      _snapshot?.delivered == true &&
      _task?.storage == IoStoragePhase.reclaimed &&
      _task?.exit?.disconnect == IoJobError.none &&
      _task?.exit?.maintenance == IoJobError.none;
  bool get canBeginNew =>
      !_busy &&
      _phase == FsWatchSessionPhase.acknowledged &&
      _history.length < 128;
  void beginNew() {
    if (!canBeginNew) throw StateError('Original observation is unresolved');
    _history.add(FsWatchRecord(_attempt!, _result!));
    _attempt = null;
    _snapshot = null;
    _result = null;
    _key = null;
    _readAttempted = false;
    _cancelSent = false;
    _viewWithdrawn = false;
    _phase = FsWatchSessionPhase.idle;
    _notify();
  }

  void _unknown() {
    _trusted = false;
    _phase = FsWatchSessionPhase.unknown;
  }

  Future<void> withdrawView() async {
    _viewWithdrawn = true;
    if (canCancel) await cancel();
  }

  Future<void> _settled() async {
    _busy = false;
    _notify();
    if (_viewWithdrawn && canCancel) await cancel();
  }

  void _acceptTask(IoTaskSnapshot value, {bool first = false}) {
    if (_attempt == null ||
        value.key == null ||
        !fsWatchSame(value.submission, _attempt!.submission) ||
        (!first && !fsWatchSame(value.key, _key))) {
      throw const FormatException('Original observation identity changed');
    }
    final key = HttpTaskValidation.identity(value.key!);
    if (first) _key = key;
    _task = value;
    _trusted = true;
  }

  void _accept(
    FsWatchSnapshot value, {
    bool first = false,
    bool reading = false,
  }) {
    if (_attempt == null ||
        value.frames > _attempt!.limits.frames ||
        value.frames < (_snapshot?.frames ?? 0) ||
        (value.ready && value.delivered) ||
        (value.delivered && !reading && _result == null) ||
        ((value.ready || value.delivered) && value.task.exit == null)) {
      throw const FormatException('Original observation state changed');
    }
    _acceptTask(value.task, first: first);
    _snapshot = value;
    if (_result == null)
      _phase = value.ready
          ? FsWatchSessionPhase.reviewReady
          : FsWatchSessionPhase.running;
  }

  Future<void> initialize() async {
    if (_busy || _sealed || _attempt != null) return;
    _viewWithdrawn = false;
    _busy = true;
    try {
      _task = await io.ioStatus();
      _trusted = true;
    } catch (_) {
      _unknown();
    } finally {
      await _settled();
    }
  }

  Future<void> start(FsWatchRequest value) async {
    if (!canStart) throw StateError('Observation admission unavailable');
    _attempt = value;
    _busy = true;
    _phase = FsWatchSessionPhase.running;
    _notify();
    try {
      _accept(await watch.startFsWatch(value), first: true);
    } catch (_) {
      _unknown();
    } finally {
      await _settled();
    }
  }

  Future<void> poll() async {
    if (!canPoll) return;
    _busy = true;
    try {
      _accept(await watch.pollFsWatch(_key!));
    } catch (_) {
      _unknown();
    } finally {
      await _settled();
    }
  }

  Future<void> read() async {
    if (!canRead) throw StateError('Original receipt unavailable');
    _busy = true;
    _readAttempted = true;
    try {
      final reply = await watch.readFsWatch(_key!);
      _accept(reply.snapshot, reading: true);
      final value = reply.result;
      if (value == null ||
          !reply.snapshot.delivered ||
          reply.snapshot.ready ||
          (!value.unknown &&
              value.deliveries.length != reply.snapshot.frames) ||
          value.deliveries.length > reply.snapshot.frames) {
        throw const FormatException('Original receipt absent or consumed');
      }
      _result = value;
      if (value.unknown || !value.nativeRetirementComplete) {
        _unknown();
      } else {
        _phase = FsWatchSessionPhase.reviewed;
      }
    } catch (_) {
      _unknown();
    } finally {
      await _settled();
    }
  }

  Future<void> cancel() async {
    if (!canCancel) throw StateError('Original cancellation unavailable');
    _busy = true;
    _cancelSent = true;
    try {
      _acceptTask(await io.cancelIo(_key!));
    } catch (_) {
      _unknown();
    } finally {
      await _settled();
    }
  }

  Future<void> repair() async {
    if (!canRepair) throw StateError('Original cleanup unavailable');
    _busy = true;
    try {
      _acceptTask(await io.repairIo(_key!));
    } catch (_) {
      _unknown();
    } finally {
      await _settled();
    }
  }

  Future<void> acknowledge() async {
    if (!canAcknowledge)
      throw StateError('Original close evidence unavailable');
    _busy = true;
    try {
      final value = await io.acknowledgeIo(_key!);
      if (value.key != null ||
          value.submission != null ||
          value.exit != null ||
          value.storage != IoStoragePhase.local ||
          value.delivery != IoDeliveryPhase.absent) {
        throw const FormatException('Original acknowledgement unresolved');
      }
      _task = value;
      _trusted = true;
      _phase = FsWatchSessionPhase.acknowledged;
    } catch (_) {
      _unknown();
    } finally {
      await _settled();
    }
  }
}
