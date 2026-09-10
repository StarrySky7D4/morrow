import 'dart:convert';
import 'dart:typed_data';
import 'package:crypto/crypto.dart' as hash;
import 'package:cryptography/cryptography.dart';

const maxKugouDatabaseBytes = 64 * 1024 * 1024;
final _signature = ascii.encode('SQLite format 3\u0000');
Uint8List kugouPageKey(int page) {
  final value = ByteData(4)..setUint32(0, page, Endian.little);
  return Uint8List.fromList(
    hash.md5.convert([
      0x1d,
      0x61,
      0x31,
      0x45,
      0xb2,
      0x47,
      0xbf,
      0x7f,
      0x3d,
      0x18,
      0x96,
      0x72,
      0x14,
      0x4f,
      0xe4,
      0xbf,
      ...value.buffer.asUint8List(),
      ...ascii.encode('sAlT'),
    ]).bytes,
  );
}

Uint8List kugouPageIv(int page) {
  var seed = page + 1;
  final data = ByteData(16);
  for (var i = 0; i < 4; i++) {
    final value = (seed * 0x9ef4 - (seed ~/ 0xce26) * 0x7fffff07) & 0xffffffff;
    seed = (value & 0x80000000) != 0
        ? (value + 0x7fffff07) & 0xffffffff
        : value;
    data.setUint32(i * 4, seed, Endian.little);
  }
  return Uint8List.fromList(hash.md5.convert(data.buffer.asUint8List()).bytes);
}

Future<Uint8List> decodeKugouDatabase(Uint8List encrypted) async {
  if (encrypted.length < 1024 || encrypted.length > maxKugouDatabaseBytes) {
    throw const FormatException('酷狗数据库大小不在支持范围内。');
  }
  final result = Uint8List.fromList(encrypted);
  try {
    if (!_equal(result.sublist(0, 16), _signature)) {
      if (result.length % 1024 != 0) throw const FormatException('酷狗数据库不完整。');
      final expected = Uint8List.fromList(result.sublist(16, 24));
      result.setRange(16, 24, result.sublist(8, 16));
      final cipher = AesCbc.with128bits(
        macAlgorithm: MacAlgorithm.empty,
        paddingAlgorithm: PaddingAlgorithm.zero,
      );
      for (var start = 0; start < result.length; start += 1024) {
        final page = start ~/ 1024 + 1;
        final first = start + (page == 1 ? 16 : 0);
        final key = kugouPageKey(page);
        final plain = await cipher.decrypt(
          SecretBox(
            Uint8List.sublistView(result, first, start + 1024),
            nonce: kugouPageIv(page),
            mac: Mac.empty,
          ),
          secretKey: SecretKey(key),
        );
        result.setRange(first, start + 1024, plain);
        key.fillRange(0, key.length, 0);
        plain.fillRange(0, plain.length, 0);
      }
      if (!_equal(result.sublist(16, 24), expected)) {
        throw const FormatException('酷狗数据库格式暂不支持。');
      }
      result.setRange(0, 16, _signature);
    }
    // The snapshot contains the last checkpoint; never create a WAL or journal.
    result[18] = 1;
    result[19] = 1;
    return result;
  } catch (_) {
    result.fillRange(0, result.length, 0);
    rethrow;
  }
}

bool _equal(List<int> a, List<int> b) {
  if (a.length != b.length) return false;
  for (var i = 0; i < a.length; i++) {
    if (a[i] != b[i]) return false;
  }
  return true;
}
