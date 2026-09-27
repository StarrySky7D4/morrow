import 'dart:async';
import 'dart:math';
import 'dart:typed_data';

import 'package:crypto/crypto.dart';

import 'guest_mutation_models.dart';
import 'io_task_models.dart';
import 'mutation_task_models.dart';

Uint8List _token() {
  final random = Random.secure();
  final result = Uint8List.fromList(
    List<int>.generate(32, (_) => random.nextInt(256)),
  );
  if (result.every((value) => value == 0)) result[0] = 1;
  return result.asUnmodifiableView();
}

Uint8List _copy(Uint8List bytes) =>
    Uint8List.fromList(bytes).asUnmodifiableView();

bool _same(Uint8List? a, Uint8List? b) =>
    a != null && b != null && GuestMutationValidation.same(a, b);

Duration Function() _monotonicClock() {
  final clock = Stopwatch()..start();
  return () => clock.elapsed;
}

final class _Stopped {
  const _Stopped();
}

final class _Submission {
  const _Submission(this.token, this.previous, this.command);
  final Uint8List token;
  final BigInt previous;
  final GuestMutationCommand? command; // Null only for the original Start.
  GuestMutationCommandKind get kind =>
      command?.kind ?? GuestMutationCommandKind.select;
}

/// The original selected owner and its exact command receipts live outside
/// Widgets. A status observation never reconstructs a lost Core frame.
final class GuestMutationExecutionSession {
  GuestMutationExecutionSession(
    this.mutations,
    this.io, {
    Duration Function()? monotonicNow,
  }) : _now = monotonicNow ?? _monotonicClock();

  static final _sessions = Expando<Expando<GuestMutationExecutionSession>>(
    'guest mutation execution sessions',
  );
  factory GuestMutationExecutionSession.forBackend(
    GuestMutationBackend mutations,
    WorkbenchIoTaskControl io, {
    Duration Function()? monotonicNow,
  }) {
    final pair = _sessions[mutations] ??=
        Expando<GuestMutationExecutionSession>('guest mutation IO');
    return pair[io] ??= GuestMutationExecutionSession(
      mutations,
      io,
      monotonicNow: monotonicNow,
    );
  }

  final GuestMutationBackend mutations;
  final WorkbenchIoTaskControl io;
  final Duration Function() _now;
  Duration? _authorizationStarted;
  bool _authorityEnded = false;
  final Set<void Function()> _listeners = {};
  void addListener(void Function() listener) => _listeners.add(listener);
  void removeListener(void Function() listener) => _listeners.remove(listener);
  void _notify() {
    for (final listener in List<void Function()>.of(_listeners)) {
      try {
        listener();
      } catch (_) {
        // A view callback cannot reclassify a verified owner result.
      }
    }
  }

  bool _busy = false;
  bool _cleanupBusy = false;
  bool _trusted = false;
  bool _submitUnknown = false;
  bool _readUnknown = false;
  bool _resultLost = false;
  bool _ackUnknown = false;
  bool _stopRequested = false;
  bool _awaitingResult = false;
  bool _reviewed = false;
  bool _prepareAttempted = false;
  bool _ready = false;
  bool _executionAttempted = false;
  bool _released = false;
  Completer<void> _stopSignal = Completer<void>();
  IoTaskSnapshot? _snapshot;
  GuestMutationState? _state;
  GuestMutationStartRequest? _request, _lastRequest;
  Uint8List? _key, _plan, _planSha256, _content, _contentSha256;
  Uint8List? _guestReference;
  String? _operationId;
  BigInt? _contentLength, _commandId;
  GuestMutationCommandKind? _lastKind;
  _Submission? _submission;
  GuestMutationReceipt? _receipt;
  GuestMutationResult? _lastResult, _history;
  GuestMutationFrame? _outcome;
  BigInt _stagedBytes = BigInt.zero;
  Object? _error, _cleanupError;

  bool get busy => _busy || _cleanupBusy;
  IoTaskSnapshot? get snapshot => _snapshot;
  GuestMutationState? get state => _state;
  GuestMutationStartRequest? get request => _lastRequest;
  String? get operationId => _operationId;
  Uint8List? get plan => _plan;
  Uint8List? get planSha256 => _planSha256;
  BigInt? get contentLength => _contentLength;
  Uint8List? get contentSha256 => _contentSha256;
  GuestMutationResult? get lastResult => _lastResult;
  GuestMutationFrame? get outcome => _outcome;
  GuestMutationResult? get history => _history;
  BigInt get stagedBytes => _stagedBytes;
  Object? get error => _error;
  Object? get cleanupError => _cleanupError;

  /// Conservative local review window, starting before the first Start send.
  /// This never grants or renews authority; the original host still enforces it.
  bool get authorizationExpired =>
      _request != null &&
      (_authorityEnded ||
          (_authorizationStarted != null &&
              _now() - _authorizationStarted! >=
                  Duration(milliseconds: _request!.selection.timeoutMs)));
  bool get uncertain =>
      _submitUnknown || _readUnknown || _resultLost || _ackUnknown;

  bool get _local =>
      _snapshot?.key == null &&
      _snapshot?.exit == null &&
      _snapshot?.storage == IoStoragePhase.local;
  bool get _ownsTask =>
      _key != null &&
      _same(_key, _snapshot?.key) &&
      _same(_request?.selection.submission, _snapshot?.submission);
  bool get _running =>
      _ownsTask &&
      _snapshot?.exit == null &&
      _snapshot?.storage == IoStoragePhase.running;
  bool get _consumed =>
      _commandId != null &&
      _state?.commandId == _commandId &&
      _state?.kind == _lastKind &&
      _state?.delivery == GuestMutationDelivery.consumed;
  bool get _canIssue =>
      !busy &&
      !authorizationExpired &&
      _trusted &&
      _running &&
      !uncertain &&
      !_stopRequested &&
      !_released &&
      !_awaitingResult &&
      _submission == null &&
      _consumed;
  bool get _canHostIssue =>
      !busy &&
      !authorizationExpired &&
      _trusted &&
      _running &&
      !_ackUnknown &&
      !_stopRequested &&
      !_released &&
      !_awaitingResult &&
      !_submitUnknown &&
      _consumed;
  bool get canStart =>
      !busy && _trusted && _local && _request == null && !uncertain;
  bool get canPrepare =>
      _canIssue &&
      _reviewed &&
      !_prepareAttempted &&
      _planSha256 != null &&
      _state?.reconcileRequired == false &&
      _state?.terminal == false;
  bool get canExecute =>
      _canIssue &&
      _ready &&
      !_executionAttempted &&
      _guestReference != null &&
      _state?.reconcileRequired == false &&
      _state?.terminal == false;
  bool get canQuery => _canIssue && _guestReference != null && _plan != null;
  bool get canHostQuery => _canHostIssue && _plan != null;
  bool get canCancelPlan =>
      (_canIssue || _canHostIssue) && _plan != null && !_executionAttempted;
  bool get canRelease => (_canIssue || _canHostIssue) && _commandId != null;
  bool get canStop => !_cleanupBusy && _trusted && _running && !_ackUnknown;
  bool get canRepair =>
      !busy &&
      _trusted &&
      _ownsTask &&
      _snapshot?.exit != null &&
      _snapshot?.storage == IoStoragePhase.recoveryRequired;
  bool get canAcknowledge =>
      !busy &&
      _trusted &&
      _ownsTask &&
      !_ackUnknown &&
      _snapshot?.exit != null &&
      _snapshot?.storage == IoStoragePhase.reclaimed;
  bool get canRetrySubmission =>
      !busy &&
      !authorizationExpired &&
      _trusted &&
      !_stopRequested &&
      !_ackUnknown &&
      _submitUnknown &&
      _submission != null &&
      (_submission!.command == null
          ? ((_local &&
                    (_snapshot?.submission == null ||
                        _same(
                          _snapshot?.submission,
                          _request?.selection.submission,
                        ))) ||
                _ownsTask)
          : _ownsTask);
  bool get canRead =>
      !busy &&
      _trusted &&
      _running &&
      !_stopRequested &&
      !_ackUnknown &&
      !_submitUnknown &&
      !_readUnknown &&
      !_resultLost &&
      _awaitingResult &&
      _receipt != null &&
      _state?.commandId == _receipt!.commandId &&
      _state?.kind == _receipt!.kind &&
      _state?.delivery == GuestMutationDelivery.ready;
  bool get canAbandon =>
      !busy &&
      _trusted &&
      _local &&
      _key == null &&
      _request != null &&
      _submission?.command == null &&
      uncertain;

  void _begin(bool allowed, String action) {
    if (busy) throw StateError('Guest mutation session is busy');
    if (!allowed) throw StateError('Guest mutation $action is unavailable');
    _busy = true;
    _notify();
  }

  void _end() {
    _busy = false;
    _notify();
  }

  void _record(Object error, {bool uncertain = false, bool cleanup = false}) {
    if (cleanup) {
      _cleanupError = error;
    } else {
      _error ??= error;
    }
    if (uncertain) _resultLost = true;
    _notify();
  }

  void _clearContent() {
    _content?.fillRange(0, _content!.length, 0);
    _content = null;
  }

  void _validateIo(IoTaskSnapshot value, {bool allowLocal = false}) {
    if (value.key != null) GuestMutationValidation.identity(value.key!);
    if (value.submission != null) {
      GuestMutationValidation.identity(value.submission!);
    }
    if (allowLocal &&
        value.key == null &&
        value.exit == null &&
        value.storage == IoStoragePhase.local) {
      // A Local record has no owned worker. An unrelated historical token
      // permits a fresh review or local abandonment, never an exact retry.
      return;
    }
    if (value.key == null ||
        !_same(value.key, _key) ||
        !_same(value.submission, _request?.selection.submission)) {
      throw const FormatException('Guest owner identity changed');
    }
  }

  void _validateState(GuestMutationState value) {
    GuestMutationValidation.u64(value.commandId, positive: true);
    GuestMutationValidation.u64(value.stagedBytes);
    if (value.reference != null) {
      GuestMutationValidation.identity(value.reference!);
    }
    if (value.expectedIdentity != null) {
      GuestMutationValidation.identity(value.expectedIdentity!);
    }
    if (value.reviewedPlanSha256 != null) {
      GuestMutationValidation.identity(value.reviewedPlanSha256!);
    }
    if (value.selected != (value.reference != null) ||
        (value.expectedIdentity != null && value.reference == null) ||
        value.stagedBytes >
            BigInt.from(GuestMutationValidation.maxContentBytes) ||
        (value.approvalDelivered &&
            (value.reviewedPlanSha256 == null ||
                (!value.selected &&
                    !(value.terminal &&
                        (value.kind == GuestMutationCommandKind.release ||
                            value.kind ==
                                GuestMutationCommandKind.hostRelease))))) ||
        (value.permitDelivered && !value.approvalDelivered)) {
      throw const FormatException('Contradictory guest owner state');
    }
  }

  Future<T?> _interruptible<T>(Future<T> operation) async {
    final value = await Future.any<Object?>([
      operation.then<Object?>((result) => result),
      _stopSignal.future.then<Object?>((_) => const _Stopped()),
    ]).timeout(const Duration(seconds: 15));
    return value is _Stopped ? null : value as T;
  }

  Future<GuestMutationTaskReply> _send(_Submission submitted) =>
      submitted.command == null
      ? mutations.start(_request!)
      : mutations.submit(_key!, submitted.token, submitted.command!);

  void _accept(GuestMutationTaskReply value, _Submission submitted) {
    _validateState(value.state);
    final start = submitted.command == null;
    if (start) {
      final key = value.io.key;
      if (key == null ||
          !_same(value.io.submission, _request?.selection.submission) ||
          (_key != null && !_same(_key, key))) {
        throw const FormatException('Guest Start identity changed');
      }
      _key ??= GuestMutationValidation.identity(key);
    }
    final expected = start ? BigInt.one : submitted.previous + BigInt.one;
    final receipt = value.receipt;
    if (value.commandId != expected ||
        receipt == null ||
        receipt.commandId != expected ||
        !_same(receipt.key, _key) ||
        !_same(receipt.submission, submitted.token) ||
        receipt.kind != submitted.kind ||
        receipt.operationId != submitted.command?.operationId ||
        receipt.disposition != submitted.command?.disposition ||
        !_sameOrNull(
          receipt.expectedGuestReference,
          submitted.command?.expectedGuestReference,
        ) ||
        receipt.expectedChunkEnd !=
            (submitted.kind == GuestMutationCommandKind.chunk
                ? submitted.command!.offset! +
                      BigInt.from(submitted.command!.bytes!.length)
                : null) ||
        value.state.commandId < expected ||
        (value.state.commandId == expected &&
            value.state.kind != submitted.kind)) {
      throw const FormatException('Guest command receipt changed');
    }
    GuestMutationValidation.receipt(receipt);
    _validateIo(value.io);
    _snapshot = value.io;
    _state = value.state;
    _trusted = true;
    _submitUnknown = false;
    _readUnknown = false;
    _commandId = expected;
    _lastKind = submitted.kind;
    _receipt = receipt;
    _awaitingResult = true;
    if (value.state.commandId != expected ||
        value.state.delivery == GuestMutationDelivery.consumed) {
      _awaitingResult = false;
      _resultLost = true;
      _record(StateError('Guest result was already consumed'));
    }
  }

  static bool _sameOrNull(Uint8List? a, Uint8List? b) =>
      a == null && b == null || _same(a, b);

  Future<bool> _submit(_Submission submitted) async {
    if (_stopRequested) return false;
    _submission = submitted; // Exact token and payload before the await.
    _receipt = null;
    _submitUnknown = true;
    _resultLost = false;
    _awaitingResult = false;
    _readUnknown = false;
    try {
      final value = await _interruptible(_send(submitted));
      if (value == null || _stopRequested) {
        _record(StateError('Guest stopped with a pending submission'));
        return false;
      }
      _accept(value, submitted);
      return !_resultLost;
    } catch (error) {
      _record(error);
      return false;
    }
  }

  void _acceptRead(GuestMutationTaskRead value) {
    final receipt = _receipt!;
    _validateState(value.reply.state);
    _validateIo(value.reply.io);
    if (value.reply.commandId != receipt.commandId ||
        value.reply.state.commandId != receipt.commandId ||
        value.reply.state.kind != receipt.kind) {
      throw const FormatException('Guest read command identity changed');
    }
    _snapshot = value.reply.io;
    _state = value.reply.state;
    _trusted = true;
    final result = value.result;
    if (result == null) {
      if (value.reply.state.delivery == GuestMutationDelivery.consumed) {
        throw StateError('Guest result consumed without its payload');
      }
      return;
    }
    if (value.reply.state.delivery != GuestMutationDelivery.consumed) {
      throw const FormatException('Guest result was not consumed');
    }
    if (result.kind == GuestMutationResultKind.frame) {
      if (result.frame == null ||
          result.owner != null ||
          result.failure != null) {
        throw const FormatException('Malformed guest Core frame');
      }
      GuestMutationValidation.frame(result.frame!, receipt);
      if (receipt.kind == GuestMutationCommandKind.prepare &&
          result.frame!.status == GuestMutationFrameStatus.completed &&
          result.frame!.phase == GuestMutationFramePhase.prepared) {
        if (_guestReference != null &&
            !_same(_guestReference, result.frame!.reference)) {
          throw const FormatException('Guest lease reference changed');
        }
        _guestReference ??= _copy(result.frame!.reference);
      }
      if (receipt.kind == GuestMutationCommandKind.execute) {
        _outcome = result.frame;
      } else if (receipt.kind == GuestMutationCommandKind.query) {
        _history = result;
      }
      if (result.frame!.stagedBytes > _stagedBytes) {
        _stagedBytes = result.frame!.stagedBytes;
      }
    } else if (result.kind == GuestMutationResultKind.owner) {
      if (result.owner == null ||
          result.frame != null ||
          result.failure != null) {
        throw const FormatException('Malformed guest owner result');
      }
      final expected = switch (receipt.kind) {
        GuestMutationCommandKind.select => MutationResultKind.selected,
        GuestMutationCommandKind.buildPlan => MutationResultKind.planned,
        GuestMutationCommandKind.hostQuery => MutationResultKind.history,
        GuestMutationCommandKind.hostCancelPlan =>
          MutationResultKind.planCancelled,
        GuestMutationCommandKind.hostRelease => MutationResultKind.released,
        _ => MutationResultKind.failure,
      };
      if (result.owner!.kind != expected &&
          result.owner!.kind != MutationResultKind.failure) {
        throw const FormatException('Wrong guest owner result');
      }
      if (receipt.kind == GuestMutationCommandKind.select &&
          result.owner!.kind == MutationResultKind.selected &&
          (!_same(result.owner!.reference, value.reply.state.reference) ||
              !_sameOrNull(
                result.owner!.expectedIdentity,
                value.reply.state.expectedIdentity,
              ))) {
        throw const FormatException('Guest selected target changed');
      }
      if (receipt.kind == GuestMutationCommandKind.hostQuery &&
          result.owner!.kind == MutationResultKind.history) {
        _history = result;
      }
    } else if (result.kind != GuestMutationResultKind.failure ||
        result.failure == null) {
      throw const FormatException('Malformed guest failure result');
    }
    _lastResult = result;
    if (result.frame?.status == GuestMutationFrameStatus.expired ||
        result.failure?.fault == GuestMutationFault.deadline) {
      _authorityEnded = true;
      _ready = false;
    }
    if (result.kind == GuestMutationResultKind.failure ||
        result.owner?.kind == MutationResultKind.failure) {
      _ready = false;
      _record(StateError('Guest mutation command failed: ${receipt.kind}'));
    }
    if (value.reply.state.reconcileRequired || value.reply.state.terminal) {
      _ready = false;
    }
    _awaitingResult = false;
    _submission = null;
    _receipt = null;
    _readUnknown = false;
    _notify();
  }

  Future<GuestMutationResult?> _waitResult() async {
    final clock = Stopwatch()..start();
    while (clock.elapsed < const Duration(seconds: 15)) {
      if (_stopRequested) {
        _readUnknown = true;
        _record(StateError('Guest stopped with an unread result'));
        return null;
      }
      try {
        final remaining = const Duration(seconds: 15) - clock.elapsed;
        final value = await _interruptible(
          mutations.read(_receipt!).timeout(remaining),
        );
        if (value == null || _stopRequested) {
          _readUnknown = true;
          _record(StateError('Guest stopped during result read'));
          return null;
        }
        _acceptRead(value);
        if (value.result != null) return value.result;
      } catch (error) {
        _readUnknown = true;
        _record(error);
        return null;
      }
      await Future.any<void>([
        Future<void>.delayed(const Duration(milliseconds: 10)),
        _stopSignal.future,
      ]);
    }
    _readUnknown = true;
    _record(TimeoutException('Guest mutation result deadline'));
    return null;
  }

  Future<GuestMutationResult?> _command(GuestMutationCommand command) async {
    if (_stopRequested) return null;
    if (authorizationExpired) {
      _ready = false;
      _record(
        StateError('Guest approval window expired; stop and reclaim the task'),
      );
      return null;
    }
    final submitted = _Submission(_token(), _commandId!, command);
    if (!await _submit(submitted)) return null;
    return _waitResult();
  }

  bool _expectOwner(GuestMutationResult? value, MutationResultKind kind) {
    if (value?.kind != GuestMutationResultKind.owner ||
        value?.owner?.kind != kind) {
      if (value != null) {
        _record(StateError('Guest expected owner $kind, got ${value.kind}'));
      }
      return false;
    }
    return true;
  }

  bool _expectFrame(
    GuestMutationResult? value,
    GuestMutationFrameKind kind, {
    GuestMutationFramePhase? phase,
  }) {
    final frame = value?.frame;
    if (value?.kind != GuestMutationResultKind.frame ||
        frame == null ||
        frame.kind != kind ||
        frame.status != GuestMutationFrameStatus.completed ||
        (phase != null && frame.phase != phase)) {
      if (value != null) {
        _record(
          StateError('Guest expected completed $kind, got ${value.kind}'),
        );
      }
      return false;
    }
    return true;
  }

  /// Local status is observation only. It never renews a lease or a permit.
  Future<void> refresh() async {
    _begin(true, 'refresh');
    try {
      final current = await io.ioStatus().timeout(const Duration(seconds: 15));
      if (_ackUnknown &&
          current.key == null &&
          current.exit == null &&
          current.storage == IoStoragePhase.local) {
        _validateIo(current, allowLocal: true);
        _resetLocal(current);
        return;
      }
      if (_key == null) {
        if (_request == null ||
            (current.key == null && current.storage == IoStoragePhase.local)) {
          _validateIo(current, allowLocal: true);
          _snapshot = current;
          _trusted = true;
          return;
        }
        if (!_same(current.submission, _request!.selection.submission)) {
          throw const FormatException('Foreign guest mutation task');
        }
        _key = GuestMutationValidation.identity(current.key!);
      }
      _validateIo(current);
      _snapshot = current;
      _trusted = true;
      if (_ackUnknown &&
          current.exit != null &&
          current.storage == IoStoragePhase.reclaimed) {
        _ackUnknown = false;
      }
      try {
        final status = await mutations
            .status(_key!)
            .timeout(const Duration(seconds: 15));
        if (status.commandId != BigInt.zero) {
          throw const FormatException('Guest status contains a receipt');
        }
        _validateState(status.state);
        _validateIo(status.io);
        if (_commandId != null && status.state.commandId < _commandId!) {
          throw const FormatException('Guest command regressed');
        }
        if (_commandId != null &&
            status.state.commandId == _commandId &&
            _lastKind != null &&
            status.state.kind != _lastKind) {
          throw const FormatException('Guest command kind changed');
        }
        _snapshot = status.io;
        _state = status.state;
        if (status.state.reconcileRequired || status.state.terminal) {
          _ready = false;
        }
        if (_awaitingResult &&
            status.state.commandId == _commandId &&
            status.state.delivery == GuestMutationDelivery.consumed) {
          _resultLost = true;
          _awaitingResult = false;
          _record(StateError('Guest result was consumed without payload'));
        } else if (_readUnknown &&
            _receipt != null &&
            status.state.commandId == _receipt!.commandId &&
            status.state.kind == _receipt!.kind &&
            status.state.delivery == GuestMutationDelivery.ready) {
          _readUnknown = false;
        }
      } catch (error) {
        _state = null;
        _record(error, cleanup: true);
      }
    } catch (error) {
      _trusted = false;
      _record(error, cleanup: true);
    } finally {
      _end();
    }
  }

  /// Start and build the host's canonical plan. No guest Prepare is issued.
  Future<void> review(
    GuestMutationStartRequest request, {
    required String operationId,
    Uint8List? content,
  }) async {
    if (busy) throw StateError('Guest mutation session is busy');
    if (!canStart) throw StateError('Guest mutation review is unavailable');
    GuestMutationValidation.start(request);
    if ((request.selection.disposition == MutationDisposition.delete &&
            content != null) ||
        (content?.length ?? 0) > GuestMutationValidation.maxContentBytes) {
      throw const FormatException('Invalid guest mutation content');
    }
    final body = request.selection.disposition == MutationDisposition.create
        ? Uint8List.fromList(content ?? Uint8List(0))
        : Uint8List(0);
    final length = BigInt.from(body.length);
    final digest = request.selection.disposition == MutationDisposition.create
        ? _copy(Uint8List.fromList(sha256.convert(body).bytes))
        : null;
    MutationTaskValidation.buildPlan(operationId, length, digest);
    _begin(true, 'review');
    _stopSignal = Completer<void>();
    _stopRequested = false;
    _error = null;
    _cleanupError = null;
    _request = request;
    _lastRequest = request;
    _operationId = operationId;
    _contentLength = length;
    _contentSha256 = digest;
    _content = body;
    _plan = null;
    _planSha256 = null;
    _guestReference = null;
    _lastResult = null;
    _history = null;
    _outcome = null;
    _stagedBytes = BigInt.zero;
    _reviewed = false;
    _prepareAttempted = false;
    _executionAttempted = false;
    _ready = false;
    _released = false;
    _authorityEnded = false;
    _authorizationStarted = _now();
    try {
      if (!await _submit(
        _Submission(request.selection.submission, BigInt.zero, null),
      )) {
        return;
      }
      if (!_expectOwner(await _waitResult(), MutationResultKind.selected)) {
        return;
      }
      final planned = await _command(
        GuestMutationCommand.buildPlan(
          operationId: operationId,
          contentLength: length,
          contentSha256: digest,
        ),
      );
      if (!_expectOwner(planned, MutationResultKind.planned)) return;
      final plan = planned!.owner!.plan;
      if (plan == null) {
        _record(
          const FormatException('Guest owner omitted its plan'),
          uncertain: true,
        );
        return;
      }
      MutationTaskValidation.plan(plan);
      final hash = _copy(Uint8List.fromList(sha256.convert(plan).bytes));
      if (!_same(hash, _state?.reviewedPlanSha256) ||
          _state?.reconcileRequired != false ||
          _state?.terminal != false ||
          _stopRequested) {
        _record(
          const FormatException('Guest reviewed plan changed'),
          uncertain: true,
        );
        return;
      }
      _plan = _copy(plan);
      _planSha256 = hash;
      _reviewed = true;
      _notify();
    } catch (error) {
      _record(error, uncertain: true);
    } finally {
      if (!_reviewed) _clearContent();
      _end();
    }
  }

  /// Only an exact previously displayed plan hash can enter guest Prepare.
  Future<void> prepare({required Uint8List reviewedPlanSha256}) async {
    if (busy) throw StateError('Guest mutation session is busy');
    if (!canPrepare || !_same(reviewedPlanSha256, _planSha256)) {
      throw StateError('Guest reviewed plan is unavailable or changed');
    }
    _begin(true, 'prepare');
    _prepareAttempted = true;
    final body = _content;
    try {
      final disposition = _request!.selection.disposition;
      final prepared = await _command(
        GuestMutationCommand.prepare(
          planSha256: _planSha256!,
          operationId: _operationId!,
          disposition: disposition,
        ),
      );
      if (!_expectFrame(
        prepared,
        disposition == MutationDisposition.create
            ? GuestMutationFrameKind.prepareCreate
            : GuestMutationFrameKind.prepareDelete,
        phase: GuestMutationFramePhase.prepared,
      )) {
        return;
      }
      if (disposition == MutationDisposition.create) {
        if (body == null) {
          _record(
            const FormatException('Guest content was discarded'),
            uncertain: true,
          );
          return;
        }
        for (
          var offset = 0;
          offset < body.length;
          offset += GuestMutationValidation.maxChunkBytes
        ) {
          if (_stopRequested) return;
          final end = min(
            offset + GuestMutationValidation.maxChunkBytes,
            body.length,
          );
          final chunk = _copy(Uint8List.fromList(body.sublist(offset, end)));
          final staged = await _command(
            GuestMutationCommand.chunk(
              offset: BigInt.from(offset),
              bytes: chunk,
              operationId: _operationId!,
              guestReference: _guestReference!,
            ),
          );
          if (!_expectFrame(
            staged,
            GuestMutationFrameKind.chunk,
            phase: GuestMutationFramePhase.prepared,
          )) {
            return;
          }
          if (staged!.frame!.stagedBytes != BigInt.from(end)) {
            _record(
              const FormatException('Guest staged byte count changed'),
              uncertain: true,
            );
            return;
          }
        }
        final committed = await _command(
          GuestMutationCommand.commitContent(
            operationId: _operationId!,
            guestReference: _guestReference!,
          ),
        );
        if (!_expectFrame(
          committed,
          GuestMutationFrameKind.commit,
          phase: GuestMutationFramePhase.prepared,
        )) {
          return;
        }
        if (!committed!.frame!.durableContent ||
            committed.frame!.stagedBytes != _contentLength) {
          _record(
            const FormatException('Guest content was not durable'),
            uncertain: true,
          );
          return;
        }
      }
      if (_stopRequested ||
          _state?.reconcileRequired != false ||
          _state?.terminal != false ||
          _snapshot?.storage != IoStoragePhase.running) {
        return;
      }
      _ready = true;
      _notify();
    } catch (error) {
      _record(error, uncertain: true);
    } finally {
      _clearContent();
      _end();
    }
  }

  /// A second explicit confirmation; an attempted Execute cannot be replayed.
  Future<void> execute({required Uint8List reviewedPlanSha256}) async {
    if (busy) throw StateError('Guest mutation session is busy');
    if (!canExecute || !_same(reviewedPlanSha256, _planSha256)) {
      throw StateError('Guest execution review is unavailable or changed');
    }
    _begin(true, 'execute');
    _executionAttempted = true;
    _ready = false;
    try {
      final result = await _command(
        GuestMutationCommand.execute(
          planSha256: _planSha256!,
          operationId: _operationId!,
          guestReference: _guestReference!,
        ),
      );
      if (result == null) return;
      final frame = result.frame;
      if (result.kind != GuestMutationResultKind.frame ||
          frame?.kind != GuestMutationFrameKind.execute ||
          frame?.status != GuestMutationFrameStatus.completed ||
          frame?.phase != GuestMutationFramePhase.observed ||
          frame?.effect == GuestMutationFrameEffect.unspecified) {
        _record(
          StateError('Guest execution has no observed OS outcome'),
          uncertain: true,
        );
      }
    } finally {
      _end();
    }
  }

  Future<void> query() async {
    _begin(canQuery, 'query');
    final previouslyReady = _ready;
    _ready = false; // History can preserve, never create, Execute permission.
    try {
      final result = await _command(
        GuestMutationCommand.query(
          operationId: _operationId!,
          guestReference: _guestReference!,
        ),
      );
      if (result == null) return;
      if (result.kind != GuestMutationResultKind.frame ||
          result.frame?.kind != GuestMutationFrameKind.query ||
          (result.frame?.status != GuestMutationFrameStatus.completed &&
              result.frame?.status !=
                  GuestMutationFrameStatus.outcomeUnknown)) {
        _record(StateError('Guest history is unavailable'));
        return;
      }
      final frame = result.frame!;
      final create =
          _request!.selection.disposition == MutationDisposition.create;
      if (previouslyReady &&
          frame.status == GuestMutationFrameStatus.completed &&
          frame.phase == GuestMutationFramePhase.prepared &&
          _same(frame.reference, _guestReference) &&
          frame.durableContent == create &&
          frame.stagedBytes == (create ? _contentLength : BigInt.zero) &&
          _state?.reconcileRequired == false &&
          _state?.terminal == false &&
          !uncertain &&
          !_stopRequested) {
        _ready = true;
        _notify();
      }
    } finally {
      _end();
    }
  }

  /// Original-owner history remains distinct from a correlated guest frame.
  Future<void> hostQuery() async {
    _begin(canHostQuery, 'host query');
    final previouslyReady = _ready;
    _ready = false;
    try {
      final result = await _command(GuestMutationCommand.hostQuery());
      if (!_expectOwner(result, MutationResultKind.history)) return;
      final history = result!.owner!;
      final create =
          _request!.selection.disposition == MutationDisposition.create;
      if (previouslyReady &&
          history.phase == MutationPhase.prepared &&
          history.operationId == _operationId &&
          history.durableContent == create &&
          history.stagedBytes == (create ? _contentLength : BigInt.zero) &&
          _guestReference != null &&
          _state?.reconcileRequired == false &&
          _state?.terminal == false &&
          !uncertain &&
          !_stopRequested) {
        _ready = true;
        _notify();
      }
    } finally {
      _end();
    }
  }

  Future<void> cancelPlan() async {
    _begin(canCancelPlan, 'cancel plan');
    _ready = false;
    _clearContent();
    try {
      if (_guestReference != null && !uncertain) {
        _expectFrame(
          await _command(
            GuestMutationCommand.cancelPlan(
              operationId: _operationId!,
              guestReference: _guestReference!,
            ),
          ),
          GuestMutationFrameKind.cancelPlan,
          phase: GuestMutationFramePhase.cancelledBeforeDispatch,
        );
      } else {
        _expectOwner(
          await _command(GuestMutationCommand.hostCancelPlan()),
          MutationResultKind.planCancelled,
        );
      }
    } finally {
      _end();
    }
  }

  /// Fall back to the original owner when the guest lease is unavailable.
  Future<void> release() async {
    _begin(canRelease, 'release');
    _ready = false;
    _clearContent();
    try {
      final useGuest = _guestReference != null && !uncertain;
      final result = await _command(
        useGuest
            ? GuestMutationCommand.release(
                operationId: _operationId!,
                guestReference: _guestReference!,
              )
            : GuestMutationCommand.hostRelease(),
      );
      final success = useGuest
          ? _expectFrame(
              result,
              GuestMutationFrameKind.release,
              phase: GuestMutationFramePhase.none,
            )
          : _expectOwner(result, MutationResultKind.released);
      if (success) _released = true;
    } finally {
      _end();
    }
  }

  /// An explicit same-token retry within the original review window. Even an
  /// exact retry can be a first delivery, so expiry must block sending it.
  Future<void> retrySubmission() async {
    _begin(canRetrySubmission, 'retry submission');
    try {
      final submitted = _submission!;
      try {
        final value = await _interruptible(_send(submitted));
        if (value == null || _stopRequested) {
          _record(StateError('Guest stopped during submission retry'));
          return;
        }
        _accept(value, submitted);
      } catch (error) {
        _record(error);
      }
    } finally {
      _end();
    }
  }

  Future<void> read() async {
    _begin(canRead, 'read');
    try {
      await _waitResult();
    } finally {
      _end();
    }
  }

  Future<void> stop() async {
    if (!canStop) throw StateError('Guest mutation stop is unavailable');
    _cleanupBusy = true;
    _stopRequested = true;
    _ready = false;
    _clearContent();
    if (!_stopSignal.isCompleted) _stopSignal.complete();
    _notify();
    try {
      final value = await io.cancelIo(_key!);
      _validateIo(value);
      _snapshot = value;
    } catch (error) {
      _trusted = false;
      _record(error, cleanup: true);
    } finally {
      _cleanupBusy = false;
      _notify();
    }
  }

  Future<void> repair() async {
    _begin(canRepair, 'repair');
    try {
      final value = await io.repairIo(_key!);
      _validateIo(value);
      _snapshot = value;
    } catch (error) {
      _trusted = false;
      _record(error, cleanup: true);
    } finally {
      _end();
    }
  }

  void _resetLocal(IoTaskSnapshot local) {
    _clearContent();
    _authorizationStarted = null;
    _authorityEnded = false;
    _snapshot = local;
    _state = null;
    _request = null;
    _key = null;
    _commandId = null;
    _lastKind = null;
    _submission = null;
    _receipt = null;
    _guestReference = null;
    _reviewed = false;
    _prepareAttempted = false;
    _executionAttempted = false;
    _ready = false;
    _released = false;
    _trusted = true;
    _submitUnknown = false;
    _readUnknown = false;
    _resultLost = false;
    _ackUnknown = false;
    _stopRequested = false;
    _awaitingResult = false;
    // Last request, plan, hash, and results remain bounded view evidence.
  }

  Future<void> acknowledge() async {
    _begin(canAcknowledge, 'acknowledge');
    _ackUnknown = true;
    try {
      final value = await io.acknowledgeIo(_key!);
      if (value.key != null ||
          value.exit != null ||
          value.storage != IoStoragePhase.local) {
        throw const FormatException('Guest owner was not acknowledged');
      }
      _validateIo(value, allowLocal: true);
      _resetLocal(value);
    } catch (error) {
      _trusted = false;
      _record(error, cleanup: true);
    } finally {
      _end();
    }
  }

  Future<void> abandon() async {
    _begin(canAbandon, 'abandon');
    try {
      _resetLocal(_snapshot!);
    } finally {
      _end();
    }
  }
}
