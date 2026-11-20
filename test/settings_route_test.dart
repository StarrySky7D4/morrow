import 'package:material_ui/material_ui.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/settings_surface.dart';

// The outer MaterialApp owns its own route transitions. Inspect the settings Navigator.
Finder settingsSnapshots(Key navigation) => find.descendant(
  of: find.byKey(navigation),
  matching: find.byType(SnapshotWidget),
);

void main() {
  testWidgets('Settings enter immediately and return the exact edited result', (
    t,
  ) async {
    final navigation = GlobalKey<NavigatorState>();
    final controller = TextEditingController(text: 'draft');
    addTearDown(controller.dispose);
    await t.pumpWidget(
      MaterialApp(
        home: CanvasNavigation(
          navigatorKey: navigation,
          child: const SizedBox.expand(key: ValueKey('home')),
        ),
      ),
    );
    final result = navigation.currentState!.push<String>(
      CanvasSettingsRoute(
        builder: (_) => SettingsSurface(
          title: 'Settings',
          child: TextField(controller: controller),
        ),
      ),
    );
    await t.pump();
    await t.pump(const Duration(milliseconds: 16));
    final page = find.byType(SettingsSurface);
    final fade = find
        .ancestor(
          of: page,
          matching: find.byKey(const ValueKey('settings-route-opacity')),
        )
        .first;
    final early = t.widget<Opacity>(fade).opacity;
    expect(early, greaterThan(0), reason: 'no delayed entrance interval');
    expect(early, lessThan(1));
    expect(settingsSnapshots(navigation), findsNothing);
    await t.pump(const Duration(milliseconds: 48));
    expect(t.widget<Opacity>(fade).opacity, greaterThan(early));
    await t.pumpAndSettle();
    const input = TextEditingValue(
      text: 'pending draft',
      selection: TextSelection(baseOffset: 2, extentOffset: 5),
    );
    controller.value = input;
    final nested = navigation.currentState!.push<void>(
      CanvasSettingsRoute(
        builder: (_) => const SettingsSurface(
          title: 'Nested',
          backKey: ValueKey('nested-back'),
          child: SizedBox(),
        ),
      ),
    );
    await t.pump();
    await t.pump(const Duration(milliseconds: 32));
    await t.tap(find.byKey(const ValueKey('nested-back')));
    await t.pumpAndSettle();
    await nested;
    expect(controller.value, input);
    expect(find.text('Settings'), findsOneWidget);
    navigation.currentState!.pop(controller.text);
    await t.pump();
    await t.pump(const Duration(milliseconds: 32));
    expect(settingsSnapshots(navigation), findsNothing);
    await t.pumpAndSettle();
    expect(await result, input.text);
    expect(find.byKey(const ValueKey('home')), findsOneWidget);
    expect(t.takeException(), isNull);
    await t.pumpWidget(const SizedBox());
  });

  testWidgets('Reduced motion omits route effects', (t) async {
    final navigation = GlobalKey<NavigatorState>();
    await t.pumpWidget(
      MaterialApp(
        builder: (context, child) => MediaQuery(
          data: const MediaQueryData(disableAnimations: true),
          child: child!,
        ),
        home: CanvasNavigation(
          navigatorKey: navigation,
          child: const SizedBox(),
        ),
      ),
    );
    navigation.currentState!.push<void>(
      CanvasSettingsRoute(
        builder: (_) =>
            const SettingsSurface(title: 'Immediate', child: SizedBox()),
      ),
    );
    await t.pump();
    await t.pump(const Duration(milliseconds: 1));
    final page = find.byType(SettingsSurface);
    expect(page.hitTestable(), findsOneWidget);
    expect(
      find.ancestor(
        of: page,
        matching: find.byKey(const ValueKey('settings-route-opacity')),
      ),
      findsNothing,
    );
    expect(settingsSnapshots(navigation), findsNothing);
    navigation.currentState!.pop();
    await t.pumpAndSettle();
    expect(t.takeException(), isNull);
    await t.pumpWidget(const SizedBox());
  });
}
