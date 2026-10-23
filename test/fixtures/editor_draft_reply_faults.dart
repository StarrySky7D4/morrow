// Deliberately withhold only a returned acknowledgement after real host work.
// Never synthesize a commit, native receipt, capture or retirement capability.
import 'package:morrow_studio/attachments/attachment.dart';
import 'package:file_selector/file_selector.dart';
import 'package:morrow_studio/main.dart' show Idea;
import 'package:morrow_studio/plugins/editor_draft.dart';
import 'package:morrow_studio/plugins/editor_draft_import.dart';
import 'package:morrow_studio/plugins/editor_draft_owner.dart';
import 'package:morrow_studio/plugins/editor_draft_session.dart';
import 'package:morrow_studio/plugins/editor_draft_submission.dart';
import 'package:morrow_studio/plugins/editor_draft_workspace.dart';
import 'package:morrow_studio/plugins/editor_commit_proof.dart';
import 'package:morrow_studio/plugins/editor_session.dart';
import 'package:morrow_studio/plugins/studio_backend.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';

final class LostBusinessReply
    implements
        WorkbenchEditorSession,
        WorkbenchEditorOperationIdentity,
        WorkbenchEditorDraftCaptureIdentity,
        WorkbenchEditorCommitSource,
        WorkbenchEditorDraftAssetBinding {
  LostBusinessReply(
    this.original, {
    this.loseReply = true,
    this.afterActualSave,
  });
  final bool loseReply;
  final void Function()? afterActualSave;
  final WorkbenchEditorSession original;
  int saves = 0;
  @override
  String get targetId => original.targetId;
  @override
  StudioBackend get studio => original.studio;
  @override
  String get commitOperation =>
      (original as WorkbenchEditorOperationIdentity).commitOperation;
  @override
  BigInt get sourceRevision =>
      (original as WorkbenchEditorOperationIdentity).sourceRevision;
  @override
  ({String scope, int events, bool complete}) get draftCapture =>
      (original as WorkbenchEditorDraftCaptureIdentity).draftCapture;
  @override
  Future<void> recordPaste(PasteInsertion insertion) =>
      original.recordPaste(insertion);
  @override
  Future<void> close() => original.close();
  @override
  Future<EditorDraftCommitEvidence> inspectCommittedSource() =>
      (original as WorkbenchEditorCommitSource).inspectCommittedSource();
  @override
  Future<void> bindDraftAssets(
    EditorDraftRecord record,
    List<IdeaAttachment> selected,
  ) => (original as WorkbenchEditorDraftAssetBinding).bindDraftAssets(
    record,
    selected,
  );
  @override
  Future<Idea> save(Idea draft, EditorFields fields) async {
    saves++;
    final result = await original.save(draft, fields);
    afterActualSave?.call();
    if (loseReply) {
      throw StateError('test withheld the original business receipt');
    }
    return result;
  }
}

final class LostProposalReply implements EditorDraftHandoffProposalControl {
  LostProposalReply(this.original, this.lostStage);
  final EditorDraftHandoffProposalControl original;
  final String lostStage;
  final calls = <String, int>{};
  void Function()? afterComplete;
  Future<void> Function()? beforeRetire;
  Future<EditorDraftHandoffProposalRecord> _dispatch(
    String stage,
    Future<EditorDraftHandoffProposalRecord> Function() work,
  ) async {
    calls[stage] = (calls[stage] ?? 0) + 1;
    if (stage == 'retire') await beforeRetire?.call();
    final result = await work();
    if (stage == 'complete') afterComplete?.call();
    if (stage == lostStage) {
      throw StateError('test withheld original $stage receipt');
    }
    return result;
  }

  @override
  Future<EditorDraftHandoffProposalRecord> prepare(
    EditorDraftHandoffProposal proposal,
  ) => _dispatch('prepare', () => original.prepare(proposal));
  @override
  Future<EditorDraftHandoffProposalRecord> complete({
    required String cardId,
    required String parentDraftId,
    required String childOperation,
  }) => _dispatch(
    'complete',
    () => original.complete(
      cardId: cardId,
      parentDraftId: parentDraftId,
      childOperation: childOperation,
    ),
  );
  @override
  Future<EditorDraftHandoffProposalRecord> retire({
    required String cardId,
    required String parentDraftId,
    required String childOperation,
  }) => _dispatch(
    'retire',
    () => original.retire(
      cardId: cardId,
      parentDraftId: parentDraftId,
      childOperation: childOperation,
    ),
  );
  @override
  Future<EditorDraftHandoffProposalRecord?> inspect({
    required String cardId,
    required String parentDraftId,
    required String childOperation,
  }) => original.inspect(
    cardId: cardId,
    parentDraftId: parentDraftId,
    childOperation: childOperation,
  );
  @override
  Future<EditorDraftHandoffProposalRecord> cancel({
    required String cardId,
    required String parentDraftId,
    required String childOperation,
  }) => original.cancel(
    cardId: cardId,
    parentDraftId: parentDraftId,
    childOperation: childOperation,
  );
  @override
  Future<EditorDraftHandoffProposalPage> page({
    String cursor = '',
    int limit = 32,
  }) => original.page(cursor: cursor, limit: limit);
  @override
  Future<List<EditorDraftHandoffProposalSummary>> discover({
    int pageSize = 32,
  }) => original.discover(pageSize: pageSize);
}

final class LostChildReply implements EditorDraftControl {
  LostChildReply(this.original);
  final EditorDraftControl original;
  final operations = <String>[];
  @override
  Future<EditorDraftRecord> save(EditorDraftWriteRequest request) async {
    operations.add(request.operation);
    final record = await original.save(request);
    if (operations.length == 1) {
      throw StateError('test withheld original child update receipt');
    }
    return record;
  }

  @override
  Future<EditorDraftRecord?> read(String cardId, String draftId) =>
      original.read(cardId, draftId);
  @override
  Future<List<EditorDraftSummary>> list() => original.list();
  @override
  Future<EditorDraftRecord> discard(String c, String d, BigInt g, String o) =>
      original.discard(c, d, g, o);
  @override
  Future<EditorDraftImportedAsset> importAsset(
    String c,
    String d,
    BigInt g,
    String p,
    String n,
    String k,
    BigInt b,
  ) => original.importAsset(c, d, g, p, n, k, b);
  @override
  Future<void> exportAsset(String c, String d, BigInt g, String a, String p) =>
      original.exportAsset(c, d, g, a, p);
}

/// The formal widget still calls its production commit owner and real host.
/// Only the selected return boundary is faulted; all other services delegate.
final class LostImportCleanupReply implements EditorDraftImportControl {
  LostImportCleanupReply(this.original);
  final EditorDraftImportControl original;
  int reconciles = 0, inspections = 0;
  @override
  Future<EditorDraftImportSnapshot> reconcile(String c, String d) async {
    reconciles++;
    await original.reconcile(c, d);
    throw StateError('test withheld original import cleanup receipt');
  }

  @override
  Future<EditorDraftImportSnapshot> list(String c, String d) {
    inspections++;
    return original.list(c, d);
  }

  @override
  Future<EditorDraftImportSnapshot> begin(EditorDraftImportRequest r) =>
      original.begin(r);
  @override
  Future<EditorDraftImportSnapshot> complete(
    EditorDraftImportRequest r, {
    String selectedPath = '',
  }) => original.complete(r, selectedPath: selectedPath);
  @override
  Future<EditorDraftImportSnapshot> inspect(String c, String d, String op) =>
      original.inspect(c, d, op);
  @override
  Future<EditorDraftImportSnapshot> export(
    String c,
    String d,
    BigInt g,
    String op,
    String path,
  ) => original.export(c, d, g, op, path);
  @override
  Future<EditorDraftImportSnapshot> prepareDecision(
    EditorDraftImportAbandon r,
  ) => original.prepareDecision(r);
  @override
  Future<EditorDraftImportSnapshot> inspectDecision(
    EditorDraftImportAbandon r,
  ) => original.inspectDecision(r);
  @override
  Future<EditorDraftImportScopePage> listDecisionScopes({
    String cursor = '',
    int limit = 32,
  }) => original.listDecisionScopes(cursor: cursor, limit: limit);
  @override
  Future<EditorDraftImportDecisionPage> listDecisions(
    String c,
    String d, {
    String cursor = '',
    int limit = 32,
  }) => original.listDecisions(c, d, cursor: cursor, limit: limit);
  @override
  Future<List<EditorDraftImportDecision>> discoverDecisions() =>
      original.discoverDecisions();
  @override
  Future<EditorDraftImportSnapshot> cancelDecision(
    EditorDraftImportAbandon r,
  ) => original.cancelDecision(r);
  @override
  Future<EditorDraftImportSnapshot> abandon(EditorDraftImportAbandon r) =>
      original.abandon(r);
}

final class FaultDraftRuntime implements WorkbenchEditorDraftRuntime {
  FaultDraftRuntime(this.host, this.stage, {this.beforeRetire});
  final RustWorkbench host;
  final String stage;
  final Future<void> Function()? beforeRetire;
  late LostProposalReply proposals;
  late LostChildReply childWrites;
  late LostImportCleanupReply importCleanup;
  int acknowledgements = 0, begins = 0;

  @override
  EditorDraftWorkspace get editorDraftWorkspace => host.editorDraftWorkspace;
  @override
  EditorDraftOwner? retainedEditorDraft(String cardId) =>
      host.retainedEditorDraft(cardId);
  @override
  Future<EditorDraftOwner> createEditorDraftOwner({
    required WorkbenchEditorSession editor,
    required EditorDraftSnapshot initial,
    required List<IdeaAttachment> attachments,
    required bool create,
  }) => host.createEditorDraftOwner(
    editor: editor,
    initial: initial,
    attachments: attachments,
    create: create,
  );
  @override
  Future<EditorDraftClosePreparation> prepareEditorDraftClose() =>
      host.prepareEditorDraftClose();
  @override
  Future<void> finishEditorDraftClose(
    EditorDraftClosePreparation preparation,
  ) => host.finishEditorDraftClose(preparation);
  @override
  Future<IdeaAttachment> importEditorDraftFile(
    EditorDraftOwner owner, {
    required String path,
    required String name,
    required String kind,
  }) => host.importEditorDraftFile(owner, path: path, name: name, kind: kind);
  @override
  Future<IdeaAttachment> importEditorDraftSource(
    EditorDraftOwner owner, {
    required XFile source,
    required String kind,
  }) => host.importEditorDraftSource(owner, source: source, kind: kind);
  @override
  EditorDraftCommitAttempt beginEditorDraftCommit(
    EditorDraftOwner owner,
    Idea template,
  ) {
    begins++;
    final original = host.editorDraftCommitServices(owner);
    proposals = LostProposalReply(original.proposals, stage);
    proposals.beforeRetire = beforeRetire;
    childWrites = LostChildReply(original.drafts);
    importCleanup = LostImportCleanupReply(original.imports);
    return owner.beginCommit(
      template,
      EditorDraftCommitServices(
        drafts: stage == 'child' ? childWrites : original.drafts,
        proposals: proposals,
        imports: stage == 'imports' ? importCleanup : original.imports,
        inspectCommit: original.inspectCommit,
        readSource: original.readSource,
        openSuccessor: original.openSuccessor,
        presentSource: original.presentSource,
        retain: original.retain,
        acknowledge: (editor, proof) async {
          acknowledgements++;
          await original.acknowledge(editor, proof);
        },
      ),
    );
  }
}
