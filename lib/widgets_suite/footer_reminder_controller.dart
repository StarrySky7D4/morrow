import 'dart:async';

import 'package:material_ui/material_ui.dart';

@immutable
class FooterAlert {
  const FooterAlert({
    required this.id,
    required this.message,
    required this.badge,
    this.scheduleDate,
    this.linkedCardId,
    required this.createdAt,
  });

  final String id;
  final String message;
  final String badge;
  final DateTime? scheduleDate;
  final String? linkedCardId;
  final DateTime createdAt;
}

class FooterReminderController extends ChangeNotifier {
  FooterAlert? _activeAlert;
  Timer? _autoDismissTimer;
  bool _disposed = false;

  FooterAlert? get activeAlert => _activeAlert;

  void post(
    FooterAlert alert, {
    Duration autoDismiss = const Duration(seconds: 24),
  }) {
    if (_disposed) return;
    _activeAlert = alert;
    _autoDismissTimer?.cancel();
    _autoDismissTimer = Timer(autoDismiss, dismiss);
    notifyListeners();
  }

  void dismiss() {
    if (_disposed || _activeAlert == null) return;
    _activeAlert = null;
    _autoDismissTimer?.cancel();
    _autoDismissTimer = null;
    notifyListeners();
  }

  @override
  void dispose() {
    _autoDismissTimer?.cancel();
    _autoDismissTimer = null;
    _disposed = true;
    super.dispose();
  }
}
