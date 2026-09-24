import 'dart:ui' as ui;
import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'animated_slider_style_test.dart' show difference;
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/appearance.dart';
import 'package:morrow_studio/experimental_controls.dart';
import 'package:morrow_studio/neumorphic_controls.dart';

Palette palette(VisualStyle style) => const Palette(
  StudioTheme.white,
  GlassMode.frosted,
).withSurfaces(SurfaceSettings(visualStyle: style));

Widget host(VisualStyle style, {bool reduced = false, bool visible = true}) {
  final p = palette(style);
  return MaterialApp(
    theme: applyVisualStyleControls(ThemeData(useMaterial3: true), p),
    home: MediaQuery(
      data: MediaQueryData(disableAnimations: reduced),
      child: TickerMode(
        enabled: visible,
        child: Scaffold(
          body: NeumorphicSwitch(palette: p, value: false, onChanged: (_) {}),
        ),
      ),
    ),
  );
}

void main() {
  testWidgets(
    'recessed switch track fades out and reverses from current frame',
    (t) async {
      await t.pumpWidget(host(VisualStyle.neumorphism));
      await t.pumpAndSettle();
      final nativeState = t.element(find.byType(Switch));
      double alpha() => t
          .widget<Switch>(find.byType(Switch))
          .trackColor!
          .resolve(<WidgetState>{})!
          .a;
      expect(alpha(), 0);
      await t.pumpWidget(host(VisualStyle.paper));
      expect(t.widget<Switch>(find.byType(Switch)).trackColor, isNotNull);
      expect(alpha(), closeTo(0, .001));
      await t.pump(const Duration(milliseconds: 60));
      final middle = alpha();
      expect(middle, greaterThan(0));
      expect(middle, lessThan(1));
      await t.pumpWidget(host(VisualStyle.neumorphism));
      expect(alpha(), closeTo(middle, .001));
      await t.pumpAndSettle();
      expect(alpha(), 0);
      expect(t.element(find.byType(Switch)), same(nativeState));
    },
  );
  testWidgets(
    'track handoff reaches native defaults and custom colors without an endpoint jump',
    (t) async {
      const captureKey = ValueKey('capture');
      Future<Uint8List> pixels() async {
        final boundary = t.renderObject<RenderRepaintBoundary>(
          find.byKey(captureKey),
        );
        final image = await boundary.toImage(pixelRatio: 1);
        final bytes = (await image.toByteData(
          format: ui.ImageByteFormat.rawRgba,
        ))!.buffer.asUint8List();
        image.dispose();
        return bytes;
      }

      for (final m3 in [false, true]) {
        for (final dark in [false, true]) {
          for (final enabled in [false, true]) {
            for (final selected in [false, true]) {
              for (final custom in [false, true]) {
                Widget build(VisualStyle style) {
                  final p = Palette(
                    dark ? StudioTheme.dark : StudioTheme.white,
                    GlassMode.clear,
                  ).withSurfaces(SurfaceSettings(visualStyle: style));
                  final base = ThemeData(
                    useMaterial3: m3,
                    brightness: dark ? Brightness.dark : Brightness.light,
                    switchTheme: custom
                        ? SwitchThemeData(
                            trackColor: WidgetStateProperty.resolveWith(
                              (states) => states.contains(WidgetState.disabled)
                                  ? const Color(0x6655bb33)
                                  : const Color(0x996622ee),
                            ),
                            trackOutlineColor: const WidgetStatePropertyAll(
                              Color(0x88669922),
                            ),
                            trackOutlineWidth: const WidgetStatePropertyAll(3),
                          )
                        : const SwitchThemeData(),
                  );
                  // Deliberately keep caller colors independent of the app theme,
                  // to exercise native fallback and custom-property precedence.
                  return MaterialApp(
                    theme: base,
                    themeAnimationDuration: Duration.zero,
                    home: Scaffold(
                      body: Center(
                        child: RepaintBoundary(
                          key: captureKey,
                          child: SizedBox(
                            width: 100,
                            height: 64,
                            child: Center(
                              child: NeumorphicSwitch(
                                palette: p,
                                value: selected,
                                activeThumbColor: Colors.amber,
                                onChanged: enabled ? (_) {} : null,
                              ),
                            ),
                          ),
                        ),
                      ),
                    ),
                  );
                }

                await t.pumpWidget(build(VisualStyle.neumorphism));
                await t.pumpAndSettle();
                await t.pumpWidget(build(VisualStyle.flat));
                await t.pump(const Duration(milliseconds: 179));
                final near = await t.runAsync(pixels);
                await t.pumpAndSettle();
                final native = await t.runAsync(pixels);
                expect(
                  difference(near!, native!),
                  lessThan(.01),
                  reason:
                      'm3=$m3/dark=$dark/enabled=$enabled/selected=$selected/custom=$custom',
                );
                expect(
                  t.widget<Switch>(find.byType(Switch)).trackColor,
                  isNull,
                );
              }
            }
          }
        }
      }
    },
  );

  testWidgets(
    'high contrast and Cupertino bypass relief; hidden and reduced motion settle',
    (t) async {
      Widget build({
        bool highContrast = false,
        bool reduced = false,
        bool visible = true,
        TargetPlatform platform = TargetPlatform.windows,
        bool adaptive = false,
        VisualStyle style = VisualStyle.neumorphism,
      }) {
        final p = palette(style);
        return MaterialApp(
          theme: applyVisualStyleControls(ThemeData(platform: platform), p),
          home: MediaQuery(
            data: MediaQueryData(
              highContrast: highContrast,
              disableAnimations: reduced,
            ),
            child: TickerMode(
              enabled: visible,
              child: Scaffold(
                body: NeumorphicSwitch(
                  palette: p,
                  value: false,
                  onChanged: (_) {},
                  adaptive: adaptive,
                ),
              ),
            ),
          ),
        );
      }

      await t.pumpWidget(build());
      await t.pumpWidget(build(style: VisualStyle.clay));
      await t.pump(const Duration(milliseconds: 30));
      expect(t.widget<Switch>(find.byType(Switch)).trackColor, isNotNull);
      await t.pumpWidget(build(style: VisualStyle.clay, visible: false));
      expect(t.widget<Switch>(find.byType(Switch)).trackColor, isNull);
      await t.pumpAndSettle();
      expect(t.binding.hasScheduledFrame, isFalse);
      await t.pumpWidget(build(reduced: true));
      expect(
        t.widget<Switch>(find.byType(Switch)).trackColor!.resolve({})!.a,
        0,
      );
      await t.pumpAndSettle();
      expect(t.binding.hasScheduledFrame, isFalse);
      await t.pumpWidget(build(highContrast: true));
      expect(t.widget<Switch>(find.byType(Switch)).trackColor, isNull);
      for (final platform in [TargetPlatform.iOS, TargetPlatform.macOS]) {
        await t.pumpWidget(build(platform: platform, adaptive: true));
        expect(t.widget<Switch>(find.byType(Switch)).trackColor, isNull);
        await t.pumpAndSettle();
        expect(t.takeException(), isNull);
      }
    },
  );

  testWidgets(
    'native switch keeps drag keyboard semantics and focus across seven styles',
    (t) async {
      for (final direction in TextDirection.values) {
        var style = VisualStyle.neumorphism;
        var value = false, enabled = true;
        var changes = 0;
        final focus = FocusNode();
        late StateSetter update;
        await t.pumpWidget(
          StatefulBuilder(
            builder: (context, setState) {
              update = setState;
              final p = palette(style);
              return MaterialApp(
                theme: applyVisualStyleControls(ThemeData(), p),
                home: Directionality(
                  textDirection: direction,
                  child: Scaffold(
                    body: Center(
                      child: NeumorphicSwitch(
                        palette: p,
                        value: value,
                        focusNode: focus,
                        onChanged: enabled
                            ? (v) {
                                changes++;
                                setState(() => value = v);
                              }
                            : null,
                      ),
                    ),
                  ),
                ),
              );
            },
          ),
        );
        await t.pumpAndSettle();
        final element = t.element(find.byType(Switch));
        focus.requestFocus();
        await t.pumpAndSettle();
        final semantics = t.ensureSemantics();
        expect(
          t.getSemantics(find.byType(Switch)),
          matchesSemantics(
            hasToggledState: true,
            isToggled: false,
            hasEnabledState: true,
            isEnabled: true,
            isFocusable: true,
            isFocused: true,
            hasTapAction: true,
            hasFocusAction: true,
          ),
        );
        await t.sendKeyEvent(LogicalKeyboardKey.space);
        await t.pumpAndSettle();
        expect(value, isTrue);
        final gesture = await t.startGesture(t.getCenter(find.byType(Switch)));
        await gesture.moveBy(
          Offset(direction == TextDirection.ltr ? -24 : 24, 0),
        );
        await t.pump();
        for (final next in VisualStyle.values) {
          update(() => style = next);
          await t.pump();
          await t.pump(const Duration(milliseconds: 30));
          expect(t.element(find.byType(Switch)), same(element));
          expect(focus.hasFocus, isTrue);
        }
        await gesture.moveBy(
          Offset(direction == TextDirection.ltr ? -40 : 40, 0),
        );
        await gesture.up();
        await t.pumpAndSettle();
        expect(value, isFalse);
        final count = changes;
        update(() => enabled = false);
        await t.pumpAndSettle();
        await t.tap(find.byType(Switch));
        await t.sendKeyEvent(LogicalKeyboardKey.space);
        await t.pumpAndSettle();
        expect(changes, count);
        expect(t.takeException(), isNull);
        semantics.dispose();
        await t.pumpWidget(const SizedBox());
        focus.dispose();
      }
    },
  );
  testWidgets('recessed light and dark lighting stays continuous on reversal', (
    t,
  ) async {
    Widget build(StudioTheme mode) {
      final p = Palette(mode, GlassMode.clear).withSurfaces(
        const SurfaceSettings(visualStyle: VisualStyle.neumorphism),
      );
      return MaterialApp(
        home: Scaffold(
          body: NeumorphicSwitch(palette: p, value: true, onChanged: (_) {}),
        ),
      );
    }

    Future<Uint8List> reliefPixels() async {
      final painter = t
          .widgetList<CustomPaint>(find.byType(CustomPaint))
          .singleWhere(
            (w) =>
                w.painter.runtimeType.toString() == '_NeumorphicSwitchPainter',
          )
          .painter!;
      final recorder = ui.PictureRecorder();
      painter.paint(Canvas(recorder), const Size(100, 64));
      final picture = recorder.endRecording();
      final image = await picture.toImage(100, 64);
      final bytes = (await image.toByteData(
        format: ui.ImageByteFormat.rawRgba,
      ))!.buffer.asUint8List();
      image.dispose();
      picture.dispose();
      return bytes;
    }

    await t.pumpWidget(build(StudioTheme.white));
    await t.pumpAndSettle();
    final light = await t.runAsync(reliefPixels);
    await t.pumpWidget(build(StudioTheme.dark));
    await t.pump(const Duration(milliseconds: 89));
    final before = await t.runAsync(reliefPixels);
    await t.pump(const Duration(milliseconds: 2));
    final after = await t.runAsync(reliefPixels);
    expect(difference(before!, after!), lessThan(.04));
    expect(difference(light!, after), greaterThan(.1));
    await t.pumpWidget(build(StudioTheme.white));
    final reversed = await t.runAsync(reliefPixels);
    expect(difference(after, reversed!), lessThan(.001));
    await t.pumpAndSettle();
    final end = await t.runAsync(reliefPixels);
    expect(difference(light, end!), lessThan(.001));
  });
}
