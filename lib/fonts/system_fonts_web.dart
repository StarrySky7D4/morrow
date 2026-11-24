import 'system_fonts.dart';

const List<SystemFontEntry> _webFallbackFonts = [
  SystemFontEntry(family: 'Segoe UI'),
  SystemFontEntry(family: 'Microsoft YaHei'),
  SystemFontEntry(family: 'Noto Sans SC'),
  SystemFontEntry(family: 'Arial'),
  SystemFontEntry(family: 'Georgia'),
  SystemFontEntry(family: 'Consolas'),
];

Future<List<SystemFontEntry>> fetchSystemFonts({
  String? locale,
  bool refresh = false,
}) async => _webFallbackFonts;
