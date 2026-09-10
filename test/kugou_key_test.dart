import 'dart:io';
import 'dart:typed_data';
import 'package:crypto/crypto.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/music/kugou_database.dart';
import 'package:morrow_studio/music/kugou_key_native.dart';

void main() {
  final plainFile = File('test/fixtures/audio/kugou_synthetic_plain.bin');
  final encryptedFile = File(
    'test/fixtures/audio/kugou_synthetic_encrypted.bin',
  );
  test('Page derivation matches independent reference vectors', () {
    String hex(List<int> bytes) =>
        bytes.map((v) => v.toRadixString(16).padLeft(2, '0')).join();
    expect(hex(kugouPageKey(0)), '1962c05fa2ebbe2428ff522b9e03ead4');
    expect(hex(kugouPageIv(0)), '055a673593892ddf3ab3b3c621c34802');
  });
  test(
    'Encrypted snapshot decodes to independent SQLite fixture without changing input',
    () async {
      final encrypted = await encryptedFile.readAsBytes();
      final before = sha256.convert(encrypted);
      final clear = await decodeKugouDatabase(encrypted);
      expect(clear, await plainFile.readAsBytes());
      expect(sha256.convert(encrypted), before);
    },
  );
  test('Plain WAL header is adjusted only on the in-memory copy', () async {
    final input = await plainFile.readAsBytes();
    input[18] = 2;
    input[19] = 2;
    final clear = await decodeKugouDatabase(input);
    expect(clear, await plainFile.readAsBytes());
    expect(input[18], 2);
    expect(input[19], 2);
  });
  test('Invalid, incomplete and oversized snapshots are rejected', () async {
    for (final bytes in [
      Uint8List(10),
      Uint8List(1025),
      Uint8List(1024),
      Uint8List(maxKugouDatabaseBytes + 1),
    ]) {
      await expectLater(decodeKugouDatabase(bytes), throwsFormatException);
    }
  });
  test(
    'Read-only key lookup uses an exact case-insensitive unique match',
    () async {
      final clear = await plainFile.readAsBytes();
      expect(queryKugouKey(clear, 'SYNTHETIC-MATCH'), 'ZmFrZS1rZXk=');
      for (final id in [
        'missing',
        'conflict',
        'empty',
        'invalid',
        'unicode',
        "' OR 1=1 --",
        'Synthetic',
        '\u0000',
        'x' * 16385,
      ]) {
        expect(
          queryKugouKey(clear, id),
          isNull,
          reason: id.length < 100 ? id : 'oversized',
        );
      }
      expect(queryKugouKey(Uint8List(1024), 'Synthetic-Match'), isNull);
      expect(clear, await plainFile.readAsBytes());
    },
    skip: !Platform.isWindows,
  );
  test(
    'Lookup reads the requested snapshot and leaves the source intact',
    () async {
      final before = await encryptedFile.readAsBytes();
      expect(
        await lookupKugouKey(
          'synthetic-match',
          databasePath: encryptedFile.absolute.path,
        ),
        'ZmFrZS1rZXk=',
      );
      expect(await encryptedFile.readAsBytes(), before);
      expect(
        await lookupKugouKey(
          'synthetic-match',
          databasePath: 'test/fixtures/audio/absent.db',
        ),
        isNull,
      );
    },
    skip: !Platform.isWindows,
  );
}
