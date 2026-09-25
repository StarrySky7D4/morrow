import 'dart:convert';
import 'theme_plugin_fixture.dart';
import 'dart:io';
import 'dart:ui' as ui;
import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/main.dart';
import 'package:morrow_studio/storage.dart';
import 'package:morrow_studio/theme_plugins/theme_plugin_controller.dart';

void main() {
  Future<void> tap(WidgetTester t, String key) async {
    final finder = find.byKey(ValueKey(key));
    await t.ensureVisible(finder);
    await t.pumpAndSettle();
    await t.tap(finder);
    await t.pumpAndSettle();
  }

  testWidgets(
    'plugin covers the workbench, preserves drafts/preferences, restores after unload',
    (t) async {
      t.view.devicePixelRatio = 1;
      t.view.physicalSize = const Size(1440, 1000);
      addTearDown(t.view.reset);
      final storage = MemoryStorage();
      final pluginStore = MemoryThemePluginStore();
      final backend = ThemeBackend();
      final c = ThemePluginController(store: pluginStore, backend: backend);
      await c.restore();
      addTearDown(c.dispose);
      await t.pumpWidget(
        MorrowApp(
          storage: storage,
          themePlugins: c,
          initialLocale: const Locale('zh'),
        ),
      );
      await t.pumpAndSettle();
      Studio studio() => t.widget<Studio>(find.byType(Studio));
      studio().onSurfaces!(
        const SurfaceSettings(
          visualStyle: VisualStyle.clay,
          components: {
            'settings': ComponentMaterial(
              enabled: true,
              opacity: .2,
              cornerRadius: 4,
            ),
          },
        ),
      );
      studio().onThemeColor(Colors.red);
      await t.pumpAndSettle();
      studio().onAppearanceCommit();
      await t.pumpAndSettle();
      final saved = jsonEncode(storage.data);
      final base = studio().palette;
      final state = t.state(find.byType(Studio));
      final search = find.byType(TextField).first;
      await t.enterText(search, 'unsaved search draft');
      final field = t.widget<TextField>(search).controller!;
      await tap(t, 'visual-style-toggle');
      await tap(t, 'theme-plugin-$themeId');
      expect(c.active, isTrue);
      expect(find.byKey(const ValueKey('theme-plugin-canvas')), findsOneWidget);
      expect(studio().palette.accent, c.plugin!.light.accent);
      expect(studio().palette.visualStyle, VisualStyle.clay);
      expect(studio().palette.surfaces.visualStyle, VisualStyle.clay);
      expect(jsonEncode(storage.data), saved);
      expect(identical(t.state(find.byType(Studio)), state), isTrue);
      expect(field.text, 'unsaved search draft');
      await tap(t, 'visual-style-toggle');
      await tap(t, 'theme-plugin-deactivate');
      expect(c.plugin, isNull);
      expect(find.byKey(const ValueKey('theme-plugin-canvas')), findsNothing);
      expect(studio().palette.accent, base.accent);
      expect(studio().palette.background, base.background);
      expect(studio().palette.surfaces.toJson(), base.surfaces.toJson());
      expect(jsonEncode(storage.data), saved);
      expect(field.text, 'unsaved search draft');
      await tap(t, 'theme-plugin-$themeId');
      await tap(t, 'visual-style-toggle');
      await tap(t, 'visual-style-paper');
      expect(c.active, isTrue);
      expect(c.plugin, isNotNull);
      expect(storage.data!['visualStyle'], 'paper');
      await t.tap(find.text('新建灵感').first);
      await t.pumpAndSettle();
      final title = find.byKey(const ValueKey('idea-title'));
      await t.enterText(title, '中秋未保存草稿');
      final editor = t.state(find.byType(NewIdeaDialog));
      final draft = t.widget<TextField>(title).controller!;
      await c.activate(themeId);
      await t.pumpAndSettle();
      expect(identical(t.state(find.byType(NewIdeaDialog)), editor), isTrue);
      expect(draft.text, '中秋未保存草稿');
      await c.deactivate();
      await t.pumpAndSettle();
      expect(identical(t.state(find.byType(NewIdeaDialog)), editor), isTrue);
      expect(draft.text, '中秋未保存草稿');
      expect(storage.data!['visualStyle'], 'paper');
      expect(t.takeException(), isNull);
      await t.pumpWidget(const SizedBox());
      final restored = ThemePluginController(
        store: pluginStore,
        backend: backend,
      );
      await restored.restore();
      addTearDown(restored.dispose);
      await t.pumpWidget(MorrowApp(storage: storage, themePlugins: restored));
      await t.pumpAndSettle();
      expect(restored.active, isFalse);
      expect(studio().palette.surfaces.visualStyle, VisualStyle.paper);
    },
  );

  testWidgets('light and dark workbench captures', (t) async {
    t.view.devicePixelRatio = 1;
    t.view.physicalSize = const Size(1440, 1000);
    addTearDown(t.view.reset);
    for (final entry in {
      'MaterialIcons':
          'C:/flutter/bin/cache/artifacts/material_fonts/MaterialIcons-Regular.otf',
      'Segoe UI': 'C:/Windows/Fonts/msyh.ttc',
    }.entries) {
      final file = File(entry.value);
      if (file.existsSync()) {
        final loader = FontLoader(entry.key)
          ..addFont(Future.value(ByteData.sublistView(file.readAsBytesSync())));
        await t.runAsync(loader.load);
      }
    }
    final backend = ThemeBackend(artwork: true);
    final c = ThemePluginController(
      store: MemoryThemePluginStore(),
      backend: backend,
    );
    addTearDown(c.dispose);
    await c.restore();
    await t.runAsync(() => c.activate(themeId));
    const capture = ValueKey('mid-autumn-capture');
    await t.pumpWidget(
      RepaintBoundary(
        key: capture,
        child: MorrowApp(themePlugins: c, initialLocale: const Locale('zh')),
      ),
    );
    await t.pumpAndSettle();
    for (final theme in [StudioTheme.white, StudioTheme.dark]) {
      t.widget<Studio>(find.byType(Studio)).onTheme(theme);
      await t.pumpAndSettle();
      await t.runAsync(
        () => precacheImage(
          MemoryImage(c.plugin!.artwork!),
          t.element(find.byType(Studio)),
        ),
      );
      await t.pumpAndSettle();
      final boundary = t.renderObject<RenderRepaintBoundary>(
        find.byKey(capture),
      );
      await t.runAsync(() async {
        final image = await boundary.toImage();
        final bytes = await image.toByteData(format: ui.ImageByteFormat.png);
        image.dispose();
        await Directory('build/mid-autumn-v3-preview').create(recursive: true);
        await File(
          'build/mid-autumn-v3-preview/${theme.name}.png',
        ).writeAsBytes(bytes!.buffer.asUint8List());
      });
      expect(t.takeException(), isNull);
    }
  });
}
