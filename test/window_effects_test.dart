import 'dart:ui' as ui;
import 'package:morrow_studio/main.dart';
import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:morrow_studio/window_effects.dart';
import 'package:flutter/services.dart';
import 'package:flutter_acrylic/flutter_acrylic.dart' as acrylic;
import 'package:flutter_test/flutter_test.dart';

void main() {
  testWidgets(
    'Opaque background crossfades never expose the desktop',
    (tester) async {
      tester.view.physicalSize = const Size(1440, 1000);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      final canvas = GlobalKey();
      await tester.pumpWidget(
        RepaintBoundary(key: canvas, child: const MorrowApp()),
      );
      await tester.pumpAndSettle();
      Future<int> alpha() async {
        final boundary =
            canvas.currentContext!.findRenderObject()! as RenderRepaintBoundary;
        return (await tester.runAsync(() async {
          final image = await boundary.toImage();
          try {
            final rgba = await image.toByteData(
              format: ui.ImageByteFormat.rawRgba,
            );
            return rgba!.getUint8((2 * image.width + 2) * 4 + 3);
          } finally {
            image.dispose();
          }
        }))!;
      }

      for (final mode in ['texture', 'solid', 'ambient']) {
        final control = find.byKey(ValueKey('background-$mode'));
        await tester.ensureVisible(control);
        await tester.tap(control);
        await tester.pump();
        await tester.pump(const Duration(milliseconds: 180));
        expect(
          await alpha(),
          255,
          reason: '$mode stays opaque during crossfade',
        );
        await tester.pumpAndSettle();
      }
      await tester.tap(find.byKey(const ValueKey('background-transparent')));
      await tester.pumpAndSettle();
      expect(await alpha(), 0);
      await tester.pumpWidget(const SizedBox());
    },
    variant: TargetPlatformVariant.only(TargetPlatform.windows),
  );
  testWidgets(
    'Native host stays transparent across background reapplications',
    (tester) async {
      const channel = MethodChannel('com.alexmercerind/flutter_acrylic');
      final calls = <MethodCall>[];
      tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(channel, (
        call,
      ) async {
        calls.add(call);
        return null;
      });
      addTearDown(
        () => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
          channel,
          null,
        ),
      );
      final background = DesktopBackground();
      await Future.wait([background.apply(), background.apply()]);
      await background.apply();
      expect(calls.map((c) => c.method), ['Initialize', 'SetEffect']);
      final effect = calls.last.arguments as Map;
      expect(effect['effect'], acrylic.WindowEffect.transparent.index);
      expect((effect['color'] as Map)['A'], 0);
    },
    variant: TargetPlatformVariant.only(TargetPlatform.windows),
  );
}
