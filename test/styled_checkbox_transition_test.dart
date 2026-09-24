import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/appearance.dart';
import 'package:morrow_studio/experimental_controls.dart';
import 'package:morrow_studio/styled_checkbox.dart';

Palette palette(VisualStyle style, {double depth = 1}) => const Palette(
  StudioTheme.white,
  GlassMode.frosted,
).withSurfaces(SurfaceSettings(visualStyle: style, styleDepth: depth));

Widget scene(
  VisualStyle style, {
  double depth = 1,
  bool? value = false,
  bool tristate = false,
  ValueChanged<bool?>? onChanged,
  bool reduced = false,
  bool visible = true,
  bool contrast = false,
  bool adaptive = false,
  TargetPlatform platform = TargetPlatform.windows,
  FocusNode? focusNode,
  OutlinedBorder? customShape,
  Duration themeDuration = Duration.zero,
}) {
  final p = palette(style, depth: depth);
  final base = ThemeData(
    platform: platform,
    useMaterial3: true,
    checkboxTheme: customShape == null
        ? const CheckboxThemeData()
        : CheckboxThemeData(shape: customShape),
  );
  final theme = customShape == null ? applyVisualStyleControls(base, p) : base;
  return MaterialApp(
    theme: theme,
    themeAnimationDuration: themeDuration,
    home: MediaQuery(
      data: MediaQueryData(disableAnimations: reduced, highContrast: contrast),
      child: TickerMode(
        enabled: visible,
        child: Scaffold(
          body: Center(
            child: StyledCheckbox(
              palette: p,
              value: value,
              tristate: tristate,
              onChanged: onChanged,
              adaptive: adaptive,
              focusNode: focusNode,
              semanticLabel: 'Choice',
              isError: true,
            ),
          ),
        ),
      ),
    ),
  );
}

Widget nativeTileScene(
  VisualStyle style, {
  StudioTheme mode = StudioTheme.white,
}) {
  final p = Palette(
    mode,
    GlassMode.frosted,
  ).withSurfaces(SurfaceSettings(visualStyle: style));
  return MaterialApp(
    theme: applyVisualStyleControls(ThemeData(useMaterial3: true), p),
    themeAnimationDuration: const Duration(milliseconds: 260),
    home: Scaffold(
      body: CheckboxListTile(
        title: const Text('Native tile'),
        value: false,
        onChanged: (_) {},
      ),
    ),
  );
}

StyledCheckboxThemeShape tileThemeShape(WidgetTester tester) =>
    Theme.of(tester.element(find.byType(Checkbox))).checkboxTheme.shape!
        as StyledCheckboxThemeShape;

StyledCheckboxBorder border(WidgetTester tester) =>
    tester.widget<Checkbox>(find.byType(Checkbox)).shape!
        as StyledCheckboxBorder;

void main() {
  testWidgets(
    'all seven shapes and inset relief reverse from the current frame',
    (tester) async {
      await tester.pumpWidget(
        scene(VisualStyle.neumorphism, onChanged: (_) {}),
      );
      await tester.pumpAndSettle();
      final native = tester.element(find.byType(Checkbox));
      expect(border(tester).recess, 1);

      await tester.pumpWidget(scene(VisualStyle.paper, onChanged: (_) {}));
      expect(border(tester).recess, 1);
      await tester.pump(const Duration(milliseconds: 60));
      final mid = border(tester);
      expect(mid.recess, inExclusiveRange(0, 1));
      expect(
        (mid.outline as RoundedRectangleBorder).borderRadius,
        isNot(palette(VisualStyle.paper).borderRadius(5)),
      );
      await tester.pumpWidget(
        scene(VisualStyle.neumorphism, onChanged: (_) {}),
      );
      expect(border(tester).recess, closeTo(mid.recess, .001));
      await tester.pumpAndSettle();
      expect(border(tester).recess, 1);
      expect(tester.element(find.byType(Checkbox)), same(native));

      for (final style in VisualStyle.values) {
        await tester.pumpWidget(scene(style, onChanged: (_) {}));
        await tester.pump(const Duration(milliseconds: 60));
        expect(tester.element(find.byType(Checkbox)), same(native));
        await tester.pumpAndSettle();
        expect(border(tester).recess, style == VisualStyle.neumorphism ? 1 : 0);
        if (style != VisualStyle.flat) {
          expect(
            (border(tester).outline as RoundedRectangleBorder).borderRadius,
            palette(style).borderRadius(5),
          );
        }
      }
    },
  );

  testWidgets('ThemeData lerp does not restart the palette transition', (
    tester,
  ) async {
    const duration = Duration(milliseconds: 260);
    const first = StyledCheckboxThemeShape(
      borderRadius: BorderRadius.all(Radius.circular(5)),
    );
    const second = StyledCheckboxThemeShape(
      borderRadius: BorderRadius.all(Radius.circular(2)),
    );
    expect(
      ShapeBorder.lerp(first, second, .5),
      isA<StyledCheckboxThemeShape>(),
    );
    expect(ShapeBorder.lerp(first, null, .5), isA<StyledCheckboxThemeShape>());
    expect(ShapeBorder.lerp(null, second, .5), isA<StyledCheckboxThemeShape>());

    await tester.pumpWidget(
      scene(
        VisualStyle.neumorphism,
        themeDuration: duration,
        onChanged: (_) {},
      ),
    );
    await tester.pumpAndSettle();
    await tester.pumpWidget(
      scene(VisualStyle.clay, themeDuration: duration, onChanged: (_) {}),
    );
    await tester.pump(const Duration(milliseconds: 180));
    expect(border(tester).recess, 0);
    expect(
      (border(tester).outline as RoundedRectangleBorder).borderRadius,
      palette(VisualStyle.clay).borderRadius(5),
    );
  });

  testWidgets('ordinary CheckboxListTile theme relief stays continuous', (
    tester,
  ) async {
    await tester.pumpWidget(nativeTileScene(VisualStyle.neumorphism));
    await tester.pumpAndSettle();
    final native = tester.element(find.byType(Checkbox));
    expect(find.byType(StyledCheckbox), findsNothing);
    expect(tester.widget<Checkbox>(find.byType(Checkbox)).shape, isNull);
    expect(tileThemeShape(tester).insetStrength, 1);

    await tester.pumpWidget(nativeTileScene(VisualStyle.paper));
    await tester.pump(const Duration(milliseconds: 65));
    final leaving = tileThemeShape(tester);
    expect(leaving.insetStrength, inExclusiveRange(0, 1));
    expect(leaving.insetShift, inExclusiveRange(0, 1.5));
    expect(leaving.insetShade.a, closeTo(.24, .01));
    expect(leaving.insetLight.a, closeTo(.8, .01));

    await tester.pumpWidget(nativeTileScene(VisualStyle.neumorphism));
    expect(
      tileThemeShape(tester).insetStrength,
      closeTo(leaving.insetStrength, .001),
    );
    await tester.pump(const Duration(milliseconds: 65));
    expect(
      tileThemeShape(tester).insetStrength,
      inExclusiveRange(leaving.insetStrength, 1),
    );
    await tester.pumpAndSettle();
    expect(tileThemeShape(tester).insetStrength, 1);
    expect(tester.element(find.byType(Checkbox)), same(native));

    await tester.pumpWidget(nativeTileScene(VisualStyle.flat));
    await tester.pump(const Duration(milliseconds: 65));
    expect(tileThemeShape(tester).insetStrength, inExclusiveRange(0, 1));
    await tester.pumpAndSettle();
    expect(
      Theme.of(tester.element(find.byType(Checkbox))).checkboxTheme.shape,
      isNull,
    );
    await tester.pumpWidget(nativeTileScene(VisualStyle.neumorphism));
    await tester.pump(const Duration(milliseconds: 65));
    expect(tileThemeShape(tester).insetStrength, inExclusiveRange(0, 1));
    await tester.pumpAndSettle();
    expect(tileThemeShape(tester).insetStrength, 1);
  });

  testWidgets('ordinary CheckboxListTile interpolates light and dark colors', (
    tester,
  ) async {
    await tester.pumpWidget(nativeTileScene(VisualStyle.neumorphism));
    await tester.pumpAndSettle();
    await tester.pumpWidget(
      nativeTileScene(VisualStyle.neumorphism, mode: StudioTheme.dark),
    );
    await tester.pump(const Duration(milliseconds: 65));
    final middle = tileThemeShape(tester);
    expect(middle.insetStrength, 1);
    expect(middle.insetShade.a, inExclusiveRange(.24, .5));
    expect(middle.insetLight.a, inExclusiveRange(.16, .8));
    await tester.pumpAndSettle();
    expect(tileThemeShape(tester).insetShade.a, closeTo(.5, .001));
    expect(tileThemeShape(tester).insetLight.a, closeTo(.16, .001));
  });

  testWidgets('depth, lighting and disabled alpha interpolate', (tester) async {
    await tester.pumpWidget(scene(VisualStyle.neumorphism, onChanged: (_) {}));
    await tester.pumpAndSettle();
    final start = border(tester);
    await tester.pumpWidget(
      scene(VisualStyle.neumorphism, depth: .3, onChanged: null),
    );
    await tester.pump(const Duration(milliseconds: 60));
    final mid = border(tester);
    expect(mid.recess, inExclusiveRange(.3, start.recess));
    expect(mid.shift, inExclusiveRange(.45, start.shift));
    expect(mid.shade.a, lessThan(start.shade.a));
    await tester.pumpAndSettle();
    expect(border(tester).recess, closeTo(.3, .001));
  });

  testWidgets('custom theme shape survives the inset and has native side', (
    tester,
  ) async {
    const custom = RoundedRectangleBorder(
      borderRadius: BorderRadius.all(Radius.circular(8)),
    );
    await tester.pumpWidget(
      scene(VisualStyle.neumorphism, customShape: custom, onChanged: (_) {}),
    );
    expect(border(tester).outline, custom);
    expect(border(tester).recess, 1);
    expect(tester.widget<Checkbox>(find.byType(Checkbox)).isError, isTrue);
    await tester.pumpWidget(scene(VisualStyle.neumorphism, onChanged: (_) {}));
    await tester.pumpAndSettle();
    await tester.pumpWidget(
      scene(VisualStyle.neumorphism, customShape: custom, onChanged: (_) {}),
    );
    await tester.pumpAndSettle();
    expect(border(tester).outline, custom);
  });

  testWidgets(
    'native tristate, disabled and semantics persist through styles',
    (tester) async {
      final semantics = tester.ensureSemantics();
      final focus = FocusNode();
      bool? value;
      int changes = 0;
      late StateSetter update;
      var style = VisualStyle.neumorphism;
      var enabled = true;
      await tester.pumpWidget(
        StatefulBuilder(
          builder: (context, setState) {
            update = setState;
            return scene(
              style,
              value: value,
              tristate: true,
              focusNode: focus,
              onChanged: enabled
                  ? (next) => setState(() {
                      value = next;
                      changes++;
                    })
                  : null,
            );
          },
        ),
      );
      final native = tester.element(find.byType(Checkbox));
      expect(tester.widget<Checkbox>(find.byType(Checkbox)).tristate, isTrue);
      expect(tester.widget<Checkbox>(find.byType(Checkbox)).isError, isTrue);
      await tester.tap(find.byType(Checkbox));
      await tester.pumpAndSettle();
      expect(value, isFalse);
      focus.requestFocus();
      await tester.pumpAndSettle();
      for (final next in VisualStyle.values) {
        update(() => style = next);
        await tester.pumpAndSettle();
        expect(tester.element(find.byType(Checkbox)), same(native));
        expect(focus.hasFocus, isTrue);
      }
      update(() => enabled = false);
      await tester.pumpAndSettle();
      await tester.tap(find.byType(Checkbox));
      await tester.pumpAndSettle();
      expect(changes, 1);
      expect(
        tester.getSemantics(find.byType(Checkbox)),
        matchesSemantics(
          label: 'Choice',
          hasCheckedState: true,
          isChecked: false,
          hasEnabledState: true,
          isEnabled: false,
        ),
      );
      semantics.dispose();
      await tester.pumpWidget(const SizedBox());
      focus.dispose();
    },
  );

  testWidgets('reduced, hidden, high contrast and adaptive Apple snap', (
    tester,
  ) async {
    await tester.pumpWidget(
      scene(VisualStyle.neumorphism, reduced: true, onChanged: (_) {}),
    );
    await tester.pumpWidget(
      scene(VisualStyle.paper, reduced: true, onChanged: (_) {}),
    );
    expect(border(tester).recess, 0);

    await tester.pumpWidget(
      scene(VisualStyle.neumorphism, visible: false, onChanged: (_) {}),
    );
    expect(border(tester).recess, 1);
    await tester.pumpWidget(
      scene(VisualStyle.clay, visible: false, onChanged: (_) {}),
    );
    expect(border(tester).recess, 0);

    await tester.pumpWidget(
      scene(VisualStyle.neumorphism, contrast: true, onChanged: (_) {}),
    );
    expect(border(tester).recess, 0);
    await tester.pumpWidget(
      scene(
        VisualStyle.neumorphism,
        adaptive: true,
        platform: TargetPlatform.iOS,
        onChanged: (_) {},
      ),
    );
    expect(tester.widget<Checkbox>(find.byType(Checkbox)).shape, isNull);
    expect(tester.takeException(), isNull);
  });
}
