import 'dart:io';
import 'dart:ui' as ui;
import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/main.dart';
import 'package:morrow_studio/liquid_glass.dart';
import 'package:morrow_studio/storage.dart';

void main() {
  testWidgets(
    'five experimental styles save, reopen and leave the picker collapsed',
    (t) async {
      t.view.devicePixelRatio = 1;
      t.view.physicalSize = const Size(1440, 1100);
      addTearDown(t.view.reset);
      final storage = MemoryStorage();
      for (final style in VisualStyle.values.where((s) => s.experimental)) {
        await t.pumpWidget(
          MorrowApp(initialLocale: const Locale('zh'), storage: storage),
        );
        await t.pumpAndSettle();
        final toggle = find.byKey(const ValueKey('visual-style-toggle'));
        await t.ensureVisible(toggle);
        await t.tap(toggle);
        await t.pumpAndSettle();
        final choice = find.byKey(ValueKey('visual-style-${style.name}'));
        await t.ensureVisible(choice);
        await t.tap(choice);
        await t.pumpAndSettle();
        expect(storage.data!['visualStyle'], style.name);
        expect(
          t.widget<Studio>(find.byType(Studio)).palette.surfaces.visualStyle,
          style,
        );
        expect(choice.hitTestable(), findsNothing);
        await t.pumpWidget(const SizedBox());
        await t.pumpWidget(
          MorrowApp(initialLocale: const Locale('zh'), storage: storage),
        );
        await t.pumpAndSettle();
        expect(
          t.widget<Studio>(find.byType(Studio)).palette.surfaces.visualStyle,
          style,
        );
        expect(choice.hitTestable(), findsNothing);
        expect(t.takeException(), isNull);
        await t.pumpWidget(const SizedBox());
      }
    },
  );

  testWidgets(
    'style switching preserves editor state, local glass and component radius',
    (t) async {
      var created = 0;
      var disposed = 0;
      const local = ComponentMaterial(
        enabled: true,
        mode: GlassMode.clear,
        blur: 3,
        opacity: .17,
        cornerRadius: 12,
      );
      for (final style in VisualStyle.values) {
        final p = const Palette(StudioTheme.white, GlassMode.liquid)
            .withSurfaces(
              SurfaceSettings(
                visualStyle: style,
                components: const {
                  'source': local,
                  'editor': ComponentMaterial(followComponent: 'source'),
                },
              ),
            );
        await t.pumpWidget(
          MaterialApp(
            home: Center(
              child: SizedBox(
                width: 300,
                height: 160,
                child: Glass(
                  p: p,
                  componentId: 'editor',
                  recessed: true,
                  child: _Editor(
                    onCreate: () => created++,
                    onDispose: () => disposed++,
                  ),
                ),
              ),
            ),
          ),
        );
        await t.pumpAndSettle();
        if (style == VisualStyle.flat) {
          await t.enterText(find.byType(TextField), '草稿 keeps its identity');
        }
        final field = t.widget<EditableText>(find.byType(EditableText));
        expect(field.controller.text, '草稿 keeps its identity');
        final material = t
            .widget<LiquidGlassSurface>(find.byType(LiquidGlassSurface))
            .material!;
        expect(material.blur, 3);
        expect(material.liquid, 0);
        expect(material.decoration.borderRadius, BorderRadius.circular(12));
        expect(
          (material.decoration.gradient! as LinearGradient).colors.first.a,
          closeTo(.17, .001),
        );
        expect(created, 1);
        expect(disposed, 0);
      }
      await t.pumpWidget(const SizedBox());
      expect(disposed, 1);
    },
  );

  testWidgets(
    'every experimental style preserves a zero-opacity panel interior',
    (t) async {
      t.view.devicePixelRatio = 1;
      addTearDown(t.view.reset);
      for (final style in VisualStyle.values.where((s) => s.experimental)) {
        for (final mode in GlassMode.values) {
          final key = GlobalKey();
          final p = const Palette(StudioTheme.white, GlassMode.clear)
              .withSurfaces(
                SurfaceSettings(
                  visualStyle: style,
                  components: {
                    'panel': ComponentMaterial(
                      enabled: true,
                      mode: mode,
                      blur: 0,
                      opacity: 0,
                    ),
                  },
                ),
              );
          await t.pumpWidget(
            MaterialApp(
              home: Center(
                child: RepaintBoundary(
                  key: key,
                  child: SizedBox(
                    width: 150,
                    height: 150,
                    child: Center(
                      child: Glass(
                        p: p,
                        componentId: 'panel',
                        child: const SizedBox(width: 100, height: 100),
                      ),
                    ),
                  ),
                ),
              ),
            ),
          );
          await t.pumpAndSettle();
          final boundary =
              key.currentContext!.findRenderObject()! as RenderRepaintBoundary;
          await t.runAsync(() async {
            final image = await boundary.toImage();
            final rgba = (await image.toByteData(
              format: ui.ImageByteFormat.rawRgba,
            ))!;
            // Center and near-edge samples catch outer shadows leaking through
            // a clear panel; liquid's optical rim is intentionally excluded.
            for (final y in [75, 114]) {
              expect(
                rgba.getUint8((y * image.width + 75) * 4 + 3),
                lessThan(8),
                reason: '${style.name}/${mode.name} at y=$y',
              );
            }
            image.dispose();
          });
          expect(t.takeException(), isNull);
        }
      }
    },
  );

  testWidgets(
    'experimental workbench captures render in light and dark themes',
    (t) async {
      t.view.devicePixelRatio = 1;
      t.view.physicalSize = const Size(1440, 1100);
      addTearDown(t.view.reset);
      final shadowSetting = debugDisableShadows;
      debugDisableShadows = false;
      addTearDown(() => debugDisableShadows = shadowSetting);
      for (final entry in {
        'MaterialIcons':
            'build/windows-corners/x64/runner/Release/data/flutter_assets/fonts/MaterialIcons-Regular.otf',
        'Segoe UI': 'C:/Windows/Fonts/msyh.ttc',
      }.entries) {
        final file = File(entry.value);
        if (file.existsSync()) {
          final loader = FontLoader(entry.key)
            ..addFont(
              Future.value(ByteData.sublistView(file.readAsBytesSync())),
            );
          await t.runAsync(loader.load);
        }
      }
      for (final theme in ['white', 'dark']) {
        for (final style in VisualStyle.values.where((s) => s.experimental)) {
          final storage = MemoryStorage();
          const key = ValueKey('experimental-workbench-capture');
          await t.pumpWidget(
            RepaintBoundary(
              key: key,
              child: MorrowApp(
                initialLocale: const Locale('zh'),
                storage: storage,
              ),
            ),
          );
          await t.pumpAndSettle();
          final studio = t.widget<Studio>(find.byType(Studio));
          studio.onTheme(
            theme == 'dark' ? StudioTheme.dark : StudioTheme.white,
          );
          studio.onSurfaces!(SurfaceSettings(visualStyle: style));
          await t.pumpAndSettle();
          expect(
            t.widget<Studio>(find.byType(Studio)).palette.surfaces.visualStyle,
            style,
          );
          expect(t.takeException(), isNull);
          final boundary = t.renderObject<RenderRepaintBoundary>(
            find.byKey(key),
          );
          await t.runAsync(() async {
            final image = await boundary.toImage();
            final bytes = await image.toByteData(
              format: ui.ImageByteFormat.png,
            );
            image.dispose();
            final out = File(
              'build/ui-style-preview/experimental-${style.name}-$theme.png',
            );
            await out.parent.create(recursive: true);
            await out.writeAsBytes(bytes!.buffer.asUint8List());
          });
          await t.pumpWidget(const SizedBox());
        }
      }
      debugDisableShadows = shadowSetting;
    },
    skip: !Platform.isWindows,
  );
}

class _Editor extends StatefulWidget {
  const _Editor({required this.onCreate, required this.onDispose});
  final VoidCallback onCreate, onDispose;
  @override
  State<_Editor> createState() => _EditorState();
}

class _EditorState extends State<_Editor> {
  final controller = TextEditingController();
  @override
  void initState() {
    super.initState();
    widget.onCreate();
  }

  @override
  void dispose() {
    widget.onDispose();
    controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => Material(
    color: Colors.transparent,
    child: TextField(controller: controller),
  );
}
