/// Experimental runtime UI messages. Descriptions and action IDs grant no permissions.
library;

import 'dart:convert';
import 'dart:typed_data';
import 'package:capnproto_dart/capnproto_dart.dart';
import 'src/generated/ui.capnp.dart' as wire;
import 'src/generated/contract_identity.dart' as contract;
import 'ui_models.dart';
export 'ui_models.dart';

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

final class UiDocument extends UiDocumentModel {
  UiDocument._(super.nodes);
  static UiDocument decode(Uint8List bytes) {
    final root = _reader(bytes).getRoot(wire.documentFactory);
    _contract(root.version, root.schemaDigest);
    final list = root.nodes;
    if (list == null || list.length < 1 || list.length > 128) _bad();
    return UiDocument._([for (var i = 0; i < list.length; i++) _node(list[i])]);
  }

  static UiNode _node(wire.NodeReader n) => UiNode(
    id: n.id ?? '',
    parent: n.parent ?? '',
    kind: Kind.values[(n.kind ?? _bad()).index],
    label: n.label ?? '',
    text: n.text ?? '',
    action: n.action ?? '',
    enabled: n.enabled,
    checked: n.checked,
    maxBytes: n.maxBytes,
    tone: Tone.values[(n.tone ?? _bad()).index],
  );
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
  final EventKind kind;
  final bool checked;
  void _validate() {
    _id(view);
    _id(node);
    _id(action);
    _uint64(generation);
    _uint64(revision);
    _uint64(serial);
    _text(text, 4096);
    if (kind != EventKind.editText && text.isNotEmpty) _bad();
    if (kind != EventKind.setToggle && checked) _bad();
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
    r.kind = wire.EventKind.values[kind.index];
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
      kind: EventKind.values[(r.kind ?? _bad()).index],
      text: r.text ?? '',
      checked: r.checked,
    );
  }
}
