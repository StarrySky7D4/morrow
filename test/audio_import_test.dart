import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';
import 'package:file_selector/file_selector.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:path_provider_platform_interface/path_provider_platform_interface.dart';
import 'package:morrow_studio/music/audio_import.dart';
import 'package:morrow_studio/music/audio_formats.dart';
import 'package:morrow_studio/music/music_controller.dart';
import 'package:morrow_studio/media/texture_repository.dart';
import 'package:morrow_studio/media/texture_source.dart';

class _Paths extends PathProviderPlatform {
  _Paths(this.folder);
  final String folder;
  @override
  Future<String?> getApplicationSupportPath() async => folder;
}

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
  ByteData.sublistView(data).setUint64(24, 12345, Endian.little);
  final digits = ascii.encode('12345');
  final mask = ascii.encode('MoOtOiTvINGwd2E6n0E1i7L5t2IoOoNk');
  for (var i = 0; i < plain.length; i++) {
    final k = i % 32;
    data[1024 + i] = plain[i] ^ mask[k] ^ digits[k % digits.length];
  }
  return data;
}

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  final library = File(
    'third_party/um_decrypt/native/windows/um_decrypt_ffi.dll',
  ).absolute.path;
  late Directory folder;
  late PathProviderPlatform originalPaths;
  setUp(() async {
    folder = await Directory.systemTemp.createTemp('morrow-import-test-');
    originalPaths = PathProviderPlatform.instance;
    PathProviderPlatform.instance = _Paths(folder.path);
  });
  tearDown(() async {
    PathProviderPlatform.instance = originalPaths;
    // Only the test-owned temporary directory contains these generated inputs.
    await folder.delete(recursive: true);
  });
  test(
    'KWM imports playable WAV, retains name/sidecar, persists and preserves source',
    () async {
      final input = File('${folder.path}/夜航.KWM');
      final encrypted = kwm(wave());
      await input.writeAsBytes(encrypted);
      await File('${folder.path}/夜航.LRC').writeAsString('[00:00.00]原目录歌词');
      String? temporaryPath;
      final track = await MusicTrack.import(
        XFile(input.path),
        prepare: (file) async {
          final prepared = await prepareMusicAudio(file, libraryPath: library);
          temporaryPath = prepared.file.path;
          return prepared;
        },
      );
      expect(track.source.name, '夜航.wav');
      expect(track.source.kind, TextureKind.audio);
      expect(track.lyrics, '[00:00.00]原目录歌词');
      expect(track.lyricSource, '歌词文件');
      expect(await File(track.source.location).readAsBytes(), wave());
      expect(await input.readAsBytes(), encrypted);
      expect(await File(temporaryPath!).exists(), isFalse);
      final restored = MusicTrack.fromJson(track.toJson());
      final resolved = await TextureRepository.resolve(restored.source);
      expect(await File.fromUri(Uri.parse(resolved.uri)).readAsBytes(), wave());
    },
    skip: !Platform.isWindows,
  );
  for (final vector in ['qmc0_static', 'mflac_map']) {
    test(
      '$vector native import matches supplied audio vector and cleans temporary file',
      () async {
        final base = 'test/fixtures/audio/$vector';
        final input = File(
          '${folder.path}/$vector.${vector.startsWith('qmc') ? 'qmc0' : 'mflac'}',
        );
        final bytes = Uint8List.fromList([
          ...await File('${base}_raw.bin').readAsBytes(),
          ...await File('${base}_suffix.bin').readAsBytes(),
        ]);
        await input.writeAsBytes(bytes);
        final prepared = await prepareMusicAudio(
          XFile(input.path),
          libraryPath: library,
        );
        final path = prepared.file.path;
        try {
          expect(
            await prepared.file.readAsBytes(),
            await File('${base}_target.bin').readAsBytes(),
          );
          expect(await input.readAsBytes(), bytes);
        } finally {
          await prepared.dispose();
        }
        expect(await File(path).exists(), isFalse);
      },
      skip: !Platform.isWindows,
    );
  }
  test(
    'Missing key and malformed containers fail without adding stored files',
    () async {
      final kgm = Uint8List(140);
      kgm.setAll(0, [
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
      final view = ByteData.sublistView(kgm);
      view.setUint32(16, 128, Endian.little);
      view.setUint32(20, 5, Endian.little);
      view.setUint32(68, 32, Endian.little);
      kgm.setAll(72, ascii.encode('0123456789abcdef0123456789abcdef'));
      final file = File('${folder.path}/locked.kgg');
      await file.writeAsBytes(kgm);
      await expectLater(
        prepareMusicAudio(
          XFile(file.path),
          libraryPath: library,
          keyDatabasePath: '${folder.path}/absent.db',
        ),
        throwsA(
          isA<FormatException>().having(
            (e) => e.message,
            'message',
            contains('密钥'),
          ),
        ),
      );
      final bad = File('${folder.path}/bad.ncm');
      await bad.writeAsBytes([1, 2, 3]);
      await expectLater(
        prepareMusicAudio(XFile(bad.path), libraryPath: library),
        throwsFormatException,
      );
      expect(await Directory('${folder.path}/textures').exists(), isFalse);
    },
    skip: !Platform.isWindows,
  );
  test(
    'Ordinary extended audio bypasses UM and oversized input is rejected before loading',
    () async {
      for (final ext in ['wma', 'ape', 'aiff', 'wv', 'dsf', 'dff']) {
        final file = XFile.fromData(wave(), path: 'ordinary.$ext');
        final prepared = await prepareMusicAudio(
          file,
          libraryPath: 'missing.dll',
        );
        expect(prepared.file, same(file));
        expect(TextureSource.kindFor(file.name), TextureKind.audio);
      }
      final file = File('${folder.path}/large.ncm');
      final handle = await file.open(mode: FileMode.write);
      await handle.truncate(maxMusicBytes + 1);
      await handle.close();
      await expectLater(
        prepareMusicAudio(XFile(file.path), libraryPath: 'missing.dll'),
        throwsA(
          isA<FormatException>().having(
            (e) => e.message,
            'message',
            contains('150 MB'),
          ),
        ),
      );
    },
  );
}
