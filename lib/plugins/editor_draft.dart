/// A durable host-owned editor draft is separate from a captured business edit.
abstract interface class WorkbenchEditorDraftSupport {
  EditorDraftControl get editorDrafts;
}

abstract interface class EditorDraftControl {
  Future<EditorDraftRecord> save(EditorDraftWriteRequest request);
  Future<EditorDraftRecord?> read(String cardId, String draftId);
  Future<List<EditorDraftSummary>> list();
  Future<EditorDraftRecord> discard(
    String cardId,
    String draftId,
    BigInt expectedGeneration,
    String operation,
  );
  Future<EditorDraftImportedAsset> importAsset(
    String cardId,
    String draftId,
    BigInt generation,
    String path,
    String name,
    String kind,
    BigInt bytes,
  );
  Future<void> exportAsset(
    String cardId,
    String draftId,
    BigInt generation,
    String assetId,
    String path,
  );
}

/// Keep [request] unchanged after an uncertain save; a transport abort is not
/// evidence that the host journal transaction was rolled back.
final class EditorDraftSaveFailure implements Exception {
  const EditorDraftSaveFailure({
    required this.request,
    required this.outcomeUnknown,
    required this.cause,
  });

  final EditorDraftWriteRequest request;
  final bool outcomeUnknown;
  final Object cause;

  @override
  String toString() => outcomeUnknown
      ? 'Editor draft save outcome is unknown; inspect the original operation'
      : 'Editor draft save was rejected before submission';
}

/// Handoff and parent retirement are separate from ordinary draft saves.
abstract interface class WorkbenchEditorDraftHandoffSupport {
  EditorDraftHandoffControl get editorDraftHandoffs;
}

abstract interface class EditorDraftHandoffControl {
  Future<EditorDraftRecord> handoff(EditorDraftHandoffRequest request);
  Future<EditorDraftRecord> retireParent(EditorDraftRetirementRequest request);
  Future<EditorDraftLineagePage> listLineages({
    String cursor = '',
    int limit = 32,
  });
  Future<List<EditorDraftLineage>> discoverLineages();
}

final class EditorDraftHandoffFailure implements Exception {
  const EditorDraftHandoffFailure({
    required this.request,
    required this.outcomeUnknown,
    required this.cause,
  });

  final EditorDraftHandoffRequest request;
  final bool outcomeUnknown;
  final Object cause;

  @override
  String toString() => outcomeUnknown
      ? 'Editor draft handoff outcome is unknown; inspect the original operation'
      : 'Editor draft handoff was rejected before submission';
}

final class EditorDraftRetirementFailure implements Exception {
  const EditorDraftRetirementFailure({
    required this.request,
    required this.outcomeUnknown,
    required this.cause,
  });

  final EditorDraftRetirementRequest request;
  final bool outcomeUnknown;
  final Object cause;

  @override
  String toString() => outcomeUnknown
      ? 'Editor draft parent retirement outcome is unknown; inspect the original operation'
      : 'Editor draft parent retirement was rejected before submission';
}

final class EditorDraftTextValue {
  const EditorDraftTextValue({
    required this.text,
    required this.selectionBase,
    required this.selectionExtent,
    required this.affinity,
    required this.directional,
    required this.composingStart,
    required this.composingEnd,
  });

  final String text;
  final int selectionBase, selectionExtent, affinity;
  final bool directional;
  final int composingStart, composingEnd;
}

final class EditorDraftValues {
  const EditorDraftValues({
    required this.title,
    required this.description,
    required this.hypothesis,
    required this.conclusion,
    required this.todos,
    required this.category,
    required this.stage,
  });

  final EditorDraftTextValue title, description, hypothesis, conclusion, todos;
  final String category, stage;
}

enum EditorDraftAssetOrigin {
  source,
  predecessor,
  staged,
  previousDraft,
  parentDraft,
}

/// A new card has no authoritative source card or predecessor evidence.
enum EditorDraftSourceKind { existingCard, newCard }

final class EditorDraftAssetSelection {
  EditorDraftAssetSelection({
    required this.origin,
    required this.assetId,
    required List<String> aliases,
  }) : aliases = List.unmodifiable(aliases);

  final EditorDraftAssetOrigin origin;
  final String assetId;
  final List<String> aliases;
}

final class EditorDraftWriteRequest {
  EditorDraftWriteRequest({
    required this.cardId,
    required this.draftId,
    required this.operation,
    required this.expectedGeneration,
    required this.sourceRevision,
    this.sourceKind = EditorDraftSourceKind.existingCard,
    required this.predecessorOperation,
    required List<int> predecessorDigest,
    required this.values,
    required List<EditorDraftAssetSelection> assets,
  }) : predecessorDigest = List.unmodifiable(predecessorDigest),
       assets = List.unmodifiable(assets);

  final String cardId, draftId, operation, predecessorOperation;
  final BigInt expectedGeneration, sourceRevision;
  final EditorDraftSourceKind sourceKind;
  final List<int> predecessorDigest;
  final EditorDraftValues values;
  final List<EditorDraftAssetSelection> assets;
}

/// Immutable evidence binding an initial child draft to one exact parent save
/// and one confirmed business commit. It grants no authority by itself.
final class EditorDraftParentLink {
  EditorDraftParentLink({
    required this.parentDraftId,
    required this.parentGeneration,
    required this.parentSaveOperation,
    required List<int> parentRequestSha256,
    required this.committedOperation,
    required List<int> committedSha256,
    required this.childOperation,
  }) : parentRequestSha256 = List.unmodifiable(parentRequestSha256),
       committedSha256 = List.unmodifiable(committedSha256);

  factory EditorDraftParentLink.fromParentRecord({
    required EditorDraftRecord parent,
    required String committedOperation,
    required List<int> committedSha256,
    required String childOperation,
  }) {
    if (!parent.active ||
        !parent.currentActive ||
        parent.currentGeneration != parent.generation ||
        parent.requestSha256.length != 32 ||
        committedSha256.length != 32) {
      throw const FormatException(
        'Parent draft lacks a confirmed request hash',
      );
    }
    return EditorDraftParentLink(
      parentDraftId: parent.request.draftId,
      parentGeneration: parent.generation,
      parentSaveOperation: parent.request.operation,
      parentRequestSha256: parent.requestSha256,
      committedOperation: committedOperation,
      committedSha256: committedSha256,
      childOperation: childOperation,
    );
  }
  final String parentDraftId, parentSaveOperation, committedOperation;
  final String childOperation;
  final BigInt parentGeneration;
  final List<int> parentRequestSha256, committedSha256;
}

final class EditorDraftParentRetirement {
  const EditorDraftParentRetirement({
    required this.childDraftId,
    required this.childOperation,
    required this.operation,
    required this.parentGeneration,
  });

  final String childDraftId, childOperation, operation;
  final BigInt parentGeneration;
}

final class EditorDraftHandoffRequest {
  const EditorDraftHandoffRequest({
    required this.request,
    required this.parentLink,
  });

  final EditorDraftWriteRequest request;
  final EditorDraftParentLink parentLink;
}

final class EditorDraftRetirementRequest {
  const EditorDraftRetirementRequest({
    required this.cardId,
    required this.childDraftId,
    required this.parentDraftId,
    required this.childOperation,
    required this.parentGeneration,
    required this.operation,
  });

  final String cardId, childDraftId, parentDraftId, childOperation, operation;
  final BigInt parentGeneration;
}

final class EditorDraftStoredAsset {
  EditorDraftStoredAsset({
    required this.selection,
    required this.name,
    required this.mediaType,
    required this.bytes,
    required List<int> sha256,
  }) : sha256 = List.unmodifiable(sha256);

  final EditorDraftAssetSelection selection;
  final String name, mediaType;
  final BigInt bytes;
  final List<int> sha256;
}

/// [generation] identifies the historical operation. [currentGeneration] and
/// [currentActive] describe the journal now, including after a later discard.
final class EditorDraftRecord {
  EditorDraftRecord({
    required this.request,
    required this.generation,
    required this.active,
    required this.currentGeneration,
    required this.currentActive,
    required this.repeated,
    required this.sourceFormat,
    required this.sourceRevision,
    required List<int> sourceSha256,
    required this.predecessorRevision,
    required List<int> predecessorSha256,
    required List<EditorDraftStoredAsset> assets,
    List<int> requestSha256 = const [],
    this.parentLink,
    this.retirement,
  }) : sourceSha256 = List.unmodifiable(sourceSha256),
       predecessorSha256 = List.unmodifiable(predecessorSha256),
       assets = List.unmodifiable(assets),
       requestSha256 = List.unmodifiable(requestSha256);

  final EditorDraftWriteRequest request;
  final BigInt generation, currentGeneration, sourceRevision;
  final BigInt predecessorRevision;
  final bool active, currentActive, repeated;
  final int sourceFormat;
  final List<int> sourceSha256, predecessorSha256;
  final List<EditorDraftStoredAsset> assets;
  final List<int> requestSha256;
  final EditorDraftParentLink? parentLink;
  final EditorDraftParentRetirement? retirement;
}

final class EditorDraftSummary {
  const EditorDraftSummary({
    required this.cardId,
    required this.draftId,
    required this.generation,
    required this.active,
  });

  final String cardId, draftId;
  final BigInt generation;
  final bool active;
}

final class EditorDraftLineage {
  const EditorDraftLineage({
    required this.cardId,
    required this.childDraftId,
    required this.childGeneration,
    required this.childActive,
    required this.parentGeneration,
    required this.parentActive,
    required this.parentLink,
    required this.cursor,
  });

  final String cardId, childDraftId, cursor;
  final BigInt childGeneration, parentGeneration;
  final bool childActive, parentActive;
  final EditorDraftParentLink parentLink;
}

final class EditorDraftLineagePage {
  EditorDraftLineagePage({
    required List<EditorDraftLineage> lineages,
    required this.nextCursor,
    required this.requestCursor,
    required this.requestLimit,
  }) : lineages = List.unmodifiable(lineages);

  final List<EditorDraftLineage> lineages;
  final String nextCursor, requestCursor;
  final int requestLimit;
}

final class EditorDraftImportedAsset {
  const EditorDraftImportedAsset({
    required this.id,
    required this.name,
    required this.kind,
    required this.bytes,
  });

  final String id, name, kind;
  final BigInt bytes;
}

enum EditorDraftResultKind {
  absent,
  record,
  list,
  imported,
  exported,
  lineages,
}

/// The caller must also compare this context with its outer host response.
final class EditorDraftEnvelope {
  EditorDraftEnvelope({
    required this.kind,
    required this.cardId,
    required this.draftId,
    required this.operation,
    required this.expectedGeneration,
    this.record,
    List<EditorDraftSummary>? summaries,
    this.asset,
    this.lineagePage,
  }) : summaries = summaries == null ? null : List.unmodifiable(summaries);

  final EditorDraftResultKind kind;
  final String cardId, draftId, operation;
  final BigInt expectedGeneration;
  final EditorDraftRecord? record;
  final List<EditorDraftSummary>? summaries;
  final EditorDraftImportedAsset? asset;
  final EditorDraftLineagePage? lineagePage;
}
