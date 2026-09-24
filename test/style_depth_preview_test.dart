import 'dart:io';
import 'dart:ui' as ui;
import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/appearance.dart';
import 'package:morrow_studio/experimental_controls.dart';

void main() {
  testWidgets(
    'render six styles at zero standard and double relief',
    (t) async {
      t.view.physicalSize = const Size(1000, 850);
      t.view.devicePixelRatio = 1;
      addTearDown(t.view.reset);
      final font = File(r'C:\Windows\Fonts\segoeui.ttf');
      await (FontLoader('DepthPreview')..addFont(
            Future.value(ByteData.sublistView(font.readAsBytesSync())),
          ))
          .load();
      const capture = ValueKey('depth-capture');
      await t.pumpWidget(
        MaterialApp(
          theme: ThemeData(fontFamily: 'DepthPreview'),
          home: Scaffold(
            body: RepaintBoundary(
              key: capture,
              child: Container(
                color: const Color(0xffeeedf2),
                padding: const EdgeInsets.all(24),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
                    const Text(
                      'Morrow / Relief depth',
                      style: TextStyle(
                        fontFamily: 'DepthPreview',
                        fontSize: 24,
                      ),
                    ),
                    const SizedBox(height: 12),
                    const Row(
                      children: [
                        SizedBox(width: 110),
                        Expanded(child: Center(child: Text('0%'))),
                        Expanded(child: Center(child: Text('100%'))),
                        Expanded(child: Center(child: Text('200%'))),
                      ],
                    ),
                    for (final style in VisualStyle.values.where(
                      (s) => s.supportsDepth,
                    ))
                      Expanded(
                        child: Row(
                          children: [
                            SizedBox(
                              width: 110,
                              child: Text(
                                style.name,
                                style: const TextStyle(
                                  fontFamily: 'DepthPreview',
                                ),
                              ),
                            ),
                            for (final depth in [0.0, 1.0, 2.0])
                              Expanded(
                                child: Padding(
                                  padding: const EdgeInsets.all(12),
                                  child: Builder(
                                    builder: (context) {
                                      final p =
                                          const Palette(
                                            StudioTheme.white,
                                            GlassMode.frosted,
                                          ).withSurfaces(
                                            SurfaceSettings(
                                              visualStyle: style,
                                              styleDepth: depth,
                                            ),
                                          );
                                      return Theme(
                                        data: applyVisualStyleControls(
                                          ThemeData(fontFamily: 'DepthPreview'),
                                          p,
                                        ),
                                        child: Glass(
                                          p: p,
                                          child: Padding(
                                            padding: const EdgeInsets.all(12),
                                            child: Row(
                                              children: [
                                                Expanded(
                                                  child: FilledButton(
                                                    onPressed: () {},
                                                    child: const Text('Action'),
                                                  ),
                                                ),
                                                const SizedBox(width: 8),
                                                Expanded(
                                                  child: TextField(
                                                    decoration:
                                                        const InputDecoration(
                                                          hintText: 'Input',
                                                          isDense: true,
                                                        ),
                                                  ),
                                                ),
                                              ],
                                            ),
                                          ),
                                        ),
                                      );
                                    },
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
      await t.pumpAndSettle();
      expect(t.takeException(), isNull);
      await t.runAsync(() async {
        final image = await t
            .renderObject<RenderRepaintBoundary>(find.byKey(capture))
            .toImage(pixelRatio: 1);
        final bytes = await image.toByteData(format: ui.ImageByteFormat.png);
        final output = File(
          'build/ui-style-preview/style-depth-comparison.png',
        );
        await output.parent.create(recursive: true);
        await output.writeAsBytes(bytes!.buffer.asUint8List());
        image.dispose();
      });
    },
    skip: !Platform.isWindows,
  );
}
