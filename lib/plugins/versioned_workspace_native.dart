part of 'workbench_native.dart';

final class _WorkspaceCachedAsset {
  const _WorkspaceCachedAsset({
    required this.revision,
    required this.metadata,
    required this.attachment,
    required this.sha256,
  });

  final BigInt revision;
  final VersionedAsset metadata;
  final IdeaAttachment attachment;
  final String sha256;

  bool matches(VersionedAsset other) =>
      metadata.id == other.id &&
      metadata.name == other.name &&
      metadata.kind == other.kind &&
      metadata.bytes == other.bytes;

  _WorkspaceCachedAsset atRevision(BigInt value) => _WorkspaceCachedAsset(
    revision: value,
    metadata: metadata,
    attachment: attachment,
    sha256: sha256,
  );
}

Future<String> _workspaceFileSha256(File file) async =>
    (await sha256.bind(file.openRead()).first).toString();

Future<List<IdeaAttachment>> _workspaceAssets(
  RustWorkbench owner,
  VersionedContentRecord record,
) async {
  final attachments = <IdeaAttachment>[];
  for (final asset in record.assets) {
    final kind = TextureKind.values.byName(asset.kind);
    final key = '${record.id}/${asset.id}';
    final cached = owner._workspaceAssetCache[key];
    if (owner._deviceFiles case final files?) {
      if (cached != null &&
          cached.revision == record.revision &&
          cached.matches(asset) &&
          files.hasDevicePreview(cached.attachment.source)) {
        attachments.add(cached.attachment);
        continue;
      }
      final exported = await owner._deviceFileJob(
        (files) =>
            files.exportDeviceFile(record.id, asset.id, asset.name, kind),
      );
      if (exported.bytes != asset.bytes) {
        files.releaseDevicePreview(exported.source);
        throw const FormatException(
          'Attachment length differs from current content',
        );
      }
      final IdeaAttachment item;
      if (cached != null &&
          cached.matches(asset) &&
          cached.sha256 == exported.sha256 &&
          files.hasDevicePreview(cached.attachment.source)) {
        files.releaseDevicePreview(exported.source);
        item = cached.attachment;
      } else {
        item = IdeaAttachment.versioned(
          source: exported.source,
          byteLength: asset.bytes,
          pluginId: asset.id,
        );
      }
      owner._workspaceAssetCache[key] = _WorkspaceCachedAsset(
        revision: record.revision,
        metadata: asset,
        attachment: item,
        sha256: exported.sha256,
      );
      attachments.add(item);
      continue;
    }
    if (cached != null &&
        cached.revision == record.revision &&
        cached.matches(asset) &&
        await File(cached.attachment.source.location).exists()) {
      attachments.add(cached.attachment);
      continue;
    }
    final extension = asset.name
        .split('.')
        .last
        .toLowerCase()
        .replaceAll(RegExp('[^a-z0-9]'), '');
    final path = '${owner.cache.path}/${record.id}_${asset.id}.$extension';
    final file = File(
      '$path.${DateTime.now().microsecondsSinceEpoch}.$extension',
    );
    await owner._call(
      host.Action.exportFile,
      configure: (request) {
        request.id = record.id;
        request.attachment = asset.id;
        request.selectedPath = file.path;
      },
    );
    owner._sessionPreviewFiles.add(file.path);
    late final String digest;
    try {
      digest = await _workspaceFileSha256(file);
    } catch (_) {
      owner._sessionPreviewFiles.remove(file.path);
      try {
        await file.delete();
      } catch (_) {}
      rethrow;
    }
    IdeaAttachment item;
    final oldFile = cached == null
        ? null
        : File(cached.attachment.source.location);
    // Metadata alone cannot prove immutable bytes across an arbitrary card
    // revision. Compare the freshly exported content before reusing its file.
    if (cached != null &&
        cached.matches(asset) &&
        cached.sha256 == digest &&
        await oldFile!.exists() &&
        await _workspaceFileSha256(oldFile) == digest) {
      await file.delete();
      owner._sessionPreviewFiles.remove(file.path);
      item = cached.attachment;
    } else {
      item = IdeaAttachment.versioned(
        source: TextureSource(
          location: file.path,
          name: asset.name,
          kind: kind,
          local: true,
        ),
        byteLength: asset.bytes,
        pluginId: asset.id,
      );
    }
    owner._workspaceAssetCache[key] = _WorkspaceCachedAsset(
      revision: record.revision,
      metadata: asset,
      attachment: item,
      sha256: digest,
    );
    attachments.add(item);
  }
  return attachments;
}

Future<Idea> _workspaceRecord(
  RustWorkbench owner,
  VersionedContentRecord initial,
) async {
  if (initial.id.isEmpty) throw const FormatException('Missing content ID');
  var record = initial;
  for (var attempt = 0; attempt < 3; attempt++) {
    final knownBefore = owner.knownContentRevision(record.id);
    if (knownBefore != null && record.revision < knownBefore) {
      record = await owner.versionedContent.read(record.id);
    }
    final view = VersionedIdeaView.fromRecord(record);
    final attachments = await _workspaceAssets(owner, record);
    final knownAfter = owner.knownContentRevision(record.id);
    if (knownAfter != null && record.revision < knownAfter) {
      record = await owner.versionedContent.read(record.id);
      continue;
    }
    final idea = Idea(
      record.title,
      record.description,
      record.category,
      record.icon < Idea.icons.length
          ? Idea.icons[record.icon]
          : Idea.icons.first,
      Color(record.color),
      id: record.id,
      stage: record.stage,
      favorite: record.favorite,
      todos: record.formatVersion == 1 ? record.todos : const [],
      completed: record.formatVersion == 1
          ? record.completed.toSet()
          : <String>{},
      hypothesis: record.hypothesis,
      conclusion: record.conclusion,
      attachments: attachments,
      contentRevision: record.revision,
      contentOwner: owner,
      contentDeleted: record.deleted,
      versioned: record.formatVersion == 2 ? view : null,
    );
    owner._rememberRevision(record.id, record.revision);
    owner._knownDeleted[record.id] = record.deleted;
    return idea;
  }
  throw StateError('Current content changed during attachment export');
}

Future<List<Idea>> _loadWorkspaceContent(RustWorkbench owner) async {
  final scanned = await owner.loadVersioned();
  final visible = <String, Idea>{};
  final revisions = <String, BigInt>{};
  void retain(Idea idea) {
    revisions[idea.id] = idea.contentRevision!;
    if (idea.contentDeleted) {
      visible.remove(idea.id);
    } else {
      visible[idea.id] = idea;
    }
  }

  for (final record in scanned) {
    retain(await _workspaceRecord(owner, record));
  }
  for (var attempt = 0; attempt < 3; attempt++) {
    for (final entry in owner._revisions.entries.toList(growable: false)) {
      final known = entry.value;
      final shown = revisions[entry.key];
      if (shown != null && shown >= known) continue;
      retain(
        await _workspaceRecord(
          owner,
          await owner.versionedContent.read(entry.key),
        ),
      );
    }
    final complete = owner._revisions.entries.every((entry) {
      final shown = revisions[entry.key];
      return shown != null && shown >= entry.value;
    });
    if (complete) {
      return List<Idea>.unmodifiable(visible.values.toList().reversed);
    }
  }
  throw StateError('Content changed during workspace scan');
}

void _requireVersionedIdea(RustWorkbench owner, String operation, Idea idea) {
  final view = idea.versioned;
  if (!owner.writable) {
    throw StateError('工作台插件不可用，已有内容仍可查看和导出。');
  }
  if (operation.isEmpty ||
      (idea.contentOwner != null && !identical(idea.contentOwner, owner)) ||
      view == null ||
      view.formatVersion != 2 ||
      view.id != idea.id ||
      idea.contentRevision != view.revision) {
    throw const FormatException('Invalid versioned mutation source');
  }
}

void _promotePreservedWorkspaceAssets(
  RustWorkbench owner,
  Idea source,
  VersionedMutationResult result,
) {
  final view = source.versioned;
  final target = result.current;
  if (view == null ||
      result.receipt.repeated ||
      result.receipt.revision != view.revision + BigInt.one ||
      target.revision != result.receipt.revision ||
      target.id != view.id ||
      target.formatVersion != 2 ||
      view.assets.length != target.assets.length) {
    return;
  }
  for (var index = 0; index < view.assets.length; index++) {
    final before = view.assets[index];
    final after = target.assets[index];
    if (before.id != after.id ||
        before.name != after.name ||
        before.kind != after.kind ||
        before.bytes != after.bytes) {
      return;
    }
  }
  // The core's task edit and non-Edit card actions inherit the source Card's
  // attachment records, including each SHA. This proof is valid only for the
  // just-committed next revision, never for a historical retry or later read.
  for (final asset in target.assets) {
    final key = '${target.id}/${asset.id}';
    final cached = owner._workspaceAssetCache[key];
    if (cached != null &&
        cached.revision == view.revision &&
        cached.matches(asset)) {
      owner._workspaceAssetCache[key] = cached.atRevision(target.revision);
    }
  }
}

Future<Idea> _workspaceAfterCommit(
  RustWorkbench owner,
  VersionedMutationResult result, {
  Idea? preservedSource,
}) async {
  try {
    if (preservedSource != null) {
      _promotePreservedWorkspaceAssets(owner, preservedSource, result);
    }
    return await _workspaceRecord(owner, result.current);
  } catch (error) {
    throw WorkbenchCommittedRefreshFailure(error);
  }
}

Future<Idea> _applyWorkspaceTask(
  RustWorkbench owner,
  String operation,
  Idea idea,
  TaskEditCommand command,
) async {
  _requireVersionedIdea(owner, operation, idea);
  final result = await owner.versionedContent.editTasks(
    operation,
    idea.id,
    idea.versioned!.revision,
    command,
  );
  return _workspaceAfterCommit(owner, result, preservedSource: idea);
}

Future<Idea> _applyWorkspaceCard(
  RustWorkbench owner,
  String operation,
  Idea idea,
  CardEditCommand command,
) async {
  _requireVersionedIdea(owner, operation, idea);
  final result = await owner.versionedContent.editCard(
    operation,
    idea.id,
    idea.versioned!.revision,
    command,
  );
  return _workspaceAfterCommit(
    owner,
    result,
    preservedSource: command.kind == CardEditKind.edit ? null : idea,
  );
}

Future<Idea> _applyVersionedWorkspaceAction(
  RustWorkbench owner,
  PluginAction action,
  Idea idea, {
  required String text,
  required bool flag,
}) async {
  if (action == PluginAction.todo) {
    throw const FormatException('Use TaskId command for versioned tasks');
  }
  if (action == PluginAction.create || action == PluginAction.edit) {
    throw const FormatException('Use versioned editor for card fields');
  }
  if (action == PluginAction.stage && text.isEmpty) {
    throw const FormatException('Missing versioned stage');
  }
  final operation =
      'ui-${DateTime.now().microsecondsSinceEpoch}-${owner._sequence++}';
  _requireVersionedIdea(owner, operation, idea);
  final result = await switch (action) {
    PluginAction.favorite => owner.versionedContent.editCard(
      operation,
      idea.id,
      idea.versioned!.revision,
      CardEditCommand.setFavorite(flag),
    ),
    PluginAction.toProject => owner.versionedContent.editCard(
      operation,
      idea.id,
      idea.versioned!.revision,
      const CardEditCommand.setCategory('进行中', '计划中'),
    ),
    PluginAction.delete => owner.versionedContent.editCard(
      operation,
      idea.id,
      idea.versioned!.revision,
      const CardEditCommand.delete(),
    ),
    PluginAction.restore => owner.versionedContent.editCard(
      operation,
      idea.id,
      idea.versioned!.revision,
      const CardEditCommand.restore(),
    ),
    PluginAction.stage => owner.versionedContent.editTasks(
      operation,
      idea.id,
      idea.versioned!.revision,
      TaskEditCommand.setStage(text),
    ),
    PluginAction.create ||
    PluginAction.edit ||
    PluginAction.todo => throw StateError('Unreachable versioned action'),
  };
  return _workspaceAfterCommit(owner, result, preservedSource: idea);
}
