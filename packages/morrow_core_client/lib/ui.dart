/// Experimental runtime UI messages. Descriptions and action IDs grant no permissions.
library;

import 'dart:convert';
import 'dart:typed_data';
import 'package:capnproto_dart/capnproto_dart.dart';
import 'src/generated/ui.capnp.dart' as wire;
import 'src/generated/contract_identity.dart' as contract;
export 'src/generated/ui.capnp.dart' show Kind, Tone, EventKind;

const maxUiBytes = 65536;
final _maxUint64 = (BigInt.one << 64) - BigInt.one;
Never _bad() => throw const FormatException('Invalid UI message');
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

void _uint64(BigInt n) {
  if (n <= BigInt.zero || n > _maxUint64) _bad();
}

void _frame(Uint8List bytes) {
  if (bytes.length < 8 || bytes.length > maxUiBytes) _bad();
  final d = ByteData.sublistView(bytes);
  final count = d.getUint32(0, Endian.little) + 1;
  if (count > 512) _bad();
  final header = ((count + 2) ~/ 2) * 8;
  if (header > bytes.length) _bad();
  var size = header;
  for (var i = 0; i < count; i++) {
    final words = d.getUint32(4 + i * 4, Endian.little);
    if (words > 8192) _bad();
    size += words * 8;
    if (size > bytes.length) _bad();
  }
  if (size != bytes.length) _bad();
}

MessageReader _reader(Uint8List bytes) {
  _frame(bytes);
  return MessageReader.deserialize(
    bytes,
    const MessageReaderOptions(
      traversalLimitInWords: 8192,
      nestingLimit: 16,
      maxSegments: 512,
    ),
  );
}

void _contract(int version, List<int>? digest) {
  if (version != contract.uiProtocolVersion ||
      digest == null ||
      digest.length != 32)
    _bad();
  for (var i = 0; i < 32; i++) {
    if (digest[i] != contract.uiDigest[i]) _bad();
  }
}

final class UiNode {
  UiNode._(wire.NodeReader r)
    : id = r.id ?? '',
      parent = r.parent ?? '',
      kind = r.kind ?? _bad(),
      label = r.label ?? '',
      text = r.text ?? '',
      action = r.action ?? '',
      enabled = r.enabled,
      checked = r.checked,
      maxBytes = r.maxBytes,
      tone = r.tone ?? _bad() {
    _id(id);
    if (parent.isNotEmpty) _id(parent);
    _text(label, 512);
    _text(text, 4096);
    final interactive = [
      wire.Kind.button,
      wire.Kind.textInput,
      wire.Kind.toggle,
    ].contains(kind);
    if (interactive) {
      _id(action);
      if (label.isEmpty) _bad();
    } else if (action.isNotEmpty || label.isNotEmpty || !enabled) {
      _bad();
    }
    if (kind != wire.Kind.text && tone != wire.Tone.normal) _bad();
    if (kind != wire.Kind.toggle && checked) _bad();
    if (kind == wire.Kind.textInput) {
      if (maxBytes < 1 ||
          maxBytes > 4096 ||
          utf8.encode(text).length > maxBytes)
        _bad();
    } else if (maxBytes != 0) {
      _bad();
    }
    if (kind != wire.Kind.text &&
        kind != wire.Kind.textInput &&
        text.isNotEmpty)
      _bad();
  }
  final String id, parent, label, text, action;
  final wire.Kind kind;
  final wire.Tone tone;
  final bool enabled, checked;
  final int maxBytes;
}

final class UiDocument {
  UiDocument._(this.nodes);
  final List<UiNode> nodes;
  static UiDocument decode(Uint8List bytes) {
    final root = _reader(bytes).getRoot(wire.documentFactory);
    _contract(root.version, root.schemaDigest);
    final list = root.nodes;
    if (list == null || list.length < 1 || list.length > 128) _bad();
    final nodes = <UiNode>[];
    var totalTextBytes = 0;
    final seen = <String, (int, wire.Kind)>{};
    for (var i = 0; i < list.length; i++) {
      final n = UiNode._(list[i]);
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
        if (n.parent.isNotEmpty || n.kind != wire.Kind.column) _bad();
        depth = 1;
      } else {
        final parent = seen[n.parent];
        if (parent == null ||
            !(parent.$2 == wire.Kind.column || parent.$2 == wire.Kind.row))
          _bad();
        depth = parent.$1 + 1;
      }
      if (depth > 8 || seen.containsKey(n.id)) _bad();
      seen[n.id] = (depth, n.kind);
      nodes.add(n);
    }
    return UiDocument._(List.unmodifiable(nodes));
  }
}

final class UiEvent {
  UiEvent({
    required this.view,
    required this.generation,
    required this.revision,
    required this.serial,
    required this.node,
    required this.action,
    required this.kind,
    this.text = '',
    this.checked = false,
  }) {
    _validate();
  }
  final String view, node, action, text;
  final BigInt generation, revision, serial;
  final wire.EventKind kind;
  final bool checked;
  void _validate() {
    _id(view);
    _id(node);
    _id(action);
    _uint64(generation);
    _uint64(revision);
    _uint64(serial);
    _text(text, 4096);
    if (kind != wire.EventKind.editText && text.isNotEmpty) _bad();
    if (kind != wire.EventKind.setToggle && checked) _bad();
  }

  Uint8List encode() {
    _validate();
    final m = MessageBuilder();
    final r = m.initRoot(wire.eventFactory);
    r.version = contract.uiProtocolVersion;
    r.schemaDigest = Uint8List.fromList(contract.uiDigest);
    r.view = view;
    r.generation = generation.toSigned(64).toInt();
    r.revision = revision.toSigned(64).toInt();
    r.serial = serial.toSigned(64).toInt();
    r.node = node;
    r.action = action;
    r.kind = kind;
    r.text = text;
    r.checked = checked;
    final bytes = m.serialize();
    _frame(bytes);
    return bytes;
  }

  static UiEvent decode(Uint8List bytes) {
    final r = _reader(bytes).getRoot(wire.eventFactory);
    _contract(r.version, r.schemaDigest);
    return UiEvent(
      view: r.view ?? '',
      generation: BigInt.from(r.generation).toUnsigned(64),
      revision: BigInt.from(r.revision).toUnsigned(64),
      serial: BigInt.from(r.serial).toUnsigned(64),
      node: r.node ?? '',
      action: r.action ?? '',
      kind: r.kind ?? _bad(),
      text: r.text ?? '',
      checked: r.checked,
    );
  }
}
