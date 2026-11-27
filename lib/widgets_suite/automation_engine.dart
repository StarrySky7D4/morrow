import 'dart:convert';
import 'dart:math';

import 'package:material_ui/material_ui.dart';
import 'package:shared_preferences/shared_preferences.dart';

import '../pending_ui_writes.dart';
import 'footer_reminder_controller.dart';

enum AutomationTrigger {
  timerStarted,
  timerCompleted,
  scheduleDue,
  countdownToday,
}

enum AutomationAction {
  playMusic,
  pauseMusic,
  footerReminder,
  enterFocusMode,
  exitFocusMode,
  startTimer,
}

abstract interface class AutomationStore {
  Future<String?> read();
  Future<void> write(String value);
}

class LocalAutomationStore implements AutomationStore {
  LocalAutomationStore({String? libraryDirectory})
    : key =
          'morrow.automation.v1:${Uri.encodeComponent(libraryDirectory ?? 'default')}';

  final String key;

  @override
  Future<String?> read() async =>
      (await SharedPreferences.getInstance()).getString(key);

  @override
  Future<void> write(String value) async {
    if (!await (await SharedPreferences.getInstance()).setString(key, value)) {
      throw StateError('Automation rules could not be saved');
    }
  }
}

class MemoryAutomationStore implements AutomationStore {
  String? value;

  @override
  Future<String?> read() async => value;

  @override
  Future<void> write(String value) async => this.value = value;
}

@immutable
class AutomationRule {
  const AutomationRule({
    required this.id,
    required this.name,
    required this.trigger,
    required this.action,
    this.enabled = true,
    this.customMessage = '',
    this.builtIn = false,
  });

  final String id;
  final String name;
  final AutomationTrigger trigger;
  final AutomationAction action;
  final bool enabled;
  final String customMessage;
  final bool builtIn;

  static String nextId() {
    final random = Random.secure();
    final hex = List.generate(
      12,
      (_) => random.nextInt(256).toRadixString(16).padLeft(2, '0'),
    ).join();
    return 'rule:$hex';
  }

  AutomationRule copyWith({
    String? name,
    AutomationTrigger? trigger,
    AutomationAction? action,
    bool? enabled,
    String? customMessage,
  }) => AutomationRule(
    id: id,
    name: name ?? this.name,
    trigger: trigger ?? this.trigger,
    action: action ?? this.action,
    enabled: enabled ?? this.enabled,
    customMessage: customMessage ?? this.customMessage,
    builtIn: builtIn,
  );

  Map<String, dynamic> toJson() => {
    'id': id,
    'name': name,
    'trigger': trigger.name,
    'action': action.name,
    'enabled': enabled,
    if (customMessage.isNotEmpty) 'customMessage': customMessage,
    if (builtIn) 'builtIn': true,
  };

  factory AutomationRule.fromJson(Map<String, dynamic> data) {
    final id = data['id'];
    final name = data['name'];
    final trigName = data['trigger'];
    final actName = data['action'];
    final enabled = data['enabled'] ?? true;
    final customMessage = data['customMessage'] ?? '';
    final builtIn = data['builtIn'] ?? false;
    if (id is! String ||
        name is! String ||
        name.trim().isEmpty ||
        trigName is! String ||
        actName is! String ||
        enabled is! bool ||
        customMessage is! String ||
        builtIn is! bool) {
      throw const FormatException('Invalid automation rule');
    }
    return AutomationRule(
      id: id,
      name: name.trim(),
      trigger: AutomationTrigger.values.firstWhere(
        (t) => t.name == trigName,
        orElse: () => throw const FormatException('Unknown trigger'),
      ),
      action: AutomationAction.values.firstWhere(
        (a) => a.name == actName,
        orElse: () => throw const FormatException('Unknown action'),
      ),
      enabled: enabled,
      customMessage: customMessage.trim(),
      builtIn: builtIn,
    );
  }
}

@immutable
class AutomationLogEntry {
  const AutomationLogEntry({
    required this.timestamp,
    required this.ruleName,
    required this.summary,
  });

  final DateTime timestamp;
  final String ruleName;
  final String summary;
}

class AutomationRuntime {
  const AutomationRuntime({
    required this.onPlayMusic,
    required this.onPauseMusic,
    required this.onFooterReminder,
    required this.onEnterFocusMode,
    required this.onExitFocusMode,
    required this.onStartTimer,
  });

  final VoidCallback onPlayMusic;
  final VoidCallback onPauseMusic;
  final void Function(FooterAlert alert) onFooterReminder;
  final VoidCallback onEnterFocusMode;
  final VoidCallback onExitFocusMode;
  final VoidCallback onStartTimer;
}

class AutomationPreferences extends ChangeNotifier {
  AutomationPreferences(this.store);

  final AutomationStore store;
  static const int maxRules = 100;
  static const int maxLogs = 20;

  static List<AutomationRule> defaultRules() => const [
    AutomationRule(
      id: 'builtin-timer-play-music',
      name: '专注开始自动播放音乐',
      trigger: AutomationTrigger.timerStarted,
      action: AutomationAction.playMusic,
      enabled: true,
      builtIn: true,
    ),
    AutomationRule(
      id: 'builtin-timer-done-remind',
      name: '定时结束底部提醒',
      trigger: AutomationTrigger.timerCompleted,
      action: AutomationAction.footerReminder,
      enabled: true,
      customMessage: '专注时段已完成，休息一下吧',
      builtIn: true,
    ),
    AutomationRule(
      id: 'builtin-schedule-due-remind',
      name: '日程到点底部提醒',
      trigger: AutomationTrigger.scheduleDue,
      action: AutomationAction.footerReminder,
      enabled: true,
      builtIn: true,
    ),
    AutomationRule(
      id: 'builtin-timer-done-pause',
      name: '定时结束暂停音乐',
      trigger: AutomationTrigger.timerCompleted,
      action: AutomationAction.pauseMusic,
      enabled: false,
      builtIn: true,
    ),
  ];

  List<AutomationRule> _rules = defaultRules();
  final List<AutomationLogEntry> _logs = [];
  Future<void> _tail = Future.value();
  bool loading = false;
  Object? error;
  bool _disposed = false;

  List<AutomationRule> get rules => _rules;
  List<AutomationLogEntry> get logs => List.unmodifiable(_logs);
  int get enabledCount => _rules.where((r) => r.enabled).length;

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
            throw const FormatException('Unsupported automation version');
          }
          final rawList = data['rules'] as List<dynamic>;
          final parsed = rawList
              .map(
                (item) => AutomationRule.fromJson(
                  Map<String, dynamic>.from(item as Map),
                ),
              )
              .toList(growable: false);
          if (parsed.length > maxRules) {
            throw const FormatException('Automation rules exceed limit');
          }
          _rules = List.unmodifiable(parsed);
        } else {
          _rules = List.unmodifiable(defaultRules());
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

  Future<void> _commit(List<AutomationRule> next) {
    if (next.length > maxRules) {
      throw const FormatException('Automation rules exceed limit');
    }
    final frozen = List<AutomationRule>.unmodifiable(next);
    final run = _tail.then((_) async {
      final previous = _rules;
      _rules = frozen;
      _notify();
      try {
        await pendingUiWrites.track(
          store.write(
            jsonEncode({
              'version': 1,
              'rules': frozen.map((r) => r.toJson()).toList(),
            }),
          ),
        );
        error = null;
      } catch (e) {
        _rules = previous;
        error = e;
        _notify();
        rethrow;
      }
    });
    _tail = run.catchError((Object _) {});
    return run;
  }

  Future<void> toggleRule(String id) => _commit([
    for (final rule in _rules)
      rule.id == id ? rule.copyWith(enabled: !rule.enabled) : rule,
  ]);

  Future<AutomationRule> addRule({
    required String name,
    required AutomationTrigger trigger,
    required AutomationAction action,
    String customMessage = '',
  }) async {
    final rule = AutomationRule(
      id: AutomationRule.nextId(),
      name: name.trim(),
      trigger: trigger,
      action: action,
      enabled: true,
      customMessage: customMessage.trim(),
    );
    await _commit([..._rules, rule]);
    return rule;
  }

  Future<void> removeRule(String id) => _commit([
    for (final rule in _rules)
      if (rule.id != id) rule,
  ]);

  List<String> trigger(
    AutomationTrigger trigger, {
    required AutomationRuntime runtime,
    String? subject,
    DateTime? scheduleDate,
    String? linkedCardId,
    DateTime? now,
  }) {
    final currentTime = now ?? DateTime.now();
    final executed = <String>[];
    for (final rule in _rules) {
      if (!rule.enabled || rule.trigger != trigger) continue;
      _executeRule(
        rule,
        runtime: runtime,
        subject: subject,
        scheduleDate: scheduleDate,
        linkedCardId: linkedCardId,
        now: currentTime,
      );
      executed.add(rule.id);
    }
    if (executed.isNotEmpty) _notify();
    return executed;
  }

  void runRuleNow(
    AutomationRule rule, {
    required AutomationRuntime runtime,
    String? subject,
    DateTime? now,
  }) {
    _executeRule(
      rule,
      runtime: runtime,
      subject: subject ?? rule.name,
      now: now ?? DateTime.now(),
    );
    _notify();
  }

  void _executeRule(
    AutomationRule rule, {
    required AutomationRuntime runtime,
    String? subject,
    DateTime? scheduleDate,
    String? linkedCardId,
    required DateTime now,
  }) {
    switch (rule.action) {
      case AutomationAction.playMusic:
        runtime.onPlayMusic();
      case AutomationAction.pauseMusic:
        runtime.onPauseMusic();
      case AutomationAction.enterFocusMode:
        runtime.onEnterFocusMode();
      case AutomationAction.exitFocusMode:
        runtime.onExitFocusMode();
      case AutomationAction.startTimer:
        runtime.onStartTimer();
      case AutomationAction.footerReminder:
        final cleanCustom = rule.customMessage.trim();
        final cleanSubject = subject?.trim() ?? '';
        final message = cleanSubject.isNotEmpty && rule.trigger == AutomationTrigger.scheduleDue
            ? cleanSubject
            : cleanCustom.isNotEmpty
            ? (cleanSubject.isNotEmpty && rule.trigger != AutomationTrigger.timerCompleted
                ? '$cleanCustom · $cleanSubject'
                : cleanCustom)
            : (cleanSubject.isNotEmpty ? cleanSubject : rule.name);
        final badge = switch (rule.trigger) {
          AutomationTrigger.scheduleDue => '日程提醒',
          AutomationTrigger.countdownToday => '倒数日到达',
          AutomationTrigger.timerCompleted => '定时完成',
          AutomationTrigger.timerStarted => '专注开始',
        };
        runtime.onFooterReminder(
          FooterAlert(
            id: '${rule.id}-${now.millisecondsSinceEpoch}',
            message: message,
            badge: badge,
            scheduleDate: scheduleDate,
            linkedCardId: linkedCardId,
            createdAt: now,
          ),
        );
    }
    final summary = subject != null && subject.trim().isNotEmpty
        ? subject.trim()
        : rule.name;
    _logs.insert(
      0,
      AutomationLogEntry(timestamp: now, ruleName: rule.name, summary: summary),
    );
    if (_logs.length > maxLogs) {
      _logs.removeRange(maxLogs, _logs.length);
    }
  }

  @override
  void dispose() {
    _disposed = true;
    super.dispose();
  }
}

class AutomationScope extends InheritedNotifier<AutomationPreferences> {
  const AutomationScope({
    super.key,
    required AutomationPreferences controller,
    required super.child,
  }) : super(notifier: controller);

  static AutomationPreferences? of(BuildContext context) =>
      context.dependOnInheritedWidgetOfExactType<AutomationScope>()?.notifier;
}
