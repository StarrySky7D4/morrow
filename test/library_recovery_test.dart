import 'dart:async';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/workbench_recovery.dart';

void main() {
  testWidgets(
    'snapshot recovery stays serialized, cancellation and failure retain other recovery actions',
    (tester) async {
      tester.view.physicalSize = const Size(360, 1000);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      var snapshots = 0, keys = 0, retries = 0;
      final pending = Completer<void>();
      await tester.pumpWidget(
        WorkbenchRecovery(
          message: '已登记内容库无法打开',
          locale: const Locale('zh'),
          onRetry: () async {
            retries++;
          },
          onRestore: () async {
            keys++;
          },
          onRestoreSnapshot: () async {
            snapshots++;
            if (snapshots == 1) return;
            await pending.future;
            throw StateError('Morrow workbench host: 备份不完整，原资料已保留');
          },
        ),
      );
      await tester.pumpAndSettle();
      await tester.tap(find.byKey(const ValueKey('restore-library-snapshot')));
      await tester.pumpAndSettle();
      expect(snapshots, 1);
      await tester.tap(find.byKey(const ValueKey('restore-library-snapshot')));
      await tester.pump();
      await tester.tap(find.text('重试打开'));
      await tester.tap(find.text('选择恢复文件'));
      expect(keys, 0);
      expect(retries, 0);
      expect(snapshots, 2);
      pending.complete();
      await tester.pumpAndSettle();
      expect(find.text('备份不完整，原资料已保留'), findsOneWidget);
      await tester.tap(find.text('选择恢复文件'));
      await tester.pumpAndSettle();
      expect(keys, 1);
      expect(tester.takeException(), isNull);
    },
  );
}
