part of 'workbench_native.dart';

TextureKind _draftAssetKind(String mediaType) {
  final type = mediaType.split(';').first.toLowerCase();
  if (type == 'image/gif') return TextureKind.gif;
  if (type.startsWith('image/')) return TextureKind.image;
  if (type.startsWith('video/')) return TextureKind.video;
  if (type.startsWith('audio/')) return TextureKind.audio;
  return TextureKind.file;
}

void _sameReviewedDraft(
  EditorDraftRecord expected,
  EditorDraftRecord? current,
) {
  if (current == null ||
      !current.active ||
      !current.currentActive ||
      current.generation != expected.generation ||
      current.currentGeneration != expected.generation ||
      !RustWorkbench._same(current.requestSha256, expected.requestSha256) ||
      !RustWorkbench._same(
        EditorDraftCodec.encodeWrite(current.request),
        EditorDraftCodec.encodeWrite(expected.request),
      )) {
    throw StateError('Original recovery draft changed; inspect it again');
  }
}

Future<EditorDraftReview> _inspectEditorDraft(
  RustWorkbench host,
  EditorDraftSummary original,
) async {
  final record = await host.editorDrafts.read(
    original.cardId,
    original.draftId,
  );
  if (record == null ||
      !original.active ||
      !record.active ||
      !record.currentActive ||
      record.generation != original.generation ||
      record.currentGeneration != original.generation) {
    throw StateError('Selected recovery draft is no longer current');
  }
  final views = <EditorDraftAssetView>[];
  for (final stored in record.assets) {
    // Export a fresh verified pin. An old local alias is display data only.
    final file = File(
      '${host.cache.path}/draft-review-${host._sequence++}.bin',
    );
    await host.editorDrafts.exportAsset(
      original.cardId,
      original.draftId,
      record.generation,
      stored.selection.assetId,
      file.path,
    );
    host._sessionPreviewFiles.add(file.path);
    final measured = (await sha256.bind(file.openRead()).first).bytes;
    if (BigInt.from(await file.length()) != stored.bytes ||
        !RustWorkbench._same(measured, stored.sha256)) {
      throw const FormatException('Recovery attachment bytes changed');
    }
    views.add(
      EditorDraftAssetView(
        attachment: IdeaAttachment.versioned(
          source: TextureSource(
            location: file.path,
            name: stored.name,
            kind: _draftAssetKind(stored.mediaType),
            local: true,
          ),
          byteLength: stored.bytes,
          pluginId: stored.selection.assetId,
        ),
        aliases: stored.selection.aliases,
        sha256: measured,
        mediaType: stored.mediaType,
      ),
    );
  }
  EditorDraftAssetCatalog.restore(record, views);
  String? capture;
  if (record.request.captureScope.isNotEmpty) {
    final file = File(
      '${host.cache.path}/draft-review-capture-${host._sequence++}.pb',
    );
    await (host.editorDrafts as EditorDraftCaptureExport).exportCapture(
      original.cardId,
      original.draftId,
      record.generation,
      file.path,
    );
    host._sessionPreviewFiles.add(file.path);
    capture = file.path;
  }
  EditorDraftCommitEvidence? commit;
  if (record.request.commitOperation.isNotEmpty) {
    try {
      final observed = await host.inspectEditorCommit(
        id: original.cardId,
        operation: record.request.commitOperation,
      );
      if (observed.sourceRevision != record.request.sourceRevision) {
        throw StateError('Original draft business source changed');
      }
      commit = observed;
    } catch (_) {
      // Absence or inspection failure does not mean rollback and never permits
      // another Create. The recovery view stays read-only in both cases.
    }
  }
  _sameReviewedDraft(
    record,
    await host.editorDrafts.read(original.cardId, original.draftId),
  );
  return EditorDraftReview(
    record: record,
    views: views,
    captureArchive: capture,
    commit: commit,
  );
}

Future<String> _exportEditorDraft(
  RustWorkbench host,
  EditorDraftReview review,
  String selectedDirectory,
) async {
  final expected = review.record;
  final observed = await host.editorDrafts.read(
    expected.request.cardId,
    expected.request.draftId,
  );
  _sameReviewedDraft(expected, observed);
  // Asset inventories and capture requirements come from the current host
  // record, not a caller-constructed UI projection.
  final record = observed!;
  final selected = Directory(selectedDirectory);
  if (!await selected.exists()) {
    throw StateError('Selected export directory is unavailable');
  }
  final output = Directory(
    '${selected.absolute.path}/morrow-draft-${DateTime.now().microsecondsSinceEpoch}-${host._sequence++}',
  );
  if (await output.exists()) {
    throw StateError('Recovery export directory already exists');
  }
  await output.create();
  final entries = <Map<String, Object>>[];
  Future<void> write(String name, List<int> bytes) async {
    final file = File('${output.path}/$name');
    await file.writeAsBytes(bytes, flush: true);
    final digest = sha256.convert(bytes).toString();
    if (await file.length() != bytes.length ||
        (await sha256.bind(file.openRead()).first).toString() != digest) {
      throw StateError('Recovery export verification failed');
    }
    entries.add({'file': name, 'bytes': bytes.length, 'sha256': digest});
  }

  final status = File('${output.path}/manifest.json');
  await status.writeAsString(
    jsonEncode({'schema': 1, 'complete': false}),
    flush: true,
  );
  await write('request.capnp', EditorDraftCodec.encodeWrite(record.request));
  if (record.request.captureScope.isNotEmpty) {
    // Re-export from the same host generation instead of trusting a cached path.
    final destination = '${output.path}/capture.pb';
    await (host.editorDrafts as EditorDraftCaptureExport).exportCapture(
      record.request.cardId,
      record.request.draftId,
      record.generation,
      destination,
    );
    final file = File(destination);
    entries.add({
      'file': 'capture.pb',
      'bytes': await file.length(),
      'sha256': (await sha256.bind(file.openRead()).first).toString(),
    });
  }
  final assets = <Map<String, Object>>[];
  for (var i = 0; i < record.assets.length; i++) {
    final asset = record.assets[i];
    final name = 'asset-$i.bin';
    final file = File('${output.path}/$name');
    await host.editorDrafts.exportAsset(
      record.request.cardId,
      record.request.draftId,
      record.generation,
      asset.selection.assetId,
      file.path,
    );
    final digest = (await sha256.bind(file.openRead()).first).bytes;
    final length = await file.length();
    if (BigInt.from(length) != asset.bytes ||
        !RustWorkbench._same(digest, asset.sha256)) {
      throw StateError('Recovery export attachment changed');
    }
    entries.add({
      'file': name,
      'bytes': length,
      'sha256': Digest(digest).toString(),
    });
    assets.add({
      'file': name,
      'id': asset.selection.assetId,
      'name': asset.name,
      'media_type': asset.mediaType,
      'aliases': asset.selection.aliases,
      'origin': asset.selection.origin.name,
    });
  }
  Map<String, Object> raw(EditorDraftTextValue value) => {
    'text': value.text,
    'selection_base': value.selectionBase,
    'selection_extent': value.selectionExtent,
    'affinity': value.affinity,
    'directional': value.directional,
    'composing_start': value.composingStart,
    'composing_end': value.composingEnd,
  };
  final values = record.request.values;
  await write(
    'raw-values.json',
    utf8.encode(
      const JsonEncoder.withIndent('  ').convert({
        'title': raw(values.title),
        'description': raw(values.description),
        'hypothesis': raw(values.hypothesis),
        'conclusion': raw(values.conclusion),
        'todos': raw(values.todos),
        'category': values.category,
        'stage': values.stage,
        'assets': assets,
      }),
    ),
  );
  _sameReviewedDraft(
    record,
    await host.editorDrafts.read(record.request.cardId, record.request.draftId),
  );
  EditorDraftCommitEvidence? commit;
  if (record.request.commitOperation.isNotEmpty) {
    try {
      final proof = await host.inspectEditorCommit(
        id: record.request.cardId,
        operation: record.request.commitOperation,
      );
      if (proof.sourceRevision == record.request.sourceRevision) commit = proof;
    } catch (_) {
      /* Unconfirmed stays unconfirmed; no mutation or replay. */
    }
  }
  await status.writeAsString(
    const JsonEncoder.withIndent('  ').convert({
      'schema': 1,
      'complete': true,
      'read_only_recovery_copy': true,
      'card_id': record.request.cardId,
      'draft_id': record.request.draftId,
      'generation': record.generation.toString(),
      'save_operation': record.request.operation,
      'request_sha256': Digest(record.requestSha256).toString(),
      'business_operation': record.request.commitOperation,
      'commit_confirmed': commit != null,
      'absence_proves_rollback': false,
      'files': entries,
    }),
    flush: true,
  );
  return output.path;
}
