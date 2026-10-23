part of 'workbench_native.dart';

final class _DraftAssetBinding {
  _DraftAssetBinding(
    this.owner,
    this.cardId,
    this.sourceRevision,
    this.scope,
    this.operation,
  );
  final RustWorkbench owner;
  final String cardId, scope, operation;
  final BigInt sourceRevision;
  Uint8List? _request;
  Future<List<VersionedAsset>>? _flight;

  Future<List<VersionedAsset>> bind(
    EditorDraftRecord original,
    List<IdeaAttachment> selected,
  ) {
    final request = original.request;
    final encoded = EditorDraftCodec.encodeWrite(request);
    if (_request != null && !RustWorkbench._same(_request!, encoded)) {
      return Future.error(StateError('Original asset binding cannot change'));
    }
    if (!original.active ||
        !original.currentActive ||
        original.currentGeneration != original.generation ||
        request.cardId != cardId ||
        request.commitOperation != operation ||
        request.captureScope != scope ||
        request.sourceRevision != sourceRevision ||
        original.assets.length != selected.length) {
      return Future.error(
        StateError('Draft business assets are not bound to this live editor'),
      );
    }
    final assets = <VersionedAsset>[];
    for (var i = 0; i < selected.length; i++) {
      final stored = original.assets[i], view = selected[i];
      if (stored.selection.assetId != view.pluginId ||
          stored.name != view.source.name ||
          stored.bytes != view.byteLength ||
          _draftAssetKind(stored.mediaType) != view.source.kind) {
        return Future.error(
          const FormatException('Draft business attachment metadata changed'),
        );
      }
      assets.add(
        VersionedAsset(
          id: stored.selection.assetId,
          name: stored.name,
          kind: view.source.kind.name,
          bytes: stored.bytes,
        ),
      );
    }
    _request ??= encoded;
    return _flight ??= (owner.editorDrafts as NativeEditorDraftControl)._job(
      () async {
        final lane = owner.editorDrafts as NativeEditorDraftControl;
        final (envelope, revision) = await lane._requestEnvelope(
          host.Action.stageEditorDraftAssets,
          configure: (outer) {
            lane._identity(outer, cardId, request.draftId, original.generation);
            outer.operation = operation;
            outer.captureScope = scope;
          },
        );
        lane._context(
          envelope,
          cardId,
          request.draftId,
          operation,
          original.generation,
        );
        final observed = lane._record(envelope, revision);
        if (!observed.active ||
            !observed.currentActive ||
            observed.generation != original.generation ||
            observed.currentGeneration != original.generation ||
            !RustWorkbench._same(
              observed.requestSha256,
              original.requestSha256,
            ) ||
            !RustWorkbench._same(
              EditorDraftCodec.encodeWrite(observed.request),
              encoded,
            )) {
          throw const FormatException('Draft business staging receipt changed');
        }
        return List.unmodifiable(assets);
      },
    );
  }
}
