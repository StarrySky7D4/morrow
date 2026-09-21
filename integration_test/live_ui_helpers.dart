import 'dart:io';
import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter_test/flutter_test.dart';

// Live integration binding only. Polling services need not ever settle.
Future<void> waitForUi(
  WidgetTester tester,
  bool Function() ready, {
  required String reason,
  Duration timeout = const Duration(seconds: 15),
}) async {
  final watch = Stopwatch()..start();
  do {
    await tester.pump(const Duration(milliseconds: 40));
    expect(tester.takeException(), isNull, reason: reason);
    if (ready()) return;
  } while (watch.elapsed < timeout);
  fail('UI timeout after $timeout: $reason');
}

Future<void> tapVisible(WidgetTester tester, Finder target) async {
  await waitForUi(
    tester,
    () => target.evaluate().length == 1,
    reason: 'unique $target',
  );
  // Asynchronous metadata may expand the settings page after the first scroll.
  // Recompute current geometry, boundedly; never tap an offscreen old position.
  for (var attempt = 0; attempt < 5; attempt++) {
    await tester.ensureVisible(target);
    for (var i = 0; i < 10; i++) {
      await tester.pump(const Duration(milliseconds: 40));
    }
    if (target.hitTestable().evaluate().length == 1) break;
  }
  expect(target.hitTestable(), findsOneWidget);
  await tester.tap(target.hitTestable());
  await tester.pump(const Duration(milliseconds: 40));
  expect(tester.takeException(), isNull);
}

// Flutter render capture, not an OS screenshot or physical-input acceptance.
Future<void> saveBoundaryPng(
  WidgetTester tester,
  GlobalKey key,
  String absolutePath,
) async {
  // Complete the app's bounded entrance transitions without waiting on timers.
  for (var i = 0; i < 10; i++) {
    await tester.pump(const Duration(milliseconds: 40));
  }
  final boundary =
      key.currentContext!.findRenderObject()! as RenderRepaintBoundary;
  final image = await boundary.toImage(pixelRatio: 1);
  try {
    expect(image.width, greaterThan(0));
    expect(image.height, greaterThan(0));
    final bytes = await image.toByteData(format: ui.ImageByteFormat.png);
    expect(bytes, isNotNull);
    final file = File(absolutePath);
    await file.parent.create(recursive: true);
    await file.writeAsBytes(
      bytes!.buffer.asUint8List(bytes.offsetInBytes, bytes.lengthInBytes),
    );
  } finally {
    image.dispose();
  }
}
