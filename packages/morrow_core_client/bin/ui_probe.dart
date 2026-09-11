import 'dart:io';
import 'package:morrow_core_client/ui.dart';

void main(List<String> args) {
  if (args.length != 1) throw ArgumentError('UI vector directory required');
  final root = args.single;
  final doc = UiDocument.decode(File('$root/document.capnp').readAsBytesSync());
  if (doc.nodes.length != 5 ||
      doc.nodes[2].text != '灵感🌈' ||
      doc.nodes[2].maxBytes != 32)
    throw StateError('Rust document mismatch');
  final expected = UiEvent.decode(
    File('$root/expected-event.capnp').readAsBytesSync(),
  );
  final event = UiEvent(
    view: 'view',
    generation: (BigInt.one << 64) - BigInt.one,
    revision: BigInt.one,
    serial: BigInt.one,
    node: 'title',
    action: 'title.edit',
    kind: EventKind.editText,
    text: '从 Dart 编辑🌈',
  );
  if (expected.generation != event.generation || expected.text != event.text)
    throw StateError('Rust event mismatch');
  File('$root/dart-event.capnp').writeAsBytesSync(event.encode());
  print(
    'PASS: decoded independent Rust UI document; encoded actual Dart event with full UInt64 and Unicode',
  );
}
