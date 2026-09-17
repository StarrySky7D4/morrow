import 'dart:io';
import 'dart:typed_data';
import 'package:flutter/foundation.dart' show listEquals;
import 'package:capnproto_dart/capnproto_dart.dart';
import '../storage.dart';
import 'workbench_native.dart';
import 'generated/studio.capnp.dart' as wire;
import 'generated/identity.dart' as contract;

/// Presentation maps are temporary Flutter adapters. Only typed preferences go
/// across the boundary; individual ideas already belong to core transactions.
class RustStudioStorage implements StudioStorage {
  RustStudioStorage._(this.backend, this._snapshot);
  final RustWorkbench backend;
  Map<String, dynamic> _snapshot;
  static Future<RustStudioStorage> open(RustWorkbench backend) async {
    final bytes = await backend.readPreferences();
    final config = bytes == null
        ? <String, dynamic>{
            'version': 1,
            'theme': 'white',
            'glass': 'frosted',
            'background': 'ambient',
          }
        : decodePreferences(bytes);
    final ideas = await backend.load();
    return RustStudioStorage._(backend, {
      ...config,
      'uiLocale': await backend.readUiLocale(),
      'ideas': ideas.map((i) => i.toJson()).toList(),
    });
  }

  @override
  Map<String, dynamic> read() => _copyValue(_snapshot) as Map<String, dynamic>;
  Future<void> _pending = Future.value();
  @override
  Future<void> write(Map<String, dynamic> data) {
    // Freeze this caller's proposal before any queued or asynchronous work.
    // In particular, changing a track/completion list after write() must not
    // change the original request or the confirmed presentation snapshot.
    late final Uint8List encoded;
    late final String locale;
    late final dynamic ideas;
    try {
      encoded = encodePreferences(data);
      locale = data['uiLocale'] as String? ?? 'system';
      ideas = _copyValue(data['ideas']);
    } catch (error, stack) {
      return Future.error(error, stack);
    }
    final result = _pending.then((_) async {
      await backend.saveUiLocale(locale);
      // Locale remains writable without a guest plugin. Preserve its confirmed
      // value even if an independent appearance write subsequently fails.
      _snapshot = {..._snapshot, 'uiLocale': locale};
      final saved = listEquals(encoded, encodePreferences(_snapshot))
          ? encoded
          : await backend.savePreferences(encoded);
      _snapshot = {
        ...decodePreferences(saved),
        'uiLocale': locale,
        'ideas': ideas,
      };
    });
    _pending = result.catchError((Object _) {});
    return result;
  }

  static dynamic _copyValue(dynamic value) {
    if (value is Map) {
      return <String, dynamic>{
        for (final entry in value.entries)
          entry.key as String: _copyValue(entry.value),
      };
    }
    if (value is List) return value.map(_copyValue).toList();
    return value;
  }
}

void _source(wire.SourceBuilder b, Map<String, dynamic> source) {
  final local = source['local'] as bool;
  b.local = local;
  b.name = source['name'] as String;
  b.kind = source['kind'] as String;
  b.location = local
      ? File(source['location'] as String).uri.toString()
      : source['location'] as String;
}

Map<String, dynamic> _readSource(wire.SourceReader s) => {
  'location': s.local ? Uri.parse(s.location!).toFilePath() : s.location!,
  'name': s.name!,
  'kind': s.kind!,
  'local': s.local,
};
Uint8List encodePreferences(Map<String, dynamic> data) {
  final message = MessageBuilder();
  final b = message.initRoot(wire.preferencesFactory);
  b.version = 1;
  b.digest = Uint8List.fromList(contract.studioDigest);
  final a = b.initAppearance();
  a.theme = data['theme'] as String? ?? 'white';
  a.glass = data['glass'] as String? ?? 'frosted';
  a.background = data['background'] as String? ?? 'ambient';
  a.solidTint = data['solidTint'] as int? ?? 0;
  a.opacity = (data['frostedOpacity'] as num?)?.toDouble() ?? 0.76;
  a.cornerRadius = (data['cornerRadius'] as num?)?.toDouble() ?? 20;
  a.windowRadius = (data['windowRadius'] as num?)?.toDouble() ?? 20;
  a.grayscale = (data['grayscale'] as num?)?.toDouble() ?? 0;
  a.lightness = (data['themeLightness'] as num?)?.toDouble() ?? 0;
  a.customColor = data['customColor'] as int? ?? 0;
  a.themeColor = data['themeColor'] as int? ?? 0;
  a.liquidCanvas = data['liquidCanvas'] as bool? ?? false;
  a.mediaPlaying = data['mediaPlaying'] as bool? ?? true;
  a.sidebarExpanded = data['sidebarExpanded'] as bool? ?? true;
  a.appearanceExpanded = data['appearanceExpanded'] as bool? ?? true;
  a.hasLightness = data['themeLightness'] != null;
  a.hasCustomColor = data['customColor'] != null;
  a.hasThemeColor = data['themeColor'] != null;
  a.canvasBlur = (data['canvasBlur'] as num?)?.toDouble() ?? 0;
  a.canvasOpacity = (data['canvasOpacity'] as num?)?.toDouble() ?? 0;
  a.canvasColor = data['canvasColor'] as int? ?? 0;
  a.hasCanvasColor = data['canvasColor'] != null;
  a.componentCustom = data['componentCustom'] as bool? ?? false;
  a.componentBlur = (data['componentBlur'] as num?)?.toDouble() ?? 22;
  a.componentOpacity = (data['componentOpacity'] as num?)?.toDouble() ?? 0.76;
  a.componentColor = data['componentColor'] as int? ?? 0;
  a.hasComponentColor = data['componentColor'] != null;
  if (data['texture'] != null) {
    b.texturePresent = true;
    _source(b.initTexture(), data['texture'] as Map<String, dynamic>);
  }
  final music = data['music'] as Map<String, dynamic>? ?? {};
  b.index = music['index'] as int? ?? 0;
  b.showLyrics = music['showLyrics'] as bool? ?? false;
  b.onlineLyrics = music['onlineLyrics'] as bool? ?? false;
  final tracks = music['tracks'] as List? ?? [];
  final list = b.initTracks(tracks.length);
  for (var i = 0; i < tracks.length; i++) {
    final t = tracks[i] as Map<String, dynamic>;
    final out = list[i];
    _source(out.initSource(), t['source'] as Map<String, dynamic>);
    if (t['cover'] != null) {
      out.coverPresent = true;
      _source(out.initCover(), t['cover'] as Map<String, dynamic>);
    }
    out.lyrics = t['lyrics'] as String? ?? '';
    out.lyricSource = t['lyricSource'] as String? ?? '';
    out.title = t['trackTitle'] as String? ?? '';
    out.artist = t['artist'] as String? ?? '';
    out.duration = (t['trackDuration'] as num?)?.toDouble() ?? 0;
    out.metadataRead = t['metadataRead'] as bool? ?? false;
  }
  final components = data['componentMaterials'] as Map<String, dynamic>? ?? {};
  final materials = b.initComponents(components.length);
  var componentIndex = 0;
  for (final entry in components.entries) {
    final value = entry.value as Map<String, dynamic>;
    final out = materials[componentIndex++];
    out.id = entry.key;
    out.enabled = value['enabled'] as bool? ?? false;
    out.blur = (value['blur'] as num?)?.toDouble() ?? 22;
    out.opacity = (value['opacity'] as num?)?.toDouble() ?? .76;
    out.hasColor = value['color'] != null;
    out.color = value['color'] as int? ?? 0;
  }
  final done = data['completed'] as List? ?? [];
  final completed = b.initCompleted(done.length);
  for (var i = 0; i < done.length; i++) {
    completed[i] = done[i] as String;
  }
  final bytes = message.serialize();
  if (bytes.length > RustWorkbench.maxPreferencesBytes) {
    throw const FormatException('配置超过 4 MiB，原设置未覆盖。');
  }
  return bytes;
}

Map<String, dynamic> decodePreferences(Uint8List bytes) {
  final r = RustWorkbench.readMessage(
    bytes,
    maxBytes: RustWorkbench.maxPreferencesBytes,
  ).getRoot(wire.preferencesFactory);
  if (r.version != 1 ||
      r.digest == null ||
      r.digest!.length != contract.studioDigest.length ||
      List.generate(
        contract.studioDigest.length,
        (i) => r.digest![i] != contract.studioDigest[i],
      ).any((v) => v)) {
    throw const FormatException('设置协议不匹配');
  }
  final a = r.appearance!;
  return {
    'version': 1,
    'componentMaterials': <String, dynamic>{
      for (final c in r.components ?? <wire.ComponentMaterialReader>[])
        c.id!: <String, dynamic>{
          'enabled': c.enabled,
          'blur': c.blur,
          'opacity': c.opacity,
          'color': c.hasColor ? c.color : null,
        },
    },
    'theme': a.theme,
    'glass': a.glass,
    'background': a.background,
    'solidTint': a.solidTint,
    'frostedOpacity': a.opacity,
    'cornerRadius': a.cornerRadius,
    'windowRadius': a.windowRadius,
    'grayscale': a.grayscale,
    'themeLightness': a.hasLightness ? a.lightness : null,
    'customColor': a.hasCustomColor ? a.customColor : null,
    'themeColor': a.hasThemeColor ? a.themeColor : null,
    'liquidCanvas': a.liquidCanvas,
    'canvasBlur': a.canvasBlur,
    'canvasOpacity': a.canvasOpacity,
    'canvasColor': a.hasCanvasColor ? a.canvasColor : null,
    'componentCustom': a.componentCustom,
    'componentBlur': a.componentBlur,
    'componentOpacity': a.componentOpacity,
    'componentColor': a.hasComponentColor ? a.componentColor : null,

    'mediaPlaying': a.mediaPlaying,
    'sidebarExpanded': a.sidebarExpanded,
    'appearanceExpanded': a.appearanceExpanded,
    'texture': r.texturePresent ? _readSource(r.texture!) : null,
    'completed': [...?r.completed].whereType<String>().toList(),
    'music': {
      'index': r.index,
      'showLyrics': r.showLyrics,
      'onlineLyrics': r.onlineLyrics,
      'tracks': [
        for (final t in r.tracks ?? <wire.TrackReader>[])
          {
            'source': _readSource(t.source!),
            'cover': t.coverPresent ? _readSource(t.cover!) : null,
            'lyrics': t.lyrics ?? '',
            'lyricSource': t.lyricSource ?? '',
            'trackTitle': t.title ?? '',
            'artist': t.artist ?? '',
            'trackDuration': t.duration,
            'metadataRead': t.metadataRead,
          },
      ],
    },
  };
}
