import 'dart:ui' as ui;
import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:flutter_acrylic/flutter_acrylic.dart' as acrylic;
import 'package:morrow_studio/main.dart';
import 'package:morrow_studio/liquid_glass.dart';
import 'package:morrow_studio/window_effects.dart';

void main() {
  testWidgets(
    'Transparent liquid canvas preserves zero interior alpha in both themes and high contrast',
    (tester) async {
      final key = GlobalKey();
      for (final dark in [false, true]) {
        for (final contrast in [false, true]) {
          await tester.pumpWidget(
            MaterialApp(
              home: Scaffold(
                body: Center(
                  child: MediaQuery(
                    data: MediaQueryData(highContrast: contrast),
                    child: RepaintBoundary(
                      key: key,
                      child: SizedBox(
                        width: 240,
                        height: 160,
                        child: LiquidGlassSurface(
                          canvas: true,
                          dark: dark,
                          tint: dark ? Colors.black : Colors.white,
                          borderRadius: BorderRadius.circular(20),
                          child: const SizedBox.expand(),
                        ),
                      ),
                    ),
                  ),
                ),
              ),
            ),
          );
          await tester.pumpAndSettle();
          final finder = find.byType(LiquidGlassSurface);
          expect(
            find.descendant(of: finder, matching: find.byType(BackdropFilter)),
            findsNothing,
          );
          final pixels = await tester.runAsync(() async {
            final boundary =
                key.currentContext!.findRenderObject()!
                    as RenderRepaintBoundary;
            final image = await boundary.toImage();
            final bytes = await image.toByteData(
              format: ui.ImageByteFormat.rawRgba,
            );
            image.dispose();
            return bytes!;
          });
          int alpha(int x, int y) => pixels!.getUint8((y * 240 + x) * 4 + 3);
          expect(alpha(120, 80), 0);
          expect(alpha(30, 30), 0);
          expect(alpha(210, 130), 0);
          expect(
            alpha(120, 1),
            greaterThan(0),
            reason: 'Glass rim remains visible',
          );
          expect(tester.takeException(), isNull);
        }
      }
    },
  );
  testWidgets(
    'Toggling canvas liquid effect never requests native Aero or opaque fill',
    (tester) async {
      tester.view.physicalSize = const Size(1440, 1000);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      const channel = MethodChannel('com.alexmercerind/flutter_acrylic');
      final calls = <MethodCall>[];
      tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(channel, (
        call,
      ) async {
        calls.add(call);
        return null;
      });
      addTearDown(
        () => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
          channel,
          null,
        ),
      );
      await tester.pumpWidget(MorrowApp(nativeBackground: DesktopBackground()));
      await tester.pumpAndSettle();
      final transparent = find.byKey(const ValueKey('background-transparent'));
      await tester.ensureVisible(transparent);
      await tester.tap(transparent);
      await tester.pumpAndSettle();
      for (final enabled in [true, false, true]) {
        final toggle = find.byKey(const ValueKey('canvas-liquid-toggle'));
        await tester.ensureVisible(toggle);
        await tester.tap(toggle);
        await tester.pumpAndSettle();
        expect(
          tester.widget<Studio>(find.byType(Studio)).palette.liquidCanvas,
          enabled,
        );
        expect(
          find.byKey(const ValueKey('liquid-canvas')),
          enabled ? findsOneWidget : findsNothing,
        );
      }
      final effects = calls
          .where((call) => call.method == 'SetEffect')
          .toList();
      expect(effects, isNotEmpty);
      for (final effect in effects) {
        expect(
          (effect.arguments as Map)['effect'],
          acrylic.WindowEffect.transparent.index,
        );
        expect(((effect.arguments as Map)['color'] as Map)['A'], 0);
      }
      expect(tester.takeException(), isNull);
    },
    variant: TargetPlatformVariant.only(TargetPlatform.windows),
  );
}
