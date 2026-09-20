import 'package:flutter/scheduler.dart';
import 'package:flutter/widgets.dart';

/// Shared sessions can notify an existing panel while a sibling is mounting.
/// Defer only repaint notification; commands and ownership are never deferred.
mixin SessionViewState<T extends StatefulWidget> on State<T> {
  bool _sessionPaintQueued = false;

  void markSessionViewDirty() {
    if (!mounted) return;
    if (SchedulerBinding.instance.schedulerPhase !=
        SchedulerPhase.persistentCallbacks) {
      setState(() {});
      return;
    }
    if (_sessionPaintQueued) return;
    _sessionPaintQueued = true;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      _sessionPaintQueued = false;
      if (mounted) setState(() {});
    });
  }
}
