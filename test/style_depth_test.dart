import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/main.dart';
import 'package:morrow_studio/storage.dart';
import 'package:morrow_studio/style_depth_slider.dart';
import 'package:morrow_studio/liquid_glass.dart';
import 'package:morrow_studio/neumorphic_controls.dart';
import 'package:morrow_studio/experimental_controls.dart';
import 'package:morrow_studio/style_input_border.dart';
import 'package:morrow_studio/plugins/studio_storage.dart';
import 'package:morrow_studio/component_material_page.dart';

void main() {
  test('depth defaults, explicit zero, follows and invalid data', () {
    expect(SurfaceSettings.fromJson({}).styleDepth, 1);
    expect(ComponentMaterial.fromJson({}).styleDepth, isNull);
    const s = SurfaceSettings(
      styleDepth: 1.6,
      components: {
        'a': ComponentMaterial(enabled: true, styleDepth: 0),
        'b': ComponentMaterial(followComponent: 'a', styleDepth: 2),
        'c': ComponentMaterial(followComponent: 'b'),
        'disabled': ComponentMaterial(styleDepth: .2),
        'inherit': ComponentMaterial(enabled: true),
      },
    );
    final r = SurfaceSettings.fromJson(s.toJson());
    expect(r.depthFor('c'), 0);
    expect(r.depthFor('disabled'), 1.6);
    expect(r.depthFor('inherit'), 1.6);
    expect(r.depthFor('missing'), 1.6);
    expect(r.components['b']!.copyWith(clearFollow: true).styleDepth, 2);
    expect(r.components['a']!.copyWith(inheritDepth: true).styleDepth, isNull);
    for (final v in [-.1, 2.1, double.nan, double.infinity]) {
      expect(
        () => SurfaceSettings.fromJson({'styleDepth': v}),
        throwsFormatException,
      );
      expect(
        () => ComponentMaterial.fromJson({'styleDepth': v}),
        throwsFormatException,
      );
    }
    final wire = decodePreferences(encodePreferences(s.toJson()));
    expect(wire['styleDepth'], 1.6);
    expect(SurfaceSettings.fromJson(wire).depthFor('c'), 0);
    expect(
      SurfaceSettings.fromJson(wire).components['inherit']!.styleDepth,
      isNull,
    );
    expect(decodePreferences(encodePreferences({}))['styleDepth'], 1);
  });

  testWidgets(
    'all supported styles vary relief without changing glass or control identity',
    (t) async {
      for (final style in VisualStyle.values.where((s) => s.supportsDepth)) {
        List<BoxShadow>? normal;
        for (final depth in [1.0, 0.0, 2.0]) {
          final p = const Palette(StudioTheme.white, GlassMode.clear)
              .withSurfaces(
                SurfaceSettings(
                  visualStyle: style,
                  styleDepth: .3,
                  components: {
                    'a': ComponentMaterial(
                      enabled: true,
                      blur: 3,
                      opacity: .17,
                      styleDepth: depth,
                    ),
                  },
                ),
              );
          await t.pumpWidget(
            MaterialApp(
              home: Center(
                child: SizedBox(
                  width: 240,
                  height: 100,
                  child: Glass(
                    componentId: 'a',
                    p: p,
                    child: const Text('Depth'),
                  ),
                ),
              ),
            ),
          );
          await t.pumpAndSettle();
          final material = t
              .widget<LiquidGlassSurface>(find.byType(LiquidGlassSurface))
              .material!;
          expect(material.blur, 3);
          expect(
            (material.decoration.gradient! as LinearGradient).colors.first.a,
            closeTo(.17, .001),
          );
          final shadows = material.decoration.boxShadow!;
          if (depth == 1) normal = shadows;
          for (var i = 0; i < shadows.length; i++) {
            expect(shadows[i].offset, normal![i].offset * depth);
            expect(shadows[i].blurRadius, normal[i].blurRadius);
            if (depth == 0) expect(shadows[i].color.a, 0);
          }
          final surface = t.widget<NeumorphicSurface>(
            find.byType(NeumorphicSurface),
          );
          expect(surface.palette.surfaces.styleDepth, depth);
        }
      }
    },
  );

  test('input relief carries depth across style and depth interpolation', () {
    for (final style in [
      VisualStyle.neumorphism,
      VisualStyle.clay,
      VisualStyle.industrial,
    ]) {
      StyledInputBorder border(double depth) {
        final p = const Palette(
          StudioTheme.white,
          GlassMode.frosted,
        ).withSurfaces(SurfaceSettings(visualStyle: style, styleDepth: depth));
        return applyVisualStyleControls(
              ThemeData(),
              p,
            ).inputDecorationTheme.enabledBorder!
            as StyledInputBorder;
      }

      final a = border(0), b = border(2);
      expect(a.relief.depth, 0);
      expect(b.relief.depth, 2);
      final middle = ShapeBorder.lerp(a, b, .5)! as StyledInputBorder;
      expect(middle.relief.depth, 1);
    }
  });

  testWidgets(
    'theme depth is hidden for flat, persists and resets to 100 percent',
    (t) async {
      t.view.devicePixelRatio = 1;
      t.view.physicalSize = const Size(1440, 1100);
      addTearDown(t.view.reset);
      final storage = MemoryStorage();
      Future<void> mount() async {
        await t.pumpWidget(
          MorrowApp(initialLocale: const Locale('zh'), storage: storage),
        );
        await t.pumpAndSettle();
      }

      await mount();
      expect(find.byType(StyleDepthSlider), findsNothing);
      final toggle = find.byKey(const ValueKey('visual-style-toggle'));
      await t.ensureVisible(toggle);
      await t.tap(toggle);
      await t.pumpAndSettle();
      final choice = find.byKey(const ValueKey('visual-style-clay'));
      await t.ensureVisible(choice);
      await t.tap(choice);
      await t.pumpAndSettle();
      final depth = find.byKey(const ValueKey('style-depth'));
      await t.ensureVisible(depth);
      await t.pumpAndSettle();
      final slider = find.descendant(of: depth, matching: find.byType(Slider));
      await t.drag(slider, const Offset(65, 0));
      await t.pumpAndSettle();
      final saved = storage.data!['styleDepth'] as double;
      expect(saved, greaterThan(1));
      expect(saved, lessThanOrEqualTo(2));
      await t.pumpWidget(const SizedBox());
      await mount();
      expect(
        t.widget<Studio>(find.byType(Studio)).palette.surfaces.styleDepth,
        saved,
      );
      final reset = find.descendant(
        of: depth,
        matching: find.byType(TextButton),
      );
      await t.ensureVisible(reset);
      await t.tap(reset);
      await t.pumpAndSettle();
      expect(storage.data!['styleDepth'], 1);
      expect(t.takeException(), isNull);
    },
  );

  testWidgets(
    'component depth previews locally and can resume inheriting theme',
    (t) async {
      t.view.physicalSize = const Size(1100, 1400);
      t.view.devicePixelRatio = 1;
      addTearDown(t.view.reset);
      final p = const Palette(StudioTheme.white, GlassMode.frosted)
          .withSurfaces(
            const SurfaceSettings(
              visualStyle: VisualStyle.neumorphism,
              styleDepth: 1.4,
            ),
          );
      const initial = ComponentMaterial(enabled: true, styleDepth: .4);
      await t.pumpWidget(
        MaterialApp(
          home: ComponentMaterialPage(
            palette: p,
            id: 'card:a',
            title: 'Card',
            initial: initial,
            entries: const {'card:a': 'Card'},
          ),
        ),
      );
      await t.pumpAndSettle();
      final depth = find.byKey(const ValueKey('component-style-depth'));
      await t.ensureVisible(depth);
      await t.pumpAndSettle();
      expect(t.widget<StyleDepthSlider>(depth).value, .4);
      final slider = find.descendant(of: depth, matching: find.byType(Slider));
      await t.drag(slider, const Offset(90, 0));
      await t.pumpAndSettle();
      expect(t.widget<StyleDepthSlider>(depth).value, greaterThan(.4));
      final inherit = find.byKey(const ValueKey('component-depth-inherit'));
      await t.ensureVisible(inherit);
      await t.tap(inherit);
      await t.pumpAndSettle();
      expect(t.widget<StyleDepthSlider>(depth).value, 1.4);
      expect(initial.styleDepth, .4);
      expect(p.surfaces.styleDepth, 1.4);
      expect(t.takeException(), isNull);
    },
  );
}
