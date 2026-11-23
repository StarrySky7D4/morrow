import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';
import 'package:morrow_studio/main.dart';

void main() {
  testWidgets(
    'Modern workbench renders cleanly across desktop, tablet, and mobile breakpoints',
    (tester) async {
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);

      for (final size in const [
        Size(1440, 900),
        Size(840, 800),
        Size(390, 844),
      ]) {
        tester.view.physicalSize = size;
        await tester.pumpWidget(const MorrowApp(initialLocale: Locale('zh')));
        await tester.pumpAndSettle();

        expect(find.byKey(const ValueKey('header-search')), findsOneWidget);
        expect(find.byKey(const ValueKey('appearance-toggle')), findsOneWidget);
        expect(find.byKey(const ValueKey('header-new-idea')), findsOneWidget);
        expect(find.byKey(const ValueKey('footer-dock')), findsOneWidget);
        expect(tester.takeException(), isNull);

        await tester.pumpWidget(const SizedBox());
        await tester.pump();
      }
    },
  );

  testWidgets(
    'Desktop rail collapse, inspector tabs, view density, focus mode, and command palette work cleanly',
    (tester) async {
      tester.view.devicePixelRatio = 1;
      tester.view.physicalSize = const Size(1440, 900);
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);

      await tester.pumpWidget(const MorrowApp(initialLocale: Locale('zh')));
      await tester.pumpAndSettle();

      // 1. Collapse sidebar to icon rail and verify nav still switches pages.
      await tester.tap(find.byKey(const ValueKey('sidebar-toggle')));
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull);

      await tester.tap(find.byKey(const ValueKey('nav-workbench.page.inbox')));
      await tester.pumpAndSettle();
      expect(find.byKey(const ValueKey('inbox-list')), findsOneWidget);

      await tester.tap(
        find.byKey(const ValueKey('nav-workbench.page.overview')),
      );
      await tester.pumpAndSettle();
      expect(find.byKey(const ValueKey('overview-cards')), findsOneWidget);

      // Expand sidebar again.
      await tester.tap(find.byKey(const ValueKey('sidebar-toggle')));
      await tester.pumpAndSettle();

      // 2. Switch Inspector Drawer tabs.
      await tester.tap(find.byKey(const ValueKey('inspector-tab-music')));
      await tester.pumpAndSettle();
      expect(find.byKey(const ValueKey('music-panel')), findsOneWidget);

      await tester.tap(find.byKey(const ValueKey('inspector-tab-appearance')));
      await tester.pumpAndSettle();
      expect(find.byKey(const ValueKey('appearance-heading')), findsOneWidget);

      await tester.tap(find.byKey(const ValueKey('inspector-tab-studio')));
      await tester.pumpAndSettle();

      // 3. Toggle View Density (Grid <-> Compact List).
      await tester.tap(find.byKey(const ValueKey('view-density-toggle')));
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull);
      await tester.tap(find.byKey(const ValueKey('view-density-toggle')));
      await tester.pumpAndSettle();

      // 4. Toggle Focus Mode on and exit via floating pill.
      await tester.tap(find.byKey(const ValueKey('focus-mode-toggle')));
      await tester.pumpAndSettle();
      expect(find.byKey(const ValueKey('exit-focus-pill')), findsOneWidget);

      await tester.tap(find.byKey(const ValueKey('exit-focus-pill')));
      await tester.pumpAndSettle();
      expect(find.byKey(const ValueKey('exit-focus-pill')), findsNothing);

      // 5. Open Command Palette, filter, and navigate to Projects.
      await tester.tap(find.byKey(const ValueKey('command-palette-trigger')));
      await tester.pumpAndSettle();
      expect(
        find.byKey(const ValueKey('command-palette-dialog')),
        findsOneWidget,
      );

      await tester.enterText(
        find.byKey(const ValueKey('command-palette-input')),
        '项目',
      );
      await tester.pumpAndSettle();

      await tester.tap(
        find.byKey(const ValueKey('palette-page-workbench.page.projects')),
      );
      await tester.pumpAndSettle();
      expect(find.byKey(const ValueKey('project-board')), findsOneWidget);
      expect(tester.takeException(), isNull);
    },
  );
}
