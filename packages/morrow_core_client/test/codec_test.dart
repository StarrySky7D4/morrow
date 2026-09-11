import 'dart:io';
import 'dart:typed_data';
import 'package:test/test.dart';
import 'package:morrow_core_client/morrow_core_client.dart';
import 'package:morrow_core_client/src/generated/contract_identity.dart';

RenameCommand command(BigInt revision) => RenameCommand(
  operationId: 'op',
  cardId: 'card',
  expectedRevision: revision,
  title: '文本 🪷',
);
void main() {
  test('full unsigned revision range round trips', () {
    for (final value in [
      BigInt.one,
      (BigInt.one << 53) + BigInt.one,
      (BigInt.one << 64) - BigInt.one,
    ]) {
      final decoded = RenameCommand.decode(command(value).encode());
      expect(decoded.expectedRevision, value);
      expect(decoded.title, '文本 🪷');
    }
  });
  test('all truncations and trailing bytes are rejected', () {
    final bytes = command(BigInt.one).encode();
    for (var i = 0; i < bytes.length; i++) {
      expect(
        () => RenameCommand.decode(Uint8List.sublistView(bytes, 0, i)),
        throwsA(anything),
      );
    }
    expect(
      () => RenameCommand.decode(Uint8List.fromList([...bytes, 0])),
      throwsFormatException,
    );
  });
  test('schema mismatch is rejected', () {
    final bytes = command(BigInt.one).encode();
    var index = -1;
    for (var i = 0; i <= bytes.length - 32; i++) {
      if (List.generate(32, (j) => bytes[i + j]).toString() ==
          runtimeDigest.toString()) {
        index = i;
        break;
      }
    }
    expect(index, greaterThanOrEqualTo(0));
    bytes[index] ^= 1;
    expect(() => RenameCommand.decode(bytes), throwsFormatException);
  });
  test('invalid identity revision and overlong title are rejected', () {
    for (final value in [BigInt.zero, BigInt.one << 64, -BigInt.one]) {
      expect(() => command(value), throwsFormatException);
    }
    expect(
      () => RenameCommand(
        operationId: 'op',
        cardId: 'C:/file',
        expectedRevision: BigInt.one,
        title: '',
      ),
      throwsFormatException,
    );
    expect(
      () => RenameCommand(
        operationId: 'op',
        cardId: 'card',
        expectedRevision: BigInt.one,
        title: 'a' * 16385,
      ),
      throwsFormatException,
    );
  });
  test('Rust generated vectors and multi-segment data decode in Dart', () {
    final root = Directory('../../build/core-test.10/vectors');
    for (final pair in [
      ('one', BigInt.one),
      ('above-js', (BigInt.one << 53) + BigInt.one),
      ('max', (BigInt.one << 64) - BigInt.one),
      ('multisegment', (BigInt.one << 64) - BigInt.one),
    ]) {
      final decoded = RenameCommand.decode(
        File('${root.path}/${pair.$1}.capnp').readAsBytesSync(),
      );
      expect(decoded.expectedRevision, pair.$2);
      expect(decoded.title, '消息 🪷');
    }
  });
}
