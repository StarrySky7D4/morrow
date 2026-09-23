import 'dart:io';
import 'dart:ui' as ui;
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_i18n/morrow_i18n.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:morrow_studio/main.dart';
import 'package:morrow_studio/storage.dart';
import 'package:morrow_studio/component_material_page.dart';
import 'package:morrow_studio/component_context_menu.dart';
import 'package:morrow_studio/neumorphic_controls.dart';

void main() {
  Future<void> tap(WidgetTester t, String key) async {
    final finder = find.byKey(ValueKey(key));
    await t.ensureVisible(finder);
    await t.pumpAndSettle();
    await t.tap(finder);
    await t.pumpAndSettle();
  }

  testWidgets('Style list persists across restart and renders both designs', (
    t,
  ) async {
    t.view.devicePixelRatio = 1;
    t.view.physicalSize = const Size(1440, 1000);
    addTearDown(t.view.reset);
    final shadows = debugDisableShadows;
    debugDisableShadows = false;
    addTearDown(() => debugDisableShadows = shadows);
    final icons = File(
      'build/windows-corners/x64/runner/Release/data/flutter_assets/fonts/MaterialIcons-Regular.otf',
    );
    if (icons.existsSync()) {
      final loader = FontLoader('MaterialIcons')
        ..addFont(Future.value(ByteData.sublistView(icons.readAsBytesSync())));
      await t.runAsync(loader.load);
    }
    final font = File('C:/Windows/Fonts/msyh.ttc');
    if (font.existsSync()) {
      final loader = FontLoader('Segoe UI')
        ..addFont(Future.value(ByteData.sublistView(font.readAsBytesSync())));
      await t.runAsync(loader.load);
    }
    final storage = MemoryStorage();
    const capture = ValueKey('style-capture');
    Future<void> mount() async {
      await t.pumpWidget(
        RepaintBoundary(
          key: capture,
          child: MorrowApp(initialLocale: const Locale('zh'), storage: storage),
        ),
      );
      await t.pumpAndSettle();
    }

    Future<void> screenshot(String name) async {
      final boundary = t.renderObject<RenderRepaintBoundary>(
        find.byKey(capture),
      );
      await t.runAsync(() async {
        final image = await boundary.toImage();
        final bytes = await image.toByteData(format: ui.ImageByteFormat.png);
        image.dispose();
        await Directory('build/ui-style-preview').create(recursive: true);
        await File(
          'build/ui-style-preview/$name.png',
        ).writeAsBytes(bytes!.buffer.asUint8List());
      });
    }

    await mount();
    expect(
      t.widget<Studio>(find.byType(Studio)).palette.surfaces.visualStyle,
      VisualStyle.flat,
    );
    await screenshot('flat');
    expect(
      find.byKey(const ValueKey('visual-style-neumorphism')).hitTestable(),
      findsNothing,
    );
    await tap(t, 'visual-style-toggle');
    await tap(t, 'visual-style-neumorphism');
    expect(storage.data!['visualStyle'], 'neumorphism');
    expect(
      t.widget<Studio>(find.byType(Studio)).palette.surfaces.visualStyle,
      VisualStyle.neumorphism,
    );
    await t.ensureVisible(find.byKey(const ValueKey('appearance-heading')));
    await t.pumpAndSettle();
    await screenshot('neumorphism');
    await t.pumpWidget(const SizedBox());
    await mount();
    expect(
      t.widget<Studio>(find.byType(Studio)).palette.surfaces.visualStyle,
      VisualStyle.neumorphism,
    );
    expect(
      find.byKey(const ValueKey('visual-style-flat')).hitTestable(),
      findsNothing,
    );
    await tap(t, 'visual-style-toggle');
    await tap(t, 'visual-style-flat');
    expect(storage.data!['visualStyle'], 'flat');
    expect(t.takeException(), isNull);
    await t.pumpWidget(const SizedBox());
    debugDisableShadows = shadows;
  });

  testWidgets(
    'Component follow source disables cycle targets and restores own values',
    (t) async {
      t.view.devicePixelRatio = 1;
      t.view.physicalSize = const Size(980, 1000);
      addTearDown(t.view.reset);
      const surfaces = SurfaceSettings(
        components: {
          'a': ComponentMaterial(enabled: true, blur: 2, opacity: .2),
          'b': ComponentMaterial(enabled: true, blur: 9, opacity: .7),
          'c': ComponentMaterial(followComponent: 'a'),
        },
      );
      final p = const Palette(
        StudioTheme.white,
        GlassMode.frosted,
      ).withSurfaces(surfaces);
      ComponentMaterial? result;
      await t.pumpWidget(
        MaterialApp(
          locale: const Locale('zh'),
          supportedLocales: L10n.supportedLocales,
          localizationsDelegates: const [
            L10n.delegate,
            GlobalMaterialLocalizations.delegate,
            GlobalWidgetsLocalizations.delegate,
            GlobalCupertinoLocalizations.delegate,
          ],
          home: AppearanceScope(
            palette: p,
            child: Builder(
              builder: (context) => Scaffold(
                body: TextButton(
                  onPressed: () async {
                    result = await Navigator.of(context)
                        .push<ComponentMaterial>(
                          MaterialPageRoute(
                            builder: (_) => ComponentMaterialPage(
                              palette: p,
                              id: 'a',
                              title: 'A',
                              initial: surfaces.components['a']!,
                              entries: const {'a': 'A', 'b': 'B', 'c': 'C'},
                            ),
                          ),
                        );
                  },
                  child: const Text('open'),
                ),
              ),
            ),
          ),
        ),
      );
      await t.pumpAndSettle();
      await t.tap(find.text('open'));
      await t.pumpAndSettle();
      DropdownButton<String> picker() => t.widget<DropdownButton<String>>(
        find.byKey(const ValueKey('component-follow-source')),
      );
      expect(
        picker().items!.singleWhere((item) => item.value == 'c').enabled,
        isFalse,
      );
      await tap(t, 'component-follow-source');
      await t.tap(find.text('B').last);
      await t.pumpAndSettle();
      expect(picker().value, 'b');
      expect(
        t
            .widget<NeumorphicSwitchListTile>(
              find.byKey(const ValueKey('component-custom-toggle')),
            )
            .onChanged,
        isNull,
      );
      await tap(t, 'component-follow-source');
      await t.tap(find.text('主题或自身设置').last);
      await t.pumpAndSettle();
      expect(picker().value, '');
      await tap(t, 'component-apply');
      expect(result!.followComponent, isNull);
      expect(result!.blur, 2);
      expect(result!.opacity, .2);
    },
  );

  testWidgets(
    'Secondary click opens menu without primary action; only enabled command runs',
    (t) async {
      var taps = 0, chosen = 0;
      await t.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: Center(
              child: ComponentContextMenu(
                actions: () => [
                  ComponentMenuAction(
                    label: 'inspect',
                    icon: Icons.tune,
                    onSelected: () => chosen++,
                  ),
                  ComponentMenuAction(
                    label: 'disabled',
                    icon: Icons.block,
                    enabled: false,
                    onSelected: () => chosen += 100,
                  ),
                ],
                child: TextButton(
                  onPressed: () => taps++,
                  child: const Text('card'),
                ),
              ),
            ),
          ),
        ),
      );
      final gesture = await t.createGesture(
        kind: PointerDeviceKind.mouse,
        buttons: kSecondaryMouseButton,
      );
      await gesture.down(t.getCenter(find.text('card')));
      await gesture.up();
      await t.pumpAndSettle();
      expect(taps, 0);
      expect(chosen, 0);
      expect(find.text('inspect'), findsOneWidget);
      await t.tap(find.text('inspect'));
      await t.pumpAndSettle();
      expect(chosen, 1);
      expect(taps, 0);
      await t.tap(find.text('card'));
      await t.pumpAndSettle();
      expect(taps, 1);
      expect(t.takeException(), isNull);
    },
  );
}
