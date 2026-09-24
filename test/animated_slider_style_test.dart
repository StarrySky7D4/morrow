import 'dart:typed_data';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/appearance.dart';
import 'package:morrow_studio/animated_slider_style.dart';
import 'package:morrow_studio/experimental_controls.dart';
import 'styled_slider_contract_test.dart' as raster;

double difference(Uint8List a, Uint8List b) {
  var difference = 0.0, total = 0.0;
  for (var i = 0; i < a.length; i += 4) {
    for (var c = 0; c < 3; c++) {
      difference += (a[i + c] * a[i + 3] - b[i + c] * b[i + 3]).abs() / 255;
      total += a[i + c] * a[i + 3] / 255;
    }
    difference += (a[i + 3] - b[i + 3]).abs();
    total += a[i + 3];
  }
  return difference / (total == 0 ? 1 : total);
}

SliderThemeData theme(Palette palette) => applyVisualStyleControls(
  ThemeData(
    sliderTheme: SliderThemeData(
      trackHeight: 4,
      thumbShape: const RoundSliderThumbShape(),
      trackShape: const RoundedRectSliderTrackShape(),
      thumbColor: palette.surface,
      disabledThumbColor: palette.surface.withValues(alpha: .5),
      activeTrackColor: palette.accent,
      disabledActiveTrackColor: palette.accent.withValues(alpha: .3),
      inactiveTrackColor: palette.surface,
      disabledInactiveTrackColor: palette.surface.withValues(alpha: .45),
      secondaryActiveTrackColor: Colors.green,
      disabledSecondaryActiveTrackColor: Colors.grey,
    ),
  ),
  palette,
).sliderTheme;

void main() {
  testWidgets('morphing slider preserves original style endpoints', (t) async {
    final previousShadows = debugDisableShadows;
    debugDisableShadows = false;
    addTearDown(() => debugDisableShadows = previousShadows);
    await t
        .runAsync(() async {
          for (final brightness in [StudioTheme.white, StudioTheme.dark]) {
            for (final style in VisualStyle.values) {
              final p = Palette(
                brightness,
                GlassMode.clear,
              ).withSurfaces(SurfaceSettings(visualStyle: style));
              final original = theme(p);
              final frame = SliderStyleFrame.target(p);
              final animated = original.copyWith(
                thumbShape: MorphingSliderThumb(frame),
                trackShape: MorphingSliderTrack(frame),
              );
              for (final enabled in [0.0, 1.0]) {
                expect(
                  difference(
                    await raster.thumbPixels(original, enabled),
                    await raster.thumbPixels(animated, enabled),
                  ),
                  lessThan(.02),
                  reason: '$style/$brightness/$enabled thumb endpoint',
                );
                for (final direction in TextDirection.values) {
                  expect(
                    difference(
                      await raster.trackPixels(
                        original,
                        enabled: enabled,
                        direction: direction,
                      ),
                      await raster.trackPixels(
                        animated,
                        enabled: enabled,
                        direction: direction,
                      ),
                    ),
                    lessThan(.02),
                    reason:
                        '$style/$brightness/$enabled/$direction track endpoint',
                  );
                }
              }
            }
          }
        })
        .whenComplete(() => debugDisableShadows = previousShadows);
  });

  testWidgets('all style pairs interpolate without a midpoint geometry jump', (
    t,
  ) async {
    final previousShadows = debugDisableShadows;
    debugDisableShadows = false;
    addTearDown(() => debugDisableShadows = previousShadows);
    await t
        .runAsync(() async {
          const p = Palette(StudioTheme.white, GlassMode.clear);
          for (final a in VisualStyle.values) {
            for (final b in VisualStyle.values) {
              if (a == b) continue;
              final start = SliderStyleFrame.target(
                p.withSurfaces(SurfaceSettings(visualStyle: a)),
              );
              final end = SliderStyleFrame.target(
                p.withSurfaces(SurfaceSettings(visualStyle: b)),
              );
              SliderThemeData middle(double progress) {
                final frame = SliderStyleFrame.lerp(start, end, progress);
                return theme(p).copyWith(
                  trackHeight: frame.trackHeight,
                  thumbShape: MorphingSliderThumb(frame),
                  trackShape: MorphingSliderTrack(frame),
                );
              }

              expect(
                difference(
                  await raster.thumbPixels(middle(.499), 1),
                  await raster.thumbPixels(middle(.501), 1),
                ),
                lessThan(.025),
                reason: '$a/$b thumb midpoint',
              );
              expect(
                difference(
                  await raster.trackPixels(middle(.499), secondary: .8),
                  await raster.trackPixels(middle(.501), secondary: .8),
                ),
                lessThan(.025),
                reason: '$a/$b track midpoint',
              );
            }
          }
        })
        .whenComplete(() => debugDisableShadows = previousShadows);
  });

  testWidgets(
    'retargeting keeps displayed geometry and child state; reduced or hidden motion settles',
    (t) async {
      final childKey = GlobalKey();
      Widget scene(
        VisualStyle style, {
        bool reduced = false,
        bool visible = true,
        double value = .4,
      }) => MaterialApp(
        home: MediaQuery(
          data: MediaQueryData(disableAnimations: reduced),
          child: TickerMode(
            enabled: visible,
            child: AnimatedSliderStyle(
              palette: const Palette(
                StudioTheme.white,
                GlassMode.clear,
              ).withSurfaces(SurfaceSettings(visualStyle: style)),
              child: Material(
                child: Slider(key: childKey, value: value, onChanged: (_) {}),
              ),
            ),
          ),
        ),
      );
      SliderStyleFrame frame() =>
          (t
                      .widget<SliderTheme>(
                        find
                            .ancestor(
                              of: find.byKey(childKey),
                              matching: find.byType(SliderTheme),
                            )
                            .first,
                      )
                      .data
                      .thumbShape!
                  as MorphingSliderThumb)
              .frame;
      await t.pumpWidget(scene(VisualStyle.clay));
      final original = childKey.currentState;
      await t.pumpWidget(scene(VisualStyle.industrial));
      await t.pump(const Duration(milliseconds: 40));
      final before = frame();
      expect(before.radius, inExclusiveRange(1, 11));
      await t.pumpWidget(scene(VisualStyle.paper));
      expect(frame().sameAs(before), isTrue);
      expect(childKey.currentState, same(original));
      await t.pump(const Duration(milliseconds: 25));
      final moving = frame();
      await t.pumpWidget(scene(VisualStyle.paper, value: .5));
      expect(
        frame().sameAs(moving),
        isTrue,
        reason: 'value updates do not restart style animation',
      );
      await t.pumpWidget(scene(VisualStyle.brutalist, reduced: true));
      expect(frame().brutalist, 1);
      await t.pumpWidget(scene(VisualStyle.neumorphism, visible: false));
      expect(frame().neumo, 1);
      await t.pump();
      expect(t.binding.hasScheduledFrame, isFalse);
      expect(childKey.currentState, same(original));
    },
  );
  testWidgets(
    'continuous tracks preserve buffer and disabled semantics in every style',
    (t) async {
      await t.runAsync(() async {
        for (final style in VisualStyle.values) {
          final p = const Palette(
            StudioTheme.white,
            GlassMode.clear,
          ).withSurfaces(SurfaceSettings(visualStyle: style));
          final frame = SliderStyleFrame.target(p);
          final data = theme(p).copyWith(
            trackShape: MorphingSliderTrack(frame),
            thumbShape: MorphingSliderThumb(frame),
          );
          for (final direction in TextDirection.values) {
            final plain = await raster.trackPixels(data, direction: direction);
            expect(
              await raster.trackPixels(
                data,
                direction: direction,
                secondary: .8,
              ),
              isNot(plain),
            );
            expect(
              await raster.trackPixels(
                data,
                direction: direction,
                secondary: .1,
              ),
              plain,
            );
            expect(
              await raster.trackPixels(
                data,
                direction: direction,
                secondary: 1.5,
              ),
              await raster.trackPixels(
                data,
                direction: direction,
                secondary: 1,
              ),
              reason: 'buffer clipped to track',
            );
            for (final enabled in [0.0, .5, 1.0]) {
              expect(
                await raster.trackPixels(
                  data,
                  direction: direction,
                  enabled: enabled,
                  interactive: false,
                ),
                await raster.trackPixels(
                  data,
                  direction: direction,
                  enabled: enabled,
                ),
              );
            }
          }
          expect(
            (await raster.trackPixels(
              data.copyWith(trackHeight: 0),
            )).every((b) => b == 0),
            isTrue,
          );
        }
      });
    },
  );
}
