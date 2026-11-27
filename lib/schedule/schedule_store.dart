import 'dart:convert';
import 'dart:math';

import 'package:material_ui/material_ui.dart';
import 'package:shared_preferences/shared_preferences.dart';

import '../pending_ui_writes.dart';

/// Local calendar & schedule persistence, independent of core/plugin wire schemas.
abstract interface class ScheduleStore {
  Future<String?> read();
  Future<void> write(String value);
}

class LocalScheduleStore implements ScheduleStore {
  LocalScheduleStore({String? libraryDirectory})
    : key =
          'morrow.schedule.v1:${Uri.encodeComponent(libraryDirectory ?? 'default')}';

  final String key;

  @override
  Future<String?> read() async =>
      (await SharedPreferences.getInstance()).getString(key);

  @override
  Future<void> write(String value) async {
    if (!await (await SharedPreferences.getInstance()).setString(key, value)) {
      throw StateError('Schedule could not be saved');
    }
  }
}

class MemoryScheduleStore implements ScheduleStore {
  String? value;

  @override
  Future<String?> read() async => value;

  @override
  Future<void> write(String value) async => this.value = value;
}

@immutable
class ScheduleEntry {
  const ScheduleEntry({
    required this.id,
    required this.title,
    required this.dateKey,
    this.timeText = '',
    this.note = '',
    this.completed = false,
    this.linkedCardId,
    required this.createdAtMs,
  });

  final String id;
  final String title;
  final String dateKey;
  final String timeText;
  final String note;
  final bool completed;
  final String? linkedCardId;
  final int createdAtMs;

  static const int maxTitleLength = 160;
  static const int maxNoteLength = 500;
  static final RegExp _datePattern = RegExp(r'^(\d{4})-(\d{2})-(\d{2})$');
  static final RegExp _timePattern = RegExp(r'^([01]\d|2[0-3]):[0-5]\d$');
  static final RegExp _idPattern = RegExp(r'^sched:[a-f0-9]{32}$');

  static DateTime dateOnly(DateTime value) =>
      DateTime(value.year, value.month, value.day);

  static String formatDateKey(DateTime date) {
    final y = date.year.toString().padLeft(4, '0');
    final m = date.month.toString().padLeft(2, '0');
    final d = date.day.toString().padLeft(2, '0');
    return '$y-$m-$d';
  }

  static DateTime? tryParseDateKey(String raw) {
    final m = _datePattern.firstMatch(raw.trim());
    if (m == null) return null;
    final year = int.parse(m.group(1)!);
    final month = int.parse(m.group(2)!);
    final day = int.parse(m.group(3)!);
    if (month < 1 || month > 12 || day < 1 || day > 31) return null;
    final parsed = DateTime(year, month, day);
    if (parsed.year != year || parsed.month != month || parsed.day != day) {
      return null;
    }
    return parsed;
  }

  static bool isValidTimeText(String raw) {
    final trimmed = raw.trim();
    return trimmed.isEmpty || _timePattern.hasMatch(trimmed);
  }

  static String nextId() {
    final random = Random.secure();
    final hex = List.generate(
      16,
      (_) => random.nextInt(256).toRadixString(16).padLeft(2, '0'),
    ).join();
    return 'sched:$hex';
  }

  DateTime get date => tryParseDateKey(dateKey) ?? DateTime(1970, 1, 1);

  bool get isAllDay => timeText.isEmpty;

  bool get hasLinkedCard =>
      linkedCardId != null && linkedCardId!.trim().isNotEmpty;

  ScheduleEntry copyWith({
    String? title,
    String? dateKey,
    String? timeText,
    String? note,
    bool? completed,
    String? linkedCardId,
    bool clearLinkedCard = false,
  }) => ScheduleEntry(
    id: id,
    title: title ?? this.title,
    dateKey: dateKey ?? this.dateKey,
    timeText: timeText ?? this.timeText,
    note: note ?? this.note,
    completed: completed ?? this.completed,
    linkedCardId: clearLinkedCard ? null : (linkedCardId ?? this.linkedCardId),
    createdAtMs: createdAtMs,
  );

  ScheduleEntry validated() {
    final cleanTitle = title.trim();
    final cleanNote = note.trim();
    final cleanTime = timeText.trim();
    final cleanLink = linkedCardId?.trim();
    if (!_idPattern.hasMatch(id)) {
      throw const FormatException('Invalid schedule id');
    }
    if (cleanTitle.isEmpty ||
        cleanTitle.length > maxTitleLength ||
        cleanTitle.contains(RegExp(r'[\r\n]'))) {
      throw const FormatException('Invalid schedule title');
    }
    if (tryParseDateKey(dateKey) == null) {
      throw const FormatException('Invalid schedule date');
    }
    if (!isValidTimeText(cleanTime)) {
      throw const FormatException('Invalid schedule time');
    }
    if (cleanNote.length > maxNoteLength) {
      throw const FormatException('Schedule note exceeds limit');
    }
    return ScheduleEntry(
      id: id,
      title: cleanTitle,
      dateKey: dateKey.trim(),
      timeText: cleanTime,
      note: cleanNote,
      completed: completed,
      linkedCardId: (cleanLink == null || cleanLink.isEmpty) ? null : cleanLink,
      createdAtMs: createdAtMs,
    );
  }

  Map<String, dynamic> toJson() => {
    'id': id,
    'title': title,
    'date': dateKey,
    if (timeText.isNotEmpty) 'time': timeText,
    if (note.isNotEmpty) 'note': note,
    if (completed) 'completed': true,
    if (hasLinkedCard) 'linkedCardId': linkedCardId,
    'createdAtMs': createdAtMs,
  };

  factory ScheduleEntry.fromJson(Map<String, dynamic> data) {
    final id = data['id'];
    final title = data['title'];
    final dateKey = data['date'];
    final timeText = data['time'] ?? '';
    final note = data['note'] ?? '';
    final completed = data['completed'] ?? false;
    final linkedCardId = data['linkedCardId'];
    final createdAtMs = data['createdAtMs'] ?? 0;
    if (id is! String ||
        title is! String ||
        dateKey is! String ||
        timeText is! String ||
        note is! String ||
        completed is! bool ||
        (linkedCardId != null && linkedCardId is! String) ||
        createdAtMs is! int) {
      throw const FormatException('Invalid schedule entry JSON');
    }
    return ScheduleEntry(
      id: id,
      title: title,
      dateKey: dateKey,
      timeText: timeText,
      note: note,
      completed: completed,
      linkedCardId: linkedCardId as String?,
      createdAtMs: createdAtMs,
    ).validated();
  }
}

class SchedulePreferences extends ChangeNotifier {
  SchedulePreferences(this.store);

  final ScheduleStore store;
  static const int maxEntries = 1000;

  List<ScheduleEntry> _entries = const [];
  Future<void> _tail = Future.value();
  bool loading = false;
  Object? error;
  bool _disposed = false;

  List<ScheduleEntry> get entries => _entries;

  static int _compareEntries(ScheduleEntry a, ScheduleEntry b) {
    final byDate = a.dateKey.compareTo(b.dateKey);
    if (byDate != 0) return byDate;
    final ta = a.isAllDay ? '99:99' : a.timeText;
    final tb = b.isAllDay ? '99:99' : b.timeText;
    final byTime = ta.compareTo(tb);
    if (byTime != 0) return byTime;
    final byCreated = a.createdAtMs.compareTo(b.createdAtMs);
    if (byCreated != 0) return byCreated;
    return a.id.compareTo(b.id);
  }

  static List<ScheduleEntry> _sortAndValidate(Iterable<ScheduleEntry> items) {
    final list = items.map((e) => e.validated()).toList(growable: false);
    if (list.length > maxEntries) {
      throw const FormatException('Schedule entry limit exceeded');
    }
    if (list.map((e) => e.id).toSet().length != list.length) {
      throw const FormatException('Duplicate schedule id');
    }
    list.sort(_compareEntries);
    return List.unmodifiable(list);
  }

  void _notify() {
    if (!_disposed) notifyListeners();
  }

  Future<void> restore() {
    loading = true;
    _notify();
    final current = _tail.then((_) async {
      try {
        final raw = await store.read();
        if (raw != null && raw.isNotEmpty) {
          final decoded = jsonDecode(raw) as Map<String, dynamic>;
          if (decoded['version'] != 1) {
            throw const FormatException('Unsupported schedule version');
          }
          final rawList = decoded['entries'] as List<dynamic>;
          _entries = _sortAndValidate(
            rawList.map(
              (item) => ScheduleEntry.fromJson(
                Map<String, dynamic>.from(item as Map),
              ),
            ),
          );
        } else {
          _entries = const [];
        }
        error = null;
      } catch (e) {
        error = e;
      } finally {
        loading = false;
        _notify();
      }
    });
    _tail = current.catchError((Object _) {});
    return current;
  }

  Future<void> _commit(List<ScheduleEntry> next) {
    final validated = _sortAndValidate(next);
    final run = _tail.then((_) async {
      final previous = _entries;
      _entries = validated;
      _notify();
      try {
        await pendingUiWrites.track(
          store.write(
            jsonEncode({
              'version': 1,
              'entries': validated.map((e) => e.toJson()).toList(),
            }),
          ),
        );
        error = null;
      } catch (e) {
        _entries = previous;
        error = e;
        _notify();
        rethrow;
      }
    });
    _tail = run.catchError((Object _) {});
    return run;
  }

  List<ScheduleEntry> entriesForDate(DateTime day) {
    final key = ScheduleEntry.formatDateKey(day);
    return _entries.where((e) => e.dateKey == key).toList(growable: false);
  }

  List<ScheduleEntry> entriesForCard(String cardId) => _entries
      .where((e) => e.linkedCardId == cardId)
      .toList(growable: false);

  ScheduleEntry? primaryEntryForCard(String cardId) {
    final matching = entriesForCard(cardId);
    if (matching.isEmpty) return null;
    for (final entry in matching) {
      if (!entry.completed) return entry;
    }
    return matching.last;
  }

  Map<String, int> entryCountsForMonth(int year, int month) {
    final prefix =
        '${year.toString().padLeft(4, '0')}-${month.toString().padLeft(2, '0')}-';
    final counts = <String, int>{};
    for (final entry in _entries) {
      if (entry.dateKey.startsWith(prefix)) {
        counts[entry.dateKey] = (counts[entry.dateKey] ?? 0) + 1;
      }
    }
    return counts;
  }

  Set<String> linkedDatesForMonth(int year, int month) {
    final prefix =
        '${year.toString().padLeft(4, '0')}-${month.toString().padLeft(2, '0')}-';
    return {
      for (final entry in _entries)
        if (entry.dateKey.startsWith(prefix) && entry.hasLinkedCard)
          entry.dateKey,
    };
  }

  int pendingCountForDate(DateTime day) {
    final key = ScheduleEntry.formatDateKey(day);
    return _entries.where((e) => e.dateKey == key && !e.completed).length;
  }

  Future<ScheduleEntry> add({
    required String title,
    required DateTime date,
    String timeText = '',
    String note = '',
    String? linkedCardId,
  }) async {
    final entry = ScheduleEntry(
      id: ScheduleEntry.nextId(),
      title: title,
      dateKey: ScheduleEntry.formatDateKey(date),
      timeText: timeText,
      note: note,
      linkedCardId: linkedCardId,
      createdAtMs: DateTime.now().millisecondsSinceEpoch,
    ).validated();
    await _commit([..._entries, entry]);
    return entry;
  }

  Future<void> update(ScheduleEntry updated) {
    final valid = updated.validated();
    return _commit([
      for (final entry in _entries) entry.id == valid.id ? valid : entry,
    ]);
  }

  Future<void> toggleCompleted(String id) => _commit([
    for (final entry in _entries)
      entry.id == id ? entry.copyWith(completed: !entry.completed) : entry,
  ]);

  Future<void> remove(String id) => _commit([
    for (final entry in _entries)
      if (entry.id != id) entry,
  ]);

  Future<void> clearLinkForCard(String cardId) {
    if (!_entries.any((e) => e.linkedCardId == cardId)) {
      return Future.value();
    }
    return _commit([
      for (final entry in _entries)
        entry.linkedCardId == cardId
            ? entry.copyWith(clearLinkedCard: true)
            : entry,
    ]);
  }

  @override
  void dispose() {
    _disposed = true;
    super.dispose();
  }
}

class SchedulePreferencesScope extends InheritedNotifier<SchedulePreferences> {
  const SchedulePreferencesScope({
    super.key,
    required SchedulePreferences controller,
    required super.child,
  }) : super(notifier: controller);

  static SchedulePreferences? of(BuildContext context) =>
      context
          .dependOnInheritedWidgetOfExactType<SchedulePreferencesScope>()
          ?.notifier;
}
