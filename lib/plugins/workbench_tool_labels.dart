import 'package:flutter/widgets.dart';
import 'package:morrow_core_client/ui_models.dart';
import 'package:morrow_i18n/morrow_i18n.dart';

/// Opt-in display adapter for the first-party ui.rs five-node text tool only.
/// No package, stored document, input value, event or conversion result is rewritten.
UiDocumentModel localizeWorkbenchToolDocument(
  BuildContext context,
  UiDocumentModel document,
) {
  final nodes = document.nodes;
  if (nodes.length != 5) return document;
  bool match(
    int index,
    String id,
    Kind kind, {
    Tone tone = Tone.normal,
    String action = '',
    int maxBytes = 0,
  }) {
    final n = nodes[index];
    return n.id == id &&
        n.parent == (index == 0 ? '' : 'root') &&
        n.kind == kind &&
        n.tone == tone &&
        n.action == action &&
        n.enabled &&
        !n.checked &&
        n.maxBytes == maxBytes;
  }

  if (!match(0, 'root', Kind.column) ||
      !match(1, 'heading', Kind.text, tone: Tone.emphasis) ||
      !match(2, 'text', Kind.textInput, action: 'text.edit', maxBytes: 1024) ||
      !match(3, 'count', Kind.text, tone: Tone.muted) ||
      !match(4, 'preview', Kind.text) ||
      nodes[1].text != '文字小工具' ||
      nodes[2].label != '输入文字') {
    return document;
  }
  final countMatch = RegExp(
    r'^(0|[1-9][0-9]{0,3}) 个字符 · 仅本次使用，不保存为卡片$',
  ).firstMatch(nodes[3].text);
  if (countMatch == null) return document;
  final count = int.parse(countMatch[1]!);
  if (count > 1024) return document;
  final l = L10n.of(context);
  UiNode display(UiNode n, {String? label, String? text}) => UiNode(
    id: n.id,
    parent: n.parent,
    kind: n.kind,
    label: label ?? n.label,
    text: text ?? n.text,
    action: n.action,
    enabled: n.enabled,
    checked: n.checked,
    maxBytes: n.maxBytes,
    tone: n.tone,
  );
  return UiDocumentModel([
    nodes[0],
    display(nodes[1], text: l.pluginsBuiltinHeading),
    display(nodes[2], label: l.pluginsBuiltinInput),
    display(nodes[3], text: l.pluginsBuiltinCount(count)),
    // Count belongs to the same last host preview, whereas the input node can
    // already contain a local pending draft. Never translate nonempty output.
    if (count == 0 && nodes[4].text == '输入后查看大写转换')
      display(nodes[4], text: l.pluginsBuiltinEmpty)
    else
      nodes[4],
  ]);
}
