import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/appearance.dart';
import 'package:morrow_studio/experimental_controls.dart';
import 'package:morrow_studio/neumorphic_controls.dart';

void main() {
  const base = Palette(StudioTheme.white, GlassMode.frosted);
  final childKey = GlobalKey();

  Widget scene(
    VisualStyle style, {
    double depth = 1,
    double radius = 12,
    Color color = const Color(0xFFF7F2E9),
    bool reduceMotion = false,
    bool dark = false,
    bool tickerEnabled = true,
    FocusNode? focusNode,
  }) {
    final palette =
        (dark ? const Palette(StudioTheme.dark, GlassMode.frosted) : base)
            .withSurfaces(SurfaceSettings(visualStyle: style));
    return MaterialApp(
      home: MediaQuery(
        data: MediaQueryData(disableAnimations: reduceMotion),
        child: TickerMode(
          enabled: tickerEnabled,
          child: Scaffold(
            body: SizedBox(
              width: 240,
              height: 70,
              child: NeumorphicSurface(
                palette: palette,
                depth: depth,
                borderRadius: BorderRadius.circular(radius),
                color: color,
                child: TextField(key: childKey, focusNode: focusNode),
              ),
            ),
          ),
        ),
      ),
    );
  }

  CustomPaint surfacePaint(WidgetTester tester) => tester.widget<CustomPaint>(
    find
        .ancestor(of: find.byKey(childKey), matching: find.byType(CustomPaint))
        .first,
  );

  testWidgets(
    'style interruption continues from displayed weights and radius',
    (tester) async {
      final focus = FocusNode();
      addTearDown(focus.dispose);
      await tester.pumpWidget(
        scene(VisualStyle.neumorphism, radius: 8, focusNode: focus),
      );
      await tester.enterText(find.byKey(childKey), 'draft');
      await tester.pump();
      expect(focus.hasFocus, isTrue);
      final original = childKey.currentState;
      expect(surfacePaint(tester).painter, isA<NeumorphicSurfacePainter>());

      await tester.pumpWidget(
        scene(VisualStyle.paper, radius: 16, focusNode: focus),
      );
      await tester.pump(const Duration(milliseconds: 40));
      final first = surfacePaint(tester).painter! as SurfaceStyleBlendPainter;
      expect(first.styleWeights.length, VisualStyle.values.length);
      expect(first.styleWeights[VisualStyle.neumorphism.index], greaterThan(0));
      expect(first.styleWeights[VisualStyle.paper.index], greaterThan(0));
      expect(first.radius.topLeft.x, inInclusiveRange(8, 16));
      final firstWeights = first.styleWeights;
      final firstRadius = first.radius;

      await tester.pumpWidget(
        scene(
          VisualStyle.clay,
          radius: 24,
          color: const Color(0xFFE0D5CA),
          focusNode: focus,
        ),
      );
      final interrupted =
          surfacePaint(tester).painter! as SurfaceStyleBlendPainter;
      for (var i = 0; i < VisualStyle.values.length; i++) {
        expect(interrupted.styleWeights[i], closeTo(firstWeights[i], .001));
      }
      expect(interrupted.radius, firstRadius);
      expect(interrupted.styleWeights[VisualStyle.clay.index], 0);
      expect(childKey.currentState, same(original));
      expect(focus.hasFocus, isTrue);
      expect(find.text('draft'), findsOneWidget);

      await tester.pump(const Duration(milliseconds: 45));
      final resumed = surfacePaint(tester).painter! as SurfaceStyleBlendPainter;
      expect(resumed.styleWeights[VisualStyle.clay.index], greaterThan(0));
      expect(
        resumed.styleWeights[VisualStyle.neumorphism.index],
        greaterThan(0),
      );
      expect(resumed.styleWeights[VisualStyle.paper.index], greaterThan(0));
      expect(resumed.radius.topLeft.x, greaterThan(firstRadius.topLeft.x));

      await tester.pumpAndSettle();
      final settled =
          surfacePaint(tester).painter! as ExperimentalSurfacePainter;
      expect(settled.style, VisualStyle.clay);
      expect(settled.radius.topLeft.x, 24);
      expect(childKey.currentState, same(original));
      expect(focus.hasFocus, isTrue);
      expect(find.text('draft'), findsOneWidget);
      expect(find.byType(TextField), findsOneWidget);
    },
  );

  testWidgets('theme reversal retains displayed light and dark relief', (
    tester,
  ) async {
    for (final style in VisualStyle.values.where(
      (s) => s != VisualStyle.flat,
    )) {
      await tester.pumpWidget(scene(style));
      await tester.pumpAndSettle();
      final original = childKey.currentState;
      await tester.pumpWidget(scene(style, dark: true));
      await tester.pump(const Duration(milliseconds: 30));
      final before = surfacePaint(tester).painter! as SurfaceStyleBlendPainter;
      expect(before.darkness, greaterThan(0));
      expect(before.darkness, lessThan(1));
      await tester.pumpWidget(scene(style));
      final reversed =
          surfacePaint(tester).painter! as SurfaceStyleBlendPainter;
      expect(reversed.darkness, closeTo(before.darkness, .00001));
      await tester.pump(const Duration(milliseconds: 20));
      final next = surfacePaint(tester).painter! as SurfaceStyleBlendPainter;
      expect(next.darkness, lessThan(before.darkness));
      expect(childKey.currentState, same(original));
      await tester.pumpAndSettle();
      final settled = surfacePaint(tester).painter!;
      expect(
        settled is NeumorphicSurfacePainter
            ? settled.dark
            : (settled as ExperimentalSurfacePainter).dark,
        isFalse,
      );
    }
  });

  testWidgets('flat fades and reduced or hidden motion settles immediately', (
    tester,
  ) async {
    await tester.pumpWidget(scene(VisualStyle.industrial));
    await tester.pumpWidget(scene(VisualStyle.flat));
    await tester.pump(const Duration(milliseconds: 45));
    final blend = surfacePaint(tester).painter! as SurfaceStyleBlendPainter;
    expect(blend.styleWeights[VisualStyle.industrial.index], greaterThan(0));
    expect(blend.styleWeights[VisualStyle.flat.index], greaterThan(0));
    await tester.pumpAndSettle();
    expect(surfacePaint(tester).painter, isNull);

    await tester.pumpWidget(scene(VisualStyle.neumorphism, reduceMotion: true));
    expect(surfacePaint(tester).painter, isA<NeumorphicSurfacePainter>());
    await tester.pumpWidget(scene(VisualStyle.brutalist, reduceMotion: true));
    expect(surfacePaint(tester).painter, isA<ExperimentalSurfacePainter>());

    await tester.pumpWidget(scene(VisualStyle.paper, tickerEnabled: false));
    expect(surfacePaint(tester).painter, isA<ExperimentalSurfacePainter>());

    await tester.pumpWidget(scene(VisualStyle.clay, depth: 0));
    expect(surfacePaint(tester).painter, isNull);
    await tester.pumpWidget(scene(VisualStyle.fluent, depth: 0));
    expect(surfacePaint(tester).painter, isNull);
    await tester.pump();
    expect(tester.binding.hasScheduledFrame, isFalse);
  });
}
