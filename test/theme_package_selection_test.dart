import 'dart:typed_data';
import 'package:file_selector/file_selector.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/theme_package_import.dart';

void main() {
  test(
    'selection is bounded and immutable across inspection and confirmation',
    () async {
      final bytes = Uint8List.fromList([1, 2, 3]);
      final selected = await ThemePackageSelection.read(
        XFile.fromData(bytes, name: 'theme.morrowplugin', path: 'theme.morrowplugin'),
      );
      bytes[0] = 9;
      expect(selected.bytes, [1, 2, 3]);
      expect(() => selected.bytes[0] = 4, throwsUnsupportedError);
      expect(selected.name, 'theme.morrowplugin');
      for (final size in [0, maxThemePackageBytes + 1]) {
        await expectLater(
          ThemePackageSelection.read(XFile.fromData(Uint8List(size))),
          throwsFormatException,
        );
      }
    },
  );
}
