import 'dart:async';
import 'dart:js_interop';
import 'package:file_selector/file_selector.dart';
import 'package:web/web.dart' as web;
import 'audio_formats.dart';
import 'prepared_audio.dart';

extension type _DecodeRequest._(JSObject _) implements JSObject {
  external factory _DecodeRequest({JSUint8Array bytes, int format});
}
extension type _DecodeResponse(JSObject _) implements JSObject {
  external int get code;
  external JSUint8Array? get bytes;
  external String? get extension;
}
Future<PreparedAudio> prepareContainer(
  XFile file,
  int format, {
  String? libraryPath,
  String? keyDatabasePath,
}) async {
  final data = await file.readAsBytes();
  if (data.length > maxMusicBytes) {
    throw const FormatException('音频过大，请选择 150 MB 以内的文件。');
  }
  final completed = Completer<PreparedAudio>();
  final worker = web.Worker(
    web.URL('um/audio-worker.mjs', web.document.baseURI).href.toJS,
    web.WorkerOptions(type: 'module'),
  );
  worker.onmessage = ((web.MessageEvent event) {
    if (completed.isCompleted) return;
    final response = _DecodeResponse(event.data as JSObject);
    if (response.code != 0) {
      completed.completeError(
        FormatException(
          response.code == 6
              ? '此音频需要本机播放密钥，请使用 Windows 桌面版导入。'
              : audioImportError(response.code),
        ),
      );
      return;
    }
    final bytes = response.bytes?.toDart;
    final extension = response.extension;
    if (bytes == null ||
        bytes.isEmpty ||
        bytes.length > maxMusicBytes ||
        extension == null ||
        !standardAudioExtensions.contains(extension)) {
      completed.completeError(const FormatException('无法读取音频内容。'));
      return;
    }
    completed.complete(
      PreparedAudio(
        XFile.fromData(bytes, name: decodedAudioName(file.name, extension)),
      ),
    );
  }).toJS;
  worker.onerror = ((web.Event event) {
    event.preventDefault();
    if (!completed.isCompleted) {
      completed.completeError(const FormatException('音频读取组件暂不可用，请重试。'));
    }
  }).toJS;
  try {
    worker.postMessage(_DecodeRequest(bytes: data.toJS, format: format));
    return await completed.future.timeout(
      const Duration(seconds: 60),
      onTimeout: () => throw const FormatException('音频读取超时，请尝试较小的文件。'),
    );
  } finally {
    worker.terminate();
  }
}
