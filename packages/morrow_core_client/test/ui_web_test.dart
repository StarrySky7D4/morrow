import 'dart:convert';
import 'dart:typed_data';
import 'package:test/test.dart';
import 'package:crypto/crypto.dart';
import 'package:capnproto_dart/capnproto_dart.dart';
import 'package:morrow_core_client/ui.dart';
import 'package:morrow_core_client/src/generated/ui.capnp.dart' as wire;
import 'package:morrow_core_client/src/generated/contract_identity.dart'
    as contract;
import 'fixtures/ui_cross_platform.dart';

void main() {
  test('exact reflection IDs remain available without JS int conversion', () {
    expect(wire.schemaId('kindSchema').toRadixString(16), '9374a4065674ce61');
    expect(wire.schemaId('eventSchema').toRadixString(16), 'd53af29d5b654897');
    expect(wire.schemaIds.length, 6);
    final StructFactory<wire.EventReader, wire.EventBuilder> factory =
        wire.eventFactory;
    if (const bool.fromEnvironment('dart.library.js_interop')) {
      expect(wire.schemaReflectionAvailable, isFalse);
      expect(factory.schema, isNull);
    } else {
      expect(wire.schemaReflectionAvailable, isTrue);
      expect(
        BigInt.from(factory.schema!.id).toUnsigned(64),
        wire.schemaId('eventSchema'),
      );
    }
  });
  test('original Rust document and max UInt64 event decode on VM and JS', () {
    final event = base64Decode(rustEvent);
    final document = base64Decode(rustDocument);
    expect(sha256.convert(event).toString(), rustEventSha256);
    expect(sha256.convert(document).toString(), rustDocumentSha256);
    expect(UiDocument.decode(document).nodes[2].text, '灵感🌈');
    final parsed = UiEvent.decode(event);
    expect(parsed.generation, (BigInt.one << 64) - BigInt.one);
    expect(parsed.text, '从 Dart 编辑🌈');
    final again = UiEvent.decode(parsed.encode());
    expect(again.generation, parsed.generation);
    expect(again.revision, parsed.revision);
    expect(again.serial, parsed.serial);
    expect(again.text, parsed.text);
  });
  test(
    'all UInt64 boundaries preserve independent exact little-endian bytes',
    () {
      final values = [
        BigInt.one,
        (BigInt.one << 53) - BigInt.one,
        BigInt.one << 53,
        (BigInt.one << 53) + BigInt.one,
        (BigInt.one << 63) - BigInt.one,
        BigInt.one << 63,
        (BigInt.one << 63) + BigInt.one,
        (BigInt.one << 64) - BigInt.one,
      ];
      for (var i = 0; i < values.length; i++) {
        final fields = [
          values[i],
          values[(i + 1) % values.length],
          values[(i + 2) % values.length],
        ];
        final value = UiEvent(
          view: 'view',
          generation: fields[0],
          revision: fields[1],
          serial: fields[2],
          node: 'field',
          action: 'edit',
          kind: EventKind.editText,
          text: 'exact🌈',
        );
        final bytes = value.encode();
        final root = MessageReader.deserialize(
          bytes,
        ).getRoot(wire.eventFactory);
        for (var field = 0; field < fields.length; field++) {
          final hex = fields[field].toRadixString(16).padLeft(16, '0');
          final expected = [
            for (var byte = 0; byte < 8; byte++)
              int.parse(hex.substring(14 - byte * 2, 16 - byte * 2), radix: 16),
          ];
          expect([
            for (var byte = 0; byte < 8; byte++)
              root.getUint8Field(8 + field * 8 + byte),
          ], expected);
        }
        final parsed = UiEvent.decode(bytes);
        expect([parsed.generation, parsed.revision, parsed.serial], fields);
        expect(parsed.encode(), bytes);
      }
    },
  );
  test(
    'browser path still rejects schema corruption, tails and invalid UInt64',
    () {
      final bytes = base64Decode(rustEvent);
      final bad = Uint8List.fromList(bytes);
      var corrupted = false;
      for (var i = 0; i <= bad.length - contract.uiDigest.length; i++) {
        if (List.generate(32, (j) => bad[i + j]).toString() ==
            contract.uiDigest.toString()) {
          bad[i] ^= 1;
          corrupted = true;
          break;
        }
      }
      expect(corrupted, isTrue);
      expect(() => UiEvent.decode(bad), throwsA(anything));
      expect(
        () => UiEvent.decode(Uint8List.fromList([...bytes, 0])),
        throwsA(anything),
      );
      for (final value in [BigInt.zero, BigInt.one << 64]) {
        expect(
          () => UiEvent(
            view: 'v',
            generation: value,
            revision: BigInt.one,
            serial: BigInt.one,
            node: 'n',
            action: 'a',
            kind: EventKind.activate,
          ),
          throwsA(anything),
        );
      }
    },
  );
}
