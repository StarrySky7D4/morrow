import 'dart:async';
import 'package:flutter/foundation.dart';
import 'channel_task_models.dart';

/// Retained by the original backend across view dismissal. Never replays a task.
final class ChannelTaskSession extends ChangeNotifier {
  ChannelTaskSession._(this.backend);
  static final _sessions = Expando<ChannelTaskSession>(
    'local channel sessions',
  );
  static ChannelTaskSession forBackend(ChannelTaskBackend backend) =>
      _sessions[backend] ??= ChannelTaskSession._(backend);
  final ChannelTaskBackend backend;
  ChannelPrepareRequest? request;
  ChannelTaskSnapshot? snapshot;
  ChannelTaskSnapshot? observedTaskResult;
  Object? failure;
  bool uncertain = false, busy = false;
  int _generation = 0;
  int get generation => _generation;
  int? _refreshGeneration;
  Timer? _timer;
  DateTime? _pollUntil;
  bool get _cleanupConfirmed =>
      snapshot != null &&
      snapshot!.resourceReclaimed &&
      snapshot!.phase == ChannelJobPhase.closed &&
      !snapshot!.snapshotPending;
  bool get canPrepare =>
      !busy && !uncertain && (snapshot == null || _cleanupConfirmed);
  void _accept(ChannelTaskSnapshot next) {
    final old = snapshot;
    if (old != null &&
        (!ChannelValidation.same(old.key, next.key) ||
            !ChannelValidation.same(old.submission, next.submission) ||
            !ChannelValidation.same(old.directory.wire, next.directory.wire) ||
            next.lastAcked < old.lastAcked ||
            next.acceptedSequence < old.acceptedSequence ||
            next.observedSequence < old.observedSequence ||
            next.uploadedFrames < old.uploadedFrames ||
            next.uploadedBytes < old.uploadedBytes ||
            next.sourceFrames < old.sourceFrames ||
            next.sourceBytes < old.sourceBytes ||
            next.observedBytes < old.observedBytes ||
            (old.inputSha256.isNotEmpty &&
                !ChannelValidation.same(old.inputSha256, next.inputSha256)) ||
            (old.taskState != ChannelTaskState.pending &&
                next.taskState != old.taskState &&
                next.taskState != ChannelTaskState.unknown) ||
            (old.taskState != ChannelTaskState.pending &&
                old.taskState != ChannelTaskState.unknown &&
                next.taskState == old.taskState &&
                (!ChannelValidation.same(old.output, next.output) ||
                    old.outputType != next.outputType ||
                    old.taskError != next.taskError)) ||
            (old.producerOutcome == ChannelProducerOutcome.unknown &&
                next.producerOutcome != ChannelProducerOutcome.unknown) ||
            (old.resourceReclaimed && !next.resourceReclaimed) ||
            (old.workerJoined && !next.workerJoined) ||
            (old.closeRequested && !next.closeRequested))) {
      throw const FormatException('Local channel binding or progress changed');
    }
    if (observedTaskResult == null &&
        next.taskState != ChannelTaskState.pending) {
      observedTaskResult = next;
    }
    snapshot = next;
  }

  Future<void> prepareAndRun(
    ChannelPrepareRequest value,
    List<ChannelSourceFrame> frames,
  ) async {
    // Own the collection before validation or any listener can mutate it. Each
    // frame already owns immutable original bytes and cursor bytes.
    final sourceFrames = List<ChannelSourceFrame>.unmodifiable(frames);
    if (!canPrepare) {
      throw StateError('The previous local channel still needs confirmation');
    }
    if (sourceFrames.length != value.frameCount ||
        sourceFrames.fold<BigInt>(
              BigInt.zero,
              (sum, frame) => sum + BigInt.from(frame.bytes.length),
            ) !=
            value.totalBytes) {
      throw const FormatException(
        'Source frames do not match the immutable preparation',
      );
    }
    for (var i = 0; i < sourceFrames.length; i++) {
      if (sourceFrames[i].sequence != BigInt.from(i + 1) ||
          sourceFrames[i].bytes.length > value.budget.maxFrameBytes ||
          (value.kind == ChannelSourceKind.byteStream &&
              sourceFrames[i].cursor.isNotEmpty)) {
        throw const FormatException(
          'Source frame exceeds its sequence, cursor rule or grant ceiling',
        );
      }
    }
    final generation = ++_generation;
    _timer?.cancel();
    _timer = null;
    _pollUntil = null;
    _refreshGeneration = null;
    request = value;
    snapshot = null;
    observedTaskResult = null;
    failure = null;
    uncertain = false;
    busy = true;
    notifyListeners();
    try {
      final prepared = await backend.prepareChannel(value);
      if (generation != _generation) return;
      _accept(prepared);
      notifyListeners();
      for (final frame in sourceFrames) {
        if (generation != _generation) return;
        if (snapshot!.closeRequested) {
          throw StateError('Channel was closed during upload');
        }
        final appended = await backend.appendChannel(snapshot!.key, frame);
        if (generation != _generation) return;
        _accept(appended);
        notifyListeners();
      }
      if (generation != _generation) return;
      if (snapshot!.closeRequested) {
        throw StateError('Channel was closed before run');
      }
      // This entry point deliberately accepts only the declared Directory input.
      final running = await backend.runChannel(
        snapshot!.key,
        snapshot!.directory.wire,
      );
      if (generation != _generation) return;
      _accept(running);
      _pollUntil = DateTime.now().add(
        Duration(milliseconds: value.lifetimeMs + 10000),
      );
      _startPolling(generation);
    } catch (error) {
      if (generation != _generation) return;
      failure = error;
      uncertain = true;
      // Keep the key, epoch and raw snapshot for explicit status/close. A lost
      // preparation reply has no guessed key and cannot trigger another grant.
    } finally {
      if (generation == _generation) {
        busy = false;
        notifyListeners();
      }
    }
  }

  Future<void> refresh() async {
    final generation = _generation;
    final current = snapshot;
    if (current == null || _refreshGeneration == generation) {
      return;
    }
    _refreshGeneration = generation;
    try {
      final updated = await backend.statusChannel(current.key);
      if (generation != _generation) return;
      _accept(updated);
      failure = null;
      uncertain = false;
      if (_cleanupConfirmed) {
        _timer?.cancel();
      }
    } catch (error) {
      if (generation != _generation) return;
      failure = error;
      uncertain = true;
      _timer?.cancel();
    } finally {
      // A reply for a reclaimed generation cannot unlock another generation's
      // pending refresh or overwrite its state through a listener.
      if (_refreshGeneration == generation) _refreshGeneration = null;
      if (generation == _generation) notifyListeners();
    }
  }

  Future<void> close() async {
    final generation = _generation;
    final current = snapshot;
    if (current == null) {
      return;
    }
    try {
      final closed = await backend.closeChannel(current.key);
      if (generation != _generation) return;
      _accept(closed);
      failure = null;
      uncertain = false;
      _pollUntil = DateTime.now().add(const Duration(seconds: 10));
      _startPolling(generation);
    } catch (error) {
      if (generation != _generation) return;
      failure = error;
      uncertain = true;
    }
    if (generation == _generation) notifyListeners();
  }

  void _startPolling(int generation) {
    if (generation != _generation) return;
    _timer?.cancel();
    _timer = Timer.periodic(const Duration(milliseconds: 250), (timer) {
      if (generation != _generation) {
        timer.cancel();
        return;
      }
      if (_pollUntil != null && DateTime.now().isAfter(_pollUntil!)) {
        timer.cancel();
        uncertain = true;
        notifyListeners();
        return;
      }
      unawaited(refresh());
    });
  }
}
