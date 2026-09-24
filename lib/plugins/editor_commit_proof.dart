import 'editor_recovery.dart';

/// The caller's host-verified S1 commit identity. This metadata is read-only
/// evidence, never authority to submit a business edit.
final class EditorDraftCommitEvidence {
  EditorDraftCommitEvidence({
    required this.id,
    required this.operation,
    required List<int> digest,
    required this.sourceRevision,
    required this.committedRevision,
  }) : digest = List.unmodifiable(digest);

  factory EditorDraftCommitEvidence.fromRecovery(EditorRecovery recovery) {
    if (recovery.status != EditorRecoveryStatus.committed ||
        recovery.id.isEmpty ||
        recovery.operation.isEmpty ||
        recovery.digest.length != 32 ||
        recovery.sourceRevision <= BigInt.zero ||
        recovery.currentRevision != recovery.sourceRevision + BigInt.one) {
      throw const FormatException('Recovery is not a confirmed S1 commit');
    }
    return EditorDraftCommitEvidence(
      id: recovery.id,
      operation: recovery.operation,
      digest: recovery.digest,
      sourceRevision: recovery.sourceRevision,
      committedRevision: recovery.currentRevision,
    );
  }

  final String id, operation;
  final List<int> digest;
  final BigInt sourceRevision, committedRevision;
}

/// Read-only historical commit inspection, including the first captured Create.
/// Failure or absence says nothing about rollback and must not cause a replay.
abstract interface class WorkbenchEditorCommitInspection {
  Future<EditorDraftCommitEvidence> inspectEditorCommit({
    required String id,
    required String operation,
  });
}

/// The original editor owns its frozen operation. This method only inspects
/// that operation, even after a lost save reply or a closed capture scope.
abstract interface class WorkbenchEditorCommitSource {
  Future<EditorDraftCommitEvidence> inspectCommittedSource();
}
