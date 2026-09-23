import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/main.dart';
import 'package:morrow_studio/plugins/editor_draft.dart';
import 'package:morrow_studio/plugins/editor_draft_session.dart';
import 'package:morrow_studio/plugins/workbench_backend.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';

final _executable = Platform.environment['MORROW_WORKBENCH_HOST'];
final _package = Platform.environment['MORROW_WORKBENCH_PACKAGE'];
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
EditorDraftValues _values(String description) => EditorDraftValues(
  title: _text(''),
  description: _text(description),
  hypothesis: _text(''),
  conclusion: _text(''),
  todos: _text(''),
  category: '进行中',
  stage: '计划中',
);
EditorDraftWriteRequest _request(
  String id, {
  String draft = 'new-draft',
  String operation = 'draft-one',
  BigInt? generation,
  String description = '',
  List<EditorDraftAssetSelection> assets = const [],
}) => EditorDraftWriteRequest(
  sourceKind: EditorDraftSourceKind.newCard,
  cardId: id,
  draftId: draft,
  operation: operation,
  expectedGeneration: generation ?? BigInt.zero,
  sourceRevision: BigInt.zero,
  predecessorOperation: '',
  predecessorDigest: const [],
  values: _values(description),
  assets: assets,
);
Future<void> _seed(RustWorkbench host, String id) => host.apply(
  PluginAction.create,
  Idea(
    'Already formal',
    'Other writer content',
    '进行中',
    Idea.icons[0],
    const Color(0xff8866aa),
    id: id,
    stage: '计划中',
  ),
);

void main() {
  test(
    'new-card session and attachment survive restart without creating formal content',
    () async {
      final directory = await Directory.systemTemp.createTemp(
        'morrow-new-draft-',
      );
      RustWorkbench? host;
      EditorDraftSession? session;
      try {
        host = await RustWorkbench.open(
          executable: _executable!,
          package: _package!,
          directory: directory,
        );
        const id = 'not-a-formal-card';
        final input = File('${directory.path}/original.txt');
        final bytes = List<int>.generate(40000, (i) => i % 239);
        await input.writeAsBytes(bytes);
        // Missing source is not an implicit new-card import grant.
        await expectLater(
          host.editorDrafts.importAsset(
            id,
            'new-draft',
            BigInt.zero,
            input.path,
            'original.txt',
            'file',
            BigInt.from(bytes.length),
          ),
          throwsA(isA<StateError>()),
        );
        var operation = 0;
        session = EditorDraftSession.newSession(
          control: host.editorDrafts,
          cardId: id,
          draftId: 'new-draft',
          sourceKind: EditorDraftSourceKind.newCard,
          sourceRevision: BigInt.zero,
          initialSnapshot: EditorDraftSnapshot(
            values: _values(''),
            assets: const [],
          ),
          operationFactory: () => 'new-session-${++operation}',
          isCurrent: () => true,
          debounce: const Duration(hours: 1),
        );
        session.observe(
          EditorDraftSnapshot(
            values: _values('Unfinished 😀'),
            assets: const [],
          ),
        );
        final first = (await session.flush())!;
        expect(first.request.sourceKind, EditorDraftSourceKind.newCard);
        expect(first.sourceFormat, 0);
        expect(first.sourceRevision, BigInt.zero);
        expect(first.sourceSha256, isEmpty);
        expect(first.request.values.title.text, isEmpty);
        expect(await host.loadWorkspaceContent(), isEmpty);
        final imported = await host.editorDrafts.importAsset(
          id,
          'new-draft',
          BigInt.one,
          input.path,
          'original.txt',
          'file',
          BigInt.from(bytes.length),
        );
        session.observe(
          EditorDraftSnapshot(
            values: _values('Unfinished 😀 with attachment'),
            assets: [
              EditorDraftAssetSelection(
                origin: EditorDraftAssetOrigin.staged,
                assetId: imported.id,
                aliases: ['local:original'],
              ),
            ],
          ),
        );
        final second = (await session.flush())!;
        expect(second.generation, BigInt.two);
        await input.delete();
        session.dispose();
        session = null;
        await host.close();
        host = await RustWorkbench.open(
          executable: _executable!,
          package: _package!,
          directory: directory,
        );
        final restored = (await host.editorDrafts.read(id, 'new-draft'))!;
        expect(restored.request.sourceKind, EditorDraftSourceKind.newCard);
        expect(
          restored.request.values.description.text,
          'Unfinished 😀 with attachment',
        );
        session = EditorDraftSession.restore(
          control: host.editorDrafts,
          record: restored,
          operationFactory: () =>
              throw StateError('Restoration must not allocate an operation'),
          isCurrent: () => true,
        );
        await session.flush();
        expect(
          (await host.editorDrafts.read(id, 'new-draft'))!.generation,
          BigInt.two,
        );
        expect(await host.loadWorkspaceContent(), isEmpty);
        final output = '${directory.path}/restored.txt';
        await host.editorDrafts.exportAsset(
          id,
          'new-draft',
          BigInt.two,
          imported.id,
          output,
        );
        expect(await File(output).readAsBytes(), bytes);
        final old = await host.editorDrafts.save(first.request);
        expect(old.repeated, isTrue);
        expect(old.generation, BigInt.one);
        expect(old.currentGeneration, BigInt.two);
        await host.editorDrafts.discard(
          id,
          'new-draft',
          BigInt.two,
          'discard-new-draft',
        );
        expect(
          (await host.editorDrafts.read(id, 'new-draft'))!.active,
          isFalse,
        );
        expect(await host.loadWorkspaceContent(), isEmpty);
      } finally {
        session?.dispose();
        await host?.close();
        await directory.delete(recursive: true);
      }
    },
    skip: !_available,
    timeout: const Timeout(Duration(minutes: 4)),
  );

  test(
    'new-card ID collision rejects first save but retains earlier draft without overwriting formal content',
    () async {
      final directory = await Directory.systemTemp.createTemp(
        'morrow-new-draft-conflict-',
      );
      RustWorkbench? host;
      try {
        host = await RustWorkbench.open(
          executable: _executable!,
          package: _package!,
          directory: directory,
        );
        const id = 'future-collision';
        final first = _request(id, description: 'Keep my unsaved input');
        await host.editorDrafts.save(first);
        await _seed(host, id);
        final before = (await host.loadWorkspaceContent()).single;
        await expectLater(
          host.editorDrafts.save(
            _request(id, draft: 'another-draft', operation: 'collision'),
          ),
          throwsA(isA<EditorDraftSaveFailure>()),
        );
        final next = await host.editorDrafts.save(
          _request(
            id,
            operation: 'keep-draft',
            generation: BigInt.one,
            description: 'Keep even after another writer created target',
          ),
        );
        expect(next.generation, BigInt.two);
        expect(next.request.sourceKind, EditorDraftSourceKind.newCard);
        final after = (await host.loadWorkspaceContent()).single;
        expect(after.id, before.id);
        expect(after.title, before.title);
        expect(after.description, before.description);
        final old = await host.editorDrafts.save(first);
        expect(old.repeated, isTrue);
        expect(old.currentGeneration, BigInt.two);
        expect(await host.editorDrafts.read(id, 'another-draft'), isNull);
      } finally {
        await host?.close();
        await directory.delete(recursive: true);
      }
    },
    skip: !_available,
    timeout: const Timeout(Duration(minutes: 4)),
  );
}
