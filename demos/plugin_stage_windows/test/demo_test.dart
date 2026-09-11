import 'dart:io';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_stage_demo/main.dart';

Future<void> ready(WidgetTester tester, bool Function() done) async {
  final deadline = DateTime.now().add(const Duration(seconds: 15));
  while (!done()) {
    if (DateTime.now().isAfter(deadline)) {
      throw StateError('Demo session timed out');
    }
    await tester.runAsync(
      () => Future<void>.delayed(const Duration(milliseconds: 60)),
    );
    await tester.pump(const Duration(milliseconds: 100));
  }
  await tester.pump(const Duration(milliseconds: 400));
}

void main() {
  testWidgets(
    'distinct live tools: real inputs, buttons, toggles, copy, theme and narrow layout',
    (tester) async {
      final root = Platform.environment['MORROW_STAGE_DEMO_ROOT'];
      expect(root, isNotNull);
      tester.view.physicalSize = const Size(1180, 900);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      Future<void> click(String label) async {
        final target = find.text(label);
        await tester.ensureVisible(target);
        await tester.tap(target);
        await tester.pump();
      }

      Future<void> updates(int n) =>
          ready(tester, () => find.text('本次已更新 $n 次').evaluate().isNotEmpty);
      await tester.pumpWidget(StageDemo(bundleFolder: root));
      await ready(tester, () => find.byType(TextField).evaluate().isNotEmpty);
      await tester.enterText(
        find.byType(TextField),
        '  alpha   beta  \n gamma ',
      );
      await updates(1);
      await click('合并为一段');
      await updates(2);
      expect(find.text('alpha beta gamma'), findsOneWidget);
      await click('整理文字');
      await updates(3);
      expect(
        tester.widget<TextField>(find.byType(TextField)).controller!.text,
        'alpha beta gamma',
      );
      await click('清空');
      await updates(4);
      for (final text in ['快', '快速', '快速连续输入']) {
        await tester.enterText(find.byType(TextField), text);
      }
      await updates(7);
      expect(
        tester.widget<TextField>(find.byType(TextField)).controller!.text,
        '快速连续输入',
      );
      await tester.ensureVisible(find.byTooltip('切换明暗主题'));
      await tester.tap(find.byTooltip('切换明暗主题'));
      await tester.pumpAndSettle();
      expect(find.text('快速连续输入'), findsNWidgets(2));
      String? copied;
      tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        SystemChannels.platform,
        (call) async {
          if (call.method == 'Clipboard.setData') {
            copied = (call.arguments as Map)['text'] as String;
          }
          return null;
        },
      );
      addTearDown(
        () => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
          SystemChannels.platform,
          null,
        ),
      );
      await tester.ensureVisible(find.byTooltip('复制结果'));
      await tester.tap(find.byTooltip('复制结果'));
      await tester.pump();
      expect(copied, '快速连续输入');
      await click('单位换算 · C');
      await ready(tester, () => find.text('长度模式（米与英尺）').evaluate().isNotEmpty);
      await tester.enterText(find.byType(TextField), '100');
      await updates(1);
      expect(find.text('212.000 °F'), findsOneWidget);
      await click('反向换算');
      await updates(2);
      expect(find.text('37.778 °C'), findsOneWidget);
      await tester.enterText(find.byType(TextField), 'abc');
      await updates(3);
      expect(find.text('等待有效数值'), findsOneWidget);
      await click('重置');
      await updates(4);
      expect(find.text('68.000 °F'), findsOneWidget);
      await click('清单助手 · C++');
      await ready(tester, () => find.text('预览只看未完成').evaluate().isNotEmpty);
      await click('收集灵感');
      await updates(1);
      expect(find.text('已完成 1 / 3 · 33%'), findsOneWidget);
      await click('预览只看未完成');
      await updates(2);
      expect(find.text('✓ 收集灵感'), findsNothing);
      await click('清除已完成');
      await updates(3);
      expect(find.text('已完成 0 / 2 · 0%'), findsOneWidget);
      tester.view.physicalSize = const Size(480, 900);
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull);
      await tester.pumpWidget(const SizedBox.shrink());
      await tester.runAsync(
        () => Future<void>.delayed(const Duration(milliseconds: 200)),
      );
    },
  );
}
