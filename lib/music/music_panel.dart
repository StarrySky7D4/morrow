import 'package:morrow_i18n/morrow_i18n.dart';
import 'audio_formats.dart';
import 'lyrics_dialog.dart';
import 'lyrics_service.dart';
import 'package:file_selector/file_selector.dart';
import 'package:flutter/material.dart';
import 'package:flutter/foundation.dart';
import '../appearance.dart';
import '../media/texture_repository.dart';
import '../media/texture_source.dart';
import 'music_controller.dart';

enum _ImportFailure implements Exception {
  audioType,
  lyricType,
  lyricSize,
  lyricEmpty,
}

class MusicPanel extends StatefulWidget {
  const MusicPanel({super.key, required this.controller});
  final MusicController controller;
  @override
  State<MusicPanel> createState() => _MusicPanelState();
}

class _MusicPanelState extends State<MusicPanel> {
  bool get androidPicker =>
      !kIsWeb && defaultTargetPlatform == TargetPlatform.android;
  List<String> get audioExtensions => [
    ...standardAudioExtensions,
    if (kIsWeb || defaultTargetPlatform == TargetPlatform.windows)
      ...containerAudioFormats.keys,
    'lrc',
  ];
  bool expanded = false, importing = false;
  MusicController get music => widget.controller;
  Future<void> pick(String type) async {
    if (importing) return;
    final labels = L10n.of(context);
    setState(() => importing = true);
    final selected = music.current;
    try {
      if (type == 'songs') {
        final files = await openFiles(
          // Android providers frequently have no MIME mapping for .lrc.
          // Show all files there, then validate names before importing.
          acceptedTypeGroups: androidPicker
              ? const []
              : [
                  XTypeGroup(
                    label: labels.visualMusic,
                    extensions: audioExtensions,
                  ),
                ],
        );
        final allowed = audioExtensions;
        if (files.any(
          (file) => !allowed.contains(file.name.toLowerCase().split('.').last),
        )) {
          throw _ImportFailure.audioType;
        }
        final lyrics = <String, String>{};
        for (final file in files.where(
          (f) => f.name.toLowerCase().endsWith('.lrc'),
        )) {
          if (await file.length() <= 1024 * 1024) {
            lyrics[file.name
                .split(RegExp(r'[/\\]'))
                .last
                .replaceFirst(RegExp(r'\.[^.]+$'), '')
                .toLowerCase()] = await file
                .readAsString();
          }
        }
        for (final file in files.where(
          (f) => !f.name.toLowerCase().endsWith('.lrc'),
        )) {
          await music.plugin?.validateImport('audio', await file.length());
          final track = await MusicTrack.import(file);
          final sidecar =
              lyrics[file.name
                  .split(RegExp(r'[/\\]'))
                  .last
                  .replaceFirst(RegExp(r'\.[^.]+$'), '')
                  .toLowerCase()];
          if (sidecar != null && sidecar.trim().isNotEmpty) {
            track.lyrics = sidecar;
            track.lyricSource = '歌词文件';
          }
          if (!mounted) {
            await TextureRepository.remove(track.source);
            return;
          }
          music.add([track]);
        }
      } else if (selected != null) {
        final file = await openFile(
          acceptedTypeGroups: androidPicker && type == 'lyrics'
              ? const []
              : [
                  type == 'lyrics'
                      ? XTypeGroup(
                          label: labels.visualLyricsFile,
                          extensions: ['lrc', 'txt'],
                        )
                      : XTypeGroup(
                          label: labels.visualSongCover,
                          extensions: ['png', 'jpg', 'jpeg', 'webp'],
                        ),
                ],
        );
        if (file == null || !mounted) return;
        if (type == 'lyrics') {
          if (![
            'lrc',
            'txt',
          ].contains(file.name.toLowerCase().split('.').last)) {
            throw _ImportFailure.lyricType;
          }
          if (await file.length() > 1024 * 1024) {
            throw _ImportFailure.lyricSize;
          }
          final lyrics = await file.readAsString();
          if (lyrics.trim().isEmpty) {
            throw _ImportFailure.lyricEmpty;
          }
          if (!mounted || !music.tracks.contains(selected)) return;
          music.setLyrics(lyrics, track: selected);
        } else {
          await music.plugin?.validateImport('image', await file.length());
          final cover = await TextureRepository.importFile(file);
          if (!mounted || !music.tracks.contains(selected)) return;
          selected.cover = cover;
          music.save();
        }
      }
    } catch (error) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(
            content: Text(switch (error) {
              _ImportFailure.audioType => L10n.of(context).visualChooseAudio,
              _ImportFailure.lyricType => L10n.of(context).visualChooseLyrics,
              _ImportFailure.lyricSize => L10n.of(context).visualLyricsSize,
              _ImportFailure.lyricEmpty => L10n.of(context).visualLyricsEmpty,
              _ => L10n.of(context).visualImportFailure,
            }),
          ),
        );
      }
    } finally {
      if (mounted) setState(() => importing = false);
    }
  }

  Future<void> searchLyrics() async {
    final selected = music.current;
    if (selected == null) return;
    final match = await showStudioDialog<LyricMatch>(
      context: context,
      builder: (_) => LyricsSearchDialog(
        title: selected.title,
        artist: selected.artist,
        service: music.lyricsService,
      ),
    );
    if (!mounted || match == null || !music.tracks.contains(selected)) return;
    music.setLyrics(
      match.lyrics,
      track: selected,
      source: 'LRCLIB · ${match.artist}',
    );
  }

  Future<void> showAllLyrics() async {
    final track = music.current;
    if (track == null) return;
    await showStudioDialog<void>(
      context: context,
      builder: (_) => StudioDialog(
        title: track.title,
        subtitle: track.lyricSource.isEmpty
            ? L10n.of(context).visualNoLyricsRead
            : musicSourceLabel(context, track.lyricSource),
        content: SizedBox(
          width: 420,
          child: SingleChildScrollView(
            child: SelectableText(
              track.lyrics.isEmpty
                  ? L10n.of(context).visualLyricsImportHint
                  : track.lyrics,
              style: const TextStyle(height: 1.8),
            ),
          ),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context),
            child: Text(L10n.of(context).visualClose),
          ),
        ],
      ),
    );
  }

  String clock(Duration value) =>
      '${value.inMinutes}:${(value.inSeconds % 60).toString().padLeft(2, '0')}';
  @override
  Widget build(BuildContext context) {
    final p = AppearanceScope.of(context);
    return ListenableBuilder(
      listenable: music,
      builder: (context, _) => Glass(
        componentId: 'music',
        p: p,
        radius: 22,
        child: Padding(
          padding: const EdgeInsets.all(16),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Row(
                children: [
                  Icon(Icons.graphic_eq_rounded, size: 17, color: p.accent),
                  const SizedBox(width: 7),
                  Expanded(
                    child: Text(
                      L10n.of(context).visualMusicPlayer,
                      style: TextStyle(
                        color: p.ink,
                        fontSize: 12,
                        fontWeight: FontWeight.w600,
                      ),
                    ),
                  ),
                  IconButton(
                    key: const ValueKey('music-add'),
                    tooltip: L10n.of(context).visualImportMusic,
                    onPressed: importing ? null : () => pick('songs'),
                    icon: const Icon(Icons.add_rounded, size: 18),
                  ),
                ],
              ),
              const SizedBox(height: 8),
              Row(
                children: [
                  Tooltip(
                    message: L10n.of(context).visualChangeCover,
                    child: InkWell(
                      onTap: music.current == null || importing
                          ? null
                          : () => pick('cover'),
                      borderRadius: p.borderRadius(12),
                      child: ClipRRect(
                        borderRadius: p.borderRadius(12),
                        child: SizedBox(
                          width: 48,
                          height: 48,
                          child: TrackCover(source: music.current?.cover),
                        ),
                      ),
                    ),
                  ),
                  const SizedBox(width: 10),
                  Expanded(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(
                          music.current?.title ??
                              L10n.of(context).visualMusicEmptyTitle,
                          maxLines: 2,
                          overflow: TextOverflow.ellipsis,
                          style: TextStyle(
                            fontSize: 11,
                            color: p.ink,
                            height: 1.4,
                          ),
                        ),
                        const SizedBox(height: 4),
                        Text(
                          music.loading
                              ? L10n.of(context).visualLoading
                              : music.current == null
                              ? L10n.of(context).visualImportMusicHint
                              : L10n.of(context).visualPlaybackPosition(
                                  music.tracks.length,
                                  music.index + 1,
                                  music.playing
                                      ? L10n.of(context).visualPlaying
                                      : L10n.of(context).visualPaused,
                                ),
                          style: TextStyle(fontSize: 9, color: p.muted),
                        ),
                      ],
                    ),
                  ),
                ],
              ),
              const SizedBox(height: 12),
              Row(
                mainAxisAlignment: MainAxisAlignment.spaceBetween,
                children: [
                  IconButton(
                    key: const ValueKey('music-previous'),
                    tooltip: L10n.of(context).visualPreviousTrack,
                    onPressed: music.current == null ? null : music.previous,
                    icon: const Icon(Icons.skip_previous_rounded),
                  ),
                  IconButton.filledTonal(
                    key: const ValueKey('music-play'),
                    tooltip: music.playing
                        ? L10n.of(context).visualPauseMusic
                        : L10n.of(context).visualPlayMusic,
                    onPressed:
                        music.current == null || music.loading || music.blocked
                        ? null
                        : music.toggle,
                    icon: Icon(
                      music.playing
                          ? Icons.pause_rounded
                          : Icons.play_arrow_rounded,
                    ),
                  ),
                  IconButton(
                    key: const ValueKey('music-next'),
                    tooltip: L10n.of(context).visualNextTrack,
                    onPressed: music.current == null ? null : music.next,
                    icon: const Icon(Icons.skip_next_rounded),
                  ),
                  IconButton(
                    key: const ValueKey('music-list-toggle'),
                    tooltip: expanded
                        ? L10n.of(context).visualCollapsePlaylist
                        : L10n.of(context).visualExpandPlaylist,
                    onPressed: () => setState(() => expanded = !expanded),
                    icon: Icon(
                      Icons.queue_music_rounded,
                      color: expanded ? p.accent : p.muted,
                      size: 21,
                    ),
                  ),
                ],
              ),
              if (music.current != null) ...[
                SliderTheme(
                  data: SliderTheme.of(context).copyWith(
                    trackHeight: 2,
                    thumbShape: const RoundSliderThumbShape(
                      enabledThumbRadius: 4,
                    ),
                    overlayShape: const RoundSliderOverlayShape(
                      overlayRadius: 10,
                    ),
                  ),
                  child: Slider(
                    key: const ValueKey('music-seek'),
                    value: music.position.inMilliseconds.toDouble().clamp(
                      0,
                      music.duration.inMilliseconds.toDouble().clamp(
                        1,
                        double.infinity,
                      ),
                    ),
                    max: music.duration.inMilliseconds.toDouble().clamp(
                      1,
                      double.infinity,
                    ),
                    onChanged: music.duration == Duration.zero
                        ? null
                        : (value) =>
                              music.seek(Duration(milliseconds: value.round())),
                  ),
                ),
                Row(
                  mainAxisAlignment: MainAxisAlignment.spaceBetween,
                  children: [
                    Text(
                      clock(music.position),
                      style: TextStyle(fontSize: 9, color: p.muted),
                    ),
                    Text(
                      clock(music.duration),
                      style: TextStyle(fontSize: 9, color: p.muted),
                    ),
                  ],
                ),
                const SizedBox(height: 7),
                Row(
                  children: [
                    Expanded(
                      child: Text(
                        L10n.of(context).visualFooterLyrics,
                        style: TextStyle(fontSize: 10, color: p.muted),
                      ),
                    ),
                    Switch(
                      key: const ValueKey('music-lyrics-toggle'),
                      value: music.showLyrics,
                      onChanged: music.setShowLyrics,
                    ),
                  ],
                ),
                Semantics(
                  container: true,
                  child: Text(
                    music.showLyrics && music.current!.lyrics.isEmpty
                        ? L10n.of(context).visualPlaylistLyricsHint
                        : L10n.of(context).visualPlaylistSaved,
                    style: TextStyle(fontSize: 9, color: p.muted),
                  ),
                ),
              ],
              if (music.error != null)
                Padding(
                  padding: const EdgeInsets.only(top: 8),
                  child: Text(
                    musicErrorLabel(context, music.error!),
                    style: TextStyle(
                      fontSize: 10,
                      color: Theme.of(context).colorScheme.error,
                    ),
                  ),
                ),
              if (importing) const LinearProgressIndicator(minHeight: 2),
              SoftSize(
                duration: motionDuration(context, 250),
                alignment: Alignment.topCenter,
                child: expanded
                    ? Column(
                        key: const ValueKey('music-playlist'),
                        children: [
                          Divider(color: p.line, height: 24),
                          if (music.tracks.isEmpty)
                            Padding(
                              padding: const EdgeInsets.only(bottom: 8),
                              child: Text(
                                L10n.of(context).visualPlaylistEmpty,
                                style: TextStyle(color: p.muted, fontSize: 10),
                              ),
                            ),
                          ConstrainedBox(
                            constraints: const BoxConstraints(maxHeight: 240),
                            child: ListView.builder(
                              shrinkWrap: true,
                              primary: false,
                              itemCount: music.tracks.length,
                              itemBuilder: (context, index) => ListTile(
                                dense: true,
                                contentPadding: EdgeInsets.zero,
                                title: Text(
                                  music.tracks[index].title,
                                  maxLines: 1,
                                  overflow: TextOverflow.ellipsis,
                                  style: TextStyle(
                                    fontSize: 10,
                                    color: index == music.index
                                        ? p.accent
                                        : p.ink,
                                  ),
                                ),
                                leading: Text(
                                  '${index + 1}'.padLeft(2, '0'),
                                  style: TextStyle(fontSize: 9, color: p.muted),
                                ),
                                minLeadingWidth: 12,
                                onTap: () => music.select(index),
                                trailing: IconButton(
                                  tooltip: L10n.of(context).visualRemoveTrack,
                                  onPressed: () => music.remove(index),
                                  icon: const Icon(
                                    Icons.close_rounded,
                                    size: 14,
                                  ),
                                ),
                              ),
                            ),
                          ),
                          SwitchListTile.adaptive(
                            dense: true,
                            contentPadding: EdgeInsets.zero,
                            title: Text(
                              L10n.of(context).visualAutoLyrics,
                              style: TextStyle(fontSize: 10),
                            ),
                            subtitle: Text(
                              L10n.of(context).visualLyricsSources,
                              style: TextStyle(fontSize: 9),
                            ),
                            value: music.onlineLyrics,
                            onChanged: music.setOnlineLyrics,
                          ),
                          if (music.current != null)
                            Text(
                              music.lyricStatus.isEmpty
                                  ? L10n.of(context).visualLyricsOnPlay
                                  : musicStatusLabel(
                                      context,
                                      music.lyricStatus,
                                    ),
                              style: TextStyle(fontSize: 10, color: p.muted),
                            ),
                          if (music.current != null)
                            Wrap(
                              spacing: 4,
                              children: [
                                TextButton.icon(
                                  onPressed: importing
                                      ? null
                                      : () => pick('cover'),
                                  icon: const Icon(
                                    Icons.image_outlined,
                                    size: 14,
                                  ),
                                  label: Text(
                                    L10n.of(context).visualCover,
                                    style: TextStyle(fontSize: 10),
                                  ),
                                ),
                                TextButton.icon(
                                  key: const ValueKey('music-search-lyrics'),
                                  onPressed: searchLyrics,
                                  icon: const Icon(
                                    Icons.travel_explore,
                                    size: 14,
                                  ),
                                  label: Text(
                                    L10n.of(context).visualSearchLyrics,
                                    style: TextStyle(fontSize: 10),
                                  ),
                                ),
                                TextButton.icon(
                                  onPressed: showAllLyrics,
                                  icon: const Icon(Icons.subject, size: 14),
                                  label: Text(
                                    L10n.of(context).visualViewLyrics,
                                    style: TextStyle(fontSize: 10),
                                  ),
                                ),
                                TextButton.icon(
                                  key: const ValueKey('music-import-lyrics'),
                                  onPressed: importing
                                      ? null
                                      : () => pick('lyrics'),
                                  icon: const Icon(
                                    Icons.lyrics_outlined,
                                    size: 14,
                                  ),
                                  label: Text(
                                    L10n.of(context).visualImportLyrics,
                                    style: TextStyle(fontSize: 10),
                                  ),
                                ),
                              ],
                            ),
                        ],
                      )
                    : const SizedBox.shrink(),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

class TrackCover extends StatefulWidget {
  const TrackCover({super.key, this.source});
  final TextureSource? source;
  @override
  State<TrackCover> createState() => _TrackCoverState();
}

class _TrackCoverState extends State<TrackCover> {
  Future<ResolvedTexture>? future;
  @override
  void initState() {
    super.initState();
    load();
  }

  @override
  void didUpdateWidget(TrackCover old) {
    super.didUpdateWidget(old);
    if (old.source != widget.source) load();
  }

  void load() {
    future = widget.source == null
        ? null
        : TextureRepository.resolve(widget.source!);
  }

  @override
  Widget build(BuildContext context) {
    final p = AppearanceScope.of(context);
    Widget fallback() => DecoratedBox(
      decoration: BoxDecoration(
        gradient: LinearGradient(
          colors: [
            p.accent.withValues(alpha: .4),
            p.surface.withValues(alpha: .25),
          ],
        ),
      ),
      child: Icon(Icons.album_rounded, color: p.accent, size: 30),
    );
    return FutureBuilder<ResolvedTexture>(
      future: future,
      builder: (context, result) => result.data?.bytes != null
          ? Image.memory(
              result.data!.bytes!,
              fit: BoxFit.cover,
              errorBuilder: (_, _, _) => fallback(),
            )
          : fallback(),
    );
  }
}

String musicSourceLabel(BuildContext context, String source) =>
    switch (source) {
      '歌词文件' => L10n.of(context).visualLyricsFile,
      '音频内嵌' => L10n.of(context).visualEmbeddedLyrics,
      _ => source,
    };

String musicStatusLabel(BuildContext context, String status) =>
    switch (status) {
      '正在读取歌词…' => L10n.of(context).visualLyricsLoading,
      '歌词解析未完成，可重新导入' => L10n.of(context).visualLyricsParseFailure,
      '未找到歌词，可导入或重新搜索' => L10n.of(context).visualLyricsMissing,
      '存在多个版本，请在搜索中选择' => L10n.of(context).visualLyricsVersions,
      '歌词读取失败，可手动导入或重试' => L10n.of(context).visualLyricsReadFailure,
      _ => musicSourceLabel(context, status),
    };

String musicErrorLabel(BuildContext context, String error) => switch (error) {
  '歌曲无法播放，请检查文件或更换音频格式。' => L10n.of(context).visualPlaybackFailure,
  '播放请求未能完成，请重试。' => L10n.of(context).visualPlaybackRequestFailure,
  '声音状态未能确认，请重试。' => L10n.of(context).visualAudioStateFailure,
  '播放列表未能更新，请重试。' => L10n.of(context).visualPlaylistUpdateFailure,
  _ => L10n.of(context).visualPlaybackFailure,
};
