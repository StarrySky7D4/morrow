import 'dart:convert';
import 'dart:typed_data';
import 'package:file_selector/file_selector.dart';
import 'package:html/dom.dart' as dom;
import 'package:html/parser.dart' as html;
import 'package:xml/xml.dart';

class RichFragment {
  const RichFragment(
    this.markdown, {
    this.files = const [],
    this.warnings = const [],
    this.ticket,
  });
  final String markdown;
  final String? ticket;
  final List<XFile> files;
  final List<String> warnings;
}

String markdownCell(String value) =>
    value.trim().replaceAll('|', r'\|').replaceAll(RegExp(r'[\r\n]+'), ' / ');
String markdownTable(List<List<String>> rows) {
  if (rows.isEmpty) return '';
  final columns = rows
      .fold<int>(0, (n, row) => n > row.length ? n : row.length)
      .clamp(1, 80);
  String row(List<String> values) =>
      '| ${List.generate(columns, (i) => i < values.length ? markdownCell(values[i]) : '').join(' | ')} |';
  return [
    row(rows.first),
    '| ${List.filled(columns, '---').join(' | ')} |',
    ...rows.skip(1).take(499).map(row),
  ].join('\n');
}

String? safeContentLink(String value) {
  final uri = Uri.tryParse(value.trim());
  if (uri == null || uri.userInfo.isNotEmpty) return null;
  if (['https', 'http'].contains(uri.scheme) && uri.host.isNotEmpty) {
    return uri.toString();
  }
  if (uri.scheme == 'mailto' && uri.path.isNotEmpty) return uri.toString();
  if (uri.scheme == 'attachment' && uri.path.isNotEmpty) return uri.toString();
  return null;
}

/// HTML is converted to inert, editable Markdown, never executed.
RichFragment htmlToMarkdown(String source, {String imagePrefix = 'clipboard'}) {
  if (source.length > 2 * 1024 * 1024) {
    throw const FormatException('富文本超过 2 MB，请将文档作为附件导入。');
  }
  final root = html.parseFragment(source);
  final files = <XFile>[];
  final warnings = <String>{};
  String visit(dom.Node node, int depth) {
    if (depth > 40) return '';
    if (node is dom.Text) return node.text.replaceAll(RegExp(r'\s+'), ' ');
    if (node is! dom.Element) return '';
    final tag = node.localName ?? '';
    if ([
      'script',
      'style',
      'iframe',
      'object',
      'embed',
      'head',
      'meta',
      'link',
    ].contains(tag)) {
      return '';
    }
    String children() =>
        node.nodes.map((child) => visit(child, depth + 1)).join();
    if (tag == 'br') return '\n';
    if (tag == 'img') {
      final src = node.attributes['src'] ?? '';
      final alt = (node.attributes['alt'] ?? '图片').replaceAll(
        RegExp(r'[\[\]]'),
        '',
      );
      if (src.startsWith('data:image/')) {
        try {
          final data = UriData.parse(src);
          if (![
                'image/png',
                'image/jpeg',
                'image/gif',
                'image/webp',
              ].contains(data.mimeType) ||
              files.length >= 10) {
            warnings.add('部分内嵌图片需要作为文件单独导入。');
            return alt;
          }
          final ext = data.mimeType == 'image/jpeg'
              ? 'jpg'
              : data.mimeType.split('/').last;
          final name = '$imagePrefix-${files.length + 1}.$ext';
          files.add(
            XFile.fromData(
              Uint8List.fromList(data.contentAsBytes()),
              name: name,
              path: name,
            ),
          );
          return '![$alt](attachment:$name)';
        } catch (_) {
          warnings.add('有一张内嵌图片无法读取。');
          return alt;
        }
      }
      final link = safeContentLink(src);
      if (link != null) return '![$alt](<$link>)';
      warnings.add('本地链接图片没有自动读取，请粘贴图片或导入原文件。');
      return alt;
    }
    if (tag == 'table') {
      final rows = <List<String>>[];
      final occupied = <int, int>{};
      var rowIndex = 0;
      for (final tr in node.querySelectorAll('tr').take(500)) {
        dom.Element? ancestor = tr.parent;
        while (ancestor != null && ancestor.localName != 'table') {
          ancestor = ancestor.parent;
        }
        if (ancestor != node) continue;
        final cells = <String>[];
        for (final cell in tr.children.where(
          (c) => c.localName == 'td' || c.localName == 'th',
        )) {
          while (cells.length < 80 &&
              (occupied[cells.length] ?? 0) > rowIndex) {
            cells.add('');
          }
          if (cells.length >= 80) break;
          final column = cells.length;
          cells.add(cell.nodes.map((n) => visit(n, depth + 1)).join().trim());
          final span = (int.tryParse(cell.attributes['colspan'] ?? '1') ?? 1)
              .clamp(1, 80);
          final rowSpan = (int.tryParse(cell.attributes['rowspan'] ?? '1') ?? 1)
              .clamp(1, 500);
          for (var c = column; c < (column + span).clamp(0, 80); c++) {
            occupied[c] = rowIndex + rowSpan;
          }
          if (span > 1 || rowSpan > 1) {
            warnings.add('合并单元格已转为阅读表格；原始排版保留在 HTML 附件。');
          }
          cells.addAll(
            List.filled(
              (span - 1).clamp(0, 80 - cells.length.clamp(0, 80)),
              '',
            ),
          );
          if (cells.length >= 80) break;
        }
        if (cells.isNotEmpty) rows.add(cells);
        rowIndex++;
      }
      return '\n\n${markdownTable(rows)}\n\n';
    }
    if (tag == 'pre') {
      final text = node.text;
      var fence = '```';
      while (text.contains(fence)) {
        fence += '`';
      }
      return '\n\n$fence\n$text\n$fence\n\n';
    }
    final text = children();
    if (tag == 'code') return '`$text`';
    if (tag == 'strong' || tag == 'b') {
      return text.trim().isEmpty ? text : '**${text.trim()}**';
    }
    if (tag == 'em' || tag == 'i') {
      return text.trim().isEmpty ? text : '*${text.trim()}*';
    }
    if (tag == 'del' || tag == 's') return '~~$text~~';
    if (tag == 'a') {
      final link = safeContentLink(node.attributes['href'] ?? '');
      return link == null ? text : '[${text.trim()}](<$link>)';
    }
    if (RegExp(r'^h[1-6]$').hasMatch(tag)) {
      return '\n\n${'#' * int.parse(tag[1])} ${text.trim()}\n\n';
    }
    if (tag == 'li') {
      return '\n${node.parent?.localName == 'ol' ? '1.' : '-'} ${text.trim()}';
    }
    if (tag == 'blockquote') {
      return '\n\n> ${text.trim().replaceAll('\n', '\n> ')}\n\n';
    }
    if (['p', 'div', 'section', 'article', 'ul', 'ol'].contains(tag)) {
      return '\n\n${text.trim()}\n\n';
    }
    if (tag == 'span') {
      final style = node.attributes['style'] ?? '';
      if (RegExp(r'font-weight\s*:\s*(bold|[7-9]00)').hasMatch(style)) {
        return '**${text.trim()}**';
      }
      if (style.contains('italic')) return '*${text.trim()}*';
    }
    return text;
  }

  final text = root.nodes
      .map((n) => visit(n, 0))
      .join()
      .replaceAll(RegExp(r'[ \t]+\n'), '\n')
      .replaceAll(RegExp(r'\n{3,}'), '\n\n')
      .trim();
  return RichFragment(text, files: files, warnings: warnings.toList());
}

RichFragment spreadsheetToMarkdown(String source) {
  if (source.length > 2 * 1024 * 1024) {
    throw const FormatException('表格过大，请导入 Excel 文件。');
  }
  final document = XmlDocument.parse(source);
  final tables = <String>[];
  for (final table
      in document.descendants
          .whereType<XmlElement>()
          .where((e) => e.name.local == 'Table')
          .take(8)) {
    final rows = <List<String>>[];
    for (final row
        in table.childElements.where((e) => e.name.local == 'Row').take(500)) {
      final cells = <String>[];
      for (final cell in row.childElements.where(
        (e) => e.name.local == 'Cell',
      )) {
        String? attr(String name) => cell.attributes
            .where((a) => a.name.local == name)
            .firstOrNull
            ?.value;
        final index = (int.tryParse(attr('Index') ?? '') ?? cells.length + 1)
            .clamp(1, 80);
        while (cells.length < index - 1) {
          cells.add('');
        }
        final value =
            cell.childElements
                .where((e) => e.name.local == 'Data')
                .firstOrNull
                ?.innerText ??
            '';
        final formula = attr('Formula');
        cells.add(formula == null ? value : '$value (公式: $formula)');
        if (cells.length >= 80) break;
      }
      rows.add(cells);
    }
    tables.add(markdownTable(rows));
  }
  return RichFragment(
    tables.where((s) => s.isNotEmpty).join('\n\n'),
    warnings: const ['表格显示值与公式已转为 Markdown，格式和合并信息保留在 XML 附件。'],
  );
}

String plainTextToMarkdown(String source) {
  final lines = source.trim().split(RegExp(r'\r?\n'));
  if (lines.length >= 2 &&
      lines.where((line) => line.contains('\t')).length >= 2) {
    return markdownTable(
      lines
          .take(500)
          .map((line) => line.split('\t').take(80).toList())
          .toList(),
    );
  }
  return source;
}

/// Bounded text fallback for RTF-only sources; the original RTF is also saved.
String rtfToPlainText(String source) {
  if (source.length > 8 * 1024 * 1024) {
    throw const FormatException('RTF 过大，请导入原始文档。');
  }
  final out = StringBuffer();
  var skip = false, uc = 1, fallback = 0;
  final stack = <(bool, int)>[];
  const destinations = {
    'fonttbl',
    'colortbl',
    'stylesheet',
    'info',
    'pict',
    'object',
    'objdata',
    'listtable',
    'listoverridetable',
    'generator',
    'datastore',
    'themedata',
    'xmlopen',
    'xmlattrname',
  };
  for (var i = 0; i < source.length; i++) {
    final c = source[i];
    if (c == '{') {
      stack.add((skip, uc));
      continue;
    }
    if (c == '}') {
      if (stack.isNotEmpty) {
        final state = stack.removeLast();
        skip = state.$1;
        uc = state.$2;
      }
      continue;
    }
    if (c != '\\') {
      if (fallback > 0) {
        fallback--;
      } else if (!skip && c != '\r' && c != '\n') {
        out.write(c);
      }
      continue;
    }
    if (++i >= source.length) break;
    final next = source[i];
    if (next == '*') {
      skip = true;
      continue;
    }
    if (next == "'") {
      if (i + 2 < source.length) {
        final value = int.tryParse(source.substring(i + 1, i + 3), radix: 16);
        if (fallback > 0) {
          fallback--;
        } else if (!skip && value != null) {
          out.writeCharCode(value);
        }
        i += 2;
      }
      continue;
    }
    if (!RegExp('[a-zA-Z]').hasMatch(next)) {
      if (fallback > 0) {
        fallback--;
      } else if (!skip) {
        if (next == '~') {
          out.write(' ');
        } else if (next == '\\' || next == '{' || next == '}') {
          out.write(next);
        }
      }
      continue;
    }
    final start = i;
    while (i + 1 < source.length &&
        RegExp('[a-zA-Z]').hasMatch(source[i + 1])) {
      i++;
    }
    final word = source.substring(start, i + 1);
    var end = i + 1;
    if (end < source.length && source[end] == '-') end++;
    while (end < source.length && RegExp('[0-9]').hasMatch(source[end])) {
      end++;
    }
    final number = int.tryParse(source.substring(i + 1, end));
    i = end - 1;
    if (end < source.length && source[end] == ' ') i++;
    if (destinations.contains(word)) skip = true;
    if (word == 'uc' && number != null) uc = number.clamp(0, 16);
    if (skip) continue;
    if (word == 'u' && number != null) {
      out.writeCharCode(number < 0 ? number + 65536 : number);
      fallback = uc;
    }
    if (word == 'par' || word == 'line' || word == 'row') out.write('\n');
    if (word == 'tab' || word == 'cell') out.write('\t');
  }
  return out.toString().trim();
}

String decodeClipboardText(List<int> bytes) {
  if ((bytes.length >= 2 && bytes[0] == 0xff && bytes[1] == 0xfe) ||
      (bytes.length >= 4 && bytes[1] == 0 && bytes[3] == 0)) {
    final offset = bytes[0] == 0xff ? 2 : 0;
    return String.fromCharCodes([
      for (var i = offset; i + 1 < bytes.length; i += 2)
        bytes[i] | bytes[i + 1] << 8,
    ]).replaceAll('\u0000', '');
  }
  return utf8.decode(bytes, allowMalformed: true).replaceAll('\u0000', '');
}
