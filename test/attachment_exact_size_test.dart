import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/attachments/attachment.dart';
import 'package:morrow_studio/media/texture_source.dart';

const _source = TextureSource(
  location: 'cache/asset.bin',
  name: 'asset.bin',
  kind: TextureKind.file,
  local: true,
);

void main() {
  test('legacy const attachment keeps its JSON shape and byte length', () {
    const legacy = IdeaAttachment(source: _source, size: 1536, pluginId: 'a');
    expect(legacy.byteLength, BigInt.from(1536));
    expect(legacy.size, 1536);
    expect(legacy.sizeLabel, '2 KB');
    final json = legacy.toJson();
    expect(json['size'], 1536);
    expect(json.containsKey('exactSize'), isFalse);
    final restored = IdeaAttachment.fromJson(json);
    expect(restored.byteLength, legacy.byteLength);
    expect(restored.toJson(), json);
  });

  test(
    'versioned u64 maximum survives JSON and label without int narrowing',
    () {
      final max = (BigInt.one << 64) - BigInt.one;
      final asset = IdeaAttachment.versioned(
        source: _source,
        byteLength: max,
        pluginId: 'asset-v2',
      );
      expect(asset.byteLength, max);
      expect(() => asset.size, throwsRangeError);
      expect(asset.sizeLabel, matches(RegExp(r'^\d+\.\d MB$')));
      final json = asset.toJson();
      expect(json['exactSize'], '18446744073709551615');
      expect(json.containsKey('size'), isFalse);
      final restored = IdeaAttachment.fromJson(json);
      expect(restored.byteLength, max);
      expect(restored.toJson(), json);
      expect(
        () => IdeaAttachment.versioned(
          source: _source,
          byteLength: BigInt.one << 64,
          pluginId: 'bad',
        ),
        throwsFormatException,
      );
    },
  );

  test('2^53 boundary permits only exact compatibility int values', () {
    final safe = (BigInt.one << 53) - BigInt.one;
    final atBoundary = IdeaAttachment.versioned(
      source: _source,
      byteLength: safe,
      pluginId: 'safe',
    );
    expect(atBoundary.size, safe.toInt());
    expect(atBoundary.toJson()['size'], safe.toInt());
    expect(IdeaAttachment.fromJson(atBoundary.toJson()).byteLength, safe);

    final above = IdeaAttachment.versioned(
      source: _source,
      byteLength: safe + BigInt.one,
      pluginId: 'above',
    );
    expect(() => above.size, throwsRangeError);
    expect(above.toJson().containsKey('size'), isFalse);
    expect(
      IdeaAttachment.fromJson(above.toJson()).byteLength,
      safe + BigInt.one,
    );
    expect(
      () => IdeaAttachment.fromJson({...above.toJson(), 'size': 1}),
      throwsFormatException,
    );
  });
}
