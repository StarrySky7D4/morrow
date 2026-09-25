@TestOn('browser')
library;

import 'dart:js_interop';
import 'dart:typed_data';
import 'package:file_selector/file_selector.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:web/web.dart' as web;
import 'package:morrow_studio/media/texture_source.dart';
import 'package:morrow_studio/media/texture_storage_web.dart' as storage;

void main() {
  test('simultaneous browser selections cannot replace one another', () async {
    final sources = await Future.wait(
      List.generate(
        12,
        (i) => storage.store(
          XFile.fromData(
            Uint8List.fromList([i, 255 - i]),
            name: 'same-name.bin',
          ),
          TextureKind.file,
        ),
      ),
    );
    try {
      expect(sources.map((v) => v.location).toSet(), hasLength(12));
      for (var i = 0; i < sources.length; i++) {
        expect((await storage.resolve(sources[i])).bytes, [i, 255 - i]);
      }
    } finally {
      await Future.wait(sources.map(storage.remove));
    }
  });

  test(
    'immutable previews own their bytes and expire without touching selected media',
    () async {
      final selected = await storage.store(
        XFile.fromData(Uint8List.fromList([1, 2, 3]), name: 'original.bin'),
        TextureKind.file,
      );
      final original = Uint8List.fromList([4, 5, 6]);
      final blob = web.Blob([original.toJS].toJS);
      final preview = storage.retainPreview(
        blob,
        'original.bin',
        TextureKind.file,
      );
      original[0] = 9;
      try {
        expect((await storage.resolve(preview)).bytes, [4, 5, 6]);
        storage.releasePreview(preview);
        expect(storage.hasPreview(preview), isFalse);
        await expectLater(storage.resolve(preview), throwsFormatException);
        expect((await storage.resolve(selected)).bytes, [1, 2, 3]);
      } finally {
        storage.releasePreview(preview);
        await storage.remove(selected);
      }
    },
  );
}
