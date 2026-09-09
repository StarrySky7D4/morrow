import 'dart:convert';
import 'dart:typed_data';
import 'track_metadata.dart';

/// Browser fallback for ID3v2.3/v2.4, FLAC comments and MP4 metadata.
/// Untimed lyrics stay untimed; no invented synchronization is generated.
TrackMetadata readEmbeddedTags(Uint8List data) {
  if (data.length >= 4 && latin1.decode(data.sublist(0, 4)) == 'fLaC') {
    return _flacTags(data);
  }
  if (data.length >= 8 && latin1.decode(data.sublist(4, 8)) == 'ftyp') {
    return _mp4Tags(data);
  }
  if (data.length < 10 ||
      ascii.decode(data.take(3).toList(), allowInvalid: true) != 'ID3') {
    return const TrackMetadata();
  }
  final version = data[3];
  if (version != 3 && version != 4) return const TrackMetadata();
  int size(int at, {bool sync = false}) {
    var result = 0;
    for (var i = 0; i < 4; i++) {
      result = (result << (sync ? 7 : 8)) | (data[at + i] & (sync ? 127 : 255));
    }
    return result;
  }

  String text(List<int> bytes, int encoding) {
    if (encoding == 0) {
      return latin1.decode(bytes).replaceAll('\u0000', '').trim();
    }
    if (encoding == 3) {
      return utf8
          .decode(bytes, allowMalformed: true)
          .replaceAll('\u0000', '')
          .trim();
    }
    var start = 0;
    var little = encoding == 1;
    if (bytes.length >= 2 &&
        ((bytes[0] == 255 && bytes[1] == 254) ||
            (bytes[0] == 254 && bytes[1] == 255))) {
      little = bytes[0] == 255;
      start = 2;
    }
    final units = <int>[];
    for (var i = start; i + 1 < bytes.length; i += 2) {
      final unit = little
          ? bytes[i] | bytes[i + 1] << 8
          : bytes[i] << 8 | bytes[i + 1];
      if (unit != 0) units.add(unit);
    }
    return String.fromCharCodes(units).trim();
  }

  final end = (10 + size(6, sync: true)).clamp(10, data.length);
  var offset = 10;
  if (data[5] & 0x40 != 0 && offset + 4 <= end) {
    offset += size(offset, sync: version == 4) + (version == 3 ? 4 : 0);
  }
  var title = '', artist = '', lyrics = '';
  while (offset + 10 <= end) {
    final id = ascii.decode(
      data.sublist(offset, offset + 4),
      allowInvalid: true,
    );
    final length = size(offset + 4, sync: version == 4);
    final flags = data[offset + 9];
    offset += 10;
    if (length <= 0 || offset + length > end) break;
    if ((version == 3 && flags & 0xc0 != 0) ||
        (version == 4 && flags & 0x0c != 0)) {
      offset += length;
      continue;
    }
    final bytes = data.sublist(offset, offset + length);
    final encoding = bytes[0];
    if (id == 'TIT2') title = text(bytes.sublist(1), encoding);
    if (id == 'TPE1') artist = text(bytes.sublist(1), encoding);
    if (id == 'USLT' && bytes.length > 4) {
      var start = 4;
      final wide = encoding == 1 || encoding == 2;
      while (start < bytes.length) {
        if (bytes[start] == 0 &&
            (!wide || (start + 1 < bytes.length && bytes[start + 1] == 0))) {
          start += wide ? 2 : 1;
          break;
        }
        start += wide ? 2 : 1;
      }
      if (start < bytes.length) lyrics = text(bytes.sublist(start), encoding);
    }
    offset += length;
  }
  return TrackMetadata(title: title, artist: artist, embeddedLyrics: lyrics);
}

TrackMetadata _flacTags(Uint8List data) {
  var offset = 4;
  final values = <String, String>{};
  while (offset + 4 <= data.length) {
    final type = data[offset];
    final length =
        data[offset + 1] << 16 | data[offset + 2] << 8 | data[offset + 3];
    offset += 4;
    final end = offset + length;
    if (end > data.length) break;
    if (type & 127 == 4) {
      int number() {
        if (offset + 4 > end) {
          throw const FormatException('Incomplete Vorbis comment');
        }
        final value = ByteData.sublistView(
          data,
          offset,
          offset + 4,
        ).getUint32(0, Endian.little);
        offset += 4;
        return value;
      }

      try {
        final vendorLength = number();
        offset += vendorLength;
        final count = number().clamp(0, 10000);
        for (var i = 0; i < count; i++) {
          final size = number();
          if (offset + size > end) break;
          final value = utf8.decode(
            data.sublist(offset, offset + size),
            allowMalformed: true,
          );
          offset += size;
          final split = value.indexOf('=');
          if (split > 0) {
            values[value.substring(0, split).toUpperCase()] = value.substring(
              split + 1,
            );
          }
        }
      } on FormatException {
        /* Truncated metadata is ignored. */
      }
    }
    offset = end;
    if (type & 128 != 0) break;
  }
  return TrackMetadata(
    title: values['TITLE'] ?? '',
    artist: values['ARTIST'] ?? '',
    embeddedLyrics: values['LYRICS'] ?? values['UNSYNCEDLYRICS'] ?? '',
  );
}

TrackMetadata _mp4Tags(Uint8List data) {
  final values = <String, String>{};
  void boxes(int begin, int end, int depth, [String? field]) {
    if (depth > 8) return;
    var offset = begin;
    while (offset + 8 <= end) {
      var length = ByteData.sublistView(data, offset, offset + 4).getUint32(0);
      final type = latin1.decode(data.sublist(offset + 4, offset + 8));
      var header = 8;
      if (length == 1) {
        if (offset + 16 > end) break;
        length = ByteData.sublistView(
          data,
          offset + 8,
          offset + 16,
        ).getUint64(0);
        header = 16;
      } else if (length == 0) {
        length = end - offset;
      }
      if (length < header || offset + length > end) break;
      final body = offset + header;
      if (['moov', 'udta', 'meta', 'ilst'].contains(type)) {
        boxes(body + (type == 'meta' ? 4 : 0), offset + length, depth + 1);
      } else if (['©lyr', '©nam', '©ART'].contains(type)) {
        boxes(body, offset + length, depth + 1, type);
      } else if (type == 'data' && field != null && length >= header + 8) {
        values[field] = utf8.decode(
          data.sublist(body + 8, offset + length),
          allowMalformed: true,
        );
      }
      offset += length;
    }
  }

  boxes(0, data.length, 0);
  return TrackMetadata(
    title: values['©nam'] ?? '',
    artist: values['©ART'] ?? '',
    embeddedLyrics: values['©lyr'] ?? '',
  );
}
