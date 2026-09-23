import 'dart:typed_data';
import 'package:capnproto_dart/capnproto_dart.dart';
import 'package:morrow_core_client/morrow_core_client.dart';
import 'package:morrow_core_client/src/generated/runtime.capnp.dart' as wire;

void main() {
  final values = <BigInt>{
    for (final s in [
      '1',
      '9007199254740992',
      '9007199254740993',
      '9007199254740994',
      '9223372036854775807',
      '9223372036854775808',
      '9223372036854775809',
      '18446744073709551614',
      '18446744073709551615',
    ])
      BigInt.parse(s),
    for (var bit = 0; bit < 64; bit++) BigInt.one << bit,
  };
  var seed = BigInt.parse('123456789abcdef', radix: 16);
  final mask = BigInt.parse('ffffffffffffffff', radix: 16);
  for (var i = 0; i < 64; i++) {
    seed = (seed * BigInt.parse('6364136223846793005') + BigInt.one) & mask;
    values.add(seed);
  }
  for (final value in values) {
    final bytes = RenameCommand(
      operationId: 'op',
      cardId: 'card',
      expectedRevision: value,
      title: 'u64',
    ).encode();
    final rename = MessageReader.deserialize(
      bytes,
    ).getRoot(wire.requestFactory).renameCard!;
    // Independent wire oracle: decimal input -> padded hex -> little endian bytes.
    final hex = value.toRadixString(16).padLeft(16, '0');
    final expected = [
      for (var i = 14; i >= 0; i -= 2)
        int.parse(hex.substring(i, i + 2), radix: 16),
    ];
    final actual = ByteData(8)
      ..setUint32(0, rename.getUint32Field(0), Endian.little)
      ..setUint32(4, rename.getUint32Field(4), Endian.little);
    if (actual.buffer.asUint8List().toString() != expected.toString())
      throw StateError('wire mismatch $value');
    if (RenameCommand.decode(bytes).expectedRevision != value)
      throw StateError('domain mismatch $value');
  }
  for (final invalid in [-BigInt.one, BigInt.zero, mask + BigInt.one]) {
    var rejected = false;
    try {
      RenameCommand(
        operationId: 'op',
        cardId: 'card',
        expectedRevision: invalid,
        title: 'x',
      ).encode();
    } on FormatException {
      rejected = true;
    }
    if (!rejected) throw StateError('invalid value admitted');
  }
  print('u64: ${values.length} exact wire/domain vectors passed');
}
