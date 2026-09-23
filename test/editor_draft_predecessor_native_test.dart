import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/attachments/attachment.dart';
import 'package:morrow_studio/main.dart';
import 'package:morrow_studio/media/texture_source.dart';
import 'package:morrow_studio/plugins/editor_draft.dart';
import 'package:morrow_studio/plugins/editor_recovery.dart';
import 'package:morrow_studio/plugins/editor_session.dart';
import 'package:morrow_studio/plugins/versioned_content.dart';
import 'package:morrow_studio/plugins/versioned_editor.dart';
import 'package:morrow_studio/plugins/versioned_editor_adapter.dart';
import 'package:morrow_studio/plugins/workbench_backend.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';

final _executable = Platform.environment['MORROW_WORKBENCH_HOST'];
final _package = Platform.environment['MORROW_WORKBENCH_PACKAGE'];
final _available =
    Platform.isWindows && _executable != null && _package != null;

EditorDraftTextValue _text(String value) => EditorDraftTextValue(
  text: value,
  selectionBase: value.length,
  selectionExtent: value.length,
  affinity: 0,
  directional: false,
  composingStart: -1,
  composingEnd: -1,
);

void main() {
  test(
    'acknowledged S1 remains an exact predecessor for a durable S2 draft',
    () async {
      final directory = await Directory.systemTemp.createTemp(
        'morrow-editor-predecessor-',
      );
      RustWorkbench? host;
      VersionedEditorSession? session;
      try {
        host = await RustWorkbench.open(
          executable: _executable!,
          package: _package!,
          directory: directory,
        );
        const id = 'predecessor-card';
        final sourceFile = File('${directory.path}/source.bin');
        final addedFile = File('${directory.path}/added.bin');
        await sourceFile.writeAsBytes([1, 2, 3, 4]);
        await addedFile.writeAsBytes([5, 6, 7, 8]);
        IdeaAttachment local(File file) => IdeaAttachment(
          source: TextureSource(
            location: file.path,
            name: file.uri.pathSegments.last,
            kind: TextureKind.file,
            local: true,
          ),
          size: 4,
        );
        await host.apply(
          PluginAction.create,
          Idea(
            'Source title',
            'Source body',
            '进行中',
            Idea.icons[0],
            const Color(0xff8866aa),
            id: id,
            stage: '计划中',
            attachments: [local(sourceFile)],
          ),
        );
        final migrated = (await host.versionedContent.migrate(
          await host.versionedContent.planMigration(id),
        )).current;
        expect(migrated.assets, hasLength(1));
        final sourceAsset = migrated.assets.single.id;
        final baseline = await host.workspaceRecord(migrated);
        session = await host.openVersionedEditor(
          id,
          expectedRevision: migrated.revision,
        );
        final adapter = VersionedWorkbenchEditorAdapter(
          session,
          baseline.versioned!,
          host.workspaceRecord,
        );
        final evidence = adapter as EditorDraftPredecessorSource;
        expect(evidence.draftPredecessor, isNull);

        final confirmation = session as VersionedEditorAcknowledgement;
        Future<void> rejectReceipt(String receiptId, String operation) async {
          await expectLater(
            confirmation.acknowledgePresented(
              VersionedCommitReceipt(
                id: receiptId,
                operation: operation,
                revision: migrated.revision + BigInt.one,
                repeated: false,
              ),
            ),
            throwsFormatException,
          );
          expect(evidence.draftPredecessor, isNull);
        }

        await rejectReceipt('another-card', 'another-operation');
        await rejectReceipt(id, 'another-operation');

        final selected = Idea(
          'S1 title',
          'S1 body',
          baseline.category,
          baseline.icon,
          baseline.color,
          id: id,
          stage: baseline.stage,
          favorite: baseline.favorite,
          hypothesis: baseline.hypothesis,
          conclusion: baseline.conclusion,
          attachments: [...baseline.attachments, local(addedFile)],
          contentRevision: baseline.contentRevision,
          contentOwner: baseline.contentOwner,
          versioned: baseline.versioned,
        );
        final confirmed = await adapter.save(
          selected,
          const EditorFields(
            title: 'S1 title',
            description: 'S1 body',
            hypothesis: '',
            conclusion: '',
            todos: '',
          ),
        );
        expect(confirmed.contentRevision, migrated.revision + BigInt.one);
        final predecessor = evidence.draftPredecessor;
        expect(predecessor, isNotNull);
        expect(predecessor!.id, id);
        expect(predecessor.operation, startsWith('editor-v2-'));
        expect(predecessor.sourceRevision, migrated.revision);
        expect(predecessor.currentRevision, confirmed.contentRevision);
        expect(predecessor.status, EditorRecoveryStatus.committed);
        expect(predecessor.digest, hasLength(32));
        expect(await host.inspectEditorRecoveries(id: id), isEmpty);

        await expectLater(
          confirmation.acknowledgePresented(
            VersionedCommitReceipt(
              id: 'another-card',
              operation: predecessor.operation,
              revision: confirmed.contentRevision!,
              repeated: true,
            ),
          ),
          throwsFormatException,
        );
        await expectLater(
          confirmation.acknowledgePresented(
            VersionedCommitReceipt(
              id: id,
              operation: 'another-operation',
              revision: confirmed.contentRevision!,
              repeated: true,
            ),
          ),
          throwsFormatException,
        );
        expect(identical(evidence.draftPredecessor, predecessor), isTrue);
        final addedAsset = confirmed.versioned!.assets
            .singleWhere((asset) => asset.id != sourceAsset)
            .id;

        // The active recovery slot was acknowledged, but the host can bind
        // the exact historical S1 evidence and both asset generations.
        final request = EditorDraftWriteRequest(
          cardId: id,
          draftId: 's2-predecessor-draft',
          operation: 's2-draft-save-1',
          expectedGeneration: BigInt.zero,
          sourceRevision: predecessor.sourceRevision,
          predecessorOperation: predecessor.operation,
          predecessorDigest: predecessor.digest,
          values: EditorDraftValues(
            title: _text('S2 typed title'),
            description: _text('S2 typed body'),
            hypothesis: _text(''),
            conclusion: _text(''),
            todos: _text(''),
            category: baseline.category,
            stage: baseline.stage,
          ),
          assets: [
            EditorDraftAssetSelection(
              origin: EditorDraftAssetOrigin.source,
              assetId: sourceAsset,
              aliases: const [],
            ),
            EditorDraftAssetSelection(
              origin: EditorDraftAssetOrigin.predecessor,
              assetId: addedAsset,
              aliases: const [],
            ),
          ],
        );
        final stored = await host.editorDrafts.save(request);
        expect(stored.generation, BigInt.one);
        expect(stored.active, isTrue);
        expect(stored.request.predecessorOperation, predecessor.operation);
        expect(stored.request.predecessorDigest, predecessor.digest);
        expect(stored.assets.map((asset) => asset.selection.origin), [
          EditorDraftAssetOrigin.source,
          EditorDraftAssetOrigin.predecessor,
        ]);
        expect(stored.assets.map((asset) => asset.selection.assetId), [
          sourceAsset,
          addedAsset,
        ]);
        expect(
          (await host.editorDrafts.read(
            id,
            request.draftId,
          ))!.request.values.title.text,
          'S2 typed title',
        );
        expect(
          (await host.versionedContent.read(id)).revision,
          migrated.revision + BigInt.one,
        );
        expect(await host.inspectEditorRecoveries(id: id), isEmpty);
        expect(evidence.draftPredecessor!.operation, predecessor.operation);
      } finally {
        if (session != null) await session.close();
        if (host != null) await host.close();
        await directory.delete(recursive: true);
      }
    },
    skip: !_available,
  );
}
