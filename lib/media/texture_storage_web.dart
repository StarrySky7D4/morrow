import 'dart:js_interop';
import 'dart:typed_data';
import 'package:file_selector/file_selector.dart';
import 'package:idb_shim/idb_browser.dart';
import 'package:web/web.dart' as web;
import 'texture_source.dart';

// Immutable, session-owned previews exported from the authoritative workbench.
// These never enter the legacy media database or survive as stale file paths.
final _previews = <String, web.Blob>{};
bool hasPreview(TextureSource source) => _previews.containsKey(source.location);
TextureSource retainPreview(web.Blob blob, String name, TextureKind kind) {
  final key = 'morrow-preview:${web.window.crypto.randomUUID()}';
  _previews[key] = blob;
  return TextureSource(location: key, name: name, kind: kind, local: true);
}

void releasePreview(TextureSource source) => _previews.remove(source.location);

Future<Database> _open() => idbFactoryWeb.open(
  // Keep the existing browser database so saved media remains available.
  'daemon-media',
  version: 1,
  onUpgradeNeeded: (event) {
    event.database.createObjectStore('textures');
  },
);

Future<TextureSource> store(XFile file, TextureKind kind) async {
  final data = await file.readAsBytes();
  final database = await _open();
  final key = 'media-${web.window.crypto.randomUUID()}';
  try {
    final transaction = database.transaction('textures', idbModeReadWrite);
    await transaction.objectStore('textures').add(data, key);
    await transaction.completed;
  } finally {
    database.close();
  }
  return TextureSource(location: key, name: file.name, kind: kind, local: true);
}

Future<web.Blob> resolveBlob(TextureSource source) async {
  if (!source.local) {
    throw const FormatException('Expected a selected local file');
  }
  if (source.location.startsWith('morrow-preview:')) {
    return _previews[source.location] ??
        (throw const FormatException('Attachment preview session has closed'));
  }
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
  return web.Blob([data.toJS].toJS);
}

Future<ResolvedTexture> resolve(TextureSource source) async {
  final blob = await resolveBlob(source);
  if (source.kind != TextureKind.video && source.kind != TextureKind.audio) {
    return ResolvedTexture(
      uri: '',
      bytes: Uint8List.view((await blob.arrayBuffer().toDart).toDart),
    );
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
          'wma' => 'audio/x-ms-wma',
          'aif' || 'aiff' => 'audio/aiff',
          _ => 'application/octet-stream',
        }
      : extension == 'webm'
      ? 'video/webm'
      : 'video/mp4';
  final url = web.URL.createObjectURL(blob.slice(0, blob.size, mime));
  return ResolvedTexture(uri: url, release: () => web.URL.revokeObjectURL(url));
}

Future<void> remove(TextureSource source) async {
  if (source.location.startsWith('morrow-preview:')) {
    releasePreview(source);
    return;
  }
  final database = await _open();
  try {
    final transaction = database.transaction('textures', idbModeReadWrite);
    await transaction.objectStore('textures').delete(source.location);
    await transaction.completed;
  } finally {
    database.close();
  }
}
