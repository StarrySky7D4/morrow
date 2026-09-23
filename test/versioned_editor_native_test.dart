import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import 'package:capnproto_dart/capnproto_dart.dart';
import 'package:crypto/crypto.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/attachments/attachment.dart';
import 'package:morrow_studio/main.dart';
import 'package:morrow_studio/media/texture_source.dart';
import 'package:morrow_studio/plugins/editor_session.dart';
import 'package:morrow_studio/plugins/generated/host.capnp.dart' as wire;
import 'package:morrow_studio/plugins/generated/identity.dart' as contract;
import 'package:morrow_studio/plugins/host_request.dart';
import 'package:morrow_studio/plugins/versioned_content.dart';
import 'package:morrow_studio/plugins/versioned_editor.dart';
import 'package:morrow_studio/plugins/workbench_backend.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';

final _executable = Platform.environment['MORROW_WORKBENCH_HOST'];
final _package = Platform.environment['MORROW_WORKBENCH_PACKAGE'];
final _python = Platform.environment['MORROW_CLOSE_TEST_PYTHON'];
final _nativeAvailable =
    Platform.isWindows && _executable != null && _package != null;

void main() {
  test(
    'real V2 editor captures HTML paste, stages an asset, and retries historical save',
    () async {
      final directory = await Directory.systemTemp.createTemp(
        'morrow-versioned-editor-',
      );
      RustWorkbench? host;
      VersionedEditorSession? editor;
      try {
        host = await RustWorkbench.open(
          executable: _executable!,
          package: _package!,
          directory: directory,
        );
        const id = 'versioned-editor-card';
        await host.apply(
          PluginAction.create,
          Idea(
            'Original',
            'initial body',
            '进行中',
            Idea.icons[0],
            const Color(0xff8866aa),
            id: id,
            stage: '计划中',
            todos: const ['same', 'same'],
            completed: const {'same'},
          ),
        );
        await expectLater(host.openVersionedEditor(id), throwsStateError);
        final content = host.versionedContent;
        final plan = await content.planMigration(id);
        final migrated = (await content.migrate(plan)).current;
        final taskIds = migrated.tasks.map((task) => task.id).toList();
        final completions = migrated.tasks
            .map((task) => task.completion)
            .toList();

        editor = await host.openVersionedEditor(id);
        expect(editor.targetId, id);
        final converted = await editor.studio.capture(
          'html',
          '<strong>Captured</strong> &amp; <em>safe</em>',
        );
        expect(converted.ticket, isNotEmpty);
        const before = 'A😀B';
        final after = before.replaceRange(1, 3, converted.markdown);
        await editor.recordPaste(
          PasteInsertion(
            id: 'versioned-html-paste',
            field: 'description',
            before: before,
            startUtf16: 1,
            endUtf16: 3,
            parts: [PastePart.ticket(converted.ticket!)],
            after: after,
          ),
        );
        await expectLater(
          editor.recordPaste(
            const PasteInsertion(
              id: 'rejected-checklist-paste',
              field: 'todos',
              before: '',
              startUtf16: 0,
              endUtf16: 0,
              parts: [PastePart.literal('not a TaskId edit')],
              after: 'not a TaskId edit',
            ),
          ),
          throwsFormatException,
        );

        final file = File('${directory.path}/asset source.bin');
        await file.writeAsBytes([0, 1, 127, 255]);
        final staged = await editor.stageAttachment(
          IdeaAttachment(
            source: TextureSource(
              location: file.path,
              name: 'original file.bin',
              kind: TextureKind.file,
              local: true,
            ),
            size: 4,
          ),
        );
        final rawDescription =
            '$after\n[original](attachment:${Uri.encodeComponent(file.path)})';
        final fields = EditorFields(
          title: '  Edited title  ',
          description: rawDescription,
          hypothesis: '  hypothesis  ',
          conclusion: '  conclusion  ',
          todos: '',
        );
        final normalized = CardEditFields(
          title: 'Edited title',
          description: '$after\n[original](attachment:${staged.id})',
          hypothesis: 'hypothesis',
          conclusion: 'conclusion',
          icon: migrated.icon,
          color: migrated.color,
          assets: [staged],
        );
        final saved = await editor.save(normalized, fields);
        expect(saved.receipt.repeated, isFalse);
        expect(saved.current.title, 'Edited title');
        expect(saved.current.description, normalized.description);
        expect(saved.current.assets.single.id, staged.id);
        expect(saved.current.tasks.map((task) => task.id), taskIds);
        expect(saved.current.tasks.map((task) => task.completion), completions);
        expect(
          saved.current.origin?.sourceRevision,
          migrated.origin?.sourceRevision,
        );
        final originalRetry = await editor.save(normalized, fields);
        expect(originalRetry.receipt.repeated, isTrue);
        expect(originalRetry.receipt.revision, saved.receipt.revision);

        final decided = await content.editTasks(
          'versioned-editor-later-task-decision',
          id,
          saved.current.revision,
          TaskEditCommand.setCompletion(taskIds.first, true),
        );
        final historical = await editor.save(normalized, fields);
        expect(historical.receipt.repeated, isTrue);
        expect(historical.receipt.revision, saved.receipt.revision);
        expect(historical.current.revision, decided.current.revision);
        expect(
          historical.current.tasks.first.completion,
          VersionedTaskCompletion.complete,
        );
        expect(historical.current.description, normalized.description);

        final changed = CardEditFields(
          title: 'Different intent',
          description: normalized.description,
          hypothesis: normalized.hypothesis,
          conclusion: normalized.conclusion,
          icon: normalized.icon,
          color: normalized.color,
          assets: normalized.assets,
        );
        await expectLater(editor.save(changed, fields), throwsStateError);
      } finally {
        await editor?.close();
        await host?.close();
        await directory.delete(recursive: true);
      }
    },
    skip: !_nativeAvailable,
    timeout: const Timeout(Duration(minutes: 3)),
  );

  test(
    'captured card commit survives a failed current read with its original operation',
    () async {
      final directory = await Directory.systemTemp.createTemp(
        'morrow-versioned-editor-read-failure-',
      );
      RustWorkbench? host;
      VersionedEditorSession? editor;
      try {
        await File(
          'test/fixtures/service_reply_proxy.py',
        ).copy('${directory.path}/workbench.db');
        await File('${directory.path}/proxy.json').writeAsString(
          jsonEncode({
            'host': _executable,
            'package': _package,
            'store': '${directory.path}/store',
            'mode': 'replace',
            'trace_requests': true,
          }),
        );
        final failureReply = MessageBuilder();
        final reply = failureReply.initRoot(wire.responseFactory);
        reply.version = 1;
        reply.digest = Uint8List.fromList(contract.hostDigest);
        reply.error = 'injected captured-card current read failure';
        await File(
          '${directory.path}/replacement.bin',
        ).writeAsBytes(failureReply.serialize());
        host = await RustWorkbench.open(
          executable: _python!,
          package: 'proxy',
          directory: directory,
        );
        const id = 'captured-card-failure';
        await host.apply(
          PluginAction.create,
          Idea(
            'Before',
            'body',
            '进行中',
            Idea.icons[0],
            const Color(0xff8866aa),
            id: id,
            todos: const ['task'],
          ),
        );
        final content = host.versionedContent;
        final migrated = (await content.migrate(
          await content.planMigration(id),
        )).current;
        editor = await host.openVersionedEditor(id);
        final fields = CardEditFields(
          title: 'Confirmed edit',
          description: 'body after commit',
          hypothesis: migrated.hypothesis,
          conclusion: migrated.conclusion,
          icon: migrated.icon,
          color: migrated.color,
          assets: migrated.assets,
        );
        final snapshot = EditorFields(
          title: fields.title,
          description: fields.description,
          hypothesis: fields.hypothesis,
          conclusion: fields.conclusion,
          todos: '',
        );
        await sendHostRequest(
          wire.Action.readVersioned,
          configure: (request) => request.id = id,
          send: (bytes) async {
            await File(
              '${directory.path}/armed.sha256',
            ).writeAsString(sha256.convert(bytes).toString());
          },
        );
        VersionedCommittedRefreshFailure? confirmed;
        try {
          await editor.save(fields, snapshot);
          fail('selected post-commit read should have failed');
        } on VersionedCommittedRefreshFailure catch (error) {
          confirmed = error;
        }
        expect(confirmed, isNotNull);
        expect(confirmed.receipt.id, id);
        expect(confirmed.receipt.repeated, isFalse);
        expect(confirmed.receipt.revision, migrated.revision + BigInt.one);
        expect(host.knownContentRevision(id), confirmed.receipt.revision);

        final different = CardEditFields(
          title: 'Changed after confirmed commit',
          description: fields.description,
          hypothesis: fields.hypothesis,
          conclusion: fields.conclusion,
          icon: fields.icon,
          color: fields.color,
          assets: fields.assets,
        );
        await expectLater(editor.save(different, snapshot), throwsStateError);
        final retry = await editor.save(fields, snapshot);
        expect(retry.receipt.repeated, isTrue);
        expect(retry.receipt.operation, confirmed.receipt.operation);
        expect(retry.receipt.revision, confirmed.receipt.revision);
        expect(retry.current.revision, confirmed.receipt.revision);
        expect(retry.current.title, fields.title);
        expect(retry.current.tasks.single.id, migrated.tasks.single.id);

        final beginOperations =
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
                .where(
                  (request) => request.action == wire.Action.beginCaptureUpload,
                )
                .map((request) => request.operation)
                .toList();
        expect(beginOperations, [
          confirmed.receipt.operation,
          confirmed.receipt.operation,
        ]);
      } finally {
        await editor?.close();
        await host?.close();
        await directory.delete(recursive: true);
      }
    },
    skip: !_nativeAvailable || _python == null,
    timeout: const Timeout(Duration(minutes: 3)),
  );
}
