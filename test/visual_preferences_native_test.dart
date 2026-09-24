import 'dart:io';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/appearance.dart';
import 'package:morrow_studio/plugins/studio_storage.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';

void main() {
  final exe = Platform.environment['MORROW_WORKBENCH_HOST'];
  final package = Platform.environment['MORROW_WORKBENCH_PACKAGE'];
  for (final style in VisualStyle.values) {
    test(
      '${style.name}: styles and branched follow chains survive host restart; cycles preserve committed settings',
      () async {
        final directory = await Directory.systemTemp.createTemp(
          'morrow-visual-prefs-',
        );
        RustWorkbench? host;
        try {
          host = await RustWorkbench.open(
            executable: exe!,
            package: package!,
            directory: directory,
          );
          final settings = SurfaceSettings(
            visualStyle: style,
            styleDepth: style.index / 3,
            components: {
              'root': ComponentMaterial(
                enabled: true,
                blur: 4,
                opacity: .15,
                mode: GlassMode.liquid,
                styleDepth: 2 - style.index / 3,
              ),
              'child': ComponentMaterial(followComponent: 'root'),
              'sibling': ComponentMaterial(followComponent: 'root'),
              'leaf': ComponentMaterial(followComponent: 'child'),
            },
          );
          final input = {
            'theme': 'dark',
            'glass': 'clear',
            'background': 'transparent',
            ...settings.toJson(),
          };
          await host.savePreferences(encodePreferences(input));
          await host.close();
          host = null;
          host = await RustWorkbench.open(
            executable: exe,
            package: package,
            directory: directory,
          );
          final saved = decodePreferences((await host.readPreferences())!);
          final restored = SurfaceSettings.fromJson(saved);
          expect(restored.visualStyle, style);
          expect(restored.styleDepth, settings.styleDepth);
          expect(restored.depthFor('leaf'), 2 - style.index / 3);
          expect(restored.components['child']!.styleDepth, isNull);
          expect(restored.resolveComponent('leaf')!.blur, 4);
          expect(restored.resolveComponent('sibling')!.opacity, .15);
          for (final invalid in [-.1, 2.1, double.nan, double.infinity]) {
            await expectLater(
              host.savePreferences(
                encodePreferences({...input, 'styleDepth': invalid}),
              ),
              throwsA(anything),
            );
          }
          expect(
            SurfaceSettings.fromJson(
              decodePreferences((await host.readPreferences())!),
            ).styleDepth,
            settings.styleDepth,
          );
          final cycle = {
            ...input,
            'componentMaterials': <String, dynamic>{
              ...settings.toJson()['componentMaterials']
                  as Map<String, dynamic>,
              'root': const ComponentMaterial(followComponent: 'leaf').toJson(),
            },
          };
          await expectLater(
            host.savePreferences(encodePreferences(cycle)),
            throwsA(anything),
          );
          final unchanged = SurfaceSettings.fromJson(
            decodePreferences((await host.readPreferences())!),
          );
          expect(unchanged.resolveComponent('leaf')!.blur, 4);
          expect(unchanged.components['root']!.followComponent, isNull);
        } finally {
          await host?.close();
          await directory.delete(recursive: true);
        }
      },
      skip: !Platform.isWindows || exe == null || package == null,
    );
  }
}
