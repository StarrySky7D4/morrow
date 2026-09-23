import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/liquid_glass.dart';

GlassMaterial _liquid() => GlassMaterial.liquid(
  tint: Colors.white,
  dark: false,
  readable: false,
  borderRadius: BorderRadius.circular(20),
);

Widget _surface({
  required Widget child,
  bool canvas = false,
  bool transparentCanvas = true,
  bool reduceMotion = false,
  bool tickerEnabled = true,
  GlassMaterial? material,
}) => MaterialApp(
  home: MediaQuery(
    data: MediaQueryData(disableAnimations: reduceMotion),
    child: Center(
      child: SizedBox(
        width: 240,
        height: 120,
        child: TickerMode(
          enabled: tickerEnabled,
          child: LiquidGlassSurface(
            tint: Colors.white,
            dark: false,
            borderRadius: BorderRadius.circular(20),
            canvas: canvas,
            transparentCanvas: transparentCanvas,
            material: material ?? _liquid(),
            child: child,
          ),
        ),
      ),
    ),
  ),
);

LiquidRimPainter _rim(WidgetTester tester) => tester
    .widgetList<CustomPaint>(find.byType(CustomPaint))
    .map((paint) => paint.painter)
    .whereType<LiquidRimPainter>()
    .single;

void main() {
  testWidgets('transparent canvas leaves its interior free of backdrop paint', (
    tester,
  ) async {
    await tester.pumpWidget(
      _surface(canvas: true, child: const Text('Canvas content')),
    );
    expect(find.byType(BackdropFilter), findsNothing);
    expect(find.text('Canvas content'), findsOneWidget);
    expect(_rim(tester).intensity, 1);
    expect(tester.takeException(), isNull);
  });

  testWidgets('hover updates optics without rebuilding the surface or child', (
    tester,
  ) async {
    var childBuilds = 0;
    await tester.pumpWidget(
      _surface(
        child: Builder(
          builder: (_) {
            childBuilds++;
            return const SizedBox.expand();
          },
        ),
      ),
    );
    final surface = tester.widget<LiquidGlassSurface>(
      find.byType(LiquidGlassSurface),
    );
    final startLight = _rim(tester).light;
    final bounds = tester.getRect(find.byType(LiquidGlassSurface));
    final mouse = await tester.createGesture(kind: ui.PointerDeviceKind.mouse);
    await mouse.addPointer(location: bounds.topLeft + const Offset(15, 15));
    await mouse.moveTo(bounds.bottomRight - const Offset(15, 15));
    await tester.pump();

    expect(_rim(tester).light, isNot(startLight));
    expect(
      tester.widget<LiquidGlassSurface>(find.byType(LiquidGlassSurface)),
      same(surface),
    );
    expect(childBuilds, 1);
    expect(find.byType(BackdropFilter), findsOneWidget);

    await mouse.removePointer();
    await tester.pump();
    expect(_rim(tester).light, const Offset(-.65, -.8));
    expect(childBuilds, 1);
    expect(tester.takeException(), isNull);
  });

  testWidgets('press feedback settles and keeps child taps and cancellation', (
    tester,
  ) async {
    var taps = 0;
    await tester.pumpWidget(
      _surface(
        child: GestureDetector(
          key: const ValueKey('target'),
          behavior: HitTestBehavior.opaque,
          onTap: () => taps++,
          child: const SizedBox.expand(),
        ),
      ),
    );
    final center = tester.getCenter(find.byKey(const ValueKey('target')));
    final gesture = await tester.startGesture(center);
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 90));
    expect(_rim(tester).press, greaterThan(0));
    await gesture.up();
    await tester.pumpAndSettle();
    expect(taps, 1);
    expect(_rim(tester).press, 0);

    final cancelled = await tester.startGesture(center);
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 50));
    expect(_rim(tester).press, greaterThan(0));
    await cancelled.cancel();
    await tester.pumpAndSettle();
    expect(taps, 1);
    expect(_rim(tester).press, 0);
    expect(tester.takeException(), isNull);
  });

  testWidgets('reduced motion does not start press animation', (tester) async {
    await tester.pumpWidget(
      _surface(reduceMotion: true, child: const SizedBox.expand()),
    );
    final gesture = await tester.startGesture(
      tester.getCenter(find.byType(LiquidGlassSurface)),
    );
    await tester.pump(const Duration(milliseconds: 90));
    expect(_rim(tester).press, 0);
    await gesture.cancel();
    expect(tester.takeException(), isNull);
  });

  testWidgets(
    'running press resets across motion, visibility, and mode changes',
    (tester) async {
      var reduced = false;
      var visible = true;
      var liquid = true;
      late StateSetter update;
      final ordinary = GlassMaterial(
        blur: 22,
        liquid: 0,
        decoration: BoxDecoration(
          borderRadius: BorderRadius.circular(20),
          color: Colors.white,
        ),
      );
      await tester.pumpWidget(
        StatefulBuilder(
          builder: (_, setState) {
            update = setState;
            return _surface(
              reduceMotion: reduced,
              tickerEnabled: visible,
              material: liquid ? _liquid() : ordinary,
              child: const SizedBox.expand(),
            );
          },
        ),
      );
      final center = tester.getCenter(find.byType(LiquidGlassSurface));
      final originalState = tester.state(find.byType(LiquidGlassSurface));

      Future<TestGesture> beginPress() async {
        final gesture = await tester.startGesture(center);
        await tester.pump();
        await tester.pump(const Duration(milliseconds: 60));
        expect(_rim(tester).press, greaterThan(0));
        return gesture;
      }

      var gesture = await beginPress();
      update(() => reduced = true);
      await tester.pump();
      expect(_rim(tester).press, 0);
      await gesture.cancel();
      update(() => reduced = false);
      await tester.pump();

      gesture = await beginPress();
      update(() => visible = false);
      await tester.pump();
      expect(_rim(tester).press, 0);
      await gesture.cancel();
      update(() => visible = true);
      await tester.pump();

      gesture = await beginPress();
      update(() => liquid = false);
      await tester.pump();
      expect(_rim(tester).press, 0);
      await gesture.cancel();
      update(() => liquid = true);
      await tester.pump();
      expect(_rim(tester).press, 0);
      expect(
        tester.state(find.byType(LiquidGlassSurface)),
        same(originalState),
      );
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets('secondary mouse press leaves the glass at rest', (tester) async {
    await tester.pumpWidget(_surface(child: const SizedBox.expand()));
    final gesture = await tester.startGesture(
      tester.getCenter(find.byType(LiquidGlassSurface)),
      kind: ui.PointerDeviceKind.mouse,
      buttons: kSecondaryButton,
    );
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 90));
    expect(_rim(tester).press, 0);
    await gesture.up();
    expect(tester.takeException(), isNull);
  });
  testWidgets('ordinary material stays optically static on hover', (
    tester,
  ) async {
    await tester.pumpWidget(
      _surface(
        material: GlassMaterial(
          blur: 22,
          liquid: 0,
          decoration: BoxDecoration(
            borderRadius: BorderRadius.circular(20),
            color: Colors.white,
          ),
        ),
        child: const SizedBox.expand(),
      ),
    );
    final bounds = tester.getRect(find.byType(LiquidGlassSurface));
    final mouse = await tester.createGesture(kind: ui.PointerDeviceKind.mouse);
    await mouse.addPointer(location: bounds.center);
    await mouse.moveTo(bounds.center + const Offset(20, 10));
    await tester.pump();
    expect(_rim(tester).intensity, 0);
    expect(_rim(tester).press, 0);
    await mouse.removePointer();
    expect(tester.takeException(), isNull);
  });
}
