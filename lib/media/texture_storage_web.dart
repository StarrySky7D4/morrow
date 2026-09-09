import 'dart:js_interop';
import 'dart:typed_data';
import 'package:file_selector/file_selector.dart';
import 'package:idb_shim/idb_browser.dart';
import 'package:web/web.dart' as web;
import 'texture_source.dart';

Future<Database> _open() => idbFactoryWeb.open(
  'daemon-media',
  version: 1,
  onUpgradeNeeded: (event) {
    event.database.createObjectStore('textures');
  },
);

Future<TextureSource> store(XFile file, TextureKind kind) async {
  final data = await file.readAsBytes();
  final database = await _open();
  final key = '${DateTime.now().microsecondsSinceEpoch}';
  try {
    final transaction = database.transaction('textures', idbModeReadWrite);
    await transaction.objectStore('textures').put(data, key);
    await transaction.completed;
  } finally {
    database.close();
  }
  return TextureSource(location: key, name: file.name, kind: kind, local: true);
}

Future<ResolvedTexture> resolve(TextureSource source) async {
  final database = await _open();
  Uint8List data;
  try {
    final transaction = database.transaction('textures', idbModeReadOnly);
    final raw = await transaction
        .objectStore('textures')
        .getObject(source.location);
    await transaction.completed;
    if (raw == null) throw const FormatException('浏览器中找不到素材，请重新导入。');
    data = raw as Uint8List;
  } finally {
    database.close();
  }
  if (source.kind != TextureKind.video && source.kind != TextureKind.audio) {
    return ResolvedTexture(uri: '', bytes: data);
  }
  final extension = source.name.toLowerCase().split('.').last;
  final mime = source.kind == TextureKind.audio
      ? switch (extension) {
          'mp3' => 'audio/mpeg',
          'wav' => 'audio/wav',
          'flac' => 'audio/flac',
          'm4a' => 'audio/mp4',
          'aac' => 'audio/aac',
          'ogg' || 'opus' => 'audio/ogg',
          _ => 'application/octet-stream',
        }
      : extension == 'webm'
      ? 'video/webm'
      : 'video/mp4';
  final url = web.URL.createObjectURL(
    web.Blob([data.toJS].toJS, web.BlobPropertyBag(type: mime)),
  );
  return ResolvedTexture(uri: url, release: () => web.URL.revokeObjectURL(url));
}

Future<void> remove(TextureSource source) async {
  final database = await _open();
  try {
    final transaction = database.transaction('textures', idbModeReadWrite);
    await transaction.objectStore('textures').delete(source.location);
    await transaction.completed;
  } finally {
    database.close();
  }
}
