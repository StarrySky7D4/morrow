import 'dart:async';

import 'session_coordinator.dart';

/// Keeps the window responsive while the original library owner exits.
/// Only a real process exit permits destroying the window.
class ApplicationShutdown {
  ApplicationShutdown({
    required this.session,
    required this.showClosing,
    required this.hideWindow,
    required this.showWindow,
    required this.destroyWindow,
    this.backgroundReminder = const Duration(seconds: 30),
    this.hideOnRequest = false,
    this.prepareClose,
  });

  final SessionCoordinator session;
  final void Function() showClosing;
  final Future<void> Function() hideWindow;
  final Future<void> Function() showWindow;
  final Future<void> Function() destroyWindow;
  final Duration backgroundReminder;
  final bool hideOnRequest;
  final Future<void> Function()? prepareClose;
  bool _preparing = false, _prepared = false;

  bool requested = false;
  bool hidden = false;
  bool destroying = false;
  Timer? _reminder;
  bool _closingStarted = false;
  bool _revealing = false;
  Object? _reportedError;
  Object? windowError;
  Future<void> _windowOperations = Future<void>.value();

  Future<void> _enqueue(Future<void> Function() action) {
    final next = _windowOperations.then((_) async {
      try {
        await action();
      } catch (error) {
        windowError = error;
        showClosing();
        rethrow;
      }
    });
    // The caller observes the result; later window operations keep their lane.
    _windowOperations = next.then((_) {}, onError: (Object _, StackTrace _) {});
    return next;
  }

  void request() {
    if (destroying) return;
    if (requested) {
      if (session.mayRecover && session.phase != SessionPhase.opening) {
        observe();
        return;
      }
      unawaited(continueInBackground());
      return;
    }
    requested = true;
    showClosing();
    if (hideOnRequest) unawaited(continueInBackground());
    observe();
  }

  void observe() {
    if (!requested || destroying) return;
    if (session.owner == null && session.phase == SessionPhase.opening) {
      return;
    }
    if (!_prepared && prepareClose != null) {
      if (!_preparing) {
        _preparing = true;
        unawaited(
          prepareClose!().then(
            (_) {
              _prepared = true;
              observe();
            },
            onError: (Object error, StackTrace _) {
              _preparing = false;
              windowError = error;
              showClosing();
              if (hidden) unawaited(_reveal());
            },
          ),
        );
      }
      return;
    }
    if (session.mayRecover) {
      destroying = true;
      _reminder?.cancel();
      unawaited(
        _enqueue(destroyWindow).catchError((Object _, StackTrace _) {
          destroying = false;
          if (hidden) unawaited(_reveal());
        }),
      );
    } else {
      if (!_closingStarted) {
        _closingStarted = true;
        unawaited(
          session.close().then(
            (_) => observe(),
            onError: (Object _, StackTrace _) => observe(),
          ),
        );
      }
      if (hidden &&
          session.closeError != null &&
          !identical(_reportedError, session.closeError)) {
        _reportedError = session.closeError;
        unawaited(_reveal());
      }
    }
  }

  Future<void> continueInBackground() async {
    if (!requested ||
        hidden ||
        destroying ||
        (session.mayRecover && session.phase != SessionPhase.opening)) {
      return;
    }
    hidden = true;
    try {
      await _enqueue(hideWindow);
    } catch (_) {
      hidden = false;
      return;
    }
    if (destroying ||
        (session.mayRecover && session.phase != SessionPhase.opening)) {
      return;
    }
    if (session.closeError != null &&
        !identical(_reportedError, session.closeError)) {
      _reportedError = session.closeError;
      await _reveal();
      return;
    }
    _reminder?.cancel();
    _reminder = Timer(backgroundReminder, () {
      if (!destroying &&
          (!session.mayRecover || session.phase == SessionPhase.opening)) {
        unawaited(_reveal());
      }
    });
  }

  Future<void> _reveal() async {
    if (!hidden || destroying || _revealing) return;
    _revealing = true;
    _reminder?.cancel();
    try {
      // A pending hide must finish before this show is allowed to run.
      await _enqueue(showWindow);
      hidden = false;
    } catch (_) {
      _reminder = Timer(const Duration(seconds: 3), () {
        if (!destroying && hidden) unawaited(_reveal());
      });
    } finally {
      _revealing = false;
    }
  }
}
