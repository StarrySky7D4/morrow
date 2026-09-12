import 'dart:typed_data';
import 'package:capnproto_dart/capnproto_dart.dart';
import 'package:file_selector/file_selector.dart';
import 'package:html/dom.dart' as dom;
import 'package:html/parser.dart' as html;
import 'package:xml/xml.dart';
import '../content/rich_content.dart';
import 'workbench_native.dart';
import 'generated/capture.capnp.dart' as wire;
import 'generated/identity.dart' as contract;

/// Platform normalization only: parse inert syntax and detach embedded bytes.
/// Markdown, tables, styles and RTF text are produced by the Rust guest.
Future<RichFragment> captureWithPlugin(
  RustWorkbench host,
  String format,
  String source,
  String prefix,
) async {
  if (source.length > 2 * 1024 * 1024) {
    throw const FormatException('内容过大，请导入原始文件。');
  }
  final files = <XFile>[];
  final warnings = <String>[];
  final nodes = <Map<String, Object>>[
    {'parent': 0, 'tag': 'root'},
  ];
  int add(Map<String, Object> node) {
    if (nodes.length >= 1024) {
      throw const FormatException('富文本结构较多，原始文件仍会保留。');
    }
    nodes.add(node);
    return nodes.length - 1;
  }

  void visitHtml(dom.Node node, int parent, int depth) {
    if (depth > 40) return;
    if (node is dom.Text) {
      add({'parent': parent, 'tag': 'text', 'text': node.text});
      return;
    }
    if (node is! dom.Element) return;
    final attributes = node.attributes;
    var src = attributes['src'] ?? '';
    if (node.localName == 'img' && src.startsWith('data:image/')) {
      try {
        final data = UriData.parse(src);
        if (![
              'image/png',
              'image/jpeg',
              'image/gif',
              'image/webp',
            ].contains(data.mimeType) ||
            files.length >= 10) {
          throw const FormatException('image');
        }
        final extension = data.mimeType == 'image/jpeg'
            ? 'jpg'
            : data.mimeType.split('/').last;
        final name = '$prefix-${files.length + 1}.$extension';
        files.add(
          XFile.fromData(data.contentAsBytes(), name: name, path: name),
        );
        src = 'attachment:$name';
      } catch (_) {
        src = '';
        warnings.add('有一张内嵌图片无法读取，请单独导入。');
      }
    }
    final id = add({
      'parent': parent,
      'tag': node.localName ?? '',
      'href': attributes['href'] ?? '',
      'src': src,
      'alt': attributes['alt'] ?? '',
      'style': attributes['style'] ?? '',
      'columns': (int.tryParse(attributes['colspan'] ?? '') ?? 1).clamp(1, 80),
      'rows': (int.tryParse(attributes['rowspan'] ?? '') ?? 1).clamp(1, 500),
    });
    for (final child in node.nodes) {
      visitHtml(child, id, depth + 1);
    }
  }

  void visitXml(XmlNode node, int parent, int depth) {
    if (depth > 40) return;
    if (node is XmlText) {
      add({'parent': parent, 'tag': 'text', 'text': node.value});
      return;
    }
    if (node is! XmlElement) return;
    String attr(String name) =>
        node.attributes.where((a) => a.name.local == name).firstOrNull?.value ??
        '';
    final id = add({
      'parent': parent,
      'tag': node.name.local,
      'index': (int.tryParse(attr('Index')) ?? 0).clamp(0, 80),
      'formula': attr('Formula'),
    });
    for (final child in node.children) {
      visitXml(child, id, depth + 1);
    }
  }

  if (format == 'html') {
    for (final child in html.parseFragment(source).nodes) {
      visitHtml(child, 0, 0);
    }
  }
  if (format == 'spreadsheet') {
    for (final child in XmlDocument.parse(source).children) {
      visitXml(child, 0, 0);
    }
  }
  final message = MessageBuilder();
  final r = message.initRoot(wire.requestFactory);
  r.version = 1;
  r.digest = Uint8List.fromList(contract.captureDigest);
  r.format = format;
  if (format == 'plain' || format == 'rtf') {
    r.source = source;
  }
  final list = r.initNodes(nodes.length);
  for (var i = 0; i < nodes.length; i++) {
    final n = nodes[i];
    final out = list[i];
    out.parent = n['parent'] as int;
    out.tag = n['tag'] as String;
    out.text = n['text'] as String? ?? '';
    out.href = n['href'] as String? ?? '';
    out.src = n['src'] as String? ?? '';
    out.alt = n['alt'] as String? ?? '';
    out.style = n['style'] as String? ?? '';
    out.columns = n['columns'] as int? ?? 1;
    out.rows = n['rows'] as int? ?? 1;
    out.index = n['index'] as int? ?? 0;
    out.formula = n['formula'] as String? ?? '';
  }
  final request = message.serialize();
  if (request.length > 65536) {
    throw const FormatException('内容较长，请使用保留的原始附件。');
  }
  final response = await host.capture(request);
  final result = RustWorkbench.readMessage(
    response,
  ).getRoot(wire.responseFactory);
  if (result.version != 1 ||
      result.digest == null ||
      result.digest!.length != contract.captureDigest.length ||
      List.generate(
        contract.captureDigest.length,
        (i) => result.digest![i] != contract.captureDigest[i],
      ).any((v) => v)) {
    throw const FormatException('内容转换版本不匹配');
  }
  return RichFragment(
    result.markdown ?? '',
    files: files,
    warnings: [
      ...warnings,
      ...[...?result.warnings].whereType<String>(),
    ],
  );
}
