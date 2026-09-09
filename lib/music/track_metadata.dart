class TrackMetadata {
  const TrackMetadata({
    this.title = '',
    this.artist = '',
    this.duration = 0,
    this.fileLyrics = '',
    this.embeddedLyrics = '',
  });
  final String title, artist, fileLyrics, embeddedLyrics;
  final double duration;
}
