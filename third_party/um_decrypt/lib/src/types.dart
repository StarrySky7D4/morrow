import 'dart:convert';
import 'dart:typed_data';

enum UmFormat { auto, ncm, qmc, kgm, kwm, tm, xiami, ximalaya, raw, qmcPayload }

enum UmAudioFormat { mp3, flac, ogg, mp4, wav, wma, dff, aac, ape }

/// Error codes are stable across the Rust, C and Dart interfaces.
final class UmDecryptException implements Exception {
  final int code;
  const UmDecryptException(this.code);
  String get message => switch (code) {
        1 => 'Invalid argument or key kind for this format',
        2 => 'Truncated container',
        3 => 'Invalid container header or footer',
        4 => 'Unsupported format',
        5 => 'Unsupported encryption version or slot',
        6 => 'External key required',
        7 => 'Invalid key, envelope or padding',
        8 => 'Decrypted header is not recognized as audio',
        9 => 'Chunk outside audio range',
        10 => 'Resource limit exceeded',
        255 => 'Internal decryption or module initialization error',
        _ => 'Unknown native error',
      };
  @override
  String toString() => 'UmDecryptException($code): $message';
}

/// QMC/KGM v5 key supplied by the application. No client database is opened.
final class UmKey {
  final int _kind;
  final Uint8List? _bytes;
  int get kind => _kind;
  Uint8List get bytes => Uint8List.fromList(_bytes ?? Uint8List(0));
  const UmKey.none()
      : _kind = 0,
        _bytes = null;
  UmKey.ekey(String base64)
      : _kind = 1,
        _bytes = Uint8List.fromList(utf8.encode(base64));
  UmKey.decoded(Uint8List key)
      : _kind = 2,
        _bytes = Uint8List.fromList(key);
}

final class UmInfo {
  final UmFormat format;
  final UmAudioFormat audioFormat;
  final int audioOffset;
  final int audioLength;
  const UmInfo.internal(
      this.format, this.audioFormat, this.audioOffset, this.audioLength);
}

final class UmDecryptedAudio {
  final UmInfo info;
  final Uint8List bytes;
  const UmDecryptedAudio.internal(this.info, this.bytes);
}
