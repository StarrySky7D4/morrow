import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/appearance.dart';
import 'package:morrow_studio/experimental_controls.dart';
import 'package:morrow_studio/neumorphic_controls.dart';

void main() {
  Widget host(
    VisualStyle style, {
    bool reduced = false,
    bool visible = true,
    WidgetStateProperty<Icon?>? custom,
  }) {
    final p = const Palette(
      StudioTheme.white,
      GlassMode.clear,
    ).withSurfaces(SurfaceSettings(visualStyle: style));
    return MaterialApp(
      theme: applyVisualStyleControls(ThemeData(), p),
      home: MediaQuery(
        data: MediaQueryData(disableAnimations: reduced),
        child: TickerMode(
          enabled: visible,
          child: Scaffold(
            body: NeumorphicSwitch(
              palette: p,
              value: false,
              onChanged: (_) {},
              thumbIcon: custom,
            ),
          ),
        ),
      ),
    );
  }

  Icon icon(WidgetTester t, [Set<WidgetState> states = const {}]) =>
      t.widget<Switch>(find.byType(Switch)).thumbIcon!.resolve(states)!;

  testWidgets(
    'glyph fades through zero, reverses continuously and keeps native switch',
    (t) async {
      await t.pumpWidget(host(VisualStyle.industrial));
      await t.pumpAndSettle();
      final native = t.element(find.byType(Switch));
      expect(icon(t).icon, Icons.power_settings_new_rounded);
      await t.pumpWidget(host(VisualStyle.brutalist));
      expect(icon(t).color!.a, 1);
      await t.pump(const Duration(milliseconds: 45));
      expect(icon(t).color!.a, closeTo(.5, .02));
      final before = icon(t);
      await t.pumpWidget(host(VisualStyle.industrial));
      expect(icon(t).icon, before.icon);
      expect(icon(t).color!.a, closeTo(before.color!.a, .001));
      await t.pumpAndSettle();
      await t.pumpWidget(host(VisualStyle.brutalist));
      await t.pump(const Duration(milliseconds: 90));
      expect(icon(t).color!.a, closeTo(0, .001));
      await t.pumpAndSettle();
      expect(icon(t).icon, Icons.stop_rounded);
      expect(t.element(find.byType(Switch)), same(native));
      await t.pumpWidget(host(VisualStyle.flat));
      await t.pump(const Duration(milliseconds: 90));
      expect(icon(t).color!.a, closeTo(.5, .02));
      await t.pumpAndSettle();
      expect(icon(t).color!.a, 0);
      expect(t.element(find.byType(Switch)), same(native));
    },
  );
  testWidgets('hidden and reduced motion settle; caller state icons survive', (
    t,
  ) async {
    await t.pumpWidget(host(VisualStyle.industrial));
    await t.pumpAndSettle();
    await t.pumpWidget(host(VisualStyle.brutalist, reduced: true));
    expect(icon(t).icon, Icons.stop_rounded);
    expect(icon(t).color!.a, 1);
    await t.pumpWidget(host(VisualStyle.flat, visible: false));
    expect(icon(t).color!.a, 0);
    final custom = WidgetStateProperty.resolveWith<Icon?>(
      (states) => Icon(
        states.contains(WidgetState.selected) ? Icons.check : Icons.close,
        size: 12,
        color: Colors.red,
      ),
    );
    await t.pumpWidget(
      host(VisualStyle.industrial, reduced: true, custom: custom),
    );
    expect(icon(t).icon, Icons.close);
    expect(icon(t, {WidgetState.selected}).icon, Icons.check);
    expect(icon(t).color!.toARGB32(), Colors.red.toARGB32());
    expect(t.takeException(), isNull);
  });
}
