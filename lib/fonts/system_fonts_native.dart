import 'dart:io';
import 'package:flutter/services.dart';

import 'system_fonts.dart';

const List<SystemFontEntry> _windowsFallbackFonts = [
  SystemFontEntry(family: 'Segoe UI'),
  SystemFontEntry(family: 'Microsoft YaHei', displayName: '微软雅黑'),
  SystemFontEntry(family: 'Microsoft YaHei UI'),
  SystemFontEntry(family: 'SimSun', displayName: '宋体'),
  SystemFontEntry(family: 'SimHei', displayName: '黑体'),
  SystemFontEntry(family: 'KaiTi', displayName: '楷体'),
  SystemFontEntry(family: 'FangSong', displayName: '仿宋'),
  SystemFontEntry(family: 'DengXian', displayName: '等线'),
  SystemFontEntry(family: 'Arial'),
  SystemFontEntry(family: 'Calibri'),
  SystemFontEntry(family: 'Cambria'),
  SystemFontEntry(family: 'Cascadia Code'),
  SystemFontEntry(family: 'Consolas'),
  SystemFontEntry(family: 'Courier New'),
  SystemFontEntry(family: 'Georgia'),
  SystemFontEntry(family: 'Tahoma'),
  SystemFontEntry(family: 'Times New Roman'),
  SystemFontEntry(family: 'Trebuchet MS'),
  SystemFontEntry(family: 'Verdana'),
];

const List<SystemFontEntry> _appleFallbackFonts = [
  SystemFontEntry(family: 'PingFang SC'),
  SystemFontEntry(family: 'SF Pro'),
  SystemFontEntry(family: 'Helvetica Neue'),
  SystemFontEntry(family: 'Hiragino Sans'),
  SystemFontEntry(family: 'Menlo'),
  SystemFontEntry(family: 'Georgia'),
  SystemFontEntry(family: 'Arial'),
];

const List<SystemFontEntry> _defaultFallbackFonts = [
  SystemFontEntry(family: 'Segoe UI'),
  SystemFontEntry(family: 'Microsoft YaHei'),
  SystemFontEntry(family: 'Noto Sans SC'),
  SystemFontEntry(family: 'Arial'),
  SystemFontEntry(family: 'Georgia'),
  SystemFontEntry(family: 'Consolas'),
];

List<SystemFontEntry> _fallbackFonts() {
  if (Platform.isWindows) return _windowsFallbackFonts;
  if (Platform.isMacOS || Platform.isIOS) return _appleFallbackFonts;
  return _defaultFallbackFonts;
}

Future<List<SystemFontEntry>> fetchSystemFonts({
  String? locale,
  bool refresh = false,
}) async {
  try {
    final raw = await SystemFontCatalog.channel.invokeListMethod<dynamic>(
      'listSystemFonts',
      {'locale': locale ?? '', 'refresh': refresh},
    );
    final parsed = SystemFontCatalog.parseChannelPayload(raw);
    if (parsed.isNotEmpty) return parsed;
  } on MissingPluginException {
    // Headless test environments or platforms without native enumeration fall back below.
  } on PlatformException {
    // Native enumeration unavailable; fall back cleanly.
  }

  if (Platform.isLinux) {
    try {
      final result = await Process.run(
        'fc-list',
        const [':', 'family'],
      ).timeout(const Duration(seconds: 2));
      if (result.exitCode == 0 && result.stdout is String) {
        final entries = <SystemFontEntry>[];
        for (final line in (result.stdout as String).split('\n')) {
          final primary = line.split(',').first.trim();
          if (primary.isNotEmpty) {
            entries.add(SystemFontEntry(family: primary));
          }
        }
        final normalized = SystemFontCatalog.normalize(entries);
        if (normalized.isNotEmpty) return normalized;
      }
    } catch (_) {
      // Ignore missing fontconfig binary and use curated fallback.
    }
  }

  return _fallbackFonts();
}
