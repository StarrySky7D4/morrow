@TestOn('browser')
library;

import 'dart:typed_data';
import 'package:file_selector/file_selector.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/media/texture_source.dart';
import 'package:morrow_studio/media/texture_storage_web.dart' as media;
import 'package:morrow_studio/plugins/studio_storage.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';
import 'package:morrow_studio/plugins/generated/studio.capnp.dart' as wire;

void main() {
  test(
    'legacy keys remain readable and transient URLs cannot poison saved preferences',
    () {
      final source = <String, dynamic>{
        'location': '1720000000000000',
        'name': 'old.png',
        'kind': 'image',
        'local': true,
      };
      expect(
        decodePreferences(encodePreferences({'texture': source}))['texture'],
        source,
      );
      for (final key in [
        'blob:https://example.com/id',
        'morrow-preview:id',
        '../file',
        '123?x=1',
      ]) {
        expect(
          () => encodePreferences({
            'texture': {...source, 'location': key},
          }),
          throwsFormatException,
        );
      }
    },
  );
  test(
    'selected browser media survives preferences encoding and later edits',
    () async {
      final sources = <TextureSource>[];
      try {
        for (final kind in [
          TextureKind.image,
          TextureKind.video,
          TextureKind.audio,
        ]) {
          sources.add(
            await media.store(
              XFile.fromData(
                Uint8List.fromList([kind.index, 7, 255]),
                name: '${kind.name}.bin',
              ),
              kind,
            ),
          );
        }
        final input = <String, dynamic>{
          'texture': sources[0].toJson(),
          'music': {
            'tracks': [
              {'source': sources[2].toJson(), 'cover': sources[0].toJson()},
            ],
          },
        };
        final encoded = encodePreferences(input);
        final request = RustWorkbench.readMessage(
          encoded,
        ).getRoot(wire.preferencesFactory);
        expect(
          Uri.parse(request.texture!.location!).isAbsolute,
          isTrue,
          reason:
              'The Rust preferences contract requires an absolute local media URI',
        );
        expect(Uri.parse(request.texture!.location!).scheme, 'file');
        final saved = decodePreferences(encoded);
        expect(saved['texture'], sources[0].toJson());
        final changed = decodePreferences(
          encodePreferences({
            ...saved,
            'theme': 'dark',
            'texture': sources[1].toJson(),
          }),
        );
        expect(changed['theme'], 'dark');
        expect(changed['texture'], sources[1].toJson());
        final tracks = (changed['music'] as Map)['tracks'] as List;
        expect(tracks.single['source'], sources[2].toJson());
        expect(tracks.single['cover'], sources[0].toJson());
        for (final source in sources) {
          expect((await media.resolveBlob(source)).size, 3);
        }
      } finally {
        for (final source in sources) {
          await media.remove(source);
        }
      }
    },
  );
}
