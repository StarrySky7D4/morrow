import 'dart:async';
import 'dart:typed_data';

import 'package:flutter/foundation.dart';

import 'channel_task_models.dart';

/// Tracks one explicit review while the original workbench waits for card edits.
/// A failed start or claim is never resubmitted. Only status may be polled.
final class ChangesPageWait extends ChangeNotifier {
  ChangesPageWait(this.backend, this.runApproved);
  final ChangesWaitBackend backend;
  final Future<void> Function(ChannelPrepareRequest) runApproved;
  ChannelPrepareRequest? original;
  ChangesWaitPhase? phase;
  Object? failure;
  bool busy = false;
  int _generation = 0;
  bool _disposed = false;
  bool _cancelRequested = false;
  Timer? _timer;

  Future<void> start(ChannelPrepareRequest request) async {
    if (original != null || request.changes == null) {
      throw StateError(
        'A changes wait is already active or has no reviewed cards',
      );
    }
    original = request;
    _cancelRequested = false;
    phase = null;
    failure = null;
    await _exchange(request, start: true);
  }

  Future<void> check({bool cancel = false}) async {
    final request = original;
    if (request == null || (busy && !cancel)) return;
    if (cancel) _cancelRequested = true;
    await _exchange(request, cancel: cancel);
  }

  Future<void> _exchange(
    ChannelPrepareRequest request, {
    bool start = false,
    bool cancel = false,
  }) async {
    final generation = ++_generation;
    _timer?.cancel();
    busy = true;
    notifyListeners();
    try {
      final reply = start
          ? await backend.startChangesWait(request)
          : await backend.changesWaitStatus(request.submission, cancel: cancel);
      if (_disposed || generation != _generation) return;
      phase = reply.phase;
      failure = null;
      if (_cancelRequested &&
          (reply.phase == ChangesWaitPhase.waiting ||
              reply.phase == ChangesWaitPhase.available)) {
        phase = ChangesWaitPhase.unconfirmed;
        return;
      }
      switch (reply.phase) {
        case ChangesWaitPhase.waiting:
          _timer = Timer(
            const Duration(milliseconds: 350),
            () => unawaited(check()),
          );
        case ChangesWaitPhase.available:
          original = null;
          await runApproved(request);
        case ChangesWaitPhase.expired:
        case ChangesWaitPhase.changed:
        case ChangesWaitPhase.cancelled:
          original = null;
        case ChangesWaitPhase.prepared:
        case ChangesWaitPhase.unconfirmed:
          // The status cannot justify a second admission.
          break;
      }
    } catch (error) {
      if (!_disposed && generation == _generation) failure = error;
    } finally {
      if (!_disposed && generation == _generation) {
        busy = false;
        notifyListeners();
      }
    }
  }

  @override
  void dispose() {
    _disposed = true;
    _generation++;
    _timer?.cancel();
    final key = original?.submission;
    if (key != null) unawaited(_cancelOriginal(key));
    super.dispose();
  }

  Future<void> _cancelOriginal(Uint8List key) async {
    try {
      await backend.changesWaitStatus(key, cancel: true);
    } catch (_) {
      // With the view gone there is no caller to claim the bounded wait.
    }
  }
}
