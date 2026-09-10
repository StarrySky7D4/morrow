import 'dart:io';
import 'dart:isolate';
import 'package:file_selector/file_selector.dart';
import 'package:um_decrypt/um_decrypt.dart';
import 'audio_formats.dart';
import 'prepared_audio.dart';
import 'kugou_key_native.dart';

Future<PreparedAudio> prepareContainer(
  XFile file,
  int format, {
  String? libraryPath,
  String? keyDatabasePath,
}) async {
  if (!Platform.isWindows) throw const FormatException('当前平台暂不支持此音频容器。');
  final path =
      libraryPath ??
      '${File(Platform.resolvedExecutable).parent.path}/um_decrypt_ffi.dll';
  if (!await File(path).exists()) {
    throw const FormatException('音频读取组件缺失，请使用完整的应用安装包。');
  }
  final inputPath = file.path;
  final UmDecryptedAudio result;
  try {
    result = await Isolate.run(() async {
      final input = File(inputPath);
      if (await input.length() > maxMusicBytes) {
        throw const UmDecryptException(10);
      }
      final data = await input.readAsBytes();
      if (data.length > maxMusicBytes) throw const UmDecryptException(10);
      final library = UmLibrary.open(path);
      final selectedFormat = UmFormat.values[format];
      try {
        return library.decrypt(data, format: selectedFormat);
      } on UmDecryptException catch (error) {
        if (error.code != 6 || selectedFormat != UmFormat.kgm) rethrow;
        final hint = library.keyHint(data, format: selectedFormat);
        if (hint == null) rethrow;
        final key = await lookupKugouKey(hint, databasePath: keyDatabasePath);
        if (key == null) rethrow;
        return library.decrypt(
          data,
          format: selectedFormat,
          key: UmKey.ekey(key),
        );
      }
    });
  } on UmDecryptException catch (error) {
    throw FormatException(audioImportError(error.code));
  } on ArgumentError {
    throw const FormatException('音频读取组件无法加载，请使用完整的应用安装包。');
  }
  if (result.bytes.isEmpty || result.bytes.length > maxMusicBytes) {
    throw const FormatException('音频内容为空或超出大小限制。');
  }
  final folder = await Directory.systemTemp.createTemp('morrow-audio-');
  final extension = result.info.audioFormat.name == 'mp4'
      ? 'm4a'
      : result.info.audioFormat.name;
  final output = File(
    '${folder.path}/${decodedAudioName(file.name, extension)}',
  );
  Future<void> cleanup() async {
    if (await output.exists()) await output.delete();
    if (await folder.exists()) await folder.delete();
  }

  try {
    await output.writeAsBytes(result.bytes, flush: true);
    return PreparedAudio(
      XFile(output.path, name: decodedAudioName(file.name, extension)),
      release: cleanup,
    );
  } catch (_) {
    await cleanup();
    rethrow;
  }
}
