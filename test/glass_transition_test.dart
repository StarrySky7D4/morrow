import 'dart:math' as math;
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/appearance.dart';
import 'package:morrow_studio/liquid_glass.dart';

class _Editor extends StatefulWidget {
  const _Editor({required this.onCreate});
  final VoidCallback onCreate;
  @override
  State<_Editor> createState() => _EditorState();
}

class _EditorState extends State<_Editor> {
  final controller = TextEditingController();
  final focus = FocusNode();
  @override
  void initState() {
    super.initState();
    widget.onCreate();
  }

  @override
  void dispose() {
    controller.dispose();
    focus.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) =>
      TextField(controller: controller, focusNode: focus);
}

void main() {
  testWidgets(
    'All six material switches interpolate blur and decoration without replacing focused content',
    (tester) async {
      var mode = GlassMode.frosted;
      var created = 0;
      late StateSetter update;
      final editor = _Editor(onCreate: () => created++);
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: StatefulBuilder(
              builder: (context, setState) {
                update = setState;
                return Center(
                  child: SizedBox(
                    width: 320,
                    height: 180,
                    child: Glass(
                      p: Palette(StudioTheme.white, mode),
                      child: editor,
                    ),
                  ),
                );
              },
            ),
          ),
        ),
      );
      await tester.enterText(find.byType(TextField), '输入和焦点都保留');
      final original = tester.state<_EditorState>(find.byType(_Editor));
      GlassMaterial material() => tester
          .widget<LiquidGlassSurface>(find.byType(LiquidGlassSurface))
          .material!;
      for (final from in GlassMode.values) {
        for (final to in GlassMode.values.where((m) => m != from)) {
          update(() => mode = from);
          await tester.pumpAndSettle();
          final start = material();
          update(() => mode = to);
          await tester.pump();
          expect(material(), start);
          await tester.pump(const Duration(milliseconds: 120));
          final middle = material();
          await tester.pumpAndSettle();
          final end = material();
          expect(
            middle.blur,
            inExclusiveRange(
              math.min(start.blur, end.blur),
              math.max(start.blur, end.blur),
            ),
          );
          final a =
              (start.decoration.gradient! as LinearGradient).colors.first.a;
          final b = (end.decoration.gradient! as LinearGradient).colors.first.a;
          final mid =
              (middle.decoration.gradient! as LinearGradient).colors.first.a;
          expect(mid, inExclusiveRange(math.min(a, b), math.max(a, b)));
          expect(
            tester.state<_EditorState>(find.byType(_Editor)),
            same(original),
          );
          expect(original.controller.text, '输入和焦点都保留');
          expect(original.focus.hasFocus, isTrue);
          expect(created, 1);
          expect(find.byType(BackdropFilter), findsOneWidget);
          final rim = tester
              .widgetList<CustomPaint>(find.byType(CustomPaint))
              .map((w) => w.painter)
              .whereType<LiquidRimPainter>()
              .single;
          expect(rim.intensity, end.liquid);
        }
      }
      expect(tester.takeException(), isNull);
    },
  );
  testWidgets(
    'Rapid reversal starts from the displayed value and reduced motion settles immediately',
    (tester) async {
      var mode = GlassMode.frosted;
      var reduced = false;
      late StateSetter update;
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: StatefulBuilder(
              builder: (context, setState) {
                update = setState;
                return MediaQuery(
                  data: MediaQueryData(disableAnimations: reduced),
                  child: Center(
                    child: SizedBox(
                      width: 320,
                      height: 180,
                      child: Glass(
                        p: Palette(StudioTheme.dark, mode),
                        child: const Text('保留内容'),
                      ),
                    ),
                  ),
                );
              },
            ),
          ),
        ),
      );
      GlassMaterial material() => tester
          .widget<LiquidGlassSurface>(find.byType(LiquidGlassSurface))
          .material!;
      update(() => mode = GlassMode.liquid);
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 140));
      final before = material();
      expect(before.liquid, inExclusiveRange(0, 1));
      update(() => mode = GlassMode.clear);
      await tester.pump();
      expect(material(), before);
      await tester.pump(const Duration(milliseconds: 70));
      expect(material().liquid, inExclusiveRange(0, before.liquid));
      update(() => mode = GlassMode.liquid);
      await tester.pumpAndSettle();
      expect(material().liquid, 1);
      update(() {
        reduced = true;
        mode = GlassMode.frosted;
      });
      await tester.pump();
      await tester.pump();
      expect(material().liquid, 0);
      expect(material().blur, 22);
      expect(tester.takeException(), isNull);
    },
  );
}
