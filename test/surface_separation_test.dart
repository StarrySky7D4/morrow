import 'dart:io';
import 'dart:ui' as ui;

import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/appearance.dart';
import 'package:morrow_studio/neumorphic_controls.dart';
import 'package:morrow_studio/surface_motion.dart';

void main() {
  for (final theme in [StudioTheme.white, StudioTheme.dark]) {
    testWidgets('adjacent relief stays separated in ${theme.name}', (t) async {
      t.view.devicePixelRatio = 1;
      t.view.physicalSize = const Size(400, 850);
      addTearDown(t.view.reset);
      const background = Color(0xff70a0b0);
      const capture = ValueKey('separation-capture');
      for (final mode in GlassMode.values) {
        await t.pumpWidget(
          MaterialApp(
            home: RepaintBoundary(
              key: capture,
              child: ColoredBox(
                color: background,
                child: Column(
                  children: [
                    for (final style in VisualStyle.values)
                      Padding(
                        padding: const EdgeInsets.only(left: 40, top: 20),
                        child: Row(
                          children: [
                            for (var i = 0; i < 2; i++) ...[
                              SurfaceInteraction(
                                child: Glass(
                                  p: Palette(theme, mode).withSurfaces(
                                    SurfaceSettings(
                                      visualStyle: style,
                                      styleDepth: 2,
                                    ),
                                  ),
                                  child: const SizedBox(width: 120, height: 60),
                                ),
                              ),
                              if (i == 0) const SizedBox(width: 14),
                            ],
                          ],
                        ),
                      ),
                  ],
                ),
              ),
            ),
          ),
        );
        await t.pumpAndSettle();
        final mouse = await t.createGesture(kind: PointerDeviceKind.mouse);
        await mouse.addPointer(location: const Offset(80, 45));
        await t.pumpAndSettle();
        await t.runAsync(() async {
          final image = await t
              .renderObject<RenderRepaintBoundary>(find.byKey(capture))
              .toImage();
          final bytes = (await image.toByteData(
            format: ui.ImageByteFormat.rawRgba,
          ))!.buffer.asUint8List();
          for (var y = 0; y < image.height; y++) {
            // 120px face + 6px paint budget leaves a 2px clear corridor.
            for (final x in [166, 167]) {
              final offset = (y * image.width + x) * 4;
              expect(
                bytes.sublist(offset, offset + 4),
                [112, 160, 176, 255],
                reason: '$theme/$mode shadow crossed the gutter at $x,$y',
              );
            }
          }
          if (mode == GlassMode.clear) {
            final png = await image.toByteData(format: ui.ImageByteFormat.png);
            final file = File(
              'build/ui-style-preview/separation-${theme.name}.png',
            );
            await file.parent.create(recursive: true);
            await file.writeAsBytes(png!.buffer.asUint8List());
          }
          image.dispose();
        });
        await mouse.removePointer();
        expect(t.takeException(), isNull);
      }
    });
  }

  testWidgets(
    'style changes retain controls and hover cannot widen a surface',
    (t) async {
      final controller = TextEditingController(text: 'retained');
      addTearDown(controller.dispose);
      Object? original;
      for (final style in VisualStyle.values) {
        await t.pumpWidget(
          MaterialApp(
            home: Scaffold(
              body: Align(
                alignment: Alignment.topLeft,
                child: SizedBox(
                  width: 760,
                  child: SurfaceInteraction(
                    child: NeumorphicSurface(
                      palette: const Palette(StudioTheme.white, GlassMode.clear)
                          .withSurfaces(
                            SurfaceSettings(visualStyle: style, styleDepth: 2),
                          ),
                      child: TextField(controller: controller),
                    ),
                  ),
                ),
              ),
            ),
          ),
        );
        await t.pumpAndSettle();
        final field = t.state(find.byType(TextField));
        original ??= field;
        expect(field, same(original));
        final mouse = await t.createGesture(kind: PointerDeviceKind.mouse);
        await mouse.addPointer(location: const Offset(200, 20));
        await t.pumpAndSettle();
        final transform = t
            .widget<AnimatedContainer>(
              find
                  .descendant(
                    of: find.byType(SurfaceInteraction),
                    matching: find.byType(AnimatedContainer),
                  )
                  .first,
            )
            .transform!;
        expect(transform.storage[0], 1);
        expect(transform.storage[5], 1);
        expect(transform.storage[13], -2);
        await mouse.removePointer();
        expect(controller.text, 'retained');
        expect(t.takeException(), isNull);
      }
    },
  );
}
