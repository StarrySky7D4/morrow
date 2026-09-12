import 'dart:async';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/protection_backup.dart';

void main() {
  testWidgets(
    'backup cancellation failure busy and success remain distinguishable in narrow panel',
    (tester) async {
      var choices = 0;
      var writes = 0;
      final pending = Completer<void>();
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: Center(
              child: SizedBox(
                width: 230,
                child: ProtectionBackup(
                  ink: Colors.black,
                  muted: Colors.grey,
                  line: Colors.grey,
                  radius: BorderRadius.circular(11),
                  chooseDestination: () async {
                    choices++;
                    return choices == 1 ? null : 'selected.backup';
                  },
                  onBackup: (path) async {
                    writes++;
                    expect(path, 'selected.backup');
                    if (writes == 1) {
                      await pending.future;
                      throw StateError('备份位置已有文件，请选择新的文件名。');
                    }
                  },
                ),
              ),
            ),
          ),
        ),
      );
      final button = find.byKey(const ValueKey('backup-protection'));
      await tester.tap(button);
      await tester.pumpAndSettle();
      expect(writes, 0);
      expect(find.textContaining('已备份'), findsNothing);
      await tester.tap(button);
      await tester.pump();
      await tester.tap(button);
      await tester.pump();
      expect(choices, 2);
      expect(writes, 1);
      pending.complete();
      await tester.pumpAndSettle();
      expect(find.text('备份位置已有文件，请选择新的文件名。'), findsOneWidget);
      await tester.tap(button);
      await tester.pumpAndSettle();
      expect(writes, 2);
      expect(find.textContaining('已备份'), findsOneWidget);
      expect(tester.takeException(), isNull);
    },
  );
  testWidgets(
    'snapshot uses its own picker and callback in a narrow settings card',
    (tester) async {
      var snapshotWrites = 0;
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: SingleChildScrollView(
              child: SizedBox(
                width: 230,
                child: ProtectionBackup(
                  ink: Colors.black,
                  muted: Colors.grey,
                  line: Colors.grey,
                  radius: BorderRadius.circular(11),
                  onBackup: (_) => throw StateError('wrong action'),
                  onSnapshot: (path) async {
                    expect(path, 'library.morrowbackup');
                    snapshotWrites++;
                  },
                  chooseSnapshotDestination: () async => 'library.morrowbackup',
                ),
              ),
            ),
          ),
        ),
      );
      await tester.tap(find.byKey(const ValueKey('backup-snapshot')));
      await tester.pumpAndSettle();
      expect(snapshotWrites, 1);
      expect(find.textContaining('内容库已备份'), findsOneWidget);
      expect(tester.takeException(), isNull);
    },
  );
}
