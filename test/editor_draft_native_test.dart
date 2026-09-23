import 'dart:convert';
import 'dart:typed_data';
import 'package:capnproto_dart/capnproto_dart.dart';
import 'package:morrow_studio/plugins/generated/host.capnp.dart' as wire;
import 'package:morrow_studio/plugins/generated/identity.dart' as contract;
import 'package:morrow_studio/plugins/host_request.dart';
import 'package:morrow_studio/plugins/versioned_content_codec.dart';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/main.dart';
import 'package:morrow_studio/plugins/editor_draft.dart';
import 'package:morrow_studio/plugins/workbench_backend.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';

final _executable = Platform.environment['MORROW_WORKBENCH_HOST'];
final _package = Platform.environment['MORROW_WORKBENCH_PACKAGE'];
final _python = Platform.environment['MORROW_CLOSE_TEST_PYTHON'];
final _available =
    Platform.isWindows && _executable != null && _package != null;

EditorDraftTextValue _text(String value) => EditorDraftTextValue(
  text: value,
  selectionBase: value.length,
  selectionExtent: 0,
  affinity: 1,
  directional: true,
  composingStart: -1,
  composingEnd: -1,
);

EditorDraftWriteRequest _request(
  String cardId,
  BigInt source, {
  String draftId = 'draft-session',
  String operation = 'draft-save-1',
  BigInt? generation,
  String title = '',
  String description = '',
  List<EditorDraftAssetSelection> assets = const [],
}) => EditorDraftWriteRequest(
  cardId: cardId,
  draftId: draftId,
  operation: operation,
  expectedGeneration: generation ?? BigInt.zero,
  sourceRevision: source,
  predecessorOperation: '',
  predecessorDigest: const [],
  values: EditorDraftValues(
    title: _text(title),
    description: _text(description),
    hypothesis: _text('unfinished 😀'),
    conclusion: _text(''),
    todos: _text(''),
    category: '进行中',
    stage: '计划中',
  ),
  assets: assets,
);

Future<BigInt> _seed(RustWorkbench host, String id) async {
  await host.apply(
    PluginAction.create,
    Idea(
      'Formal original',
      'Formal body',
      '进行中',
      Idea.icons[0],
      const Color(0xff8866aa),
      id: id,
      stage: '计划中',
      todos: const ['keep TaskId'],
    ),
  );
  final plan = await host.versionedContent.planMigration(id);
  return (await host.versionedContent.migrate(plan)).current.revision;
}

String _hex(List<int> bytes) =>
    bytes.map((v) => v.toRadixString(16).padLeft(2, '0')).join();

Future<void> _armFinishLoss(
  Directory directory,
  EditorDraftWriteRequest draft,
) async {
  Future<Uint8List> frame(String token) async {
    late Uint8List result;
    await sendHostRequest(
      wire.Action.finishEditorDraft,
      configure: (outer) {
        outer.transfer = token;
        outer.operation = draft.operation;
        outer.id = draft.cardId;
        outer.attachment = draft.draftId;
        outer.revision = VersionedContentCodec.wireU64(
          draft.expectedGeneration,
        );
      },
      send: (bytes) async {
        result = Uint8List.fromList(bytes);
      },
    );
    return result;
  }

  final a = await frame('upload-0');
  final b = await frame('upload-9');
  expect(a.length, b.length);
  final mask = [for (var i = 0; i < a.length; i++) a[i] == b[i] ? 255 : 0];
  expect(mask.where((v) => v == 0), hasLength(1));
  await File(
    '${directory.path}/armed.mask.json',
  ).writeAsString(jsonEncode({'template': _hex(a), 'mask': _hex(mask)}));
}

Future<void> _armReadOrDiscardLoss(
  Directory directory,
  EditorDraftWriteRequest draft,
  wire.Action action,
) async {
  Future<Uint8List> frame(String correlation) async {
    late Uint8List result;
    await sendHostRequest(
      action,
      configure: (outer) {
        outer.id = draft.cardId;
        outer.attachment = draft.draftId;
        if (action == wire.Action.discardEditorDraft) {
          outer.operation = 'draft-discard';
          outer.revision = 1;
        }
        outer.transfer = correlation;
      },
      send: (bytes) async {
        result = Uint8List.fromList(bytes);
      },
    );
    return result;
  }

  final a = await frame('draft-request-0');
  final b = await frame('draft-request-9');
  expect(a.length, b.length);
  final mask = [for (var i = 0; i < a.length; i++) a[i] == b[i] ? 255 : 0];
  expect(mask.where((v) => v == 0), hasLength(1));
  await File(
    '${directory.path}/armed.mask.json',
  ).writeAsString(jsonEncode({'template': _hex(a), 'mask': _hex(mask)}));
}

void main() {
  test(
    'native draft transfer preserves large raw draft and historical receipts across restart',
    () async {
      final directory = await Directory.systemTemp.createTemp(
        'morrow-draft-wire-',
      );
      RustWorkbench? host;
      try {
        host = await RustWorkbench.open(
          executable: _executable!,
          package: _package!,
          directory: directory,
        );
        const id = 'draft-wire-card';
        final source = await _seed(host, id);
        expect(await host.editorDrafts.read(id, 'draft-session'), isNull);
        await expectLater(
          host.editorDrafts.save(
            _request(id, source, title: String.fromCharCode(0xd800)),
          ),
          throwsA(
            isA<EditorDraftSaveFailure>().having(
              (error) => error.outcomeUnknown,
              'local rejection before submission',
              isFalse,
            ),
          ),
        );
        expect(await host.editorDrafts.list(), isEmpty);
        final original = _request(id, source, description: '字😀' * 30000);
        final first = await host.editorDrafts.save(original);
        expect(first.generation, BigInt.one);
        expect(first.currentGeneration, BigInt.one);
        expect(first.repeated, isFalse);
        expect(first.request.values.title.text, isEmpty);
        expect(
          first.request.values.description.text,
          original.values.description.text,
        );
        expect(
          first.request.values.description.selectionBase,
          original.values.description.text.length,
        );
        final formal = await host.versionedContent.read(id);
        expect(formal.revision, source);
        expect(formal.title, 'Formal original');
        final next = _request(
          id,
          source,
          operation: 'draft-save-2',
          generation: BigInt.one,
          title: 'Newer raw text',
        );
        expect((await host.editorDrafts.save(next)).generation, BigInt.two);
        final historical = await host.editorDrafts.save(original);
        expect(historical.repeated, isTrue);
        expect(historical.generation, BigInt.one);
        expect(historical.currentGeneration, BigInt.two);
        expect(
          (await host.editorDrafts.read(
            id,
            'draft-session',
          ))!.request.values.title.text,
          'Newer raw text',
        );
        await expectLater(
          host.editorDrafts.save(
            _request(id, source, title: 'Changed original operation'),
          ),
          throwsA(isA<EditorDraftSaveFailure>()),
        );
        // Whole transfer serialization covers parallel callers without stealing
        // another operation's in-progress download buffer.
        final lists = await Future.wait(
          List.generate(3, (_) => host!.editorDrafts.list()),
        );
        expect(
          lists.every((list) => list.single.generation == BigInt.two),
          isTrue,
        );
        await host.close();
        host = await RustWorkbench.open(
          executable: _executable!,
          package: _package!,
          directory: directory,
        );
        final restored = (await host.editorDrafts.read(id, 'draft-session'))!;
        expect(restored.request.operation, next.operation);
        expect(restored.generation, BigInt.two);
        expect(restored.request.values.title.text, 'Newer raw text');
        final discarded = await host.editorDrafts.discard(
          id,
          'draft-session',
          BigInt.two,
          'draft-discard',
        );
        expect(discarded.active, isFalse);
        expect(discarded.generation, BigInt.from(3));
        final oldAfterDiscard = await host.editorDrafts.save(original);
        expect(oldAfterDiscard.currentActive, isFalse);
        expect(oldAfterDiscard.currentGeneration, BigInt.from(3));
        final current = (await host.editorDrafts.read(id, 'draft-session'))!;
        expect(current.active, isFalse);
        expect((await host.versionedContent.read(id)).revision, source);
      } finally {
        await host?.close();
        await directory.delete(recursive: true);
      }
    },
    skip: !_available,
    timeout: const Timeout(Duration(minutes: 4)),
  );

  test(
    'native draft attachment is scoped and survives original file deletion and restart',
    () async {
      final directory = await Directory.systemTemp.createTemp(
        'morrow-draft-asset-',
      );
      RustWorkbench? host;
      try {
        host = await RustWorkbench.open(
          executable: _executable!,
          package: _package!,
          directory: directory,
        );
        const id = 'draft-asset-card';
        final source = await _seed(host, id);
        final file = File('${directory.path}/source.txt');
        final bytes = List<int>.generate(70000, (i) => i % 251);
        await file.writeAsBytes(bytes);
        final imported = await host.editorDrafts.importAsset(
          id,
          'draft-session',
          BigInt.zero,
          file.path,
          'source.txt',
          'file',
          BigInt.from(bytes.length),
        );
        final assets = [
          EditorDraftAssetSelection(
            origin: EditorDraftAssetOrigin.staged,
            assetId: imported.id,
            aliases: ['local:original'],
          ),
        ];
        await expectLater(
          host.editorDrafts.save(
            _request(
              id,
              source,
              draftId: 'other-draft',
              operation: 'other-draft-save',
              assets: assets,
            ),
          ),
          throwsA(isA<EditorDraftSaveFailure>()),
        );
        final request = _request(id, source, assets: assets);
        final saved = await host.editorDrafts.save(request);
        expect(saved.assets.single.bytes, BigInt.from(bytes.length));
        await file.delete();
        await host.close();
        host = await RustWorkbench.open(
          executable: _executable!,
          package: _package!,
          directory: directory,
        );
        final restored = (await host.editorDrafts.read(id, 'draft-session'))!;
        expect(restored.assets.single.sha256, saved.assets.single.sha256);
        final target = '${directory.path}/restored.txt';
        await host.editorDrafts.exportAsset(
          id,
          'draft-session',
          BigInt.one,
          imported.id,
          target,
        );
        expect(await File(target).readAsBytes(), bytes);
        await expectLater(
          host.editorDrafts.exportAsset(
            id,
            'draft-session',
            BigInt.zero,
            imported.id,
            '${directory.path}/stale.txt',
          ),
          throwsA(isA<StateError>()),
        );
        expect(await File('${directory.path}/stale.txt').exists(), isFalse);
      } finally {
        await host?.close();
        await directory.delete(recursive: true);
      }
    },
    skip: !_available,
    timeout: const Timeout(Duration(minutes: 4)),
  );

  for (final mode in ['replace', 'eof']) {
    test(
      'lost draft finish reply ($mode) preserves fixed proposal and reconciles after restart',
      () async {
        final directory = await Directory.systemTemp.createTemp(
          'morrow-draft-reply-',
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
              'mode': mode,
              'trace_requests': true,
            }),
          );
          final replacement = MessageBuilder();
          final error = replacement.initRoot(wire.responseFactory);
          error.version = 1;
          error.digest = Uint8List.fromList(contract.hostDigest);
          error.error = 'injected draft finish reply loss';
          await File(
            '${directory.path}/replacement.bin',
          ).writeAsBytes(replacement.serialize());
          host = await RustWorkbench.open(
            executable: _python!,
            package: 'proxy',
            directory: directory,
          );
          const id = 'draft-lost-reply-card';
          final source = await _seed(host, id);
          final request = _request(
            id,
            source,
            description: 'large raw draft ' * 12000,
          );
          await _armFinishLoss(directory, request);
          await expectLater(
            host.editorDrafts.save(request),
            throwsA(
              isA<EditorDraftSaveFailure>()
                  .having(
                    (error) => error.outcomeUnknown,
                    'unknown effect',
                    isTrue,
                  )
                  .having(
                    (error) => identical(error.request, request),
                    'frozen proposal',
                    isTrue,
                  ),
            ),
          );
          final receipt = RustWorkbench.readMessage(
            await File('${directory.path}/receipt.bin').readAsBytes(),
          ).getRoot(wire.responseFactory);
          expect(receipt.error ?? '', isEmpty);
          expect(receipt.revision, 1);
          // Clear only the failed save's paired response buffer, never the journal.
          if (mode == 'replace') {
            final sameSession = (await host.editorDrafts.read(
              id,
              'draft-session',
            ))!;
            expect(sameSession.generation, BigInt.one);
            expect(sameSession.request.operation, request.operation);
          }
          await host.close();
          host = await RustWorkbench.open(
            executable: _executable!,
            package: _package!,
            directory: Directory('${directory.path}/store'),
            managed: true,
          );
          final restored = (await host.editorDrafts.read(id, 'draft-session'))!;
          expect(restored.request.operation, request.operation);
          expect(
            restored.request.values.description.text,
            request.values.description.text,
          );
          expect(restored.generation, BigInt.one);
          final originalRetry = await host.editorDrafts.save(request);
          expect(originalRetry.repeated, isTrue);
          expect(originalRetry.generation, BigInt.one);
          expect((await host.versionedContent.read(id)).revision, source);
        } finally {
          await host?.close();
          await directory.delete(recursive: true);
        }
      },
      skip: !_available || _python == null,
      timeout: const Timeout(Duration(minutes: 4)),
    );
  }
  for (final action in [
    wire.Action.readEditorDraft,
    wire.Action.discardEditorDraft,
  ]) {
    test(
      'lost ${action.name} first reply can be reconciled in the same native session',
      () async {
        final directory = await Directory.systemTemp.createTemp(
          'morrow-draft-query-reply-',
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
            }),
          );
          final replacement = MessageBuilder();
          final error = replacement.initRoot(wire.responseFactory);
          error.version = 1;
          error.digest = Uint8List.fromList(contract.hostDigest);
          error.error = 'injected draft response loss';
          await File(
            '${directory.path}/replacement.bin',
          ).writeAsBytes(replacement.serialize());
          host = await RustWorkbench.open(
            executable: _python!,
            package: 'proxy',
            directory: directory,
          );
          const id = 'draft-query-loss-card';
          final source = await _seed(host, id);
          final request = _request(
            id,
            source,
            description: 'large draft text ' * 12000,
          );
          await host.editorDrafts.save(request);
          await _armReadOrDiscardLoss(directory, request, action);
          final Future<Object?> failed = action == wire.Action.readEditorDraft
              ? host.editorDrafts.read(id, request.draftId)
              : host.editorDrafts.discard(
                  id,
                  request.draftId,
                  BigInt.one,
                  'draft-discard',
                );
          await expectLater(failed, throwsStateError);
          final receipt = RustWorkbench.readMessage(
            await File('${directory.path}/receipt.bin').readAsBytes(),
          ).getRoot(wire.responseFactory);
          expect(receipt.error ?? '', isEmpty);
          final restored = (await host.editorDrafts.read(id, request.draftId))!;
          expect(restored.currentActive, action == wire.Action.readEditorDraft);
          expect(
            restored.generation,
            action == wire.Action.readEditorDraft ? BigInt.one : BigInt.two,
          );
          if (action == wire.Action.discardEditorDraft) {
            expect(
              (await host.editorDrafts.discard(
                id,
                request.draftId,
                BigInt.one,
                'draft-discard',
              )).repeated,
              isTrue,
            );
          }
          expect((await host.versionedContent.read(id)).revision, source);
        } finally {
          await host?.close();
          await directory.delete(recursive: true);
        }
      },
      skip: !_available || _python == null,
      timeout: const Timeout(Duration(minutes: 4)),
    );
  }
}
