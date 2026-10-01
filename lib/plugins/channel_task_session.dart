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
  bool uncertain = false, busy = false, _polling = false;
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
    if (!canPrepare) {
      throw StateError('The previous local channel still needs confirmation');
    }
    if (frames.length != value.frameCount ||
        frames.fold<BigInt>(
              BigInt.zero,
              (sum, frame) => sum + BigInt.from(frame.bytes.length),
            ) !=
            value.totalBytes) {
      throw const FormatException(
        'Source frames do not match the immutable preparation',
      );
    }
    for (var i = 0; i < frames.length; i++) {
      if (frames[i].sequence != BigInt.from(i + 1) ||
          frames[i].bytes.length > value.budget.maxFrameBytes ||
          (value.kind == ChannelSourceKind.byteStream &&
              frames[i].cursor.isNotEmpty)) {
        throw const FormatException(
          'Source frame exceeds its sequence, cursor rule or grant ceiling',
        );
      }
    }
    request = value;
    snapshot = null;
    observedTaskResult = null;
    failure = null;
    uncertain = false;
    busy = true;
    notifyListeners();
    try {
      _accept(await backend.prepareChannel(value));
      notifyListeners();
      for (final frame in frames) {
        if (snapshot!.closeRequested) {
          throw StateError('Channel was closed during upload');
        }
        _accept(await backend.appendChannel(snapshot!.key, frame));
        notifyListeners();
      }
      if (snapshot!.closeRequested) {
        throw StateError('Channel was closed before run');
      }
      // This entry point deliberately accepts only the declared Directory input.
      _accept(
        await backend.runChannel(snapshot!.key, snapshot!.directory.wire),
      );
      _pollUntil = DateTime.now().add(
        Duration(milliseconds: value.lifetimeMs + 10000),
      );
      _startPolling();
    } catch (error) {
      failure = error;
      uncertain = true;
      // Keep the key, epoch and raw snapshot for explicit status/close. A lost
      // preparation reply has no guessed key and cannot trigger another grant.
    } finally {
      busy = false;
      notifyListeners();
    }
  }

  Future<void> refresh() async {
    final current = snapshot;
    if (current == null || _polling) {
      return;
    }
    _polling = true;
    try {
      _accept(await backend.statusChannel(current.key));
      failure = null;
      uncertain = false;
      if (_cleanupConfirmed) {
        _timer?.cancel();
      }
    } catch (error) {
      failure = error;
      uncertain = true;
      _timer?.cancel();
    } finally {
      _polling = false;
      notifyListeners();
    }
  }

  Future<void> close() async {
    final current = snapshot;
    if (current == null) {
      return;
    }
    try {
      _accept(await backend.closeChannel(current.key));
      failure = null;
      uncertain = false;
      _pollUntil = DateTime.now().add(const Duration(seconds: 10));
      _startPolling();
    } catch (error) {
      failure = error;
      uncertain = true;
    }
    notifyListeners();
  }

  void _startPolling() {
    _timer?.cancel();
    _timer = Timer.periodic(const Duration(milliseconds: 250), (_) {
      if (_pollUntil != null && DateTime.now().isAfter(_pollUntil!)) {
        _timer?.cancel();
        uncertain = true;
        notifyListeners();
        return;
      }
      unawaited(refresh());
    });
  }
}
