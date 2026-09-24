import 'dart:io';
import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/appearance.dart';
import 'package:morrow_studio/experimental_controls.dart';
import 'package:morrow_studio/neumorphic_controls.dart';

const _previewKey = ValueKey('depth-state-capture');

void main() {
  testWidgets(
    'render light and dark raised and inset surfaces at three depths',
    (tester) async {
      tester.view.physicalSize = const Size(1050, 910);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.reset);
      final font = File(r'C:\Windows\Fonts\segoeui.ttf');
      await (FontLoader('DepthStates')..addFont(
            Future.value(ByteData.sublistView(font.readAsBytesSync())),
          ))
          .load();

      await tester.pumpWidget(
        MaterialApp(
          theme: ThemeData(fontFamily: 'DepthStates'),
          home: Scaffold(
            body: RepaintBoundary(
              key: _previewKey,
              child: Container(
                color: const Color(0xFFF5F5F9),
                padding: const EdgeInsets.all(18),
                child: Column(
                  children: [
                    const Align(
                      alignment: Alignment.centerLeft,
                      child: Text(
                        'Morrow / Relief states',
                        style: TextStyle(fontSize: 23),
                      ),
                    ),
                    const SizedBox(height: 10),
                    const Row(
                      children: [
                        SizedBox(width: 180),
                        Expanded(child: Center(child: Text('0%'))),
                        Expanded(child: Center(child: Text('100%'))),
                        Expanded(child: Center(child: Text('200%'))),
                      ],
                    ),
                    for (final style in [
                      VisualStyle.brutalist,
                      VisualStyle.industrial,
                      VisualStyle.neumorphism,
                    ])
                      for (final dark in [false, true])
                        for (final inset in [false, true])
                          SizedBox(
                            height: 66,
                            child: Row(
                              children: [
                                SizedBox(
                                  width: 180,
                                  child: Text(
                                    style.name +
                                        (dark ? ' / dark' : ' / light') +
                                        (inset ? ' / pressed' : ' / raised'),
                                    style: const TextStyle(fontSize: 13),
                                  ),
                                ),
                                for (final depth in [0.0, 1.0, 2.0])
                                  Expanded(
                                    child: Padding(
                                      padding: const EdgeInsets.all(5),
                                      child: CustomPaint(
                                        painter: _CheckerPainter(dark),
                                        child: Center(
                                          child: SizedBox(
                                            width: 206,
                                            height: 42,
                                            child: Builder(
                                              builder: (context) {
                                                final palette =
                                                    Palette(
                                                      dark
                                                          ? StudioTheme.dark
                                                          : StudioTheme.white,
                                                      GlassMode.clear,
                                                    ).withSurfaces(
                                                      SurfaceSettings(
                                                        visualStyle: style,
                                                        styleDepth: depth,
                                                      ),
                                                    );
                                                return NeumorphicSurface(
                                                  palette: palette,
                                                  depth: inset ? -1 : 1,
                                                  borderRadius: palette
                                                      .borderRadius(12),
                                                  child: Center(
                                                    child: Text(
                                                      inset
                                                          ? 'PRESSED'
                                                          : 'RAISED',
                                                      style: TextStyle(
                                                        color: dark
                                                            ? Colors.white
                                                            : const Color(
                                                                0xFF211E26,
                                                              ),
                                                        fontSize: 12,
                                                      ),
                                                    ),
                                                  ),
                                                );
                                              },
                                            ),
                                          ),
                                        ),
                                      ),
                                    ),
                                  ),
                              ],
                            ),
                          ),
                  ],
                ),
              ),
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull);
      await tester.runAsync(() async {
        final image = await tester
            .renderObject<RenderRepaintBoundary>(find.byKey(_previewKey))
            .toImage(pixelRatio: 1);
        final bytes = await image.toByteData(format: ui.ImageByteFormat.png);
        final output = File('build/ui-style-preview/style-depth-states.png');
        await output.parent.create(recursive: true);
        await output.writeAsBytes(bytes!.buffer.asUint8List());
        image.dispose();
      });
    },
    skip: !Platform.isWindows,
  );

  test(
    'brutalist relief keeps its center clear and extrusion outside',
    () async {
      const background = Color(0xFF69BBD0);
      for (final dark in [false, true]) {
        for (final depth in [0.0, 1.0, 2.0]) {
          for (final inset in [false, true]) {
            final recorder = ui.PictureRecorder();
            final canvas = Canvas(recorder)
              ..drawColor(background, BlendMode.src)
              ..translate(8, 8);
            ExperimentalSurfacePainter(
              style: VisualStyle.brutalist,
              depth: inset ? -depth : depth,
              radius: BorderRadius.zero,
              surface: Colors.transparent,
              dark: dark,
              enabled: true,
              highContrast: false,
              fill: false,
              focused: false,
              focusColor: const Color(0xFFFF00FF),
            ).paint(canvas, const Size(60, 40));
            final picture = recorder.endRecording();
            final image = await picture.toImage(80, 60);
            picture.dispose();
            final bytes = (await image.toByteData(
              format: ui.ImageByteFormat.rawRgba,
            ))!.buffer.asUint8List();
            image.dispose();
            Color pixel(int x, int y) {
              final offset = (y * 80 + x) * 4;
              return Color.fromARGB(
                bytes[offset + 3],
                bytes[offset],
                bytes[offset + 1],
                bytes[offset + 2],
              );
            }

            expect(pixel(38, 28), background);
            expect(
              pixel(12, 28),
              background,
              reason: 'A second inner frame must not appear',
            );
            expect(
              pixel(72, 51) == background,
              !(!inset && depth == 2),
              reason: 'Only the raised 200% state reaches the outer corner',
            );
            if (depth > 0) {
              expect(pixel(8, 28), isNot(background));
            } else {
              expect(pixel(8, 28), background);
            }
          }
        }
      }
    },
  );
}

class _CheckerPainter extends CustomPainter {
  const _CheckerPainter(this.dark);

  final bool dark;

  @override
  void paint(Canvas canvas, Size size) {
    final base = dark ? const Color(0xFF292B36) : const Color(0xFFE0E3EA);
    final alternate = dark ? const Color(0xFF353844) : const Color(0xFFEBEDF2);
    canvas.drawRect(Offset.zero & size, Paint()..color = base);
    for (var y = 0; y < size.height; y += 12) {
      for (var x = 0; x < size.width; x += 12) {
        if (((x ~/ 12) + (y ~/ 12)).isEven) {
          canvas.drawRect(
            Rect.fromLTWH(x.toDouble(), y.toDouble(), 12, 12),
            Paint()..color = alternate,
          );
        }
      }
    }
  }

  @override
  bool shouldRepaint(covariant _CheckerPainter oldDelegate) =>
      dark != oldDelegate.dark;
}
