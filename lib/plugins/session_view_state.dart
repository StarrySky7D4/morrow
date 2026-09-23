import 'package:flutter/scheduler.dart';
import 'package:flutter/widgets.dart';

/// Shared sessions can notify an existing panel while a sibling is mounting.
/// Defer only repaint notification; commands and ownership are never deferred.
mixin SessionViewState<T extends StatefulWidget> on State<T> {
  bool _sessionPaintQueued = false;
  bool _viewActive = true;
  bool get sessionViewActive => _viewActive;

  /// Display activity only; hiding a view never cancels a business operation.
  void sessionViewVisibilityChanged() {}

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    final active =
        TickerMode.valuesOf(context).enabled &&
        (ModalRoute.of(context)?.isCurrent ?? true);
    if (_viewActive != active) {
      _viewActive = active;
      sessionViewVisibilityChanged();
    }
  }

  void markSessionViewDirty() {
    if (!mounted || !_viewActive) return;
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
