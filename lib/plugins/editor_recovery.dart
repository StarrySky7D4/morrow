import 'versioned_content.dart';

enum EditorRecoveryStatus { pending, committed, conflict }

/// A read-only observation, not authority to replace a newer proposal.
final class EditorRecovery {
  EditorRecovery({
    required this.id,
    required this.title,
    required this.operation,
    required List<int> digest,
    required this.sourceRevision,
    required this.currentRevision,
    required this.status,
  }) : digest = List.unmodifiable(digest);
  final String id, title, operation;
  final List<int> digest;
  final BigInt sourceRevision, currentRevision;
  final EditorRecoveryStatus status;
}

abstract interface class WorkbenchEditorRecovery {
  Future<List<EditorRecovery>> inspectEditorRecoveries({String? id});
  Future<VersionedMutationResult> resumeEditorRecovery(EditorRecovery observed);
  Future<void> acknowledgeEditorRecovery(EditorRecovery observed);
  Future<void> abandonEditorRecovery(EditorRecovery observed);
}

/// A verified historical S1 description for a successor draft. This is a
/// read-only evidence reference, not authority to resume a capture or write.
abstract interface class EditorDraftPredecessorSource {
  EditorRecovery? get draftPredecessor;
}

/// Presentation must succeed before the capture recovery record is cleared.
abstract interface class VersionedEditorAcknowledgement {
  Future<void> acknowledgePresented(VersionedCommitReceipt receipt);
}
