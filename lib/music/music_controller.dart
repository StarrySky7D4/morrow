import '../plugins/studio_backend.dart';
import 'dart:async';
import 'audio_import.dart';
import 'prepared_audio.dart';
import 'package:file_selector/file_selector.dart';
import 'lyrics_service.dart';
import 'track_metadata_native.dart'
    if (dart.library.js_interop) 'track_metadata_web.dart';
import 'package:flutter/foundation.dart';
import 'package:media_kit/media_kit.dart';
import '../media/texture_repository.dart';
import '../media/texture_source.dart';

class LyricLine {
  const LyricLine(this.time, this.text);
  final Duration time;
  final String text;
}

List<LyricLine> parseLyrics(String input) {
  final offset =
      int.tryParse(
        RegExp(r'\[offset:([+-]?\d+)\]').firstMatch(input)?.group(1) ?? '',
      ) ??
      0;
  final stamp = RegExp(r'\[(\d+):(\d{2})(?:[.:](\d{1,3}))?\]');
  final result = <LyricLine>[];
  for (final line in input.split('\n')) {
    final matches = stamp.allMatches(line).toList();
    if (matches.isEmpty) continue;
    final text = line.substring(matches.last.end).trim();
    for (final match in matches) {
      final ms =
          int.parse(match[1]!) * 60000 +
          int.parse(match[2]!) * 1000 +
          int.parse((match[3] ?? '0').padRight(3, '0')) -
          offset;
      result.add(LyricLine(Duration(milliseconds: ms.clamp(0, 1 << 40)), text));
    }
  }
  result.sort((a, b) => a.time.compareTo(b.time));
  return result;
}

class MusicTrack {
  MusicTrack({
    required this.source,
    this.cover,
    this.lyrics = '',
    this.lyricSource = '',
    this.trackTitle = '',
    this.artist = '',
    this.trackDuration = 0,
    this.metadataRead = false,
  });
  final TextureSource source;
  TextureSource? cover;
  String lyrics, lyricSource, trackTitle, artist;
  double trackDuration;
  bool metadataRead;
  String get title => trackTitle.isNotEmpty
      ? trackTitle
      : source.name.replaceFirst(RegExp(r'\.[^.]+$'), '');
  static Future<MusicTrack> import(
    XFile file, {
    Future<PreparedAudio> Function(XFile)? prepare,
  }) async {
    final audio = await (prepare ?? prepareMusicAudio)(file);
    try {
      final original = await readTrackMetadata(file);
      final metadata = identical(audio.file, file)
          ? original
          : await readTrackMetadata(audio.file);
      final sidecar = original.fileLyrics.trim();
      final embedded = metadata.embeddedLyrics.isNotEmpty
          ? metadata.embeddedLyrics
          : original.embeddedLyrics;
      final source = await TextureRepository.importFile(audio.file);
      return MusicTrack(
        source: source,
        metadataRead: true,
        trackTitle: metadata.title.isNotEmpty ? metadata.title : original.title,
        artist: metadata.artist.isNotEmpty ? metadata.artist : original.artist,
        trackDuration: metadata.duration,
        lyrics: sidecar.isNotEmpty ? sidecar : embedded,
        lyricSource: sidecar.isNotEmpty
            ? '歌词文件'
            : embedded.isNotEmpty
            ? '音频内嵌'
            : '',
      );
    } finally {
      await audio.dispose();
    }
  }

  Map<String, dynamic> toJson() => {
    'source': source.toJson(),
    'cover': cover?.toJson(),
    'lyrics': lyrics,
    'lyricSource': lyricSource,
    'trackTitle': trackTitle,
    'artist': artist,
    'trackDuration': trackDuration,
    'metadataRead': metadataRead,
  };
  factory MusicTrack.fromJson(Map<String, dynamic> data) => MusicTrack(
    source: TextureSource.fromJson(data['source'] as Map<String, dynamic>),
    cover: data['cover'] == null
        ? null
        : TextureSource.fromJson(data['cover'] as Map<String, dynamic>),
    lyrics: data['lyrics'] as String? ?? '',
    lyricSource:
        data['lyricSource'] as String? ??
        ((data['lyrics'] as String? ?? '').isNotEmpty ? '歌词文件' : ''),
    trackTitle: data['trackTitle'] as String? ?? '',
    artist: data['artist'] as String? ?? '',
    trackDuration: (data['trackDuration'] as num?)?.toDouble() ?? 0,
    metadataRead: data['metadataRead'] as bool? ?? false,
  );
}

/// A lazy transport keeps the audio engine alive when its controls are hidden.
abstract class MusicTransport {
  Stream<bool> get playing;
  Stream<bool> get completed;
  Stream<Duration> get position;
  Stream<Duration> get duration;
  Stream<String> get errors;
  Future<void> open(String uri);
  Future<void> play();
  Future<void> pause();
  Future<void> stop();
  Future<void> seek(Duration position);
  Future<void> dispose();
}

class MediaKitMusicTransport implements MusicTransport {
  MediaKitMusicTransport() {
    MediaKit.ensureInitialized();
    player = Player();
  }
  late final Player player;
  @override
  Stream<bool> get playing => player.stream.playing;
  @override
  Stream<bool> get completed => player.stream.completed;
  @override
  Stream<Duration> get position => player.stream.position;
  @override
  Stream<Duration> get duration => player.stream.duration;
  @override
  Stream<String> get errors => player.stream.error;
  @override
  Future<void> open(String uri) => player.open(Media(uri), play: false);
  @override
  Future<void> play() => player.play();
  @override
  Future<void> pause() => player.pause();
  @override
  Future<void> stop() => player.stop();
  @override
  Future<void> seek(Duration position) => player.seek(position);
  @override
  Future<void> dispose() => player.dispose();
}

class MusicController extends ChangeNotifier {
  MusicController({
    List<MusicTrack>? tracks,
    int index = 0,
    this.showLyrics = false,
    this.onlineLyrics = false,
    LyricsService? lyricsService,
    this.onSave,
    this.plugin,
    MusicTransport Function()? createTransport,
    Future<ResolvedTexture> Function(TextureSource)? resolve,
  }) : lyricsService = lyricsService ?? LyricsService(),
       tracks = tracks ?? [],
       _createTransport = createTransport ?? MediaKitMusicTransport.new,
       _resolve = resolve ?? TextureRepository.resolve {
    this.index = this.tracks.isEmpty
        ? 0
        : index.clamp(0, this.tracks.length - 1);
    _readLyrics();
  }
  final List<MusicTrack> tracks;
  Object orderRevision = Object();
  final LyricsService lyricsService;
  bool onlineLyrics;
  final _lyricRequests = <MusicTrack>{};
  final _lyricAttempted = <MusicTrack>{};
  final lyricMessages = <MusicTrack, String>{};
  bool get lyricsLoading => _lyricRequests.contains(current);
  String get lyricStatus => lyricsLoading
      ? '正在读取歌词…'
      : lyricMessages[current] ?? current?.lyricSource ?? '';
  late int index;
  bool showLyrics, playing = false, loading = false, blocked = false;
  Duration position = Duration.zero, duration = Duration.zero;
  String? error;
  final VoidCallback? onSave;
  final StudioBackend? plugin;
  int _lyricGeneration = 0, _selectionIntent = 0;
  final MusicTransport Function() _createTransport;
  final Future<ResolvedTexture> Function(TextureSource) _resolve;
  MusicTransport? _transport;
  ResolvedTexture? _resolved;
  final _subscriptions = <StreamSubscription<dynamic>>[];
  List<LyricLine> _lyrics = [];
  int _revision = 0;
  bool _disposed = false, _opened = false;
  Future<void> _pending = Future.value();
  MusicTrack? get current => tracks.isEmpty ? null : tracks[index];
  String? get lyric => lyricForDisplay(untimedLabel: '无时间轴');

  String? lyricForDisplay({required String untimedLabel}) {
    if (_lyrics.isEmpty) {
      final plain = current?.lyrics.trim() ?? '';
      if (plain.isEmpty) return null;
      return '${plain.split('\n').first} · $untimedLabel';
    }
    String line = '♪ ${current?.title ?? ''}';
    for (final item in _lyrics) {
      if (item.time > position) break;
      line = item.text.isEmpty ? '♪' : item.text;
    }
    return line;
  }

  void _readLyrics() {
    final text = current?.lyrics ?? '';
    final service = plugin;
    if (service == null) {
      _lyrics = parseLyrics(text);
      return;
    }
    final generation = ++_lyricGeneration;
    _lyrics = [];
    service
        .parseLyrics(text)
        .then(
          (value) {
            if (!_disposed && generation == _lyricGeneration) {
              _lyrics = value;
              _notify();
            }
          },
          onError: (Object _) {
            if (!_disposed &&
                generation == _lyricGeneration &&
                current != null) {
              lyricMessages[current!] = '歌词解析未完成，可重新导入';
              _notify();
            }
          },
        );
  }

  Future<PlaybackDecision> _policy(
    String action, {
    int value = 0,
    bool flag = false,
  }) => plugin!.playback(
    action,
    count: tracks.length,
    index: index,
    playing: playing,
    blocked: blocked,
    positionMs: position.inMilliseconds,
    durationMs: duration.inMilliseconds,
    value: value,
    flag: flag,
  );

  void _notify() {
    if (!_disposed) notifyListeners();
  }

  void save() {
    onSave?.call();
    _notify();
  }

  void setLyrics(String value, {MusicTrack? track, String source = '歌词文件'}) {
    final selected = track ?? current;
    if (selected == null || !tracks.contains(selected)) return;
    selected.lyrics = value;
    selected.lyricSource = source;
    lyricMessages.remove(selected);
    _readLyrics();
    save();
  }

  void setShowLyrics(bool value) {
    showLyrics = value;
    save();
  }

  void add(List<MusicTrack> values) {
    tracks.addAll(values);
    orderRevision = Object();
    _readLyrics();
    save();
  }

  Map<String, dynamic> toJson() => {
    'tracks': tracks.map((t) => t.toJson()).toList(),
    'index': index,
    'showLyrics': showLyrics,
    'onlineLyrics': onlineLyrics,
  };

  void setOnlineLyrics(bool value) {
    onlineLyrics = value;
    save();
    if (value && current != null) unawaited(loadLyrics(current!, retry: true));
  }

  Future<void> loadLyrics(MusicTrack track, {bool retry = false}) async {
    if (_disposed ||
        track.lyrics.trim().isNotEmpty ||
        _lyricRequests.contains(track) ||
        (!retry && _lyricAttempted.contains(track))) {
      return;
    }
    _lyricRequests.add(track);
    _lyricAttempted.add(track);
    _notify();
    try {
      if (!track.metadataRead && track.source.local) {
        final resolved = await _resolve(track.source);
        try {
          final metadata = await readTrackMetadata(
            XFile(
              resolved.uri.startsWith('file:')
                  ? Uri.parse(resolved.uri).toFilePath()
                  : resolved.uri,
              name: track.source.name,
            ),
          );
          if (_disposed || !tracks.contains(track)) return;
          track.metadataRead = true;
          track.trackTitle = metadata.title;
          track.artist = metadata.artist;
          track.trackDuration = metadata.duration;
          final value = metadata.fileLyrics.trim().isNotEmpty
              ? metadata.fileLyrics
              : metadata.embeddedLyrics;
          if (value.trim().isNotEmpty && track.lyrics.trim().isEmpty) {
            setLyrics(
              value,
              track: track,
              source: metadata.fileLyrics.trim().isNotEmpty ? '歌词文件' : '音频内嵌',
            );
          }
        } finally {
          resolved.release?.call();
        }
      }
      if (_disposed ||
          !tracks.contains(track) ||
          track.lyrics.trim().isNotEmpty ||
          !onlineLyrics) {
        return;
      }
      final results = await lyricsService.search(track.title, track.artist);
      if (_disposed ||
          !tracks.contains(track) ||
          !onlineLyrics ||
          track.lyrics.trim().isNotEmpty) {
        return;
      }
      final candidateIndex = plugin == null
          ? null
          : await plugin!.matchLyrics(
              results,
              track.title,
              track.artist,
              track.trackDuration,
            );
      if (_disposed ||
          !tracks.contains(track) ||
          !onlineLyrics ||
          track.lyrics.trim().isNotEmpty) {
        return;
      }
      final match = plugin == null
          ? LyricsService.exactMatch(
              results,
              track.title,
              track.artist,
              track.trackDuration,
            )
          : candidateIndex == null
          ? null
          : results[candidateIndex];
      if (match != null) {
        setLyrics(
          match.lyrics,
          track: track,
          source: 'LRCLIB · ${match.artist}',
        );
      } else {
        lyricMessages[track] = results.isEmpty
            ? '未找到歌词，可导入或重新搜索'
            : '存在多个版本，请在搜索中选择';
      }
    } catch (_) {
      if (!_disposed && tracks.contains(track)) {
        lyricMessages[track] = '歌词读取失败，可手动导入或重试';
      }
    } finally {
      _lyricRequests.remove(track);
      if (!_disposed) {
        _readLyrics();
        save();
      }
    }
  }

  Future<void> _run(Future<void> Function() work) {
    _pending = _pending.then((_) async {
      if (_disposed) return;
      try {
        await work();
      } catch (_) {
        error = '歌曲无法播放，请检查文件或更换音频格式。';
        playing = loading = false;
        _opened = false;
        try {
          await _transport?.pause();
        } catch (_) {}
        _notify();
      }
    });
    return _pending;
  }

  MusicTransport _engine() {
    if (_transport != null) return _transport!;
    final engine = _transport = _createTransport();
    _subscriptions.addAll([
      engine.playing.listen((value) {
        playing = value && !blocked;
        _notify();
      }),
      engine.position.listen((value) {
        final previousSecond = position.inSeconds;
        final previousLyric = lyric;
        position = value;
        if (position.inSeconds != previousSecond || lyric != previousLyric) {
          _notify();
        }
      }),
      engine.duration.listen((value) {
        duration = value;
        _notify();
      }),
      engine.completed.listen((value) {
        if (value && !loading && !blocked && error == null) next();
      }),
      engine.errors.listen((value) {
        error = '歌曲无法播放，请检查文件或更换音频格式。';
        playing = loading = false;
        _opened = false;
        _notify();
      }),
    ]);
    return engine;
  }

  Future<void> select(int value, {bool play = true}) async {
    if (tracks.isEmpty || _disposed) return;
    final intent = ++_selectionIntent;
    if (plugin != null) {
      try {
        final decision = await _policy('select', value: value, flag: play);
        if (_disposed || intent != _selectionIntent || tracks.isEmpty) return;
        index = decision.index;
        play = decision.playing;
      } catch (_) {
        error = '播放请求未能完成，请重试。';
        _notify();
        return;
      }
    } else {
      index = value % tracks.length;
    }
    final track = current!;
    final revision = ++_revision;
    loading = true;
    playing = false;
    error = null;
    _opened = false;
    position = duration = Duration.zero;
    _readLyrics();
    save();
    unawaited(loadLyrics(track));
    return _run(() async {
      if (revision != _revision) return;
      final engine = _engine();
      await engine.stop();
      _resolved?.release?.call();
      _resolved = null;
      final source = await _resolve(track.source);
      if (_disposed || revision != _revision) {
        source.release?.call();
        return;
      }
      _resolved = source;
      await engine.open(source.uri).timeout(const Duration(seconds: 20));
      if (_disposed || revision != _revision) return;
      _opened = error == null;
      loading = false;
      if (play && !blocked && error == null) await engine.play();
      _notify();
    });
  }

  Future<void> toggle() async {
    if (blocked || current == null) return;
    if (plugin != null) {
      try {
        final decision = await _policy('toggle');
        if (!decision.playing) {
          await pause();
          return;
        }
      } catch (_) {
        error = '播放请求未能完成，请重试。';
        _notify();
        return;
      }
    } else if (playing) {
      return pause();
    }
    if (!_opened) return select(index);
    return _run(() async {
      if (!blocked) await _engine().play();
    });
  }

  Future<void> pause() {
    playing = false;
    _notify();
    return _run(() async {
      await _transport?.pause();
    });
  }

  Future<void> setBlocked(bool value) async {
    blocked = value;
    if (value) await pause();
    if (plugin != null) {
      try {
        await _policy('block', flag: value);
      } catch (_) {
        if (!_disposed) {
          error = '声音状态未能确认，请重试。';
        }
      }
    }
    _notify();
  }

  Future<void> next() => select(index + 1);
  Future<void> previous() => select(index - 1);
  void reorder(MusicTrack source, MusicTrack target, bool after) {
    if (_disposed ||
        blocked ||
        loading ||
        identical(source, target) ||
        !tracks.contains(source) ||
        !tracks.contains(target)) {
      return;
    }
    final selected = current;
    ++_selectionIntent;
    tracks.remove(source);
    tracks.insert(tracks.indexOf(target) + (after ? 1 : 0), source);
    index = selected == null ? 0 : tracks.indexOf(selected);
    orderRevision = Object();
    // The same transport and track stay active: no reopen, seek or play call.
    save();
  }

  Future<void> seek(Duration value) => _run(() async {
    if (plugin != null) {
      final decision = await _policy('seek', value: value.inMilliseconds);
      value = Duration(milliseconds: decision.positionMs);
    }
    await _transport?.seek(value);
  });
  Future<void> remove(int value) async {
    if (value < 0 || value >= tracks.length) return;
    final wasCurrent = value == index, resume = playing;
    final target = tracks[value];
    final previousTracks = List<MusicTrack>.of(tracks);
    PlaybackDecision? decision;
    if (plugin != null) {
      try {
        decision = await _policy('remove', value: value);
      } catch (_) {
        error = '播放列表未能更新，请重试。';
        _notify();
        return;
      }
      if (_disposed ||
          value >= tracks.length ||
          !identical(tracks[value], target) ||
          !listEquals(tracks, previousTracks)) {
        return;
      }
    }
    ++_selectionIntent;
    tracks.removeAt(value);
    orderRevision = Object();
    if (decision != null) {
      index = decision.index;
    } else if (value < index || index >= tracks.length) {
      index = (index - 1).clamp(0, tracks.isEmpty ? 0 : tracks.length - 1);
    }
    if (wasCurrent) {
      if (tracks.isNotEmpty) {
        await select(index, play: resume);
      } else {
        ++_revision;
        _opened = false;
        playing = loading = false;
        position = duration = Duration.zero;
        error = null;
        await _run(() async {
          await _transport?.stop();
          _resolved?.release?.call();
          _resolved = null;
        });
      }
    }
    _readLyrics();
    save();
  }

  @override
  void dispose() {
    _disposed = true;
    ++_revision;
    for (final subscription in _subscriptions) {
      subscription.cancel();
    }
    _pending.whenComplete(() async {
      await _transport?.dispose();
      _resolved?.release?.call();
    });
    super.dispose();
  }
}
