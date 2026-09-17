import 'dart:convert';
import 'dart:typed_data';
import 'package:crypto/crypto.dart';
import 'generated/language_pack.pb.dart' as pb;
import 'generated/language_pins.dart';

/// Bounded bundled PB/LZ4 data. Pins bind the exact ARB compilation consumed by
/// gen-l10n, including keys, ICU source and typed placeholder metadata.
/// This is integrity checking, not an external language-pack signature policy.
abstract final class LanguagePackCodec {
  static const maxRawBytes = 4 * 1024 * 1024;
  static const maxContainerBytes = maxRawBytes + maxRawBytes ~/ 255 + 128;
  static pb.LanguagePack validate(Uint8List container, String locale) {
    final pin = languagePins[locale];
    if (pin == null ||
        container.length < 50 ||
        container.length > maxContainerBytes) {
      throw const FormatException('Unsupported or oversized language pack');
    }
    final header = ByteData.sublistView(container);
    if (ascii.decode(container.sublist(0, 8), allowInvalid: true) !=
            'MROWLNG1' ||
        header.getUint16(8, Endian.little) != 1) {
      throw const FormatException('Unsupported language container');
    }
    final length = header.getUint32(10, Endian.little);
    final compressed = header.getUint32(14, Endian.little);
    if (length == 0 ||
        length > maxRawBytes ||
        compressed != container.length - 50) {
      throw const FormatException('Invalid language lengths');
    }
    final raw = _lz4(Uint8List.sublistView(container, 50), length);
    final actual = sha256.convert(raw);
    if (actual.toString() != pin.rawSha ||
        !_equal(actual.bytes, Uint8List.sublistView(container, 18, 50))) {
      throw const FormatException('Language digest mismatch');
    }
    // Decode only the bounded, exact pinned compiler output. An untrusted
    // protobuf body never reaches an allocating decoder before the pin check.
    final pack = pb.LanguagePack.fromBuffer(raw);
    if (pack.schemaVersion != 1 ||
        pack.locale != locale ||
        _hex(pack.catalogSha256) != pin.catalogSha ||
        pack.messages.length != pin.count ||
        pack.messages.length > 4096) {
      throw const FormatException('Language catalog mismatch');
    }
    var previous = '';
    for (final message in pack.messages) {
      if (message.key.compareTo(previous) <= 0 ||
          utf8.encode(message.icu).length > 65536) {
        throw const FormatException('Invalid language entry');
      }
      previous = message.key;
    }
    return pack;
  }

  static bool _equal(List<int> a, List<int> b) {
    if (a.length != b.length) return false;
    var difference = 0;
    for (var i = 0; i < a.length; i++) {
      difference |= a[i] ^ b[i];
    }
    return difference == 0;
  }

  static String _hex(List<int> bytes) =>
      bytes.map((b) => b.toRadixString(16).padLeft(2, '0')).join();
  static Uint8List _lz4(Uint8List input, int size) {
    final output = Uint8List(size);
    var source = 0;
    var target = 0;
    int length(int initial) {
      var result = initial;
      if (initial == 15) {
        int extra;
        do {
          if (source >= input.length) {
            throw const FormatException('Truncated LZ4 length');
          }
          extra = input[source++];
          result += extra;
          if (result > size) {
            throw const FormatException('Oversized LZ4 sequence');
          }
        } while (extra == 255);
      }
      return result;
    }

    while (source < input.length) {
      final token = input[source++];
      final literals = length(token >> 4);
      if (source + literals > input.length || target + literals > size) {
        throw const FormatException('Invalid LZ4 literals');
      }
      output.setRange(target, target + literals, input, source);
      source += literals;
      target += literals;
      if (source == input.length) break;
      if (source + 2 > input.length) {
        throw const FormatException('Truncated LZ4 offset');
      }
      final offset = input[source] | input[source + 1] << 8;
      source += 2;
      if (offset == 0 || offset > target) {
        throw const FormatException('Invalid LZ4 offset');
      }
      final count = length(token & 15) + 4;
      if (target + count > size) {
        throw const FormatException('Oversized LZ4 match');
      }
      for (var i = 0; i < count; i++) {
        output[target] = output[target - offset];
        target++;
      }
    }
    if (target != size) throw const FormatException('Incomplete LZ4 output');
    return output;
  }
}
