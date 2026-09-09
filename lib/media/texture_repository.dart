import 'package:file_selector/file_selector.dart';
import 'texture_source.dart';
import 'texture_storage_native.dart'
    if (dart.library.js_interop) 'texture_storage_web.dart'
    as platform;

class TextureRepository {
  static Future<TextureSource?> pick() async {
    final file = await openFile(
      acceptedTypeGroups: const [
        XTypeGroup(
          label: '图片、动图与视频',
          extensions: [
            'png',
            'jpg',
            'jpeg',
            'webp',
            'gif',
            'bmp',
            'mp4',
            'webm',
            'mov',
            'mkv',
            'm4v',
          ],
        ),
      ],
    );
    if (file == null) return null;
    return importFile(file);
  }

  static Future<TextureSource> importFile(XFile file) async {
    final kind = TextureSource.kindFor(file.name);
    final limit = kind == TextureKind.video || kind == TextureKind.audio
        ? 150
        : 25;
    if (await file.length() > limit * 1024 * 1024) {
      throw FormatException('文件过大，请选择 $limit MB 以内的素材。');
    }
    return platform.store(file, kind);
  }

  static Future<ResolvedTexture> resolve(TextureSource source) async {
    if (!source.local) return ResolvedTexture(uri: source.location);
    return platform.resolve(source);
  }
}
