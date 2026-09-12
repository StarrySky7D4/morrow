import 'dart:io';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import '../window_effects.dart';

Future<void> qualifyCanvas(String output) async {
  const channel = MethodChannel('morrow/window_shape');
  final tint = ValueNotifier<double>(0);
  final evidence = <String>[];
  try {
    await DesktopBackground().apply();
    runApp(
      Directionality(
        textDirection: TextDirection.ltr,
        child: ValueListenableBuilder<double>(
          valueListenable: tint,
          builder: (_, alpha, _) => Stack(
            fit: StackFit.expand,
            children: [
              Positioned.fill(
                child: ColoredBox(color: Colors.green.withValues(alpha: alpha)),
              ),
              const Positioned(
                left: 64,
                top: 80,
                width: 128,
                height: 32,
                child: ColoredBox(color: Colors.white),
              ),
            ],
          ),
        ),
      ),
    );
    await Future<void>.delayed(const Duration(milliseconds: 600));
    await channel.invokeMethod<void>('startCanvasProbe');
    Future<List<int>> sample(double blur, double alpha) async {
      tint.value = alpha;
      await channel.invokeMethod<void>('setCanvasBlur', {
        'blur': blur,
        'topInset': 0.0,
      });
      await Future<void>.delayed(const Duration(milliseconds: 650));
      final pixels = (await channel.invokeListMethod<int>(
        'sampleCanvasProbe',
      ))!;
      evidence.add(
        'blur=$blur alpha=$alpha pixels=${pixels.map((v) => v.toRadixString(16)).join(",")}',
      );
      return pixels;
    }

    int distance(int a, int b) => [0, 8, 16].fold(
      0,
      (sum, shift) => sum + (((a >> shift) & 255) - ((b >> shift) & 255)).abs(),
    );
    final clear = await sample(0, 0);
    for (final pixel in clear.take(31)) {
      if (distance(pixel, 0xf03250) > 18 && distance(pixel, 0x1e96e6) > 18) {
        throw StateError('Zero opacity did not expose the owned source');
      }
    }
    final low = await sample(.4, 0); // Slider 1% (range 0..40).
    final low2 = await sample(.8, 0);
    final medium = await sample(4, 0);
    final blurred = await sample(12, 0);
    int delta(List<int> a, List<int> b, [int start = 0]) => List.generate(
      31,
      (i) => distance(a[start + i], b[start + i]),
    ).reduce((a, b) => a + b);
    final lowDelta = delta(clear, low);
    final low2Delta = delta(clear, low2);
    final mediumDelta = delta(clear, medium);
    final fullDelta = delta(clear, blurred);
    evidence.add(
      'strength deltas: 1%=$lowDelta 2%=$low2Delta 10%=$mediumDelta 30%=$fullDelta',
    );
    if (lowDelta >= fullDelta * .1 ||
        low2Delta < lowDelta ||
        mediumDelta <= low2Delta ||
        mediumDelta >= fullDelta * .8) {
      throw StateError('Low frost does not progressively blend from clear');
    }
    if (blurred.take(31).every((p) => distance(p, 0) < 80) ||
        List.generate(
              31,
              (i) => distance(clear[i], blurred[i]),
            ).reduce((a, b) => a + b) <
            300) {
      throw StateError('Canvas blur is black or did not change the source');
    }
    if (delta(clear, blurred, 31) < 300) {
      throw StateError('Caption background did not receive blur');
    }
    for (final p in [
      clear.last,
      low.last,
      low2.last,
      medium.last,
      blurred.last,
    ]) {
      if (distance(p, 0xffffff) > 12) {
        throw StateError('Blur affected foreground');
      }
    }
    final colored = await sample(12, .5);
    if (List.generate(
          31,
          (i) => distance(blurred[i], colored[i]),
        ).reduce((a, b) => a + b) <
        200) {
      throw StateError('Canvas tint did not change pixels');
    }
    if (delta(blurred, colored, 31) < 200 ||
        distance(colored.last, 0xffffff) > 12) {
      throw StateError('Caption tint missing or foreground affected');
    }
    final restored = await sample(0, 0);
    for (var i = 0; i < 63; i++) {
      if (distance(clear[i], restored[i]) > 18) {
        throw StateError('Transparency was not restored');
      }
    }
    await File(output).writeAsString(
      'PASS: owned desktop source, zero alpha, progressive low frost, canvas and caption blur, sharp foreground, full canvas tint and return to transparent.\n\n${evidence.join("\n")}',
    );
    exit(0);
  } catch (e, stack) {
    await File(
      output,
    ).writeAsString('FAIL: $e\n$stack\n${evidence.join("\n")}');
    exit(1);
  }
}
