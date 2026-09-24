import 'dart:typed_data';
import 'dart:ui' as ui;
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/appearance.dart';
import 'package:morrow_studio/experimental_controls.dart';
import 'package:morrow_studio/style_input_border.dart';
import 'package:morrow_studio/neumorphic_controls.dart';

Future<Uint8List> pixels(
  InputBorder border, {
  bool labelGap = false,
  TextDirection direction = TextDirection.ltr,
}) async {
  final recorder = ui.PictureRecorder();
  final canvas = Canvas(recorder);
  border.paint(
    canvas,
    const Rect.fromLTWH(12, 12, 180, 58),
    gapStart: labelGap ? (direction == TextDirection.ltr ? 40 : 140) : null,
    gapExtent: labelGap ? 50 : 0,
    gapPercentage: labelGap ? 1 : 0,
    textDirection: direction,
  );
  final picture = recorder.endRecording();
  final image = await picture.toImage(204, 82);
  final data = await image.toByteData(format: ui.ImageByteFormat.rawRgba);
  image.dispose();
  picture.dispose();
  return Uint8List.fromList(data!.buffer.asUint8List());
}

double alphaDifference(Uint8List a, Uint8List b) {
  var difference = 0, reference = 0;
  for (var i = 3; i < a.length; i += 4) {
    difference += (a[i] - b[i]).abs();
    reference += a[i];
  }
  return difference / (reference == 0 ? 1 : reference);
}

void main() {
  testWidgets(
    'styled input outlines retain inset pixels at both transition ends',
    (t) async {
      final borders = <String, InputBorder>{};
      for (final theme in [StudioTheme.white, StudioTheme.dark]) {
        for (final style in VisualStyle.values) {
          final palette = Palette(
            theme,
            GlassMode.clear,
          ).withSurfaces(SurfaceSettings(visualStyle: style));
          borders['${style.name}/${theme.name}'] = applyVisualStyleControls(
            ThemeData(
              inputDecorationTheme: const InputDecorationTheme(
                enabledBorder: StyledInputBorder(
                  borderSide: BorderSide(color: Colors.grey),
                ),
              ),
            ),
            palette,
          ).inputDecorationTheme.enabledBorder!;
        }
      }
      await t.runAsync(() async {
        final images = <String, Uint8List>{};
        for (final e in borders.entries) {
          images[e.key] = await pixels(e.value);
        }
        for (final a in borders.entries) {
          for (final b in borders.entries) {
            if (a.key == b.key) continue;
            final early =
                ShapeBorder.lerp(a.value, b.value, .0001)! as InputBorder;
            final late =
                ShapeBorder.lerp(a.value, b.value, .9999)! as InputBorder;
            expect(
              alphaDifference(images[a.key]!, await pixels(early)),
              lessThan(.02),
              reason: '${a.key} -> ${b.key} start',
            );
            expect(
              alphaDifference(images[b.key]!, await pixels(late)),
              lessThan(.02),
              reason: '${a.key} -> ${b.key} end',
            );
            final middle = await pixels(
              ShapeBorder.lerp(a.value, b.value, .5)! as InputBorder,
            );
            expect(
              middle[(41 * 204 + 102) * 4 + 3],
              0,
              reason: 'transparent interior must stay clear',
            );
          }
        }
      });
    },
  );

  testWidgets(
    'new relief tween keeps light continuous and survives retargeting',
    (t) async {
      const light = NeumorphicInputBorder(
        surface: Colors.grey,
        dark: false,
        borderRadius: BorderRadius.all(Radius.circular(12)),
      );
      const dark = NeumorphicInputBorder(
        surface: Colors.grey,
        dark: true,
        borderRadius: BorderRadius.all(Radius.circular(12)),
      );
      final clay = ExperimentalInputBorder(
        palette: const Palette(StudioTheme.dark, GlassMode.clear),
        style: VisualStyle.clay,
        borderRadius: BorderRadius.circular(12),
      );
      await t.runAsync(() async {
        final before = await pixels(
          ShapeBorder.lerp(light, dark, .499)! as InputBorder,
        );
        final after = await pixels(
          ShapeBorder.lerp(light, dark, .501)! as InputBorder,
        );
        expect(alphaDifference(before, after), lessThan(.02));
        final interrupted =
            ShapeBorder.lerp(light, clay, .37)! as StyledInputBorder;
        final reversed =
            ShapeBorder.lerp(interrupted, dark, .0001)! as StyledInputBorder;
        expect(
          alphaDifference(await pixels(interrupted), await pixels(reversed)),
          lessThan(.02),
        );
        expect(await pixels(interrupted.copyWith()), await pixels(interrupted));
        expect(await pixels(interrupted.scale(1)), await pixels(interrupted));
        final next =
            ShapeBorder.lerp(interrupted, dark, .25)! as StyledInputBorder;
        expect(
          next.relief.neumorphic,
          greaterThan(interrupted.relief.neumorphic),
        );
        expect(next.relief.clay, lessThan(interrupted.relief.clay));
        expect(next.relief.industrial, 0);
      });
    },
  );

  testWidgets(
    'actual themed input preserves IME, selection and focus through reversals',
    (t) async {
      final controller = TextEditingController();
      final focus = FocusNode();
      addTearDown(controller.dispose);
      addTearDown(focus.dispose);
      final fieldKey = GlobalKey();
      Widget scene(VisualStyle style, StudioTheme brightness) {
        final palette = Palette(
          brightness,
          GlassMode.clear,
        ).withSurfaces(SurfaceSettings(visualStyle: style));
        final base = ThemeData(
          inputDecorationTheme: const InputDecorationTheme(
            border: StyledInputBorder(),
            enabledBorder: StyledInputBorder(),
            focusedBorder: StyledInputBorder(
              borderSide: BorderSide(color: Colors.blue),
            ),
          ),
        );
        return MaterialApp(
          theme: applyVisualStyleControls(base, palette),
          home: Scaffold(
            body: TextField(
              key: fieldKey,
              controller: controller,
              focusNode: focus,
              decoration: const InputDecoration(labelText: 'Draft'),
            ),
          ),
        );
      }

      await t.pumpWidget(scene(VisualStyle.neumorphism, StudioTheme.white));
      focus.requestFocus();
      await t.pump();
      const editing = TextEditingValue(
        text: '草稿 draft',
        selection: TextSelection(baseOffset: 2, extentOffset: 5),
        composing: TextRange(start: 0, end: 2),
      );
      controller.value = editing;
      await t.pump();
      final original = fieldKey.currentState;
      for (final style in [
        ...VisualStyle.values,
        ...VisualStyle.values.reversed,
      ]) {
        await t.pumpWidget(
          scene(
            style,
            style.index.isEven ? StudioTheme.dark : StudioTheme.white,
          ),
        );
        await t.pump(const Duration(milliseconds: 35));
        expect(fieldKey.currentState, same(original));
        expect(controller.value, editing);
        expect(focus.hasFocus, isTrue);
        expect(t.takeException(), isNull);
      }
      await t.pumpAndSettle();
      expect(controller.value, editing);
    },
  );

  testWidgets('floating label clears inset decoration in LTR and RTL', (
    t,
  ) async {
    await t.runAsync(() async {
      for (final style in [
        VisualStyle.neumorphism,
        VisualStyle.clay,
        VisualStyle.industrial,
      ]) {
        final palette = const Palette(
          StudioTheme.white,
          GlassMode.clear,
        ).withSurfaces(SurfaceSettings(visualStyle: style));
        final border = applyVisualStyleControls(ThemeData(), palette)
            .inputDecorationTheme
            .enabledBorder!
            .copyWith(borderSide: BorderSide.none);
        for (final direction in TextDirection.values) {
          final full = await pixels(border, direction: direction);
          final gap = await pixels(
            border,
            labelGap: true,
            direction: direction,
          );
          final start = direction == TextDirection.ltr ? 53 : 103;
          int topAlpha(Uint8List bytes) {
            var sum = 0;
            for (var y = 12; y < 17; y++) {
              for (var x = start; x < start + 35; x++) {
                sum += bytes[(y * 204 + x) * 4 + 3];
              }
            }
            return sum;
          }

          expect(
            topAlpha(full),
            greaterThan(0),
            reason: '$style/$direction reference',
          );
          expect(topAlpha(gap), 0, reason: '$style/$direction label clearance');
          expect(gap[(41 * 204 + 102) * 4 + 3], 0);
        }
      }
    });
  });

  testWidgets('decorative style opacity fades pixels without adding a fill', (
    t,
  ) async {
    await t.runAsync(() async {
      for (final style in VisualStyle.values.where((s) => s.experimental)) {
        for (final depth in [1.0, -1.0]) {
          for (final opacity in [0.0, .5, 1.0]) {
            final recorder = ui.PictureRecorder();
            final canvas = Canvas(recorder)..translate(12, 12);
            ExperimentalSurfacePainter(
              style: style,
              depth: depth,
              radius: BorderRadius.circular(10),
              surface: Colors.white,
              dark: false,
              enabled: true,
              highContrast: false,
              fill: false,
              focused: true,
              focusColor: Colors.purple,
              opacity: opacity,
            ).paint(canvas, const Size(180, 58));
            final picture = recorder.endRecording();
            final image = await picture.toImage(204, 82);
            final bytes = (await image.toByteData(
              format: ui.ImageByteFormat.rawRgba,
            ))!.buffer.asUint8List();
            if (opacity == 0) {
              expect(bytes.every((b) => b == 0), isTrue);
            }
            expect(bytes[(41 * 204 + 102) * 4 + 3], 0);
            image.dispose();
            picture.dispose();
          }
        }
      }
    });
  });
}
