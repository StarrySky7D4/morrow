import 'dart:io';
import 'package:file_selector/file_selector.dart';
import 'package:path_provider/path_provider.dart';
import 'texture_source.dart';

Future<TextureSource> store(XFile file, TextureKind kind) async {
  final folder = Directory(
    '${(await getApplicationSupportDirectory()).path}/textures',
  );
  await folder.create(recursive: true);
  final extension = file.name
      .toLowerCase()
      .split('.')
      .last
      .replaceAll(RegExp('[^a-z0-9]'), '');
  final target =
      '${folder.path}/${DateTime.now().microsecondsSinceEpoch}.$extension';
  await file.saveTo(target);
  return TextureSource(
    location: target,
    name: file.name,
    kind: kind,
    local: true,
  );
}

Future<ResolvedTexture> resolve(TextureSource source) async {
  final file = File(source.location);
  if (!await file.exists()) throw const FormatException('已保存的素材不存在，请重新导入。');
  return ResolvedTexture(
    uri: file.uri.toString(),
    bytes: source.kind == TextureKind.video || source.kind == TextureKind.audio
        ? null
        : await file.readAsBytes(),
  );
}
