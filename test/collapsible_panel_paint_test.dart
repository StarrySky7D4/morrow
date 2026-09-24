import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/collapsible_panel.dart';

class _PaintCounter {
  int calls = 0;
}

class _CountingPainter extends CustomPainter {
  const _CountingPainter(this.counter, this.color);

  final _PaintCounter counter;
  final Color color;

  @override
  void paint(Canvas canvas, Size size) {
    counter.calls++;
    canvas.drawRect(Offset.zero & size, Paint()..color = color);
  }

  @override
  bool shouldRepaint(covariant _CountingPainter oldDelegate) =>
      color != oldDelegate.color;
}

void main() {
  testWidgets(
    'collapsed panel skips child paint while closed and survives reversal',
    (tester) async {
      final counter = _PaintCounter();
      var expanded = false;
      var color = Colors.red;
      late StateSetter update;

      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: StatefulBuilder(
              builder: (context, setState) {
                update = setState;
                return Column(
                  children: [
                    CollapsiblePanel(
                      expanded: expanded,
                      child: SizedBox(
                        width: 120,
                        height: 80,
                        child: CustomPaint(
                          painter: _CountingPainter(counter, color),
                        ),
                      ),
                    ),
                  ],
                );
              },
            ),
          ),
        ),
      );
      expect(counter.calls, 0);

      update(() => color = Colors.blue);
      await tester.pump();
      expect(counter.calls, 0);

      update(() => expanded = true);
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 100));
      expect(
        tester.getSize(find.byType(CollapsiblePanel)).height,
        inExclusiveRange(0, 80),
      );
      expect(counter.calls, greaterThan(0));

      update(() => expanded = false);
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 40));
      expect(
        tester.getSize(find.byType(CollapsiblePanel)).height,
        inExclusiveRange(0, 80),
      );

      update(() => expanded = true);
      await tester.pumpAndSettle();
      expect(tester.getSize(find.byType(CollapsiblePanel)).height, 80);
      final beforeRepaint = counter.calls;
      update(() => color = Colors.green);
      await tester.pump();
      expect(counter.calls, greaterThan(beforeRepaint));

      update(() => expanded = false);
      await tester.pumpAndSettle();
      expect(tester.getSize(find.byType(CollapsiblePanel)).height, 0);
      final beforeClosedChange = counter.calls;
      update(() => color = Colors.yellow);
      await tester.pump();
      expect(counter.calls, beforeClosedChange);
    },
  );

  testWidgets('collapsed panel stops painting when animations are disabled', (
    tester,
  ) async {
    final counter = _PaintCounter();
    var expanded = true;
    var color = Colors.red;
    late StateSetter update;

    await tester.pumpWidget(
      MaterialApp(
        home: MediaQuery(
          data: const MediaQueryData(disableAnimations: true),
          child: Scaffold(
            body: StatefulBuilder(
              builder: (context, setState) {
                update = setState;
                return CollapsiblePanel(
                  expanded: expanded,
                  child: SizedBox(
                    width: 120,
                    height: 80,
                    child: CustomPaint(
                      painter: _CountingPainter(counter, color),
                    ),
                  ),
                );
              },
            ),
          ),
        ),
      ),
    );
    expect(counter.calls, greaterThan(0));

    update(() => expanded = false);
    await tester.pump();
    expect(tester.getSize(find.byType(CollapsiblePanel)).height, 0);
    final beforeClosedChange = counter.calls;
    update(() => color = Colors.blue);
    await tester.pump();
    expect(counter.calls, beforeClosedChange);

    update(() => expanded = true);
    await tester.pump();
    expect(tester.getSize(find.byType(CollapsiblePanel)).height, 80);
    expect(counter.calls, greaterThan(beforeClosedChange));
  });
}
