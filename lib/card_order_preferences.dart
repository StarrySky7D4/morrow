import 'dart:convert';
import 'pending_ui_writes.dart';
import 'package:flutter/material.dart';
import 'package:shared_preferences/shared_preferences.dart';
import 'tip_preferences.dart' show TipStore;
import 'hold_reorder.dart' show moveRelative;

class LocalCardOrderStore implements TipStore {
  LocalCardOrderStore(String? library)
    : key = 'morrow.card-order.v1:${Uri.encodeComponent(library ?? 'default')}';
  final String key;
  @override
  Future<String?> read() async =>
      (await SharedPreferences.getInstance()).getString(key);
  @override
  Future<void> write(String value) async {
    if (!await (await SharedPreferences.getInstance()).setString(key, value)) {
      throw StateError('Card order could not be saved');
    }
  }
}

class CardOrderPreferences extends ChangeNotifier {
  CardOrderPreferences(this.store);
  final TipStore store;
  Map<String, List<String>> _orders = {};
  Set<String> _manual = {};
  bool loading = false, busy = false, _disposed = false;
  Object revision = Object();
  Object? error;
  bool manual(String page) => _manual.contains(page);
  void _notify() {
    revision = Object();
    if (!_disposed) notifyListeners();
  }

  Future<void> restore() async {
    loading = true;
    _notify();
    try {
      final raw = await store.read();
      if (raw != null) {
        final data = jsonDecode(raw) as Map<String, dynamic>;
        if (data['version'] != 1) {
          throw const FormatException('Card order version');
        }
        final orders = (data['orders'] as Map<String, dynamic>).map(
          (key, value) => MapEntry(
            key,
            List<String>.unmodifiable((value as List).cast<String>()),
          ),
        );
        if (orders.length > 5 ||
            orders.values.any(
              (v) => v.length > 10000 || v.toSet().length != v.length,
            )) {
          throw const FormatException('Card order limit');
        }
        _orders = orders;
        _manual = (data['manual'] as List).cast<String>().toSet();
      }
      error = null;
    } catch (e) {
      error = e;
    } finally {
      loading = false;
      _notify();
    }
  }

  List<String> order(String page, Iterable<String> ids) {
    final remaining = ids.toSet();
    return [
      for (final id in _orders[page] ?? <String>[])
        if (remaining.remove(id)) id,
      ...remaining,
    ];
  }

  Future<void> _save(
    Map<String, List<String>> orders,
    Set<String> manual,
  ) async {
    if (busy || loading) return;
    final previousOrders = _orders;
    final previousManual = _manual;
    _orders = orders;
    _manual = manual;
    busy = true;
    _notify();
    try {
      await pendingUiWrites.track(
        store.write(
          jsonEncode({
            'version': 1,
            'orders': orders,
            'manual': manual.toList(),
          }),
        ),
      );
      error = null;
    } catch (e) {
      _orders = previousOrders;
      _manual = previousManual;
      error = e;
      rethrow;
    } finally {
      busy = false;
      _notify();
    }
  }

  Future<void> select(String page, bool enabled) => manual(page) == enabled
      ? Future.value()
      : _save(
          {..._orders},
          {..._manual}
            ..remove(page)
            ..addAll(enabled ? [page] : []),
        );

  Future<void> move(
    String page,
    String source,
    String target,
    bool after, {
    required List<String> all,
    required List<String> visible,
  }) async {
    if (busy ||
        loading ||
        !visible.contains(source) ||
        !visible.contains(target) ||
        source == target) {
      return;
    }
    final full = order(page, all);
    if (visible.toSet().length != visible.length ||
        !full.toSet().containsAll(visible)) {
      return;
    }
    final changed = moveRelative(visible, source, target, after);
    final shown = visible.toSet();
    var index = 0;
    // Replace only visible slots; filtered-out cards retain their positions.
    final merged = [
      for (final id in full) shown.contains(id) ? changed[index++] : id,
    ];
    if (merged.length > 10000) throw const FormatException('Card order limit');
    await _save(
      {..._orders, page: List.unmodifiable(merged)},
      {..._manual, page},
    );
  }

  @override
  void dispose() {
    _disposed = true;
    super.dispose();
  }
}

class CardOrderScope extends InheritedNotifier<CardOrderPreferences> {
  const CardOrderScope({
    super.key,
    required CardOrderPreferences controller,
    required super.child,
  }) : super(notifier: controller);
  static CardOrderPreferences? maybeOf(BuildContext context) =>
      context.dependOnInheritedWidgetOfExactType<CardOrderScope>()?.notifier;
}
