import 'dart:io';
import 'package:flutter/services.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/appearance.dart';
import 'package:morrow_studio/neumorphic_controls.dart';

void main() {
  final icons = File(
    'build/windows-corners/x64/runner/Release/data/flutter_assets/fonts/MaterialIcons-Regular.otf',
  );
  final font = File(r'C:\Windows\Fonts\segoeui.ttf');
  final canCapture =
      Platform.isWindows && font.existsSync() && icons.existsSync();

  setUpAll(() async {
    if (!canCapture) return;
    await (FontLoader(
          'MaterialIcons',
        )..addFont(Future.value(ByteData.sublistView(icons.readAsBytesSync()))))
        .load();
    await (FontLoader('GalleryFont')
          ..addFont(Future.value(ByteData.sublistView(font.readAsBytesSync()))))
        .load();
  });
  final surfaces = const SurfaceSettings(visualStyle: VisualStyle.neumorphism);
  final light = const Palette(
    StudioTheme.white,
    GlassMode.frosted,
  ).withSurfaces(surfaces);
  final dark = const Palette(
    StudioTheme.dark,
    GlassMode.frosted,
  ).withSurfaces(surfaces);

  Future<void> capture(
    WidgetTester tester,
    Palette palette,
    String file,
  ) async {
    tester.view.physicalSize = const Size(800, 600);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);
    final base = ThemeData(
      useMaterial3: true,
      fontFamily: 'GalleryFont',
      brightness: palette.dark ? Brightness.dark : Brightness.light,
      colorScheme: ColorScheme.fromSeed(
        seedColor: palette.accent,
        brightness: palette.dark ? Brightness.dark : Brightness.light,
      ),
    );
    await tester.pumpWidget(
      MaterialApp(
        theme: applyNeumorphicControls(base, palette),
        home: Scaffold(
          backgroundColor: palette.background,
          body: Center(
            child: RepaintBoundary(
              key: const ValueKey('gallery'),
              child: Container(
                width: 600,
                height: 510,
                color: palette.background,
                child: Padding(
                  padding: const EdgeInsets.all(24),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        palette.dark
                            ? 'Neumorphism · dark'
                            : 'Neumorphism · light',
                        style: TextStyle(
                          fontSize: 20,
                          fontWeight: FontWeight.w600,
                          color: palette.ink,
                        ),
                      ),
                      const SizedBox(height: 22),
                      Row(
                        children: [
                          NeumorphicSurface(
                            palette: palette,
                            fill: true,
                            borderRadius: palette.borderRadius(14),
                            child: const SizedBox(
                              width: 168,
                              height: 54,
                              child: Center(child: Text('Raised surface')),
                            ),
                          ),
                          const SizedBox(width: 24),
                          NeumorphicSurface(
                            palette: palette,
                            depth: -1,
                            fill: true,
                            borderRadius: palette.borderRadius(14),
                            child: const SizedBox(
                              width: 168,
                              height: 54,
                              child: Center(child: Text('Inset surface')),
                            ),
                          ),
                        ],
                      ),
                      const SizedBox(height: 24),
                      Row(
                        children: [
                          FilledButton(
                            onPressed: () {},
                            child: const Text('Primary'),
                          ),
                          const SizedBox(width: 12),
                          OutlinedButton(
                            onPressed: () {},
                            child: const Text('Secondary'),
                          ),
                          const SizedBox(width: 12),
                          FilledButton(
                            onPressed: null,
                            child: const Text('Disabled'),
                          ),
                        ],
                      ),
                      const SizedBox(height: 20),
                      const SizedBox(
                        width: 360,
                        child: TextField(
                          decoration: InputDecoration(
                            labelText: 'Recessed search field',
                            prefixIcon: Icon(Icons.search),
                          ),
                        ),
                      ),
                      const SizedBox(height: 16),
                      Row(
                        children: [
                          NeumorphicSwitch(
                            palette: palette,
                            value: false,
                            onChanged: (_) {},
                          ),
                          const SizedBox(width: 12),
                          NeumorphicSwitch(
                            palette: palette,
                            value: true,
                            onChanged: (_) {},
                          ),
                          const SizedBox(width: 12),
                          NeumorphicSwitch(
                            palette: palette,
                            value: false,
                            onChanged: null,
                          ),
                          const SizedBox(width: 24),
                          NeumorphicCheckbox(
                            palette: palette,
                            value: false,
                            onChanged: (_) {},
                          ),
                          NeumorphicCheckbox(
                            palette: palette,
                            value: true,
                            onChanged: (_) {},
                          ),
                        ],
                      ),
                      const SizedBox(height: 12),
                      Row(
                        children: [
                          NeumorphicChoiceChip(
                            palette: palette,
                            label: const Text('Unselected'),
                            selected: false,
                            onSelected: (_) {},
                          ),
                          const SizedBox(width: 12),
                          NeumorphicChoiceChip(
                            palette: palette,
                            label: const Text('Selected'),
                            selected: true,
                            onSelected: (_) {},
                          ),
                        ],
                      ),
                      const SizedBox(height: 10),
                      SizedBox(
                        width: 320,
                        child: Slider(value: .42, onChanged: (_) {}),
                      ),
                    ],
                  ),
                ),
              ),
            ),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();
    await expectLater(
      find.byKey(const ValueKey('gallery')),
      matchesGoldenFile(file),
    );
  }

  testWidgets('light control gallery', (tester) async {
    await capture(tester, light, 'goldens/neumorphic_controls_light.png');
  }, skip: !canCapture);

  testWidgets('dark control gallery', (tester) async {
    await capture(tester, dark, 'goldens/neumorphic_controls_dark.png');
  }, skip: !canCapture);
}
