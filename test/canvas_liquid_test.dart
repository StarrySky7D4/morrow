import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/main.dart';
import 'package:morrow_studio/liquid_glass.dart';
import 'package:morrow_studio/storage.dart';

void main() {
  testWidgets(
    'Canvas liquid switch works independently for all four backgrounds on desktop and mobile',
    (tester) async {
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      for (final width in [1440.0, 390.0]) {
        tester.view.physicalSize = Size(width, 1000);
        final storage = MemoryStorage();
        await tester.pumpWidget(MorrowApp(storage: storage));
        await tester.pumpAndSettle();
        if (width < 1050) {
          await tester.tap(find.byKey(const ValueKey('appearance-toggle')));
          await tester.pumpAndSettle();
        }
        final toggle = find.byKey(const ValueKey('canvas-liquid-toggle'));
        final layer = find.byKey(const ValueKey('liquid-canvas'));
        Palette palette() => tester.widget<Studio>(find.byType(Studio)).palette;
        expect(palette().backdrop, BackgroundMode.ambient);
        expect(toggle, findsOneWidget);
        await tester.ensureVisible(toggle);
        await tester.tap(toggle);
        await tester.pumpAndSettle();
        expect(palette().liquidCanvas, isTrue);
        for (final mode in BackgroundMode.values) {
          final option = find.byKey(ValueKey('background-${mode.name}'));
          await tester.ensureVisible(option);
          await tester.tap(option);
          await tester.pump();
          await tester.pump(const Duration(milliseconds: 100));
          expect(toggle, findsOneWidget);
          await tester.pumpAndSettle();
          expect(palette().backdrop, mode);
          expect(palette().liquidCanvas, isTrue);
          expect(storage.data!['liquidCanvas'], isTrue);
          expect(layer, findsOneWidget);
          final surface = tester.widget<LiquidGlassSurface>(layer);
          expect(surface.canvas, isTrue);
          expect(surface.transparentCanvas, mode == BackgroundMode.transparent);
          expect(
            find.descendant(of: layer, matching: find.byType(BackdropFilter)),
            mode == BackgroundMode.transparent ? findsNothing : findsOneWidget,
          );
          expect(tester.takeException(), isNull);
        }
        final texture = find.byKey(const ValueKey('background-texture'));
        await tester.ensureVisible(texture);
        await tester.tap(texture);
        await tester.pumpAndSettle();
        final clear = find.byKey(const ValueKey('mode-clear'));
        await tester.ensureVisible(clear);
        await tester.tap(clear);
        await tester.pumpAndSettle();
        expect(palette().mode, GlassMode.clear);
        expect(palette().liquidCanvas, isTrue);
        await tester.pumpWidget(const SizedBox());
        await tester.pumpWidget(MorrowApp(storage: storage));
        await tester.pumpAndSettle();
        if (width < 1050) {
          await tester.tap(find.byKey(const ValueKey('appearance-toggle')));
          await tester.pumpAndSettle();
        }
        expect(palette().backdrop, BackgroundMode.texture);
        expect(palette().mode, GlassMode.clear);
        expect(palette().liquidCanvas, isTrue);
        expect(layer, findsOneWidget);
        await tester.ensureVisible(toggle);
        await tester.tap(toggle);
        await tester.pumpAndSettle();
        expect(palette().backdrop, BackgroundMode.texture);
        expect(palette().mode, GlassMode.clear);
        expect(palette().liquidCanvas, isFalse);
        expect(layer, findsNothing);
        expect(storage.data!['liquidCanvas'], isFalse);
        expect(tester.takeException(), isNull);
        await tester.pumpWidget(const SizedBox());
      }
    },
  );
}
