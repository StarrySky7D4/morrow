import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/main.dart';
import 'package:morrow_studio/storage.dart';

void main() {
  Palette palette(WidgetTester t) =>
      t.widget<Studio>(find.byType(Studio)).palette;
  Future<void> tap(WidgetTester t, String key) async {
    final f = find.byKey(ValueKey(key));
    await t.ensureVisible(f);
    await t.pumpAndSettle();
    await t.tap(f);
    await t.pumpAndSettle();
  }

  Future<void> setup(
    WidgetTester t,
    MemoryStorage storage, {
    double width = 1440,
  }) async {
    t.view.devicePixelRatio = 1;
    t.view.physicalSize = Size(width, 1000);
    addTearDown(t.view.resetDevicePixelRatio);
    addTearDown(t.view.resetPhysicalSize);
    await t.pumpWidget(MorrowApp(storage: storage));
    await t.pumpAndSettle();
  }

  Future<void> preview(WidgetTester t, String hex) async {
    await t.enterText(find.byKey(const ValueKey('color-hex')), hex);
    await t.tap(find.byTooltip('预览色值'));
    await t.pumpAndSettle();
  }

  testWidgets(
    'Theme preview is global, cancels safely and commits across restart',
    (t) async {
      final storage = MemoryStorage();
      await setup(t, storage);
      final original = palette(t);
      await tap(t, 'theme-color-compass');
      await preview(t, '#12785A');
      final selected = palette(t);
      expect(selected.themeColor!.toARGB32(), 0xff12785a);
      expect(selected.customColor, original.customColor);
      expect(selected.componentColor(Colors.orange), selected.accent);
      expect(selected.componentColor(Colors.blue), selected.accent);
      final scheme = t
          .widget<MaterialApp>(find.byType(MaterialApp))
          .theme!
          .colorScheme;
      expect(scheme.primary, selected.accent);
      expect(scheme.secondary, selected.accent);
      expect(scheme.tertiary, selected.accent);
      expect(scheme.onPrimary, selected.onAccent);
      // A content save during preview must keep the committed theme.
      t.widget<Studio>(find.byType(Studio)).onSave({
        'ideas': [],
        'completed': <String>[],
      });
      await t.pumpAndSettle();
      expect(storage.data!['themeColor'], isNull);
      await t.tap(find.text('取消'));
      await t.pumpAndSettle();
      expect(palette(t).themeColor, isNull);
      expect(palette(t).accent, original.accent);

      await tap(t, 'theme-color-compass');
      await preview(t, '#12785A');
      await t.tap(find.text('应用颜色'));
      await t.pumpAndSettle();
      expect(storage.data!['themeColor'], 0xff12785a);
      await tap(t, 'theme-dark');
      expect(palette(t).themeColor!.toARGB32(), 0xff12785a);
      await t.pumpWidget(const SizedBox());
      await t.pumpWidget(MorrowApp(storage: storage));
      await t.pumpAndSettle();
      expect(palette(t).themeColor!.toARGB32(), 0xff12785a);
      expect(palette(t).theme, StudioTheme.dark);
      await tap(t, 'theme-color-reset');
      expect(palette(t).themeColor, isNull);
      expect(storage.data!['themeColor'], isNull);
      expect(
        palette(t).accent,
        const Palette(StudioTheme.dark, GlassMode.frosted).accent,
      );
      expect(t.takeException(), isNull);
      await t.pumpWidget(const SizedBox());
    },
  );

  testWidgets(
    'Mobile compass validates input, previews wheel and restores on dismiss',
    (t) async {
      final storage = MemoryStorage();
      await setup(t, storage, width: 390);
      await tap(t, 'appearance-toggle');
      await tap(t, 'theme-color-compass');
      await preview(t, '#NOTHEX');
      expect(find.text('请输入 6 位十六进制色值'), findsOneWidget);
      expect(palette(t).themeColor, isNull);
      await t.tap(find.byKey(const ValueKey('color-wheel')));
      await t.pumpAndSettle();
      expect(palette(t).themeColor, isNotNull);
      final slider = t.widget<Slider>(
        find.byKey(const ValueKey('color-brightness')),
      );
      slider.onChanged!(.6);
      await t.pumpAndSettle();
      // System back / route dismissal follows the same rollback path.
      Navigator.of(t.element(find.byKey(const ValueKey('color-wheel')))).pop();
      await t.pumpAndSettle();
      expect(palette(t).themeColor, isNull);
      expect(storage.data?['themeColor'], isNull);
      expect(t.takeException(), isNull);
      await t.pumpWidget(const SizedBox());
    },
  );

  testWidgets(
    'Invalid optional theme color does not discard existing appearance',
    (t) async {
      final storage = MemoryStorage()
        ..data = {
          'version': 1,
          'theme': 'dark',
          'themeColor': 'invalid',
          'ideas': <dynamic>[],
          'completed': <String>[],
        };
      await setup(t, storage);
      expect(palette(t).theme, StudioTheme.dark);
      expect(palette(t).themeColor, isNull);
      expect(t.takeException(), isNull);
      await t.pumpWidget(const SizedBox());
    },
  );

  test(
    'Theme colors retain readable accent and label contrast across appearances',
    () {
      double contrast(Color a, Color b) {
        final x = a.computeLuminance(), y = b.computeLuminance();
        return ((x > y ? x : y) + .05) / ((x > y ? y : x) + .05);
      }

      for (final theme in StudioTheme.values) {
        for (final mode in GlassMode.values) {
          for (final canvas in BackgroundMode.values) {
            for (final lightness in [.1, .45, .5, .9]) {
              for (final seed in [
                Colors.black,
                Colors.white,
                Colors.yellow,
                Colors.red,
                Colors.green,
                Colors.blue,
              ]) {
                final p = Palette(
                  theme,
                  mode,
                  canvas,
                  0,
                  .76,
                  null,
                  null,
                  true,
                  20,
                  0,
                  lightness,
                  20,
                  false,
                  seed,
                );
                expect(
                  contrast(p.accent, p.background),
                  greaterThanOrEqualTo(4.5),
                );
                expect(
                  contrast(p.onAccent, p.accent),
                  greaterThanOrEqualTo(4.5),
                );
              }
            }
          }
        }
      }
    },
  );
}
