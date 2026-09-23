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

/// No captured-save request was sent. The draft may safely be changed before a new attempt.
class EditorPreparationException implements Exception {
  const EditorPreparationException(this.cause);
  final Object cause;
  @override
  String toString() => cause.toString();
}
