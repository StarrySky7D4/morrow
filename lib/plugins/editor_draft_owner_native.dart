part of 'workbench_native.dart';

Future<EditorDraftOwner> _createEditorDraftOwner(
  RustWorkbench host,
  WorkbenchEditorSession editor,
  EditorDraftSnapshot initial,
  List<IdeaAttachment> attachments,
  bool create,
) async {
  bool current() =>
      host._closingProcess == null &&
      host._failure == null &&
      !host.editorDraftWorkspace.disposed;
  if (!current() ||
      !host.writable ||
      editor is! WorkbenchEditorOperationIdentity) {
    throw StateError('Native draft owner is unavailable');
  }
  final original = editor as WorkbenchEditorOperationIdentity;
  if (create != (original.sourceRevision == BigInt.zero)) {
    throw StateError('Original editor source changed');
  }
  String operation() =>
      'draft-${DateTime.now().microsecondsSinceEpoch}-${host._sequence++}';
  final draftId = operation();
  final EditorDraftAssetCatalog catalog;
  Idea? baseline;
  if (create) {
    if (attachments.isNotEmpty) {
      throw StateError('A new draft must import its attachments durably');
    }
    catalog = EditorDraftAssetCatalog.newCard(
      cardId: editor.targetId,
      draftId: draftId,
    );
  } else {
    final source = await host.versionedContent.read(editor.targetId);
    if (!current() || source.revision != original.sourceRevision) {
      throw StateError('Original editor baseline is no longer current');
    }
    baseline = await host.workspaceRecord(source);
    final views = <EditorDraftAssetView>[];
    for (final attachment in attachments) {
      if (!attachment.source.local) {
        throw UnsupportedError('Draft file preview adapter is unavailable');
      }
      final file = File(attachment.source.location);
      if (BigInt.from(await file.length()) != attachment.byteLength) {
        throw const FormatException('Source attachment preview changed');
      }
      views.add(
        EditorDraftAssetView(
          attachment: attachment,
          aliases: [attachment.source.location, attachment.source.name],
          sha256: (await sha256.bind(file.openRead()).first).bytes,
        ),
      );
    }
    catalog = EditorDraftAssetCatalog.existingCard(
      cardId: editor.targetId,
      draftId: draftId,
      source: source,
      sourceViews: views,
    );
  }
  if (!current()) throw StateError('Draft workbench changed');
  final owner = EditorDraftOwner(
    workspace: host.editorDraftWorkspace,
    control: host.editorDrafts,
    editor: editor,
    catalog: catalog,
    initial: EditorDraftSnapshot(
      values: initial.values,
      assets: catalog.selectionsFor(attachments),
    ),
    isCurrent: current,
    operationFactory: operation,
    baseline: baseline,
  );
  host._draftOwners.add(owner);
  return owner;
}

Future<IdeaAttachment> _importEditorDraftFile(
  RustWorkbench host,
  EditorDraftOwner owner,
  String path,
  String name,
  String kind,
) async {
  if (!host._draftOwners.contains(owner)) {
    throw StateError('Draft belongs to another workbench');
  }
  final view = await owner.importAsset(
    control: host.editorDraftImports,
    selectedPath: path,
    prepare: (record) async {
      final file = File(path);
      final length = await file.length();
      if (length < 0 || length > IdeaAttachment.maxSize) {
        throw const FormatException('Draft attachment byte limit');
      }
      return EditorDraftImportRequest(
        cardId: owner.session.cardId,
        draftId: owner.session.draftId,
        operation:
            'draft-import-${DateTime.now().microsecondsSinceEpoch}-${host._sequence++}',
        expectedGeneration: record.generation,
        name: name,
        kind: kind,
        bytes: BigInt.from(length),
        sha256: (await sha256.bind(file.openRead()).first).bytes,
      );
    },
    export: (record) async {
      final request = record.request;
      final extension = name
          .split('.')
          .last
          .toLowerCase()
          .replaceAll(RegExp('[^a-z0-9]'), '');
      final file = File(
        '${host.cache.path}/draft-${DateTime.now().microsecondsSinceEpoch}-${host._sequence++}.$extension',
      );
      final exported = await host.editorDraftImports.export(
        request.cardId,
        request.draftId,
        record.currentGeneration,
        request.operation,
        file.path,
      );
      host._sessionPreviewFiles.add(file.path);
      final measured = (await sha256.bind(file.openRead()).first).bytes;
      if (BigInt.from(await file.length()) != request.bytes ||
          exported.exportBytes != request.bytes ||
          !RustWorkbench._same(exported.exportSha256, request.sha256) ||
          !RustWorkbench._same(measured, request.sha256)) {
        throw const FormatException('Exported draft attachment changed');
      }
      return EditorDraftAssetView(
        attachment: IdeaAttachment.versioned(
          source: TextureSource(
            location: file.path,
            name: name,
            kind: TextureKind.values.byName(kind),
            local: true,
          ),
          byteLength: request.bytes,
          pluginId: record.assetId,
        ),
        aliases: [file.path, name, record.assetId],
        sha256: measured,
      );
    },
  );
  return view.attachment;
}
