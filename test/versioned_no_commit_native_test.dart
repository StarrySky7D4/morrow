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
import 'package:morrow_studio/plugins/versioned_content.dart';
import 'package:morrow_studio/plugins/versioned_content_codec.dart';
import 'package:morrow_studio/plugins/workbench_backend.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';

final _executable = Platform.environment['MORROW_WORKBENCH_HOST'];
final _package = Platform.environment['MORROW_WORKBENCH_PACKAGE'];
final _python = Platform.environment['MORROW_CLOSE_TEST_PYTHON'];
final _available =
    Platform.isWindows && _executable != null && _package != null;

Future<VersionedContentRecord> _seed(RustWorkbench host) async {
  await host.apply(
    PluginAction.create,
    Idea(
      'NoCommit card',
      'body',
      '进行中',
      Idea.icons[0],
      const Color(0xff8866aa),
      id: 'no-commit-card',
      stage: '计划中',
      todos: const ['task'],
    ),
  );
  final content = host.versionedContent;
  return (await content.migrate(
    await content.planMigration('no-commit-card'),
  )).current;
}

void main() {
  test(
    'real host proves source conflict and invalid task absent, then accepts fresh intent',
    () async {
      final directory = await Directory.systemTemp.createTemp(
        'morrow-no-commit-',
      );
      RustWorkbench? host;
      try {
        host = await RustWorkbench.open(
          executable: _executable!,
          package: _package!,
          directory: directory,
        );
        final source = await _seed(host);
        final content = host.versionedContent;
        final changed = await content.editCard(
          'advance-source',
          source.id,
          source.revision,
          const CardEditCommand.setFavorite(true),
        );
        await expectLater(
          content.editCard(
            'stale-source',
            source.id,
            source.revision,
            const CardEditCommand.setFavorite(false),
          ),
          throwsA(
            isA<VersionedMutationNoCommit>()
                .having((error) => error.id, 'card', source.id)
                .having((error) => error.operation, 'operation', 'stale-source')
                .having(
                  (error) => error.sourceRevision,
                  'source',
                  source.revision,
                ),
          ),
        );
        await expectLater(
          content.editTasks(
            'invalid-task',
            source.id,
            changed.current.revision,
            TaskEditCommand.remove('nonexistent-task'),
          ),
          throwsA(
            isA<VersionedMutationNoCommit>()
                .having((error) => error.id, 'card', source.id)
                .having((error) => error.operation, 'operation', 'invalid-task')
                .having(
                  (error) => error.sourceRevision,
                  'source',
                  changed.current.revision,
                ),
          ),
        );
        final afterRejects = await content.read(source.id);
        expect(afterRejects.revision, changed.current.revision);
        expect(afterRejects.favorite, isTrue);
        expect(afterRejects.tasks, hasLength(1));
        final accepted = await content.editTasks(
          'fresh-intent',
          source.id,
          afterRejects.revision,
          TaskEditCommand.rename(source.tasks.single.id, 'Recovered draft'),
        );
        expect(accepted.current.tasks.single.text, 'Recovered draft');
        expect(accepted.current.revision, afterRejects.revision + BigInt.one);
      } finally {
        await host?.close();
        await directory.delete(recursive: true);
      }
    },
    skip: !_available,
    timeout: const Timeout(Duration(minutes: 3)),
  );

  test(
    'real historical operation mismatch never claims no commit, including after reopen',
    () async {
      final directory = await Directory.systemTemp.createTemp(
        'morrow-no-commit-history-',
      );
      RustWorkbench? host;
      try {
        host = await RustWorkbench.open(
          executable: _executable!,
          package: _package!,
          directory: directory,
        );
        final source = await _seed(host);
        const operation = 'historical-success';
        const original = CardEditCommand.setFavorite(true);
        final saved = await host.versionedContent.editCard(
          operation,
          source.id,
          source.revision,
          original,
        );
        await host.close();
        host = null;
        host = await RustWorkbench.open(
          executable: _executable!,
          package: _package!,
          directory: directory,
        );
        await expectLater(
          host.versionedContent.editCard(
            operation,
            source.id,
            source.revision,
            const CardEditCommand.setFavorite(false),
          ),
          throwsA(
            allOf(isA<StateError>(), isNot(isA<VersionedMutationNoCommit>())),
          ),
        );
        final retried = await host.versionedContent.editCard(
          operation,
          source.id,
          source.revision,
          original,
        );
        expect(retried.receipt.repeated, isTrue);
        expect(retried.receipt.revision, saved.receipt.revision);
        expect(retried.current.favorite, isTrue);
      } finally {
        await host?.close();
        await directory.delete(recursive: true);
      }
    },
    skip: !_available,
    timeout: const Timeout(Duration(minutes: 3)),
  );

  for (final malformedMarker in ['generic', 'payload', 'revision', 'read']) {
    test(
      'lost receipt stays unknown and exact retry commits once: $malformedMarker',
      () async {
        final directory = await Directory.systemTemp.createTemp(
          'morrow-no-commit-lost-',
        );
        RustWorkbench? host;
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
          final message = MessageBuilder();
          final reply = message.initRoot(wire.responseFactory);
          reply.version = 1;
          reply.digest = Uint8List.fromList(contract.hostDigest);
          // An apparently deterministic error string is not authoritative evidence.
          reply.error = 'task source revision conflict';
          if (malformedMarker != 'generic') reply.uiCode = 1001;
          if (malformedMarker == 'payload') {
            reply.payload = Uint8List.fromList([1]);
          }
          if (malformedMarker == 'revision') reply.revision = 3;
          await File(
            '${directory.path}/replacement.bin',
          ).writeAsBytes(message.serialize());
          host = await RustWorkbench.open(
            executable: _python!,
            package: 'proxy',
            directory: directory,
          );
          final source = await _seed(host);
          const operation = 'lost-receipt';
          final command = TaskEditCommand.rename(
            source.tasks.single.id,
            'Committed once',
          );
          await sendHostRequest(
            malformedMarker == 'read'
                ? wire.Action.readVersioned
                : wire.Action.editTasks,
            configure: (request) {
              request.id = source.id;
              if (malformedMarker == 'read') return;
              request.operation = operation;
              request.revision = VersionedContentCodec.wireU64(source.revision);
              request.payload = VersionedContentCodec.encodeTaskEdit(command);
            },
            send: (bytes) async {
              await File(
                '${directory.path}/armed.sha256',
              ).writeAsString(sha256.convert(bytes).toString());
            },
          );
          await expectLater(
            host.versionedContent.editTasks(
              operation,
              source.id,
              source.revision,
              command,
            ),
            throwsA(
              malformedMarker == 'read'
                  ? isA<VersionedCommittedRefreshFailure>()
                  : allOf(
                      isA<StateError>(),
                      isNot(isA<VersionedMutationNoCommit>()),
                    ),
            ),
          );
          final actualReply = RustWorkbench.readMessage(
            await File('${directory.path}/receipt.bin').readAsBytes(),
          ).getRoot(wire.responseFactory);
          expect(actualReply.error ?? '', isEmpty);
          final retry = await host.versionedContent.editTasks(
            operation,
            source.id,
            source.revision,
            command,
          );
          expect(retry.receipt.repeated, isTrue);
          expect(retry.current.revision, source.revision + BigInt.one);
          expect(retry.current.tasks.single.text, 'Committed once');
          final requests =
              (await File('${directory.path}/trace.jsonl').readAsLines())
                  .map((line) => jsonDecode(line) as Map<String, dynamic>)
                  .where((event) => event['event'] == 'request')
                  .map((event) {
                    final hex = event['hex'] as String;
                    return RustWorkbench.readMessage(
                      Uint8List.fromList([
                        for (var offset = 0; offset < hex.length; offset += 2)
                          int.parse(
                            hex.substring(offset, offset + 2),
                            radix: 16,
                          ),
                      ]),
                    ).getRoot(wire.requestFactory);
                  })
                  .where((request) => request.action == wire.Action.editTasks)
                  .map((request) => request.operation)
                  .toList();
          expect(requests, [operation, operation]);
        } finally {
          await host?.close();
          await directory.delete(recursive: true);
        }
      },
      skip: !_available || _python == null,
      timeout: const Timeout(Duration(minutes: 3)),
    );
  }
}
