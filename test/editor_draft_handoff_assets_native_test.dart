import 'dart:io';

import 'package:crypto/crypto.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/attachments/attachment.dart';
import 'package:morrow_studio/media/texture_source.dart';
import 'package:morrow_studio/plugins/editor_draft.dart';
import 'package:morrow_studio/plugins/editor_draft_assets.dart';
import 'package:morrow_studio/plugins/editor_draft_binding.dart';
import 'package:morrow_studio/plugins/editor_draft_session.dart';
import 'package:morrow_studio/plugins/editor_recovery.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';

import 'fixtures/editor_draft_handoff_proposal_fixture.dart';

final _host = Platform.environment['MORROW_WORKBENCH_HOST'];
final _package = Platform.environment['MORROW_WORKBENCH_PACKAGE'];

void main() {
  test(
    'catalog hands off verified pins, resumes after retirement, and rejects released selections',
    () async {
      final directory = await Directory.systemTemp.createTemp(
        'morrow-handoff-assets-',
      );
      var host = await RustWorkbench.open(
        executable: _host!,
        package: _package!,
        directory: directory,
      );
      EditorDraftSession? session;
      EditorDraftBinding? binding;
      final controllers = List.generate(5, (_) => TextEditingController());
      try {
        late EditorRecovery committed;
        final proposal = await seedHandoffProposal(
          host,
          directory,
          onCommitted: (value) => committed = value,
        );
        final original = proposal.handoff.request;
        final link = proposal.handoff.parentLink;
        final parent = (await host.editorDrafts.read(
          original.cardId,
          link.parentDraftId,
        ))!;
        final source = await host.versionedContent.read(original.cardId);
        expect(source.assets, isEmpty);
        expect(await File('${directory.path}/source.bin').exists(), isFalse);

        // Use only exported host pins with measured digests; the selected
        // original path is already gone and is never imported again.
        final output = '${directory.path}/parent-preview.bin';
        final pin = parent.assets.single;
        await host.editorDrafts.exportAsset(
          original.cardId,
          link.parentDraftId,
          parent.generation,
          pin.selection.assetId,
          output,
        );
        final bytes = await File(output).readAsBytes();
        expect(bytes, proposalBytes);
        var attachment = IdeaAttachment.versioned(
          source: TextureSource(
            location: output,
            name: pin.name,
            kind: TextureKind.file,
            local: true,
          ),
          byteLength: pin.bytes,
          pluginId: pin.selection.assetId,
        );
        var view = EditorDraftAssetView(
          attachment: attachment,
          aliases: pin.selection.aliases,
          sha256: sha256.convert(bytes).bytes,
          mediaType: pin.mediaType,
        );
        var catalog = EditorDraftAssetCatalog.handoff(
          parent: parent,
          parentLink: link,
          childDraftId: original.draftId,
          source: source,
          committed: EditorDraftCommitEvidence.fromRecovery(committed),
          parentViews: [view],
        );
        final selections = catalog.selectionsFor([attachment]);
        expect(selections.single.origin, EditorDraftAssetOrigin.parentDraft);
        final request = EditorDraftWriteRequest(
          cardId: original.cardId,
          draftId: original.draftId,
          operation: original.operation,
          expectedGeneration: BigInt.zero,
          sourceRevision: source.revision,
          predecessorOperation: '',
          predecessorDigest: const [],
          values: parent.request.values,
          assets: selections,
        );
        await host.editorDraftHandoffProposals.prepare(
          EditorDraftHandoffProposal(
            handoff: EditorDraftHandoffRequest(
              request: request,
              parentLink: link,
            ),
            retirementOperation: proposal.retirementOperation,
          ),
        );
        await host.editorDraftHandoffProposals.complete(
          cardId: request.cardId,
          parentDraftId: link.parentDraftId,
          childOperation: request.operation,
        );
        final child = (await host.editorDrafts.read(
          request.cardId,
          request.draftId,
        ))!;
        catalog = catalog.afterConfirmedDraft(child, [view]);
        expect(
          catalog.selectionsFor([attachment]).single.origin,
          EditorDraftAssetOrigin.previousDraft,
        );
        expect(child.request.values.description.composingStart, 12);
        expect(child.request.values.description.selectionBase, 17);
        await host.editorDraftHandoffProposals.retire(
          cardId: request.cardId,
          parentDraftId: link.parentDraftId,
          childOperation: request.operation,
        );
        expect(
          (await host.editorDrafts.read(
            request.cardId,
            link.parentDraftId,
          ))!.active,
          isFalse,
        );
        await File(output).delete();
        await host.close();
        host = await RustWorkbench.open(
          executable: _host!,
          package: _package!,
          directory: directory,
        );
        final restored = (await host.editorDrafts.read(
          request.cardId,
          request.draftId,
        ))!;
        final restoredSource = await host.versionedContent.read(request.cardId);
        // Recreate the view from a newly exported pin. A vanished pre-restart
        // cache path must not be presented as a usable attachment preview.
        final oldAttachment = attachment;
        final restoredPath = '${directory.path}/restored-preview.bin';
        await host.editorDrafts.exportAsset(
          request.cardId,
          request.draftId,
          restored.generation,
          pin.selection.assetId,
          restoredPath,
        );
        final restoredBytes = await File(restoredPath).readAsBytes();
        expect(restoredBytes, bytes);
        attachment = IdeaAttachment.versioned(
          source: TextureSource(
            location: restoredPath,
            name: pin.name,
            kind: TextureKind.file,
            local: true,
          ),
          byteLength: pin.bytes,
          pluginId: pin.selection.assetId,
        );
        view = EditorDraftAssetView(
          attachment: attachment,
          aliases: pin.selection.aliases,
          sha256: sha256.convert(restoredBytes).bytes,
          mediaType: pin.mediaType,
        );
        catalog = EditorDraftAssetCatalog.existingCard(
          cardId: request.cardId,
          draftId: request.draftId,
          source: restoredSource,
          sourceViews: const [],
          previousDraft: restored,
          previousViews: [view],
        );
        expect(
          () => catalog.selectionsFor([oldAttachment]),
          throwsFormatException,
        );
        expect(await File(attachment.source.location).readAsBytes(), bytes);
        var operation = 0;
        session = EditorDraftSession.restore(
          control: host.editorDrafts,
          record: restored,
          operationFactory: () => 'continued-${++operation}',
          isCurrent: () => true,
          debounce: null,
        );
        session.adoptConfirmedAssetPins(restored);
        final selected = <IdeaAttachment>[attachment];
        binding = EditorDraftBinding(
          session: session,
          title: controllers[0],
          description: controllers[1],
          hypothesis: controllers[2],
          conclusion: controllers[3],
          todos: controllers[4],
          readMetadata: () => EditorDraftMetadata(
            category: source.category,
            stage: source.stage,
            assets: catalog.selectionsFor(selected),
          ),
          isCurrent: () => true,
        );
        binding.applyCurrentToView((_) {});
        binding.attach();
        expect(binding.capture(), isTrue);
        expect(session.dirty, isFalse);
        expect(
          controllers[1].value.composing,
          const TextRange(start: 12, end: 18),
        );

        const earlier = TextEditingValue(
          text: 'S2 continued 😀',
          selection: TextSelection(
            baseOffset: 0,
            extentOffset: 2,
            isDirectional: true,
          ),
        );
        const later = TextEditingValue(
          text: 'S3 composition 😀 still local',
          selection: TextSelection(
            baseOffset: 3,
            extentOffset: 7,
            affinity: TextAffinity.upstream,
            isDirectional: true,
          ),
          composing: TextRange(start: 3, end: 7),
        );
        controllers[1].value = earlier;
        final flight = session.flush();
        // A late acknowledgement must not replace this newer complete raw value.
        controllers[1].value = later;
        final second = (await flight)!;
        expect(second.request.values.description.text, earlier.text);
        expect(session.current.values.description.text, later.text);
        expect(controllers[1].value, later);
        expect(session.dirty, isTrue);
        catalog = catalog.afterConfirmedDraft(second, [view]);
        session.adoptConfirmedAssetPins(second);
        final third = (await session.flush())!;
        catalog = catalog.afterConfirmedDraft(third, [view]);
        session.adoptConfirmedAssetPins(third);
        expect(third.request.values.description.composingStart, 3);
        expect(third.request.values.description.composingEnd, 7);
        expect(
          third.request.values.description.affinity,
          TextAffinity.upstream.index,
        );
        expect(third.request.values.description.directional, isTrue);
        expect(third.request.values.title.text, proposalRawTitle());
        expect(third.request.assets.single.aliases, pin.selection.aliases);
        await host.editorDrafts.exportAsset(
          request.cardId,
          request.draftId,
          third.generation,
          pin.selection.assetId,
          '${directory.path}/child-preview.bin',
        );
        expect(
          await File('${directory.path}/child-preview.bin').readAsBytes(),
          bytes,
        );
        expect(
          (await host.versionedContent.read(request.cardId)).revision,
          source.revision,
        );

        selected.clear();
        expect(binding.capture(), isTrue);
        final fourth = (await session.flush())!;
        catalog = catalog.afterConfirmedDraft(fourth, const []);
        session.adoptConfirmedAssetPins(fourth);
        expect(
          () => catalog.selectionsFor([attachment]),
          throwsFormatException,
        );
        selected.add(attachment);
        expect(binding.capture(), isFalse);
        expect(binding.hasUncapturedChanges, isTrue);
        expect(controllers[1].value, later);
        expect(session.current.assets, isEmpty);
        expect(
          (await host.editorDrafts.read(
            request.cardId,
            request.draftId,
          ))!.generation,
          fourth.generation,
        );
        await expectLater(
          host.editorDrafts.exportAsset(
            request.cardId,
            request.draftId,
            fourth.generation,
            pin.selection.assetId,
            '${directory.path}/released.bin',
          ),
          throwsA(anything),
        );
        expect(await File('${directory.path}/released.bin').exists(), isFalse);
      } finally {
        binding?.dispose();
        session?.dispose();
        for (final controller in controllers) {
          controller.dispose();
        }
        await host.close();
        // Preserve the task-owned library for failure diagnosis; never touch user data.
      }
    },
    skip: !Platform.isWindows || _host == null || _package == null,
    timeout: const Timeout(Duration(minutes: 4)),
  );
}
