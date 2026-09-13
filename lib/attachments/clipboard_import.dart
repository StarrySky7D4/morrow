import '../plugins/capture_models.dart';
import '../plugins/studio_backend.dart';
import 'dart:async';
import 'dart:convert';
import 'dart:typed_data';
import 'package:file_selector/file_selector.dart';
import 'package:flutter/foundation.dart';
import 'package:super_clipboard/super_clipboard.dart';
import '../content/rich_content.dart';
import 'attachment.dart';
import '../media/texture_source.dart';
import 'office_clipboard.dart';

class PastedContent {
  const PastedContent({
    this.text = '',
    this.markdown = '',
    this.files = const [],
    this.warnings = const [],
    this.formats = const [],
    this.textParts = const [],
    this.markdownParts = const [],
  });
  final String text, markdown;
  final List<XFile> files;
  final List<String> warnings, formats;
  final List<PastePart> textParts, markdownParts;
}

class _PasteText {
  const _PasteText(this.value, [this.ticket]);
  final String value;
  final String? ticket;
  List<PastePart> get parts => value.isEmpty
      ? []
      : [
          if (ticket case final ticket?)
            PastePart.ticket(ticket)
          else
            PastePart.literal(value),
        ];
}

List<PastePart> _joinParts(Iterable<List<PastePart>> entries) {
  final result = <PastePart>[];
  var first = true;
  for (final entry in entries) {
    if (!first) result.add(const PastePart.literal('\n\n'));
    first = false;
    result.addAll(entry);
  }
  return List.unmodifiable(result);
}

Future<XFile?> _readFile(
  ClipboardDataReader item,
  FileFormat? format,
  String fallback,
) async {
  final result = Completer<XFile?>();
  final progress = item.getFile(
    format,
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
        final name = file.fileName ?? fallback;
        result.complete(
          XFile.fromData(bytes.takeBytes(), name: name, path: name),
        );
      } catch (error, stack) {
        if (!result.isCompleted) result.completeError(error, stack);
      }
    },
    onError: (error) {
      if (!result.isCompleted) result.completeError(error);
    },
  );
  if (progress == null) return null;
  return result.future.timeout(const Duration(seconds: 30));
}

Future<PastedContent> readPaste([
  ClipboardReader? reader,
  StudioBackend? plugin,
]) async {
  Future<_PasteText> plain(String text, {String? parent}) async {
    if (plugin == null) return _PasteText(plainTextToMarkdown(text));
    final result = await plugin.capture('plain', text, parentTicket: parent);
    return _PasteText(result.markdown, result.ticket);
  }

  Future<_PasteText> convertRtf(String text) async {
    if (plugin == null) return _PasteText(rtfToPlainText(text));
    final result = await plugin.capture('rtf', text);
    return _PasteText(result.markdown, result.ticket);
  }

  final systemRead = reader == null;
  final sequence = systemRead ? await clipboardSequence() : null;
  reader ??= await SystemClipboard.instance?.read();
  if (reader == null) throw const FormatException('此环境不支持读取剪贴板，请使用导入文件。');
  final files = <XFile>[];
  final texts = <String>[], textParts = <List<PastePart>>[];
  final rich = <_PasteText>[];
  void addText(String value, [List<PastePart>? parts]) {
    texts.add(value);
    textParts.add(parts ?? [PastePart.literal(value)]);
  }

  final warnings = <String>{}, formats = <String>{};
  final prefix = 'clipboard-${DateTime.now().microsecondsSinceEpoch}';
  var memoryBytes = 0;
  Future<void> addFile(XFile file) async {
    if (files.length >= 20) {
      warnings.add('最多导入 20 个附件，其余内容请分次粘贴。');
      return;
    }
    final size = await file.length();
    if (size > IdeaAttachment.maxSize ||
        memoryBytes + size > IdeaAttachment.maxSize) {
      warnings.add('本次粘贴的文件总量超过 200 MB，请分次导入。');
      return;
    }
    memoryBytes += size;
    files.add(file);
  }

  XFile textFile(String text, String extension, String label) {
    final name = '$prefix-$label.$extension';
    return XFile.fromData(
      Uint8List.fromList(utf8.encode(text)),
      name: name,
      path: name,
    );
  }

  var index = 0;
  for (final item in reader.items.take(20)) {
    index++;
    formats.addAll(item.platformFormats);
    try {
      if (!kIsWeb && item.canProvide(Formats.fileUri)) {
        final uri = await item.readValue(Formats.fileUri);
        if (uri != null && uri.scheme == 'file') {
          await addFile(XFile(uri.toFilePath()));
          continue;
        }
      }
      final text = await item.readValue(Formats.plainText) ?? '';
      if (text.length > 2 * 1024 * 1024) {
        throw const FormatException('文本超过 2 MB，请作为文件导入。');
      }
      if (text.isNotEmpty) addText(text);
      final html = await item.readValue(Formats.htmlText);
      if (html != null && html.isNotEmpty) {
        await addFile(textFile(html, 'html', 'rich-$index'));
        try {
          final fragment = plugin == null
              ? htmlToMarkdown(html, imagePrefix: '$prefix-$index')
              : await plugin.capture(
                  'html',
                  html,
                  imagePrefix: '$prefix-$index',
                );
          if (fragment.markdown.isNotEmpty) {
            rich.add(_PasteText(fragment.markdown, fragment.ticket));
          }
          warnings.addAll(fragment.warnings);
          for (final file in fragment.files) {
            await addFile(file);
          }
        } catch (_) {
          if (text.isNotEmpty) {
            final fallback = await plain(text);
            rich.add(fallback);
            if (fallback.ticket case final ticket?) {
              textParts.last = [
                PastePart.ticket(ticket, selection: 'inputPlainText'),
              ];
            }
          }
          warnings.add('富文本排版无法完整转换，已保留可读文字。');
        }
      } else if (text.isNotEmpty) {
        final table = await plain(text);
        rich.add(table);
        if (table.ticket case final ticket?) {
          textParts.last = [
            PastePart.ticket(ticket, selection: 'inputPlainText'),
          ];
        }
        if (table.value != text) {
          await addFile(textFile(text, 'tsv', 'table-$index'));
          warnings.add('表格已转换为 Markdown，完整数据保留在 TSV 附件中。');
        }
      } else if (item.canProvide(Formats.uri)) {
        final uri = await item.readValue(Formats.uri);
        if (uri != null) {
          addText(uri.uri.toString());
          rich.add(_PasteText(uri.uri.toString()));
        }
      }
      // Preserve the original downloadable item; rich text flavors are handled above.
      final candidates = <(FileFormat, String)>[
        (Formats.docx, 'docx'),
        (Formats.xlsx, 'xlsx'),
        (Formats.pptx, 'pptx'),
        (Formats.doc, 'doc'),
        (Formats.xls, 'xls'),
        (Formats.ppt, 'ppt'),
        (Formats.pdf, 'pdf'),
        (Formats.gif, 'gif'),
        (Formats.webp, 'webp'),
        (Formats.png, 'png'),
        (Formats.jpeg, 'jpg'),
        (Formats.svg, 'svg'),
        (Formats.tiff, 'tiff'),
        (Formats.bmp, 'bmp'),
        (Formats.mp4, 'mp4'),
        (Formats.mov, 'mov'),
        (Formats.webm, 'webm'),
        (Formats.mp3, 'mp3'),
        (Formats.wav, 'wav'),
        (Formats.ogg, 'ogg'),
        (Formats.zip, 'zip'),
        (Formats.csv, 'csv'),
        (Formats.json, 'json'),
      ];
      final suggested = await item.getSuggestedName();
      final candidate = candidates
          .where((c) => item.canProvide(c.$1))
          .firstOrNull;
      if (candidate != null || suggested != null) {
        final file = await _readFile(
          item,
          suggested != null ? null : candidate!.$1,
          suggested ?? '$prefix-$index.${candidate!.$2}',
        );
        if (file != null) await addFile(file);
      }
      if (item.canProvide(Formats.rtf)) {
        final rtf = await _readFile(item, Formats.rtf, '$prefix-$index.rtf');
        if (rtf != null) {
          await addFile(rtf);
          if (text.isEmpty && (html == null || html.isEmpty)) {
            final rtfText = await convertRtf(
              decodeClipboardText(await rtf.readAsBytes()),
            );
            addText(rtfText.value, rtfText.parts);
            rich.add(await plain(rtfText.value, parent: rtfText.ticket));
          }
        }
      }
    } catch (error) {
      warnings.add(
        error is FormatException ? error.message : '有一项剪贴板内容无法读取，其余可读内容已保留。',
      );
    }
  }
  if (reader.items.length > 20) warnings.add('本次只读取前 20 项，请分次粘贴更多内容。');
  if (systemRead && windowsOfficeClipboard) {
    try {
      final office = await readOfficeClipboard(sequence);
      warnings.addAll((office['warnings'] as List? ?? []).cast<String>());
      formats.addAll((office['formats'] as List? ?? []).cast<String>());
      _PasteText? officeText;
      for (final raw in (office['files'] as List? ?? [])) {
        final entry = raw as Map;
        final bytes = entry['bytes'] as Uint8List;
        final name = '$prefix-${entry['name']}';
        if (name.endsWith('.rtf') &&
            files.any((f) => f.name.endsWith('.rtf'))) {
          continue;
        }
        if (name.endsWith('.png') &&
            files.any(
              (f) => IdeaAttachment.kindFor(f.name) == TextureKind.image,
            )) {
          continue;
        }
        await addFile(XFile.fromData(bytes, name: name, path: name));
        if (name.endsWith('.xml')) {
          try {
            final fragment = plugin == null
                ? spreadsheetToMarkdown(decodeClipboardText(bytes))
                : await plugin.capture(
                    'spreadsheet',
                    decodeClipboardText(bytes),
                  );
            if (fragment.markdown.isNotEmpty) {
              officeText = _PasteText(fragment.markdown, fragment.ticket);
            }
            warnings.addAll(fragment.warnings);
          } catch (_) {
            warnings.add('Excel 原始表格已保留为 XML 附件。');
          }
        }
        if (name.endsWith('.rtf') && texts.isEmpty && rich.isEmpty) {
          officeText = await convertRtf(decodeClipboardText(bytes));
          addText(officeText.value, officeText.parts);
        }
      }
      if (officeText != null && officeText.value.isNotEmpty) {
        rich
          ..clear()
          ..add(officeText);
      }
    } catch (_) {
      warnings.add('Office 原始对象未能读取，已保留其他可用内容。');
    }
    final after = await clipboardSequence();
    if (sequence != null && after != null && sequence != after) {
      throw const FormatException('读取期间剪贴板发生了变化，请重新粘贴。');
    }
  }
  return PastedContent(
    text: texts.join('\n\n'),
    markdown: rich.map((entry) => entry.value).join('\n\n'),
    textParts: _joinParts(textParts),
    markdownParts: _joinParts(rich.map((entry) => entry.parts)),
    files: files,
    warnings: warnings.toList(),
    formats: formats.toList(),
  );
}
