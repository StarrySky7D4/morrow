part of 'workbench_native.dart';

/// Shares the draft lane; a missing mutation reply never triggers a retry.
final class NativeEditorDraftResumeControl implements EditorDraftResumeControl {
  NativeEditorDraftResumeControl(this._owner);
  final RustWorkbench _owner;
  NativeEditorDraftControl get _draft =>
      _owner.editorDrafts as NativeEditorDraftControl;

  @override
  Future<EditorDraftRecord> resume(EditorDraftResumeRequest proposal) {
    final Uint8List body;
    try {
      body = EditorDraftCodec.encodeResume(proposal);
    } catch (error) {
      return Future.error(
        EditorDraftResumeFailure(
          request: proposal,
          outcomeUnknown: false,
          cause: error,
        ),
      );
    }
    return _draft._job(() async {
      final request = proposal.request;
      var token = '';
      var outcomeUnknown = false;
      try {
        final begin = await _owner._call(
          host.Action.beginEditorDraft,
          configure: (outer) {
            outer.operation = request.operation;
            outer.totalLength = body.length;
            outer.sha256 = Uint8List.fromList(sha256.convert(body).bytes);
          },
        );
        token = begin.transfer ?? '';
        if (token.isEmpty || begin.offset != 0) {
          throw const FormatException('Invalid draft handoff upload admission');
        }
        for (var offset = 0; offset < body.length;) {
          final end = (offset + NativeEditorDraftControl._chunkBytes).clamp(
            0,
            body.length,
          );
          final reply = await _owner._call(
            host.Action.appendEditorDraft,
            configure: (outer) {
              outer.transfer = token;
              outer.offset = offset;
              outer.payload = Uint8List.sublistView(body, offset, end);
            },
          );
          if (reply.transfer != token ||
              reply.offsetBigInt != BigInt.from(end)) {
            throw const FormatException(
              'Draft handoff upload receipt mismatch',
            );
          }
          offset = end;
        }
        outcomeUnknown = true;
        final reply = await _owner._call(
          host.Action.finishEditorDraftResume,
          configure: (outer) {
            outer.transfer = token;
            outer.operation = request.operation;
            _draft._identity(
              outer,
              request.cardId,
              request.draftId,
              BigInt.zero,
            );
          },
        );
        final (envelope, revision) = await _draft._download(reply);
        _draft._context(
          envelope,
          request.cardId,
          request.draftId,
          request.operation,
          BigInt.zero,
        );
        final record = _draft._record(envelope, revision);
        final link = record.resumeLink;
        if (!record.active ||
            record.generation != BigInt.one ||
            record.retirement != null ||
            link == null ||
            !RustWorkbench._same(
              EditorDraftCodec.encodeResume(
                EditorDraftResumeRequest(
                  request: record.request,
                  resumeLink: link,
                ),
              ),
              body,
            )) {
          throw const FormatException('Draft handoff receipt changed proposal');
        }
        return record;
      } catch (error) {
        await _draft._abort(token);
        throw EditorDraftResumeFailure(
          request: proposal,
          outcomeUnknown: outcomeUnknown,
          cause: error,
        );
      }
    });
  }

  @override
  Future<EditorDraftRecord> retireResumeParent(
    EditorDraftRetirementRequest proposal,
  ) {
    try {
      EditorDraftCodec.validateRetirement(proposal);
    } catch (error) {
      return Future.error(
        EditorDraftRetirementFailure(
          request: proposal,
          outcomeUnknown: false,
          cause: error,
        ),
      );
    }
    return _draft._job(() async {
      // After dispatch a readback failure cannot prove no commit.
      try {
        final (envelope, revision) = await _draft._requestEnvelope(
          host.Action.retireEditorDraftResumeParent,
          configure: (outer) {
            _draft._identity(
              outer,
              proposal.cardId,
              proposal.childDraftId,
              proposal.parentGeneration,
            );
            outer.name = proposal.parentDraftId;
            outer.kind = proposal.childOperation;
            outer.operation = proposal.operation;
          },
        );
        _draft._context(
          envelope,
          proposal.cardId,
          proposal.parentDraftId,
          proposal.operation,
          proposal.parentGeneration,
        );
        final record = _draft._record(envelope, revision);
        final marker = record.retirement;
        if (record.active ||
            record.currentActive ||
            record.generation != proposal.parentGeneration + BigInt.one ||
            record.currentGeneration != record.generation ||
            marker == null ||
            marker.childDraftId != proposal.childDraftId ||
            marker.childOperation != proposal.childOperation ||
            marker.operation != proposal.operation ||
            marker.parentGeneration != proposal.parentGeneration) {
          throw const FormatException(
            'Draft parent retirement receipt mismatch',
          );
        }
        return record;
      } catch (error) {
        throw EditorDraftRetirementFailure(
          request: proposal,
          outcomeUnknown: true,
          cause: error,
        );
      }
    });
  }
}

/// One original fresh scope, successor write and parent-retirement attempt.
/// Any interrupted stage is retained. Only a positively observed receipt can
/// advance to the next stage; a missing reply never causes a second write.
final class _NativeDraftResumeAttempt {
  _NativeDraftResumeAttempt(this.owner, this.original)
    : control = NativeEditorDraftResumeControl(owner);
  final RustWorkbench owner;
  final EditorDraftRecord original;
  final NativeEditorDraftResumeControl control;
  Future<EditorDraftOwner>? _flight;
  EditorDraftOwner? _adopted;
  WorkbenchEditorSession? _editor;
  Idea? _baseline;
  EditorDraftResumeRequest? _proposal;
  bool _started = false, _resumeDispatched = false, _retireDispatched = false;
  bool _ancestorDispatched = false;
  bool _openDispatched = false;

  bool get blocksClose =>
      _adopted == null &&
      (_flight != null ||
          _ancestorDispatched ||
          _openDispatched ||
          _resumeDispatched);

  bool get current =>
      owner._closingProcess == null &&
      owner._failure == null &&
      !owner.editorDraftWorkspace.disposed;
  void _current() {
    if (!current ||
        !owner.writable ||
        owner.editorDraftWorkspace.preparingClose) {
      throw StateError('Resume workspace changed');
    }
  }

  String _operation() =>
      'draft-resume-${DateTime.now().microsecondsSinceEpoch}-${owner._sequence++}';

  Future<EditorDraftOwner> run() {
    if (_adopted case final result?) return Future.value(result);
    return _flight ??= Future<EditorDraftOwner>.sync(() {
      _current();
      if (_started) return _inspect();
      _started = true;
      return _start();
    }).whenComplete(() => _flight = null);
  }

  EditorDraftRetirementRequest _retirement(EditorDraftRecord child) {
    final link = child.resumeLink!;
    return EditorDraftRetirementRequest(
      cardId: child.request.cardId,
      childDraftId: child.request.draftId,
      parentDraftId: link.parentDraftId,
      childOperation: link.parentBusinessOperation,
      parentGeneration: link.parentGeneration,
      operation: link.retirementOperation,
    );
  }

  void _verifyRetirement(EditorDraftRecord? parent, EditorDraftRecord child) {
    final link = child.resumeLink!;
    final marker = parent?.retirement;
    if (parent == null ||
        parent.active ||
        parent.currentActive ||
        parent.generation != link.parentGeneration + BigInt.one ||
        parent.currentGeneration != parent.generation ||
        !RustWorkbench._same(parent.requestSha256, link.parentRequestSha256) ||
        marker == null ||
        marker.childDraftId != child.request.draftId ||
        marker.childOperation != link.parentBusinessOperation ||
        marker.operation != link.retirementOperation ||
        marker.parentGeneration != link.parentGeneration) {
      throw StateError('Original resume parent retirement is not confirmed');
    }
  }

  Future<EditorDraftOwner> _start() async {
    final request = original.request;
    // Same-process drafts must use their retained owner. A recovery projection
    // cannot steal or close any live owner, including a sibling's capture.
    if (owner._draftOwners.any(
      (candidate) =>
          candidate.session.cardId == request.cardId &&
          !candidate.retired &&
          !candidate.captureClosed &&
          !candidate.session.disposed,
    )) {
      throw StateError('Use the existing original editor owner');
    }
    _sameReviewedDraft(
      original,
      await owner.editorDrafts.read(request.cardId, request.draftId),
    );
    _current();
    if (original.resumeLink case final link?) {
      final ancestor = await owner.editorDrafts.read(
        request.cardId,
        link.parentDraftId,
      );
      _current();
      if (ancestor?.currentActive == true) {
        // This exact retirement is recorded in the durable child. The host
        // resolves its original transaction before considering a conditional
        // write; no dead capture session is reopened.
        _ancestorDispatched = true;
        _verifyRetirement(
          await control.retireResumeParent(_retirement(original)),
          original,
        );
      } else {
        _verifyRetirement(ancestor, original);
      }
      _current();
    }
    final imports = await owner.editorDraftImports.reconcile(
      request.cardId,
      request.draftId,
    );
    _current();
    if (imports.records == null ||
        imports.records!.isNotEmpty ||
        imports.currentGeneration != original.generation ||
        !imports.mainActive) {
      throw StateError('Resolve the original draft imports before continuing');
    }
    if (request.sourceKind == EditorDraftSourceKind.newCard) {
      _openDispatched = true;
      _editor = await owner.openEditor(request.cardId, create: true);
    } else {
      final source = await owner.versionedContent.read(request.cardId);
      _current();
      if (source.revision != request.sourceRevision || source.deleted) {
        throw StateError('Business source changed; review the saved draft');
      }
      _baseline = await owner.workspaceRecord(source);
      _current();
      if (source.formatVersion == 1) {
        _openDispatched = true;
        _editor = await owner._openLegacyEditor(
          request.cardId,
          create: false,
          expectedRevision: source.revision,
        );
      } else {
        _openDispatched = true;
        final native = await owner.openVersionedEditor(
          request.cardId,
          expectedRevision: source.revision,
        );
        _editor = VersionedWorkbenchEditorAdapter(
          native,
          _baseline!.versioned!,
          owner.workspaceRecord,
          isCurrent: () => current,
          deferRecoveryAcknowledgement: true,
        );
      }
    }
    _current();
    final editor = _editor!;
    if (editor is! WorkbenchEditorOperationIdentity ||
        editor is! WorkbenchEditorDraftCaptureIdentity) {
      throw StateError('Fresh editor identity is unavailable');
    }
    final identity = editor as WorkbenchEditorOperationIdentity;
    final capture =
        (editor as WorkbenchEditorDraftCaptureIdentity).draftCapture;
    if (!capture.complete ||
        capture.events != 0 ||
        capture.scope.isEmpty ||
        capture.scope == request.captureScope ||
        identity.sourceRevision != request.sourceRevision) {
      throw StateError('Fresh editor capture does not match the saved source');
    }
    final proposal = EditorDraftResumeRequest(
      request: EditorDraftWriteRequest(
        cardId: request.cardId,
        draftId: _operation(),
        operation: request.commitOperation,
        expectedGeneration: BigInt.zero,
        sourceRevision: request.sourceRevision,
        sourceKind: request.sourceKind,
        commitOperation: identity.commitOperation,
        captureScope: capture.scope,
        captureEvents: 0,
        predecessorOperation: request.predecessorOperation,
        predecessorDigest: request.predecessorDigest,
        values: request.values,
        assets: [
          for (final asset in request.assets)
            EditorDraftAssetSelection(
              origin: EditorDraftAssetOrigin.parentDraft,
              assetId: asset.assetId,
              aliases: asset.aliases,
            ),
        ],
      ),
      resumeLink: EditorDraftResumeLink(
        parentDraftId: request.draftId,
        parentGeneration: original.generation,
        parentSaveOperation: request.operation,
        parentRequestSha256: original.requestSha256,
        parentBusinessOperation: request.commitOperation,
        retirementOperation: _operation(),
      ),
    );
    _proposal = proposal;
    _resumeDispatched = true;
    return _complete(await control.resume(proposal));
  }

  Future<EditorDraftOwner> _inspect() async {
    final proposal = _proposal;
    if (!_resumeDispatched || proposal == null) {
      // An earlier scope opening or cleanup is retained without replay. In
      // particular, an ambiguous ancestor retirement is never resubmitted.
      throw StateError(
        _ancestorDispatched
            ? 'Inspect the original ancestor retirement before reopening'
            : 'The original resume preparation is incomplete; review the saved draft',
      );
    }
    final child = await owner.editorDrafts.read(
      proposal.request.cardId,
      proposal.request.draftId,
    );
    _current();
    if (child == null) {
      throw StateError('The original resume result is not confirmed');
    }
    return _complete(child);
  }

  Future<EditorDraftOwner> _complete(EditorDraftRecord child) async {
    _current();
    final proposal = _proposal!;
    if (child.resumeLink == null ||
        !child.active ||
        !child.currentActive ||
        child.generation != BigInt.one ||
        child.currentGeneration != BigInt.one ||
        !RustWorkbench._same(
          EditorDraftCodec.encodeResume(
            EditorDraftResumeRequest(
              request: child.request,
              resumeLink: child.resumeLink!,
            ),
          ),
          EditorDraftCodec.encodeResume(proposal),
        )) {
      throw StateError('Original fresh successor receipt changed');
    }
    var parent = await owner.editorDrafts.read(
      original.request.cardId,
      original.request.draftId,
    );
    _current();
    if (parent?.currentActive == true && !_retireDispatched) {
      _retireDispatched = true;
      parent = await control.retireResumeParent(_retirement(child));
      _current();
    }
    _verifyRetirement(parent, child);
    final fresh = await _inspectEditorDraft(
      owner,
      EditorDraftSummary(
        cardId: child.request.cardId,
        draftId: child.request.draftId,
        generation: child.generation,
        active: true,
      ),
    );
    _current();
    final result = EditorDraftOwner.resumed(
      workspace: owner.editorDraftWorkspace,
      control: owner.editorDrafts,
      editor: _editor!,
      record: fresh.record,
      views: fresh.views,
      isCurrent: () => current,
      operationFactory: _operation,
      baseline: _baseline,
    );
    owner._draftOwners.add(result);
    return _adopted = result;
  }
}
