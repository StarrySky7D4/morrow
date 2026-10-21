part of 'editor_draft_owner.dart';

/// Native services retain the new capture before any proposal is submitted.
/// None of these callbacks retries an old business or journal operation.
final class EditorDraftCommitServices {
  const EditorDraftCommitServices({
    required this.drafts,
    required this.proposals,
    required this.inspectCommit,
    required this.readSource,
    required this.openSuccessor,
    required this.presentSource,
    required this.retain,
    required this.acknowledge,
  });
  final EditorDraftControl drafts;
  final EditorDraftHandoffProposalControl proposals;
  final Future<EditorDraftCommitEvidence> Function(WorkbenchEditorSession)
  inspectCommit;
  final Future<VersionedContentRecord> Function(String) readSource;
  final Future<WorkbenchEditorSession> Function(Idea) openSuccessor;
  final Future<Idea> Function(VersionedContentRecord) presentSource;
  final void Function(EditorDraftOwner) retain;
  final Future<void> Function(WorkbenchEditorSession, EditorDraftCommitEvidence)
  acknowledge;
}

/// One original S1 and one immutable successor proposal. The workbench owner
/// retains this object, including errors and newly opened captures, if its view
/// disappears. A failed stage never dispatches itself again.
final class EditorDraftCommitAttempt {
  EditorDraftCommitAttempt._(this.parent, this.services, this.template);
  final EditorDraftOwner parent;
  final EditorDraftCommitServices services;
  final Idea template;
  Future<EditorDraftOwner>? _starting;
  Future<void>? _finishing;
  EditorDraftRecord? frozenParent;
  EditorDraftSnapshot? submittedSnapshot;
  List<EditorDraftAssetView>? submittedViews;
  Idea? confirmed;
  EditorDraftCommitEvidence? evidence;
  WorkbenchEditorSession? successorEditor;
  EditorDraftHandoffProposal? proposal;
  EditorDraftHandoffProposalRecord? proposalRecord;
  EditorDraftOwner? child;
  EditorDraftAssetCatalog? _childCatalog;
  Future<EditorDraftOwner>? _inspection;
  Object? failure;
  String stage = 'parent';
  bool _transferred = false;
  int? _transferredParentGeneration;
  bool _finished = false;
  bool get finished => _finished;
  bool get viewTransferred => _transferred;

  void _current() {
    if (!parent.isCurrent() ||
        parent.workspace.preparingClose ||
        parent.workspace.disposed ||
        parent.session.disposed) {
      throw StateError('Original editor commit workspace changed');
    }
  }

  void _captureParent() {
    _current();
    final view = parent._view;
    if (view?.attached == true && !view!.capture()) {
      throw StateError('Latest parent view is incomplete');
    }
    if (parent.session.captureBlocked ||
        parent.importing ||
        parent.hasUnresolvedImports ||
        !parent.captureIdentity.complete) {
      throw StateError('Original editor has unresolved input');
    }
  }

  Future<EditorDraftOwner> start() =>
      _starting ??= _start().catchError((Object error) {
        failure = error;
        throw error;
      });

  Future<EditorDraftOwner> _start() async {
    _captureParent();
    parent.session.pauseAutoSave();
    await parent.session.ensureJournal();
    _captureParent();
    final record = (await parent.session.flushLatestVisible(
      timeout: const Duration(seconds: 20),
    ))!;
    _captureParent();
    if (parent.session.dirty ||
        !identical(record, parent._adopted) ||
        record.request.commitOperation !=
            (parent.editor as WorkbenchEditorOperationIdentity)
                .commitOperation) {
      throw StateError('Original parent is not completely durable');
    }
    frozenParent = record;
    submittedSnapshot = parent.session.current;
    submittedViews = List.unmodifiable(parent._adoptedViews!);
    final values = record.request.values;
    final taskLines = values.todos.text
        .split('\n')
        .map((v) => v.trim())
        .where((v) => v.isNotEmpty)
        .toSet()
        .toList();
    final draft = Idea(
      values.title.text.trim(),
      values.description.text.trim().isEmpty
          ? '从一个小小的念头开始。'
          : values.description.text.trim(),
      values.category,
      template.icon,
      template.color,
      id: template.id,
      stage: values.stage,
      hypothesis: values.hypothesis.text.trim(),
      conclusion: values.conclusion.text.trim(),
      todos: taskLines,
      completed: template.completed.intersection(taskLines.toSet()),
      favorite: template.favorite,
      time: template.time,
      contentOwner: template.contentOwner,
      contentRevision: template.contentRevision,
      versioned: template.versioned,
      attachments: [for (final view in submittedViews!) view.attachment],
    );
    if (draft.id != parent.session.cardId || draft.title.isEmpty) {
      throw const FormatException('Business save needs this card and a title');
    }
    stage = 'business';
    confirmed = await parent.editor.save(
      draft,
      EditorFields(
        title: values.title.text,
        description: values.description.text,
        hypothesis: values.hypothesis.text,
        conclusion: values.conclusion.text,
        todos: values.todos.text,
      ),
    );
    return _afterBusiness();
  }

  Future<EditorDraftOwner> _afterBusiness() async {
    final record = frozenParent!;
    _current();
    stage = 'proof';
    final proof = await services.inspectCommit(parent.editor);
    evidence = proof;
    _current();
    if (proof.id != record.request.cardId ||
        proof.operation != record.request.commitOperation ||
        proof.sourceRevision != parent.session.sourceRevision ||
        proof.committedRevision != proof.sourceRevision + BigInt.one ||
        confirmed!.contentRevision != proof.committedRevision ||
        proof.digest.length != 32) {
      throw StateError('Original business commit proof changed');
    }
    final source = await services.readSource(proof.id);
    _current();
    if (source.revision != proof.committedRevision || source.deleted) {
      throw StateError('Committed source is no longer current');
    }
    stage = 'successor';
    final editor = await services.openSuccessor(confirmed!);
    // Keep this original capture even if the next validation fails.
    successorEditor = editor;
    _current();
    if (editor is! WorkbenchEditorOperationIdentity ||
        editor is! WorkbenchEditorDraftCaptureIdentity ||
        editor.targetId != proof.id ||
        (editor as WorkbenchEditorOperationIdentity).sourceRevision !=
            source.revision) {
      throw StateError('Successor original capture does not match S1');
    }
    final capture =
        (editor as WorkbenchEditorDraftCaptureIdentity).draftCapture;
    if (!capture.complete || capture.scope.isEmpty || capture.events != 0) {
      throw StateError('Successor capture is not fresh');
    }
    final childId = parent.operationFactory();
    final operation = parent.operationFactory();
    final link = EditorDraftParentLink.fromParentRecord(
      parent: record,
      committedOperation: proof.operation,
      committedSha256: proof.digest,
      childOperation: operation,
    );
    final catalog = EditorDraftAssetCatalog.handoff(
      parent: record,
      parentLink: link,
      childDraftId: childId,
      source: source,
      committed: proof,
      parentViews: submittedViews!,
    );
    _childCatalog = catalog;
    final request = EditorDraftWriteRequest(
      cardId: proof.id,
      draftId: childId,
      operation: operation,
      expectedGeneration: BigInt.zero,
      sourceRevision: source.revision,
      sourceKind: EditorDraftSourceKind.existingCard,
      predecessorOperation: '',
      predecessorDigest: const [],
      commitOperation:
          (editor as WorkbenchEditorOperationIdentity).commitOperation,
      values: record.request.values,
      assets: catalog.selectionsFor([
        for (final v in submittedViews!) v.attachment,
      ]),
      captureScope: capture.scope,
      captureEvents: capture.events,
    );
    final intent = EditorDraftHandoffProposal(
      handoff: EditorDraftHandoffRequest(request: request, parentLink: link),
      retirementOperation: parent.operationFactory(),
    );
    proposal = intent;
    stage = 'prepare';
    proposalRecord = await services.proposals.prepare(intent);
    _checkProposal(proposalRecord!);
    _current();
    if (proposalRecord!.summary.status !=
        EditorDraftHandoffProposalStatus.pending) {
      throw StateError('Fresh successor proposal is not pending');
    }
    return _completeProposal();
  }

  Future<EditorDraftOwner> _completeProposal() async {
    final intent = proposal!;
    stage = 'complete';
    proposalRecord = await services.proposals.complete(
      cardId: intent.handoff.request.cardId,
      parentDraftId: intent.handoff.parentLink.parentDraftId,
      childOperation: intent.handoff.request.operation,
    );
    _checkProposal(proposalRecord!);
    _current();
    if (proposalRecord!.summary.status !=
        EditorDraftHandoffProposalStatus.childCommitted) {
      throw StateError('Original successor was not committed');
    }
    return _adoptChild();
  }

  Future<EditorDraftOwner> _adoptChild() async {
    final request = proposal!.handoff.request;
    final stored = await services.drafts.read(request.cardId, request.draftId);
    _current();
    if (stored == null ||
        stored.generation != BigInt.one ||
        !stored.active ||
        !stored.currentActive ||
        stored.currentGeneration != stored.generation ||
        !_equal(
          EditorDraftCodec.encodeWrite(stored.request),
          EditorDraftCodec.encodeWrite(request),
        )) {
      throw StateError('Original first child receipt changed');
    }
    final next = EditorDraftOwner._handoff(
      workspace: parent.workspace,
      control: services.drafts,
      editor: successorEditor!,
      catalog: _childCatalog!,
      record: stored,
      views: submittedViews!,
      isCurrent: parent.isCurrent,
      operationFactory: parent.operationFactory,
      baseline: confirmed,
    );
    child = next;
    next._incoming = this;
    services.retain(next);
    next.session.pauseAutoSave();
    stage = 'child';
    _copyLatest();
    await next.session.flushLatestVisible(timeout: const Duration(seconds: 20));
    _current();
    // Re-capture after the await. New input remains dirty in the retained child.
    _copyLatest();
    return next;
  }

  /// Explicit recovery of a lost reply. It observes the original operation;
  /// only a later, never-dispatched stage may run after that exact observation.
  /// Missing or ambiguous observations leave the original attempt frozen.
  Future<EditorDraftOwner> inspectAndContinue() {
    if (failure == null || _finished) {
      throw StateError('No failed original commit to inspect');
    }
    return _inspection ??= _inspectAndContinue()
        .catchError((Object error) {
          failure = error;
          throw error;
        })
        .whenComplete(() {
          _inspection = null;
        });
  }

  Future<EditorDraftOwner> _inspectAndContinue() async {
    _current();
    final failedStage = stage;
    if (failedStage == 'parent') {
      if (parent.session.unknown) await parent.session.inspectPending();
      if (parent.session.unknown || parent.session.conflicted) {
        throw StateError('Original parent journal remains unresolved');
      }
      failure = null;
      return _start();
    }
    if (failedStage == 'business' || failedStage == 'proof') {
      final proof = await services.inspectCommit(parent.editor);
      final source = await services.readSource(proof.id);
      _current();
      if (proof.id != frozenParent!.request.cardId ||
          proof.operation != frozenParent!.request.commitOperation ||
          proof.sourceRevision != parent.session.sourceRevision ||
          source.revision != proof.committedRevision ||
          source.deleted) {
        throw StateError('Original business result is not currently provable');
      }
      confirmed = await services.presentSource(source);
      _current();
      failure = null;
      return _afterBusiness();
    }
    if (failedStage == 'prepare' || failedStage == 'complete') {
      final observed = await inspectProposal();
      if (observed == null ||
          observed.summary.status ==
              EditorDraftHandoffProposalStatus.conflict ||
          observed.summary.status ==
              EditorDraftHandoffProposalStatus.cancelled) {
        throw StateError('Original successor remains unresolved');
      }
      proposalRecord = observed;
      if (failedStage == 'prepare' &&
          observed.summary.status == EditorDraftHandoffProposalStatus.pending) {
        failure = null;
        return _completeProposal();
      }
      if (observed.summary.status !=
          EditorDraftHandoffProposalStatus.childCommitted) {
        throw StateError('Original child result is not confirmed');
      }
      failure = null;
      return _adoptChild();
    }
    if (failedStage == 'child' && child != null) {
      final next = child!;
      if (next.session.unknown) await next.session.inspectPending();
      if (next.session.unknown || next.session.conflicted) {
        throw StateError('Latest child original remains unresolved');
      }
      if (!_transferred) _copyLatest();
      await next.session.flushLatestVisible(
        timeout: const Duration(seconds: 20),
      );
      if (!_transferred) _copyLatest();
      failure = null;
      if (_transferred) _finishing = null;
      return next;
    }
    if (failedStage == 'retire' && _transferred && child != null) {
      final observed = await inspectProposal();
      if (observed == null ||
          observed.summary.status !=
              EditorDraftHandoffProposalStatus.parentRetired ||
          observed.summary.parentActive ||
          !observed.summary.childActive) {
        throw StateError('Original parent retirement remains unresolved');
      }
      proposalRecord = observed;
      if (child!.session.unknown) await child!.session.inspectPending();
      if (child!.session.unknown) {
        throw StateError('Latest S2 remains unresolved');
      }
      failure = null;
      _finishing = null;
      return child!;
    }
    throw StateError(
      'This original stage requires separate explicit resolution: $failedStage',
    );
  }

  static bool _equal(List<int> a, List<int> b) =>
      a.length == b.length &&
      Iterable<int>.generate(a.length).every((i) => a[i] == b[i]);

  void _checkProposal(EditorDraftHandoffProposalRecord observed) {
    final intent = proposal!;
    if (!_equal(
          EditorDraftCodec.encodeHandoffProposal(intent),
          EditorDraftCodec.encodeHandoffProposal(observed.proposal),
        ) ||
        observed.summary.cardId != intent.handoff.request.cardId ||
        observed.summary.parentDraftId !=
            intent.handoff.parentLink.parentDraftId ||
        observed.summary.childDraftId != intent.handoff.request.draftId ||
        observed.summary.childOperation != intent.handoff.request.operation ||
        observed.summary.retirementOperation != intent.retirementOperation) {
      throw StateError('Original successor proposal changed');
    }
  }

  /// Reads the exact original proposal after a lost reply. No branch here
  /// executes business work, completes a child, retires a parent or clears S1.
  Future<EditorDraftHandoffProposalRecord?> inspectProposal() async {
    _current();
    final intent = proposal;
    if (intent == null) throw StateError('No original proposal to inspect');
    final observed = await services.proposals.inspect(
      cardId: intent.handoff.request.cardId,
      parentDraftId: intent.handoff.parentLink.parentDraftId,
      childOperation: intent.handoff.request.operation,
    );
    _current();
    if (observed != null) _checkProposal(observed);
    return observed;
  }

  void _copyLatest() {
    _captureParent();
    if (_transferred) throw StateError('Parent view already transferred');
    final next = child!;
    final latest = parent.session.current;
    final selected = parent.catalog.viewsForSelections(latest.assets);
    final capture = next.captureIdentity;
    next.session.observe(
      EditorDraftSnapshot(
        values: latest.values,
        assets: next.catalog.selectionsFor([
          for (final v in selected) v.attachment,
        ]),
        captureScope: capture.scope,
        captureEvents: capture.events,
      ),
    );
  }

  /// The view transition is synchronous, between two retained owners. It must
  /// detach the parent's lease before attaching the child's binding.
  void transferView(void Function(EditorDraftOwner child) transfer) {
    if (_transferred || failure != null || child == null) {
      throw StateError('Original successor is not ready for a view');
    }
    _copyLatest();
    final generation = parent.session.localGeneration;
    transfer(child!);
    _current();
    if (parent.workspace.hasView(parent.session) ||
        generation != parent.session.localGeneration) {
      throw StateError('Parent changed during view transfer');
    }
    _transferred = true;
    _transferredParentGeneration = generation;
  }

  bool get hasSuccessorChanges {
    final next = child;
    final original = submittedSnapshot;
    if (next == null || original == null) return true;
    final valuesEqual = EditorDraftSnapshot(
      values: next.session.current.values,
      assets: const [],
    ).sameAs(EditorDraftSnapshot(values: original.values, assets: const []));
    final selected = next.catalog.viewsForSelections(
      next.session.current.assets,
    );
    return !valuesEqual ||
        selected.length != submittedViews!.length ||
        Iterable<int>.generate(selected.length).any(
          (i) =>
              !identical(selected[i].attachment, submittedViews![i].attachment),
        );
  }

  Future<void> finish() => _finishing ??= _finish().catchError((Object error) {
    failure = error;
    throw error;
  });

  Future<void> _finish() async {
    _current();
    if (!_transferred ||
        failure != null ||
        child == null ||
        parent.session.localGeneration != _transferredParentGeneration) {
      throw StateError('Original view transfer is incomplete');
    }
    final next = child!;
    void captureChild() {
      final view = next._view;
      if (view?.attached == true && !view!.capture()) {
        throw StateError('Successor view is incomplete');
      }
      if (!next.captureIdentity.complete ||
          next.hasUnresolvedImports ||
          next.importing ||
          parent.session.localGeneration != _transferredParentGeneration) {
        throw StateError('Successor changed during retirement');
      }
    }

    captureChild();
    await next.session.flushLatestVisible(timeout: const Duration(seconds: 20));
    _current();
    captureChild();
    if (next.session.dirty) throw StateError('Latest S2 is not durable');
    stage = 'retire';
    final intent = proposal!;
    final retired =
        proposalRecord!.summary.status ==
            EditorDraftHandoffProposalStatus.parentRetired
        ? proposalRecord!
        : await services.proposals.retire(
            cardId: intent.handoff.request.cardId,
            parentDraftId: intent.handoff.parentLink.parentDraftId,
            childOperation: intent.handoff.request.operation,
          );
    _checkProposal(retired);
    proposalRecord = retired;
    _current();
    if (retired.summary.status !=
            EditorDraftHandoffProposalStatus.parentRetired ||
        retired.summary.parentActive ||
        !retired.summary.childActive) {
      throw StateError('Original parent retirement is not confirmed');
    }
    // Input arriving while retirement was in flight still belongs to the child.
    captureChild();
    await next.session.flushLatestVisible(timeout: const Duration(seconds: 20));
    _current();
    captureChild();
    if (next.session.dirty) {
      throw StateError('Latest S2 changed before S1 acknowledgement');
    }
    stage = 'acknowledge';
    await services.acknowledge(parent.editor, evidence!);
    _current();
    parent.workspace.releaseRetiredParent(
      parent.session,
      next.session,
      retired,
      expectedLocalGeneration: _transferredParentGeneration!,
    );
    parent._retired = true;
    stage = 'close';
    await parent.closeCaptureAfterDrafts();
    _finished = true;
    stage = 'finished';
    next.session.resumeAutoSave();
  }
}

extension EditorDraftOwnerCommit on EditorDraftOwner {
  EditorDraftCommitAttempt beginCommit(
    Idea template,
    EditorDraftCommitServices services,
  ) {
    if (commitFrozen ||
        workspace.preparingClose ||
        !isCurrent() ||
        session.disposed ||
        importing ||
        hasUnresolvedImports ||
        !captureIdentity.complete) {
      throw StateError('Original draft commit is unavailable');
    }
    final attempt = EditorDraftCommitAttempt._(this, services, template);
    _commit = attempt;
    return attempt;
  }
}
