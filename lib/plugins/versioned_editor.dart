import '../attachments/attachment.dart';
import 'editor_session.dart' show EditorFields, PasteInsertion;
import 'studio_backend.dart';
import 'versioned_content.dart';

/// Captured editing of an existing format-2 card. Task changes use the
/// separate TaskId mutation API; this session edits only common card fields.
abstract interface class WorkbenchVersionedEditorSupport {
  Future<VersionedEditorSession> openVersionedEditor(
    String id, {
    BigInt? expectedRevision,
  });
}

abstract interface class VersionedEditorSession {
  String get targetId;
  StudioBackend get studio;
  Future<void> recordPaste(PasteInsertion insertion);

  /// Stage a selected file for this card and return its durable asset identity.
  Future<VersionedAsset> stageAttachment(IdeaAttachment attachment);

  /// The first attempt freezes its operation, source revision, and payload.
  /// Further calls may only retry that same edit while its outcome is pending.
  Future<VersionedMutationResult> save(
    CardEditFields fields,
    EditorFields editor,
  );
  Future<void> close();
}
