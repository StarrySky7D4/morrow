import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/main.dart';
import 'package:morrow_studio/storage.dart';

void main() {
  Future<void> tap(WidgetTester t, String key) async {
    final f = find.byKey(ValueKey(key));
    await t.ensureVisible(f);
    await t.pumpAndSettle();
    await t.tap(f);
    await t.pumpAndSettle();
  }

  void size(WidgetTester t, double width) {
    t.view.devicePixelRatio = 1;
    t.view.physicalSize = Size(width, 700);
  }

  testWidgets(
    'Narrow settings start closed and keep content and settings exclusive',
    (t) async {
      addTearDown(t.view.reset);
      for (final width in [390.0, 800.0, 1049.0]) {
        size(t, width);
        final storage = MemoryStorage()
          ..data = {
            'version': 1,
            'theme': 'white',
            'glass': 'frosted',
            'background': 'ambient',
            'appearanceExpanded': true,
            'ideas': <dynamic>[],
            'completed': <String>[],
          };
        await t.pumpWidget(MorrowApp(storage: storage));
        await t.pumpAndSettle();
        final page = find.byKey(const ValueKey('page-概览'));
        expect(page, findsOneWidget);
        expect(
          find.byKey(const ValueKey('compact-settings-page')),
          findsNothing,
        );
        expect(find.byKey(const ValueKey('theme-color-compass')), findsNothing);
        final field = find.descendant(
          of: find.byKey(const ValueKey('header-search')),
          matching: find.byType(TextField),
        );
        await t.enterText(field, '保留搜索');
        await t.pumpAndSettle();
        final scroll = t
            .state<ScrollableState>(
              find
                  .descendant(of: page, matching: find.byType(Scrollable))
                  .first,
            )
            .position;
        scroll.jumpTo(scroll.maxScrollExtent / 2);
        await t.pumpAndSettle();
        final offset = scroll.pixels;
        await tap(t, 'appearance-toggle');
        expect(page, findsNothing);
        expect(find.byKey(const ValueKey('header-search')), findsNothing);
        expect(find.byKey(const ValueKey('footer-dock')), findsNothing);
        expect(
          find.byKey(const ValueKey('compact-settings-page')),
          findsOneWidget,
        );
        await tap(t, 'theme-dark');
        expect(storage.data!['theme'], 'dark');
        expect(storage.data!['appearanceExpanded'], isTrue);
        await tap(t, 'compact-settings-back');
        expect(page, findsOneWidget);
        expect(t.widget<TextField>(field).controller!.text, '保留搜索');
        expect(scroll.pixels, closeTo(offset, .1));
        await tap(t, 'appearance-toggle');
        await t.pumpWidget(const SizedBox());
        await t.pumpWidget(MorrowApp(storage: storage));
        await t.pumpAndSettle();
        expect(
          find.byKey(const ValueKey('compact-settings-page')),
          findsNothing,
        );
        expect(page, findsOneWidget);
        expect(t.takeException(), isNull);
        await t.pumpWidget(const SizedBox());
      }
    },
  );

  testWidgets('System back dismisses the color dialog before narrow settings', (
    t,
  ) async {
    addTearDown(t.view.reset);
    size(t, 390);
    await t.pumpWidget(MorrowApp(storage: MemoryStorage()));
    await t.pumpAndSettle();
    await tap(t, 'appearance-toggle');
    await tap(t, 'theme-color-compass');
    await t.binding.handlePopRoute();
    await t.pumpAndSettle();
    expect(find.byKey(const ValueKey('color-wheel')), findsNothing);
    expect(find.byKey(const ValueKey('compact-settings-page')), findsOneWidget);
    await t.binding.handlePopRoute();
    await t.pumpAndSettle();
    expect(find.byKey(const ValueKey('compact-settings-page')), findsNothing);
    expect(find.byKey(const ValueKey('page-概览')), findsOneWidget);
    await tap(t, 'appearance-toggle');
    await t.sendKeyEvent(LogicalKeyboardKey.escape);
    await t.pumpAndSettle();
    expect(find.byKey(const ValueKey('compact-settings-page')), findsNothing);
    expect(t.takeException(), isNull);
    await t.pumpWidget(const SizedBox());
  });

  testWidgets(
    'Resize preserves desktop preference and resets narrow settings navigation',
    (t) async {
      addTearDown(t.view.reset);
      final storage = MemoryStorage();
      size(t, 1440);
      await t.pumpWidget(MorrowApp(storage: storage));
      await t.pumpAndSettle();
      await tap(t, 'appearance-toggle');
      expect(storage.data!['appearanceExpanded'], isFalse);
      size(t, 800);
      await t.pumpAndSettle();
      expect(find.byKey(const ValueKey('compact-settings-page')), findsNothing);
      await tap(t, 'appearance-toggle');
      await tap(t, 'theme-dark');
      expect(storage.data!['appearanceExpanded'], isFalse);
      size(t, 1050);
      await t.pumpAndSettle();
      expect(find.byKey(const ValueKey('compact-settings-page')), findsNothing);
      expect(
        t.getSize(find.byKey(const ValueKey('settings-side-panel'))).width,
        0,
      );
      size(t, 390);
      await t.pumpAndSettle();
      expect(find.byKey(const ValueKey('theme-color-compass')), findsNothing);
      expect(find.byKey(const ValueKey('page-概览')), findsOneWidget);
      expect(t.takeException(), isNull);
      await t.pumpWidget(const SizedBox());
    },
  );

  testWidgets(
    'Settings fade through without overlap in both directions and reverse safely',
    (t) async {
      addTearDown(t.view.reset);
      size(t, 390);
      await t.pumpWidget(MorrowApp(storage: MemoryStorage()));
      await t.pumpAndSettle();
      final content = find.byKey(const ValueKey('page-概览'));
      final settings = find.byKey(const ValueKey('compact-settings-page'));
      double opacity(String key) =>
          t.widget<Opacity>(find.byKey(ValueKey(key))).opacity;
      Future<void> openStart() async {
        await t.tap(find.byKey(const ValueKey('appearance-toggle')));
        await t.pump();
        await t.pump(const Duration(milliseconds: 80));
      }

      await openStart();
      expect(content, findsOneWidget);
      expect(settings, findsNothing);
      expect(opacity('workspace-transition-opacity'), inExclusiveRange(0, 1));
      await t.binding.handlePopRoute();
      await t.pumpAndSettle();
      expect(content, findsOneWidget);
      expect(settings, findsNothing);

      await openStart();
      await t.pump(const Duration(milliseconds: 160));
      expect(content, findsNothing);
      expect(settings, findsOneWidget);
      expect(opacity('settings-transition-opacity'), inExclusiveRange(0, 1));
      await t.pumpAndSettle();
      expect(find.byTooltip('关闭设置'), findsNothing);
      expect(find.byTooltip('返回工作台'), findsOneWidget);

      await t.tap(find.byKey(const ValueKey('compact-settings-back')));
      await t.pump();
      await t.pump(const Duration(milliseconds: 80));
      expect(content, findsNothing);
      expect(settings, findsOneWidget);
      expect(opacity('settings-transition-opacity'), inExclusiveRange(0, 1));
      await t.pump(const Duration(milliseconds: 160));
      expect(settings, findsNothing);
      expect(content, findsOneWidget);
      expect(opacity('workspace-transition-opacity'), inExclusiveRange(0, 1));
      await t.pumpAndSettle();
      expect(opacity('workspace-transition-opacity'), 1);
      expect(t.takeException(), isNull);
      await t.pumpWidget(const SizedBox());
    },
  );

  testWidgets(
    'Reduced motion switches settings immediately and can finish an active transition',
    (t) async {
      addTearDown(t.view.reset);
      addTearDown(t.platformDispatcher.clearAccessibilityFeaturesTestValue);
      size(t, 390);
      await t.pumpWidget(MorrowApp(storage: MemoryStorage()));
      await t.pumpAndSettle();
      await t.tap(find.byKey(const ValueKey('appearance-toggle')));
      await t.pump();
      await t.pump(const Duration(milliseconds: 80));
      t.platformDispatcher.accessibilityFeaturesTestValue =
          const FakeAccessibilityFeatures(disableAnimations: true);
      await t.pump();
      expect(find.byKey(const ValueKey('page-概览')), findsNothing);
      expect(
        t
            .widget<Opacity>(
              find.byKey(const ValueKey('settings-transition-opacity')),
            )
            .opacity,
        1,
      );
      await t.pumpAndSettle();
      await t.tap(find.byKey(const ValueKey('compact-settings-back')));
      await t.pump();
      expect(find.byKey(const ValueKey('compact-settings-page')), findsNothing);
      expect(
        t
            .widget<Opacity>(
              find.byKey(const ValueKey('workspace-transition-opacity')),
            )
            .opacity,
        1,
      );
      expect(t.takeException(), isNull);
      await t.pumpWidget(const SizedBox());
    },
  );
}
