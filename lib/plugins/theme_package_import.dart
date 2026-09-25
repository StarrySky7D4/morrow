import 'dart:typed_data';
import 'package:file_selector/file_selector.dart';
import 'plugin_library.dart';

// Mirrors core::plugin_package::MAX_PACKAGE_BYTES (container + LZ4 overhead).
const maxThemePackageBytes = 4276930;

/// One bounded immutable selection is used for preview and confirmation.
final class ThemePackageSelection {
  ThemePackageSelection._(this.name, Uint8List bytes)
    : bytes = Uint8List.fromList(bytes).asUnmodifiableView();
  final String name;
  final Uint8List bytes;
  static Future<ThemePackageSelection> read(XFile file) async {
    final length = await file.length();
    if (length < 1 || length > maxThemePackageBytes) {
      throw const FormatException(
        'Theme package size exceeds the supported limit',
      );
    }
    final bytes = await file.readAsBytes();
    if (bytes.length != length) {
      throw const FormatException(
        'Selected theme file changed; select it again',
      );
    }
    return ThemePackageSelection._(file.name, bytes);
  }
}

abstract interface class ThemePackageImportControl {
  bool get supportsThemePackageImport;
  Future<PluginLibraryPage> inspectThemePackage(ThemePackageSelection selected);
  Future<void> importThemePackage(
    ThemePackageSelection selected,
    Uint8List digest,
    BigInt revision,
  );
}

/// Device-local side channel; the normal protocol carries only metadata.
abstract interface class WorkbenchDeviceThemes {
  Future<void> sendThemePackage(
    Uint8List frame,
    ThemePackageSelection selected,
  );
}
