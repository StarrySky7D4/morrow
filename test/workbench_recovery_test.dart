import 'dart:async';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/workbench_recovery.dart';

void main() {
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
