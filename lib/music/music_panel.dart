import 'lyrics_dialog.dart';
import 'lyrics_service.dart';
import 'package:file_selector/file_selector.dart';
import 'package:flutter/material.dart';
import '../appearance.dart';
import '../media/texture_repository.dart';
import '../media/texture_source.dart';
import 'music_controller.dart';

class MusicPanel extends StatefulWidget {
  const MusicPanel({super.key, required this.controller});
  final MusicController controller;
  @override
  State<MusicPanel> createState() => _MusicPanelState();
}

class _MusicPanelState extends State<MusicPanel> {
  bool expanded = false, importing = false;
  MusicController get music => widget.controller;
  Future<void> pick(String type) async {
    if (importing) return;
    setState(() => importing = true);
    final selected = music.current;
    try {
      if (type == 'songs') {
        final files = await openFiles(
          acceptedTypeGroups: const [
            XTypeGroup(
              label: '音乐',
              extensions: [
                'mp3',
                'wav',
                'flac',
                'm4a',
                'aac',
                'ogg',
                'opus',
                'lrc',
              ],
            ),
          ],
        );
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
          if (!mounted) return;
          music.add([track]);
        }
      } else if (selected != null) {
        final file = await openFile(
          acceptedTypeGroups: [
            type == 'lyrics'
                ? const XTypeGroup(label: '歌词文件', extensions: ['lrc', 'txt'])
                : const XTypeGroup(
                    label: '歌曲封面',
                    extensions: ['png', 'jpg', 'jpeg', 'webp'],
                  ),
          ],
        );
        if (file == null || !mounted) return;
        if (type == 'lyrics') {
          if (await file.length() > 1024 * 1024) {
            throw const FormatException('歌词文件请控制在 1 MB 以内。');
          }
          final lyrics = await file.readAsString();
          if (lyrics.trim().isEmpty) throw const FormatException('歌词文件为空。');
          if (!mounted || !music.tracks.contains(selected)) return;
          music.setLyrics(lyrics, track: selected);
        } else {
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
            content: Text(
              error is FormatException ? error.message : '导入失败，请检查文件、编码与存储空间。',
            ),
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
        subtitle: track.lyricSource.isEmpty ? '尚未读取到歌词' : track.lyricSource,
        content: SizedBox(
          width: 420,
          child: SingleChildScrollView(
            child: SelectableText(
              track.lyrics.isEmpty ? '可导入歌词文件，或联网搜索。' : track.lyrics,
              style: const TextStyle(height: 1.8),
            ),
          ),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context),
            child: const Text('关闭'),
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
                      '随身听',
                      style: TextStyle(
                        color: p.ink,
                        fontSize: 12,
                        fontWeight: FontWeight.w600,
                      ),
                    ),
                  ),
                  IconButton(
                    key: const ValueKey('music-add'),
                    tooltip: '导入音乐',
                    onPressed: importing ? null : () => pick('songs'),
                    icon: const Icon(Icons.add_rounded, size: 18),
                  ),
                ],
              ),
              const SizedBox(height: 8),
              Row(
                children: [
                  Tooltip(
                    message: '更换歌曲封面',
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
                          music.current?.title ?? '留一点空间给音乐',
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
                              ? '正在载入…'
                              : music.current == null
                              ? '点击 + 导入本地歌曲'
                              : '${music.index + 1} / ${music.tracks.length} · ${music.playing ? '播放中' : '已暂停'}',
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
                    tooltip: '上一首',
                    onPressed: music.current == null ? null : music.previous,
                    icon: const Icon(Icons.skip_previous_rounded),
                  ),
                  IconButton.filledTonal(
                    key: const ValueKey('music-play'),
                    tooltip: music.playing ? '暂停音乐' : '播放音乐',
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
                    tooltip: '下一首',
                    onPressed: music.current == null ? null : music.next,
                    icon: const Icon(Icons.skip_next_rounded),
                  ),
                  IconButton(
                    key: const ValueKey('music-list-toggle'),
                    tooltip: expanded ? '收起播放列表' : '展开播放列表',
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
                        '底部显示歌词',
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
                        ? '从播放列表菜单导入 LRC 歌词'
                        : '播放列表与歌词自动保存',
                    style: TextStyle(fontSize: 9, color: p.muted),
                  ),
                ),
              ],
              if (music.error != null)
                Padding(
                  padding: const EdgeInsets.only(top: 8),
                  child: Text(
                    music.error!,
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
                                '播放列表还是空的',
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
                                  tooltip: '移出播放列表',
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
                            title: const Text(
                              '自动联网补全歌词',
                              style: TextStyle(fontSize: 10),
                            ),
                            subtitle: const Text(
                              '本地文件 → 内嵌 → LRCLIB',
                              style: TextStyle(fontSize: 9),
                            ),
                            value: music.onlineLyrics,
                            onChanged: music.setOnlineLyrics,
                          ),
                          if (music.current != null)
                            Text(
                              music.lyricStatus.isEmpty
                                  ? '播放时自动读取歌词'
                                  : music.lyricStatus,
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
                                  label: const Text(
                                    '封面',
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
                                  label: const Text(
                                    '搜索歌词',
                                    style: TextStyle(fontSize: 10),
                                  ),
                                ),
                                TextButton.icon(
                                  onPressed: showAllLyrics,
                                  icon: const Icon(Icons.subject, size: 14),
                                  label: const Text(
                                    '查看歌词',
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
                                  label: const Text(
                                    '导入歌词',
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
