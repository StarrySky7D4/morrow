import 'dart:typed_data' show Endian;
import 'package:flutter/services.dart';
import 'package:crypto/crypto.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_i18n/morrow_i18n.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  Future<Uint8List> bytes(String code) async {
    final data = await rootBundle.load(
      'packages/morrow_i18n/assets/languages/$code.mlang',
    );
    return data.buffer.asUint8List(data.offsetInBytes, data.lengthInBytes);
  }

  test(
    'bundled PB/LZ4 resources validate and typed intl plurals execute',
    () async {
      for (final code in ['en', 'zh']) {
        final pack = LanguagePackCodec.validate(await bytes(code), code);
        expect(pack.locale, code);
        expect(pack.messages.any((m) => m.key == 'commonCount'), true);
        final messages = await L10n.delegate.load(Locale(code));
        expect(messages.commonGreeting('Ada'), contains('Ada'));
      }
      final en = L10n.forLocale(const Locale('en'));
      expect(en.commonCount(0), 'No items');
      expect(en.commonCount(1), '1 item');
      expect(en.commonCount(2), '2 items');
      expect(L10n.forLocale(const Locale('zh')).commonCount(2), '2 项');
    },
  );
  test(
    'digest, locale, version, length and trailing bytes fail closed',
    () async {
      final good = await bytes('en');
      final corruptions = <Uint8List>[
        Uint8List.fromList(good)..[18] ^= 1,
        Uint8List.fromList(good)..[good.length - 1] ^= 1,
        Uint8List.fromList(good)..[8] = 2,
        Uint8List.fromList(good.sublist(0, good.length - 1)),
        Uint8List.fromList([...good, 0]),
        Uint8List.fromList(good)
          ..[10] = 0xff
          ..[11] = 0xff
          ..[12] = 0xff
          ..[13] = 0x7f,
      ];
      for (final bad in corruptions) {
        expect(
          () => LanguagePackCodec.validate(bad, 'en'),
          throwsFormatException,
        );
      }
      expect(
        () => LanguagePackCodec.validate(good, 'zh'),
        throwsFormatException,
      );
      expect(
        () => LanguagePackCodec.validate(good, 'fr'),
        throwsFormatException,
      );
    },
  );
  test(
    'missing message with recomputed container hash cannot replace compiled catalog',
    () async {
      final pack = LanguagePackCodec.validate(await bytes('en'), 'en');
      pack.messages.removeLast();
      final raw = Uint8List.fromList(pack.writeToBuffer());
      final compressed = <int>[0xf0];
      var remaining = raw.length - 15;
      while (remaining >= 255) {
        compressed.add(255);
        remaining -= 255;
      }
      compressed.add(remaining);
      compressed.addAll(raw);
      final header = ByteData(50);
      header.buffer.asUint8List().setRange(0, 8, 'MROWLNG1'.codeUnits);
      header.setUint16(8, 1, Endian.little);
      header.setUint32(10, raw.length, Endian.little);
      header.setUint32(14, compressed.length, Endian.little);
      header.buffer.asUint8List().setRange(18, 50, sha256.convert(raw).bytes);
      final replacement = Uint8List.fromList([
        ...header.buffer.asUint8List(),
        ...compressed,
      ]);
      expect(
        () => LanguagePackCodec.validate(replacement, 'en'),
        throwsFormatException,
      );
    },
  );
  testWidgets('missing delegates fallback to Chinese without assertion', (
    tester,
  ) async {
    late AppLocalizations messages;
    await tester.pumpWidget(
      Builder(
        builder: (context) {
          messages = L10n.of(context);
          return const SizedBox();
        },
      ),
    );
    expect(messages.commonCancel, '取消');
    expect(L10n.forLocale(const Locale('en')).commonCancel, 'Cancel');
    expect(L10n.forLocale(const Locale('fr')).commonCancel, '取消');
  });
}
