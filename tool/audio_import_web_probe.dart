import 'dart:async';
import 'dart:convert';
import 'dart:js_interop';
import 'dart:typed_data';
import 'package:flutter/widgets.dart';
import 'package:file_selector/file_selector.dart';
import 'package:http/http.dart' as http;
import 'package:web/web.dart' as web;
import 'package:morrow_studio/music/audio_import.dart';
import 'package:morrow_studio/music/music_controller.dart';
import 'package:morrow_studio/media/texture_repository.dart';

@JS('audioProbeResult')
external set audioProbeResult(JSAny? value);

Uint8List wave() {
  final data = Uint8List(16044);
  final view = ByteData.sublistView(data);
  data.setAll(0, ascii.encode('RIFF'));
  view.setUint32(4, data.length - 8, Endian.little);
  data.setAll(8, ascii.encode('WAVEfmt '));
  view.setUint32(16, 16, Endian.little);
  view.setUint16(20, 1, Endian.little);
  view.setUint16(22, 1, Endian.little);
  view.setUint32(24, 8000, Endian.little);
  view.setUint32(28, 16000, Endian.little);
  view.setUint16(32, 2, Endian.little);
  view.setUint16(34, 16, Endian.little);
  data.setAll(36, ascii.encode('data'));
  view.setUint32(40, 16000, Endian.little);
  return data;
}

Uint8List kwm(Uint8List plain) {
  final data = Uint8List(1024 + plain.length);
  data.setAll(0, ascii.encode('yeelion-kuwo-tme'));
  ByteData.sublistView(data).setUint32(24, 12345, Endian.little);
  final digits = ascii.encode('12345');
  final mask = ascii.encode('MoOtOiTvINGwd2E6n0E1i7L5t2IoOoNk');
  for (var i = 0; i < plain.length; i++) {
    final k = i % 32;
    data[1024 + i] = plain[i] ^ mask[k] ^ digits[k % digits.length];
  }
  return data;
}

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  runApp(const SizedBox());
  try {
    for (final vector in ['qmc0_static', 'mflac_map']) {
      Future<Uint8List> part(String suffix) async => (await http.get(
        Uri.base.resolve('/fixtures/${vector}_$suffix.bin'),
      )).bodyBytes;
      final input = Uint8List.fromList([
        ...await part('raw'),
        ...await part('suffix'),
      ]);
      final original = Uint8List.fromList(input);
      final file = XFile.fromData(
        input,
        name: '$vector.${vector.startsWith('qmc') ? 'qmc0' : 'mflac'}',
      );
      final decoded = await prepareMusicAudio(file);
      final bytes = await decoded.file.readAsBytes();
      final expected = await part('target');
      if (base64Encode(bytes) != base64Encode(expected) ||
          base64Encode(input) != base64Encode(original)) {
        throw StateError('Worker bytes differ for $vector');
      }
      await decoded.dispose();
    }
    final input = XFile.fromData(kwm(wave()), name: 'browser.kwm');
    final track = await MusicTrack.import(input);
    if (track.source.name != 'browser.wav') {
      throw StateError('Converted name lost');
    }
    final restored = MusicTrack.fromJson(track.toJson());
    final resolved = await TextureRepository.resolve(restored.source);
    final actual = (await http.get(Uri.parse(resolved.uri))).bodyBytes;
    if (base64Encode(actual) != base64Encode(wave())) {
      throw StateError('Stored waveform differs');
    }
    final audio = web.HTMLAudioElement()
      ..src = resolved.uri
      ..muted = true;
    try {
      await audio.play().toDart.timeout(const Duration(seconds: 10));
      await Future<void>.delayed(const Duration(milliseconds: 200));
      if (audio.duration < .9 || audio.currentTime <= 0) {
        throw StateError('Decoded WAV did not play');
      }
    } finally {
      audio.pause();
      audio.removeAttribute('src');
      audio.load();
      resolved.release?.call();
      await TextureRepository.remove(restored.source);
    }
    try {
      await prepareMusicAudio(
        XFile.fromData(Uint8List.fromList([1, 2, 3]), name: 'bad.ncm'),
      );
      throw StateError('Malformed container was accepted');
    } on FormatException {
      /* Expected failure. */
    }
    final locked = Uint8List(140);
    locked.setAll(0, [
      0x7c,
      0xd5,
      0x32,
      0xeb,
      0x86,
      0x02,
      0x7f,
      0x4b,
      0xa8,
      0xaf,
      0xa6,
      0x8e,
      0x0f,
      0xff,
      0x99,
      0x14,
    ]);
    final header = ByteData.sublistView(locked);
    header.setUint32(16, 128, Endian.little);
    header.setUint32(20, 5, Endian.little);
    header.setUint32(68, 32, Endian.little);
    locked.setAll(72, ascii.encode('0123456789abcdef0123456789abcdef'));
    try {
      await prepareMusicAudio(XFile.fromData(locked, name: 'locked.kgg'));
      throw StateError('Missing-key KGG was accepted');
    } on FormatException catch (error) {
      if (!error.message.contains('Windows 桌面版')) {
        throw StateError('Missing desktop guidance: $error');
      }
    }
    audioProbeResult =
        'PASS: Worker QMC/MFLAC exact bytes; KWM import, persisted playlist, WAV playback; malformed input; missing-key desktop guidance'
            .toJS;
  } catch (error) {
    audioProbeResult = 'FAIL: $error'.toJS;
  }
}
