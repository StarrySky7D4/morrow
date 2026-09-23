import 'dart:async';
import 'package:flutter/foundation.dart';

enum SessionPhase {
  idle,
  opening,
  active,
  closing,
  closingUnconfirmed,
  processExited,
  recoveryRequired,
}

/// The actual process exit observation is separate from closing RPCs and work.
/// Timeouts only change presentation; they never release the old owner.
class SessionCoordinator extends ChangeNotifier {
  SessionCoordinator({this.interactionDeadline = const Duration(seconds: 3)});
  final Duration interactionDeadline;
  SessionPhase phase = SessionPhase.idle;
  Object? owner;
  String? libraryLocation;
  int generation = 0;
  int? exitCode;
  Object? closeError;
  Future<void>? _flight, _close;
  Future<void> Function()? _requestClose;
  Timer? _deadline;
  bool _disposed = false;
  bool get mayRecover => owner == null || exitCode != null;
  bool get waitingForExit => !mayRecover && _close != null;
  void _changed() {
    if (!_disposed) notifyListeners();
  }

  Future<void> run(Future<void> Function() action) {
    if (_flight case final pending?) return pending;
    if (!mayRecover) {
      return Future.error(StateError('Previous library owner has not exited'));
    }
    final completion = Completer<void>();
    _flight = completion.future;
    phase = SessionPhase.opening;
    generation++;
    owner = null;
    exitCode = null;
    _close = null;
    closeError = null;
    _deadline?.cancel();
    _changed();
    Future.sync(action).then(
      (_) {
        _flight = null;
        if (owner == null) phase = SessionPhase.idle;
        _changed();
        completion.complete();
      },
      onError: (Object error, StackTrace stack) {
        _flight = null;
        if (owner == null) phase = SessionPhase.idle;
        _changed();
        completion.completeError(error, stack);
      },
    );
    return completion.future;
  }

  void attach({
    required Object process,
    required String library,
    required Future<int> exited,
    required Future<void> Function() close,
  }) {
    if (owner != null) {
      throw StateError('A library owner is already registered');
    }
    owner = process;
    libraryLocation = library;
    _requestClose = close;
    final epoch = generation;
    exited.then(
      (code) {
        if (generation != epoch || !identical(owner, process)) return;
        exitCode = code;
        _deadline?.cancel();
        phase = SessionPhase.processExited;
        _changed();
        // Reopening still performs the host's real identity/lock/audit checks.
        // No result here confirms or clears any business operation.
        if (generation == epoch) {
          phase = SessionPhase.recoveryRequired;
          _changed();
        }
      },
      onError: (Object error) {
        if (generation != epoch || !identical(owner, process)) return;
        closeError = error;
        phase = SessionPhase.closingUnconfirmed;
        _changed();
      },
    );
    _changed();
  }

  void active() {
    if (exitCode == null && _close == null) {
      phase = SessionPhase.active;
      _changed();
    }
  }

  Future<void> close() {
    if (_close case final pending?) return pending;
    if (owner == null) return Future.value();
    final epoch = generation, process = owner;
    phase = exitCode == null
        ? SessionPhase.closing
        : SessionPhase.recoveryRequired;
    final completion = Completer<void>();
    _close = completion.future;
    // Always observe failure, even if the UI elects not to await close.
    unawaited(completion.future.catchError((Object _) {}));
    _deadline = Timer(interactionDeadline, () {
      if (generation == epoch &&
          identical(owner, process) &&
          exitCode == null) {
        phase = SessionPhase.closingUnconfirmed;
        _changed();
      }
    });
    _changed();
    Future.sync(_requestClose!).then(
      completion.complete,
      onError: (Object error, StackTrace stack) {
        if (generation == epoch && identical(owner, process)) {
          closeError = error;
          _changed();
        }
        completion.completeError(error, stack);
      },
    );
    return completion.future;
  }

  @override
  void dispose() {
    _disposed = true;
    _deadline?.cancel();
    super.dispose();
  }
}
