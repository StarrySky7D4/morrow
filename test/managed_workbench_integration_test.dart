import 'dart:io';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/main.dart';
import 'package:morrow_studio/plugins/workbench_backend.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';

void main() {
  final executable = Platform.environment['MORROW_WORKBENCH_HOST'];
  final package = Platform.environment['MORROW_WORKBENCH_PACKAGE'];
  test(
    'managed restart follows restored library and key recovery uses selected directory',
    () async {
      final root = await Directory.systemTemp.createTemp('morrow-managed-');
      RustWorkbench? backend;
      Future<RustWorkbench> open() => RustWorkbench.open(
        executable: executable!,
        package: package!,
        directory: root,
        managed: true,
      );
      try {
        backend = await open();
        await backend.apply(
          PluginAction.create,
          Idea(
            '备份中的卡片',
            '保留正文',
            '灵感',
            Idea.icons[0],
            const Color(0xff8866aa),
            id: 'restored-card',
          ),
        );
        final snapshot = '${root.path}/archive.morrowbackup';
        final keyBackup = '${root.path}/key.backup';
        await backend.backupProtection(keyBackup);
        await backend.backupSnapshot(snapshot);
        await backend.apply(
          PluginAction.create,
          Idea(
            '备份后的卡片',
            '原库保留',
            '灵感',
            Idea.icons[0],
            const Color(0xff8866aa),
            id: 'later-card',
          ),
        );
        final restored = Directory('${root.path}/restored');
        await RustWorkbench.restoreSnapshot(
          executable: executable!,
          archive: snapshot,
          destination: restored,
        );
        final selectionFile = File('${root.path}/active-library.pb.lz4');
        final originalSelection = await selectionFile.readAsBytes();
        await expectLater(
          RustWorkbench.activateLibrary(
            executable: executable,
            root: root,
            selected: restored,
          ),
          throwsStateError,
        );
        expect(await selectionFile.readAsBytes(), originalSelection);
        await backend.close();
        backend = null;
        final originalDb = File('${root.path}/workbench.db');
        final originalBytes = await originalDb.readAsBytes();
        await RustWorkbench.activateLibrary(
          executable: executable,
          root: root,
          selected: restored,
        );
        backend = await open();
        expect((await backend.load()).map((c) => c.id).toList(), [
          'restored-card',
        ]);
        await backend.close();
        backend = null;
        backend =
            await open(); // A new process must resolve persisted selection.
        expect((await backend.load()).single.description, '保留正文');
        await backend.close();
        backend = null;
        final selectedKey = File('${restored.path}/workbench.db.audit-key');
        final originalRootKey = await File(
          '${root.path}/workbench.db.audit-key',
        ).readAsBytes();
        await selectedKey.delete();
        await expectLater(open(), throwsStateError);
        expect(await selectedKey.exists(), isFalse);
        await RustWorkbench.restoreKey(
          executable: executable,
          directory: root,
          selected: keyBackup,
          managed: true,
        );
        expect(
          await selectedKey.readAsBytes(),
          await File(keyBackup).readAsBytes(),
        );
        expect(
          await File('${root.path}/workbench.db.audit-key').readAsBytes(),
          originalRootKey,
        );
        backend = await open();
        expect((await backend.load()).single.id, 'restored-card');
        await backend.close();
        backend = null;
        expect(await originalDb.readAsBytes(), originalBytes);
        await restored.rename('${root.path}/moved');
        await expectLater(open(), throwsStateError);
        expect(await restored.exists(), isFalse);
        expect(await originalDb.readAsBytes(), originalBytes);
      } finally {
        await backend?.close();
        await root.delete(recursive: true);
      }
    },
    skip: executable == null || package == null || !Platform.isWindows,
    timeout: const Timeout(Duration(minutes: 2)),
  );
}
