import 'package:morrow_studio/main.dart';
import 'package:morrow_studio/liquid_glass.dart';
import 'package:morrow_studio/component_material_page.dart';
import 'package:morrow_studio/desktop_frame.dart';
import 'package:morrow_studio/storage.dart';
import 'package:morrow_studio/window_effects.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

class NoBackground extends DesktopBackground {
  @override
  Future<void> apply({double frost = 0}) async {}
}

void main() {
  Future<void> size(WidgetTester tester, double width) async {
    tester.view.devicePixelRatio = 1;
    tester.view.physicalSize = Size(width, 900);
    addTearDown(tester.view.resetDevicePixelRatio);
    addTearDown(tester.view.resetPhysicalSize);
  }

  Future<void> tap(WidgetTester tester, String key) async {
    final finder = find.byKey(ValueKey(key));
    await tester.ensureVisible(finder);
    await tester.pumpAndSettle();
    await tester.tap(finder);
    await tester.pumpAndSettle();
  }

  Future<void> slide(WidgetTester tester, String key, double value) async {
    final slider = tester.widget<Slider>(find.byKey(ValueKey(key)));
    slider.onChanged!(value);
    slider.onChangeEnd?.call(value);
    await tester.pumpAndSettle();
  }

  Palette palette(WidgetTester tester) =>
      tester.widget<Studio>(find.byType(Studio)).palette;

  testWidgets('Per-card material changes only its own glass shell', (
    tester,
  ) async {
    await size(tester, 390);
    const surfaces = SurfaceSettings(
      componentCustom: true,
      componentBlur: 40,
      componentOpacity: 1,
      components: {
        'card:a': ComponentMaterial(
          enabled: true,
          blur: 2,
          opacity: 0,
          color: Colors.red,
        ),
        'card:b': ComponentMaterial(
          enabled: false,
          blur: 40,
          opacity: 1,
          color: Colors.blue,
        ),
      },
    );
    const p = Palette(
      StudioTheme.white,
      GlassMode.clear,
      BackgroundMode.transparent,
      0,
      .76,
      null,
      null,
      true,
      20,
      0,
      null,
      20,
      false,
      null,
      surfaces,
    );
    await tester.pumpWidget(
      const MaterialApp(
        home: Row(
          children: [
            Glass(
              p: p,
              componentId: 'card:a',
              child: SizedBox(width: 100, height: 100),
            ),
            Glass(
              p: p,
              componentId: 'card:b',
              child: SizedBox(width: 100, height: 100),
            ),
          ],
        ),
      ),
    );
    await tester.pumpAndSettle();
    final materials = tester
        .widgetList<LiquidGlassSurface>(find.byType(LiquidGlassSurface))
        .toList();
    expect(materials[0].material!.blur, 2);
    expect(
      (materials[0].material!.decoration.gradient! as LinearGradient)
          .colors
          .first
          .a,
      0,
    );
    expect(materials[1].material!.blur, 1);
    expect(materials[1].tint, isNot(Colors.red));
    await tester.pumpWidget(
      MaterialApp(
        home: ComponentMaterialPage(
          palette: p,
          id: 'card:a',
          title: '很长的卡片标题，也应在窄屏内完整操作',
          initial: surfaces.components['card:a']!,
        ),
      ),
    );
    await tester.pumpAndSettle();
    await tester.ensureVisible(find.byKey(const ValueKey('component-apply')));
    await tester.pumpAndSettle();
    expect(tester.takeException(), isNull);
  });

  testWidgets(
    'Canvas tint is independent and component override survives theme and reload',
    (tester) async {
      await size(tester, 1440);
      final storage = MemoryStorage();
      await tester.pumpWidget(MorrowApp(storage: storage));
      await tester.pumpAndSettle();
      expect(palette(tester).surfaces.componentCustom, isFalse);
      await tap(tester, 'background-transparent');
      await slide(tester, 'canvas-opacity', .3);
      await slide(tester, 'canvas-blur', 12);
      var studio = tester.widget<Studio>(find.byType(Studio));
      studio.onSurfaces!(
        palette(tester).surfaces.copyWith(canvasColor: const Color(0xff44aa99)),
      );
      studio.onAppearanceCommit();
      await tester.pumpAndSettle();
      final canvas = tester.widget<AnimatedContainer>(
        find.byKey(const ValueKey('transparent-canvas-tint')),
      );
      expect(
        (canvas.decoration! as BoxDecoration).color,
        const Color(0xff44aa99).withValues(alpha: .3),
      );
      // A background autosave must not commit a still-open color preview.
      studio = tester.widget<Studio>(find.byType(Studio));
      final before = palette(tester).surfaces;
      studio.onSurfaces!(before.copyWith(canvasColor: Colors.red));
      studio.onSave({'ideas': <dynamic>[], 'completed': <String>[]});
      await tester.pumpAndSettle();
      expect(storage.read()!['canvasColor'], 0xff44aa99);
      studio.onSurfaces!(before);
      await tester.pumpAndSettle();
      await tap(tester, 'component-settings');
      await tap(tester, 'component-entry:hero');
      await tap(tester, 'component-custom-toggle');
      await slide(tester, 'component-blur', 1);
      await slide(tester, 'component-opacity', .18);
      await tap(tester, 'component-apply');
      await tester.pageBack();
      await tester.pumpAndSettle();
      expect(palette(tester).surfaces.components['hero']!.blur, 1);
      expect(palette(tester).surfaces.components['hero']!.enabled, isTrue);
      expect(palette(tester).surfaces.components['navigation'], isNull);
      await tap(tester, 'theme-dark');
      expect(palette(tester).surfaces.components['hero']!.opacity, .18);
      expect(palette(tester).surfaces.canvasOpacity, .3);
      await tap(tester, 'component-settings');
      await tap(tester, 'component-entry:hero');
      await tap(tester, 'component-custom-toggle');
      await tap(tester, 'component-apply');
      await tester.pageBack();
      await tester.pumpAndSettle();
      expect(palette(tester).surfaces.components['hero']!.enabled, isFalse);
      expect(palette(tester).surfaces.components['hero']!.blur, 1);
      await tap(tester, 'component-settings');
      await tap(tester, 'component-entry:navigation');
      await tap(tester, 'component-custom-toggle');
      await slide(tester, 'component-opacity', .8);
      await tester.pageBack(); // cancel: no live or persisted mutation
      await tester.pumpAndSettle();
      await tester.pageBack();
      await tester.pumpAndSettle();
      expect(palette(tester).surfaces.components['navigation'], isNull);
      await slide(tester, 'canvas-opacity', 0);
      final zeroCanvas = tester.widget<AnimatedContainer>(
        find.byKey(const ValueKey('transparent-canvas-tint')),
      );
      expect((zeroCanvas.decoration! as BoxDecoration).color!.a, 0);
      await tester.pumpWidget(const SizedBox());
      await tester.pumpWidget(MorrowApp(storage: storage));
      await tester.pumpAndSettle();
      expect(palette(tester).surfaces.components['hero']!.enabled, isFalse);
      expect(palette(tester).surfaces.components['hero']!.opacity, .18);
      expect(palette(tester).surfaces.components['navigation'], isNull);
      expect(palette(tester).surfaces.canvasColor, const Color(0xff44aa99));
      expect(palette(tester).surfaces.canvasBlur, 12);
      expect(palette(tester).surfaces.canvasOpacity, 0);
      expect(tester.takeException(), isNull);
    },
    variant: TargetPlatformVariant.only(TargetPlatform.windows),
  );

  testWidgets(
    'Custom controls are collapsed, presets locked, values and zero radius persist',
    (tester) async {
      await size(tester, 1440);
      final storage = MemoryStorage();
      await tester.pumpWidget(MorrowApp(storage: storage));
      await tester.pumpAndSettle();
      expect(find.byKey(const ValueKey('theme-mist')), findsNothing);
      expect(find.byKey(const ValueKey('corners-square')), findsNothing);
      expect(find.byKey(const ValueKey('theme-grayscale')), findsNothing);
      await tap(tester, 'theme-custom');
      expect(find.byKey(const ValueKey('theme-lightness')), findsNothing);
      await tap(tester, 'custom-tone-toggle');
      await slide(tester, 'theme-grayscale', 1);
      await slide(tester, 'theme-lightness', .24);
      await slide(tester, 'corner-radius', 0);
      final custom = palette(tester);
      expect(custom.borderRadius(20), BorderRadius.zero);
      expect(custom.background.r, closeTo(.24, .001));
      expect(custom.accent.r, closeTo(custom.accent.g, .00001));
      expect(custom.dark, isTrue);
      for (final theme in [StudioTheme.white, StudioTheme.dark]) {
        await tap(tester, 'theme-${theme.name}');
        expect(find.byKey(const ValueKey('theme-grayscale')), findsNothing);
        expect(
          palette(tester).background,
          Palette(theme, GlassMode.frosted).background,
        );
        expect(
          palette(tester).accent,
          Palette(theme, GlassMode.frosted).accent,
        );
        tester.widget<Studio>(find.byType(Studio)).onLightness(.8);
        tester.widget<Studio>(find.byType(Studio)).onGrayscale(0);
        await tester.pumpAndSettle();
        expect(palette(tester).themeLightness, .24);
        expect(palette(tester).grayscale, 1);
      }
      await tap(tester, 'theme-custom');
      expect(find.byKey(const ValueKey('theme-grayscale')), findsNothing);
      expect(palette(tester).background, custom.background);
      expect(storage.data!['cornerRadius'], 0);
      expect(storage.data!.containsKey('rounded'), isFalse);
      await tester.pumpWidget(const SizedBox());
      await tester.pumpWidget(MorrowApp(storage: storage));
      await tester.pumpAndSettle();
      expect(palette(tester).background, custom.background);
      expect(palette(tester).borderRadius(20), BorderRadius.zero);
      expect(find.byKey(const ValueKey('theme-grayscale')), findsNothing);
      await slide(tester, 'corner-radius', 32);
      expect(palette(tester).borderRadius(20).topLeft.x, 32);
      expect(tester.takeException(), isNull);
      await tester.pumpWidget(const SizedBox());
    },
  );

  testWidgets(
    'Legacy mist and square settings migrate without dropping content',
    (tester) async {
      await size(tester, 1440);
      final storage = MemoryStorage();
      await tester.pumpWidget(MorrowApp(storage: storage));
      await tester.pumpAndSettle();
      await slide(tester, 'corner-radius', 32);
      final count = (storage.data!['ideas'] as List).length;
      storage.data!.addAll({'theme': 'mist', 'rounded': false});
      await tester.pumpWidget(const SizedBox());
      await tester.pumpWidget(MorrowApp(storage: storage));
      await tester.pumpAndSettle();
      expect(palette(tester).theme, StudioTheme.custom);
      expect(palette(tester).cornerRadius, 0);
      await slide(tester, 'corner-radius', 12);
      expect((storage.data!['ideas'] as List).length, count);
      expect(storage.data!['theme'], 'custom');
      expect(storage.data!.containsKey('rounded'), isFalse);
      await tester.pumpWidget(const SizedBox());
    },
  );

  testWidgets(
    'Footer stays at viewport bottom during page and settings scroll',
    (tester) async {
      await size(tester, 1440);
      for (final width in [1440.0, 800.0, 390.0]) {
        tester.view.physicalSize = Size(width, 900);
        await tester.pumpWidget(const MorrowApp());
        await tester.pumpAndSettle();
        final footer = find.byKey(const ValueKey('footer-dock'));
        final rect = tester.getRect(footer);
        expect(rect.bottom, closeTo(900 - (width >= 1050 ? 24 : 12), .1));
        final page = find.byKey(const ValueKey('page-概览'));
        final scrollable = find
            .descendant(of: page, matching: find.byType(Scrollable))
            .first;
        tester.state<ScrollableState>(scrollable).position.jumpTo(600);
        await tester.pumpAndSettle();
        expect(tester.getRect(footer), rect);
        expect(
          find.ancestor(of: footer, matching: find.byType(Scrollable)),
          findsNothing,
        );
        if (width >= 760) {
          await tester.tap(find.text('已收藏'));
          await tester.pumpAndSettle();
          expect(tester.getRect(footer), rect);
        }
        await tester.tap(find.byKey(const ValueKey('appearance-toggle')));
        await tester.pumpAndSettle();
        if (width < 1050) {
          expect(footer, findsNothing);
          await tester.tap(find.byKey(const ValueKey('compact-settings-back')));
          await tester.pumpAndSettle();
        }
        expect(tester.getRect(footer), rect);
        expect(tester.takeException(), isNull);
        await tester.pumpWidget(const SizedBox());
      }
    },
  );

  testWidgets(
    'Window radius updates native channel and outline independently and restores',
    (tester) async {
      await size(tester, 1440);
      final calls = <MethodCall>[];
      final messenger = tester.binding.defaultBinaryMessenger;
      const shape = MethodChannel('morrow/window_shape');
      const manager = MethodChannel('window_manager');
      messenger.setMockMethodCallHandler(shape, (call) async {
        calls.add(call);
        return null;
      });
      messenger.setMockMethodCallHandler(manager, (_) async => false);
      addTearDown(() {
        messenger.setMockMethodCallHandler(shape, null);
        messenger.setMockMethodCallHandler(manager, null);
      });
      final storage = MemoryStorage();
      await tester.pumpWidget(
        MorrowApp(storage: storage, nativeBackground: NoBackground()),
      );
      await tester.pumpAndSettle();
      expect(calls.last.arguments, 20.0);
      for (final radius in [0.0, 32.0, 12.0]) {
        await slide(tester, 'window-radius', radius);
        expect(calls.last.method, 'setRadius');
        expect(calls.last.arguments, radius);
        final outline = tester.widget<AnimatedContainer>(
          find.byKey(const ValueKey('desktop-outline')),
        );
        expect(
          (outline.decoration as BoxDecoration).borderRadius,
          BorderRadius.circular(radius),
        );
        expect(palette(tester).cornerRadius, 20);
      }
      expect(storage.data!['windowRadius'], 12);
      await tester.pumpWidget(const SizedBox());
      await tester.pumpWidget(
        MorrowApp(storage: storage, nativeBackground: NoBackground()),
      );
      await tester.pumpAndSettle();
      expect(calls.last.arguments, 12.0);
      expect(find.byType(DesktopFrame), findsOneWidget);
      await tap(tester, 'background-transparent');
      await slide(tester, 'canvas-opacity', .3);
      final tintRect = tester.getRect(
        find.byKey(const ValueKey('transparent-canvas-tint')),
      );
      final captionRect = tester.getRect(
        find.byKey(const ValueKey('desktop-caption')),
      );
      expect(tintRect.top, captionRect.top);
      expect(tintRect.bottom, greaterThan(captionRect.bottom));
      expect(palette(tester).captionColor, Colors.transparent);
      expect(tester.takeException(), isNull);
      await tester.pumpWidget(const SizedBox());
    },
    variant: TargetPlatformVariant.only(TargetPlatform.windows),
  );
}
