@TestOn('vm')
library;

import 'dart:io';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/studio_storage.dart';

void main() {
  test(
    'native absolute media paths and remote sources retain their representation',
    () {
      final source = <String, dynamic>{
        'location':
            '${Directory.systemTemp.path}${Platform.pathSeparator}图片 #1.png',
        'name': '图片 #1.png',
        'kind': 'image',
        'local': true,
      };
      final track = {
        ...source,
        'location': 'https://example.com/music.mp3',
        'kind': 'audio',
        'local': false,
      };
      final saved = decodePreferences(
        encodePreferences({
          'texture': source,
          'music': {
            'tracks': [
              {'source': track, 'cover': source},
            ],
          },
        }),
      );
      expect(saved['texture'], source);
      expect((saved['music'] as Map)['tracks'].single['source'], track);
      expect((saved['music'] as Map)['tracks'].single['cover'], source);
    },
  );
}
