import 'dart:io';
import 'dart:isolate';
import 'dart:typed_data';
import 'package:audio_metadata_reader/audio_metadata_reader.dart';
import 'package:file_selector/file_selector.dart';
import 'track_metadata.dart';
import 'embedded_tags.dart';

Future<TrackMetadata> readTrackMetadata(XFile source) => Isolate.run(() async {
  final file = File(source.path);
  String title = '', artist = '', embedded = '', sidecar = '';
  double duration = 0;
  try {
    final data = readMetadata(file);
    title = data.title ?? '';
    artist = data.artist ?? '';
    duration = (data.duration?.inMilliseconds ?? 0) / 1000;
    embedded = data.lyrics ?? '';
  } catch (_) {
    /* Untagged or unsupported audio is still playable. */
  }
  if (embedded.isEmpty && source.name.toLowerCase().endsWith('.mp3')) {
    try {
      final bytes = BytesBuilder(copy: false);
      final length = (await file.length()).clamp(0, 8 * 1024 * 1024);
      await for (final chunk in file.openRead(0, length)) {
        bytes.add(chunk);
      }
      final tags = readEmbeddedTags(bytes.takeBytes());
      embedded = tags.embeddedLyrics;
      if (title.isEmpty) title = tags.title;
      if (artist.isEmpty) artist = tags.artist;
    } catch (_) {
      /* A missing tag does not prevent playback. */
    }
  }
  try {
    final stem = source.name
        .split(RegExp(r'[/\\]'))
        .last
        .replaceFirst(RegExp(r'\.[^.]+$'), '')
        .toLowerCase();
    final candidates = <File>[];
    await for (final entry in file.parent.list(followLinks: false)) {
      if (entry is File &&
          entry.uri.pathSegments.last.toLowerCase() == '$stem.lrc') {
        candidates.add(entry);
      }
    }
    if (candidates.isNotEmpty &&
        await candidates.first.length() <= 1024 * 1024) {
      sidecar = await candidates.first.readAsString();
    }
  } catch (_) {
    /* Manual UTF-8 lyric import remains available. */
  }
  return TrackMetadata(
    title: title,
    artist: artist,
    duration: duration,
    fileLyrics: sidecar,
    embeddedLyrics: embedded,
  );
});
