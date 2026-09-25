import 'dart:convert';
import 'pending_ui_writes.dart';
import 'dart:math';
import 'package:flutter/material.dart';
import 'package:shared_preferences/shared_preferences.dart';

/// Local presentation text, independent of the business plugin's wire contract.
abstract interface class TipStore {
  Future<String?> read();
  Future<void> write(String value);
}

class LocalTipStore implements TipStore {
  LocalTipStore({String? libraryDirectory})
    : key =
          'morrow.tip-text.v1:${Uri.encodeComponent(libraryDirectory ?? 'default')}';
  final String key;
  @override
  Future<String?> read() async =>
      (await SharedPreferences.getInstance()).getString(key);
  @override
  Future<void> write(String value) async {
    if (!await (await SharedPreferences.getInstance()).setString(key, value)) {
      throw StateError('Tip text could not be saved');
    }
  }
}

class MemoryTipStore implements TipStore {
  String? value;
  @override
  Future<String?> read() async => value;
  @override
  Future<void> write(String value) async => this.value = value;
}

@immutable
class TipItem {
  const TipItem(this.id, this.text);
  final String id, text;
  TipItem withText(String value) => TipItem(id, value);
  factory TipItem.empty() {
    final random = Random.secure();
    return TipItem(
      'daily:${List.generate(16, (_) => random.nextInt(256).toRadixString(16).padLeft(2, '0')).join()}',
      '',
    );
  }
}

class TipPreferences extends ChangeNotifier {
  TipPreferences(this.store);
  final TipStore store;
  static const componentIds = {'corner-tips', 'footer', 'daily'};
  static const defaultDailyIds = ['给自己倒一杯水', '把一个想法写下来', '留十分钟，随便探索'];
  static const _textIds = {'corner-tips', 'footer', 'daily-caption'};
  static const maxLength = 16000;
  static const maxLines = 100;
  Map<String, List<String>> _values = {};
  List<TipItem>? _daily;
  Future<void> _tail = Future.value();
  bool _disposed = false;
  bool loading = false;
  Object? loadError;

  List<String>? customLines(String id) => id == 'daily'
      ? _daily?.map((item) => item.text).toList(growable: false)
      : _values[id];
  List<String> lines(String id, List<String> defaults) =>
      customLines(id) ?? defaults;
  List<TipItem> items(String id, List<String> defaults) => id == 'daily'
      ? _daily ??
            List.generate(
              defaults.length,
              (i) => TipItem(defaultDailyIds[i], defaults[i]),
            )
      : [
          for (final (i, text) in lines(id, defaults).indexed)
            TipItem('tip-$i', text),
        ];
  String dailyCaption(String fallback) =>
      _values['daily-caption']?.first ?? fallback;

  static List<TipItem> _validateItems(List<TipItem> items) {
    final normalized = items
        .where((item) => item.text.trim().isNotEmpty)
        .map((item) => item.withText(item.text.trim()))
        .toList(growable: false);
    parse(normalized.map((item) => item.text).join('\n'));
    if (normalized.length > maxLines ||
        normalized.map((item) => item.id).toSet().length != normalized.length ||
        normalized.any(
          (item) =>
              (!defaultDailyIds.contains(item.id) &&
                  !RegExp(r'^daily:[a-f0-9]{32}$').hasMatch(item.id)) ||
              item.text.contains(RegExp(r'[\r\n]')),
        )) {
      throw const FormatException('Invalid daily tips');
    }
    return List.unmodifiable(normalized);
  }

  static List<String> parse(String text) {
    final lines = text
        .split(RegExp(r'\r?\n'))
        .map((s) => s.trim())
        .where((s) => s.isNotEmpty)
        .toList(growable: false);
    if (text.length > maxLength || lines.length > maxLines) {
      throw const FormatException('Tip text exceeds the limit');
    }
    return List.unmodifiable(lines);
  }

  void _notify() {
    if (!_disposed) notifyListeners();
  }

  Future<void> restore() {
    loading = true;
    _notify();
    final result = _tail.then((_) async {
      try {
        final raw = await store.read();
        final data = raw == null
            ? null
            : jsonDecode(raw) as Map<String, dynamic>;
        final values = <String, List<String>>{};
        if (data != null) {
          if (data['version'] != 1) throw const FormatException('Tip version');
          for (final entry in (data['text'] as Map<String, dynamic>).entries) {
            if (!_textIds.contains(entry.key)) continue;
            final lines = parse(entry.value as String);
            if (entry.key == 'daily-caption' && lines.length > 1) {
              throw const FormatException('Invalid daily caption');
            }
            if (lines.isNotEmpty) values[entry.key] = lines;
          }
        }
        final daily = data?['daily'] as List?;
        final restoredDaily = daily == null
            ? null
            : _validateItems([
                for (final item in daily)
                  TipItem(item['id'] as String, item['text'] as String),
              ]);
        _values = values;
        _daily = restoredDaily?.isEmpty == true ? null : restoredDaily;
        loadError = null;
      } catch (error) {
        loadError = error;
      } finally {
        loading = false;
        _notify();
      }
    });
    _tail = result;
    return result;
  }

  Future<void> save(String id, String text) {
    if (!_textIds.contains(id)) throw ArgumentError.value(id);
    final lines = parse(text);
    return _commit((values) {
      if (lines.isEmpty) {
        values.remove(id);
      } else {
        values[id] = lines;
      }
    });
  }

  Future<void> saveItems(String id, List<TipItem>? items, {String? caption}) {
    if (id != 'daily') {
      return save(id, items?.map((item) => item.text).join('\n') ?? '');
    }
    final daily = items == null ? null : _validateItems(items);
    final captionLines = parse(caption ?? '');
    if (captionLines.length > 1) {
      throw const FormatException('Invalid daily caption');
    }
    return _commit(
      (values) {
        if (captionLines.isEmpty) {
          values.remove('daily-caption');
        } else {
          values['daily-caption'] = captionLines;
        }
      },
      updateDaily: true,
      daily: daily?.isEmpty == true ? null : daily,
    );
  }

  Future<void> _commit(
    void Function(Map<String, List<String>>) change, {
    bool updateDaily = false,
    List<TipItem>? daily,
  }) {
    final result = _tail.then((_) async {
      final values = {..._values};
      change(values);
      final nextDaily = updateDaily ? daily : _daily;
      await store.write(
        jsonEncode({
          'version': 1,
          'text': values.map((id, lines) => MapEntry(id, lines.join('\n'))),
          if (nextDaily != null)
            'daily': [
              for (final item in nextDaily) {'id': item.id, 'text': item.text},
            ],
        }),
      );
      _values = values;
      _daily = nextDaily;
      loadError = null;
      _notify();
    });
    _tail = result.catchError((Object _) {});
    return pendingUiWrites.track(result);
  }

  @override
  void dispose() {
    _disposed = true;
    super.dispose();
  }
}

class TipPreferencesScope extends InheritedNotifier<TipPreferences> {
  const TipPreferencesScope({
    super.key,
    required TipPreferences controller,
    required super.child,
  }) : super(notifier: controller);
  static TipPreferences? maybeOf(BuildContext context) => context
      .dependOnInheritedWidgetOfExactType<TipPreferencesScope>()
      ?.notifier;
}
