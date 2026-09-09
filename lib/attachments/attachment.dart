import 'package:file_selector/file_selector.dart';
import '../media/texture_source.dart';
import '../media/texture_storage_native.dart'
    if (dart.library.js_interop) '../media/texture_storage_web.dart'
    as storage;

class IdeaAttachment {
  const IdeaAttachment({required this.source, required this.size});
  final TextureSource source;
  final int size;
  static const maxSize = 200 * 1024 * 1024;
  String get extension => source.name.toLowerCase().split('.').last;
  bool get previewable => source.kind != TextureKind.file;
  String get sizeLabel => size >= 1024 * 1024
      ? '${(size / 1024 / 1024).toStringAsFixed(1)} MB'
      : '${(size / 1024).ceil()} KB';
  Map<String, dynamic> toJson() => {'source': source.toJson(), 'size': size};
  factory IdeaAttachment.fromJson(Map<String, dynamic> data) => IdeaAttachment(
    source: TextureSource.fromJson(data['source'] as Map<String, dynamic>),
    size: data['size'] as int? ?? 0,
  );
  static TextureKind kindFor(String name) {
    final ext = name.toLowerCase().split('.').last;
    if (['png', 'jpg', 'jpeg', 'webp', 'bmp', 'gif'].contains(ext)) {
      return TextureSource.kindFor(name);
    }
    final kind = TextureSource.kindFor(name);
    return kind == TextureKind.image ? TextureKind.file : kind;
  }

  static Future<IdeaAttachment> import(XFile file) async {
    final size = await file.length();
    if (size > maxSize) {
      throw FormatException('${file.name} 超过 200 MB，请选择较小的文件。');
    }
    final source = await storage.store(file, kindFor(file.name));
    return IdeaAttachment(source: source, size: size);
  }
}
