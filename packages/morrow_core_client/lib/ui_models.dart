/// Validated immutable UI values, independent of transport or generated reflection metadata.
library;

import 'dart:convert';

enum Kind { column, row, text, button, textInput, toggle }

enum Tone { normal, muted, emphasis }

enum EventKind { activate, editText, setToggle }

Never _bad() => throw const FormatException('Invalid UI model');
void _id(String s) {
  if (s.isEmpty ||
      utf8.encode(s).length > 256 ||
      RegExp(r'[\x00-\x1f\x7f-\x9f/\\:]').hasMatch(s))
    _bad();
}

void _text(String s, int max) {
  if (utf8.encode(s).length > max ||
      RegExp(r'[\x00-\x08\x0b-\x1f\x7f-\x9f]').hasMatch(s))
    _bad();
}

final class UiNode {
  UiNode({
    required this.id,
    required this.parent,
    required this.kind,
    this.label = '',
    this.text = '',
    this.action = '',
    this.enabled = true,
    this.checked = false,
    this.maxBytes = 0,
    this.tone = Tone.normal,
  }) {
    _id(id);
    if (parent.isNotEmpty) _id(parent);
    _text(label, 512);
    _text(text, 4096);
    final interactive = [
      Kind.button,
      Kind.textInput,
      Kind.toggle,
    ].contains(kind);
    if (interactive) {
      _id(action);
      if (label.isEmpty) _bad();
    } else if (action.isNotEmpty || label.isNotEmpty || !enabled) {
      _bad();
    }
    if (kind != Kind.text && tone != Tone.normal) _bad();
    if (kind != Kind.toggle && checked) _bad();
    if (kind == Kind.textInput) {
      if (maxBytes < 1 ||
          maxBytes > 4096 ||
          utf8.encode(text).length > maxBytes)
        _bad();
    } else if (maxBytes != 0) {
      _bad();
    }
    if (kind != Kind.text && kind != Kind.textInput && text.isNotEmpty) _bad();
  }
  final String id, parent, label, text, action;
  final Kind kind;
  final Tone tone;
  final bool enabled, checked;
  final int maxBytes;
}

class UiDocumentModel {
  UiDocumentModel(List<UiNode> nodes) : nodes = List.unmodifiable(nodes) {
    if (nodes.isEmpty || nodes.length > 128) _bad();
    var totalTextBytes = 0;
    final seen = <String, (int, Kind)>{};
    for (var i = 0; i < nodes.length; i++) {
      final n = nodes[i];
      totalTextBytes += [
        n.id,
        n.parent,
        n.label,
        n.text,
        n.action,
      ].fold<int>(0, (total, s) => total + utf8.encode(s).length);
      if (totalTextBytes > 32768) _bad();
      int depth;
      if (i == 0) {
        if (n.parent.isNotEmpty || n.kind != Kind.column) _bad();
        depth = 1;
      } else {
        final parent = seen[n.parent];
        if (parent == null ||
            !(parent.$2 == Kind.column || parent.$2 == Kind.row))
          _bad();
        depth = parent.$1 + 1;
      }
      if (depth > 8 || seen.containsKey(n.id)) _bad();
      seen[n.id] = (depth, n.kind);
    }
  }
  final List<UiNode> nodes;
}
