part of 'music_panel.dart';

extension _MusicMenus on _MusicPanelState {
  bool get canPlay =>
      mounted && music.tracks.isNotEmpty && !music.blocked && !music.loading;

  List<ComponentMenuAction> panelActions() {
    final l = L10n.of(context);
    return [
      ComponentMenuAction(
        label: l.visualImportMusic,
        icon: Icons.library_music_outlined,
        isEnabled: () => mounted && widget.writable && !importing,
        onSelected: () => pick('songs'),
      ),
      ComponentMenuAction(
        label: music.playing ? l.visualPauseMusic : l.visualPlayMusic,
        icon: music.playing ? Icons.pause : Icons.play_arrow,
        isEnabled: () => canPlay,
        onSelected: music.toggle,
      ),
      ComponentMenuAction(
        label: l.visualPreviousTrack,
        icon: Icons.skip_previous,
        isEnabled: () => canPlay,
        onSelected: music.previous,
      ),
      ComponentMenuAction(
        label: l.visualNextTrack,
        icon: Icons.skip_next,
        isEnabled: () => canPlay,
        onSelected: music.next,
      ),
      ComponentMenuAction(
        label: expanded ? l.visualCollapsePlaylist : l.visualExpandPlaylist,
        icon: Icons.queue_music,
        onSelected: togglePlaylist,
      ),
      ComponentMenuAction(
        label: l.visualFooterLyrics,
        icon: music.showLyrics ? Icons.check : Icons.lyrics_outlined,
        isEnabled: () => mounted && widget.writable,
        onSelected: () => music.setShowLyrics(!music.showLyrics),
      ),
      ...?widget.extraActions?.call(),
    ];
  }

  Widget trackMenu(MusicTrack track, Widget child) {
    final l = L10n.of(context);
    bool present() => mounted && music.tracks.contains(track);
    return ComponentContextMenu(
      key: ObjectKey(track),
      actions: () => [
        for (final delta in [-1, 1])
          ComponentMenuAction(
            label: Localizations.localeOf(context).languageCode == 'zh'
                ? (delta < 0 ? '上移歌曲' : '下移歌曲')
                : (delta < 0 ? 'Move track up' : 'Move track down'),
            icon: delta < 0 ? Icons.arrow_upward : Icons.arrow_downward,
            isEnabled: () =>
                present() &&
                widget.writable &&
                !importing &&
                !music.loading &&
                !music.blocked &&
                music.tracks.indexOf(track) + delta >= 0 &&
                music.tracks.indexOf(track) + delta < music.tracks.length,
            onSelected: () => music.reorder(
              track,
              music.tracks[music.tracks.indexOf(track) + delta],
              delta > 0,
            ),
          ),
        ComponentMenuAction(
          label: l.visualPlayMusic,
          icon: Icons.play_arrow,
          isEnabled: () => present() && canPlay,
          onSelected: () => music.select(music.tracks.indexOf(track)),
        ),
        ComponentMenuAction(
          label: l.visualRemoveTrack,
          icon: Icons.playlist_remove,
          isEnabled: () =>
              present() && widget.writable && !importing && !music.loading,
          onSelected: () => music.remove(music.tracks.indexOf(track)),
        ),
      ],
      child: child,
    );
  }
}
