import 'dart:async';
import 'dart:convert';
import 'dart:developer' show Timeline;
import 'dart:io';
import 'dart:ui' show FrameTiming, FramePhase;
import 'package:flutter/widgets.dart';

/// Opt-in, local qualification of startup milestones. Never writes library data.
/// Raster timestamps, rather than callback delivery time, exclude batching delay.
class StartupProbe {
  StartupProbe(this.output);
  final String? output;
  final int _origin = Timeline.now;
  final stages = <String, Object>{};
  final _ready = Completer<void>();
  final _revealed = Completer<void>();
  int? _workbenchFrame;
  int? _revealedFrame;
  final _transitionFrames = <Map<String, Object>>[];

  void attach() {
    if (output != null) WidgetsBinding.instance.addTimingsCallback(_frames);
  }

  void mark(String stage) {
    if (output == null) return;
    final now = Timeline.now;
    stages[stage] = (now - _origin) / 1000;
  }

  void workbenchPainted() {
    if (output == null) return;
    _workbenchFrame ??=
        WidgetsBinding.instance.platformDispatcher.frameData.frameNumber;
    mark('workbench_build');
  }

  void revealed() {
    if (output == null) return;
    _revealedFrame ??=
        WidgetsBinding.instance.platformDispatcher.frameData.frameNumber;
  }

  void _frames(List<FrameTiming> frames) {
    for (final frame in frames) {
      final finish = frame.timestampInMicroseconds(FramePhase.rasterFinish);
      stages.putIfAbsent('first_frame', () => (finish - _origin) / 1000);
      if (_workbenchFrame != null &&
          frame.frameNumber >= _workbenchFrame! &&
          !_ready.isCompleted) {
        stages['workbench_frame'] = (finish - _origin) / 1000;
        _ready.complete();
      }
      if (_workbenchFrame != null &&
          frame.frameNumber > _workbenchFrame! &&
          !_revealed.isCompleted) {
        _transitionFrames.add({
          'build_ms': frame.buildDuration.inMicroseconds / 1000,
          'raster_ms': frame.rasterDuration.inMicroseconds / 1000,
        });
      }
      if (_revealedFrame != null &&
          frame.frameNumber >= _revealedFrame! &&
          !_revealed.isCompleted) {
        stages['revealed_frame'] = (finish - _origin) / 1000;
        stages['transition_frames'] = _transitionFrames;
        _revealed.complete();
      }
    }
  }

  Future<void> finish() async {
    if (output == null) return;
    try {
      await Future.wait([
        _ready.future,
        _revealed.future,
      ]).timeout(const Duration(seconds: 30));
      await File(output!).writeAsString(jsonEncode(stages));
    } finally {
      WidgetsBinding.instance.removeTimingsCallback(_frames);
    }
  }

  Future<void> fail(Object error) async {
    if (output == null) return;
    WidgetsBinding.instance.removeTimingsCallback(_frames);
    await File(
      output!,
    ).writeAsString(jsonEncode({...stages, 'error': error.toString()}));
  }
}
