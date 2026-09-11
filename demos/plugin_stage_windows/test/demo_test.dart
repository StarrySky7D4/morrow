import 'dart:io';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_stage_demo/main.dart';

Future<void> ready(WidgetTester tester, bool Function() done) async {
  final deadline = DateTime.now().add(const Duration(seconds: 15));
  while (!done()) {
    if (DateTime.now().isAfter(deadline)) {
      throw StateError(
        'Demo session timed out: ${tester.widgetList<Text>(find.byType(Text)).map((w) => w.data).join(" | ")}',
      );
    }
    await tester.runAsync(
      () => Future<void>.delayed(const Duration(milliseconds: 60)),
    );
    await tester.pump(const Duration(milliseconds: 100));
  }
}

void main() {
  testWidgets(
    'live bundled plugins, input, language replacement and theme preserve working UI',
    (tester) async {
      final root = Platform.environment['MORROW_STAGE_DEMO_ROOT'];
      expect(
        root,
        isNotNull,
        reason: 'Build the Windows Demo bundle before integration tests',
      );
      tester.view.physicalSize = const Size(1180, 800);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      await tester.pumpWidget(StageDemo(bundleFolder: root));
      await ready(tester, () => find.byType(TextField).evaluate().isNotEmpty);
      await tester.enterText(find.byType(TextField), '真实交互验证');
      await ready(tester, () => find.text('本次已更新 1 次').evaluate().isNotEmpty);
      expect(find.text('真实交互验证'), findsNWidgets(2));
      await tester.tap(find.byTooltip('切换明暗主题'));
      await tester.pumpAndSettle();
      expect(find.text('真实交互验证'), findsNWidgets(2));
      for (final title in ['快', '快速', '快速连续输入']) {
        await tester.enterText(find.byType(TextField), title);
      }
      await ready(tester, () => find.text('本次已更新 4 次').evaluate().isNotEmpty);
      expect(find.text('快速连续输入'), findsNWidgets(2));
      for (final language in ['C 插件', 'C++ 插件']) {
        await tester.tap(find.text(language));
        await tester.pump(const Duration(milliseconds: 100));
        await ready(tester, () => find.byType(TextField).evaluate().isNotEmpty);
        await tester.enterText(find.byType(TextField), '下一张卡片');
        await ready(tester, () => find.text('本次已更新 1 次').evaluate().isNotEmpty);
        expect(find.text('下一张卡片'), findsNWidgets(2));
      }
      tester.view.physicalSize = const Size(480, 800);
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull);
      await tester.pumpWidget(const SizedBox.shrink());
      await tester.runAsync(
        () => Future<void>.delayed(const Duration(milliseconds: 200)),
      );
    },
  );
}
