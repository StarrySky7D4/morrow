import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/main.dart';
import 'package:morrow_studio/component_material_page.dart';
import 'package:morrow_studio/liquid_glass.dart';
import 'package:morrow_studio/settings_surface.dart';
import 'package:morrow_studio/storage.dart';
import 'settings_surface_test.dart' show tapKey;

void main() {
  testWidgets(
    'Component draft adapts to resize, applies locally and persists on reload',
    (t) async {
      t.view.devicePixelRatio = 1;
      t.view.physicalSize = const Size(1200, 850);
      addTearDown(t.view.reset);
      final storage = MemoryStorage();
      await t.pumpWidget(MorrowApp(storage: storage));
      await t.pumpAndSettle();
      expect(find.byType(Scrollbar), findsNothing);
      await tapKey(t, 'component-settings');
      expect(t.getSize(find.byType(SettingsSurface)).width, 1200);
      await tapKey(t, 'component-entry:hero');
      await tapKey(t, 'component-custom-toggle');
      await tapKey(t, 'component-mode-liquid');
      t
          .widget<Slider>(find.byKey(const ValueKey('component-radius')))
          .onChanged!(12);
      t
          .widget<Slider>(find.byKey(const ValueKey('component-opacity')))
          .onChanged!(.21);
      await t.pumpAndSettle();
      for (final width in [320.0, 1540.0, 700.0]) {
        t.view.physicalSize = Size(width, 850);
        await t.pumpAndSettle();
        expect(t.getSize(find.byType(SettingsSurface)).width, width);
        expect(
          t
              .widget<Slider>(find.byKey(const ValueKey('component-radius')))
              .value,
          12,
        );
        expect(
          t
              .widget<ChoiceChip>(
                find.descendant(
                  of: find.byKey(const ValueKey('component-mode-liquid')),
                  matching: find.byType(ChoiceChip),
                ),
              )
              .selected,
          isTrue,
        );
        expect(find.byType(Scrollbar), findsNothing);
        expect(t.takeException(), isNull);
      }
      final surface = t.widget<LiquidGlassSurface>(
        find.descendant(
          of: find.byKey(const ValueKey('component-preview')),
          matching: find.byType(LiquidGlassSurface),
        ),
      );
      expect(surface.material!.liquid, 1);
      expect(surface.borderRadius.topLeft.x, 12);
      await tapKey(t, 'component-apply');
      final entry = (storage.data!['componentMaterials'] as Map)['hero'] as Map;
      expect(entry['mode'], 'liquid');
      expect(entry['cornerRadius'], 12);
      expect(entry['opacity'], .21);
      await t.pumpWidget(const SizedBox());
      await t.pumpWidget(MorrowApp(storage: storage));
      await t.pumpAndSettle();
      final p = t.widget<Studio>(find.byType(Studio)).palette;
      expect(p.surfaces.components['hero']!.mode, GlassMode.liquid);
      expect(p.surfaces.components['hero']!.cornerRadius, 12);
      expect(p.surfaces.components['music'], isNull);
      await tapKey(t, 'appearance-toggle');
      await tapKey(t, 'component-settings');
      await tapKey(t, 'component-entry:hero');
      await tapKey(t, 'component-reset');
      expect(
        t
            .widget<Slider>(find.byKey(const ValueKey('component-opacity')))
            .onChanged,
        isNull,
      );
      await tapKey(t, 'component-apply');
      expect(
        (storage.data!['componentMaterials'] as Map)['hero']['enabled'],
        isFalse,
      );
      expect(find.byType(ComponentMaterialListPage), findsOneWidget);
      await t.pumpWidget(const SizedBox());
    },
    variant: TargetPlatformVariant.only(TargetPlatform.windows),
  );
  testWidgets(
    'Desktop settings expand, fill the window and remain open across resize',
    (t) async {
      t.view.devicePixelRatio = 1;
      t.view.physicalSize = const Size(1540, 850);
      addTearDown(t.view.reset);
      await t.pumpWidget(MorrowApp(storage: MemoryStorage()));
      await t.pumpAndSettle();
      final expand = find.byKey(const ValueKey('settings-expand'));
      final heading = find.byKey(const ValueKey('appearance-heading'));
      final title = find.byKey(const ValueKey('appearance-heading-title'));
      final leftIcon = find.byKey(const ValueKey('appearance-heading-icon'));
      final rightIcon = find.descendant(
        of: expand,
        matching: find.byType(Icon),
      );
      for (final width in [1180.0, 1540.0]) {
        t.view.physicalSize = Size(width, 850);
        await t.pumpAndSettle();
        expect(
          t.getRect(title).left,
          closeTo(t.getRect(heading).left + 48, .1),
        );
        expect(t.getRect(expand).right, closeTo(t.getRect(heading).right, .1));
        expect(
          t.getCenter(leftIcon).dy,
          closeTo(t.getCenter(rightIcon).dy, .1),
        );
        expect(
          t.getCenter(leftIcon).dx - t.getRect(heading).left,
          closeTo(t.getRect(heading).right - t.getCenter(rightIcon).dx, .1),
        );
      }
      await tapKey(t, 'settings-expand');
      expect(
        find.byKey(const ValueKey('page-workbench.page.overview')),
        findsNothing,
      );
      final settings = find.byKey(const ValueKey('compact-settings-page'));
      expect(t.getSize(settings).width, 1540);
      expect(find.byType(Scrollbar), findsNothing);
      for (final width in [390.0, 700.0, 1540.0, 2560.0]) {
        t.view.physicalSize = Size(width, 850);
        await t.pumpAndSettle();
        expect(t.getSize(settings).width, width);
        expect(
          t.getRect(title).left,
          closeTo(t.getRect(heading).left + 48, .1),
        );
        final column = t.getRect(
          find.byKey(const ValueKey('settings-content-column')),
        );
        expect(column.width, lessThanOrEqualTo(920));
        expect(column.center.dx, closeTo(width / 2, .1));
        if (width < 920) {
          expect(column.width, width - (width < 600 ? 24 : 48));
        }
        final header = find.byType(SettingsPageHeader);
        expect(header, findsOneWidget);
        expect(
          find.descendant(of: header, matching: find.byType(Glass)),
          findsNothing,
        );
        final sections = t.widget<SettingsSections>(
          find.byType(SettingsSections).first,
        );
        final first = t.getRect(find.byWidget(sections.children.first));
        final second = t.getRect(find.byWidget(sections.children[1]));
        expect(second.left, first.left);
        expect(second.width, first.width);
        expect(second.top, greaterThanOrEqualTo(first.bottom + 19));
        expect(
          find.byKey(const ValueKey('theme-color-compass')),
          findsOneWidget,
        );
        expect(t.takeException(), isNull);
      }
      await tapKey(t, 'compact-settings-back');
      expect(
        find.byKey(const ValueKey('page-workbench.page.overview')),
        findsOneWidget,
      );
      await t.pumpWidget(const SizedBox());
    },
    variant: TargetPlatformVariant.only(TargetPlatform.windows),
  );
}
