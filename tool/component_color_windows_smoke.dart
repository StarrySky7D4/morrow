// Standalone release qualification. Uses the normal app binding and one stable
// view; native MSAA continuously queries the same process while dialogs close.
import 'dart:io';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart' show LiveWidgetController, find;
import 'package:window_manager/window_manager.dart';
import 'package:morrow_studio/main.dart';
import 'package:morrow_studio/color_compass.dart';
import 'package:morrow_studio/desktop_frame.dart';
import 'package:morrow_studio/storage.dart';
import 'package:morrow_studio/window_effects.dart';
import 'package:morrow_studio/theme_plugins/theme_plugin_controller.dart';
import '../test/theme_plugin_fixture.dart';

Future<void> main() async {
  final binding = WidgetsFlutterBinding.ensureInitialized();
  final ui = LiveWidgetController(binding);
  final output = File(Platform.environment['MORROW_COLOR_TEST_REPORT']!);
  final errors = <String>[];
  final messages = <String>[];
  FlutterError.onError = (details) {
    errors.add(details.toString());
  };
  void check(bool pass, String message) {
    if (!pass) throw StateError(message);
    messages.add('PASS: $message');
    output.writeAsStringSync(messages.join('\n'));
  }

  Future<void> settle() async {
    for (var i = 0; i < 12; i++) {
      await ui.pump(const Duration(milliseconds: 40));
    }
  }

  Future<void> tap(String key) async {
    stderr.writeln('STEP: $key');
    final finder = find.byKey(ValueKey(key));
    await ui.ensureVisible(finder);
    await settle();
    await ui.tap(finder);
    await settle();
  }

  Process? probe;
  final theme = ThemePluginController(
    store: MemoryThemePluginStore(),
    backend: ThemeBackend(artwork: true),
  );
  try {
    await initializeDesktopFrame();
    await windowManager.setSize(const Size(1440, 1000));
    await theme.restore();
    final storage = MemoryStorage();
    runApp(
      MorrowApp(
        storage: storage,
        themePlugins: theme,
        nativeBackground: DesktopBackground(),
        initialLocale: const Locale('zh'),
      ),
    );
    await settle();
    probe = await Process.start(
      Platform.environment['MORROW_ACCESSIBILITY_PROBE']!,
      ['$pid', '35000', Platform.environment['MORROW_ACCESSIBILITY_LOG']!],
    );
    await settle();
    await tap('component-settings');
    await tap('component-entry:hero');
    await tap('component-custom-toggle');
    for (var n = 0; n < 3; n++) {
      await tap('component-color');
      final field = find.byKey(const ValueKey('color-hex'));
      await ui.tap(field);
      await settle();
      ui.widget<TextField>(field).controller!.text = '#2468AB';
      await settle();
      await ui.tap(find.widgetWithText(FilledButton, '应用颜色'));
      await settle();
      check(
        find.byType(ColorCompassDialog).evaluate().isEmpty,
        'color dialog closes $n',
      );
    }
    await tap('component-apply');
    check(
      (storage.data!['componentMaterials'] as Map)['hero']['color'] ==
          0xff2468ab,
      'component color committed',
    );
    await tap('component-entry:hero');
    await tap('component-color');
    await theme.activate(themeId);
    await settle();
    check(
      ui
              .widget<FilledButton>(find.widgetWithText(FilledButton, '应用颜色'))
              .onPressed ==
          null,
      'open color dialog revoked by theme',
    );
    await ui.tap(
      find.descendant(
        of: find.byType(ColorCompassDialog),
        matching: find.text('取消'),
      ),
    );
    await settle();
    check(
      find.byKey(const ValueKey('component-color')).evaluate().isEmpty,
      'theme hides component compass',
    );
    await theme.setFullOverride(true);
    await settle();
    check(
      find.byKey(const ValueKey('component-apply')).evaluate().isEmpty,
      'material override hides apply',
    );
    await theme.deactivate();
    await settle();
    check(
      find.byKey(const ValueKey('component-color')).evaluate().length == 1,
      'theme unload restores compass',
    );
    await tap('component-apply');
    check(
      (storage.data!['componentMaterials'] as Map)['hero']['color'] ==
          0xff2468ab,
      'theme transition preserves saved color',
    );
    check(await probe.exitCode == 0, 'native accessibility hit testing');
    check(errors.isEmpty, 'no Flutter errors: $errors');
    exit(0);
  } catch (error, stack) {
    await output.writeAsString(
      '${messages.join('\n')}\nFAIL: $error\n$stack\n$errors',
    );
    exit(1);
  }
}
