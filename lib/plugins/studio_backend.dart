import '../content/rich_content.dart' show RichFragment;
import '../music/music_controller.dart' show LyricLine;
import '../music/lyrics_service.dart' show LyricMatch;

class PlaybackDecision {
  const PlaybackDecision(
    this.index,
    this.playing,
    this.positionMs,
    this.effect,
  );
  final int index, positionMs;
  final bool playing;
  final String effect;
}

abstract class StudioBackend {
  Future<RichFragment> capture(
    String format,
    String source, {
    String imagePrefix = "clipboard",
  });
  Future<List<LyricLine>> parseLyrics(String text);
  Future<int?> matchLyrics(
    List<LyricMatch> candidates,
    String title,
    String artist,
    double duration,
  );
  Future<void> validateImport(
    String kind,
    int size, {
    String url = '',
    bool attachment = false,
  });
  Future<PlaybackDecision> playback(
    String action, {
    required int count,
    required int index,
    required bool playing,
    required bool blocked,
    int positionMs = 0,
    int durationMs = 0,
    int value = 0,
    bool flag = false,
  });
}
