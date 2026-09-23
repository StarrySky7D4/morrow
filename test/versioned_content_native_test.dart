import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import 'package:capnproto_dart/capnproto_dart.dart';
import 'package:crypto/crypto.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/main.dart';
import 'package:morrow_studio/plugins/generated/host.capnp.dart' as wire;
import 'package:morrow_studio/plugins/generated/identity.dart' as contract;
import 'package:morrow_studio/plugins/host_request.dart';
import 'package:morrow_studio/plugins/workbench_backend.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';
import 'package:morrow_studio/plugins/versioned_content.dart';

final _hostExecutable = Platform.environment['MORROW_WORKBENCH_HOST'];
final _guestPackage = Platform.environment['MORROW_WORKBENCH_PACKAGE'];
final _python = Platform.environment['MORROW_CLOSE_TEST_PYTHON'];
final _nativeAvailable =
    Platform.isWindows && _hostExecutable != null && _guestPackage != null;

class _NativeWorkspace {
  _NativeWorkspace(this.directory);

  final Directory directory;
  RustWorkbench? backend;

  static Future<_NativeWorkspace> create() async => _NativeWorkspace(
    await Directory.systemTemp.createTemp('morrow-versioned-native-'),
  );

  Future<RustWorkbench> open({bool withPackage = true}) async {
    if (backend != null) throw StateError('close the previous host first');
    return backend = await RustWorkbench.open(
      executable: _hostExecutable!,
      package: withPackage
          ? _guestPackage!
          : '${directory.path}${Platform.pathSeparator}missing-plugin.morrowpkg',
      directory: directory,
    );
  }

  Future<void> stop() async {
    await backend?.close();
    backend = null;
  }

  Future<void> dispose() async {
    await stop();
    await directory.delete(recursive: true);
  }

  Future<Idea> createLegacy(String id, {String title = 'Original'}) async {
    final backend = this.backend ?? (throw StateError('host is not open'));
    return backend.apply(
      PluginAction.create,
      Idea(
        title,
        'original **body**',
        '进行中',
        Idea.icons[0],
        const Color(0xff8866aa),
        id: id,
        stage: '计划中',
        todos: const ['same', 'same'],
        completed: const {'same'},
      ),
    );
  }
}

void main() {
  test(
    'real guest migration, explicit duplicate decisions, card edit, and mixed query',
    () async {
      final workspace = await _NativeWorkspace.create();
      try {
        final host = await workspace.open();
        expect(host.writable, isTrue);
        await workspace.createLegacy('versioned-card');
        await workspace.createLegacy('still-legacy', title: 'Other card');
        final content = host.versionedContent;
        final legacy = await content.read('versioned-card');
        expect(legacy.formatVersion, 1);
        expect(legacy.revision, BigInt.one);
        expect(legacy.origin, isNull);
        expect(legacy.todos, ['same', 'same']);

        final plan = await content.planMigration('versioned-card');
        expect(plan.id, 'versioned-card');
        expect(plan.sourceRevision, legacy.revision);
        final migration = await content.migrate(plan);
        expect(migration.receipt.repeated, isFalse);
        expect(migration.receipt.operation, plan.operation);
        final migrated = migration.current;
        expect(migrated.formatVersion, 2);
        expect(migrated.revision, BigInt.from(2));
        expect(migrated.tasks.map((t) => t.text), ['same', 'same']);
        expect(migrated.tasks.map((t) => t.completion), [
          VersionedTaskCompletion.legacyAmbiguous,
          VersionedTaskCompletion.legacyAmbiguous,
        ]);
        expect(migrated.tasks[0].id, isNot(migrated.tasks[1].id));
        expect(migrated.ambiguousCount, 2);
        expect(migrated.origin?.cardId, 'versioned-card');
        expect(migrated.origin?.sourceRevision, legacy.revision);
        expect(migrated.origin?.originalTitle, legacy.title);
        expect(migrated.origin?.mapping.map((m) => m.taskId), [
          migrated.tasks[0].id,
          migrated.tasks[1].id,
        ]);
        expect(migrated.origin?.originalProperties, isNotEmpty);

        final first = migrated.tasks[0].id;
        final second = migrated.tasks[1].id;
        final completed = await content.editTasks(
          'native-decide-first',
          migrated.id,
          migrated.revision,
          TaskEditCommand.setCompletion(first, true),
        );
        expect(
          completed.current.tasks[0].completion,
          VersionedTaskCompletion.complete,
        );
        expect(
          completed.current.tasks[1].completion,
          VersionedTaskCompletion.legacyAmbiguous,
        );
        final decided = await content.editTasks(
          'native-decide-second',
          migrated.id,
          completed.current.revision,
          TaskEditCommand.setCompletion(second, false),
        );
        expect(decided.current.ambiguousCount, 0);
        expect(decided.current.completeCount, 1);
        expect(decided.current.incompleteCount, 1);

        final stage = await content.editTasks(
          'native-stage-only',
          migrated.id,
          decided.current.revision,
          TaskEditCommand.setStage('推进中'),
        );
        expect(stage.current.stage, '推进中');
        expect(stage.current.completeCount, 1);
        expect(stage.current.incompleteCount, 1);
        final edited = await content.editCard(
          'native-card-fields',
          migrated.id,
          stage.current.revision,
          CardEditCommand.edit(
            CardEditFields(
              title: 'Edited title',
              description: 'Edited **body**',
              hypothesis: 'A hypothesis',
              conclusion: 'A conclusion',
              icon: stage.current.icon,
              color: stage.current.color,
              assets: stage.current.assets,
            ),
          ),
        );
        expect(edited.current.title, 'Edited title');
        expect(edited.current.description, 'Edited **body**');
        expect(edited.current.tasks.map((t) => t.id), [first, second]);
        expect(edited.current.tasks.map((t) => t.completion), [
          VersionedTaskCompletion.complete,
          VersionedTaskCompletion.incomplete,
        ]);
        expect(edited.current.origin?.sourceRevision, legacy.revision);

        final page = await content.page();
        expect(page.ids, containsAll(['versioned-card', 'still-legacy']));
        expect((await content.read('still-legacy')).formatVersion, 1);
        final ids = await content.query(
          '概览',
          '全部',
          '',
          '最近添加',
          operation: 'native-mixed-query',
        );
        expect(ids, containsAll(['versioned-card', 'still-legacy']));

        // A stored receipt is historical; the client must separately read the
        // newer card before showing a result to the user.
        final retry = await content.editTasks(
          'native-decide-first',
          migrated.id,
          migrated.revision,
          TaskEditCommand.setCompletion(first, true),
        );
        expect(retry.receipt.repeated, isTrue);
        expect(retry.receipt.revision, completed.receipt.revision);
        expect(retry.current.revision, edited.current.revision);
        expect(retry.current.title, 'Edited title');
      } finally {
        await workspace.dispose();
      }
    },
    skip: !_nativeAvailable,
    timeout: const Timeout(Duration(minutes: 3)),
  );

  test(
    'migration and edit retry survive restart; missing package reads V2 only',
    () async {
      final workspace = await _NativeWorkspace.create();
      try {
        var host = await workspace.open();
        await workspace.createLegacy('restart-card');
        var content = host.versionedContent;
        final plan = await content.planMigration('restart-card');
        final migration = await content.migrate(plan);
        final task = migration.current.tasks.first;
        final command = TaskEditCommand.rename(task.id, 'Renamed task');
        final edit = await content.editTasks(
          'native-restart-edit',
          plan.id,
          migration.current.revision,
          command,
        );
        final newer = await content.editCard(
          'native-restart-favorite',
          plan.id,
          edit.current.revision,
          const CardEditCommand.setFavorite(true),
        );
        await workspace.stop();

        host = await workspace.open();
        content = host.versionedContent;
        final migrationRetry = await content.migrate(plan);
        expect(migrationRetry.receipt.repeated, isTrue);
        expect(migrationRetry.receipt.revision, migration.receipt.revision);
        expect(migrationRetry.current.revision, newer.current.revision);
        expect(migrationRetry.current.favorite, isTrue);
        final editRetry = await content.editTasks(
          'native-restart-edit',
          plan.id,
          migration.current.revision,
          command,
        );
        expect(editRetry.receipt.repeated, isTrue);
        expect(editRetry.receipt.revision, edit.receipt.revision);
        expect(editRetry.current.revision, newer.current.revision);
        expect(editRetry.current.tasks.first.text, 'Renamed task');
        await workspace.stop();

        host = await workspace.open(withPackage: false);
        expect(host.writable, isFalse);
        content = host.versionedContent;
        final readonly = await content.read(plan.id);
        expect(readonly.formatVersion, 2);
        expect(readonly.revision, newer.current.revision);
        expect(readonly.favorite, isTrue);
        expect((await content.page()).ids, contains(plan.id));
        await expectLater(
          content.editTasks(
            'native-readonly-edit',
            plan.id,
            readonly.revision,
            TaskEditCommand.setStage('不能提交'),
          ),
          throwsA(anything),
        );
        expect((await content.read(plan.id)).revision, readonly.revision);
      } finally {
        await workspace.dispose();
      }
    },
    skip: !_nativeAvailable,
    timeout: const Timeout(Duration(minutes: 3)),
  );

  test(
    'mixed snapshot scan preserves formats, task identity and tombstones after restart',
    () async {
      final workspace = await _NativeWorkspace.create();
      try {
        var host = await workspace.open();
        await workspace.createLegacy('scan-legacy');
        await workspace.createLegacy('scan-v2');
        await workspace.createLegacy('scan-deleted');
        final content = host.versionedContent;
        final migrated = (await content.migrate(
          await content.planMigration('scan-v2'),
        )).current;
        final deletedBase = (await content.migrate(
          await content.planMigration('scan-deleted'),
        )).current;
        final deleted = (await content.editCard(
          'snapshot-delete',
          deletedBase.id,
          deletedBase.revision,
          const CardEditCommand.delete(),
        )).current;
        final before = await host.loadVersioned(pageLimit: 1);
        expect(before, hasLength(3));
        expect(() => before.clear(), throwsUnsupportedError);
        final versions = {for (final item in before) item.id: item.revision};
        await workspace.stop();

        // Missing business code may disable writes, but it must not require a
        // downgrade or migration to read the existing mixed-format library.
        host = await workspace.open(withPackage: false);
        expect(host.writable, isFalse);
        final records = await host.loadVersioned(pageLimit: 1);
        expect(records, hasLength(3));
        expect({for (final item in records) item.id: item.revision}, versions);
        final byId = {for (final item in records) item.id: item};
        expect(byId['scan-legacy']!.formatVersion, 1);
        expect(byId['scan-legacy']!.todos, ['same', 'same']);
        final current = byId['scan-v2']!;
        expect(current.formatVersion, 2);
        expect(
          current.tasks.map((task) => task.id),
          migrated.tasks.map((task) => task.id),
        );
        expect(
          current.tasks.map((task) => task.completion),
          everyElement(VersionedTaskCompletion.legacyAmbiguous),
        );
        expect(current.ambiguousCount, 2);
        expect(
          current.origin!.originalProperties,
          migrated.origin!.originalProperties,
        );
        expect(byId['scan-deleted']!.deleted, isTrue);
        expect(byId['scan-deleted']!.revision, deleted.revision);
        expect(
          byId['scan-deleted']!.tasks.map((task) => task.id),
          deletedBase.tasks.map((task) => task.id),
        );
      } finally {
        await workspace.dispose();
      }
    },
    skip: !_nativeAvailable,
    timeout: const Timeout(Duration(minutes: 3)),
  );

  test(
    'versioned query pages one captured operation without rerunning it',
    () async {
      final workspace = await _NativeWorkspace.create();
      try {
        final host = await workspace.open();
        final expected = <String>[];
        for (var index = 0; index < 129; index++) {
          final id = 'page-${index.toString().padLeft(3, '0')}';
          expected.add(id);
          await workspace.createLegacy(id);
        }
        final content = host.versionedContent;
        Future<List<String>> original() => content.query(
          '概览',
          '全部',
          '',
          '最近添加',
          operation: 'native-paged-query-original',
        );
        final first = await original();
        expect(first, hasLength(129));
        expect(first, containsAll(expected));

        await workspace.createLegacy('page-newer');
        expect(await original(), first);
        final fresh = await content.query(
          '概览',
          '全部',
          '',
          '最近添加',
          operation: 'native-paged-query-new',
        );
        expect(fresh, hasLength(130));
        expect(fresh, containsAll([...expected, 'page-newer']));
      } finally {
        await workspace.dispose();
      }
    },
    skip: !_nativeAvailable,
    timeout: const Timeout(Duration(minutes: 5)),
  );

  test(
    'confirmed edit with failed current read retains receipt and revision',
    () async {
      final directory = await Directory.systemTemp.createTemp(
        'morrow-versioned-read-failure-',
      );
      RustWorkbench? host;
      try {
        // The proxy forwards all frames to the real managed host and replaces
        // exactly one post-commit ReadVersioned reply with a valid host error.
        await File(
          'test/fixtures/service_reply_proxy.py',
        ).copy('${directory.path}/workbench.db');
        await File('${directory.path}/proxy.json').writeAsString(
          jsonEncode({
            'host': _hostExecutable,
            'package': _guestPackage,
            'store': '${directory.path}/store',
            'mode': 'replace',
            'trace_requests': true,
          }),
        );
        final failure = MessageBuilder();
        final reply = failure.initRoot(wire.responseFactory);
        reply.version = 1;
        reply.digest = Uint8List.fromList(contract.hostDigest);
        reply.error = 'injected post-commit versioned read failure';
        await File(
          '${directory.path}/replacement.bin',
        ).writeAsBytes(failure.serialize());
        host = await RustWorkbench.open(
          executable: _python!,
          package: 'proxy',
          directory: directory,
        );
        await host.apply(
          PluginAction.create,
          Idea(
            'Failure card',
            'body',
            '进行中',
            Idea.icons[0],
            const Color(0xff8866aa),
            id: 'versioned-failure-card',
            todos: const ['task'],
          ),
        );
        final content = host.versionedContent;
        final plan = await content.planMigration('versioned-failure-card');
        final migrated = await content.migrate(plan);
        final taskId = migrated.current.tasks.single.id;

        await sendHostRequest(
          wire.Action.readVersioned,
          configure: (request) => request.id = plan.id,
          send: (bytes) async {
            await File(
              '${directory.path}/armed.sha256',
            ).writeAsString(sha256.convert(bytes).toString());
          },
        );
        const operation = 'native-confirmed-read-failure';
        final command = TaskEditCommand.setCompletion(taskId, true);
        VersionedCommittedRefreshFailure? confirmed;
        try {
          await content.editTasks(
            operation,
            plan.id,
            migrated.current.revision,
            command,
          );
          fail('the selected post-commit read should have failed');
        } on VersionedCommittedRefreshFailure catch (error) {
          confirmed = error;
        }
        expect(confirmed, isNotNull);
        expect(confirmed.receipt.id, plan.id);
        expect(confirmed.receipt.operation, operation);
        expect(confirmed.receipt.repeated, isFalse);
        expect(
          confirmed.receipt.revision,
          migrated.current.revision + BigInt.one,
        );
        expect(host.knownContentRevision(plan.id), confirmed.receipt.revision);
        expect(host.knownContentDeleted(plan.id), isFalse);

        final retry = await content.editTasks(
          operation,
          plan.id,
          migrated.current.revision,
          command,
        );
        expect(retry.receipt.repeated, isTrue);
        expect(retry.receipt.revision, confirmed.receipt.revision);
        expect(retry.current.revision, confirmed.receipt.revision);
        expect(
          retry.current.tasks.single.completion,
          VersionedTaskCompletion.complete,
        );
        final trace =
            (await File('${directory.path}/trace.jsonl').readAsLines())
                .map((line) => jsonDecode(line) as Map)
                .where((entry) => entry['event'] == 'request')
                .map((entry) {
                  final hex = entry['hex'] as String;
                  final bytes = Uint8List.fromList([
                    for (var offset = 0; offset < hex.length; offset += 2)
                      int.parse(hex.substring(offset, offset + 2), radix: 16),
                  ]);
                  return RustWorkbench.readMessage(
                    bytes,
                  ).getRoot(wire.requestFactory);
                })
                .where((request) => request.action == wire.Action.editTasks)
                .map((request) => request.operation)
                .toList();
        expect(trace, [operation, operation]);
      } finally {
        await host?.close();
        await directory.delete(recursive: true);
      }
    },
    skip: !_nativeAvailable || _python == null,
    timeout: const Timeout(Duration(minutes: 3)),
  );
}
