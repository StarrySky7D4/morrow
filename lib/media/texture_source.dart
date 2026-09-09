import 'dart:typed_data';

enum TextureKind { image, gif, video, audio }

class TextureSource {
  const TextureSource({
    required this.location,
    required this.name,
    required this.kind,
    this.local = false,
  });
  final String location, name;
  final TextureKind kind;
  final bool local;
  Map<String, dynamic> toJson() => {
    'location': location,
    'name': name,
    'kind': kind.name,
    'local': local,
  };
  factory TextureSource.fromJson(Map<String, dynamic> data) => TextureSource(
    location: data['location'] as String,
    name: data['name'] as String,
    kind: TextureKind.values.byName(data['kind'] as String),
    local: data['local'] as bool,
  );
  static TextureKind kindFor(String name) {
    final extension = name.toLowerCase().split('.').last;
    if (extension == 'gif') return TextureKind.gif;
    if ([
      'mp3',
      'wav',
      'flac',
      'm4a',
      'aac',
      'ogg',
      'opus',
    ].contains(extension)) {
      return TextureKind.audio;
    }
    if (['mp4', 'webm', 'mov', 'mkv', 'm4v'].contains(extension)) {
      return TextureKind.video;
    }
    return TextureKind.image;
  }

  static bool validUrl(String value) {
    final uri = Uri.tryParse(value.trim());
    return uri != null &&
        ['https', 'http'].contains(uri.scheme) &&
        uri.host.isNotEmpty &&
        uri.userInfo.isEmpty;
  }
}

class ResolvedTexture {
  ResolvedTexture({required this.uri, this.bytes, this.release});
  final String uri;
  final Uint8List? bytes;
  final void Function()? release;
}
