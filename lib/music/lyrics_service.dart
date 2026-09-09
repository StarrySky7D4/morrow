import 'dart:convert';
import 'package:http/http.dart' as http;

class LyricMatch {
  const LyricMatch({
    required this.title,
    required this.artist,
    required this.lyrics,
    this.album = '',
    this.duration = 0,
    this.synced = false,
  });
  final String title, artist, album, lyrics;
  final double duration;
  final bool synced;
  factory LyricMatch.fromJson(Map<String, dynamic> data) {
    final sync = data['syncedLyrics'] as String? ?? '';
    return LyricMatch(
      title: data['trackName'] as String? ?? '',
      artist: data['artistName'] as String? ?? '',
      album: data['albumName'] as String? ?? '',
      duration: (data['duration'] as num?)?.toDouble() ?? 0,
      synced: sync.trim().isNotEmpty,
      lyrics: sync.trim().isNotEmpty
          ? sync
          : data['plainLyrics'] as String? ?? '',
    );
  }
}

class LyricsService {
  LyricsService({this.client});
  final http.Client? client;
  Future<List<LyricMatch>> search(String title, String artist) async {
    if (title.trim().isEmpty) return [];
    final connection = client ?? http.Client();
    try {
      final response = await connection
          .get(
            Uri.https('lrclib.net', '/api/search', {
              'track_name': title.trim(),
              if (artist.trim().isNotEmpty) 'artist_name': artist.trim(),
            }),
          )
          .timeout(const Duration(seconds: 12));
      if (response.statusCode != 200) {
        throw const FormatException('歌词服务暂不可用，请稍后重试。');
      }
      final records = jsonDecode(utf8.decode(response.bodyBytes)) as List;
      return records
          .take(30)
          .map((e) => LyricMatch.fromJson(e as Map<String, dynamic>))
          .where((e) => e.lyrics.trim().isNotEmpty)
          .toList();
    } finally {
      if (client == null) connection.close();
    }
  }

  static LyricMatch? exactMatch(
    List<LyricMatch> results,
    String title,
    String artist,
    double duration,
  ) {
    String normalized(String value) =>
        value.toLowerCase().replaceAll(RegExp(r'\s+'), '').trim();
    final matches = results
        .where(
          (r) =>
              normalized(r.title) == normalized(title) &&
              (artist.isEmpty || normalized(r.artist) == normalized(artist)) &&
              (duration <= 0 || (r.duration - duration).abs() <= 2),
        )
        .toList();
    // Ambiguous titles/artists require a manual choice, never silently pick one.
    if (artist.isEmpty &&
        matches.map((m) => normalized(m.artist)).toSet().length > 1) {
      return null;
    }
    matches.sort((a, b) => (b.synced ? 1 : 0).compareTo(a.synced ? 1 : 0));
    return matches.isEmpty ? null : matches.first;
  }
}
