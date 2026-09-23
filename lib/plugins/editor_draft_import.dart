/// An import is a host-owned, durable selection intent. It never edits a card.
abstract interface class WorkbenchEditorDraftImportSupport {
  EditorDraftImportControl get editorDraftImports;
}

abstract interface class EditorDraftImportControl {
  Future<EditorDraftImportSnapshot> begin(EditorDraftImportRequest request);
  Future<EditorDraftImportSnapshot> complete(
    EditorDraftImportRequest request, {
    String selectedPath = '',
  });
  Future<EditorDraftImportSnapshot> inspect(
    String cardId,
    String draftId,
    String importOperation,
  );
  Future<EditorDraftImportSnapshot> list(String cardId, String draftId);
  Future<EditorDraftImportSnapshot> export(
    String cardId,
    String draftId,
    BigInt currentGeneration,
    String importOperation,
    String selectedPath,
  );
  Future<EditorDraftImportSnapshot> prepareDecision(
    EditorDraftImportAbandon intent,
  );
  Future<EditorDraftImportSnapshot> inspectDecision(
    EditorDraftImportAbandon intent,
  );
  Future<EditorDraftImportScopePage> listDecisionScopes({
    String cursor = '',
    int limit = 32,
  });
  Future<EditorDraftImportDecisionPage> listDecisions(
    String cardId,
    String draftId, {
    String cursor = '',
    int limit = 32,
  });

  /// Read all known decision scopes and their decisions as one queued job.
  Future<List<EditorDraftImportDecision>> discoverDecisions();
  Future<EditorDraftImportSnapshot> cancelDecision(
    EditorDraftImportAbandon intent,
  );
  Future<EditorDraftImportSnapshot> abandon(EditorDraftImportAbandon intent);
  Future<EditorDraftImportSnapshot> reconcile(String cardId, String draftId);
}

/// All request fields, including the digest, are frozen before queue admission.
final class EditorDraftImportRequest {
  EditorDraftImportRequest({
    required this.cardId,
    required this.draftId,
    required this.operation,
    required this.expectedGeneration,
    required this.name,
    required this.kind,
    required this.bytes,
    required List<int> sha256,
  }) : sha256 = List.unmodifiable(sha256);

  final String cardId, draftId, operation, name, kind;
  final BigInt expectedGeneration, bytes;
  final List<int> sha256;
}

final class EditorDraftImportAbandon {
  const EditorDraftImportAbandon({
    required this.cardId,
    required this.draftId,
    required this.importOperation,
    required this.operation,
    required this.currentGeneration,
  });

  final String cardId, draftId, importOperation, operation;
  final BigInt currentGeneration;
}

/// A transport failure after sending can have committed. The exact original
/// request or abandon intent must remain available for explicit retry.
final class EditorDraftImportFailure implements Exception {
  const EditorDraftImportFailure({
    required this.intent,
    required this.outcomeUnknown,
    required this.cause,
  });

  final Object intent;
  final bool outcomeUnknown;
  final Object cause;

  @override
  String toString() => outcomeUnknown
      ? 'Editor draft import outcome is unknown; inspect and retry the original operation'
      : 'Editor draft import was rejected before submission';
}

enum EditorDraftImportPhase { pending, ready, retired }

enum EditorDraftImportResultKind {
  absent,
  record,
  list,
  exported,
  reconciled,
  decision,
  decisions,
  scopes,
}

enum EditorDraftImportDecisionStatus { pending, committed, cancelled, conflict }

/// An audited abandonment intent. [expectedGeneration] is the immutable
/// decision baseline, even if the main draft later advances.
final class EditorDraftImportDecision {
  const EditorDraftImportDecision({
    required this.request,
    required this.operation,
    required this.expectedGeneration,
    required this.status,
    required this.currentGeneration,
    required this.mainActive,
    required this.stagingRevision,
    required this.decisionRevision,
    required this.committedRevision,
  });

  final EditorDraftImportRequest request;
  final String operation;
  final BigInt expectedGeneration, currentGeneration;
  final EditorDraftImportDecisionStatus status;
  final bool mainActive;
  final BigInt stagingRevision, decisionRevision, committedRevision;

  EditorDraftImportAbandon get intent => EditorDraftImportAbandon(
    cardId: request.cardId,
    draftId: request.draftId,
    importOperation: request.operation,
    operation: operation,
    currentGeneration: expectedGeneration,
  );
}

/// A global discovery result contains only stable scope identities. Inactive
/// and terminal scopes remain discoverable without opening a business card.
final class EditorDraftImportScope {
  const EditorDraftImportScope({required this.cardId, required this.draftId});
  final String cardId, draftId;
}

final class EditorDraftImportScopePage {
  EditorDraftImportScopePage({
    required this.requestCursor,
    required this.requestLimit,
    required this.nextCursor,
    required List<EditorDraftImportScope> scopes,
  }) : scopes = List.unmodifiable(scopes);

  final String requestCursor, nextCursor;
  final int requestLimit;
  final List<EditorDraftImportScope> scopes;
}

/// A bounded, cursor-bound read. The cursor is opaque and belongs to this
/// exact card, draft and page request; callers must not infer local order.
final class EditorDraftImportDecisionPage {
  EditorDraftImportDecisionPage({
    required this.cardId,
    required this.draftId,
    required this.requestCursor,
    required this.requestLimit,
    required this.nextCursor,
    required List<EditorDraftImportDecision> decisions,
    required this.currentGeneration,
    required this.mainActive,
    required this.stagingRevision,
  }) : decisions = List.unmodifiable(decisions);

  final String cardId, draftId, requestCursor, nextCursor;
  final int requestLimit;
  final List<EditorDraftImportDecision> decisions;
  final BigInt currentGeneration, stagingRevision;
  final bool mainActive;
}

final class EditorDraftImportRecord {
  const EditorDraftImportRecord({
    required this.request,
    required this.assetId,
    required this.phase,
    required this.currentActive,
    required this.bytesRetained,
    required this.stagingRevision,
    required this.repeated,
    required this.currentGeneration,
    required this.mainActive,
  });

  final EditorDraftImportRequest request;
  final String assetId;
  final EditorDraftImportPhase phase;
  final bool currentActive, bytesRetained, repeated, mainActive;
  final BigInt stagingRevision, currentGeneration;
}

/// One validated response, including the host's current main-draft context.
final class EditorDraftImportSnapshot {
  EditorDraftImportSnapshot({
    required this.kind,
    required this.cardId,
    required this.draftId,
    required this.operation,
    required this.expectedGeneration,
    required this.importOperation,
    required this.currentGeneration,
    required this.mainActive,
    required this.stagingRevision,
    this.record,
    List<EditorDraftImportRecord>? records,
    List<int>? exportSha256,
    required this.exportBytes,
    this.decision,
    List<EditorDraftImportDecision>? decisions,
    List<EditorDraftImportScope>? scopes,
    this.requestCursor = '',
    this.requestLimit = 0,
    this.nextCursor = '',
  }) : records = records == null ? null : List.unmodifiable(records),
       decisions = decisions == null ? null : List.unmodifiable(decisions),
       scopes = scopes == null ? null : List.unmodifiable(scopes),
       exportSha256 = exportSha256 == null
           ? null
           : List.unmodifiable(exportSha256);

  final EditorDraftImportResultKind kind;
  final String cardId, draftId, operation, importOperation;
  final BigInt expectedGeneration, currentGeneration, stagingRevision;
  final bool mainActive;
  final EditorDraftImportRecord? record;
  final List<EditorDraftImportRecord>? records;
  final List<int>? exportSha256;
  final BigInt exportBytes;
  final EditorDraftImportDecision? decision;
  final List<EditorDraftImportDecision>? decisions;
  final List<EditorDraftImportScope>? scopes;
  final String requestCursor, nextCursor;
  final int requestLimit;
}
