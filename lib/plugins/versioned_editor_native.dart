part of 'workbench_native.dart';

Future<VersionedEditorSession> openNativeVersionedEditor(
  RustWorkbench owner,
  String id, {
  BigInt? expectedRevision,
}) async {
  if (id.isEmpty) throw const FormatException('Missing editor target');
  final current = await owner.versionedContent.read(id);
  if (expectedRevision != null && current.revision != expectedRevision) {
    throw StateError('The editor baseline changed before opening');
  }
  if (current.formatVersion != 2 || current.deleted) {
    throw StateError('The card cannot be opened in the versioned editor');
  }
  final reply = await owner._call(
    host.Action.openCaptureScope,
    configure: (request) {
      request.id = id;
      request.revisionBigInt = current.revision;
    },
  );
  final scope = reply.captureScope ?? '';
  if (scope.isEmpty) throw const FormatException('Missing editor scope');
  return _NativeVersionedEditorSession(owner, id, current.revision, scope);
}

final class _StagedVersionedAsset {
  const _StagedVersionedAsset(this.asset, this.source);
  final VersionedAsset asset;
  final IdeaAttachment source;
}

final class _NativeVersionedEditorSession
    implements
        VersionedEditorSession,
        VersionedEditorAcknowledgement,
        VersionedEditorCommitObservation,
        EditorDraftPredecessorSource {
  _NativeVersionedEditorSession(
    this.owner,
    this.targetId,
    this.revision,
    this.scope,
  );

  final RustWorkbench owner;
  @override
  final String targetId;
  final BigInt revision;
  final String scope;
  @override
  late final RustStudioPlugin studio = RustStudioPlugin(
    owner,
    captureScope: scope,
  );
  final _staged = <String, _StagedVersionedAsset>{};
  bool _closed = false;
  Future<void>? _closing;
  String? _operation, _fingerprint;
  CardEditFields? _fields;
  EditorFields? _editor;
  Uint8List? _pendingBytes;
  Future<VersionedMutationResult>? _inFlight;
  EditorRecovery? _draftPredecessor;

  @override
  EditorRecovery? get draftPredecessor => _draftPredecessor;

  @override
  Future<void> recordPaste(PasteInsertion insertion) async {
    if (_closed || _operation != null) {
      throw StateError('This editor is closed or awaiting its original save');
    }
    if (!const {
      'title',
      'description',
      'hypothesis',
      'conclusion',
    }.contains(insertion.field)) {
      throw const FormatException(
        'Versioned card paste requires a common text field',
      );
    }
    final message = MessageBuilder();
    final upload = message.initRoot(host.pasteUploadFactory);
    upload.scope = scope;
    final event = upload.initEvent();
    event.id = insertion.id;
    event.field = insertion.field;
    event.before = insertion.before;
    event.startUtf16 = insertion.startUtf16;
    event.endUtf16 = insertion.endUtf16;
    event.after = insertion.after;
    final parts = event.initParts(insertion.parts.length);
    for (var i = 0; i < insertion.parts.length; i++) {
      final part = insertion.parts[i];
      parts[i].ticket = part.ticket;
      parts[i].literal = part.literal;
      parts[i].selection = part.selection;
    }
    await owner._captureUpload(
      insertion.id,
      message.serialize(),
      host.Action.finishPaste,
    );
  }

  @override
  Future<VersionedAsset> stageAttachment(IdeaAttachment attachment) async {
    if (_closed || _operation != null) {
      throw StateError('This editor is closed or awaiting its original save');
    }
    if (attachment.pluginId != null || !attachment.source.local) {
      throw const FormatException('Select a local file to stage');
    }
    final imported = await owner._importAttachment(targetId, attachment);
    if (_closed || _operation != null) {
      throw StateError('The editor changed while staging the file');
    }
    final id = imported.pluginId;
    if (id == null || id.isEmpty || _staged.containsKey(id)) {
      throw const FormatException('Invalid staged asset identity');
    }
    final asset = VersionedAsset(
      id: id,
      name: attachment.source.name,
      kind: attachment.source.kind.name,
      bytes: BigInt.from(imported.size),
    );
    _staged[id] = _StagedVersionedAsset(asset, attachment);
    return asset;
  }

  String _intentFingerprint(CardEditFields fields, EditorFields editor) =>
      jsonEncode([
        fields.title,
        fields.description,
        fields.hypothesis,
        fields.conclusion,
        fields.icon,
        fields.color,
        [
          for (final asset in fields.assets)
            [asset.id, asset.name, asset.kind, asset.bytes.toString()],
        ],
        editor.title,
        editor.description,
        editor.hypothesis,
        editor.conclusion,
        editor.todos,
      ]);

  @override
  Future<VersionedMutationResult> save(
    CardEditFields fields,
    EditorFields editor,
  ) {
    if (_closed) return Future.error(StateError('The editor is closed'));
    final fingerprint = _intentFingerprint(fields, editor);
    if (_fingerprint != null && _fingerprint != fingerprint) {
      return Future.error(
        StateError('Retry the original pending edit without changing it'),
      );
    }
    _fingerprint ??= fingerprint;
    _operation ??=
        'editor-v2-${DateTime.now().microsecondsSinceEpoch}-${owner._sequence++}';
    _fields ??= CardEditFields(
      title: fields.title,
      description: fields.description,
      hypothesis: fields.hypothesis,
      conclusion: fields.conclusion,
      icon: fields.icon,
      color: fields.color,
      assets: fields.assets,
    );
    _editor ??= EditorFields(
      title: editor.title,
      description: editor.description,
      hypothesis: editor.hypothesis,
      conclusion: editor.conclusion,
      todos: editor.todos,
    );
    return _inFlight ??= _save().whenComplete(() => _inFlight = null);
  }

  List<_StagedVersionedAsset> _selectedAliases(CardEditFields fields) {
    final selected = <_StagedVersionedAsset>[];
    final seen = <String>{};
    for (final asset in fields.assets) {
      if (!seen.add(asset.id)) {
        throw const FormatException('Duplicate submitted asset identity');
      }
      final staged = _staged[asset.id];
      if (staged == null) continue;
      final expected = staged.asset;
      if (asset.name != expected.name ||
          asset.kind != expected.kind ||
          asset.bytes != expected.bytes) {
        throw const FormatException('Staged asset metadata changed');
      }
      selected.add(staged);
    }
    return selected;
  }

  String _normalizedDescription(
    String raw,
    List<_StagedVersionedAsset> aliases,
  ) {
    var description = raw.trim();
    if (description.isEmpty) description = '从一个小小的念头开始。';
    for (final staged in aliases) {
      final source = staged.source.source;
      final names = {
        source.location,
        source.name,
        Uri.encodeComponent(source.location),
        Uri.encodeComponent(source.name),
      }.toList()..sort((a, b) => b.length.compareTo(a.length));
      for (final name in names) {
        description = description.replaceAll(
          RegExp('attachment:${RegExp.escape(name)}(?=[\\s)>]|\u0024)'),
          'attachment:${staged.asset.id}',
        );
      }
    }
    return description;
  }

  void _validateFields(
    CardEditFields fields,
    EditorFields editor,
    List<_StagedVersionedAsset> aliases,
  ) {
    if (editor.todos.isNotEmpty ||
        fields.title != editor.title.trim() ||
        fields.description !=
            _normalizedDescription(editor.description, aliases) ||
        fields.hypothesis != editor.hypothesis.trim() ||
        fields.conclusion != editor.conclusion.trim()) {
      throw const FormatException(
        'Editor fields differ from the submitted card',
      );
    }
  }

  Future<VersionedMutationResult> _save() async {
    if (_pendingBytes == null) {
      try {
        final fields = _fields!;
        final editor = _editor!;
        final aliases = _selectedAliases(fields);
        _validateFields(fields, editor, aliases);
        final payload = VersionedContentCodec.encodeCardEdit(
          CardEditCommand.edit(fields),
        );
        final message = MessageBuilder();
        final save = message.initRoot(host.capturedCardSaveFactory);
        save.scope = scope;
        save.operation = _operation;
        save.target = targetId;
        save.revisionBigInt = revision;
        save.payload = payload;
        final snapshot = save.initSnapshot();
        snapshot.title = editor.title;
        snapshot.description = editor.description;
        snapshot.hypothesis = editor.hypothesis;
        snapshot.conclusion = editor.conclusion;
        snapshot.todos = '';
        final out = snapshot.initAliases(aliases.length);
        for (var i = 0; i < aliases.length; i++) {
          final staged = aliases[i];
          out[i].id = staged.asset.id;
          out[i].location = staged.source.source.location;
          out[i].name = staged.source.source.name;
        }
        _pendingBytes = message.serialize();
        if (_pendingBytes!.length > 4 * 1024 * 1024) {
          throw const FormatException('Captured card save exceeds 4 MiB');
        }
      } catch (error) {
        _operation = null;
        _fingerprint = null;
        _fields = null;
        _editor = null;
        _pendingBytes = null;
        throw EditorPreparationException(error);
      }
    }
    final reply = await owner._captureUpload(
      _operation!,
      _pendingBytes!,
      host.Action.finishCapturedCard,
    );
    final receipt = VersionedContentCodec.decodeCommit(
      reply.payload ?? Uint8List(0),
      id: targetId,
      operation: _operation!,
      sourceRevision: revision,
      outerRevision: reply.revisionBigInt,
    );
    owner._rememberRevision(targetId, receipt.revision);
    try {
      final current = await owner.versionedContent.read(targetId);
      return VersionedMutationResult(receipt: receipt, current: current);
    } catch (error) {
      throw VersionedCommittedRefreshFailure(receipt, error);
    }
  }

  bool _samePredecessor(EditorRecovery a, EditorRecovery b) =>
      a.id == b.id &&
      a.operation == b.operation &&
      a.sourceRevision == b.sourceRevision &&
      base64Encode(a.digest) == base64Encode(b.digest);

  Future<({EditorRecovery? observed, bool pending})> _inspectPresented(
    VersionedCommitReceipt receipt, {
    required bool allowUnobservedAbsence,
    required bool exactCurrentRevision,
  }) async {
    if (receipt.id != targetId ||
        receipt.operation != _operation ||
        receipt.revision != revision + BigInt.one) {
      throw const FormatException('Editor acknowledgement identity changed');
    }
    final rows = await owner.inspectEditorRecoveries(id: targetId);
    if (rows.isEmpty) {
      final prior = _draftPredecessor;
      if (prior == null && allowUnobservedAbsence) {
        // Preserve the previous acknowledgement replay behavior.
        return (observed: null, pending: false);
      }
      if (prior == null ||
          !prior.matchesPresentedReceipt(
            receipt,
            sourceRevision: revision,
            exactCurrentRevision: exactCurrentRevision,
          )) {
        throw StateError('Original editor recovery proof is unavailable');
      }
      return (observed: prior, pending: false);
    }
    if (rows.length != 1) {
      throw StateError('Editor recovery outcome changed');
    }
    final observed = rows.single;
    if (!observed.matchesPresentedReceipt(
      receipt,
      sourceRevision: revision,
      exactCurrentRevision: exactCurrentRevision,
    )) {
      throw StateError('Editor recovery outcome changed');
    }
    final prior = _draftPredecessor;
    if (prior != null && !_samePredecessor(prior, observed)) {
      throw StateError('Editor draft predecessor changed');
    }
    _draftPredecessor ??= observed;
    return (observed: observed, pending: true);
  }

  @override
  Future<EditorRecovery> observePresented(
    VersionedCommitReceipt receipt,
  ) async {
    final observation = await _inspectPresented(
      receipt,
      allowUnobservedAbsence: false,
      exactCurrentRevision: true,
    );
    return observation.observed!;
  }

  @override
  Future<void> acknowledgePresented(VersionedCommitReceipt receipt) async {
    final observation = await _inspectPresented(
      receipt,
      allowUnobservedAbsence: true,
      exactCurrentRevision: false,
    );
    if (observation.pending) {
      await owner.acknowledgeEditorRecovery(observation.observed!);
    }
  }

  @override
  Future<void> close() {
    if (_closing != null) return _closing!;
    _closed = true;
    return _closing = owner
        ._call(
          host.Action.closeCaptureScope,
          configure: (request) => request.captureScope = scope,
        )
        .then((_) {});
  }
}
