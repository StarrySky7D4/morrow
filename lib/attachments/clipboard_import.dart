import 'dart:async';
import 'dart:typed_data';
import 'package:file_selector/file_selector.dart';
import 'package:flutter/foundation.dart';
import 'package:super_clipboard/super_clipboard.dart';
import 'attachment.dart';

class PastedContent {
  const PastedContent({this.text = '', this.files = const []});
  final String text;
  final List<XFile> files;
}

Future<PastedContent> readPaste([ClipboardReader? reader]) async {
  reader ??= await SystemClipboard.instance?.read();
  if (reader == null) throw const FormatException('此环境不支持读取剪贴板，请使用导入文件。');
  final files = <XFile>[];
  var hasFilePaths = false;
  for (final item in reader.items.take(20)) {
    if (!kIsWeb && item.canProvide(Formats.fileUri)) {
      final uri = await item.readValue(Formats.fileUri);
      if (uri != null && uri.scheme == 'file') {
        files.add(XFile(uri.toFilePath()));
        hasFilePaths = true;
        continue;
      }
    }
    final png = item.canProvide(Formats.png);
    final name = await item.getSuggestedName();
    if (!png && name == null) continue;
    final result = Completer<XFile?>();
    final progress = item.getFile(
      png ? Formats.png : null,
      (file) async {
        try {
          if ((file.fileSize ?? 0) > IdeaAttachment.maxSize) {
            throw const FormatException('剪贴板文件超过 200 MB。');
          }
          final bytes = BytesBuilder(copy: false);
          await for (final chunk in file.getStream()) {
            if (bytes.length + chunk.length > IdeaAttachment.maxSize) {
              throw const FormatException('剪贴板文件超过 200 MB。');
            }
            bytes.add(chunk);
          }
          result.complete(
            XFile.fromData(
              bytes.takeBytes(),
              name: file.fileName ?? name ?? '粘贴图片.png',
              path: file.fileName ?? name ?? '粘贴图片.png',
            ),
          );
        } catch (error, stack) {
          result.completeError(error, stack);
        }
      },
      onError: (error) {
        if (!result.isCompleted) result.completeError(error);
      },
    );
    if (progress == null) continue;
    final file = await result.future.timeout(const Duration(seconds: 30));
    if (file != null) files.add(file);
  }
  final text = hasFilePaths
      ? ''
      : await reader.readValue(Formats.plainText) ?? '';
  return PastedContent(text: text, files: files);
}
