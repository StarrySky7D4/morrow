import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:morrow_i18n/morrow_i18n.dart';
import 'package:morrow_studio/appearance.dart';
import 'package:morrow_studio/visual_style_picker.dart';

void main() {
  test(
    'all bundled locales name and describe the five experimental styles',
    () {
      for (final locale in L10n.supportedLocales) {
        final l = L10n.forLocale(locale);
        expect(l.mainStyleExperimental, isNotEmpty, reason: locale.toString());
        for (final value in [
          l.mainStylePaper,
          l.mainStylePaperDescription,
          l.mainStyleClay,
          l.mainStyleClayDescription,
          l.mainStyleFluent,
          l.mainStyleFluentDescription,
          l.mainStyleBrutalist,
          l.mainStyleBrutalistDescription,
          l.mainStyleIndustrial,
          l.mainStyleIndustrialDescription,
        ]) {
          expect(value, isNotEmpty, reason: locale.toString());
        }
      }
    },
  );

  testWidgets('seven choices stay collapsed and selection updates semantics', (
    tester,
  ) async {
    var selected = VisualStyle.flat;
    const base = Palette(StudioTheme.white, GlassMode.frosted);

    await tester.pumpWidget(
      MaterialApp(
        locale: const Locale('en'),
        supportedLocales: L10n.supportedLocales,
        localizationsDelegates: const [
          L10n.delegate,
          GlobalMaterialLocalizations.delegate,
          GlobalWidgetsLocalizations.delegate,
          GlobalCupertinoLocalizations.delegate,
        ],
        home: StatefulBuilder(
          builder: (context, update) => Scaffold(
            body: SingleChildScrollView(
              child: SizedBox(
                width: 440,
                child: VisualStylePicker(
                  palette: base.withSurfaces(
                    SurfaceSettings(visualStyle: selected),
                  ),
                  onChanged: (style) => update(() => selected = style),
                ),
              ),
            ),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();

    for (final style in VisualStyle.values) {
      final choice = find.byKey(ValueKey('visual-style-${style.name}'));
      expect(choice.hitTestable(), findsNothing);
      await tester.tap(find.byKey(const ValueKey('visual-style-toggle')));
      await tester.pumpAndSettle();
      expect(choice, findsOneWidget);
      expect(
        find.byKey(ValueKey('visual-style-preview-${style.name}')),
        findsOneWidget,
      );
      await tester.ensureVisible(choice);
      await tester.tap(choice);
      await tester.pumpAndSettle();
      expect(selected, style);
      expect(choice.hitTestable(), findsNothing);
      final selectedNodes = find.byWidgetPredicate(
        (widget) =>
            widget is Semantics &&
            widget.properties.selected == true &&
            widget.properties.label != null &&
            widget.properties.label!.contains(switch (style) {
              VisualStyle.flat => 'Flat',
              VisualStyle.neumorphism => 'Neumorphism',
              VisualStyle.paper => 'Paper',
              VisualStyle.clay => 'Clay',
              VisualStyle.fluent => 'Fluent',
              VisualStyle.brutalist => 'Brutalist',
              VisualStyle.industrial => 'Industrial',
            }),
      );
      expect(selectedNodes, findsOneWidget);
    }
    expect(tester.takeException(), isNull);
  });

  testWidgets('narrow layout and large text keep all choices usable', (
    tester,
  ) async {
    tester.view.physicalSize = const Size(250, 1000);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    VisualStyle? selection;
    await tester.pumpWidget(
      MaterialApp(
        locale: const Locale('zh'),
        supportedLocales: L10n.supportedLocales,
        localizationsDelegates: const [
          L10n.delegate,
          GlobalMaterialLocalizations.delegate,
          GlobalWidgetsLocalizations.delegate,
          GlobalCupertinoLocalizations.delegate,
        ],
        home: MediaQuery(
          data: const MediaQueryData(
            size: Size(250, 1000),
            textScaler: TextScaler.linear(2),
          ),
          child: Scaffold(
            body: SingleChildScrollView(
              child: VisualStylePicker(
                palette: const Palette(StudioTheme.dark, GlassMode.clear),
                onChanged: (style) => selection = style,
              ),
            ),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.byKey(const ValueKey('visual-style-toggle')));
    await tester.pumpAndSettle();
    final industrial = find.byKey(const ValueKey('visual-style-industrial'));
    await tester.ensureVisible(industrial);
    await tester.pumpAndSettle();
    await tester.tap(industrial);
    await tester.pumpAndSettle();
    expect(selection, VisualStyle.industrial);
    expect(tester.takeException(), isNull);
  });
}
