import 'dart:convert';
import 'dart:typed_data';
import 'dart:ui' as ui;
import 'package:crypto/crypto.dart';
import 'package:flutter/material.dart';
import 'package:shared_preferences/shared_preferences.dart';
import '../plugins/plugin_library.dart';
import 'ui_theme_tokens.dart';

const themeDescriptionHandler = 'theme.describe';

class UiThemePlugin {
  UiThemePlugin._(
    this.id,
    this.names,
    this.descriptions,
    this.captions,
    this.light,
    this.dark,
    this.artworkSpec,
  );
  final String id;
  final Map<String, String> names, descriptions, captions;
  final UiThemeTokens light, dark;
  final Map<String, dynamic>? artworkSpec;
  Uint8List? artwork;
  String name(Locale locale) => names[locale.languageCode] ?? names['en']!;
  String caption(Locale locale) =>
      captions[locale.languageCode] ?? captions['en']!;
  String description(Locale locale) =>
      descriptions[locale.languageCode] ?? descriptions['en']!;
  factory UiThemePlugin.parse(String source, {required String expectedId}) {
    if (utf8.encode(source).length > 16384) {
      throw const FormatException('Theme description budget');
    }
    final data = jsonDecode(source) as Map<String, dynamic>;
    if (data['schemaVersion'] != 1 || data['id'] != expectedId) {
      throw const FormatException('Unsupported UI theme plugin');
    }
    Map<String, String> labels(String key) {
      final value = Map<String, String>.from(data[key] as Map);
      if (value['en']?.isNotEmpty != true ||
          value['zh']?.isNotEmpty != true ||
          value.values.any((s) => s.length > 256)) {
        throw const FormatException('Invalid theme labels');
      }
      return Map.unmodifiable(value);
    }

    final artwork = data['artwork'] as Map<String, dynamic>?;
    if (artwork != null &&
        (artwork['bytes'] is! int ||
            (artwork['bytes'] as int) < 1 ||
            (artwork['bytes'] as int) > 1024 * 1024 ||
            artwork['sha256'] is! String ||
            !RegExp(r'^[a-f0-9]{64}$').hasMatch(artwork['sha256']) ||
            artwork['width'] is! int ||
            artwork['height'] is! int ||
            (artwork['width'] as int) < 1 ||
            (artwork['width'] as int) > 2048 ||
            (artwork['height'] as int) < 1 ||
            (artwork['height'] as int) > 2048)) {
      throw const FormatException('Invalid artwork budget');
    }
    return UiThemePlugin._(
      expectedId,
      labels('name'),
      labels('description'),
      labels('caption'),
      UiThemeTokens.fromJson(data['light'] as Map<String, dynamic>),
      UiThemeTokens.fromJson(data['dark'] as Map<String, dynamic>),
      artwork,
    );
  }
}

abstract interface class ThemePluginStore {
  Future<String?> read();
  Future<void> write(String value);
}

/// Only presentation overrides live here. Install/enable state belongs to the plugin registry.
class LocalThemePluginStore implements ThemePluginStore {
  static const key = 'morrow.ui-theme-options.v2';
  @override
  Future<String?> read() async =>
      (await SharedPreferences.getInstance()).getString(key);
  @override
  Future<void> write(String value) async {
    if (!await (await SharedPreferences.getInstance()).setString(key, value)) {
      throw StateError('UI theme options could not be saved');
    }
  }
}

class MemoryThemePluginStore implements ThemePluginStore {
  String? value;
  @override
  Future<String?> read() async => value;
  @override
  Future<void> write(String value) async => this.value = value;
}

/// Executes bounded pure theme providers through the existing plugin manager.
/// Never grants content/IO privileges, imports a bundled fallback or restores an
/// enabled state from preferences. Registry changes invalidate cached presentation.
class ThemePluginController extends ChangeNotifier {
  ThemePluginController({required this.store, this.backend});
  final ThemePluginStore store;
  final ExternalPluginControl? backend;
  UiThemePlugin? _plugin;
  String? _digest;
  List<PluginLibraryEntry> _entries = [];
  BigInt? _revision;
  Map<String, bool> _overrides = {};
  Object? _error;
  bool _disposed = false;
  int _pending = 0;
  Future<void> _tail = Future.value();
  List<PluginLibraryEntry> get entries => List.unmodifiable(_entries);
  UiThemePlugin? get plugin => _plugin;
  bool get active => _plugin != null;
  bool get fullOverride => active && _overrides[_plugin!.id] == true;
  bool get busy => _pending > 0;
  Object? get error => _error;
  UiThemeTokens? tokens(bool dark) => dark ? _plugin?.dark : _plugin?.light;
  void _notify() {
    if (!_disposed) notifyListeners();
  }

  Future<bool> _serial(Future<void> Function() action) {
    if (_disposed) return Future.value(false);
    _pending++;
    _notify();
    final result = _tail.then((_) async {
      if (_disposed) return false;
      try {
        await action();
        _error = null;
        return true;
      } catch (error) {
        _error = error;
        return false;
      }
    });
    _tail = result.then((_) {
      _pending--;
      _notify();
    });
    return result;
  }

  Future<void> restore() async {
    await _serial(() async {
      try {
        final saved = await store.read();
        if (saved != null) {
          _overrides = Map<String, bool>.from(jsonDecode(saved) as Map);
        }
      } catch (_) {
        _overrides = {};
      }
      await _refresh();
    });
  }

  Future<void> refresh() async {
    await _serial(_refresh);
  }

  Future<void> _refresh() async {
    final api = backend;
    if (api == null) {
      _plugin = null;
      _entries = [];
      return;
    }
    try {
      var page = await api.pluginPage();
      final revision = page.revision;
      final themes = <PluginLibraryEntry>[];
      final cursors = <String>{};
      var total = 0;
      while (true) {
        if (page.revision != revision ||
            (total += page.entries.length) > 1024) {
          throw const FormatException('Invalid plugin catalog');
        }
        themes.addAll(page.entries.where((entry) => entry.isTheme));
        if (page.cursor.isEmpty) break;
        if (!cursors.add(page.cursor)) {
          throw const FormatException('Repeated catalog cursor');
        }
        page = await api.pluginPage(cursor: page.cursor, revision: revision);
      }
      _entries = themes;
      _revision = revision;
      final enabled = themes.where((e) => e.enabled).toList();
      if (enabled.length > 1) throw StateError('Multiple active theme plugins');
      if (enabled.isEmpty) {
        _plugin = null;
        _digest = null;
        return;
      }
      final entry = enabled.single;
      if (!entry.available) throw StateError('Theme plugin unavailable');
      final digest = base64Encode(entry.digest);
      if (_digest == digest && _plugin?.id == entry.id) return;
      // Do not keep a revoked theme visible while its replacement is loading.
      _plugin = null;
      _digest = null;
      _notify();
      final describe = entry.handlers.singleWhere(
        (h) => h.name == themeDescriptionHandler,
      );
      final raw = await api.transformExternal(
        entry,
        revision,
        describe,
        Uint8List(0),
      );
      final theme = UiThemePlugin.parse(utf8.decode(raw), expectedId: entry.id);
      final spec = theme.artworkSpec;
      if (spec != null) {
        final handler = entry.handlers.singleWhere(
          (h) =>
              h.name == 'theme.artwork' &&
              h.inputType == 'morrow.ui.theme.chunk.v1' &&
              h.outputType == 'bytes',
        );
        final bytes = BytesBuilder(copy: false);
        final length = spec['bytes'] as int;
        for (var offset = 0; offset < length; offset += 32768) {
          if (_disposed) return;
          final input = ByteData(4)..setUint32(0, offset, Endian.little);
          final chunk = await api.transformExternal(
            entry,
            revision,
            handler,
            input.buffer.asUint8List(),
          );
          if (chunk.length != (length - offset).clamp(0, 32768)) {
            throw const FormatException('Invalid artwork chunk');
          }
          bytes.add(chunk);
        }
        final image = bytes.takeBytes();
        if (sha256.convert(image).toString() != spec['sha256']) {
          throw const FormatException('Artwork integrity mismatch');
        }
        final buffer = await ui.ImmutableBuffer.fromUint8List(image);
        try {
          final descriptor = await ui.ImageDescriptor.encoded(buffer);
          try {
            if (descriptor.width != spec['width'] ||
                descriptor.height != spec['height']) {
              throw const FormatException('Artwork dimensions mismatch');
            }
          } finally {
            descriptor.dispose();
          }
        } finally {
          buffer.dispose();
        }
        theme.artwork = image;
      }
      // Publication requires a fresh revision after the last asset response.
      if ((await api.pluginPage()).revision != revision) {
        throw StateError('Theme selection changed');
      }
      if (!_disposed) {
        _plugin = theme;
        _digest = digest;
      }
    } catch (_) {
      _plugin = null;
      _digest = null;
      rethrow;
    }
  }

  Future<bool> activate(String id) => _serial(() async {
    await _refresh();
    final entry = _entries.singleWhere((e) => e.id == id);
    // Themes requesting privileges must be approved in the full plugin manager.
    if (!entry.declared.every(entry.approved.contains)) {
      throw StateError('Theme requires approval');
    }
    try {
      await backend!.configureExternal(entry, _revision!, entry.approved, true);
    } finally {
      await _refresh();
    }
  });
  Future<bool> deactivate() => _serial(() async {
    await _refresh();
    final entry = _entries.where((e) => e.enabled).firstOrNull;
    try {
      if (entry != null) {
        await backend!.configureExternal(
          entry,
          _revision!,
          entry.approved,
          false,
        );
      }
    } finally {
      await _refresh();
    }
  });
  Future<bool> setFullOverride(bool value) => _serial(() async {
    if (_plugin == null) return;
    final next = {..._overrides, _plugin!.id: value};
    await store.write(jsonEncode(next));
    _overrides = next;
  });
  @override
  void dispose() {
    _disposed = true;
    super.dispose();
  }
}

class ThemePluginScope extends InheritedNotifier<ThemePluginController> {
  const ThemePluginScope({
    super.key,
    required ThemePluginController controller,
    required super.child,
  }) : super(notifier: controller);
  static ThemePluginController? of(BuildContext context) =>
      context.dependOnInheritedWidgetOfExactType<ThemePluginScope>()?.notifier;
}
