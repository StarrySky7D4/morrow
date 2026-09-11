import 'dart:io';
import 'dart:typed_data';
import 'package:test/test.dart';
import 'package:capnproto_dart/capnproto_dart.dart';
import 'package:morrow_core_client/ui.dart';
import 'package:morrow_core_client/src/generated/ui.capnp.dart' as wire;
import 'package:morrow_core_client/src/generated/contract_identity.dart'
    as contract;

void main() {
  final bytes = File('test/fixtures/ui/document.capnp').readAsBytesSync();
  test('independent Rust form is immutable and bounded', () {
    final d = UiDocument.decode(bytes);
    expect(d.nodes.length, 5);
    expect(d.nodes[2].text, '灵感🌈');
    expect(d.nodes[2].maxBytes, 32);
    expect(() => d.nodes.clear(), throwsUnsupportedError);
  });
  test('rejects every truncation, trailing bytes and schema corruption', () {
    for (var i = 0; i < bytes.length; i++) {
      expect(
        () => UiDocument.decode(Uint8List.sublistView(bytes, 0, i)),
        throwsA(anything),
      );
    }
    expect(
      () => UiDocument.decode(Uint8List.fromList([...bytes, 0])),
      throwsA(anything),
    );
    final corrupt = Uint8List.fromList(bytes);
    for (var i = 0; i <= corrupt.length - 32; i++) {
      if (List.generate(32, (j) => corrupt[i + j]).toString() ==
          contract.uiDigest.toString()) {
        corrupt[i] ^= 1;
        break;
      }
    }
    expect(() => UiDocument.decode(corrupt), throwsA(anything));
  });
  test('malformed tree and interactive field combinations reject', () {
    for (var mode = 0; mode < 5; mode++) {
      final m = MessageBuilder();
      final r = m.initRoot(wire.documentFactory);
      r.version = contract.uiProtocolVersion;
      r.schemaDigest = Uint8List.fromList(contract.uiDigest);
      final ns = r.initNodes(2);
      ns[0].id = 'root';
      ns[0].kind = wire.Kind.column;
      ns[1].id = mode == 0 ? 'root' : 'child';
      ns[1].parent = mode == 1 ? 'child' : 'root';
      ns[1].kind = mode == 2 ? wire.Kind.button : wire.Kind.text;
      if (mode == 3) ns[1].checked = true;
      if (mode == 4) ns[1].tone = wire.Tone.emphasis;
      final encoded = m.serialize();
      if (mode == 4) {
        expect(UiDocument.decode(encoded).nodes[1].tone, Tone.emphasis);
      } else {
        expect(() => UiDocument.decode(encoded), throwsA(anything));
      }
    }
  });
  test('events preserve all UInt64 bits and separate event payloads', () {
    final e = UiEvent.decode(
      File('test/fixtures/ui/expected-event.capnp').readAsBytesSync(),
    );
    expect(e.generation, (BigInt.one << 64) - BigInt.one);
    expect(e.text, '从 Dart 编辑🌈');
    expect(UiEvent.decode(e.encode()).generation, e.generation);
    expect(
      () => UiEvent(
        view: 'view',
        generation: BigInt.one,
        revision: BigInt.one,
        serial: BigInt.one,
        node: 'n',
        action: 'a',
        kind: EventKind.activate,
        text: 'forged',
      ),
      throwsA(anything),
    );
  });
  test('expanded text exceeds budget even inside a bounded frame', () {
    final m = MessageBuilder();
    final r = m.initRoot(wire.documentFactory);
    r.version = contract.uiProtocolVersion;
    r.schemaDigest = Uint8List.fromList(contract.uiDigest);
    final nodes = r.initNodes(10);
    nodes[0].id = 'root';
    nodes[0].kind = wire.Kind.column;
    for (var i = 1; i < 10; i++) {
      nodes[i].id = 'n$i';
      nodes[i].parent = 'root';
      nodes[i].kind = wire.Kind.text;
      nodes[i].text = 'x' * 4096;
    }
    final bytes = m.serialize();
    expect(bytes.length, lessThan(65536));
    expect(() => UiDocument.decode(bytes), throwsA(anything));
  });
}
