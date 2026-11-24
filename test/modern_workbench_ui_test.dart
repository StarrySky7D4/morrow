import 'dart:ui' show PointerDeviceKind;
import 'package:flutter/rendering.dart' show RenderBackdropFilter;
import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';
import 'package:morrow_i18n/morrow_i18n.dart';
import 'package:morrow_studio/liquid_glass.dart';
import 'package:morrow_studio/little_tips.dart';
import 'package:morrow_studio/main.dart';
import 'package:morrow_studio/plugins/workbench_ids.dart';
import 'package:morrow_studio/surface_motion.dart';
import 'package:morrow_studio/ui/command_bar.dart';
import 'package:morrow_studio/ui/workbench_rail.dart';
import 'package:morrow_studio/workspace_viewport.dart';

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
        final triggerFinder = find.byKey(
          const ValueKey('command-palette-trigger'),
        );
        if (triggerFinder.evaluate().isNotEmpty) {
          final searchCenterY = tester
              .getCenter(find.byKey(const ValueKey('header-search')))
              .dy;
          final triggerCenterY = tester.getCenter(triggerFinder).dy;
          expect((searchCenterY - triggerCenterY).abs(), lessThanOrEqualTo(2.0));
        }
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

      // 1. Collapse sidebar to icon rail and verify flat 34x34 toggle + nav switching.
      final sidebarToggle = find.byKey(const ValueKey('sidebar-toggle'));
      expect(tester.widget<InkWell>(sidebarToggle), isA<InkWell>());
      expect(
        find.ancestor(of: sidebarToggle, matching: find.byType(IconButton)),
        findsNothing,
      );
      expect(tester.getSize(sidebarToggle), const Size(34, 34));
      await tester.tap(sidebarToggle);
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

  testWidgets(
    'Corner radius propagates to FooterOverlay, SurfaceInteraction, and per-component overrides',
    (tester) async {
      tester.view.devicePixelRatio = 1;
      tester.view.physicalSize = const Size(1440, 900);
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);

      await tester.pumpWidget(const MorrowApp(initialLocale: Locale('zh')));
      await tester.pumpAndSettle();

      final studio = tester.widget<Studio>(find.byType(Studio));
      studio.onRadius(8);
      await tester.pumpAndSettle();

      final footerBox = tester.widget<DecoratedBox>(
        find
            .descendant(
              of: find.byType(FooterOverlay),
              matching: find.byType(DecoratedBox),
            )
            .first,
      );
      final footerDeco = footerBox.decoration as BoxDecoration;
      expect(
        (footerDeco.borderRadius! as BorderRadius).topLeft.x,
        closeTo(8.8, 0.001),
      );

      final gridInteraction = tester.widget<SurfaceInteraction>(
        find.byType(SurfaceInteraction).first,
      );
      expect(gridInteraction.borderRadius.topLeft.x, closeTo(8.0, 0.001));

      await tester.tap(find.byKey(const ValueKey('view-density-toggle')));
      await tester.pumpAndSettle();

      final listInteraction = tester.widget<SurfaceInteraction>(
        find.byType(SurfaceInteraction).first,
      );
      expect(listInteraction.borderRadius.topLeft.x, closeTo(6.4, 0.001));

      // Per-component cornerRadius override applies cleanly via componentBorderRadius
      const customPalette = Palette(
        StudioTheme.white,
        GlassMode.frosted,
        BackgroundMode.ambient,
        0,
        .76,
        null,
        null,
        true,
        20,
        0,
        null,
        20,
        false,
        null,
        SurfaceSettings(
          components: {'card:a': ComponentMaterial(enabled: true, cornerRadius: 4)},
        ),
      );
      expect(
        customPalette
            .componentBorderRadius(20, componentId: 'card:a')
            .topLeft
            .x,
        closeTo(4.0, 0.001),
      );
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets(
    'Glass shadows render in frosted, clear, and liquid modes while recessed stays shadowless',
    (tester) async {
      for (final mode in GlassMode.values) {
        await tester.pumpWidget(
          MaterialApp(
            home: Row(
              children: [
                Glass(
                  p: Palette(StudioTheme.white, mode),
                  child: const SizedBox(width: 80, height: 60),
                ),
                Glass(
                  p: Palette(StudioTheme.white, mode),
                  recessed: true,
                  child: const SizedBox(width: 80, height: 60),
                ),
              ],
            ),
          ),
        );
        await tester.pumpAndSettle();

        final surfaces = tester
            .widgetList<LiquidGlassSurface>(find.byType(LiquidGlassSurface))
            .toList();
        expect(surfaces.length, 2);
        expect(surfaces[0].material!.decoration.boxShadow, isNotEmpty);
        expect(surfaces[1].material!.decoration.boxShadow, isEmpty);
        expect(tester.takeException(), isNull);
      }
    },
  );

  testWidgets(
    'BackdropGroup is mounted in workbench and tiled panels opt in while dialogs/footer opt out',
    (tester) async {
      tester.view.devicePixelRatio = 1;
      tester.view.physicalSize = const Size(1440, 900);
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);

      await tester.pumpWidget(const MorrowApp(initialLocale: Locale('zh')));
      await tester.pumpAndSettle();

      expect(find.byType(BackdropGroup), findsOneWidget);

      final railSurface = tester.widget<LiquidGlassSurface>(
        find
            .descendant(
              of: find.byType(WorkbenchRail),
              matching: find.byType(LiquidGlassSurface),
            )
            .first,
      );
      expect(railSurface.useBackdropGroup, isTrue);

      await tester.pumpWidget(
        MaterialApp(
          home: BackdropGroup(
            child: Column(
              children: [
                const Glass(
                  p: Palette(StudioTheme.white, GlassMode.frosted),
                  dialog: true,
                  child: SizedBox(width: 80, height: 60),
                ),
                const Glass(
                  p: Palette(StudioTheme.white, GlassMode.frosted),
                  useBackdropGroup: false,
                  child: SizedBox(width: 80, height: 60),
                ),
              ],
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();

      final surfaces = tester
          .widgetList<LiquidGlassSurface>(find.byType(LiquidGlassSurface))
          .toList();
      expect(surfaces[0].useBackdropGroup, isFalse);
      expect(surfaces[1].useBackdropGroup, isFalse);
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets(
    'Glass blur quantization/OuterShadowPainter and WorkspaceViewport directional transitions render cleanly',
    (tester) async {
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: Center(
              child: SurfaceInteraction(
                child: Glass(
                  p: const Palette(StudioTheme.white, GlassMode.frosted),
                  child: const SizedBox(width: 140, height: 80),
                ),
              ),
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();
      expect(find.byType(BackdropFilter), findsOneWidget);

      // WorkspaceViewport keeps a single CustomScrollView without FadeTransition saveLayer
      var page = 'p0';
      var pageIndex = 0;
      late StateSetter update;
      await tester.pumpWidget(
        MaterialApp(
          home: StatefulBuilder(
            builder: (context, setState) {
              update = setState;
              return WorkspaceViewport(
                page: page,
                pageIndex: pageIndex,
                ids: const ['0', '1', '2'],
                header: const SizedBox(height: 48),
                footer: const SizedBox(height: 24),
                twoColumnWidth: 500,
                itemBuilder: (_, i) => SizedBox(height: 80, child: Text('$page-$i')),
              );
            },
          ),
        ),
      );
      await tester.pumpAndSettle();
      expect(find.byType(CustomScrollView), findsOneWidget);

      update(() {
        page = 'p2';
        pageIndex = 2;
      });
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 60));
      expect(find.byType(CustomScrollView), findsOneWidget);
      await tester.pumpAndSettle();
      expect(find.text('p2-0'), findsOneWidget);
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets(
    'Inspector tab switches animate smoothly with both outgoing and incoming bodies mounted mid-transition',
    (tester) async {
      tester.view.devicePixelRatio = 1;
      tester.view.physicalSize = const Size(1440, 900);
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);

      await tester.pumpWidget(const MorrowApp(initialLocale: Locale('zh')));
      await tester.pumpAndSettle();

      expect(find.byKey(const ValueKey('inspector-body-studio')), findsOneWidget);
      expect(find.byKey(const ValueKey('inspector-body-music')), findsNothing);

      // Switch Studio -> Music and inspect mid-transition (~60ms)
      await tester.tap(find.byKey(const ValueKey('inspector-tab-music')));
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 60));
      expect(find.byKey(const ValueKey('inspector-body-studio')), findsOneWidget);
      expect(find.byKey(const ValueKey('inspector-body-music')), findsOneWidget);

      await tester.pumpAndSettle();
      expect(find.byKey(const ValueKey('inspector-body-studio')), findsNothing);
      expect(find.byKey(const ValueKey('inspector-body-music')), findsOneWidget);

      // Switch Music -> Appearance and verify reverse-capable directional transition
      await tester.tap(find.byKey(const ValueKey('inspector-tab-appearance')));
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 60));
      expect(find.byKey(const ValueKey('inspector-body-music')), findsOneWidget);
      expect(
        find.byKey(const ValueKey('inspector-body-appearance')),
        findsOneWidget,
      );

      await tester.pumpAndSettle();
      expect(find.byKey(const ValueKey('inspector-body-music')), findsNothing);
      expect(
        find.byKey(const ValueKey('inspector-body-appearance')),
        findsOneWidget,
      );
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets(
    'Liquid Glass inside BackdropGroup coalesces resting backdrop and handles hover without layout/paint errors',
    (tester) async {
      await tester.pumpWidget(
        MaterialApp(
          home: BackdropGroup(
            child: Scaffold(
              body: Center(
                child: SizedBox(
                  width: 240,
                  height: 120,
                  child: Glass(
                    p: const Palette(StudioTheme.white, GlassMode.liquid),
                    child: const Center(child: Text('Liquid')),
                  ),
                ),
              ),
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();

      final filterRender = tester.renderObject<RenderBackdropFilter>(
        find.byType(BackdropFilter),
      );
      expect(filterRender.backdropKey, isNotNull);

      final bounds = tester.getRect(find.byType(LiquidGlassSurface));
      final gesture = await tester.createGesture(
        kind: PointerDeviceKind.mouse,
      );
      await gesture.addPointer(location: bounds.center);
      await gesture.moveTo(bounds.center + const Offset(24, 16));
      await tester.pump();
      await gesture.removePointer();
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets(
    'WorkbenchCommandBar recovery PopupMenuButton matches adjacent action icon style and opens menu',
    (tester) async {
      final search = TextEditingController();
      final focus = FocusNode();
      addTearDown(search.dispose);
      addTearDown(focus.dispose);

      const palette = Palette(StudioTheme.white, GlassMode.frosted);
      String? selectedAction;

      await tester.pumpWidget(
        MaterialApp(
          locale: const Locale('zh'),
          localizationsDelegates: const [
            L10n.delegate,
            ...GlobalMaterialLocalizations.delegates,
          ],
          supportedLocales: L10n.supportedLocales,
          home: Scaffold(
            body: Center(
              child: SizedBox(
                width: 960,
                child: Builder(
                  builder: (context) => WorkbenchCommandBar(
                    palette: palette,
                    section: WorkbenchPage.overview,
                    visibleCount: 4,
                    sidebar: true,
                    desktop: true,
                    showAppearance: true,
                    focusMode: false,
                    compactCards: false,
                    search: search,
                    searchFocus: focus,
                    onToggleAppearance: () {},
                    onToggleFocusMode: () {},
                    onToggleCompactCards: () {},
                    onOpenCommandPalette: () {},
                    onCreateIdea: () {},
                    onClearSearch: () {},
                    wrapSearchInteractions: (child) => child,
                    recoveryControl: Material(
                      type: MaterialType.transparency,
                      child: PopupMenuButton<String>(
                        key: const ValueKey('editor-recovery-open'),
                        tooltip: L10n.of(context).mainEditorRecoveryTitle,
                        borderRadius: palette.borderRadius(11),
                        shape: RoundedRectangleBorder(
                          borderRadius: palette.borderRadius(12),
                        ),
                        onSelected: (value) => selectedAction = value,
                        itemBuilder: (_) => [
                          PopupMenuItem(
                            key: const ValueKey('editor-recovery-menu-edits'),
                            value: 'edits',
                            child: Text(
                              L10n.of(context).mainEditorRecoveryTitle,
                            ),
                          ),
                        ],
                        child: WorkbenchCommandBar.actionIconBox(
                          context: context,
                          p: palette,
                          icon: Icon(
                            Icons.history_rounded,
                            size: 18,
                            color: palette.muted,
                          ),
                        ),
                      ),
                    ),
                  ),
                ),
              ),
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();

      final recoveryButton = find.byKey(const ValueKey('editor-recovery-open'));
      final focusButton = find.byKey(const ValueKey('focus-mode-toggle'));
      final appearanceButton = find.byKey(const ValueKey('appearance-toggle'));

      expect(tester.getSize(recoveryButton), const Size(36, 36));
      expect(tester.getSize(recoveryButton), tester.getSize(focusButton));
      expect(tester.getSize(recoveryButton), tester.getSize(appearanceButton));
      expect(
        find.descendant(of: recoveryButton, matching: find.byType(IconButton)),
        findsNothing,
      );

      await tester.tap(recoveryButton);
      await tester.pumpAndSettle();
      expect(
        find.byKey(const ValueKey('editor-recovery-menu-edits')),
        findsOneWidget,
      );

      await tester.tap(find.byKey(const ValueKey('editor-recovery-menu-edits')));
      await tester.pumpAndSettle();
      expect(selectedAction, 'edits');
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets(
    'BackdropFilter suspends inside inspector and route transitions and skips tiled cards on transparent dark mode',
    (tester) async {
      tester.view.devicePixelRatio = 1;
      tester.view.physicalSize = const Size(1440, 900);
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);

      await tester.pumpWidget(const MorrowApp(initialLocale: Locale('zh')));
      await tester.pumpAndSettle();

      // Switch Studio -> Music and check mid-transition (60ms): BackdropFilter suspended inside both fading bodies.
      await tester.tap(find.byKey(const ValueKey('inspector-tab-music')));
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 60));
      expect(
        find.descendant(
          of: find.byKey(const ValueKey('inspector-body-studio')),
          matching: find.byType(BackdropFilter),
        ),
        findsNothing,
      );
      expect(
        find.descendant(
          of: find.byKey(const ValueKey('inspector-body-music')),
          matching: find.byType(BackdropFilter),
        ),
        findsNothing,
      );
      await tester.pumpAndSettle();
      expect(
        find.descendant(
          of: find.byKey(const ValueKey('inspector-body-music')),
          matching: find.byType(BackdropFilter),
        ),
        findsWidgets,
      );

      // Push CanvasSettingsRoute and check mid-transition (32ms): BackdropFilter suspended inside settings-route-opacity.
      await tester.tap(find.byKey(const ValueKey('inspector-tab-appearance')));
      await tester.pumpAndSettle();
      await tester.ensureVisible(find.byKey(const ValueKey('font-settings')));
      await tester.tap(find.byKey(const ValueKey('font-settings')));
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 32));
      expect(
        find.descendant(
          of: find.byKey(const ValueKey('settings-route-opacity')).last,
          matching: find.byType(BackdropFilter),
        ),
        findsNothing,
      );
      await tester.pumpAndSettle();
      await tester.tap(find.byType(BackButton));
      await tester.pumpAndSettle();

      // Switch to dark + transparent backdrop: tiled grouped cards omit BackdropFilter, dialogs keep it.
      const darkTransparent = Palette(
        StudioTheme.dark,
        GlassMode.frosted,
        BackgroundMode.transparent,
      );
      await tester.pumpWidget(
        MaterialApp(
          home: BackdropGroup(
            child: Scaffold(
              body: Column(
                children: [
                  const Glass(
                    key: ValueKey('tiled-transparent-card'),
                    p: darkTransparent,
                    child: SizedBox(width: 120, height: 60),
                  ),
                  const Glass(
                    key: ValueKey('dialog-transparent-card'),
                    p: darkTransparent,
                    dialog: true,
                    child: SizedBox(width: 120, height: 60),
                  ),
                ],
              ),
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();
      expect(
        find.descendant(
          of: find.byKey(const ValueKey('tiled-transparent-card')),
          matching: find.byType(BackdropFilter),
        ),
        findsNothing,
      );
      expect(
        find.descendant(
          of: find.byKey(const ValueKey('dialog-transparent-card')),
          matching: find.byType(BackdropFilter),
        ),
        findsOneWidget,
      );
      expect(tester.takeException(), isNull);
    },
  );
}
