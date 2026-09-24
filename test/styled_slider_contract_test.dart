import 'dart:ui' as ui;
import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/appearance.dart';
import 'package:morrow_studio/animated_slider_style.dart';
import 'package:morrow_studio/experimental_controls.dart';
import 'package:morrow_studio/neumorphic_controls.dart';

class _CanvasContext extends PaintingContext {
  _CanvasContext(this.canvas)
    : super(ContainerLayer(), const Rect.fromLTWH(0, 0, 240, 48));
  @override
  final Canvas canvas;
}

Future<Uint8List> trackPixels(
  SliderThemeData theme, {
  double enabled = 1,
  bool interactive = true,
  double? secondary,
  TextDirection direction = TextDirection.ltr,
}) async {
  final box = RenderConstrainedBox(
    additionalConstraints: const BoxConstraints.tightFor(
      width: 240,
      height: 48,
    ),
  );
  box.layout(const BoxConstraints.tightFor(width: 240, height: 48));
  final recorder = ui.PictureRecorder();
  theme = theme.copyWith(
    overlayShape: const RoundSliderOverlayShape(overlayRadius: 20),
  );
  final shape = theme.trackShape!;
  final rect = shape.getPreferredRect(
    parentBox: box,
    sliderTheme: theme,
    isEnabled: interactive,
    isDiscrete: false,
  );
  double at(double value) => direction == TextDirection.ltr
      ? rect.left + rect.width * value
      : rect.right - rect.width * value;
  shape.paint(
    _CanvasContext(Canvas(recorder)),
    Offset.zero,
    parentBox: box,
    sliderTheme: theme,
    enableAnimation: AlwaysStoppedAnimation(enabled),
    textDirection: direction,
    thumbCenter: Offset(at(.3), rect.center.dy),
    secondaryOffset: secondary == null
        ? null
        : Offset(at(secondary), rect.center.dy),
    isEnabled: interactive,
  );
  final picture = recorder.endRecording();
  final image = await picture.toImage(240, 48);
  final data = await image.toByteData(format: ui.ImageByteFormat.rawRgba);
  final result = Uint8List.fromList(data!.buffer.asUint8List());
  image.dispose();
  picture.dispose();
  box.dispose();
  return result;
}

Future<Uint8List> thumbPixels(SliderThemeData theme, double enabled) async {
  final box = RenderConstrainedBox(
    additionalConstraints: const BoxConstraints.tightFor(
      width: 240,
      height: 48,
    ),
  );
  box.layout(const BoxConstraints.tightFor(width: 240, height: 48));
  final recorder = ui.PictureRecorder();
  final label = TextPainter(textDirection: TextDirection.ltr);
  theme.thumbShape!.paint(
    _CanvasContext(Canvas(recorder)),
    const Offset(120, 24),
    activationAnimation: const AlwaysStoppedAnimation(0),
    enableAnimation: AlwaysStoppedAnimation(enabled),
    isDiscrete: false,
    labelPainter: label,
    parentBox: box,
    sliderTheme: theme,
    textDirection: TextDirection.ltr,
    value: .5,
    textScaleFactor: 1,
    sizeWithOverflow: const Size(240, 48),
  );
  final picture = recorder.endRecording();
  final image = await picture.toImage(240, 48);
  final data = await image.toByteData(format: ui.ImageByteFormat.rawRgba);
  final result = Uint8List.fromList(data!.buffer.asUint8List());
  image.dispose();
  picture.dispose();
  label.dispose();
  box.dispose();
  return result;
}

void main() {
  final palette = const Palette(
    StudioTheme.white,
    GlassMode.clear,
  ).withSurfaces(const SurfaceSettings(visualStyle: VisualStyle.neumorphism));
  SliderThemeData theme() =>
      applyNeumorphicControls(ThemeData(), palette).sliderTheme;
  testWidgets('relief track uses animation position when interaction changes', (
    t,
  ) async {
    await t.runAsync(() async {
      for (final value in [0.0, .25, .5, .75, 1.0]) {
        expect(
          await trackPixels(theme(), enabled: value, interactive: false),
          await trackPixels(theme(), enabled: value),
          reason:
              'interaction flag must not skip native enable tween at $value',
        );
      }
      expect(
        await trackPixels(theme(), enabled: 0),
        isNot(await trackPixels(theme())),
      );
    });
  });
  testWidgets(
    'relief track paints buffered range only ahead of current value in both directions',
    (t) async {
      await t.runAsync(() async {
        for (final direction in TextDirection.values) {
          final custom = theme().copyWith(
            activeTrackColor: Colors.red,
            inactiveTrackColor: Colors.white,
            secondaryActiveTrackColor: Colors.green,
            disabledSecondaryActiveTrackColor: Colors.grey,
          );
          final plain = await trackPixels(custom, direction: direction);
          final buffer = await trackPixels(
            custom,
            direction: direction,
            secondary: .8,
          );
          expect(buffer, isNot(plain), reason: '$direction buffered segment');
          expect(
            await trackPixels(custom, direction: direction, secondary: .1),
            plain,
          );
          final x = direction == TextDirection.ltr ? 140 : 100;
          final pixel = (24 * 240 + x) * 4;
          expect(
            buffer[pixel + 1],
            greaterThan(buffer[pixel]),
            reason: 'buffer uses supplied green color',
          );
          final activeX = direction == TextDirection.ltr ? 55 : 185;
          final active = (24 * 240 + activeX) * 4;
          expect(
            buffer[active],
            greaterThan(buffer[active + 1]),
            reason: 'active uses supplied red color',
          );
        }
        expect(
          (await trackPixels(
            theme().copyWith(trackHeight: 0),
          )).every((x) => x == 0),
          isTrue,
        );
      });
    },
  );
  testWidgets('every styled thumb respects enabled and disabled theme colors', (
    t,
  ) async {
    await t.runAsync(() async {
      for (final style in VisualStyle.values.where(
        (s) => s != VisualStyle.flat,
      )) {
        final styled =
            applyVisualStyleControls(
              ThemeData(),
              palette.withSurfaces(SurfaceSettings(visualStyle: style)),
            ).sliderTheme.copyWith(
              thumbColor: Colors.red,
              disabledThumbColor: Colors.blue,
            );
        for (final enabled in [0.0, .25, .5, .75, 1.0]) {
          final bytes = await thumbPixels(styled, enabled);
          final color = Color.lerp(Colors.blue, Colors.red, enabled)!;
          // Sample the fill beside the industrial grip stripe.
          final index = (24 * 240 + 124) * 4;
          expect(
            bytes[index],
            closeTo(color.r * 255, 1),
            reason: '$style/$enabled red',
          );
          expect(bytes[index + 1], closeTo(color.g * 255, 1));
          expect(bytes[index + 2], closeTo(color.b * 255, 1));
          expect(bytes[index + 3], 255);
        }
      }
    });
  });

  testWidgets(
    'native slider retains drag focus and callbacks through style changes in LTR and RTL',
    (t) async {
      for (final direction in TextDirection.values) {
        final focus = FocusNode();
        var value = .3, changes = 0, starts = 0, ends = 0;
        var enabled = true;
        var style = VisualStyle.neumorphism;
        late StateSetter update;
        final sliderKey = GlobalKey();
        await t.pumpWidget(
          StatefulBuilder(
            builder: (context, setState) {
              update = setState;
              return MaterialApp(
                theme: applyVisualStyleControls(
                  ThemeData(),
                  palette.withSurfaces(SurfaceSettings(visualStyle: style)),
                ),
                home: Directionality(
                  textDirection: direction,
                  child: Scaffold(
                    body: Center(
                      child: SizedBox(
                        width: 300,
                        child: AnimatedSliderStyle(
                          palette: palette.withSurfaces(
                            SurfaceSettings(visualStyle: style),
                          ),
                          child: Slider(
                            key: sliderKey,
                            value: value,
                            secondaryTrackValue: .9,
                            focusNode: focus,
                            divisions: 20,
                            onChanged: enabled
                                ? (next) {
                                    changes++;
                                    setState(() => value = next);
                                  }
                                : null,
                            onChangeStart: (_) => starts++,
                            onChangeEnd: (_) => ends++,
                          ),
                        ),
                      ),
                    ),
                  ),
                ),
              );
            },
          ),
        );
        await t.pumpAndSettle();
        focus.requestFocus();
        await t.pumpAndSettle();
        final original = sliderKey.currentState;
        final before = value;
        await t.sendKeyEvent(LogicalKeyboardKey.arrowRight);
        await t.pumpAndSettle();
        expect(
          value,
          direction == TextDirection.ltr
              ? greaterThan(before)
              : lessThan(before),
        );
        starts = 0;
        ends = 0;
        final gesture = await t.startGesture(t.getCenter(find.byType(Slider)));
        await t.pump();
        for (final next in VisualStyle.values) {
          update(() => style = next);
          await t.pump();
          await t.pump(const Duration(milliseconds: 35));
          await gesture.moveBy(
            Offset(direction == TextDirection.ltr ? 8 : -8, 0),
          );
          await t.pump();
          expect(sliderKey.currentState, same(original));
          expect(focus.hasFocus, isTrue);
          expect(t.takeException(), isNull);
        }
        await gesture.up();
        await t.pumpAndSettle();
        expect(starts, 1);
        expect(ends, 1);
        expect(changes, greaterThan(1));
        final count = changes;
        final saved = value;
        update(() => enabled = false);
        await t.pump();
        await t.tap(find.byType(Slider));
        await t.sendKeyEvent(LogicalKeyboardKey.arrowRight);
        await t.pumpAndSettle();
        expect(changes, count);
        expect(value, saved);
        await t.pumpWidget(const SizedBox());
        focus.dispose();
      }
    },
  );
}
