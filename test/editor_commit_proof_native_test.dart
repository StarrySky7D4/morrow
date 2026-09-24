import 'dart:convert';
import 'dart:typed_data';
import 'dart:io';
import 'package:crypto/crypto.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/main.dart';
import 'package:morrow_studio/attachments/attachment.dart';
import 'package:morrow_studio/media/texture_source.dart';
import 'package:morrow_studio/plugins/editor_commit_proof.dart';
import 'package:morrow_studio/plugins/editor_draft.dart';
import 'package:morrow_studio/plugins/editor_draft_assets.dart';
import 'package:morrow_studio/plugins/editor_recovery.dart';
import 'package:morrow_studio/plugins/editor_session.dart';
import 'package:morrow_studio/plugins/versioned_content.dart';
import 'package:morrow_studio/plugins/versioned_editor_adapter.dart';
import 'package:morrow_studio/plugins/versioned_editor.dart';
import 'package:morrow_studio/plugins/workbench_backend.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';
import 'package:morrow_studio/plugins/generated/host.capnp.dart' as wire;
import 'package:morrow_studio/plugins/host_request.dart';

final executable = Platform.environment['MORROW_WORKBENCH_HOST'];
final package = Platform.environment['MORROW_WORKBENCH_PACKAGE'];
final available = Platform.isWindows && executable != null && package != null;
EditorDraftTextValue text(String value) => EditorDraftTextValue(
  text: value,
  selectionBase: 0,
  selectionExtent: value.length,
  affinity: 1,
  directional: true,
  composingStart: value.isEmpty ? -1 : 0,
  composingEnd: value.isEmpty ? -1 : value.length,
);
final values = EditorDraftValues(
  title: text('Uncommitted S2'),
  description: text('raw S2 😀 attachment:new.bin'),
  hypothesis: text(''),
  conclusion: text(''),
  todos: text(''),
  category: '进行中',
  stage: '计划中',
);
EditorDraftWriteRequest write(
  String id,
  String draftId,
  String operation,
  BigInt generation,
  BigInt revision,
  List<EditorDraftAssetSelection> assets, {
  bool newCard = false,
}) => EditorDraftWriteRequest(
  cardId: id,
  draftId: draftId,
  operation: operation,
  expectedGeneration: generation,
  sourceRevision: revision,
  sourceKind: newCard
      ? EditorDraftSourceKind.newCard
      : EditorDraftSourceKind.existingCard,
  predecessorOperation: '',
  predecessorDigest: const [],
  values: values,
  assets: assets,
);
Idea idea(String id) => Idea(
  'Committed S1',
  'Business body',
  '进行中',
  Idea.icons[0],
  const Color(0xff8866aa),
  id: id,
  stage: '计划中',
);
const fields = EditorFields(
  title: 'Committed S1',
  description: 'Business body',
  hypothesis: '',
  conclusion: '',
  todos: '',
);

void main() {
  test(
    'historical acknowledgement remains valid after the live card advances',
    () async {
      final directory = await Directory.systemTemp.createTemp(
        'morrow-history-ack-',
      );
      final host = await RustWorkbench.open(
        executable: executable!,
        package: package!,
        directory: directory,
      );
      VersionedEditorSession? session;
      try {
        for (final observedFirst in [false, true]) {
          final id = 'advanced-$observedFirst';
          await host.apply(PluginAction.create, idea(id));
          final source = (await host.versionedContent.migrate(
            await host.versionedContent.planMigration(id),
          )).current;
          session = await host.openVersionedEditor(
            id,
            expectedRevision: source.revision,
          );
          final result = await session.save(
            CardEditFields(
              title: fields.title,
              description: fields.description,
              hypothesis: '',
              conclusion: '',
              icon: source.icon,
              color: source.color,
              assets: source.assets,
            ),
            fields,
          );
          final observer = session as VersionedEditorCommitObservation;
          if (observedFirst) {
            await observer.observePresented(result.receipt);
          }
          await host.versionedContent.editCard(
            'advance-$observedFirst',
            id,
            result.receipt.revision,
            const CardEditCommand.setFavorite(true),
          );
          await expectLater(
            observer.observePresented(result.receipt),
            throwsStateError,
          );
          await (session as VersionedEditorAcknowledgement)
              .acknowledgePresented(result.receipt);
          expect(await host.inspectEditorRecoveries(id: id), isEmpty);
          expect(
            (await host.versionedContent.read(id)).revision,
            result.receipt.revision + BigInt.one,
          );
          final proof = await host.inspectEditorCommit(
            id: id,
            operation: result.receipt.operation,
          );
          expect(proof.committedRevision, result.receipt.revision);
          await session.close();
          session = null;
        }
      } finally {
        await session?.close();
        await host.close();
      }
    },
    skip: !available,
    timeout: const Timeout(Duration(minutes: 4)),
  );

  test(
    'lost inspection reply permits only explicit read-only retry, never Create replay',
    () async {
      final python = Platform.environment['MORROW_CLOSE_TEST_PYTHON']!;
      final directory = await Directory.systemTemp.createTemp(
        'morrow-proof-reply-',
      );
      await File(
        'test/fixtures/service_reply_proxy.py',
      ).copy('${directory.path}/workbench.db');
      await File('${directory.path}/proxy.json').writeAsString(
        jsonEncode({
          'host': executable,
          'package': package,
          'store': '${directory.path}/store',
          'mode': 'malformed',
          'trace_requests': true,
        }),
      );
      final host = await RustWorkbench.open(
        executable: python,
        package: 'proxy',
        directory: directory,
      );
      WorkbenchEditorSession? editor;
      try {
        editor = await host.openEditor('lost-proof-card', create: true);
        await editor.save(idea('lost-proof-card'), fields);
        final proof = await (editor as WorkbenchEditorCommitSource)
            .inspectCommittedSource();
        await editor.close();
        late Uint8List request;
        await sendHostRequest(
          wire.Action.inspectEditorCommit,
          configure: (r) {
            r.id = proof.id;
            r.operation = proof.operation;
          },
          send: (bytes) async => request = Uint8List.fromList(bytes),
        );
        await File(
          '${directory.path}/armed.sha256',
        ).writeAsString(sha256.convert(request).toString());
        final trace = File('${directory.path}/trace.jsonl');
        final before = (await trace.readAsLines()).length;
        await expectLater(
          host.inspectEditorCommit(id: proof.id, operation: proof.operation),
          throwsA(anything),
        );
        final recovered = await host.inspectEditorCommit(
          id: proof.id,
          operation: proof.operation,
        );
        expect(recovered.digest, proof.digest);
        final actions = (await trace.readAsLines())
            .skip(before)
            .map(jsonDecode)
            .where((row) => row['event'] == 'request')
            .map((row) {
              final hex = row['hex'] as String;
              final bytes = Uint8List.fromList([
                for (var i = 0; i < hex.length; i += 2)
                  int.parse(hex.substring(i, i + 2), radix: 16),
              ]);
              return RustWorkbench.readMessage(
                bytes,
              ).getRoot(wire.requestFactory).action;
            })
            .toList();
        expect(actions, [
          wire.Action.inspectEditorCommit,
          wire.Action.inspectEditorCommit,
        ]);
        expect(await File('${directory.path}/receipt.bin').exists(), isTrue);
        expect(
          (await host.versionedContent.read(proof.id)).revision,
          BigInt.one,
        );
      } finally {
        await editor?.close();
        await host.close();
      }
    },
    skip:
        !available || Platform.environment['MORROW_CLOSE_TEST_PYTHON'] == null,
    timeout: const Timeout(Duration(minutes: 4)),
  );

  test(
    'first captured Create provides real proof for durable child handoff across restart',
    () async {
      final directory = await Directory.systemTemp.createTemp(
        'morrow-create-proof-',
      );
      var host = await RustWorkbench.open(
        executable: executable!,
        package: package!,
        directory: directory,
      );
      WorkbenchEditorSession? editor;
      try {
        const id = 'new-card', parentId = 'parent', childId = 'child';
        final empty = await host.editorDrafts.save(
          write(
            id,
            parentId,
            'empty',
            BigInt.zero,
            BigInt.zero,
            const [],
            newCard: true,
          ),
        );
        final file = File('${directory.path}/new.bin');
        const bytes = [7, 0, 42, 255];
        await file.writeAsBytes(bytes);
        final asset = await host.editorDrafts.importAsset(
          id,
          parentId,
          empty.generation,
          file.path,
          'new.bin',
          'file',
          BigInt.from(bytes.length),
        );
        final parent = await host.editorDrafts.save(
          write(id, parentId, 'with-pin', empty.generation, BigInt.zero, [
            EditorDraftAssetSelection(
              origin: EditorDraftAssetOrigin.staged,
              assetId: asset.id,
              aliases: const ['new.bin'],
            ),
          ], newCard: true),
        );
        await file.delete();
        editor = await host.openEditor(id, create: true);
        final original = editor as WorkbenchEditorCommitSource;
        await expectLater(original.inspectCommittedSource(), throwsStateError);
        final saved = await editor.save(idea(id), fields);
        expect(saved.contentRevision, BigInt.one);
        await editor.close();
        final proof = await original.inspectCommittedSource();
        expect(proof.sourceRevision, BigInt.zero);
        expect(proof.committedRevision, BigInt.one);
        expect(proof.operation, startsWith('editor-'));
        expect(proof.digest, hasLength(32));
        expect(await original.inspectCommittedSource(), same(proof));
        expect(await host.inspectEditorRecoveries(id: id), isEmpty);
        final source = await host.versionedContent.read(id);
        final link = EditorDraftParentLink.fromParentRecord(
          parent: parent,
          committedOperation: proof.operation,
          committedSha256: proof.digest,
          childOperation: 'child-save',
        );
        final output = '${directory.path}/preview.bin';
        await host.editorDrafts.exportAsset(
          id,
          parentId,
          parent.generation,
          asset.id,
          output,
        );
        final exported = await File(output).readAsBytes();
        expect(exported, bytes);
        final attachment = IdeaAttachment.versioned(
          source: TextureSource(
            location: output,
            name: 'new.bin',
            kind: TextureKind.file,
            local: true,
          ),
          byteLength: asset.bytes,
          pluginId: asset.id,
        );
        final view = EditorDraftAssetView(
          attachment: attachment,
          aliases: const ['new.bin'],
          sha256: sha256.convert(exported).bytes,
          mediaType: parent.assets.single.mediaType,
        );
        final catalog = EditorDraftAssetCatalog.handoff(
          parent: parent,
          parentLink: link,
          childDraftId: childId,
          source: source,
          committed: proof,
          parentViews: [view],
        );
        final request = write(
          id,
          childId,
          'child-save',
          BigInt.zero,
          BigInt.one,
          catalog.selectionsFor([attachment]),
        );
        await host.editorDraftHandoffProposals.prepare(
          EditorDraftHandoffProposal(
            handoff: EditorDraftHandoffRequest(
              request: request,
              parentLink: link,
            ),
            retirementOperation: 'retire-parent',
          ),
        );
        await host.editorDraftHandoffProposals.complete(
          cardId: id,
          parentDraftId: parentId,
          childOperation: request.operation,
        );
        final child = (await host.editorDrafts.read(id, childId))!;
        expect(
          catalog
              .afterConfirmedDraft(child, [view])
              .selectionsFor([attachment])
              .single
              .origin,
          EditorDraftAssetOrigin.previousDraft,
        );
        await host.editorDraftHandoffProposals.retire(
          cardId: id,
          parentDraftId: parentId,
          childOperation: request.operation,
        );
        expect((await host.versionedContent.read(id)).revision, BigInt.one);
        // Advance the live card; proof must still identify the historical Create.
        await host.apply(PluginAction.favorite, saved, flag: true);
        expect((await host.versionedContent.read(id)).revision, BigInt.two);
        await host.close();
        host = await RustWorkbench.open(
          executable: executable!,
          package: '${directory.path}/unavailable.morrowplugin',
          directory: directory,
        );
        expect(host.writable, isFalse);
        final restored = await host.inspectEditorCommit(
          id: id,
          operation: proof.operation,
        );
        expect(restored.digest, proof.digest);
        expect(restored.sourceRevision, BigInt.zero);
        expect(restored.committedRevision, BigInt.one);
        await expectLater(
          host.inspectEditorCommit(id: id, operation: 'missing'),
          throwsStateError,
        );
        await expectLater(
          host.inspectEditorCommit(
            id: 'another-card',
            operation: proof.operation,
          ),
          throwsStateError,
        );
        final durable = (await host.editorDrafts.read(id, childId))!;
        expect(
          durable.request.values.description.text,
          values.description.text,
        );
        expect(
          durable.request.values.description.composingEnd,
          values.description.composingEnd,
        );
        await host.editorDrafts.exportAsset(
          id,
          childId,
          durable.generation,
          asset.id,
          '${directory.path}/after-restart.bin',
        );
        expect(
          await File('${directory.path}/after-restart.bin').readAsBytes(),
          bytes,
        );
        expect((await host.versionedContent.read(id)).revision, BigInt.two);
      } finally {
        await editor?.close();
        await host.close();
      }
    },
    skip: !available,
    timeout: const Timeout(Duration(minutes: 4)),
  );

  test(
    'V2 deferred observation preserves recovery until exact explicit acknowledgement',
    () async {
      final directory = await Directory.systemTemp.createTemp(
        'morrow-deferred-proof-',
      );
      final host = await RustWorkbench.open(
        executable: executable!,
        package: package!,
        directory: directory,
      );
      VersionedWorkbenchEditorAdapter? adapter;
      WorkbenchEditorSession? successor;
      var current = true;
      try {
        const id = 'v2-proof';
        await host.apply(PluginAction.create, idea(id));
        final source = (await host.versionedContent.migrate(
          await host.versionedContent.planMigration(id),
        )).current;
        final baseline = await host.workspaceRecord(source);
        final parent = await host.editorDrafts.save(
          write(id, 'parent', 'raw-s2', BigInt.zero, source.revision, const []),
        );
        final native = await host.openVersionedEditor(
          id,
          expectedRevision: source.revision,
        );
        adapter = VersionedWorkbenchEditorAdapter(
          native,
          baseline.versioned!,
          host.workspaceRecord,
          isCurrent: () => current,
          deferRecoveryAcknowledgement: true,
          reopen: (confirmed) async {
            final next = await host.openVersionedEditor(
              id,
              expectedRevision: confirmed.contentRevision,
            );
            return VersionedWorkbenchEditorAdapter(
              next,
              confirmed.versioned!,
              host.workspaceRecord,
              isCurrent: () => current,
              deferRecoveryAcknowledgement: true,
            );
          },
        );
        final proposal = Idea(
          fields.title,
          fields.description,
          baseline.category,
          baseline.icon,
          baseline.color,
          id: id,
          stage: baseline.stage,
          favorite: baseline.favorite,
          attachments: baseline.attachments,
          contentRevision: baseline.contentRevision,
          contentOwner: baseline.contentOwner,
          versioned: baseline.versioned,
        );
        final saved = await adapter.save(proposal, fields);
        final recovery = (await host.inspectEditorRecoveries(id: id)).single;
        expect(recovery.status, EditorRecoveryStatus.committed);
        final proof = await host.inspectEditorCommit(
          id: id,
          operation: recovery.operation,
        );
        expect(proof.digest, recovery.digest);
        successor = await adapter.continueAfterCommit(saved);
        expect(await host.inspectEditorRecoveries(id: id), hasLength(1));
        final request = write(
          id,
          'child',
          'handoff',
          BigInt.zero,
          saved.contentRevision!,
          const [],
        );
        final link = EditorDraftParentLink.fromParentRecord(
          parent: parent,
          committedOperation: proof.operation,
          committedSha256: proof.digest,
          childOperation: request.operation,
        );
        await host.editorDraftHandoffProposals.prepare(
          EditorDraftHandoffProposal(
            handoff: EditorDraftHandoffRequest(
              request: request,
              parentLink: link,
            ),
            retirementOperation: 'retire',
          ),
        );
        expect(await host.inspectEditorRecoveries(id: id), hasLength(1));
        final receipt = VersionedCommitReceipt(
          id: id,
          operation: proof.operation,
          revision: proof.committedRevision,
          repeated: false,
        );
        final confirmation = adapter as VersionedEditorDeferredAcknowledgement;
        await confirmation.acknowledgeAccepted(receipt);
        await confirmation.acknowledgeAccepted(receipt);
        expect(await host.inspectEditorRecoveries(id: id), isEmpty);
        current = false;
        await expectLater(
          confirmation.acknowledgeAccepted(receipt),
          throwsStateError,
        );
        current = true;
        await host.editorDraftHandoffProposals.complete(
          cardId: id,
          parentDraftId: 'parent',
          childOperation: 'handoff',
        );
        await host.editorDraftHandoffProposals.retire(
          cardId: id,
          parentDraftId: 'parent',
          childOperation: 'handoff',
        );
        expect(
          (await host.editorDrafts.read(
            id,
            'child',
          ))!.request.values.description.text,
          values.description.text,
        );
        expect(
          (await host.versionedContent.read(id)).revision,
          source.revision + BigInt.one,
        );
      } finally {
        await successor?.close();
        await adapter?.close();
        await host.close();
      }
    },
    skip: !available,
    timeout: const Timeout(Duration(minutes: 4)),
  );
}
