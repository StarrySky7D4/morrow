import 'theme_plugin_fixture.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_plugin_ui/online.dart';
import 'package:morrow_studio/main.dart';
import 'package:morrow_studio/theme_plugins/theme_plugin_controller.dart';
import 'plugin_library_test.dart' as fixture;

void main() {
  testWidgets(
    'plugin management enables, replaces, disables and removes themes',
    (t) async {
      t.view.physicalSize = const Size(1440, 1000);
      t.view.devicePixelRatio = 1;
      addTearDown(t.view.reset);
      final backend = ThemeBackend();
      final theme = ThemePluginController(
        store: MemoryThemePluginStore(),
        backend: backend,
      );
      addTearDown(theme.dispose);
      await theme.restore();
      await t.pumpWidget(
        MorrowApp(
          workbench: backend,
          themePlugins: theme,
          initialLocale: const Locale('zh'),
        ),
      );
      await t.pumpAndSettle();
      await fixture.click(t, 'plugin-settings-open');
      await fixture.click(t, 'plugin-entry-$themeId');
      expect(
        find.byKey(const ValueKey('plugin-transform-$themeId')),
        findsNothing,
      );
      await fixture.click(t, 'plugin-approve-$themeId');
      expect(theme.plugin!.id, themeId);
      expect(
        find.byKey(const ValueKey('plugin-approve-$themeId')),
        findsNothing,
      );
      await fixture.click(t, 'plugin-entry-$otherThemeId');
      await fixture.click(t, 'plugin-approve-$otherThemeId');
      expect(theme.plugin!.id, otherThemeId);
      expect(
        backend.entries.where((e) => e.isTheme && e.enabled),
        hasLength(1),
      );
      await fixture.click(t, 'plugin-disable-$otherThemeId');
      expect(theme.active, isFalse);
      await fixture.click(t, 'plugin-remove-$otherThemeId');
      expect(theme.entries.map((e) => e.id), [themeId]);
      expect(backend.entries.singleWhere((e) => e.id == 'a').enabled, isTrue);
      expect(t.takeException(), isNull);
    },
  );
  testWidgets(
    'live business plugin form survives theme load, override and unload',
    (t) async {
      t.view.physicalSize = const Size(1440, 1000);
      t.view.devicePixelRatio = 1;
      addTearDown(t.view.reset);
      final backend = ThemeBackend();
      final theme = ThemePluginController(
        store: MemoryThemePluginStore(),
        backend: backend,
      );
      await theme.restore();
      addTearDown(theme.dispose);
      await t.pumpWidget(
        MorrowApp(
          workbench: backend,
          themePlugins: theme,
          initialLocale: const Locale('zh'),
        ),
      );
      await t.pumpAndSettle();
      await fixture.click(t, 'plugin-settings-open');
      await fixture.click(t, 'plugin-entry-a');
      await fixture.click(t, 'plugin-ui-a');
      final form = find.byType(ManagedPluginForm);
      final state = t.state(form);
      final field = find.descendant(of: form, matching: find.byType(TextField));
      await t.enterText(field, 'before');
      await t.pumpAndSettle();
      for (final change in [
        () => theme.activate(themeId),
        () => theme.setFullOverride(true),
        theme.deactivate,
      ]) {
        expect(await change(), isTrue);
        await t.pumpAndSettle();
        expect(identical(t.state(form), state), isTrue);
        expect(backend.views, hasLength(1));
        expect(backend.views.single.closes, 0);
        expect(t.widget<TextField>(field).controller!.text, 'BEFORE');
        await t.enterText(field, 'after');
        await t.pumpAndSettle();
        expect(t.widget<TextField>(field).controller!.text, 'AFTER');
        await t.enterText(field, 'before');
        await t.pumpAndSettle();
      }
      expect(backend.configurations, 2);
      expect(backend.imports, 0);
      expect(backend.removals, 0);
      expect(t.takeException(), isNull);
      await t.pumpWidget(const SizedBox());
      await t.pumpAndSettle();
      expect(backend.views.single.closes, 1);
    },
  );
}
