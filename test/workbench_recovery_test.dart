import 'dart:async';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/workbench_recovery.dart';

void main() {
  testWidgets('English recovery keeps failure state while locale changes', (
    tester,
  ) async {
    tester.view.physicalSize = const Size(380, 900);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    final locale = ValueNotifier(const Locale('en'));
    addTearDown(locale.dispose);
    await tester.pumpWidget(
      ValueListenableBuilder<Locale>(
        valueListenable: locale,
        builder: (context, value, child) => WorkbenchRecovery(
          locale: value,
          failure: StateError('unclassified startup error'),
          onRetry: () async =>
              throw const RecoverySwitchUnconfirmed('C:/retained'),
        ),
      ),
    );
    await tester.pumpAndSettle();
    expect(find.text('Reopen workspace'), findsOneWidget);
    await tester.tap(find.text('Try again'));
    await tester.pumpAndSettle();
    expect(find.textContaining('C:/retained'), findsOneWidget);
    expect(
      find.textContaining('switching could not be confirmed'),
      findsOneWidget,
    );
    locale.value = const Locale('zh');
    await tester.pumpAndSettle();
    expect(find.text('重新打开工作台'), findsOneWidget);
    expect(find.textContaining('C:/retained'), findsOneWidget);
    expect(find.textContaining('切换结果未确认'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });
  testWidgets(
    'recovery stays usable after cancellation and failure, serializes work',
    (tester) async {
      tester.view.physicalSize = const Size(380, 900);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      var calls = 0;
      final pending = Completer<void>();
      await tester.pumpWidget(
        WorkbenchRecovery(
          message: '保护文件缺失',
          locale: const Locale('zh'),
          onRetry: () async {
            calls++;
          },
          onRestore: () async {
            calls++;
            if (calls == 1) return; // Picker cancellation.
            await pending.future;
            throw StateError('Morrow workbench host: 所选文件不属于此内容库');
          },
        ),
      );
      await tester.pumpAndSettle();
      await tester.tap(find.text('选择恢复文件'));
      await tester.pumpAndSettle();
      expect(calls, 1);
      await tester.tap(find.text('选择恢复文件'));
      await tester.pump();
      expect(find.byType(LinearProgressIndicator), findsOneWidget);
      await tester.tap(find.text('重试打开'));
      await tester.pump();
      expect(calls, 2);
      pending.complete();
      await tester.pumpAndSettle();
      expect(find.text('所选文件不属于此内容库'), findsOneWidget);
      expect(tester.takeException(), isNull);
      await tester.tap(find.text('重试打开'));
      await tester.pumpAndSettle();
      expect(calls, 3);
      expect(find.text('所选文件不属于此内容库'), findsNothing);
    },
  );
}
