import 'dart:typed_data';

import 'io_task_models.dart';
import 'mutation_task_models.dart';

bool _same(List<int>? a, List<int>? b) =>
    a != null &&
    b != null &&
    a.length == b.length &&
    Iterable<int>.generate(a.length).every((i) => a[i] == b[i]);

enum MutationRecoveryPhase {
  idle,
  discovering,
  reconciling,
  resultLost,
  unknown,
  stopping,
  failed,
}

enum MutationRecoveryNotice {
  status,
  startUnknown,
  submissionUnknown,
  readUnknown,
  resultLost,
  controlUnknown,
  identity,
  interrupted,
}

enum _SubmissionKind { discoverStart, reconcileStart, next, release }

final class _Submission {
  _Submission.startDiscovery(this.discovery)
    : kind = _SubmissionKind.discoverStart,
      reconciliation = null,
      token = discovery!.submission,
      scanLimit = null,
      previous = BigInt.zero;
  _Submission.startReconciliation(this.reconciliation)
    : kind = _SubmissionKind.reconcileStart,
      discovery = null,
      token = reconciliation!.submission,
      scanLimit = null,
      previous = BigInt.zero;
  _Submission.command(this.kind, Uint8List token, this.previous, this.scanLimit)
    : discovery = null,
      reconciliation = null,
      token = MutationTaskValidation.identity(token);

  final _SubmissionKind kind;
  final MutationDiscoverRequest? discovery;
  final MutationReconcileRequest? reconciliation;
  final Uint8List token;
  final int? scanLimit;
  final BigInt previous;
  MutationCommandKind get commandKind => switch (kind) {
    _SubmissionKind.discoverStart ||
    _SubmissionKind.next => MutationCommandKind.discover,
    _SubmissionKind.reconcileStart => MutationCommandKind.reconcile,
    _SubmissionKind.release => MutationCommandKind.release,
  };
}

/// One bounded, read-only recovery flow. The host remains authoritative for
/// history, worker state, and all cleanup. No timer, effect, or automatic retry.
final class MutationRecoverySession {
  MutationRecoverySession(this.mutations, this.io);

  static final _sessions = Expando<Expando<MutationRecoverySession>>(
    'mutation recovery sessions',
  );
  factory MutationRecoverySession.forBackend(
    MutationTaskBackend mutations,
    WorkbenchIoTaskControl io,
  ) {
    final pair = _sessions[mutations] ??= Expando<MutationRecoverySession>(
      'mutation recovery IO',
    );
    return pair[io] ??= MutationRecoverySession(mutations, io);
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
        // A view callback cannot turn a successful host action into uncertainty.
      }
    }
  }

  bool _busy = false;
  bool _trusted = false;
  bool _ackUnknown = false;
  bool _stopRequested = false;
  bool _unknownSubmission = false;
  bool _readUnknown = false;
  bool _awaitingResult = false;
  bool _resultLost = false;
  IoTaskSnapshot? _snapshot;
  MutationState? _state;
  Uint8List? _key;
  BigInt? _commandId;
  _Submission? _submission;
  MutationDiscoverRequest? _discoveryRequest;
  MutationReconcileRequest? _reconcileRequest;
  MutationResult? _page;
  MutationResult? _reconciliation;
  MutationRecoveryPhase _phase = MutationRecoveryPhase.idle;
  MutationRecoveryNotice? _notice;

  bool get busy => _busy;
  IoTaskSnapshot? get snapshot => _snapshot;
  MutationState? get state => _state;
  MutationRecoveryPhase get phase => _phase;
  MutationRecoveryNotice? get notice => _notice;
  MutationDiscoverRequest? get discoveryRequest => _discoveryRequest;
  MutationReconcileRequest? get reconcileRequest => _reconcileRequest;
  MutationResult? get page => _page;

  /// Last delivered page cursor, retained through an uncertain Next/Release.
  /// A caller may explicitly start a new authorized discovery with it while
  /// the issuing host session still exists; no continuation is automatic.
  Uint8List? get checkpoint => _page?.checkpoint;
  MutationResult? get reconciliation => _reconciliation;
  bool get _local =>
      _snapshot?.key == null &&
      _snapshot?.exit == null &&
      _snapshot?.storage == IoStoragePhase.local;
  bool get _ownsTask =>
      _key != null &&
      _same(_key, _snapshot?.key) &&
      _same(_attemptSubmission, _snapshot?.submission);
  Uint8List? get _attemptSubmission =>
      _discoveryRequest?.submission ?? _reconcileRequest?.submission;
  bool get canStart =>
      !_busy &&
      _trusted &&
      _local &&
      _discoveryRequest == null &&
      _reconcileRequest == null;
  bool get canAbandon =>
      !_busy &&
      _trusted &&
      _local &&
      _key == null &&
      _unknownSubmission &&
      _submission != null &&
      _submission!.previous == BigInt.zero;
  bool get canRead =>
      !_busy &&
      _trusted &&
      _ownsTask &&
      !_ackUnknown &&
      !_stopRequested &&
      !_unknownSubmission &&
      !_resultLost &&
      _awaitingResult &&
      _commandId != null &&
      _state?.commandId == _commandId &&
      _snapshot?.exit == null &&
      _state?.delivery == MutationDelivery.ready;
  bool get canNext =>
      !_busy &&
      _trusted &&
      _ownsTask &&
      _discoveryRequest != null &&
      !_ackUnknown &&
      !_stopRequested &&
      !_unknownSubmission &&
      !_readUnknown &&
      !_awaitingResult &&
      !_resultLost &&
      _page?.kind == MutationResultKind.plans &&
      !_page!.done &&
      _snapshot?.exit == null &&
      _state?.kind == MutationCommandKind.discover &&
      _state?.commandId == _commandId &&
      _state?.terminal == false &&
      _state?.delivery == MutationDelivery.consumed &&
      _phase == MutationRecoveryPhase.discovering;
  bool get canRelease =>
      !_busy &&
      _trusted &&
      _ownsTask &&
      _discoveryRequest != null &&
      !_ackUnknown &&
      !_stopRequested &&
      !_unknownSubmission &&
      !_awaitingResult &&
      _submission?.kind != _SubmissionKind.release &&
      _snapshot?.exit == null &&
      _state?.kind == MutationCommandKind.discover &&
      _state?.commandId == _commandId &&
      _state?.delivery == MutationDelivery.consumed;
  bool get canStop =>
      !_busy &&
      _trusted &&
      _ownsTask &&
      !_ackUnknown &&
      _snapshot?.exit == null &&
      _snapshot?.storage != IoStoragePhase.stopping;
  bool get canRepair =>
      !_busy &&
      _trusted &&
      _ownsTask &&
      !_ackUnknown &&
      _snapshot?.exit != null &&
      _snapshot?.storage == IoStoragePhase.recoveryRequired;
  bool get canAcknowledge =>
      !_busy &&
      _trusted &&
      _ownsTask &&
      !_ackUnknown &&
      _snapshot?.exit != null &&
      _snapshot?.storage == IoStoragePhase.reclaimed;
  bool get canRetrySubmission =>
      !_busy &&
      _trusted &&
      !_ackUnknown &&
      !_stopRequested &&
      _unknownSubmission &&
      !_resultLost &&
      _submission != null &&
      ((_submission!.previous == BigInt.zero &&
              ((_local &&
                      (_snapshot?.submission == null ||
                          _same(_snapshot?.submission, _attemptSubmission))) ||
                  _ownsTask)) ||
          (_submission!.previous != BigInt.zero && _ownsTask));

  void _begin(bool available, String action) {
    if (_busy) {
      throw StateError('Mutation recovery session is busy');
    }
    if (!available) {
      throw StateError('Mutation recovery $action is unavailable');
    }
    _busy = true;
    _notify();
  }

  void _end() {
    _busy = false;
    _notify();
  }

  void _validateIo(
    IoTaskSnapshot value, {
    bool localAllowed = false,
    Uint8List? expectedKey,
  }) {
    if (value.key != null) MutationTaskValidation.identity(value.key!);
    if (value.submission != null) {
      MutationTaskValidation.identity(value.submission!);
    }
    if (localAllowed &&
        value.key == null &&
        value.exit == null &&
        value.storage == IoStoragePhase.local) {
      return;
    }
    if (value.key == null ||
        !_same(value.key, expectedKey ?? _key) ||
        !_same(value.submission, _attemptSubmission)) {
      throw const FormatException('Mutation recovery task identity changed');
    }
  }

  void _acceptReply(
    MutationTaskReply reply,
    _Submission submission, {
    required bool start,
  }) {
    MutationTaskValidation.commandId(reply.commandId);
    MutationTaskValidation.commandId(reply.state.commandId);
    if (start) {
      if (reply.commandId != BigInt.one ||
          reply.state.commandId < BigInt.one ||
          (reply.state.commandId == BigInt.one &&
              reply.state.kind != submission.commandKind)) {
        throw const FormatException('Mutation recovery start identity changed');
      }
      final key = reply.io.key;
      if (key == null || !_same(reply.io.submission, submission.token)) {
        throw const FormatException('Mutation recovery start task changed');
      }
      if (_key != null && !_same(_key, key)) {
        throw const FormatException('Mutation recovery start key changed');
      }
    } else if (_key == null ||
        reply.commandId != submission.previous + BigInt.one ||
        reply.state.commandId < reply.commandId ||
        (reply.state.commandId == reply.commandId &&
            reply.state.kind != submission.commandKind)) {
      throw const FormatException('Mutation recovery command identity changed');
    }
    _validateIo(reply.io, expectedKey: start ? reply.io.key : null);
    if (start) _key ??= MutationTaskValidation.identity(reply.io.key!);
    _snapshot = reply.io;
    _state = reply.state;
    _trusted = true;
    _unknownSubmission = false;
    _readUnknown = false;
    _commandId = reply.commandId;
    _awaitingResult = true;
    _notice = null;
    if (reply.state.commandId != reply.commandId ||
        reply.state.delivery == MutationDelivery.consumed) {
      _markResultLost();
    }
  }

  void _markResultLost() {
    _resultLost = true;
    _awaitingResult = false;
    _unknownSubmission = false;
    _readUnknown = false;
    _phase = MutationRecoveryPhase.resultLost;
    _notice = MutationRecoveryNotice.resultLost;
  }

  void _reset(IoTaskSnapshot local) {
    _key = null;
    _snapshot = local;
    _state = null;
    _commandId = null;
    _submission = null;
    _discoveryRequest = null;
    _reconcileRequest = null;
    _page = null;
    _reconciliation = null;
    _ackUnknown = false;
    _stopRequested = false;
    _unknownSubmission = false;
    _readUnknown = false;
    _awaitingResult = false;
    _resultLost = false;
    _trusted = true;
    _phase = MutationRecoveryPhase.idle;
    _notice = null;
  }

  void _observeStatus(MutationTaskReply reply) {
    if (reply.commandId != BigInt.zero) {
      throw const FormatException('Mutation recovery status has receipt');
    }
    _validateIo(reply.io);
    MutationTaskValidation.commandId(reply.state.commandId);
    final pending = _submission;
    if (pending != null &&
        ((_unknownSubmission &&
                reply.state.commandId == pending.previous + BigInt.one) ||
            (!_unknownSubmission && reply.state.commandId == _commandId)) &&
        reply.state.kind != pending.commandKind) {
      throw const FormatException('Mutation recovery status kind changed');
    }
    _snapshot = reply.io;
    _state = reply.state;
    _trusted = true;
    final submission = pending;
    if (submission == null) return;
    if (_unknownSubmission) {
      if (reply.state.commandId == submission.previous) {
        return; // An exact explicit retry remains available.
      }
      // Status does not carry the submission receipt. Matching IO identity
      // proves the task, but only an explicit exact-token retry proves which
      // command belongs to this session, even for the startup command.
      return;
    }
    if (_awaitingResult && _commandId != null) {
      if (reply.state.commandId != _commandId ||
          reply.state.delivery == MutationDelivery.consumed) {
        _markResultLost();
      } else if (_readUnknown) {
        _readUnknown = false;
        _notice = null;
      }
    }
  }

  /// Read-only observation. A foreign task is never adopted. A lost start can
  /// adopt only a matching submission, including a cleanup-only worker task.
  Future<void> refresh() async {
    _begin(true, 'refresh');
    try {
      final local = await io.ioStatus();
      if (_ackUnknown &&
          local.key == null &&
          local.exit == null &&
          local.storage == IoStoragePhase.local) {
        _reset(local);
        return;
      }
      if (_key == null) {
        if (_submission == null) {
          if (local.key != null ||
              local.exit != null ||
              local.storage != IoStoragePhase.local) {
            _snapshot = local;
            _trusted = false;
            _notice = MutationRecoveryNotice.identity;
            return;
          }
          _validateIo(local, localAllowed: true);
          _snapshot = local;
          _trusted = true;
          _notice = null;
          return;
        }
        if (local.key != null && _same(local.submission, _attemptSubmission)) {
          _key = MutationTaskValidation.identity(local.key!);
        } else {
          _snapshot = local;
          _trusted = _local;
          if (_trusted) _validateIo(local, localAllowed: true);
          // No task exists: a historical foreign token cannot prevent the
          // caller from discarding its own in-memory attempt. It still cannot
          // be adopted or used to retry that foreign submission.
          _notice = _trusted
              ? MutationRecoveryNotice.startUnknown
              : MutationRecoveryNotice.identity;
          return;
        }
      }
      _validateIo(local);
      _snapshot = local;
      _trusted = true;
      if (_ackUnknown &&
          local.exit != null &&
          local.storage == IoStoragePhase.reclaimed) {
        // The previous ACK did not remove this exact task. An explicit ACK
        // retry is safe; no control action is issued by refresh itself.
        _ackUnknown = false;
      }
      if (local.exit != null && _reconciliation == null && _page == null) {
        _notice ??= MutationRecoveryNotice.interrupted;
      }
      MutationTaskReply? status;
      try {
        status = await mutations.status(_key!);
      } catch (_) {
        // A matching cleanup-only task may have no mutation scheduler state.
        _state = null;
        _notice ??= MutationRecoveryNotice.status;
        _trusted = true; // Only IO cleanup gates remain available.
      }
      if (status != null) _observeStatus(status);
    } catch (_) {
      _trusted = false;
      _notice = MutationRecoveryNotice.identity;
    } finally {
      _end();
    }
  }

  Future<MutationTaskReply> _send(_Submission submission) =>
      switch (submission.kind) {
        _SubmissionKind.discoverStart => mutations.startDiscovery(
          submission.discovery!,
        ),
        _SubmissionKind.reconcileStart => mutations.startReconciliation(
          submission.reconciliation!,
        ),
        _SubmissionKind.next => mutations.submitNextPlans(
          _key!,
          submission.token,
          submission.scanLimit!,
        ),
        _SubmissionKind.release => mutations.submitRelease(
          _key!,
          submission.token,
        ),
      };

  Future<void> _submit(_Submission submission, {required bool start}) async {
    _submission = submission; // Retain exact bytes before the first await.
    _unknownSubmission = true;
    _readUnknown = false;
    _awaitingResult = false;
    _resultLost = false;
    _notice = null;
    try {
      _acceptReply(await _send(submission), submission, start: start);
    } catch (_) {
      _trusted = false;
      _phase = MutationRecoveryPhase.unknown;
      _notice = start
          ? MutationRecoveryNotice.startUnknown
          : MutationRecoveryNotice.submissionUnknown;
    }
  }

  Future<void> startDiscovery(MutationDiscoverRequest request) async {
    if (_busy) throw StateError('Mutation recovery session is busy');
    if (!canStart) throw StateError('Mutation recovery start is unavailable');
    MutationTaskValidation.validateDiscovery(request);
    _begin(true, 'start');
    try {
      _discoveryRequest = request;
      _phase = MutationRecoveryPhase.discovering;
      await _submit(_Submission.startDiscovery(request), start: true);
    } finally {
      _end();
    }
  }

  Future<void> startReconciliation(MutationReconcileRequest request) async {
    if (_busy) throw StateError('Mutation recovery session is busy');
    if (!canStart) throw StateError('Mutation recovery start is unavailable');
    MutationTaskValidation.validateReconciliation(request);
    _begin(true, 'start');
    try {
      _reconcileRequest = request;
      _phase = MutationRecoveryPhase.reconciling;
      await _submit(_Submission.startReconciliation(request), start: true);
    } finally {
      _end();
    }
  }

  Future<void> nextPage(Uint8List submission, int scanLimit) async {
    if (_busy) throw StateError('Mutation recovery session is busy');
    if (!canNext) {
      throw StateError('Mutation recovery next page is unavailable');
    }
    MutationTaskValidation.identity(submission);
    MutationTaskValidation.scanLimit(scanLimit);
    _begin(true, 'next page');
    try {
      await _submit(
        _Submission.command(
          _SubmissionKind.next,
          submission,
          _state!.commandId,
          scanLimit,
        ),
        start: false,
      );
    } finally {
      _end();
    }
  }

  Future<void> release(Uint8List submission) async {
    if (_busy) throw StateError('Mutation recovery session is busy');
    if (!canRelease) {
      throw StateError('Mutation recovery release is unavailable');
    }
    MutationTaskValidation.identity(submission);
    _begin(true, 'release');
    try {
      await _submit(
        _Submission.command(
          _SubmissionKind.release,
          submission,
          _state!.commandId,
          null,
        ),
        start: false,
      );
    } finally {
      _end();
    }
  }

  Future<void> retrySubmission() async {
    _begin(canRetrySubmission, 'retry');
    try {
      final submission = _submission!;
      _unknownSubmission = true;
      _notice = null;
      try {
        _acceptReply(
          await _send(submission),
          submission,
          start: submission.previous == BigInt.zero,
        );
      } catch (_) {
        _trusted = false;
        _phase = MutationRecoveryPhase.unknown;
        _notice = submission.previous == BigInt.zero
            ? MutationRecoveryNotice.startUnknown
            : MutationRecoveryNotice.submissionUnknown;
      }
    } finally {
      _end();
    }
  }

  /// Forget an unresolved start only after IO confirms there is no task.
  /// This never retries or starts a host action.
  Future<void> abandon() async {
    _begin(canAbandon, 'abandon');
    try {
      _reset(_snapshot!);
    } finally {
      _end();
    }
  }

  Future<void> read() async {
    _begin(canRead, 'read');
    _readUnknown = true;
    try {
      final value = await mutations.read(_key!, _commandId!);
      _validateIo(value.reply.io);
      if (value.reply.commandId != _commandId ||
          value.reply.state.commandId != _commandId ||
          value.reply.state.kind != _submission!.commandKind) {
        throw const FormatException('Mutation recovery read identity changed');
      }
      _snapshot = value.reply.io;
      _state = value.reply.state;
      _trusted = true;
      _readUnknown = false;
      if (value.result == null) {
        if (value.reply.state.delivery == MutationDelivery.consumed) {
          _markResultLost();
        }
        return;
      }
      if (value.reply.state.delivery != MutationDelivery.consumed) {
        throw const FormatException('Mutation recovery result not consumed');
      }
      final result = value.result!;
      if (result.kind != MutationResultKind.plans &&
          result.checkpoint != null) {
        throw const FormatException('Checkpoint without discovery page');
      }
      if (_discoveryRequest != null) {
        if (_submission!.kind == _SubmissionKind.release) {
          if (result.kind != MutationResultKind.released) {
            throw const FormatException('Unexpected mutation release result');
          }
          _phase = MutationRecoveryPhase.stopping;
        } else if (result.kind == MutationResultKind.plans) {
          final plans = result.plans;
          if (plans == null ||
              plans.length > MutationTaskValidation.maxPlanScanLimit ||
              result.scanned > MutationTaskValidation.maxPlanScanLimit ||
              plans.length > result.scanned ||
              (!result.done && result.scanned == 0) ||
              (result.done && result.checkpoint != null) ||
              (!result.done && result.checkpoint == null) ||
              (result.checkpoint != null &&
                  _invalidCheckpoint(result.checkpoint!)) ||
              result.done != value.reply.state.terminal ||
              plans.any(
                (p) =>
                    p.isEmpty || p.length > MutationTaskValidation.maxPlanBytes,
              )) {
            throw const FormatException('Invalid mutation recovery page');
          }
          _page = result;
          _phase = MutationRecoveryPhase.discovering;
        } else if (result.kind == MutationResultKind.failure &&
            value.reply.state.terminal &&
            !value.reply.state.reconcileRequired) {
          _phase = MutationRecoveryPhase.failed;
          _notice = MutationRecoveryNotice.interrupted;
        } else {
          throw const FormatException('Unexpected mutation discovery result');
        }
      } else if (result.kind == MutationResultKind.reconciled) {
        _reconciliation = result;
        _phase = result.phase == MutationPhase.outcomeUnknown
            ? MutationRecoveryPhase.unknown
            : MutationRecoveryPhase.reconciling;
      } else if (result.kind == MutationResultKind.failure) {
        _phase = MutationRecoveryPhase.failed;
        _notice = MutationRecoveryNotice.interrupted;
      } else {
        throw const FormatException(
          'Unexpected mutation reconciliation result',
        );
      }
      _awaitingResult = false;
      if (result.kind != MutationResultKind.failure) _notice = null;
    } catch (_) {
      _trusted = false;
      _phase = MutationRecoveryPhase.unknown;
      _notice = MutationRecoveryNotice.readUnknown;
    } finally {
      _end();
    }
  }

  bool _invalidCheckpoint(Uint8List value) {
    try {
      MutationTaskValidation.identity(value);
      return false;
    } on FormatException {
      return true;
    }
  }

  Future<void> stop() async {
    _begin(canStop, 'stop');
    _stopRequested = true;
    try {
      final value = await io.cancelIo(_key!);
      _validateIo(value);
      _snapshot = value;
      _trusted = true;
      if (_awaitingResult || _readUnknown) _markResultLost();
      _phase = MutationRecoveryPhase.stopping;
      _notice = null;
    } catch (_) {
      _trusted = false;
      _notice = MutationRecoveryNotice.controlUnknown;
    } finally {
      _end();
    }
  }

  Future<void> repair() async {
    _begin(canRepair, 'repair');
    try {
      final value = await io.repairIo(_key!);
      _validateIo(value);
      _snapshot = value;
      _trusted = true;
      _notice = null;
    } catch (_) {
      _trusted = false;
      _notice = MutationRecoveryNotice.controlUnknown;
    } finally {
      _end();
    }
  }

  Future<void> acknowledge() async {
    _begin(canAcknowledge, 'acknowledge');
    _ackUnknown = true;
    try {
      final value = await io.acknowledgeIo(_key!);
      if (value.key != null ||
          value.exit != null ||
          value.storage != IoStoragePhase.local) {
        throw const FormatException(
          'Mutation recovery acknowledgement incomplete',
        );
      }
      _reset(value);
    } catch (_) {
      _trusted = false;
      _notice = MutationRecoveryNotice.controlUnknown;
    } finally {
      _end();
    }
  }
}
