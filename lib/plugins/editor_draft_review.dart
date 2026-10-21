import 'editor_draft.dart';
import 'editor_draft_assets.dart';
import 'editor_draft_retirement.dart';

/// A read-only recovery projection. It carries no live capture, replay grant,
/// draft owner or business acknowledgement authority.
final class EditorDraftReview {
  EditorDraftReview({
    required this.record,
    required List<EditorDraftAssetView> views,
    this.captureArchive,
    this.commit,
  }) : views = List.unmodifiable(views);
  final EditorDraftRecord record;
  final List<EditorDraftAssetView> views;
  final String? captureArchive;
  final EditorDraftCommitEvidence? commit;
}

abstract interface class WorkbenchEditorDraftRecovery {
  EditorDraftControl get editorDrafts;
  Future<EditorDraftReview> inspectEditorDraft(EditorDraftSummary original);
  Future<String> exportEditorDraft(
    EditorDraftReview review,
    String selectedDirectory,
  );
}

abstract interface class WorkbenchEditorDraftMaintenance {
  /// Retained by the workbench, including after a dialog is closed. This never
  /// takes over a live editor owner or retires an S1/S2 handoff parent.
  EditorDraftRetirementAttempt retirementFor(EditorDraftReview review);
}
