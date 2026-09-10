import 'dart:io';
import 'package:flutter_test/flutter_test.dart';
import 'package:media_kit/media_kit.dart';
import 'package:file_selector/file_selector.dart';
import 'package:morrow_studio/music/audio_import.dart';
import '../test/audio_import_test.dart' show wave, kwm;

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  test(
    'Bundled UM audio plays in the bundled native audio engine',
    () async {
      final root = Directory('build/music-um/windows').absolute.path;
      MediaKit.ensureInitialized(libmpv: '$root/libmpv-2.dll');
      final player = Player();
      final folder = await Directory.systemTemp.createTemp(
        'morrow-playback-test-',
      );
      final file = File('${folder.path}/playback.kwm');
      await file.writeAsBytes(kwm(wave()));
      final audio = await prepareMusicAudio(
        XFile(file.path),
        libraryPath: '$root/um_decrypt_ffi.dll',
      );
      try {
        await player.setVolume(0);
        final progressed = player.stream.position.firstWhere(
          (p) => p.inMilliseconds > 0,
        );
        await player.open(Media(Uri.file(audio.file.path).toString()));
        await progressed.timeout(const Duration(seconds: 10));
        expect(player.state.duration.inMilliseconds, greaterThanOrEqualTo(900));
      } finally {
        await player.dispose();
        await audio.dispose();
        await file.delete();
        await folder.delete();
      }
    },
    skip: !Platform.isWindows,
  );
}
