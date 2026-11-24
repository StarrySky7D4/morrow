import 'package:flutter/services.dart';

import 'font_choice.dart';
import 'system_fonts_native.dart'
    if (dart.library.js_interop) 'system_fonts_web.dart'
    as impl;

class SystemFontEntry {
  const SystemFontEntry({required this.family, String? displayName})
    : displayName = displayName ?? family;

  final String family;
  final String displayName;

  bool get hasDistinctDisplayName =>
      displayName.isNotEmpty &&
      displayName.toLowerCase() != family.toLowerCase();

  bool matchesQuery(String query) {
    final normalized = query.trim().toLowerCase();
    if (normalized.isEmpty) return true;
    return displayName.toLowerCase().contains(normalized) ||
        family.toLowerCase().contains(normalized);
  }

  @override
  bool operator ==(Object other) =>
      other is SystemFontEntry &&
      family == other.family &&
      displayName == other.displayName;

  @override
  int get hashCode => Object.hash(family, displayName);
}

class SystemFontCatalog {
  static const channel = MethodChannel('morrow/system_fonts');
  static final Map<String, List<SystemFontEntry>> _cache = {};

  static void clearCache() => _cache.clear();

  static List<SystemFontEntry> normalize(Iterable<SystemFontEntry> raw) {
    final byFamily = <String, SystemFontEntry>{};
    for (final entry in raw) {
      final family = entry.family.trim();
      if (family.isEmpty || family.startsWith('@')) continue;
      try {
        FontChoice(family: family).validate();
      } on FormatException {
        continue;
      }
      final key = family.toLowerCase();
      if (byFamily.containsKey(key)) continue;
      final trimmedDisplay = entry.displayName.trim();
      final displayName = trimmedDisplay.isEmpty ? family : trimmedDisplay;
      byFamily[key] = SystemFontEntry(
        family: family,
        displayName: displayName,
      );
    }
    final list = byFamily.values.toList()
      ..sort((a, b) {
        final byDisplay = a.displayName.toLowerCase().compareTo(
          b.displayName.toLowerCase(),
        );
        if (byDisplay != 0) return byDisplay;
        return a.family.toLowerCase().compareTo(b.family.toLowerCase());
      });
    return list;
  }

  static List<SystemFontEntry> parseChannelPayload(List<dynamic>? raw) {
    if (raw == null || raw.isEmpty) return const [];
    final parsed = <SystemFontEntry>[];
    for (final item in raw) {
      if (item is Map) {
        final family = item['family'];
        if (family is String) {
          final display = item['displayName'];
          parsed.add(
            SystemFontEntry(
              family: family,
              displayName: display is String ? display : family,
            ),
          );
        }
      } else if (item is String) {
        parsed.add(SystemFontEntry(family: item));
      }
    }
    return normalize(parsed);
  }

  static Future<List<SystemFontEntry>> load({
    String? locale,
    bool refresh = false,
  }) async {
    final key = (locale ?? '').trim().toLowerCase();
    if (!refresh && _cache.containsKey(key)) {
      return _cache[key]!;
    }
    final raw = await impl.fetchSystemFonts(locale: locale, refresh: refresh);
    final normalized = List<SystemFontEntry>.unmodifiable(normalize(raw));
    if (normalized.isNotEmpty) {
      _cache[key] = normalized;
    }
    return normalized;
  }
}
