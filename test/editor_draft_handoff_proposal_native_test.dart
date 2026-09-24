import 'dart:io';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/editor_draft.dart';
import 'package:morrow_studio/plugins/editor_draft_codec.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';
import 'fixtures/editor_draft_handoff_proposal_fixture.dart';

final executable = Platform.environment['MORROW_WORKBENCH_HOST'];
final package = Platform.environment['MORROW_WORKBENCH_PACKAGE'];

void main() {
  test(
    'persistent proposal supports read-only discovery, explicit completion and cancellation',
    () async {
      final directory = await Directory.systemTemp.createTemp(
        'morrow-handoff-proposal-',
      );
      var host = await RustWorkbench.open(
        executable: executable!,
        package: package!,
        directory: directory,
      );
      try {
        final proposal = await seedHandoffProposal(host, directory);
        final request = proposal.handoff.request;
        final parent = proposal.handoff.parentLink.parentDraftId;
        final control = host.editorDraftHandoffProposals;
        expect(
          await control.inspect(
            cardId: request.cardId,
            parentDraftId: parent,
            childOperation: request.operation,
          ),
          isNull,
        );
        final prepared = await control.prepare(proposal);
        expect(
          prepared.summary.status,
          EditorDraftHandoffProposalStatus.pending,
        );
        expect(
          await host.editorDrafts.read(request.cardId, request.draftId),
          isNull,
        );
        expect(
          EditorDraftCodec.encodeHandoffProposal(prepared.proposal),
          EditorDraftCodec.encodeHandoffProposal(proposal),
        );
        final page = await control.page(limit: 1);
        expect(page.proposals, hasLength(1));
        expect(page.proposals.single.childOperation, request.operation);
        expect(page.nextCursor, isEmpty);
        expect(
          (await control.discover(pageSize: 1)).single.cursor,
          prepared.summary.cursor,
        );
        expect(
          await host.editorDrafts.read(request.cardId, request.draftId),
          isNull,
        );
        final completed = await control.complete(
          cardId: request.cardId,
          parentDraftId: parent,
          childOperation: request.operation,
        );
        expect(
          completed.summary.status,
          EditorDraftHandoffProposalStatus.childCommitted,
        );
        final child = await host.editorDrafts.read(
          request.cardId,
          request.draftId,
        );
        expect(child!.request.values.title.text, proposalRawTitle());
        final destination = '${directory.path}/child-export.bin';
        await host.editorDrafts.exportAsset(
          request.cardId,
          request.draftId,
          child.generation,
          child.assets.single.selection.assetId,
          destination,
        );
        expect(await File(destination).readAsBytes(), proposalBytes);
        expect(await File('${directory.path}/source.bin').exists(), isFalse);
        await control.retire(
          cardId: request.cardId,
          parentDraftId: parent,
          childOperation: request.operation,
        );
        expect(
          (await host.editorDrafts.read(request.cardId, parent))!.active,
          isFalse,
        );
        await host.close();
        host = await RustWorkbench.open(
          executable: executable!,
          package: package!,
          directory: directory,
        );
        final restored = (await host.editorDraftHandoffProposals.discover(
          pageSize: 1,
        )).single;
        expect(restored.status, EditorDraftHandoffProposalStatus.parentRetired);
        final historical = await host.editorDraftHandoffProposals.inspect(
          cardId: restored.cardId,
          parentDraftId: restored.parentDraftId,
          childOperation: restored.childOperation,
        );
        expect(
          EditorDraftCodec.encodeHandoffProposal(historical!.proposal),
          EditorDraftCodec.encodeHandoffProposal(proposal),
        );

        final cancelled = await seedHandoffProposal(host, directory);
        await host.editorDraftHandoffProposals.prepare(cancelled);
        final cr = cancelled.handoff.request;
        final cp = cancelled.handoff.parentLink.parentDraftId;
        final receipt = await host.editorDraftHandoffProposals.cancel(
          cardId: cr.cardId,
          parentDraftId: cp,
          childOperation: cr.operation,
        );
        expect(
          receipt.summary.status,
          EditorDraftHandoffProposalStatus.cancelled,
        );
        expect(receipt.summary.revision, BigInt.two);
        await expectLater(
          host.editorDraftHandoffProposals.complete(
            cardId: cr.cardId,
            parentDraftId: cp,
            childOperation: cr.operation,
          ),
          throwsA(isA<EditorDraftHandoffProposalFailure>()),
        );
        await expectLater(
          host.editorDraftHandoffs.handoff(cancelled.handoff),
          throwsA(isA<EditorDraftHandoffFailure>()),
        );
        expect(await host.editorDrafts.read(cr.cardId, cr.draftId), isNull);
        expect(
          await host.editorDraftHandoffProposals.discover(pageSize: 1),
          hasLength(2),
        );
      } on EditorDraftHandoffProposalFailure catch (error) {
        // Preserve the typed Unknown result while exposing its cause in test logs.
        stderr.writeln("Proposal failure cause: ${error.cause}");
        rethrow;
      } finally {
        await host.close();
        // Keep the task-owned fixture directory available for failed-test diagnosis.
      }
    },
    skip: !Platform.isWindows || executable == null || package == null,
    timeout: const Timeout(Duration(minutes: 4)),
  );
}
