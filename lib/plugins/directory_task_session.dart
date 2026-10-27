import 'dart:typed_data';
import 'directory_task_models.dart';
import 'io_task_models.dart';

bool _same(List<int>? a, List<int>? b) =>
    a != null &&
    b != null &&
    a.length == b.length &&
    Iterable<int>.generate(a.length).every((i) => a[i] == b[i]);

enum DirectoryTaskPhase {
  idle,
  capturing,
  ready,
  running,
  completed,
  cancelled,
  failed,
  unknown,
}

enum DirectoryTaskNotice {
  status,
  startUnknown,
  commandUnknown,
  readUnknown,
  controlUnknown,
  interrupted,
}

enum _Expected { captured, guest }

class DirectoryTaskRecord {
  DirectoryTaskRecord(this.phase, this.notice, this.entries, this.submission);
  final DirectoryTaskPhase phase;
  final DirectoryTaskNotice? notice;
  final int? entries;
  final Uint8List submission;
}

/// Retained by the backend pair, independently of pages and view listeners.
/// Once-only commands are never recovered by re-sending; only a lost START can
/// be located through the original submission. Worker reclamation is separate.
class DirectoryTaskSession {
  DirectoryTaskSession(
    this.directories,
    this.io, {
    this.pollInterval = const Duration(milliseconds: 30),
    this.observationLimit = const Duration(seconds: 35),
  });
  static final _sessions = Expando<Expando<DirectoryTaskSession>>(
    'directory tasks',
  );
  factory DirectoryTaskSession.forBackend(
    DirectoryTaskBackend directories,
    WorkbenchIoTaskControl io,
  ) {
    final pair = _sessions[directories] ??= Expando<DirectoryTaskSession>(
      'directory IO',
    );
    return pair[io] ??= DirectoryTaskSession(directories, io);
  }
  final DirectoryTaskBackend directories;
  final WorkbenchIoTaskControl io;
  final Duration pollInterval, observationLimit;
  final _listeners = <void Function()>{};
  void addListener(void Function() listener) => _listeners.add(listener);
  void removeListener(void Function() listener) => _listeners.remove(listener);
  void _notify() {
    for (final f in List.of(_listeners)) {
      f();
    }
  }

  DirectoryTaskRequest? _attempt;
  DirectoryTaskRequest? get attempt => _attempt;
  IoTaskSnapshot? _snapshot;
  IoTaskSnapshot? get snapshot => _snapshot;
  Uint8List? _key;
  DirectoryTaskPhase _phase = DirectoryTaskPhase.idle;
  DirectoryTaskPhase get phase => _phase;
  DirectoryTaskNotice? _notice;
  DirectoryTaskNotice? get notice => _notice;
  DirectoryTaskCaptured? _captured;
  DirectoryTaskCaptured? get captured => _captured;
  DirectoryTaskGuest? _result;
  DirectoryTaskGuest? get result => _result;
  _Expected? _expected;
  bool _trusted = false, _busy = false, _controlBusy = false, _pumping = false;
  bool _cancelled = false, _ackUnknown = false;
  bool _nativeViewRevoked = false;
  int _epoch = 0;
  final _history = <DirectoryTaskRecord>[];
  List<DirectoryTaskRecord> get history => List.unmodifiable(_history);
  bool get busy => _busy || _controlBusy || _pumping;
  bool get ownsTask => _key != null && _same(_key, _snapshot?.key);
  bool get _local =>
      _snapshot?.key == null &&
      _snapshot?.exit == null &&
      _snapshot?.storage == IoStoragePhase.local;
  bool get canStart =>
      !busy && _trusted && _local && _attempt == null && _history.length < 512;
  bool get canExecute =>
      !busy &&
      _trusted &&
      ownsTask &&
      !_cancelled &&
      _snapshot?.exit == null &&
      [
        DirectoryTaskPhase.capturing,
        DirectoryTaskPhase.ready,
        DirectoryTaskPhase.running,
      ].contains(_phase);
  bool get canCancel =>
      !_controlBusy &&
      ownsTask &&
      _snapshot?.exit == null &&
      (!_cancelled ||
          (_trusted && _snapshot?.storage == IoStoragePhase.running));
  bool get canRepair =>
      !busy &&
      _trusted &&
      ownsTask &&
      _snapshot?.exit != null &&
      _snapshot?.storage == IoStoragePhase.recoveryRequired;
  bool get canAcknowledge =>
      !busy &&
      _trusted &&
      ownsTask &&
      _snapshot?.exit != null &&
      _snapshot?.storage == IoStoragePhase.reclaimed;
  bool get canAbandon =>
      !busy && _trusted && _local && _attempt != null && _key == null;

  void _accept(
    IoTaskSnapshot value, {
    bool start = false,
    bool completed = false,
  }) {
    if (value.key != null) HttpTaskValidation.identity(value.key!);
    if (value.submission != null) {
      HttpTaskValidation.identity(value.submission!);
    }
    if (_key != null &&
        (!_same(value.key, _key) ||
            !_same(value.submission, _attempt?.submission))) {
      throw const FormatException('Directory owner identity changed');
    }
    if (start) {
      if (value.key == null || !_same(value.submission, _attempt?.submission)) {
        throw const FormatException('Directory submission changed');
      }
      _key = HttpTaskValidation.identity(value.key!);
    }
    _snapshot = value;
    _trusted = true;
    if (value.exit != null &&
        !completed &&
        [
          DirectoryTaskPhase.capturing,
          DirectoryTaskPhase.ready,
          DirectoryTaskPhase.running,
        ].contains(_phase)) {
      _phase = DirectoryTaskPhase.failed;
      _notice ??= DirectoryTaskNotice.interrupted;
    }
  }

  Future<void> refresh() async {
    if (_busy || _controlBusy) return;
    _busy = true;
    final epoch = _epoch;
    try {
      final value = _key == null || _ackUnknown
          ? await io.ioStatus()
          : await io.pollIo(_key!);
      if (epoch != _epoch) return;
      if (_ackUnknown &&
          value.key == null &&
          value.exit == null &&
          value.storage == IoStoragePhase.local) {
        _archive();
        _reset();
        _accept(value);
        return;
      }
      final recovered =
          _key == null &&
          _attempt != null &&
          _notice == DirectoryTaskNotice.startUnknown &&
          value.key != null &&
          _same(value.submission, _attempt!.submission);
      _accept(value, start: recovered);
      if (recovered) {
        _phase = _nativeViewRevoked
            ? DirectoryTaskPhase.unknown
            : value.exit == null
            ? DirectoryTaskPhase.capturing
            : DirectoryTaskPhase.failed;
        _notice = _nativeViewRevoked
            ? DirectoryTaskNotice.interrupted
            : value.exit == null
            ? null
            : DirectoryTaskNotice.interrupted;
      } else if (_notice == DirectoryTaskNotice.status ||
          _notice == DirectoryTaskNotice.controlUnknown) {
        _notice = null;
      }
    } catch (_) {
      if (epoch == _epoch) {
        _trusted = false;
        _notice ??= DirectoryTaskNotice.status;
      }
    } finally {
      _busy = false;
      _notify();
    }
  }

  Future<void> start(DirectoryTaskRequest value) async {
    if (!canStart) throw StateError('Directory admission unavailable');
    DirectoryTaskValidation.request(value);
    _attempt = value;
    _busy = true;
    _cancelled = false;
    _notice = null;
    _phase = DirectoryTaskPhase.capturing;
    _expected = _Expected.captured;
    final epoch = ++_epoch;
    _notify();
    try {
      final response = await directories.startDirectory(value);
      if (epoch == _epoch) {
        _accept(response, start: true);
        // A view can leave while START is still in flight. Keep its original
        // acknowledgement, then cancel that exact owner once; never replace it.
        if (_nativeViewRevoked && canCancel) await cancel();
      }
    } catch (_) {
      if (epoch == _epoch) {
        _trusted = false;
        _phase = DirectoryTaskPhase.unknown;
        _notice = DirectoryTaskNotice.startUnknown;
      }
    } finally {
      _busy = false;
      _notify();
    }
  }

  /// Revocation is sticky even before the original START acknowledges a key.
  /// A lost reply remains Unknown; revocation never probes or replays it.
  Future<void> revokeNativePickerView() async {
    if (_attempt?.nativePicker != true ||
        _nativeViewRevoked ||
        ![
          DirectoryTaskPhase.capturing,
          DirectoryTaskPhase.ready,
          DirectoryTaskPhase.running,
          DirectoryTaskPhase.unknown,
        ].contains(_phase)) {
      return;
    }
    _nativeViewRevoked = true;
    _cancelled = true;
    if (_phase == DirectoryTaskPhase.unknown) {
      _notify();
      return;
    }
    _phase = DirectoryTaskPhase.cancelled;
    if (canCancel) {
      await cancel();
    } else {
      _notify();
    }
  }

  Future<void> advance() async {
    if (_busy ||
        _controlBusy ||
        !_trusted ||
        !ownsTask ||
        _cancelled ||
        ![
          DirectoryTaskPhase.capturing,
          DirectoryTaskPhase.ready,
          DirectoryTaskPhase.running,
        ].contains(_phase)) {
      return;
    }
    _busy = true;
    final epoch = _epoch;
    var reading = false;
    try {
      if (_expected == null && _phase == DirectoryTaskPhase.ready) {
        _expected = _Expected.guest;
        _phase = DirectoryTaskPhase.running;
        final value = await directories.runDirectoryGuest(
          _key!,
          _attempt!.timeoutMs,
        );
        if (epoch == _epoch) _accept(value);
      } else if (_snapshot?.delivery == IoDeliveryPhase.ready) {
        reading = true;
        final value = _expected == _Expected.guest
            ? await directories.readDirectoryGuest(_key!)
            : await directories.readDirectory(_key!);
        if (epoch != _epoch) return;
        _accept(
          value.snapshot,
          completed:
              value.result is DirectoryTaskGuest &&
              _expected == _Expected.guest,
        );
        final result = value.result;
        if (result == null) return;
        if (result is DirectoryTaskCaptured &&
            _expected == _Expected.captured &&
            result.entries >= 0 &&
            result.entries <= _attempt!.maxEntries &&
            result.metadataBytes >= BigInt.zero &&
            result.metadataBytes <= _attempt!.maxMetadataBytes) {
          _captured = result;
          _expected = null;
          _phase = DirectoryTaskPhase.ready;
        } else if (result is DirectoryTaskRefused &&
            _expected == _Expected.captured) {
          _expected = null;
          _cancelled = true;
          _phase = result.fault == DirectoryCommandFault.cancelled
              ? DirectoryTaskPhase.cancelled
              : DirectoryTaskPhase.failed;
          if (result.fault == DirectoryCommandFault.expired) {
            _notice = DirectoryTaskNotice.interrupted;
          }
        } else if (result is DirectoryTaskGuest &&
            _expected == _Expected.guest) {
          _result = result;
          _expected = null;
          _phase = result.unknown
              ? DirectoryTaskPhase.unknown
              : result.succeeded
              ? DirectoryTaskPhase.completed
              : DirectoryTaskPhase.failed;
          if (result.unknown) _notice = DirectoryTaskNotice.readUnknown;
        } else {
          throw const FormatException('Unexpected directory delivery');
        }
      }
    } catch (_) {
      if (epoch == _epoch) {
        _trusted = false;
        _phase = DirectoryTaskPhase.unknown;
        _notice = reading
            ? DirectoryTaskNotice.readUnknown
            : DirectoryTaskNotice.commandUnknown;
      }
    } finally {
      _busy = false;
      _notify();
    }
  }

  /// Explicit user action drains one original capture and one guest invocation.
  /// Timeout/lost reply leaves the attempt retained for inspection and cancel.
  Future<void> execute() async {
    if (!canExecute) return;
    _pumping = true;
    final watch = Stopwatch()..start();
    _notify();
    try {
      while (!_cancelled &&
          [
            DirectoryTaskPhase.capturing,
            DirectoryTaskPhase.ready,
            DirectoryTaskPhase.running,
          ].contains(_phase)) {
        if (watch.elapsed > observationLimit) {
          _phase = DirectoryTaskPhase.unknown;
          _notice = DirectoryTaskNotice.interrupted;
          break;
        }
        await refresh();
        if (!_trusted || _cancelled) break;
        await advance();
        if (_snapshot?.delivery != IoDeliveryPhase.ready) {
          await Future<void>.delayed(pollInterval);
        }
      }
    } finally {
      _pumping = false;
      _notify();
    }
  }

  Future<void> cancel() async {
    if (!canCancel) return;
    _cancelled = true;
    ++_epoch;
    _controlBusy = true;
    // A later physical cancellation never relabels an already unknown result.
    if (_phase != DirectoryTaskPhase.unknown) {
      _phase = DirectoryTaskPhase.cancelled;
    }
    _notify();
    try {
      _accept(await io.cancelIo(_key!));
    } catch (_) {
      _trusted = false;
      _notice ??= DirectoryTaskNotice.controlUnknown;
    } finally {
      _controlBusy = false;
      _notify();
    }
  }

  Future<void> repair() async {
    if (!canRepair) return;
    _controlBusy = true;
    _notify();
    try {
      _accept(await io.repairIo(_key!));
    } catch (_) {
      _trusted = false;
      _notice = DirectoryTaskNotice.controlUnknown;
    } finally {
      _controlBusy = false;
      _notify();
    }
  }

  Future<void> acknowledge() async {
    if (!canAcknowledge) return;
    _controlBusy = true;
    _ackUnknown = true;
    _notify();
    try {
      final value = await io.acknowledgeIo(_key!);
      if (value.key != null ||
          value.exit != null ||
          value.storage != IoStoragePhase.local) {
        throw const FormatException('Directory acknowledgement incomplete');
      }
      _archive();
      _reset();
      _accept(value);
    } catch (_) {
      _trusted = false;
      _notice = DirectoryTaskNotice.controlUnknown;
    } finally {
      _controlBusy = false;
      _notify();
    }
  }

  void abandon() {
    if (!canAbandon) return;
    _archive();
    _reset();
    _notify();
  }

  void _archive() {
    if (_attempt != null) {
      _history.add(
        DirectoryTaskRecord(
          _phase,
          _notice,
          _captured?.entries,
          _attempt!.submission,
        ),
      );
    }
  }

  void _reset() {
    ++_epoch;
    _attempt = null;
    _key = null;
    _captured = null;
    _result = null;
    _expected = null;
    _phase = DirectoryTaskPhase.idle;
    _notice = null;
    _cancelled = false;
    _ackUnknown = false;
    _nativeViewRevoked = false;
  }
}
