import '../attachments/attachment.dart';
import 'editor_draft.dart';

/// Native editors can bind exact host-owned journal pins before a business
/// submission. A preview's plugin ID alone is never a staged business asset.
abstract interface class WorkbenchEditorDraftAssetBinding {
  Future<void> bindDraftAssets(
    EditorDraftRecord original,
    List<IdeaAttachment> selected,
  );
}
