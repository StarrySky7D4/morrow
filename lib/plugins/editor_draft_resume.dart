import 'editor_draft_owner.dart';
import 'editor_draft_review.dart';

abstract interface class WorkbenchEditorDraftResumeSupport {
  /// Retains the original attempt in the workbench. A second call inspects a
  /// dispatched successor; it never allocates another operation or scope.
  Future<EditorDraftOwner> resumeEditorDraft(EditorDraftReview original);
}
