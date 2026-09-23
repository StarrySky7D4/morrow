import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/appearance.dart';
import 'package:morrow_studio/liquid_glass.dart';

void main() {
  test(
    'visual style defaults to flat and follows serialize without losing local values',
    () {
      const original = SurfaceSettings(
        components: {
          'card:a': ComponentMaterial(
            enabled: true,
            blur: 3,
            opacity: .2,
            followComponent: 'card:b',
          ),
          'card:b': ComponentMaterial(enabled: true, blur: 7, opacity: .6),
        },
      );
      expect(SurfaceSettings.fromJson({}).visualStyle, VisualStyle.flat);
      final restored = SurfaceSettings.fromJson(
        original.copyWith(visualStyle: VisualStyle.neumorphism).toJson(),
      );
      expect(restored.visualStyle, VisualStyle.neumorphism);
      expect(restored.resolveComponent('card:a')!.blur, 7);
      expect(restored.components['card:a']!.blur, 3);
      expect(
        restored.components['card:a']!.copyWith(clearFollow: true).blur,
        3,
      );
      expect(
        restored
            .copyWith(
              components: {
                ...restored.components,
                'card:b': const ComponentMaterial(followComponent: 'missing'),
              },
            )
            .resolveComponent('card:a')!
            .enabled,
        isFalse,
      );
      expect(restored.resolveComponent('missing'), isNull);
      expect(
        () => SurfaceSettings.fromJson({'visualStyle': 'other'}),
        throwsFormatException,
      );
    },
  );

  test('component links reject direct and indirect cycles', () {
    const direct = SurfaceSettings(
      components: {'a': ComponentMaterial(followComponent: 'a')},
    );
    const indirect = SurfaceSettings(
      components: {
        'a': ComponentMaterial(followComponent: 'b'),
        'b': ComponentMaterial(followComponent: 'c'),
        'c': ComponentMaterial(followComponent: 'a'),
      },
    );
    expect(() => direct.resolveComponent('a'), throwsFormatException);
    expect(() => indirect.resolveComponent('a'), throwsFormatException);
  });

  testWidgets('neumorphism leaves the clear panel interior translucent', (
    tester,
  ) async {
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.resetDevicePixelRatio);
    const surfaces = SurfaceSettings(
      visualStyle: VisualStyle.neumorphism,
      components: {
        'panel': ComponentMaterial(
          enabled: true,
          mode: GlassMode.clear,
          blur: 1,
          opacity: .18,
        ),
      },
    );
    final key = GlobalKey();
    await tester.pumpWidget(
      MaterialApp(
        home: Center(
          child: RepaintBoundary(
            key: key,
            child: SizedBox(
              width: 140,
              height: 140,
              child: Center(
                child: Glass(
                  p: const Palette(
                    StudioTheme.white,
                    GlassMode.clear,
                  ).withSurfaces(surfaces),
                  componentId: 'panel',
                  child: const SizedBox(width: 100, height: 100),
                ),
              ),
            ),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();
    final boundary =
        key.currentContext!.findRenderObject()! as RenderRepaintBoundary;
    await tester.runAsync(() async {
      final image = await boundary.toImage(pixelRatio: 1);
      final rgba = await image.toByteData(format: ui.ImageByteFormat.rawRgba);
      expect(rgba, isNotNull);
      // Five pixels inside the lower edge, where the shifted light shadow
      // would otherwise leak through the transparent panel.
      final alpha = rgba!.getUint8((115 * image.width + 70) * 4 + 3);
      expect(alpha, lessThan(82));
      image.dispose();
    });
  });

  testWidgets(
    'neumorphism adds two-sided shadows without losing glass opacity',
    (tester) async {
      const local = SurfaceSettings(
        visualStyle: VisualStyle.neumorphism,
        components: {
          'panel': ComponentMaterial(
            enabled: true,
            blur: 2,
            opacity: .18,
            mode: GlassMode.clear,
          ),
        },
      );
      final palette = const Palette(
        StudioTheme.white,
        GlassMode.frosted,
      ).withSurfaces(local);
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: Center(
              child: Glass(
                p: palette,
                componentId: 'panel',
                child: const SizedBox(width: 100, height: 100),
              ),
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();
      final surface = tester.widget<LiquidGlassSurface>(
        find.byType(LiquidGlassSurface),
      );
      final material = surface.material!;
      expect(material.blur, 2);
      expect(material.decoration.boxShadow, hasLength(2));
      expect(material.decoration.boxShadow![0].offset.dx, lessThan(0));
      expect(material.decoration.boxShadow![1].offset.dx, greaterThan(0));
      final gradient = material.decoration.gradient! as LinearGradient;
      expect(gradient.colors.first.a, closeTo(.18, .001));
    },
  );
}
