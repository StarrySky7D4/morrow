import 'dart:typed_data';
import 'dart:ui' as ui;
import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/appearance.dart';
import 'package:morrow_studio/experimental_controls.dart';
import 'package:morrow_studio/neumorphic_controls.dart';

Future<Uint8List> render(CustomPainter painter) async {
  final recorder = ui.PictureRecorder();
  painter.paint(Canvas(recorder)..translate(16, 16), const Size(160, 64));
  final picture = recorder.endRecording();
  final image = await picture.toImage(192, 96);
  final bytes = await image.toByteData(format: ui.ImageByteFormat.rawRgba);
  final result = Uint8List.fromList(bytes!.buffer.asUint8List());
  image.dispose();
  picture.dispose();
  return result;
}

int alpha(Uint8List bytes) {
  var result = 0;
  for (var i = 3; i < bytes.length; i += 4) {
    result += bytes[i];
  }
  return result;
}

CustomPainter relief(
  VisualStyle style,
  double depth, {
  bool dark = false,
  bool contrast = false,
  bool focused = false,
  bool fill = false,
}) => style == VisualStyle.neumorphism
    ? NeumorphicSurfacePainter(
        depth: depth,
        radius: BorderRadius.circular(12),
        surface: Colors.grey,
        dark: dark,
        enabled: true,
        highContrast: contrast,
        fill: fill,
        focused: focused,
        focusColor: Colors.blue,
      )
    : ExperimentalSurfacePainter(
        style: style,
        depth: depth,
        radius: BorderRadius.circular(12),
        surface: Colors.grey,
        dark: dark,
        enabled: true,
        highContrast: contrast,
        fill: fill,
        focused: focused,
        focusColor: Colors.blue,
      );

void main() {
  testWidgets('press relief approaches zero from both sides in every style', (
    t,
  ) async {
    await t.runAsync(() async {
      final failures = <String>[];
      for (final style in VisualStyle.values.where(
        (s) => s != VisualStyle.flat,
      )) {
        for (final dark in [false, true]) {
          for (final mode in ['normal', 'focused', 'contrast']) {
            for (final direction in [-1.0, 1.0]) {
              CustomPainter painter(double depth) => relief(
                style,
                depth,
                dark: dark,
                focused: mode == 'focused',
                contrast: mode == 'contrast',
              );
              final full = alpha(await render(painter(direction)));
              final tinyPixels = await render(painter(direction * .001));
              final zero = await render(painter(0));
              var difference = 0;
              for (var i = 3; i < zero.length; i += 4) {
                difference += (zero[i] - tinyPixels[i]).abs();
              }
              final tiny = difference;
              if (tiny >= full * .01) {
                failures.add(
                  '$style dark=$dark mode=$mode side=$direction ratio=${tiny / full}',
                );
              }
              expect(tinyPixels[(48 * 192 + 96) * 4 + 3], 0);
              if (mode == 'normal') {
                expect(alpha(zero), 0);
              } else {
                expect(
                  alpha(zero),
                  greaterThan(0),
                  reason: 'focus/contrast stays visible at neutral depth',
                );
              }
            }
          }
        }
      }
      expect(failures, isEmpty);
    });
  });
  testWidgets('surface keeps explicit fill and focus across neutral depth', (
    t,
  ) async {
    final boundaryKey = GlobalKey();
    final childKey = GlobalKey();
    for (final style in VisualStyle.values.where(
      (s) => s != VisualStyle.flat,
    )) {
      for (final fill in [false, true]) {
        for (final mode in ['normal', 'focused', 'contrast']) {
          Element? original;
          for (final depth in [1.0, .001, 0.0, -.001, -1.0]) {
            await t.pumpWidget(
              MaterialApp(
                home: Center(
                  child: RepaintBoundary(
                    key: boundaryKey,
                    child: MediaQuery(
                      data: MediaQueryData(
                        disableAnimations: true,
                        highContrast: mode == 'contrast',
                      ),
                      child: NeumorphicSurface(
                        palette: const Palette(
                          StudioTheme.white,
                          GlassMode.clear,
                        ).withSurfaces(SurfaceSettings(visualStyle: style)),
                        depth: depth,
                        fill: fill,
                        color: const Color(0x66808080),
                        focused: mode == 'focused',
                        child: SizedBox(key: childKey, width: 160, height: 64),
                      ),
                    ),
                  ),
                ),
              ),
            );
            original ??= childKey.currentContext! as Element;
            expect(childKey.currentContext, same(original));
            final boundary =
                boundaryKey.currentContext!.findRenderObject()!
                    as RenderRepaintBoundary;
            final bytes = await t.runAsync(() async {
              final image = await boundary.toImage();
              final data = await image.toByteData(
                format: ui.ImageByteFormat.rawRgba,
              );
              image.dispose();
              return Uint8List.fromList(data!.buffer.asUint8List());
            });
            expect(
              bytes![(32 * 160 + 80) * 4 + 3],
              fill ? 102 : 0,
              reason: '$style/$mode fill=$fill depth=$depth',
            );
            if (depth == 0 && !fill) {
              expect(alpha(bytes), mode == 'normal' ? 0 : greaterThan(0));
            }
          }
        }
      }
    }
  });

  testWidgets('button press reversal keeps current depth and one callback', (
    t,
  ) async {
    for (final style in VisualStyle.values.where(
      (s) => s != VisualStyle.flat,
    )) {
      final states = WidgetStatesController();
      final focus = FocusNode();
      var taps = 0;
      await t.pumpWidget(
        MaterialApp(
          theme: applyVisualStyleControls(
            ThemeData(),
            const Palette(
              StudioTheme.white,
              GlassMode.clear,
            ).withSurfaces(SurfaceSettings(visualStyle: style)),
          ),
          home: Scaffold(
            body: FilledButton(
              statesController: states,
              focusNode: focus,
              onPressed: () => taps++,
              child: const Text('Press'),
            ),
          ),
        ),
      );
      await t.pumpAndSettle();
      focus.requestFocus();
      await t.pumpAndSettle();
      final surface = find.ancestor(
        of: find.text('Press'),
        matching: find.byType(NeumorphicSurface),
      );
      final original = t.state(surface);
      CustomPainter current() {
        final paint = t.widget<CustomPaint>(
          find
              .descendant(of: surface, matching: find.byType(CustomPaint))
              .first,
        );
        return (paint.foregroundPainter ?? paint.painter)!;
      }

      states.update(WidgetState.pressed, true);
      await t.pump();
      await t.pump(const Duration(milliseconds: 20));
      final before = (current() as SurfaceStyleBlendPainter).depth;
      expect(before, inExclusiveRange(-1, 1));
      states.update(WidgetState.pressed, false);
      await t.pump();
      expect(
        (current() as SurfaceStyleBlendPainter).depth,
        closeTo(before, .00001),
      );
      await t.pump(const Duration(milliseconds: 20));
      expect(
        (current() as SurfaceStyleBlendPainter).depth,
        greaterThan(before),
      );
      expect(t.state(surface), same(original));
      expect(focus.hasFocus, isTrue);
      await t.pumpAndSettle();
      await t.tap(find.text('Press'));
      await t.pumpAndSettle();
      expect(taps, 1);
      await t.pumpWidget(const SizedBox());
      states.dispose();
      focus.dispose();
    }
  });
}
