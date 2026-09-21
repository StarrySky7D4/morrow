import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/main.dart';
import 'package:morrow_studio/component_material_page.dart';
import 'package:morrow_studio/little_tips.dart';
import 'package:morrow_studio/settings_surface.dart';
import 'package:morrow_studio/storage.dart';

Future<void> tapKey(WidgetTester t, String key) async {
  final f = find.byKey(ValueKey(key));
  await t.ensureVisible(f);
  await t.pumpAndSettle();
  await t.tap(f);
  await t.pumpAndSettle();
}

void main() {
  testWidgets('Large component library stays lazy and edits its last card', (
    t,
  ) async {
    t.view.devicePixelRatio = 1;
    t.view.physicalSize = const Size(1180, 820);
    addTearDown(t.view.reset);
    final storage = MemoryStorage()
      ..data = {
        'theme': 'white',
        'glass': 'frosted',
        'background': 'ambient',
        'ideas': List.generate(
          300,
          (i) => Idea(
            'Card $i',
            'Content',
            '灵感',
            Icons.star,
            Colors.blue,
          ).toJson(),
        ),
      };
    await t.pumpWidget(MorrowApp(storage: storage));
    await t.pumpAndSettle();
    await tapKey(t, 'component-settings');
    final page = t.widget<ComponentMaterialListPage>(
      find.byType(ComponentMaterialListPage),
    );
    expect(page.entries.length, greaterThanOrEqualTo(300));
    expect(find.byType(ListTile).evaluate().length, lessThan(20));
    final id = page.entries.keys.last;
    final entry = find.byKey(ValueKey('component-entry:$id'));
    final scroll = find.descendant(
      of: find.byKey(const PageStorageKey('component-list-scroll')),
      matching: find.byType(Scrollable),
    );
    await t.scrollUntilVisible(entry, 700, scrollable: scroll, maxScrolls: 100);
    await t.pumpAndSettle();
    final offset = t.state<ScrollableState>(scroll).position.pixels;
    await tapKey(t, 'component-entry:$id');
    await tapKey(t, 'component-custom-toggle');
    t
        .widget<Slider>(find.byKey(const ValueKey('component-opacity')))
        .onChanged!(.37);
    await t.pumpAndSettle();
    await tapKey(t, 'component-apply');
    expect((storage.data!['componentMaterials'] as Map)[id]['opacity'], .37);
    expect(
      t.state<ScrollableState>(scroll).position.pixels,
      closeTo(offset, 1),
    );
    expect(entry, findsOneWidget);
    expect(t.takeException(), isNull);
    await t.pumpWidget(const SizedBox());
  });

  testWidgets('Floating tips overlap the viewport and material is opt-in', (
    t,
  ) async {
    t.view.devicePixelRatio = 1;
    t.view.physicalSize = const Size(800, 800);
    addTearDown(t.view.reset);
    await t.pumpWidget(MorrowApp(storage: MemoryStorage()));
    await t.pumpAndSettle();
    final footer = find.byType(FooterOverlay);
    final page = find.byKey(const ValueKey('page-workbench.page.overview'));
    expect(t.getRect(page).overlaps(t.getRect(footer)), isTrue);
    expect(
      t.getRect(page).bottom,
      greaterThanOrEqualTo(t.getRect(footer).bottom),
    );
    expect(
      find.descendant(of: footer, matching: find.byType(Glass)),
      findsNothing,
    );
    final studio = t.widget<Studio>(find.byType(Studio));
    studio.onSurfaces!(
      studio.palette.surfaces.copyWith(
        components: {
          'footer': const ComponentMaterial(
            enabled: true,
            opacity: .3,
            blur: 4,
          ),
        },
      ),
    );
    await t.pumpAndSettle();
    expect(
      find.descendant(of: footer, matching: find.byType(Glass)),
      findsOneWidget,
    );
    expect(
      t.getRect(page).bottom,
      greaterThanOrEqualTo(t.getRect(footer).bottom),
    );
    expect(t.takeException(), isNull);
    await t.pumpWidget(const SizedBox());
  });

  testWidgets(
    'Component pages retain the canvas and react to live theme changes',
    (t) async {
      t.view.devicePixelRatio = 1;
      t.view.physicalSize = const Size(390, 740);
      addTearDown(t.view.reset);
      await t.pumpWidget(
        MorrowApp(initialLocale: const Locale('zh'), storage: MemoryStorage()),
      );
      await t.pumpAndSettle();
      await tapKey(t, 'appearance-toggle');
      await tapKey(t, 'component-settings');
      expect(find.byType(ComponentMaterialListPage), findsOneWidget);
      expect(find.byKey(const ValueKey('compact-settings-page')), findsNothing);
      final studio = t.widget<Studio>(find.byType(Studio));
      studio.onTheme(StudioTheme.dark);
      studio.onBackground(BackgroundMode.transparent);
      studio.onSurfaces!(
        studio.palette.surfaces.copyWith(
          canvasOpacity: .23,
          canvasColor: Colors.teal,
        ),
      );
      await t.pumpAndSettle();
      final listContext = t.element(find.byType(SettingsSurface));
      expect(AppearanceScope.of(listContext).dark, isTrue);
      expect(
        AppearanceScope.of(listContext).backdrop,
        BackgroundMode.transparent,
      );
      expect(
        find.byKey(const ValueKey('transparent-canvas-tint')),
        findsOneWidget,
      );
      expect(
        t
            .widget<Scaffold>(
              find.descendant(
                of: find.byType(SettingsSurface),
                matching: find.byType(Scaffold),
              ),
            )
            .backgroundColor,
        Colors.transparent,
      );
      await tapKey(t, 'component-entry:hero');
      expect(find.byType(ComponentMaterialPage), findsOneWidget);
      final page = find.byType(SettingsSurface);
      expect(AppearanceScope.of(t.element(page)).dark, isTrue);
      final header = find.byType(SettingsPageHeader);
      expect(header, findsOneWidget);
      expect(
        find.descendant(of: header, matching: find.byType(Glass)),
        findsNothing,
      );
      expect(
        t
            .widget<Text>(
              find.descendant(of: header, matching: find.byType(Text)),
            )
            .style!
            .color,
        AppearanceScope.of(t.element(page)).ink,
      );
      await tapKey(t, 'component-custom-toggle');
      t
          .widget<Slider>(find.byKey(const ValueKey('component-opacity')))
          .onChanged!(.27);
      await t.pumpAndSettle();
      // Changing global material must not discard a component's pending draft.
      t.widget<Studio>(find.byType(Studio)).onMode(GlassMode.liquid);
      await t.pumpAndSettle();
      expect(AppearanceScope.of(t.element(page)).mode, GlassMode.liquid);
      expect(
        t.widget<Slider>(find.byKey(const ValueKey('component-opacity'))).value,
        .27,
      );
      await t.binding.handlePopRoute();
      await t.pumpAndSettle();
      expect(find.byType(ComponentMaterialListPage), findsOneWidget);
      expect(find.byKey(const ValueKey('compact-settings-page')), findsNothing);
      await t.sendKeyEvent(LogicalKeyboardKey.escape);
      await t.pumpAndSettle();
      expect(
        find.byKey(const ValueKey('compact-settings-page')),
        findsOneWidget,
      );
      await t.binding.handlePopRoute();
      await t.pumpAndSettle();
      expect(
        find.byKey(const ValueKey('page-workbench.page.overview')),
        findsOneWidget,
      );
      expect(t.takeException(), isNull);
      await t.pumpWidget(const SizedBox());
    },
  );

  testWidgets(
    'Bottom errors can be dismissed without invoking the retry action',
    (t) async {
      await t.pumpWidget(MorrowApp(storage: MemoryStorage()));
      await t.pumpAndSettle();
      var retried = false;
      ScaffoldMessenger.of(t.element(find.byType(Studio))).showSnackBar(
        SnackBar(
          content: const Text('test error'),
          duration: const Duration(minutes: 1),
          action: SnackBarAction(
            label: 'Retry',
            onPressed: () => retried = true,
          ),
        ),
      );
      await t.pumpAndSettle();
      final dismiss = find.descendant(
        of: find.byType(SnackBar),
        matching: find.byType(IconButton),
      );
      expect(dismiss, findsOneWidget);
      await t.tap(dismiss);
      await t.pumpAndSettle();
      expect(find.text('test error'), findsNothing);
      expect(retried, isFalse);
      await t.pumpWidget(const SizedBox());
    },
  );
}
