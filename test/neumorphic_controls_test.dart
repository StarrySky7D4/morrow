import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/appearance.dart';
import 'package:morrow_studio/neumorphic_controls.dart';

void main() {
  const flat = Palette(StudioTheme.white, GlassMode.frosted);
  final raised = flat.withSurfaces(
    const SurfaceSettings(visualStyle: VisualStyle.neumorphism),
  );

  test('flat returns the original ThemeData object', () {
    final base = ThemeData(useMaterial3: true);
    expect(identical(applyNeumorphicControls(base, flat), base), isTrue);
    final input = applyNeumorphicControls(base, raised).inputDecorationTheme;
    expect(input.enabledBorder, isA<NeumorphicInputBorder>());
    expect(
      ShapeBorder.lerp(input.enabledBorder, input.focusedBorder, .5),
      isA<NeumorphicInputBorder>(),
    );
  });

  testWidgets('surface painter changes side without replacing its child', (
    tester,
  ) async {
    final key = GlobalKey();
    Widget build(Palette palette, double depth) => MaterialApp(
      home: Scaffold(
        body: NeumorphicSurface(
          palette: palette,
          depth: depth,
          child: TextField(key: key),
        ),
      ),
    );
    await tester.pumpWidget(build(flat, 1));
    final original = key.currentState;
    final flatPaint = tester.widget<CustomPaint>(
      find
          .ancestor(of: find.byKey(key), matching: find.byType(CustomPaint))
          .first,
    );
    expect(flatPaint.painter, isNull);
    expect(flatPaint.foregroundPainter, isNull);

    await tester.pumpWidget(build(raised, 1));
    await tester.pumpAndSettle();
    expect(key.currentState, same(original));
    final raisedPaint = tester.widget<CustomPaint>(
      find
          .ancestor(of: find.byKey(key), matching: find.byType(CustomPaint))
          .first,
    );
    expect((raisedPaint.painter as NeumorphicSurfacePainter).depth, 1);

    await tester.pumpWidget(build(raised, -1));
    await tester.pumpAndSettle();
    expect(key.currentState, same(original));
    final insetPaint = tester.widget<CustomPaint>(
      find
          .ancestor(of: find.byKey(key), matching: find.byType(CustomPaint))
          .first,
    );
    expect(insetPaint.painter, isNull);
    expect(
      (insetPaint.foregroundPainter as NeumorphicSurfacePainter).depth,
      -1,
    );
  });

  testWidgets('button keeps disabled behavior and presses into recess', (
    tester,
  ) async {
    final states = WidgetStatesController();
    var taps = 0;
    final theme = applyNeumorphicControls(
      ThemeData(useMaterial3: true),
      raised,
    );
    await tester.pumpWidget(
      MaterialApp(
        theme: theme,
        home: Scaffold(
          body: Column(
            children: [
              FilledButton(
                statesController: states,
                onPressed: () => taps++,
                child: const Text('Enabled'),
              ),
              FilledButton(onPressed: null, child: const Text('Disabled')),
            ],
          ),
        ),
      ),
    );
    final enabled = find.ancestor(
      of: find.text('Enabled'),
      matching: find.byType(NeumorphicSurface),
    );
    expect(tester.widget<NeumorphicSurface>(enabled).depth, 1);
    states.update(WidgetState.pressed, true);
    await tester.pump();
    expect(tester.widget<NeumorphicSurface>(enabled).depth, -1);
    states.update(WidgetState.pressed, false);
    await tester.tap(find.text('Enabled'));
    await tester.tap(find.text('Disabled'));
    expect(taps, 1);
    final disabled = find.ancestor(
      of: find.text('Disabled'),
      matching: find.byType(NeumorphicSurface),
    );
    expect(tester.widget<NeumorphicSurface>(disabled).enabled, isFalse);
    states.dispose();
  });

  testWidgets('input, switch, slider and chip preserve their callbacks', (
    tester,
  ) async {
    var checked = false;
    var sliderValue = .2;
    var selected = false;
    final theme = applyNeumorphicControls(
      ThemeData(useMaterial3: true),
      raised,
    );
    await tester.pumpWidget(
      MaterialApp(
        theme: theme,
        home: StatefulBuilder(
          builder: (context, setState) {
            return Scaffold(
              body: Column(
                children: [
                  const TextField(
                    decoration: InputDecoration(labelText: 'Search'),
                  ),
                  Switch(
                    value: checked,
                    onChanged: (value) => setState(() => checked = value),
                  ),
                  Slider(
                    value: sliderValue,
                    onChanged: (value) => setState(() => sliderValue = value),
                  ),
                  ChoiceChip(
                    label: const Text('Choice'),
                    selected: selected,
                    onSelected: (value) => setState(() => selected = value),
                  ),
                ],
              ),
            );
          },
        ),
      ),
    );
    expect(find.byType(TextField), findsOneWidget);
    await tester.tap(find.byType(Switch));
    await tester.tap(find.text('Choice'));
    await tester.drag(find.byType(Slider), const Offset(60, 0));
    expect(checked, isTrue);
    expect(selected, isTrue);
    expect(sliderValue, greaterThan(.2));
  });

  testWidgets('native switch keeps focus and state while style changes', (
    tester,
  ) async {
    final focus = FocusNode();
    addTearDown(focus.dispose);
    var value = false;
    Widget build(Palette palette) => MaterialApp(
      theme: applyNeumorphicControls(ThemeData(useMaterial3: true), palette),
      home: StatefulBuilder(
        builder: (context, setState) => Scaffold(
          body: NeumorphicSwitch(
            palette: palette,
            value: value,
            onChanged: (next) => setState(() => value = next),
            focusNode: focus,
          ),
        ),
      ),
    );
    await tester.pumpWidget(build(flat));
    final nativeElement = tester.element(find.byType(Switch));
    focus.requestFocus();
    await tester.pump();
    expect(focus.hasFocus, isTrue);
    await tester.pumpWidget(build(raised));
    await tester.pumpAndSettle();
    expect(tester.element(find.byType(Switch)), same(nativeElement));
    expect(focus.hasFocus, isTrue);
    await tester.tap(find.byType(Switch));
    await tester.pumpAndSettle();
    expect(value, isTrue);
  });

  testWidgets('checkbox keeps tristate, and disabled tile ignores taps', (
    tester,
  ) async {
    bool? checked = false;
    await tester.pumpWidget(
      MaterialApp(
        theme: applyNeumorphicControls(ThemeData(useMaterial3: true), raised),
        home: MediaQuery(
          data: const MediaQueryData(textScaler: TextScaler.linear(1.6)),
          child: StatefulBuilder(
            builder: (context, setState) => Scaffold(
              body: SizedBox(
                width: 360,
                child: Column(
                  children: [
                    NeumorphicCheckbox(
                      palette: raised,
                      value: checked,
                      tristate: true,
                      semanticLabel: 'Tri-state option',
                      onChanged: (next) => setState(() => checked = next),
                    ),
                    NeumorphicSwitchListTile(
                      palette: raised,
                      value: false,
                      onChanged: null,
                      title: const Text('Disabled material option'),
                      subtitle: const Text(
                        'The row remains in the reading order',
                      ),
                    ),
                  ],
                ),
              ),
            ),
          ),
        ),
      ),
    );
    await tester.tap(find.byType(Checkbox));
    await tester.pumpAndSettle();
    expect(checked, isTrue);
    await tester.tap(find.byType(Checkbox));
    await tester.pumpAndSettle();
    expect(checked, isNull);
    await tester.tap(find.text('Disabled material option'));
    expect(tester.widget<Switch>(find.byType(Switch)).onChanged, isNull);
    expect(tester.takeException(), isNull);
    expect(find.byType(Switch), findsOneWidget);
  });
}
