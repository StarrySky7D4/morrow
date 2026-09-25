import 'dart:convert';
import 'dart:io';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/main.dart';
import 'package:morrow_studio/liquid_glass.dart';
import 'package:morrow_studio/storage.dart';
import 'package:morrow_studio/theme_plugins/theme_plugin_controller.dart';
import 'package:morrow_studio/theme_plugins/theme_plugin_tile.dart';
import 'theme_plugin_fixture.dart';
import 'plugin_library_test.dart' as fixture;

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  final manifest = File('plugins/mid_autumn/theme.json').readAsStringSync();
  testWidgets('installed theme controls fit a narrow screen with double text', (
    t,
  ) async {
    t.view.physicalSize = const Size(300, 1000);
    t.view.devicePixelRatio = 1;
    addTearDown(t.view.reset);
    final c = ThemePluginController(
      store: MemoryThemePluginStore(),
      backend: ThemeBackend(),
    );
    addTearDown(c.dispose);
    await c.restore();
    await t.pumpWidget(
      ThemePluginScope(
        controller: c,
        child: MaterialApp(
          home: Scaffold(
            body: MediaQuery(
              data: const MediaQueryData(textScaler: TextScaler.linear(2)),
              child: SingleChildScrollView(
                child: Builder(
                  builder: (context) => ThemePluginTile(
                    controller: ThemePluginScope.of(context)!,
                    palette: const Palette(
                      StudioTheme.white,
                      GlassMode.frosted,
                    ),
                    onSelected: () {},
                  ),
                ),
              ),
            ),
          ),
        ),
      ),
    );
    await t.pumpAndSettle();
    await fixture.click(t, 'theme-plugin-$themeId');
    await fixture.click(t, 'theme-plugin-full-override');
    expect(c.fullOverride, isTrue);
    await fixture.click(t, 'theme-plugin-deactivate');
    expect(c.active, isFalse);
    expect(t.takeException(), isNull);
  });
  test(
    'registry owns install, mutual exclusion, restart and uninstall',
    () async {
      final api = ThemeBackend();
      final store = MemoryThemePluginStore();
      final c = ThemePluginController(store: store, backend: api);
      await c.restore();
      expect(c.active, isFalse);
      expect(c.entries.length, 2);
      expect(await c.activate(themeId), isTrue);
      expect(await c.setFullOverride(true), isTrue);
      final restored = ThemePluginController(store: store, backend: api);
      await restored.restore();
      expect(restored.plugin!.id, themeId);
      expect(restored.fullOverride, isTrue);
      await restored.activate(otherThemeId);
      expect(
        api.entries.where((e) => e.isTheme && e.enabled).single.id,
        otherThemeId,
      );
      expect(api.entries.singleWhere((e) => e.id == 'a').enabled, isTrue);
      expect(restored.fullOverride, isFalse);
      final entry = api.entries.singleWhere((e) => e.id == otherThemeId);
      await api.removeExternal(entry, api.revision);
      await restored.refresh();
      expect(restored.active, isFalse);
      expect(restored.entries.length, 1);
      c.dispose();
      restored.dispose();
    },
  );
  test(
    'old embedded theme state cannot activate or reinstall a package',
    () async {
      final c = ThemePluginController(
        store: MemoryThemePluginStore()
          ..value = '{"version":1,"loaded":true,"active":true}',
        backend: ThemeBackend(installed: false),
      );
      await c.restore();
      expect(c.plugin, isNull);
      expect(c.entries, isEmpty);
      c.dispose();
    },
  );
  test('failed preference write keeps current material mode', () async {
    final store = _FailingStore();
    final c = ThemePluginController(store: store, backend: ThemeBackend());
    await c.restore();
    await c.activate(themeId);
    store.fail = true;
    expect(await c.setFullOverride(true), isFalse);
    expect(c.fullOverride, isFalse);
    expect(c.active, isTrue);
    store.fail = false;
    expect(await c.setFullOverride(true), isTrue);
    c.dispose();
  });
  test(
    'bad payload, asset digest and changed registry cannot publish a theme',
    () async {
        for (final fault in ['descriptor', 'digest', 'dimensions', 'revision']) {
        final api = ThemeBackend(artwork: fault != 'descriptor');
        if (fault == 'descriptor') api.descriptions[themeId] = '{}';
          if (fault == 'digest') api.image![0] ^= 1;
          if (fault == 'dimensions') {
            final descriptor = jsonDecode(api.descriptions[themeId]!) as Map;
            (descriptor['artwork'] as Map)['width'] = 1;
            api.descriptions[themeId] = jsonEncode(descriptor);
          }
        if (fault == 'revision') api.staleImage = true;
        final c = ThemePluginController(
          store: MemoryThemePluginStore(),
          backend: api,
        );
        await c.restore();
        expect(await c.activate(themeId), isFalse);
        expect(c.active, isFalse);
        expect(c.error, isNotNull);
        expect(api.entries.singleWhere((e) => e.id == 'a').enabled, isTrue);
        c.dispose();
      }
    },
  );
  testWidgets(
    'theme combines with styles and hides only ineffective controls',
    (t) async {
      t.view.physicalSize = const Size(1440, 1000);
      t.view.devicePixelRatio = 1;
      addTearDown(t.view.reset);
      final c = ThemePluginController(
        store: MemoryThemePluginStore(),
        backend: ThemeBackend(),
      );
      addTearDown(c.dispose);
      await c.restore();
      final storage = MemoryStorage();
      await t.pumpWidget(MorrowApp(storage: storage, themePlugins: c));
      await t.pumpAndSettle();
      expect(find.byKey(const ValueKey('theme-color-compass')), findsOneWidget);
      await c.activate(themeId);
      await t.pumpAndSettle();
      expect(find.byKey(const ValueKey('theme-color-compass')), findsNothing);
      expect(find.byKey(const ValueKey('theme-color-reset')), findsNothing);
      expect(find.byKey(const ValueKey('theme-custom')), findsNothing);
      expect(find.byKey(const ValueKey('theme-dark')), findsOneWidget);
      expect(find.byKey(const ValueKey('component-settings')), findsOneWidget);
      expect(
        find.byKey(const ValueKey('background-transparent')),
        findsOneWidget,
      );
      await c.setFullOverride(true);
      await t.pumpAndSettle();
      for (final key in [
        'component-settings',
        'corner-radius',
        'background-transparent',
        'frosted-opacity',
        'canvas-liquid-toggle',
      ]) {
        expect(find.byKey(ValueKey(key)), findsNothing, reason: key);
      }
      await fixture.click(t, 'visual-style-toggle');
      await fixture.click(t, 'visual-style-clay');
      expect(c.active, isTrue);
      expect(
        t.widget<Studio>(find.byType(Studio)).palette.visualStyle,
        VisualStyle.clay,
      );
      expect(find.byKey(const ValueKey('style-depth')), findsOneWidget);
      await c.deactivate();
      await t.pumpAndSettle();
      for (final key in [
        'theme-color-compass',
        'theme-custom',
        'component-settings',
        'corner-radius',
        'background-transparent',
      ]) {
        expect(find.byKey(ValueKey(key)), findsOneWidget, reason: key);
      }
      expect(
        t.widget<Studio>(find.byType(Studio)).palette.visualStyle,
        VisualStyle.clay,
      );
      expect(t.takeException(), isNull);
    },
  );
  testWidgets(
    'theme preserves custom glass until material override is selected',
    (t) async {
      final tokens = UiThemePlugin.parse(manifest, expectedId: themeId).light;
      for (final override in [false, true, false]) {
        final p = Palette(
          StudioTheme.white,
          GlassMode.clear,
          BackgroundMode.solid,
          0,
          .76,
          Colors.blue,
          null,
          true,
          20,
          0,
          null,
          20,
          false,
          null,
          const SurfaceSettings(
            components: {
              'card': ComponentMaterial(
                enabled: true,
                mode: GlassMode.liquid,
                cornerRadius: 6,
                blur: 8,
                opacity: .3,
              ),
            },
          ),
          tokens,
          override,
        );
        await t.pumpWidget(
          MaterialApp(
            home: Glass(
              p: p,
              componentId: 'card',
              child: const SizedBox(width: 200, height: 120),
            ),
          ),
        );
        await t.pumpAndSettle();
        final glass = t.widget<LiquidGlassSurface>(
          find.byType(LiquidGlassSurface),
        );
        expect(glass.material!.liquid, override ? 0 : 1);
        expect(glass.borderRadius.topLeft.x, override ? tokens.radius : 6);
        expect(p.solidColor, override ? tokens.background : Colors.blue);
      }
    },
  );
  test('theme validates identity, budget and readable light/dark tokens', () {
    final plugin = UiThemePlugin.parse(manifest, expectedId: themeId);
    double contrast(Color a, Color b) {
      final x = a.computeLuminance(), y = b.computeLuminance();
      return ((x > y ? x : y) + .05) / ((x > y ? y : x) + .05);
    }

    for (final tokens in [plugin.light, plugin.dark]) {
      for (final text in [tokens.ink, tokens.muted, tokens.accent]) {
        expect(contrast(text, tokens.background), greaterThanOrEqualTo(4.5));
        expect(contrast(text, tokens.surface), greaterThanOrEqualTo(4.5));
      }
    }
    expect(
      () => UiThemePlugin.parse(manifest, expectedId: otherThemeId),
      throwsFormatException,
    );
    final data = jsonDecode(manifest) as Map<String, dynamic>;
    data['artwork']['bytes'] = 2000000;
    expect(
      () => UiThemePlugin.parse(jsonEncode(data), expectedId: themeId),
      throwsFormatException,
    );
  });
}

class _FailingStore extends MemoryThemePluginStore {
  bool fail = false;
  @override
  Future<void> write(String value) async {
    if (fail) throw StateError('disk full');
    await super.write(value);
  }
}
