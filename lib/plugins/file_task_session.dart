import 'dart:async';
import 'dart:typed_data';
import 'package:crypto/crypto.dart';
import 'file_task_models.dart';
import 'io_task_models.dart';

bool _same(List<int>? a, List<int>? b) =>
    a != null &&
    b != null &&
    a.length == b.length &&
    Iterable<int>.generate(a.length).every((i) => a[i] == b[i]);

enum FileTaskPhase {
  idle,
  capturing,
  reading,
  finishing,
  verified,
  cancelled,
  failed,
  unknown,
}

enum FileTaskNotice {
  status,
  startUnknown,
  commandUnknown,
  readUnknown,
  controlUnknown,
  integrity,
  identity,
  interrupted,
}

enum _Expected { captured, chunk, finished }

class _DigestSink implements Sink<Digest> {
  Digest? value;
  @override
  void add(Digest data) {
    value = data;
  }

  @override
  void close() {}
}

class FileTaskRecord {
  FileTaskRecord(
    this.phase,
    this.length,
    this.received,
    this.digest,
    this.notice,
  );
  final FileTaskPhase phase;
  final BigInt? length;
  final BigInt received;
  final String? digest;
  final FileTaskNotice? notice;
}

/// One explicit file attempt per backend. No file spool, filesystem access or
/// automatic retry. The caller owns polling; verify() is an explicitly requested
/// bounded drain. Removing a page listener does not cancel or lose the attempt.
class FileTaskSession {
  FileTaskSession(
    this.files,
    this.io, {
    this.pollInterval = const Duration(milliseconds: 30),
    this.observationLimit = const Duration(seconds: 35),
  });
  static final _sessions = Expando<Expando<FileTaskSession>>(
    'file task sessions',
  );
  factory FileTaskSession.forBackend(
    FileTaskBackend files,
    WorkbenchIoTaskControl io,
  ) {
    final pair = _sessions[files] ??= Expando<FileTaskSession>('file task IO');
    return pair[io] ??= FileTaskSession(files, io);
  }
  final FileTaskBackend files;
  final WorkbenchIoTaskControl io;
  final Duration pollInterval, observationLimit;
  final Set<void Function()> _listeners = {};
  void addListener(void Function() listener) => _listeners.add(listener);
  void removeListener(void Function() listener) => _listeners.remove(listener);
  void _notify() {
    for (final f in List.of(_listeners)) {
      f();
    }
  }

  IoTaskSnapshot? _snapshot;
  IoTaskSnapshot? get snapshot => _snapshot;
  FileTaskRequest? _attempt;
  FileTaskRequest? get attempt => _attempt;
  Uint8List? _key;
  FileTaskPhase _phase = FileTaskPhase.idle;
  FileTaskPhase get phase => _phase;
  FileTaskNotice? _notice;
  FileTaskNotice? get notice => _notice;
  bool _trusted = false, _busy = false, _controlBusy = false, _pumping = false;
  bool _cancelled = false, _verifiedBytes = false, _ackUnknown = false;
  int _epoch = 0;
  _Expected? _expected;
  BigInt? _length;
  BigInt? get length => _length;
  BigInt _received = BigInt.zero;
  BigInt get received => _received;
  Uint8List? _expectedHash;
  final _preview = <int>[];
  Uint8List get preview => Uint8List.fromList(_preview).asUnmodifiableView();
  String? _digest;
  String? get digest => _digest;
  Sink<List<int>>? _hash;
  _DigestSink? _digestSink;
  final List<FileTaskRecord> _history = [];
  List<FileTaskRecord> get history => List.unmodifiable(_history);
  bool get busy => _busy || _controlBusy || _pumping;
  bool get ownsTask => _key != null && _same(_key, _snapshot?.key);
  bool get _local =>
      _snapshot?.key == null &&
      _snapshot?.exit == null &&
      _snapshot?.storage == IoStoragePhase.local;
  bool get canStart => !busy && _trusted && _local && _attempt == null;
  bool get canVerify =>
      !busy &&
      _trusted &&
      ownsTask &&
      !_cancelled &&
      _snapshot?.exit == null &&
      (_phase == FileTaskPhase.capturing ||
          _phase == FileTaskPhase.reading ||
          _phase == FileTaskPhase.finishing);
  // Cancellation is allowed while a once-only read is in flight. Its epoch makes
  // that reply ineligible for delivery even when cancellation's own reply is lost.
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

  void _accept(IoTaskSnapshot value, {bool start = false}) {
    if (value.key != null) HttpTaskValidation.identity(value.key!);
    if (value.submission != null)
      HttpTaskValidation.identity(value.submission!);
    if (_key != null &&
        (!_same(value.key, _key) ||
            !_same(value.submission, _attempt?.submission))) {
      throw const FormatException('File task identity changed');
    }
    if (start) {
      if (value.key == null || !_same(value.submission, _attempt?.submission)) {
        throw const FormatException('File submission changed');
      }
      _key = HttpTaskValidation.identity(value.key!);
    }
    _snapshot = value;
    _trusted = true;
    if (value.exit != null &&
        _attempt != null &&
        _phase != FileTaskPhase.verified &&
        _phase != FileTaskPhase.cancelled &&
        _phase != FileTaskPhase.unknown) {
      _phase = FileTaskPhase.failed;
      _notice ??= FileTaskNotice.interrupted;
      _discardHash();
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
      // Only a lost START can be recovered by status. A lost read/command remains
      // uncertain even if status reports ready: the payload is not replayable.
      final recovered =
          _key == null &&
          _attempt != null &&
          _notice == FileTaskNotice.startUnknown &&
          value.key != null &&
          _same(value.submission, _attempt!.submission);
      _accept(value, start: recovered);
      if (recovered) {
        _phase = value.exit == null
            ? FileTaskPhase.capturing
            : FileTaskPhase.failed;
        _notice = value.exit == null ? null : FileTaskNotice.interrupted;
      } else if (_notice == FileTaskNotice.status ||
          _notice == FileTaskNotice.controlUnknown) {
        _notice = null;
      }
    } catch (_) {
      if (epoch == _epoch) {
        _trusted = false;
        _notice ??= FileTaskNotice.status;
      }
    } finally {
      _busy = false;
      _notify();
    }
  }

  Future<void> start(FileTaskRequest request) async {
    if (!canStart) throw StateError('File task admission is unavailable');
    FileTaskValidation.validateRequest(request);
    _attempt = request;
    _busy = true;
    _cancelled = false;
    _notice = null;
    _phase = FileTaskPhase.capturing;
    _expected = _Expected.captured;
    final epoch = ++_epoch;
    _notify();
    try {
      final value = await files.startFile(request);
      if (epoch == _epoch) _accept(value, start: true);
    } catch (_) {
      if (epoch == _epoch) {
        _trusted = false;
        _phase = FileTaskPhase.unknown;
        _notice = FileTaskNotice.startUnknown;
      }
    } finally {
      _busy = false;
      _notify();
    }
  }

  /// One bounded command or delivery. Only the current expected kind may be
  /// consumed. A successful digest is not completion until Finished is received.
  Future<void> advance() async {
    if (_busy ||
        _controlBusy ||
        !_trusted ||
        !ownsTask ||
        _cancelled ||
        ![
          FileTaskPhase.capturing,
          FileTaskPhase.reading,
          FileTaskPhase.finishing,
        ].contains(_phase))
      return;
    _busy = true;
    final epoch = _epoch;
    var delivering = false, decoded = false;
    try {
      if (_expected == null) {
        _expected = _verifiedBytes ? _Expected.finished : _Expected.chunk;
        if (_verifiedBytes) _phase = FileTaskPhase.finishing;
        final value = _verifiedBytes
            ? await files.finishFile(_key!)
            : await files.requestFileChunk(_key!, _received, 65536);
        if (epoch == _epoch) _accept(value);
      } else if (_snapshot?.delivery == IoDeliveryPhase.ready) {
        delivering = true;
        final value = await files.readFile(_key!);
        if (epoch != _epoch) return;
        _accept(value.snapshot);
        if (value.result != null) {
          decoded = true;
          _consume(value.result!);
        }
      }
    } catch (_) {
      if (epoch == _epoch) {
        _phase = decoded ? FileTaskPhase.failed : FileTaskPhase.unknown;
        _notice = decoded
            ? FileTaskNotice.integrity
            : delivering
            ? FileTaskNotice.readUnknown
            : FileTaskNotice.commandUnknown;
        _trusted = false;
        _discardHash();
      }
    } finally {
      _busy = false;
      _notify();
    }
  }

  void _consume(FileTaskResult result) {
    if (result is FileTaskCaptured && _expected == _Expected.captured) {
      if (result.length < BigInt.zero ||
          result.length > _attempt!.maxBytes ||
          result.sha256.length != 32) {
        throw const FormatException('Invalid captured file');
      }
      _length = result.length;
      _expectedHash = result.sha256;
      _digestSink = _DigestSink();
      _hash = sha256.startChunkedConversion(_digestSink!);
      _expected = null;
      _phase = FileTaskPhase.reading;
      if (_length == BigInt.zero) _finishBytes();
    } else if (result is FileTaskChunk && _expected == _Expected.chunk) {
      final end = _received + BigInt.from(result.bytes.length);
      if (result.offset != _received ||
          result.bytes.length > 65536 ||
          result.bytes.isEmpty ||
          end > _length! ||
          result.eof != (end == _length)) {
        throw const FormatException(
          'File chunk gap, duplicate or premature EOF',
        );
      }
      _hash!.add(result.bytes);
      final remaining = 4096 - _preview.length;
      if (remaining > 0) _preview.addAll(result.bytes.take(remaining));
      _received = end;
      _expected = null;
      if (result.eof) _finishBytes();
    } else if (result is FileTaskFinished &&
        _expected == _Expected.finished &&
        _verifiedBytes) {
      _expected = null;
      _phase = FileTaskPhase.verified;
    } else {
      throw const FormatException('Unexpected file result');
    }
  }

  void _finishBytes() {
    _hash!.close();
    _hash = null;
    final value = _digestSink!.value!;
    _digestSink = null;
    if (_received != _length || !_same(value.bytes, _expectedHash)) {
      throw const FormatException('File digest mismatch');
    }
    _digest = value.toString();
    _verifiedBytes = true;
  }

  void _discardHash() {
    _hash = null;
    _digestSink = null;
  }

  /// Explicit user-authorized drain, bounded in memory and observation time.
  /// Status queries can repeat; admitted chunks, Finish and consumed reads cannot.
  Future<void> verify() async {
    if (!canVerify) return;
    _pumping = true;
    final watch = Stopwatch()..start();
    _notify();
    try {
      while (!_cancelled &&
          [
            FileTaskPhase.capturing,
            FileTaskPhase.reading,
            FileTaskPhase.finishing,
          ].contains(_phase)) {
        if (watch.elapsed > observationLimit) {
          _phase = FileTaskPhase.failed;
          _notice = FileTaskNotice.interrupted;
          _discardHash();
          break;
        }
        await refresh();
        if (!_trusted || _cancelled) break;
        await advance();
        if (_snapshot?.delivery != IoDeliveryPhase.ready)
          await Future<void>.delayed(pollInterval);
      }
    } finally {
      _pumping = false;
      _notify();
    }
    if (_phase == FileTaskPhase.failed && canCancel)
      await cancel(preserveFailure: true);
  }

  Future<void> cancel({bool preserveFailure = false}) async {
    if (!canCancel) return;
    _cancelled = true;
    ++_epoch;
    _controlBusy = true;
    _discardHash();
    if (!preserveFailure) _phase = FileTaskPhase.cancelled;
    _notify();
    try {
      _accept(await io.cancelIo(_key!));
    } catch (_) {
      _trusted = false;
      _notice ??= FileTaskNotice.controlUnknown;
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
      _notice = FileTaskNotice.controlUnknown;
    } finally {
      _controlBusy = false;
      _notify();
    }
  }

  Future<void> acknowledge() async {
    if (!canAcknowledge) return;
    _controlBusy = true;
    _notify();
    try {
      _ackUnknown = true;
      final value = await io.acknowledgeIo(_key!);
      if (value.key != null ||
          value.exit != null ||
          value.storage != IoStoragePhase.local) {
        throw const FormatException('File task acknowledgement incomplete');
      }
      _archive();
      _reset();
      _accept(value);
    } catch (_) {
      _trusted = false;
      _notice = FileTaskNotice.controlUnknown;
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
    _history.insert(
      0,
      FileTaskRecord(_phase, _length, _received, _digest, _notice),
    );
    if (_history.length > 5) _history.removeLast();
  }

  void _reset() {
    ++_epoch;
    _attempt = null;
    _key = null;
    _phase = FileTaskPhase.idle;
    _notice = null;
    _expected = null;
    _length = null;
    _received = BigInt.zero;
    _expectedHash = null;
    _preview.clear();
    _digest = null;
    _cancelled = false;
    _verifiedBytes = false;
    _ackUnknown = false;
    _discardHash();
  }
}
