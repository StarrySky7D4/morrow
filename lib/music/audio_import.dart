import 'package:file_selector/file_selector.dart';
import 'audio_formats.dart';
import 'prepared_audio.dart';
import 'audio_import_native.dart'
    if (dart.library.js_interop) 'audio_import_web.dart'
    as platform;

Future<PreparedAudio> prepareMusicAudio(
  XFile file, {
  String? libraryPath,
  String? keyDatabasePath,
}) async {
  final length = await file.length();
  if (length == 0) throw const FormatException('音频文件为空。');
  if (length > maxMusicBytes) {
    throw const FormatException('音频过大，请选择 150 MB 以内的文件。');
  }
  final extension = audioExtension(file.name);
  final format = containerAudioFormats[extension];
  if (format != null) {
    return platform.prepareContainer(
      file,
      format,
      libraryPath: libraryPath,
      keyDatabasePath: keyDatabasePath,
    );
  }
  if (!standardAudioExtensions.contains(extension)) {
    throw const FormatException('请选择支持的音频文件。');
  }
  return PreparedAudio(file);
}
