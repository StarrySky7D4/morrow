import 'dart:async';
import 'dart:convert';

import 'package:material_ui/material_ui.dart';
import 'package:shared_preferences/shared_preferences.dart';

import '../pending_ui_writes.dart';
import '../schedule/schedule_store.dart';

enum WorkbenchTimerMode { focus, shortBreak, longBreak, custom, stopwatch }

abstract interface class TimerStore {
  Future<String?> read();
  Future<void> write(String value);
}

class LocalTimerStore implements TimerStore {
  LocalTimerStore({String? libraryDirectory})
    : key =
          'morrow.timer.v1:${Uri.encodeComponent(libraryDirectory ?? 'default')}';

  final String key;

  @override
  Future<String?> read() async =>
      (await SharedPreferences.getInstance()).getString(key);

  @override
  Future<void> write(String value) async {
    if (!await (await SharedPreferences.getInstance()).setString(key, value)) {
      throw StateError('Timer state could not be saved');
    }
  }
}

class MemoryTimerStore implements TimerStore {
  String? value;

  @override
  Future<String?> read() async => value;

  @override
  Future<void> write(String value) async => this.value = value;
}

class TimerController extends ChangeNotifier {
  TimerController(this.store, {DateTime Function()? nowProvider})
    : _now = nowProvider ?? DateTime.now {
    _statsDateKey = ScheduleEntry.formatDateKey(_now());
  }

  final TimerStore store;
  final DateTime Function() _now;
  static const int minCustomMinutes = 1;
  static const int maxCustomMinutes = 360;
  static const int minCustomSeconds = 1;
  static const int maxCustomSeconds = maxCustomMinutes * 60;

  WorkbenchTimerMode _mode = WorkbenchTimerMode.focus;
  int _customMinutes = 45;
  int _customSeconds = 45 * 60;
  int _targetSeconds = 25 * 60;
  int _remainingSeconds = 25 * 60;
  int _elapsedSeconds = 0;
  bool _running = false;
  String? _linkedCardId;
  String? _linkedLabel;
  String _statsDateKey = '';
  int _completedSessionsToday = 0;
  int _focusMinutesToday = 0;
  int _secondFractionCarry = 0;
  Map<String, int> _cardFocusMinutes = {};

  VoidCallback? onStarted;
  VoidCallback? onCompleted;

  Timer? _ticker;
  Future<void> _tail = Future.value();
  bool _disposed = false;
  bool loading = false;
  Object? error;

  WorkbenchTimerMode get mode => _mode;
  int get customMinutes => _customMinutes;
  int get customTotalSeconds => _customSeconds;
  int get targetSeconds => _targetSeconds;
  int get remainingSeconds => _remainingSeconds;
  int get elapsedSeconds => _elapsedSeconds;
  bool get running => _running;
  String? get linkedCardId => _linkedCardId;
  String? get linkedLabel => _linkedLabel;
  int get completedSessionsToday {
    _rolloverDayIfNeeded();
    return _completedSessionsToday;
  }

  int get focusMinutesToday {
    _rolloverDayIfNeeded();
    return _focusMinutesToday;
  }

  Map<String, int> get cardFocusMinutes => Map.unmodifiable(_cardFocusMinutes);

  int focusMinutesForCard(String cardId) => _cardFocusMinutes[cardId] ?? 0;

  double get progress {
    if (_mode == WorkbenchTimerMode.stopwatch) {
      return (_elapsedSeconds % 3600) / 3600.0;
    }
    if (_targetSeconds <= 0) return 0;
    return ((_targetSeconds - _remainingSeconds) / _targetSeconds).clamp(
      0.0,
      1.0,
    );
  }

  String get formattedTime {
    final seconds = _mode == WorkbenchTimerMode.stopwatch
        ? _elapsedSeconds
        : _remainingSeconds;
    final h = seconds ~/ 3600;
    final m = (seconds % 3600) ~/ 60;
    final s = seconds % 60;
    if (h > 0) {
      return '${h.toString().padLeft(2, '0')}:${m.toString().padLeft(2, '0')}:${s.toString().padLeft(2, '0')}';
    }
    return '${m.toString().padLeft(2, '0')}:${s.toString().padLeft(2, '0')}';
  }

  int _defaultTargetForMode(WorkbenchTimerMode mode) => switch (mode) {
    WorkbenchTimerMode.focus => 25 * 60,
    WorkbenchTimerMode.shortBreak => 5 * 60,
    WorkbenchTimerMode.longBreak => 15 * 60,
    WorkbenchTimerMode.custom => _customSeconds,
    WorkbenchTimerMode.stopwatch => 0,
  };

  void _rolloverDayIfNeeded() {
    final todayKey = ScheduleEntry.formatDateKey(_now());
    if (_statsDateKey != todayKey) {
      _statsDateKey = todayKey;
      _completedSessionsToday = 0;
      _focusMinutesToday = 0;
    }
  }

  void _notify() {
    if (!_disposed) notifyListeners();
  }

  Future<void> restore() {
    loading = true;
    _notify();
    final run = _tail.then((_) async {
      try {
        final raw = await store.read();
        if (raw != null && raw.isNotEmpty) {
          final data = jsonDecode(raw) as Map<String, dynamic>;
          if (data['version'] != 1) {
            throw const FormatException('Unsupported timer version');
          }
          final modeName = data['mode'] as String? ?? 'focus';
          _mode = WorkbenchTimerMode.values.firstWhere(
            (m) => m.name == modeName,
            orElse: () => WorkbenchTimerMode.focus,
          );
          _customMinutes = ((data['customMinutes'] as int?) ?? 45).clamp(
            minCustomMinutes,
            maxCustomMinutes,
          );
          final rawCustomSeconds = data['customSeconds'];
          if (rawCustomSeconds is int && rawCustomSeconds >= minCustomSeconds) {
            _customSeconds = rawCustomSeconds.clamp(
              minCustomSeconds,
              maxCustomSeconds,
            );
            _customMinutes = (_customSeconds / 60).round().clamp(
              minCustomMinutes,
              maxCustomMinutes,
            );
          } else {
            _customSeconds = _customMinutes * 60;
          }
          _targetSeconds = _defaultTargetForMode(_mode);
          _remainingSeconds = _mode == WorkbenchTimerMode.stopwatch
              ? 0
              : _targetSeconds;
          _elapsedSeconds = 0;
          final rawCard = data['linkedCardId'];
          _linkedCardId = rawCard is String && rawCard.isNotEmpty
              ? rawCard
              : null;
          final rawLabel = data['linkedLabel'];
          _linkedLabel = rawLabel is String && rawLabel.isNotEmpty
              ? rawLabel
              : null;
          final savedDay = data['statsDateKey'] as String? ?? '';
          final todayKey = ScheduleEntry.formatDateKey(_now());
          if (savedDay == todayKey) {
            _statsDateKey = savedDay;
            _completedSessionsToday =
                (data['completedSessionsToday'] as int? ?? 0).clamp(0, 10000);
            _focusMinutesToday = (data['focusMinutesToday'] as int? ?? 0).clamp(
              0,
              100000,
            );
          } else {
            _statsDateKey = todayKey;
            _completedSessionsToday = 0;
            _focusMinutesToday = 0;
          }
          final rawCardMinutes = data['cardFocusMinutes'];
          if (rawCardMinutes is Map) {
            _cardFocusMinutes = {
              for (final e in rawCardMinutes.entries)
                if (e.key is String && e.value is int && (e.value as int) > 0)
                  e.key as String: e.value as int,
            };
          }
        }
        error = null;
      } catch (e) {
        error = e;
      } finally {
        loading = false;
        _notify();
      }
    });
    _tail = run.catchError((Object _) {});
    return run;
  }

  Future<void> _save() {
    _rolloverDayIfNeeded();
    final payload = jsonEncode({
      'version': 1,
      'mode': _mode.name,
      'customMinutes': _customMinutes,
      'customSeconds': _customSeconds,
      if (_linkedCardId != null) 'linkedCardId': _linkedCardId,
      if (_linkedLabel != null) 'linkedLabel': _linkedLabel,
      'statsDateKey': _statsDateKey,
      'completedSessionsToday': _completedSessionsToday,
      'focusMinutesToday': _focusMinutesToday,
      'cardFocusMinutes': _cardFocusMinutes,
    });
    final run = _tail.then((_) => pendingUiWrites.track(store.write(payload)));
    _tail = run.catchError((Object _) {});
    return run;
  }

  void selectMode(
    WorkbenchTimerMode mode, {
    int? customMinutes,
    int? customSeconds,
  }) {
    _stopTicker();
    _mode = mode;
    if (customSeconds != null) {
      _customSeconds = customSeconds.clamp(minCustomSeconds, maxCustomSeconds);
      _customMinutes = (_customSeconds / 60).round().clamp(
        minCustomMinutes,
        maxCustomMinutes,
      );
    } else if (customMinutes != null) {
      _customMinutes = customMinutes.clamp(minCustomMinutes, maxCustomMinutes);
      _customSeconds = _customMinutes * 60;
    }
    _targetSeconds = _defaultTargetForMode(_mode);
    _remainingSeconds = _targetSeconds;
    _elapsedSeconds = 0;
    _secondFractionCarry = 0;
    _notify();
    unawaited(_save());
  }

  void setCustomDuration({required int minutes, int seconds = 0}) {
    final total = (minutes * 60 + seconds).clamp(
      minCustomSeconds,
      maxCustomSeconds,
    );
    _stopTicker();
    _mode = WorkbenchTimerMode.custom;
    _customSeconds = total;
    _customMinutes = (total / 60).round().clamp(
      minCustomMinutes,
      maxCustomMinutes,
    );
    _targetSeconds = total;
    _remainingSeconds = total;
    _elapsedSeconds = 0;
    _secondFractionCarry = 0;
    _notify();
    unawaited(_save());
  }

  void adjustMinutes(int delta) {
    if (_mode == WorkbenchTimerMode.stopwatch) return;
    final currentMin = (_targetSeconds / 60).round().clamp(
      minCustomMinutes,
      maxCustomMinutes,
    );
    final nextMin = (currentMin + delta).clamp(
      minCustomMinutes,
      maxCustomMinutes,
    );
    _stopTicker();
    _mode = WorkbenchTimerMode.custom;
    _customMinutes = nextMin;
    _customSeconds = nextMin * 60;
    _targetSeconds = _customSeconds;
    _remainingSeconds = _targetSeconds;
    _elapsedSeconds = 0;
    _secondFractionCarry = 0;
    _notify();
    unawaited(_save());
  }

  void bindCard(String? cardId, {String? label}) {
    final cleanId = cardId?.trim();
    _linkedCardId = (cleanId == null || cleanId.isEmpty) ? null : cleanId;
    final cleanLabel = label?.trim();
    _linkedLabel =
        (_linkedCardId == null || cleanLabel == null || cleanLabel.isEmpty)
        ? null
        : cleanLabel;
    _notify();
    unawaited(_save());
  }

  void start() {
    if (_running) return;
    if (_mode != WorkbenchTimerMode.stopwatch && _remainingSeconds <= 0) {
      _remainingSeconds = _targetSeconds;
      _elapsedSeconds = 0;
    }
    _running = true;
    _ticker?.cancel();
    _ticker = Timer.periodic(const Duration(seconds: 1), (_) => tickOneSecond());
    _notify();
    onStarted?.call();
  }

  void pause() {
    if (!_running) return;
    _stopTicker();
    _notify();
    unawaited(_save());
  }

  void toggle() => _running ? pause() : start();

  void reset() {
    _stopTicker();
    _remainingSeconds = _mode == WorkbenchTimerMode.stopwatch ? 0 : _targetSeconds;
    _elapsedSeconds = 0;
    _secondFractionCarry = 0;
    _notify();
  }

  void tickOneSecond() {
    if (!_running) return;
    _elapsedSeconds += 1;
    final trackFocus =
        _mode == WorkbenchTimerMode.focus ||
        _mode == WorkbenchTimerMode.custom ||
        _mode == WorkbenchTimerMode.stopwatch;
    if (trackFocus) {
      _secondFractionCarry += 1;
      if (_secondFractionCarry >= 60) {
        _secondFractionCarry -= 60;
        _rolloverDayIfNeeded();
        _focusMinutesToday += 1;
        if (_linkedCardId != null) {
          _cardFocusMinutes = {
            ..._cardFocusMinutes,
            _linkedCardId!: (_cardFocusMinutes[_linkedCardId!] ?? 0) + 1,
          };
        }
      }
    }

    if (_mode == WorkbenchTimerMode.stopwatch) {
      _notify();
      return;
    }

    if (_remainingSeconds > 1) {
      _remainingSeconds -= 1;
      _notify();
      return;
    }

    // Completed countdown
    _remainingSeconds = 0;
    _stopTicker();
    _rolloverDayIfNeeded();
    if (_mode == WorkbenchTimerMode.focus || _mode == WorkbenchTimerMode.custom) {
      _completedSessionsToday += 1;
    }
    _notify();
    unawaited(_save());
    onCompleted?.call();
  }

  void _stopTicker() {
    _running = false;
    _ticker?.cancel();
    _ticker = null;
  }

  @override
  void dispose() {
    _stopTicker();
    _disposed = true;
    super.dispose();
  }
}

class TimerScope extends InheritedNotifier<TimerController> {
  const TimerScope({
    super.key,
    required TimerController controller,
    required super.child,
  }) : super(notifier: controller);

  static TimerController? of(BuildContext context) =>
      context.dependOnInheritedWidgetOfExactType<TimerScope>()?.notifier;
}
