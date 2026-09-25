import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/color_compass.dart';
import 'package:morrow_studio/main.dart';
import 'package:morrow_studio/storage.dart';
import 'package:morrow_studio/theme_plugins/theme_plugin_controller.dart';
import 'theme_plugin_fixture.dart';

Future<void> click(WidgetTester t, String key) async {
  final f = find.byKey(ValueKey(key));
  await t.ensureVisible(f);
  await t.pumpAndSettle();
  await t.tap(f);
  await t.pumpAndSettle();
}

void main() {
  testWidgets('component compass saves its color without theme', (t) async {
    t.view.physicalSize = const Size(1440, 1000);
    t.view.devicePixelRatio = 1;
    addTearDown(t.view.reset);
    final semantics = t.ensureSemantics();
    final storage = MemoryStorage();
    await t.pumpWidget(
      MorrowApp(storage: storage, initialLocale: const Locale('zh')),
    );
    await t.pumpAndSettle();
    await click(t, 'component-settings');
    await click(t, 'component-entry:hero');
    await click(t, 'component-custom-toggle');
    await click(t, 'component-color');
    await t.enterText(find.byKey(const ValueKey('color-hex')), '#2468AB');
    await t.tap(find.text('应用颜色'));
    await t.pump();
    expect(find.byType(ColorCompassDialog), findsOneWidget);
    expect(
      t
          .widget<Glass>(find.byKey(const ValueKey('component-preview')))
          .p
          .surfaces
          .components['hero']!
          .color,
      isNull,
      reason: 'parent preview waits for native dialog subtree removal',
    );
    await t.pumpAndSettle();
    expect(find.byType(ColorCompassDialog), findsNothing);
    await click(t, 'component-apply');
    expect(
      (storage.data!['componentMaterials'] as Map)['hero']['color'],
      0xff2468ab,
    );
    expect(t.takeException(), isNull);
    semantics.dispose();
  });

  testWidgets(
    'theme hides component compass and blocks an already open picker',
    (t) async {
      t.view.physicalSize = const Size(1440, 1000);
      t.view.devicePixelRatio = 1;
      addTearDown(t.view.reset);
      final storage = MemoryStorage();
      final theme = ThemePluginController(
        store: MemoryThemePluginStore(),
        backend: ThemeBackend(),
      );
      addTearDown(theme.dispose);
      await theme.restore();
      await t.pumpWidget(
        MorrowApp(
          storage: storage,
          themePlugins: theme,
          initialLocale: const Locale('zh'),
        ),
      );
      await t.pumpAndSettle();
      await click(t, 'component-settings');
      await click(t, 'component-entry:hero');
      await click(t, 'component-custom-toggle');
      await click(t, 'component-color');
      await t.enterText(find.byKey(const ValueKey('color-hex')), '#2468AB');
      final staleApply = t
          .widget<FilledButton>(find.widgetWithText(FilledButton, '应用颜色'))
          .onPressed!;
      await theme.activate(themeId);
      await t.pumpAndSettle();
      final apply = t.widget<FilledButton>(
        find.widgetWithText(FilledButton, '应用颜色'),
      );
      expect(apply.onPressed, isNull);
      staleApply();
      await t.pumpAndSettle();
      expect(find.byType(ColorCompassDialog), findsOneWidget);
      await theme.deactivate();
      await t.pumpAndSettle();
      expect(
        t
            .widget<FilledButton>(find.widgetWithText(FilledButton, '应用颜色'))
            .onPressed,
        isNull,
        reason: 'a revoked edit cannot be committed after re-enabling controls',
      );
      await theme.activate(themeId);
      await t.pumpAndSettle();
      await t.tap(
        find.descendant(
          of: find.byType(ColorCompassDialog),
          matching: find.text('取消'),
        ),
      );
      await t.pumpAndSettle();
      expect(find.byKey(const ValueKey('component-color')), findsNothing);
      expect(
        find.byKey(const ValueKey('component-color-inherit')),
        findsNothing,
      );
      await theme.deactivate();
      await t.pumpAndSettle();
      expect(find.byKey(const ValueKey('component-color')), findsOneWidget);
      await click(t, 'component-apply');
      expect(
        (storage.data!['componentMaterials'] as Map)['hero']['color'],
        isNull,
      );
      expect(t.takeException(), isNull);
    },
  );
}
