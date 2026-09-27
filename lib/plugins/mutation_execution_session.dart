import 'dart:async';
import 'dart:math';
import 'dart:typed_data';

import 'package:crypto/crypto.dart';

import 'io_task_models.dart';
import 'mutation_task_models.dart';

bool _sameBytes(List<int>? left, List<int>? right) =>
    left != null &&
    right != null &&
    left.length == right.length &&
    Iterable<int>.generate(left.length).every((i) => left[i] == right[i]);

Uint8List _newToken() {
  final random = Random.secure();
  final value = Uint8List.fromList(
    List<int>.generate(32, (_) => random.nextInt(256)),
  );
  if (value.every((byte) => byte == 0)) value[0] = 1;
  return value.asUnmodifiableView();
}

enum _Command {
  start,
  buildPlan,
  prepare,
  chunk,
  commit,
  execute,
  query,
  cancelPlan,
  release,
}

final class _Stopped {
  const _Stopped();
}

extension on _Command {
  MutationCommandKind get kind => switch (this) {
    _Command.start => MutationCommandKind.select,
    _Command.buildPlan => MutationCommandKind.buildPlan,
    _Command.prepare => MutationCommandKind.prepare,
    _Command.chunk => MutationCommandKind.chunk,
    _Command.commit => MutationCommandKind.commitContent,
    _Command.execute => MutationCommandKind.execute,
    _Command.query => MutationCommandKind.query,
    _Command.cancelPlan => MutationCommandKind.cancelPlan,
    _Command.release => MutationCommandKind.release,
  };
}

final class _Submission {
  _Submission(
    this.command,
    this.token,
    this.previous, {
    this.offset,
    this.bytes,
  });
  final _Command command;
  final Uint8List token;
  final BigInt previous;
  final BigInt? offset;
  final Uint8List? bytes;
}

/// One original selected owner. A lost receipt never starts another command.
/// The caller must explicitly review the requested mutation before prepare,
/// and explicitly invoke execute after durable preparation is known.
final class MutationExecutionSession {
  MutationExecutionSession(this.mutations, this.io);

  static final _sessions = Expando<Expando<MutationExecutionSession>>(
    'mutation execution sessions',
  );
  factory MutationExecutionSession.forBackend(
    MutationTaskBackend mutations,
    WorkbenchIoTaskControl io,
  ) {
    final pair = _sessions[mutations] ??= Expando<MutationExecutionSession>(
      'mutation execution IO',
    );
    return pair[io] ??= MutationExecutionSession(mutations, io);
  }

  final MutationTaskBackend mutations;
  final WorkbenchIoTaskControl io;
  final Set<void Function()> _listeners = {};
  void addListener(void Function() listener) => _listeners.add(listener);
  void removeListener(void Function() listener) => _listeners.remove(listener);
  void _notify() {
    for (final listener in List<void Function()>.of(_listeners)) {
      try {
        listener();
      } catch (_) {
        // A view callback cannot change a known host result into uncertainty.
      }
    }
  }

  bool _busy = false;
  bool _cleanupBusy = false;
  bool _trusted = false;
  bool _uncertain = false;
  bool _ackUnknown = false;
  bool _stopRequested = false;
  bool _awaitingResult = false;
  bool _readUnknown = false;
  bool _resultLost = false;
  bool _ready = false;
  bool _executionAttempted = false;
  bool _released = false;
  Completer<void> _stopSignal = Completer<void>();
  IoTaskSnapshot? _snapshot;
  MutationState? _state;
  MutationStartRequest? _request;
  MutationStartRequest? _lastRequest;
  Uint8List? _key;
  Uint8List? _plan;
  MutationResult? _lastResult;
  MutationResult? _outcome;
  MutationResult? _history;
  BigInt _stagedBytes = BigInt.zero;
  BigInt? _commandId;
  _Submission? _submission;
  String? _operationId;
  BigInt? _contentLength;
  Uint8List? _contentSha256;
  Object? _error;
  Object? _cleanupError;

  bool get busy => _busy || _cleanupBusy;
  IoTaskSnapshot? get snapshot => _snapshot;
  MutationState? get state => _state;
  MutationStartRequest? get request => _lastRequest;
  Uint8List? get plan => _plan;
  MutationResult? get lastResult => _lastResult;
  MutationResult? get outcome => _outcome;
  MutationResult? get history => _history;
  BigInt get stagedBytes => _stagedBytes;
  Object? get error => _error;
  Object? get cleanupError => _cleanupError;
  bool get ready => canExecute;
  bool get uncertain =>
      _uncertain || _readUnknown || _resultLost || _ackUnknown;

  bool get _local =>
      _snapshot?.key == null &&
      _snapshot?.exit == null &&
      _snapshot?.storage == IoStoragePhase.local;
  bool get _ownsTask =>
      _key != null &&
      _sameBytes(_key, _snapshot?.key) &&
      _sameBytes(_request?.submission, _snapshot?.submission);
  bool get _canIssue =>
      !busy &&
      _trusted &&
      _ownsTask &&
      !uncertain &&
      !_stopRequested &&
      !_released &&
      !_awaitingResult &&
      _submission == null &&
      _snapshot?.exit == null &&
      _snapshot?.storage == IoStoragePhase.running &&
      _state?.delivery == MutationDelivery.consumed &&
      _state?.commandId == _commandId;
  bool get canStart =>
      !busy && _trusted && _local && _request == null && !uncertain;
  bool get canExecute =>
      _canIssue &&
      _ready &&
      !_executionAttempted &&
      _state?.reconcileRequired == false &&
      _state?.terminal == false &&
      (_history == null || _history!.phase == MutationPhase.prepared);
  bool get canQuery => _canIssue && _plan != null;
  bool get canCancelPlan => _canIssue && _plan != null && !_executionAttempted;
  bool get canRelease => _canIssue;
  bool get canStop =>
      !_cleanupBusy &&
      _trusted &&
      _ownsTask &&
      !_ackUnknown &&
      _snapshot?.exit == null &&
      _snapshot?.storage == IoStoragePhase.running;
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
  bool get canAbandon =>
      !busy &&
      _trusted &&
      _local &&
      _key == null &&
      _request != null &&
      _submission?.command == _Command.start &&
      uncertain;
  bool get canRetrySubmission =>
      !busy &&
      _trusted &&
      !_ackUnknown &&
      !_stopRequested &&
      _uncertain &&
      !_resultLost &&
      _submission != null &&
      (_submission!.command == _Command.start
          ? (_local || _ownsTask)
          : _ownsTask);
  bool get canRead =>
      !busy &&
      _trusted &&
      _ownsTask &&
      !_ackUnknown &&
      !_stopRequested &&
      !_uncertain &&
      !_resultLost &&
      _awaitingResult &&
      _commandId != null &&
      _state?.commandId == _commandId &&
      _state?.kind == _submission?.command.kind &&
      _state?.delivery == MutationDelivery.ready &&
      _snapshot?.exit == null;

  void _begin(bool allowed, String action) {
    if (busy) throw StateError('Mutation execution session is busy');
    if (!allowed) throw StateError('Mutation execution $action is unavailable');
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
    if (uncertain) _uncertain = true;
    _notify();
  }

  void _validateIo(IoTaskSnapshot value, {bool allowLocal = false}) {
    if (value.key != null) MutationTaskValidation.identity(value.key!);
    if (value.submission != null) {
      MutationTaskValidation.identity(value.submission!);
    }
    if (allowLocal &&
        value.key == null &&
        value.exit == null &&
        value.storage == IoStoragePhase.local) {
      if (value.submission != null &&
          !_sameBytes(
            value.submission,
            _request?.submission ?? _lastRequest?.submission,
          )) {
        throw const FormatException('Foreign local mutation submission');
      }
      return;
    }
    if (value.key == null ||
        !_sameBytes(value.key, _key) ||
        !_sameBytes(value.submission, _request?.submission)) {
      throw const FormatException('Mutation execution task identity changed');
    }
  }

  Future<MutationTaskReply> _send(
    _Submission submission,
  ) => switch (submission.command) {
    _Command.start => mutations.startSelected(_request!),
    _Command.buildPlan => mutations.submitBuildPlan(
      _key!,
      submission.token,
      operationId: _operationId!,
      contentLength: _contentLength!,
      contentSha256: _contentSha256,
    ),
    _Command.prepare => mutations.submitPrepare(
      _key!,
      submission.token,
      _plan!,
    ),
    _Command.chunk => mutations.submitChunk(
      _key!,
      submission.token,
      submission.offset!,
      submission.bytes!,
    ),
    _Command.commit => mutations.submitCommitContent(_key!, submission.token),
    _Command.execute => mutations.submitExecute(_key!, submission.token),
    _Command.query => mutations.submitQuery(_key!, submission.token),
    _Command.cancelPlan => mutations.submitCancelPlan(_key!, submission.token),
    _Command.release => mutations.submitRelease(_key!, submission.token),
  };

  /// Observe an in-flight call for at most one command deadline. Stop wins the
  /// race without accepting a later receipt as authority to advance the chain.
  Future<T?> _interruptible<T>(Future<T> operation) async {
    final value = await Future.any<Object?>([
      operation.then<Object?>((result) => result),
      _stopSignal.future.then<Object?>((_) => const _Stopped()),
    ]).timeout(const Duration(seconds: 15));
    return value is _Stopped ? null : value as T;
  }

  void _accept(MutationTaskReply reply, _Submission submitted) {
    MutationTaskValidation.commandId(reply.commandId);
    MutationTaskValidation.commandId(reply.state.commandId);
    final start = submitted.command == _Command.start;
    if (start) {
      final key = reply.io.key;
      if (key == null ||
          !_sameBytes(reply.io.submission, _request?.submission) ||
          (_key != null && !_sameBytes(_key, key))) {
        throw const FormatException(
          'Mutation execution start identity changed',
        );
      }
      // Preserve the original key even if later reply fields are malformed.
      // A status with the same submission but another key cannot be adopted.
      _key ??= MutationTaskValidation.identity(key);
    }
    final expected = start ? BigInt.one : submitted.previous + BigInt.one;
    if (reply.commandId != expected || reply.state.commandId < expected) {
      throw const FormatException('Mutation execution command changed');
    }
    if (reply.state.commandId == expected &&
        reply.state.kind != submitted.command.kind) {
      throw const FormatException('Mutation execution command kind changed');
    }
    _validateIo(reply.io);
    _snapshot = reply.io;
    _state = reply.state;
    _trusted = true;
    _uncertain = false;
    _readUnknown = false;
    _commandId = expected;
    _awaitingResult = true;
    if (reply.state.commandId != expected ||
        reply.state.delivery == MutationDelivery.consumed) {
      _resultLost = true;
      _awaitingResult = false;
      _record(
        StateError('Mutation execution result was already consumed'),
        uncertain: true,
      );
    }
  }

  Future<bool> _submit(_Submission submitted) async {
    if (_stopRequested) return false;
    _submission = submitted; // Retain the exact token and payload before send.
    _uncertain = true;
    _awaitingResult = false;
    _readUnknown = false;
    _resultLost = false;
    try {
      final reply = await _interruptible(_send(submitted));
      if (reply == null || _stopRequested) {
        _record(
          StateError('Mutation stopped with a pending submission'),
          uncertain: true,
        );
        return false;
      }
      _accept(reply, submitted);
      return !_resultLost;
    } catch (error) {
      _record(error, uncertain: true);
      return false;
    }
  }

  void _acceptRead(MutationTaskRead value) {
    _validateIo(value.reply.io);
    if (value.reply.commandId != _commandId ||
        value.reply.state.commandId != _commandId ||
        value.reply.state.kind != _submission?.command.kind) {
      throw const FormatException('Mutation execution read identity changed');
    }
    _snapshot = value.reply.io;
    _state = value.reply.state;
    _trusted = true;
    if (value.result == null) {
      if (value.reply.state.delivery == MutationDelivery.consumed) {
        throw StateError(
          'Mutation execution result was consumed without a reply',
        );
      }
      return;
    }
    if (value.reply.state.delivery != MutationDelivery.consumed) {
      throw const FormatException('Mutation execution result was not consumed');
    }
    final result = value.result!;
    if ((result.kind == MutationResultKind.prepared ||
            result.kind == MutationResultKind.created ||
            result.kind == MutationResultKind.deleted) &&
        result.operationId != _operationId) {
      throw const FormatException('Mutation operation identity changed');
    }
    if (result.kind == MutationResultKind.history &&
        result.phase != MutationPhase.none &&
        result.operationId != _operationId) {
      throw const FormatException('Mutation history identity changed');
    }
    if (result.kind == MutationResultKind.failure) {
      _record(
        StateError(
          'Mutation execution command failed: ${result.failure?.target ?? result.failure?.delivery}',
        ),
      );
    }
    _lastResult = result;
    if (result.kind == MutationResultKind.created ||
        result.kind == MutationResultKind.deleted) {
      _outcome = result;
    } else if (result.kind == MutationResultKind.history) {
      _history = result;
    }
    _stagedBytes = result.stagedBytes > _stagedBytes
        ? result.stagedBytes
        : _stagedBytes;
    _awaitingResult = false;
    _submission = null;
    _readUnknown = false;
    _notify();
  }

  Future<MutationResult?> _waitResult() async {
    final timer = Stopwatch()..start();
    while (timer.elapsed < const Duration(seconds: 15)) {
      if (_stopRequested) {
        _record(
          StateError('Mutation stopped with a pending result'),
          uncertain: true,
        );
        return null;
      }
      try {
        final remaining = const Duration(seconds: 15) - timer.elapsed;
        if (remaining <= Duration.zero) break;
        final value = await _interruptible(
          mutations.read(_key!, _commandId!).timeout(remaining),
        );
        if (value == null || _stopRequested) {
          _readUnknown = true;
          _record(
            StateError('Mutation stopped during receipt read'),
            uncertain: true,
          );
          return null;
        }
        _acceptRead(value);
        if (value.result != null) return value.result;
      } catch (error) {
        _readUnknown = true;
        _record(error, uncertain: true);
        return null;
      }
      await Future.any<void>([
        Future<void>.delayed(const Duration(milliseconds: 10)),
        _stopSignal.future,
      ]);
    }
    _record(
      TimeoutException('Mutation execution receipt deadline'),
      uncertain: true,
    );
    return null;
  }

  Future<MutationResult?> _command(
    _Command command, {
    BigInt? offset,
    Uint8List? bytes,
  }) async {
    if (_stopRequested) return null;
    final submitted = _Submission(
      command,
      _newToken(),
      _commandId!,
      offset: offset,
      bytes: bytes,
    );
    if (!await _submit(submitted)) return null;
    return _waitResult();
  }

  bool _expect(MutationResult? value, MutationResultKind kind) {
    if (value == null) return false;
    if (value.kind != kind) {
      _record(
        StateError('Unexpected mutation ${value.kind}; expected $kind'),
        uncertain: value.kind != MutationResultKind.failure,
      );
      return false;
    }
    if ((kind == MutationResultKind.prepared ||
            kind == MutationResultKind.created ||
            kind == MutationResultKind.deleted) &&
        value.operationId != _operationId) {
      _record(
        const FormatException('Mutation operation identity changed'),
        uncertain: true,
      );
      return false;
    }
    if (kind == MutationResultKind.prepared &&
        value.phase != MutationPhase.prepared) {
      _record(
        const FormatException('Mutation preparation phase changed'),
        uncertain: true,
      );
      return false;
    }
    return true;
  }

  /// Observe current owner state. This never sends a mutation command.
  Future<void> refresh() async {
    _begin(true, 'refresh');
    try {
      final current = await io.ioStatus();
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
        if (!_sameBytes(current.submission, _request!.submission)) {
          throw const FormatException('Foreign mutation task');
        }
        _key = MutationTaskValidation.identity(current.key!);
      }
      _validateIo(current);
      _snapshot = current;
      _trusted = true;
      if (_ackUnknown &&
          current.exit != null &&
          current.storage == IoStoragePhase.reclaimed) {
        _ackUnknown = false; // Explicit ACK may be retried after observation.
      }
      try {
        final status = await mutations.status(_key!);
        if (status.commandId != BigInt.zero) {
          throw const FormatException('Mutation status contains a receipt');
        }
        _validateIo(status.io);
        if (_commandId != null && status.state.commandId < _commandId!) {
          throw const FormatException('Mutation command regressed');
        }
        _snapshot = status.io;
        _state = status.state;
        if (status.state.reconcileRequired || status.state.terminal) {
          _ready = false;
        }
        if (_commandId != null &&
            status.state.commandId == _commandId &&
            _submission != null &&
            status.state.kind != _submission!.command.kind) {
          throw const FormatException('Mutation status kind changed');
        }
        if (_awaitingResult &&
            status.state.commandId == _commandId &&
            status.state.delivery == MutationDelivery.consumed) {
          _resultLost = true;
          _awaitingResult = false;
          _record(
            StateError('Mutation execution result was consumed'),
            uncertain: true,
          );
        }
      } catch (error) {
        _state =
            null; // A matching cleanup-only task can still stop/repair/ack.
        _record(error, cleanup: true);
      }
    } catch (error) {
      _trusted = false;
      _record(error, cleanup: true);
    } finally {
      _end();
    }
  }

  Future<void> prepare(
    MutationStartRequest request, {
    required String operationId,
    Uint8List? content,
  }) async {
    if (busy) throw StateError('Mutation execution session is busy');
    if (!canStart) throw StateError('Mutation execution start is unavailable');
    MutationTaskValidation.validateStart(request);
    if (request.disposition == MutationDisposition.replace ||
        (request.disposition == MutationDisposition.delete &&
            content != null) ||
        (content?.length ?? 0) > MutationTaskValidation.maxContentBytes) {
      throw const FormatException(
        'Unsupported mutation content or disposition',
      );
    }
    final ownedContent = request.disposition == MutationDisposition.create
        ? Uint8List.fromList(content ?? Uint8List(0))
        : Uint8List(0);
    final length = BigInt.from(ownedContent.length);
    final digest = request.disposition == MutationDisposition.create
        ? Uint8List.fromList(sha256.convert(ownedContent).bytes)
        : null;
    MutationTaskValidation.buildPlan(operationId, length, digest);
    _begin(true, 'prepare');
    _stopSignal = Completer<void>();
    _stopRequested = false;
    _error = null; // Only a new explicit attempt resets the original error.
    _cleanupError = null;
    _request = request;
    _lastRequest = request;
    _operationId = operationId;
    _contentLength = length;
    _contentSha256 = digest;
    _plan = null;
    _lastResult = null;
    _outcome = null;
    _history = null;
    _stagedBytes = BigInt.zero;
    _ready = false;
    _executionAttempted = false;
    _released = false;
    try {
      if (!await _submit(
        _Submission(_Command.start, request.submission, BigInt.zero),
      )) {
        return;
      }
      if (!_expect(await _waitResult(), MutationResultKind.selected)) return;
      final planned = await _command(_Command.buildPlan);
      if (!_expect(planned, MutationResultKind.planned)) return;
      final plan = planned!.plan;
      if (plan == null) {
        _record(
          const FormatException('Mutation owner omitted its canonical plan'),
          uncertain: true,
        );
        return;
      }
      MutationTaskValidation.plan(plan);
      _plan = plan;
      if (!_expect(
        await _command(_Command.prepare),
        MutationResultKind.prepared,
      )) {
        return;
      }
      if (request.disposition == MutationDisposition.create) {
        for (
          var offset = 0;
          offset < ownedContent.length;
          offset += MutationTaskValidation.maxChunkBytes
        ) {
          final end =
              (offset + MutationTaskValidation.maxChunkBytes <
                  ownedContent.length)
              ? offset + MutationTaskValidation.maxChunkBytes
              : ownedContent.length;
          final chunk = Uint8List.fromList(ownedContent.sublist(offset, end));
          final staged = await _command(
            _Command.chunk,
            offset: BigInt.from(offset),
            bytes: chunk,
          );
          if (!_expect(staged, MutationResultKind.staged)) return;
          if (staged!.stagedBytes != BigInt.from(end)) {
            _record(
              const FormatException('Mutation staged byte count changed'),
              uncertain: true,
            );
            return;
          }
        }
        final committed = await _command(_Command.commit);
        if (!_expect(committed, MutationResultKind.staged)) return;
        if (!committed!.durableContent || committed.stagedBytes != length) {
          _record(
            const FormatException('Mutation content was not durably committed'),
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
      ownedContent.fillRange(0, ownedContent.length, 0);
      _end();
    }
  }

  Future<void> execute() async {
    _begin(canExecute, 'execute');
    _executionAttempted = true; // Never infer a failed send was not dispatched.
    _ready = false;
    try {
      final result = await _command(_Command.execute);
      final expected = _request!.disposition == MutationDisposition.create
          ? MutationResultKind.created
          : MutationResultKind.deleted;
      if (!_expect(result, expected)) return;
      if (result!.phase != MutationPhase.observed ||
          result.effect == MutationEffect.unspecified) {
        _record(
          const FormatException('Mutation effect proof is unavailable'),
          uncertain: true,
        );
      }
    } finally {
      _end();
    }
  }

  Future<void> query() async {
    _begin(canQuery, 'query');
    try {
      final result = await _command(_Command.query);
      if (_expect(result, MutationResultKind.history) &&
          (result!.phase != MutationPhase.prepared ||
              (_request!.disposition == MutationDisposition.create &&
                  (!result.durableContent ||
                      result.stagedBytes != _contentLength)) ||
              _state?.reconcileRequired == true ||
              _state?.terminal == true)) {
        _ready = false;
      }
    } finally {
      _end();
    }
  }

  Future<void> cancelPlan() async {
    _begin(canCancelPlan, 'cancel plan');
    _ready = false;
    try {
      _expect(
        await _command(_Command.cancelPlan),
        MutationResultKind.planCancelled,
      );
    } finally {
      _end();
    }
  }

  Future<void> release() async {
    _begin(canRelease, 'release');
    try {
      if (_expect(
        await _command(_Command.release),
        MutationResultKind.released,
      )) {
        _released = true;
        _ready = false;
      }
    } finally {
      _end();
    }
  }

  /// Exact-token retry only; it never resumes a preparation chain or execute.
  Future<void> retrySubmission() async {
    _begin(canRetrySubmission, 'retry submission');
    try {
      final submitted = _submission!;
      try {
        final reply = await _interruptible(_send(submitted));
        if (reply == null || _stopRequested) {
          _record(
            StateError('Mutation stopped with a pending submission retry'),
            uncertain: true,
          );
          return;
        }
        _accept(reply, submitted);
      } catch (error) {
        _record(error, uncertain: true);
      }
    } finally {
      _end();
    }
  }

  /// Explicitly collect one known ready receipt after a lost delivery.
  Future<void> read() async {
    _begin(canRead, 'read');
    try {
      await _waitResult();
    } finally {
      _end();
    }
  }

  Future<void> stop() async {
    if (!canStop) throw StateError('Mutation execution stop is unavailable');
    _cleanupBusy = true;
    _stopRequested = true; // Intent blocks all effects before awaiting IO.
    _ready = false;
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
    _snapshot = local;
    _state = null;
    _request = null;
    _key = null;
    _commandId = null;
    _submission = null;
    _operationId = null;
    _contentLength = null;
    _contentSha256 = null;
    _ready = false;
    _trusted = true;
    _uncertain = false;
    _ackUnknown = false;
    _stopRequested = false;
    _awaitingResult = false;
    _readUnknown = false;
    _resultLost = false;
    _executionAttempted = false;
    _released = false;
    // Plan, result and original error are intentionally retained for the view.
  }

  Future<void> acknowledge() async {
    _begin(canAcknowledge, 'acknowledge');
    _ackUnknown = true;
    try {
      final value = await io.acknowledgeIo(_key!);
      if (value.key != null ||
          value.exit != null ||
          value.storage != IoStoragePhase.local) {
        throw const FormatException('Mutation owner was not acknowledged');
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

  /// Discard an unresolved admission only after current IO proves no task.
  Future<void> abandon() async {
    _begin(canAbandon, 'abandon');
    try {
      _resetLocal(_snapshot!);
    } finally {
      _end();
    }
  }
}
