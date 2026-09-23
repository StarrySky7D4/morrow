import 'package:file_selector/file_selector.dart';
import '../media/texture_source.dart';
import '../media/texture_storage_native.dart'
    if (dart.library.js_interop) '../media/texture_storage_web.dart'
    as storage;

class IdeaAttachment {
  const IdeaAttachment({required this.source, required int size, this.pluginId})
    : _legacySize = size,
      _exactSize = null;

  IdeaAttachment.versioned({
    required this.source,
    required BigInt byteLength,
    required String pluginId,
  }) : _legacySize = null,
       _exactSize = _checkedExactSize(byteLength),
       pluginId = pluginId {
    if (pluginId.isEmpty) {
      throw const FormatException('Missing versioned asset ID');
    }
  }

  final TextureSource source;
  final int? _legacySize;
  final BigInt? _exactSize;
  final String? pluginId;
  static const maxSize = 200 * 1024 * 1024;
  static final BigInt _maxSafeInt = (BigInt.one << 53) - BigInt.one;
  static final BigInt _maxU64 = (BigInt.one << 64) - BigInt.one;

  static BigInt _checkedExactSize(BigInt value) {
    if (value < BigInt.zero || value > _maxU64) {
      throw const FormatException('Asset byte length is outside u64');
    }
    return value;
  }

  BigInt get byteLength => _exactSize ?? BigInt.from(_legacySize!);

  /// A compatibility view for old int-only consumers. It never narrows an
  /// exact u64 value that could lose precision on a web runtime.
  int get size {
    final bytes = byteLength;
    if (bytes < BigInt.zero || bytes > _maxSafeInt) {
      throw RangeError('Asset byte length is not a safe int');
    }
    return bytes.toInt();
  }

  String get extension => source.name.toLowerCase().split('.').last;
  bool get previewable => source.kind != TextureKind.file;
  String get sizeLabel {
    final bytes = byteLength;
    final mib = BigInt.from(1024 * 1024);
    if (bytes >= mib) {
      final tenths = (bytes * BigInt.from(10) + (mib ~/ BigInt.two)) ~/ mib;
      return '${tenths ~/ BigInt.from(10)}.${tenths.remainder(BigInt.from(10))} MB';
    }
    return '${(bytes + BigInt.from(1023)) ~/ BigInt.from(1024)} KB';
  }

  Map<String, dynamic> toJson() => {
    'source': source.toJson(),
    if (_exactSize case final exact?) ...{
      if (exact <= _maxSafeInt) 'size': exact.toInt(),
      'exactSize': exact.toString(),
    } else
      'size': size,
    if (pluginId != null) 'pluginId': pluginId,
  };
  factory IdeaAttachment.fromJson(Map<String, dynamic> data) {
    final source = TextureSource.fromJson(
      data['source'] as Map<String, dynamic>,
    );
    if (data['exactSize'] case final String exact) {
      if (!RegExp(r'^(0|[1-9][0-9]*)$').hasMatch(exact)) {
        throw const FormatException('Invalid exact asset length');
      }
      final length = _checkedExactSize(BigInt.parse(exact));
      final legacy = data['size'];
      if (legacy != null &&
          (legacy is! int ||
              length > _maxSafeInt ||
              BigInt.from(legacy) != length)) {
        throw const FormatException('Conflicting asset length fields');
      }
      final id = data['pluginId'];
      if (id is! String) {
        throw const FormatException('Missing versioned asset ID');
      }
      return IdeaAttachment.versioned(
        source: source,
        byteLength: length,
        pluginId: id,
      );
    }
    if (data.containsKey('exactSize')) {
      throw const FormatException('Invalid exact asset length');
    }
    return IdeaAttachment(
      source: source,
      size: data['size'] as int? ?? 0,
      pluginId: data['pluginId'] as String?,
    );
  }
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
