import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/appearance.dart';
import 'package:morrow_studio/experimental_controls.dart';
import 'package:morrow_studio/neumorphic_controls.dart';

void main() {
  const flat = Palette(StudioTheme.white, GlassMode.frosted);
  Palette styled(VisualStyle style) =>
      flat.withSurfaces(SurfaceSettings(visualStyle: style));

  test('dispatcher keeps flat identity and routes the legacy style', () {
    final base = ThemeData(useMaterial3: true);
    expect(identical(applyVisualStyleControls(base, flat), base), isTrue);
    final neumo = styled(VisualStyle.neumorphism);
    expect(
      applyVisualStyleControls(base, neumo).inputDecorationTheme.enabledBorder,
      isA<NeumorphicInputBorder>(),
    );
    for (final style in VisualStyle.values.where(
      (value) => value.experimental,
    )) {
      final theme = applyVisualStyleControls(base, styled(style));
      expect(
        theme.inputDecorationTheme.enabledBorder,
        isA<OutlineInputBorder>(),
      );
      if (style == VisualStyle.clay || style == VisualStyle.industrial) {
        expect(
          theme.inputDecorationTheme.enabledBorder,
          isA<ExperimentalInputBorder>(),
        );
      }
      expect(theme.filledButtonTheme.style?.backgroundBuilder, isNotNull);
      expect(theme.sliderTheme.trackHeight, greaterThan(0));
    }
  });

  test('recessed input borders keep explicit enabled state', () {
    final base = ThemeData(useMaterial3: true);
    for (final style in [VisualStyle.clay, VisualStyle.industrial]) {
      final input = applyVisualStyleControls(
        base,
        styled(style),
      ).inputDecorationTheme;
      final enabled = input.enabledBorder! as ExperimentalInputBorder;
      final focused = input.focusedBorder! as ExperimentalInputBorder;
      final disabled = input.disabledBorder! as ExperimentalInputBorder;
      expect(enabled.enabled, isTrue);
      expect(focused.enabled, isTrue);
      expect(disabled.enabled, isFalse);
      expect(disabled.borderSide.color.a, lessThan(enabled.borderSide.color.a));
      expect(enabled.copyWith().enabled, isTrue);
      expect(disabled.copyWith().enabled, isFalse);
      expect(disabled.scale(.5).enabled, isFalse);
      expect(
        (ShapeBorder.lerp(enabled, disabled, .75)! as ExperimentalInputBorder)
            .enabled,
        isFalse,
      );
      expect(
        disabled,
        isNot(
          ExperimentalInputBorder(
            palette: disabled.palette,
            style: style,
            borderSide: disabled.borderSide,
            borderRadius: disabled.borderRadius,
            enabled: true,
          ),
        ),
      );
    }
  });
  testWidgets('experimental surfaces keep the child element across styles', (
    tester,
  ) async {
    final key = GlobalKey();
    Widget build(VisualStyle style, double depth, {bool enabled = true}) =>
        MaterialApp(
          home: Scaffold(
            body: NeumorphicSurface(
              palette: styled(style),
              depth: depth,
              enabled: enabled,
              child: TextField(key: key),
            ),
          ),
        );

    await tester.pumpWidget(build(VisualStyle.flat, 1));
    final original = key.currentState;
    for (final style in VisualStyle.values.where(
      (value) => value.experimental,
    )) {
      await tester.pumpWidget(build(style, 1));
      await tester.pumpAndSettle();
      expect(key.currentState, same(original));
      final painter = tester
          .widgetList<CustomPaint>(find.byType(CustomPaint))
          .map((paint) => paint.painter)
          .whereType<ExperimentalSurfacePainter>()
          .single;
      expect(painter.style, style);
      expect(painter.fill, isFalse);
      expect(painter.enabled, isTrue);
      await tester.pumpWidget(build(style, -1, enabled: false));
      await tester.pumpAndSettle();
      final inset = tester
          .widgetList<CustomPaint>(find.byType(CustomPaint))
          .map((paint) => paint.foregroundPainter)
          .whereType<ExperimentalSurfacePainter>()
          .single;
      expect(inset.depth, -1);
      expect(inset.enabled, isFalse);
      expect(key.currentState, same(original));
    }
  });

  testWidgets('native controls retain callbacks and disabled state', (
    tester,
  ) async {
    for (final style in VisualStyle.values.where(
      (value) => value.experimental,
    )) {
      var switched = false;
      var checked = false;
      var pressed = 0;
      final palette = styled(style);
      await tester.pumpWidget(
        MaterialApp(
          theme: applyVisualStyleControls(
            ThemeData(useMaterial3: true),
            palette,
          ),
          home: StatefulBuilder(
            builder: (context, setState) => Scaffold(
              body: Column(
                children: [
                  FilledButton(
                    onPressed: () => pressed++,
                    child: const Text('Press'),
                  ),
                  FilledButton(onPressed: null, child: const Text('Disabled')),
                  Switch(
                    value: switched,
                    onChanged: (next) => setState(() => switched = next),
                  ),
                  Checkbox(
                    value: checked,
                    onChanged: (next) => setState(() => checked = next!),
                  ),
                ],
              ),
            ),
          ),
        ),
      );
      await tester.tap(find.text('Press'));
      await tester.tap(find.text('Disabled'));
      await tester.tap(find.byType(Switch));
      await tester.tap(find.byType(Checkbox));
      expect(pressed, 1, reason: style.name);
      expect(switched, isTrue, reason: style.name);
      expect(checked, isTrue, reason: style.name);
      expect(
        tester
            .widget<FilledButton>(find.widgetWithText(FilledButton, 'Disabled'))
            .onPressed,
        isNull,
      );
      expect(tester.takeException(), isNull);
    }
  });
}
