import 'dart:typed_data';
import 'dart:ui' as ui;
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/appearance.dart';
import 'package:morrow_studio/experimental_controls.dart';
import 'package:morrow_studio/neumorphic_controls.dart';

Future<Uint8List> render(CustomPainter painter) async {
  final recorder = ui.PictureRecorder();
  final canvas = Canvas(recorder)..translate(16, 16);
  painter.paint(canvas, const Size(160, 64));
  final picture = recorder.endRecording();
  final image = await picture.toImage(192, 96);
  final bytes = await image.toByteData(format: ui.ImageByteFormat.rawRgba);
  final result = Uint8List.fromList(bytes!.buffer.asUint8List());
  image.dispose();
  picture.dispose();
  return result;
}

void main() {
  testWidgets(
    'relief light interpolation has continuous pixels and exact endpoints',
    (tester) async {
      await tester.runAsync(() async {
        for (final style in VisualStyle.values.where(
          (s) => s != VisualStyle.flat,
        )) {
          for (final depth in [-1.0, 1.0]) {
            for (final highContrast in [false, true]) {
              CustomPainter painter(bool dark, double? mix) =>
                  style == VisualStyle.neumorphism
                  ? NeumorphicSurfacePainter(
                      depth: depth,
                      radius: BorderRadius.circular(12),
                      surface: Colors.grey,
                      dark: dark,
                      darkMix: mix,
                      enabled: true,
                      highContrast: highContrast,
                      fill: false,
                      focused: false,
                      focusColor: Colors.blue,
                    )
                  : ExperimentalSurfacePainter(
                      style: style,
                      depth: depth,
                      radius: BorderRadius.circular(12),
                      surface: Colors.grey,
                      dark: dark,
                      darkMix: mix,
                      enabled: true,
                      highContrast: highContrast,
                      fill: false,
                      focused: false,
                      focusColor: Colors.blue,
                    );
              expect(
                await render(painter(false, 0)),
                await render(painter(false, null)),
              );
              expect(
                await render(painter(true, 1)),
                await render(painter(true, null)),
              );
              final before = await render(painter(false, .499));
              final after = await render(painter(true, .501));
              var delta = 0, total = 0;
              // Compare premultiplied color and alpha; transparent RGB carries no visual information.
              for (var i = 0; i < before.length; i += 4) {
                for (var channel = 0; channel < 3; channel++) {
                  delta +=
                      ((before[i + channel] * before[i + 3] -
                                  after[i + channel] * after[i + 3]) /
                              255)
                          .abs()
                          .round();
                  total += (before[i + channel] * before[i + 3] / 255).round();
                }
                delta += (before[i + 3] - after[i + 3]).abs();
                total += before[i + 3];
              }
              expect(
                delta / total,
                lessThan(.025),
                reason: '$style depth=$depth contrast=$highContrast',
              );
              expect(
                after[(48 * 192 + 96) * 4 + 3],
                0,
                reason: 'relief must not fill transparent center',
              );
            }
          }
        }
      });
    },
  );
  testWidgets('recessed relief fades to zero without a blur jump', (
    tester,
  ) async {
    await tester.runAsync(() async {
      for (final style in VisualStyle.values.where(
        (s) => s != VisualStyle.flat,
      )) {
        CustomPainter painter(double depth) => style == VisualStyle.neumorphism
            ? NeumorphicSurfacePainter(
                depth: depth,
                radius: BorderRadius.circular(12),
                surface: Colors.grey,
                dark: false,
                enabled: true,
                highContrast: false,
                fill: false,
                focused: false,
                focusColor: Colors.blue,
              )
            : ExperimentalSurfacePainter(
                style: style,
                depth: depth,
                radius: BorderRadius.circular(12),
                surface: Colors.grey,
                dark: false,
                enabled: true,
                highContrast: false,
                fill: false,
                focused: false,
                focusColor: Colors.blue,
              );
        int alpha(Uint8List bytes) {
          var sum = 0;
          for (var i = 3; i < bytes.length; i += 4) {
            sum += bytes[i];
          }
          return sum;
        }

        final full = alpha(await render(painter(-1)));
        final tiny = alpha(await render(painter(-.001)));
        expect(tiny, lessThan(full * .01), reason: style.name);
      }
    });
  });
}
