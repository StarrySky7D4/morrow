import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';
import 'package:morrow_studio/main.dart';
import 'package:morrow_studio/plugins/plugin_library.dart';
import 'package:morrow_studio/plugins/studio_backend.dart';
import 'package:morrow_studio/plugins/workbench_backend.dart';
import 'plugin_library_test.dart' as fixture;

const themeId = 'org.morrow.theme.mid-autumn';
const otherThemeId = 'org.example.theme.other';
const themeHandlers = [
  PluginTransformHandler(
    name: 'theme.describe',
    inputType: 'morrow.ui.theme.request.v1',
    outputType: 'morrow.ui.theme.v1',
    maxInputBytes: 1,
    maxOutputBytes: 16384,
  ),
  PluginTransformHandler(
    name: 'theme.artwork',
    inputType: 'morrow.ui.theme.chunk.v1',
    outputType: 'bytes',
    maxInputBytes: 4,
    maxOutputBytes: 32768,
  ),
];
PluginLibraryEntry themeEntry(String id, {bool enabled = false}) =>
    PluginLibraryEntry(
      id: id,
      name: id == themeId ? '中秋 · 月满庭' : '另一主题',
      version: '1.0.0',
      digest: Uint8List.fromList(List.filled(32, id == themeId ? 1 : 2)),
      enabled: enabled,
      builtin: false,
      available: true,
      declared: const [],
      approved: const [],
      dependencies: const [],
      handlers: themeHandlers,
      issue: '',
    );

class ThemeBackend extends fixture.FakeBackend implements WorkbenchBackend {
  ThemeBackend({bool artwork = false, bool installed = true})
    : super([
        if (installed) themeEntry(themeId),
        if (installed) themeEntry(otherThemeId),
        fixture.entry('a', enabled: true, handlers: fixture.uiHandlers),
      ]) {
    final data =
        jsonDecode(File('plugins/mid_autumn/theme.json').readAsStringSync())
            as Map<String, dynamic>;
    if (!artwork) data.remove('artwork');
    descriptions[themeId] = jsonEncode(data);
    descriptions[otherThemeId] = jsonEncode({
      ...data,
      'id': otherThemeId,
      'name': {'en': 'Another theme', 'zh': '另一主题'},
    });
    if (artwork) {
      image = File(
        'plugins/mid_autumn/artwork/moonlit-garden.webp',
      ).readAsBytesSync();
    }
  }
  final descriptions = <String, String>{};
  Uint8List? image;
  bool staleImage = false;
  @override
  bool get writable => true;
  @override
  StudioBackend? get studio => null;
  @override
  Future<Idea> apply(
    PluginAction action,
    Idea idea, {
    String text = '',
    bool flag = false,
  }) async => idea;
  @override
  Future<List<String>> query(
    String section,
    String filter,
    String text,
    String sort, {
    String? operation,
  }) async => [];
  @override
  Future<void> configureExternal(
    PluginLibraryEntry value,
    BigInt revision,
    List<String> approved,
    bool enable,
  ) async {
    if (!value.isTheme) {
      return super.configureExternal(value, revision, approved, enable);
    }
    if (revision != this.revision) throw StateError('stale decision');
    configurations++;
    this.revision += BigInt.one;
    entries = [
      for (final entry in entries)
        if (entry.isTheme)
          themeEntry(
            entry.id,
            enabled: entry.id == value.id
                ? enable
                : (enable ? false : entry.enabled),
          )
        else
          entry,
    ];
  }

  @override
  Future<Uint8List> transformExternal(
    PluginLibraryEntry value,
    BigInt revision,
    PluginTransformHandler handler,
    Uint8List input,
  ) async {
    if (!value.isTheme) {
      return super.transformExternal(value, revision, handler, input);
    }
    if (revision != this.revision ||
        !entries.singleWhere((e) => e.id == value.id).enabled) {
      throw StateError('not authorized');
    }
    if (handler.name == 'theme.describe') {
      return Uint8List.fromList(utf8.encode(descriptions[value.id]!));
    }
    final offset = ByteData.sublistView(input).getUint32(0, Endian.little);
    final result = image!.sublist(
      offset,
      (offset + 32768).clamp(0, image!.length),
    );
    if (staleImage) this.revision += BigInt.one;
    return result;
  }
}
