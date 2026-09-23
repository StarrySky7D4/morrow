import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/collapsible_panel.dart';
import 'package:morrow_studio/surface_motion.dart';

void main() {
  testWidgets('Surface press feedback yields to scrolling and child taps', (
    tester,
  ) async {
    var taps = 0;
    await tester.pumpWidget(
      MaterialApp(
        home: Center(
          child: SurfaceInteraction(
            child: GestureDetector(
              onTap: () => taps++,
              child: const SizedBox(
                width: 160,
                height: 60,
                child: ColoredBox(color: Colors.white),
              ),
            ),
          ),
        ),
      ),
    );

    double scale() => tester
        .widget<AnimatedContainer>(find.byType(AnimatedContainer).first)
        .transform!
        .storage[0];

    final gesture = await tester.startGesture(
      tester.getCenter(find.byType(SurfaceInteraction)),
    );
    await tester.pump();
    expect(scale(), closeTo(0.988, 0.0001));
    await gesture.moveBy(const Offset(0, 30));
    await tester.pump();
    expect(scale(), closeTo(1, 0.0001));
    await gesture.up();
    await tester.pumpAndSettle();
    expect(taps, 0);

    await tester.tap(find.byType(SurfaceInteraction));
    await tester.pumpAndSettle();
    expect(taps, 1);

    final rightClick = await tester.startGesture(
      tester.getCenter(find.byType(SurfaceInteraction)),
      kind: PointerDeviceKind.mouse,
      buttons: kSecondaryMouseButton,
    );
    await tester.pump();
    expect(scale(), greaterThanOrEqualTo(1));
    await rightClick.up();
  });

  testWidgets('Pointer state clears when motion or tickers are disabled', (
    tester,
  ) async {
    var reduced = false;
    var ticking = true;
    late StateSetter update;
    await tester.pumpWidget(
      MaterialApp(
        home: StatefulBuilder(
          builder: (context, setState) {
            update = setState;
            return MediaQuery(
              data: MediaQuery.of(context).copyWith(disableAnimations: reduced),
              child: TickerMode(
                enabled: ticking,
                child: Center(
                  child: SurfaceInteraction(
                    child: const SizedBox(
                      width: 160,
                      height: 60,
                      child: ColoredBox(color: Colors.white),
                    ),
                  ),
                ),
              ),
            );
          },
        ),
      ),
    );

    double scale() => tester
        .widget<AnimatedContainer>(find.byType(AnimatedContainer).first)
        .transform!
        .storage[0];
    final target = find.byType(SurfaceInteraction);
    final first = await tester.startGesture(tester.getCenter(target));
    await tester.pump();
    expect(scale(), closeTo(0.988, 0.0001));
    update(() => reduced = true);
    await tester.pump();
    update(() => reduced = false);
    await tester.pump();
    expect(scale(), 1);
    await first.up();

    final second = await tester.startGesture(tester.getCenter(target));
    await tester.pump();
    expect(scale(), closeTo(0.988, 0.0001));
    update(() => ticking = false);
    await tester.pump();
    update(() => ticking = true);
    await tester.pump();
    expect(scale(), 1);
    await second.up();
  });

  testWidgets('Attachment keeps one child and snaps when motion is disabled', (
    tester,
  ) async {
    var attached = false;
    late StateSetter update;
    final controller = TextEditingController();
    addTearDown(controller.dispose);
    await tester.pumpWidget(
      MaterialApp(
        home: MediaQuery(
          data: const MediaQueryData(disableAnimations: true),
          child: Scaffold(
            body: StatefulBuilder(
              builder: (context, setState) {
                update = setState;
                return SurfaceAttachment(
                  attached: attached,
                  child: TextField(
                    key: const ValueKey('attachment-field'),
                    controller: controller,
                  ),
                );
              },
            ),
          ),
        ),
      ),
    );

    await tester.enterText(
      find.byKey(const ValueKey('attachment-field')),
      'keep me',
    );
    final original = tester.state<State>(
      find.byKey(const ValueKey('attachment-field')),
    );
    update(() => attached = true);
    await tester.pump();
    expect(
      tester.widget<AnimatedSlide>(find.byType(AnimatedSlide)).duration,
      Duration.zero,
    );
    expect(
      tester.widget<AnimatedSlide>(find.byType(AnimatedSlide)).offset,
      Offset.zero,
    );
    expect(
      tester.state<State>(find.byKey(const ValueKey('attachment-field'))),
      same(original),
    );
    expect(controller.text, 'keep me');
  });

  testWidgets('Panel reverses mid-flight and retains the same editor', (
    tester,
  ) async {
    var expanded = true;
    late StateSetter update;
    final controller = TextEditingController();
    addTearDown(controller.dispose);
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: Row(
            children: [
              StatefulBuilder(
                builder: (context, setState) {
                  update = setState;
                  return CollapsiblePanel(
                    expanded: expanded,
                    axis: Axis.horizontal,
                    extent: 200,
                    child: TextField(controller: controller),
                  );
                },
              ),
              const Expanded(child: SizedBox()),
            ],
          ),
        ),
      ),
    );

    await tester.enterText(find.byType(TextField), 'retained');
    final original = tester.state<State>(find.byType(TextField));
    update(() => expanded = false);
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 90));
    final halfway = tester.getSize(find.byType(CollapsiblePanel)).width;
    expect(halfway, inExclusiveRange(0, 200));
    update(() => expanded = true);
    await tester.pumpAndSettle();
    expect(tester.getSize(find.byType(CollapsiblePanel)).width, 200);
    expect(tester.state<State>(find.byType(TextField)), same(original));
    expect(controller.text, 'retained');
  });
}
