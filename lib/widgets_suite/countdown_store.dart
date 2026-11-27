import 'dart:convert';
import 'dart:math';

import 'package:material_ui/material_ui.dart';
import 'package:shared_preferences/shared_preferences.dart';

import '../pending_ui_writes.dart';
import '../schedule/schedule_store.dart';

abstract interface class CountdownStore {
  Future<String?> read();
  Future<void> write(String value);
}

class LocalCountdownStore implements CountdownStore {
  LocalCountdownStore({String? libraryDirectory})
    : key =
          'morrow.countdown.v1:${Uri.encodeComponent(libraryDirectory ?? 'default')}';

  final String key;

  @override
  Future<String?> read() async =>
      (await SharedPreferences.getInstance()).getString(key);

  @override
  Future<void> write(String value) async {
    if (!await (await SharedPreferences.getInstance()).setString(key, value)) {
      throw StateError('Countdown could not be saved');
    }
  }
}

class MemoryCountdownStore implements CountdownStore {
  String? value;

  @override
  Future<String?> read() async => value;

  @override
  Future<void> write(String value) async => this.value = value;
}

@immutable
class CountdownItem {
  const CountdownItem({
    required this.id,
    required this.title,
    required this.targetDateKey,
    this.note = '',
    this.pinned = true,
    this.linkedCardId,
    required this.createdAtMs,
  });

  final String id;
  final String title;
  final String targetDateKey;
  final String note;
  final bool pinned;
  final String? linkedCardId;
  final int createdAtMs;

  static const int maxTitleLength = 120;
  static const int maxNoteLength = 300;
  static final RegExp _idPattern = RegExp(r'^cd:[a-f0-9]{32}$');

  static String nextId() {
    final random = Random.secure();
    final hex = List.generate(
      16,
      (_) => random.nextInt(256).toRadixString(16).padLeft(2, '0'),
    ).join();
    return 'cd:$hex';
  }

  DateTime get targetDate =>
      ScheduleEntry.tryParseDateKey(targetDateKey) ?? DateTime(1970, 1, 1);

  bool get hasLinkedCard =>
      linkedCardId != null && linkedCardId!.trim().isNotEmpty;

  int daysFrom(DateTime now) {
    final base = ScheduleEntry.dateOnly(now);
    return targetDate.difference(base).inDays;
  }

  CountdownItem copyWith({
    String? title,
    String? targetDateKey,
    String? note,
    bool? pinned,
    String? linkedCardId,
    bool clearLinkedCard = false,
  }) => CountdownItem(
    id: id,
    title: title ?? this.title,
    targetDateKey: targetDateKey ?? this.targetDateKey,
    note: note ?? this.note,
    pinned: pinned ?? this.pinned,
    linkedCardId: clearLinkedCard ? null : (linkedCardId ?? this.linkedCardId),
    createdAtMs: createdAtMs,
  );

  CountdownItem validated() {
    final cleanTitle = title.trim();
    final cleanNote = note.trim();
    final cleanLink = linkedCardId?.trim();
    if (!_idPattern.hasMatch(id)) {
      throw const FormatException('Invalid countdown id');
    }
    if (cleanTitle.isEmpty ||
        cleanTitle.length > maxTitleLength ||
        cleanTitle.contains(RegExp(r'[\r\n]'))) {
      throw const FormatException('Invalid countdown title');
    }
    if (ScheduleEntry.tryParseDateKey(targetDateKey) == null) {
      throw const FormatException('Invalid countdown target date');
    }
    if (cleanNote.length > maxNoteLength) {
      throw const FormatException('Countdown note exceeds limit');
    }
    return CountdownItem(
      id: id,
      title: cleanTitle,
      targetDateKey: targetDateKey.trim(),
      note: cleanNote,
      pinned: pinned,
      linkedCardId: (cleanLink == null || cleanLink.isEmpty) ? null : cleanLink,
      createdAtMs: createdAtMs,
    );
  }

  Map<String, dynamic> toJson() => {
    'id': id,
    'title': title,
    'targetDate': targetDateKey,
    if (note.isNotEmpty) 'note': note,
    'pinned': pinned,
    if (hasLinkedCard) 'linkedCardId': linkedCardId,
    'createdAtMs': createdAtMs,
  };

  factory CountdownItem.fromJson(Map<String, dynamic> data) {
    final id = data['id'];
    final title = data['title'];
    final targetDateKey = data['targetDate'];
    final note = data['note'] ?? '';
    final pinned = data['pinned'] ?? true;
    final linkedCardId = data['linkedCardId'];
    final createdAtMs = data['createdAtMs'] ?? 0;
    if (id is! String ||
        title is! String ||
        targetDateKey is! String ||
        note is! String ||
        pinned is! bool ||
        (linkedCardId != null && linkedCardId is! String) ||
        createdAtMs is! int) {
      throw const FormatException('Invalid countdown JSON');
    }
    return CountdownItem(
      id: id,
      title: title,
      targetDateKey: targetDateKey,
      note: note,
      pinned: pinned,
      linkedCardId: linkedCardId as String?,
      createdAtMs: createdAtMs,
    ).validated();
  }
}

class CountdownPreferences extends ChangeNotifier {
  CountdownPreferences(this.store, {DateTime Function()? nowProvider})
    : _now = nowProvider ?? DateTime.now;

  final CountdownStore store;
  final DateTime Function() _now;
  static const int maxEntries = 200;

  List<CountdownItem> _entries = const [];
  Future<void> _tail = Future.value();
  bool loading = false;
  Object? error;
  bool _disposed = false;

  List<CountdownItem> get entries => _entries;

  List<CountdownItem> _sort(Iterable<CountdownItem> items) {
    final today = ScheduleEntry.dateOnly(_now());
    final list = items.map((e) => e.validated()).toList(growable: false);
    if (list.length > maxEntries) {
      throw const FormatException('Countdown limit exceeded');
    }
    if (list.map((e) => e.id).toSet().length != list.length) {
      throw const FormatException('Duplicate countdown id');
    }
    list.sort((a, b) {
      if (a.pinned != b.pinned) return a.pinned ? -1 : 1;
      final da = a.daysFrom(today);
      final db = b.daysFrom(today);
      final aUpcoming = da >= 0;
      final bUpcoming = db >= 0;
      if (aUpcoming != bUpcoming) return aUpcoming ? -1 : 1;
      if (aUpcoming) {
        final cmp = da.compareTo(db);
        if (cmp != 0) return cmp;
      } else {
        final cmp = db.compareTo(da);
        if (cmp != 0) return cmp;
      }
      return a.createdAtMs.compareTo(b.createdAtMs);
    });
    return List.unmodifiable(list);
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
            throw const FormatException('Unsupported countdown version');
          }
          final rawList = data['entries'] as List<dynamic>;
          _entries = _sort(
            rawList.map(
              (item) => CountdownItem.fromJson(
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
    _tail = run.catchError((Object _) {});
    return run;
  }

  Future<void> _commit(List<CountdownItem> next) {
    final sorted = _sort(next);
    final run = _tail.then((_) async {
      final previous = _entries;
      _entries = sorted;
      _notify();
      try {
        await pendingUiWrites.track(
          store.write(
            jsonEncode({
              'version': 1,
              'entries': sorted.map((e) => e.toJson()).toList(),
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

  CountdownItem? primaryForFooter(DateTime now) {
    for (final item in _entries) {
      if (item.pinned && item.daysFrom(now) >= 0) return item;
    }
    for (final item in _entries) {
      if (item.daysFrom(now) >= 0) return item;
    }
    return _entries.isEmpty ? null : _entries.first;
  }

  List<CountdownItem> todayItems(DateTime now) => _entries
      .where((item) => item.daysFrom(now) == 0)
      .toList(growable: false);

  Future<CountdownItem> add({
    required String title,
    required DateTime targetDate,
    String note = '',
    bool pinned = true,
    String? linkedCardId,
  }) async {
    final item = CountdownItem(
      id: CountdownItem.nextId(),
      title: title,
      targetDateKey: ScheduleEntry.formatDateKey(targetDate),
      note: note,
      pinned: pinned,
      linkedCardId: linkedCardId,
      createdAtMs: _now().millisecondsSinceEpoch,
    ).validated();
    await _commit([..._entries, item]);
    return item;
  }

  Future<void> update(CountdownItem updated) {
    final valid = updated.validated();
    return _commit([
      for (final item in _entries) item.id == valid.id ? valid : item,
    ]);
  }

  Future<void> togglePinned(String id) => _commit([
    for (final item in _entries)
      item.id == id ? item.copyWith(pinned: !item.pinned) : item,
  ]);

  Future<void> remove(String id) => _commit([
    for (final item in _entries)
      if (item.id != id) item,
  ]);

  @override
  void dispose() {
    _disposed = true;
    super.dispose();
  }
}

class CountdownScope extends InheritedNotifier<CountdownPreferences> {
  const CountdownScope({
    super.key,
    required CountdownPreferences controller,
    required super.child,
  }) : super(notifier: controller);

  static CountdownPreferences? of(BuildContext context) =>
      context.dependOnInheritedWidgetOfExactType<CountdownScope>()?.notifier;
}
