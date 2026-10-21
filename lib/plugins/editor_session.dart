import '../main.dart' show Idea;
import 'studio_backend.dart';
import 'capture_models.dart';
export 'capture_models.dart';

/// Optional capability. Existing local/Web storage keeps its original save route.
abstract interface class WorkbenchEditorSupport {
  Future<List<Idea>> refreshEditorContent();
  Future<WorkbenchEditorSession> openEditor(
    String target, {
    required bool create,
  });
}

abstract interface class WorkbenchEditorSession {
  String get targetId;
  StudioBackend get studio;
  Future<void> recordPaste(PasteInsertion insertion);

  /// The first attempt freezes its operation and payload; later calls retry that attempt.
  Future<Idea> save(Idea draft, EditorFields fields);
  Future<void> close();
}

/// Optional capability for starting a new edit from an exact confirmed save.
abstract interface class WorkbenchEditorContinuation {
  Future<WorkbenchEditorSession> continueAfterCommit(Idea confirmed);
}

/// Reserved by the original native editor before any business submission, so
/// its workbench draft can durably identify a lost first Create receipt.
/// The identity is for inspection, never authority to replay after restart.
abstract interface class WorkbenchEditorOperationIdentity {
  String get commitOperation;
  BigInt get sourceRevision;
}

/// Observed identity of this original live capture scope. An archived draft
/// never reconstructs this capability from its persisted strings.
abstract interface class WorkbenchEditorDraftCaptureIdentity {
  ({String scope, int events, bool complete}) get draftCapture;
}

/// No captured-save request was sent. The draft may safely be changed before a new attempt.
class EditorPreparationException implements Exception {
  const EditorPreparationException(this.cause);
  final Object cause;
  @override
  String toString() => cause.toString();
}
