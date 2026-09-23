import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import 'package:capnproto_dart/capnproto_dart.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/main.dart';
import 'package:morrow_studio/plugins/editor_recovery.dart';
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

Future<VersionedContentRecord> _seed(RustWorkbench host, String id) async {
  await host.apply(
    PluginAction.create,
    Idea(
      'Before recovery',
      'original body',
      '进行中',
      Idea.icons[0],
      const Color(0xff8866aa),
      id: id,
      stage: '计划中',
      todos: const ['keep TaskId'],
    ),
  );
  final content = host.versionedContent;
  final plan = await content.planMigration(id);
  return (await content.migrate(plan)).current;
}

(CardEditFields, EditorFields) _edit(VersionedContentRecord source) {
  final fields = CardEditFields(
    title: 'Recovered title',
    description: 'original body after save',
    hypothesis: source.hypothesis,
    conclusion: source.conclusion,
    icon: source.icon,
    color: source.color,
    assets: source.assets,
  );
  final snapshot = EditorFields(
    title: fields.title,
    description: fields.description,
    hypothesis: fields.hypothesis,
    conclusion: fields.conclusion,
    todos: '',
  );
  return (fields, snapshot);
}

EditorRecovery _altered(
  EditorRecovery original, {
  String? operation,
  List<int>? digest,
}) => EditorRecovery(
  id: original.id,
  title: original.title,
  operation: operation ?? original.operation,
  digest: digest ?? original.digest,
  sourceRevision: original.sourceRevision,
  currentRevision: original.currentRevision,
  status: original.status,
);

String _hex(List<int> bytes) =>
    bytes.map((byte) => byte.toRadixString(16).padLeft(2, '0')).join();

Future<Uint8List> _finishFrame(String token) async {
  late Uint8List frame;
  await sendHostRequest(
    wire.Action.finishCapturedCard,
    configure: (request) => request.transfer = token,
    send: (bytes) async => frame = Uint8List.fromList(bytes),
  );
  return frame;
}

Future<void> _armLostFinishReply(Directory directory) async {
  final first = await _finishFrame('upload-0');
  final second = await _finishFrame('upload-9');
  expect(second.length, first.length);
  final mask = <int>[
    for (var i = 0; i < first.length; i++)
      if (first[i] == second[i]) 255 else 0,
  ];
  expect(mask.where((value) => value == 0), hasLength(1));
  await File(
    '${directory.path}/armed.mask.json',
  ).writeAsString(jsonEncode({'template': _hex(first), 'mask': _hex(mask)}));
}

Future<int> _finishRequests(Directory directory) async {
  final entries = await File('${directory.path}/trace.jsonl').readAsLines();
  var count = 0;
  for (final line in entries) {
    final event = jsonDecode(line) as Map<String, dynamic>;
    if (event['event'] != 'request') continue;
    final encoded = event['hex'] as String;
    final bytes = Uint8List.fromList([
      for (var offset = 0; offset < encoded.length; offset += 2)
        int.parse(encoded.substring(offset, offset + 2), radix: 16),
    ]);
    final request = RustWorkbench.readMessage(
      bytes,
    ).getRoot(wire.requestFactory);
    if (request.action == wire.Action.finishCapturedCard) count++;
  }
  return count;
}

Future<void> _executeSql(Directory directory, String statement) async {
  const script = '''
import sqlite3
import sys
with sqlite3.connect(sys.argv[1]) as db:
    db.executescript(sys.argv[2])
''';
  final result = await Process.run(_python!, [
    '-c',
    script,
    '${directory.path}/workbench.db',
    statement,
  ]);
  expect(result.exitCode, 0, reason: '${result.stderr}');
}

void main() {
  test(
    'committed editor proposal remains inspectable after native host restart until exact ack',
    () async {
      final directory = await Directory.systemTemp.createTemp(
        'morrow-editor-recovery-',
      );
      RustWorkbench? host;
      VersionedEditorSession? editor;
      try {
        host = await RustWorkbench.open(
          executable: _executable!,
          package: _package!,
          directory: directory,
        );
        const id = 'editor-recovery-committed';
        final migrated = await _seed(host, id);
        editor = await host.openVersionedEditor(id);
        final (fields, snapshot) = _edit(migrated);
        final saved = await editor.save(fields, snapshot);
        expect(saved.receipt.repeated, isFalse);
        expect(saved.current.revision, migrated.revision + BigInt.one);
        final original = (await host.inspectEditorRecoveries(id: id)).single;
        expect(original.status, EditorRecoveryStatus.committed);
        expect(original.operation, saved.receipt.operation);
        expect(original.sourceRevision, migrated.revision);
        expect(original.currentRevision, saved.current.revision);
        expect(original.digest, hasLength(32));

        await editor.close();
        editor = null;
        await host.close();
        host = await RustWorkbench.open(
          executable: _executable!,
          package: _package!,
          directory: directory,
        );
        final before = await host.versionedContent.read(id);
        final all = await host.inspectEditorRecoveries();
        expect(all.map((row) => row.id), contains(id));
        final observed = (await host.inspectEditorRecoveries(id: id)).single;
        final after = await host.versionedContent.read(id);
        expect(after.revision, before.revision);
        expect(after.title, before.title);
        expect(observed.operation, original.operation);
        expect(observed.digest, original.digest);
        expect(observed.status, EditorRecoveryStatus.committed);
        expect(observed.currentRevision, before.revision);
        await expectLater(
          host.abandonEditorRecovery(observed),
          throwsStateError,
        );
        expect(
          (await host.inspectEditorRecoveries(id: id)).single.status,
          EditorRecoveryStatus.committed,
        );

        await expectLater(
          host.acknowledgeEditorRecovery(
            _altered(observed, operation: 'different-operation'),
          ),
          throwsStateError,
        );
        await expectLater(
          host.acknowledgeEditorRecovery(
            _altered(observed, digest: List<int>.filled(32, 0)),
          ),
          throwsStateError,
        );
        expect(
          (await host.inspectEditorRecoveries(id: id)).single.operation,
          observed.operation,
        );
        await host.acknowledgeEditorRecovery(observed);
        await host.acknowledgeEditorRecovery(observed);
        await expectLater(
          host.abandonEditorRecovery(observed),
          throwsStateError,
        );
        expect(await host.inspectEditorRecoveries(id: id), isEmpty);
        expect(
          (await host.versionedContent.read(id)).revision,
          before.revision,
        );
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
    'lost FinishCapturedCard reply resumes original committed operation after restart',
    () async {
      final directory = await Directory.systemTemp.createTemp(
        'morrow-editor-recovery-lost-reply-',
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
        final replacement = MessageBuilder();
        final reply = replacement.initRoot(wire.responseFactory);
        reply.version = 1;
        reply.digest = Uint8List.fromList(contract.hostDigest);
        reply.error = 'injected FinishCapturedCard reply loss';
        await File(
          '${directory.path}/replacement.bin',
        ).writeAsBytes(replacement.serialize());
        host = await RustWorkbench.open(
          executable: _python!,
          package: 'proxy',
          directory: directory,
        );
        const id = 'editor-recovery-lost-reply';
        final migrated = await _seed(host, id);
        editor = await host.openVersionedEditor(id);
        final (fields, snapshot) = _edit(migrated);
        await _armLostFinishReply(directory);
        await expectLater(editor.save(fields, snapshot), throwsStateError);
        final actualReply = RustWorkbench.readMessage(
          await File('${directory.path}/receipt.bin').readAsBytes(),
        ).getRoot(wire.responseFactory);
        expect(actualReply.error ?? '', isEmpty);
        expect(await _finishRequests(directory), 1);
        final committed = (await host.inspectEditorRecoveries(id: id)).single;
        expect(committed.status, EditorRecoveryStatus.committed);
        expect(committed.sourceRevision, migrated.revision);
        expect(committed.currentRevision, migrated.revision + BigInt.one);
        expect(
          (await host.versionedContent.read(id)).revision,
          committed.currentRevision,
        );

        await editor.close();
        editor = null;
        await host.close();
        host = await RustWorkbench.open(
          executable: _executable!,
          package: _package!,
          directory: Directory('${directory.path}/store'),
          managed: true,
        );
        final observed = (await host.inspectEditorRecoveries(id: id)).single;
        expect(observed.operation, committed.operation);
        expect(observed.digest, committed.digest);
        expect(observed.status, EditorRecoveryStatus.committed);
        final replayed = await host.resumeEditorRecovery(observed);
        expect(replayed.receipt.repeated, isTrue);
        expect(replayed.receipt.operation, committed.operation);
        expect(replayed.receipt.revision, committed.currentRevision);
        expect(replayed.current.revision, committed.currentRevision);
        expect(replayed.current.title, fields.title);
        expect(await _finishRequests(directory), 1);
        await host.acknowledgeEditorRecovery(observed);
        expect(await host.inspectEditorRecoveries(id: id), isEmpty);
      } finally {
        await editor?.close();
        await host?.close();
        await directory.delete(recursive: true);
      }
    },
    skip: !_nativeAvailable || _python == null,
    timeout: const Timeout(Duration(minutes: 3)),
  );

  test(
    'failed business commit can be abandoned after restart and fresh edit succeeds',
    () async {
      final directory = await Directory.systemTemp.createTemp(
        'morrow-editor-recovery-abandon-',
      );
      RustWorkbench? host;
      VersionedEditorSession? editor;
      try {
        host = await RustWorkbench.open(
          executable: _executable!,
          package: _package!,
          directory: directory,
        );
        const id = 'editor-recovery-abandon';
        final migrated = await _seed(host, id);
        editor = await host.openVersionedEditor(id);
        final (fields, snapshot) = _edit(migrated);
        await _executeSql(
          directory,
          "CREATE TRIGGER review_editor_business_failure "
          "BEFORE UPDATE OF payload ON cards "
          "WHEN OLD.id='editor-recovery-abandon' "
          "BEGIN SELECT RAISE(ABORT,'business card commit blocked'); END;",
        );
        await expectLater(editor.save(fields, snapshot), throwsStateError);
        final pending = (await host.inspectEditorRecoveries(id: id)).single;
        expect(pending.status, EditorRecoveryStatus.pending);
        expect(pending.sourceRevision, migrated.revision);
        expect(pending.currentRevision, migrated.revision);
        expect(
          (await host.versionedContent.read(id)).revision,
          migrated.revision,
        );
        await _executeSql(
          directory,
          'DROP TRIGGER review_editor_business_failure;',
        );
        await editor.close();
        editor = null;
        await host.close();
        host = await RustWorkbench.open(
          executable: _executable!,
          package: _package!,
          directory: directory,
        );
        final observed = (await host.inspectEditorRecoveries(id: id)).single;
        expect(observed.operation, pending.operation);
        expect(observed.digest, pending.digest);
        expect(observed.status, EditorRecoveryStatus.pending);
        await host.abandonEditorRecovery(observed);
        await host.abandonEditorRecovery(observed);
        expect(await host.inspectEditorRecoveries(id: id), isEmpty);
        expect(
          (await host.versionedContent.read(id)).revision,
          migrated.revision,
        );
        await expectLater(
          host.acknowledgeEditorRecovery(observed),
          throwsStateError,
        );
        await expectLater(
          host.resumeEditorRecovery(observed),
          throwsStateError,
        );

        editor = await host.openVersionedEditor(id);
        final freshFields = CardEditFields(
          title: 'Fresh edit after abandon',
          description: fields.description,
          hypothesis: fields.hypothesis,
          conclusion: fields.conclusion,
          icon: fields.icon,
          color: fields.color,
          assets: fields.assets,
        );
        final freshSnapshot = EditorFields(
          title: freshFields.title,
          description: freshFields.description,
          hypothesis: freshFields.hypothesis,
          conclusion: freshFields.conclusion,
          todos: '',
        );
        final saved = await editor.save(freshFields, freshSnapshot);
        expect(saved.receipt.repeated, isFalse);
        expect(saved.receipt.operation, isNot(observed.operation));
        expect(saved.current.revision, migrated.revision + BigInt.one);
        expect(saved.current.title, freshFields.title);
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
