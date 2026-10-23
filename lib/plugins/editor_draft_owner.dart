import 'dart:async';
import 'package:file_selector/file_selector.dart';

import '../attachments/attachment.dart';
import '../main.dart' show Idea;
import 'editor_draft.dart';
import 'editor_draft_assets.dart';
import 'editor_draft_binding.dart';
import 'editor_draft_import.dart';
import 'editor_draft_import_session.dart';
import 'editor_draft_session.dart';
import 'editor_draft_workspace.dart';
import 'editor_session.dart';
import 'editor_draft_codec.dart';
import 'editor_draft_submission.dart';
import 'versioned_content.dart';

part 'editor_draft_commit.dart';

/// The native workbench owns this runtime. A dialog owns only a view lease.
abstract interface class WorkbenchEditorDraftRuntime {
  EditorDraftWorkspace get editorDraftWorkspace;
  EditorDraftOwner? retainedEditorDraft(String cardId);
  Future<EditorDraftOwner> createEditorDraftOwner({
    required WorkbenchEditorSession editor,
    required EditorDraftSnapshot initial,
    required List<IdeaAttachment> attachments,
    required bool create,
  });
  Future<EditorDraftClosePreparation> prepareEditorDraftClose();
  Future<IdeaAttachment> importEditorDraftFile(
    EditorDraftOwner owner, {
    required String path,
    required String name,
    required String kind,
  });
  Future<IdeaAttachment> importEditorDraftSource(
    EditorDraftOwner owner, {
    required XFile source,
    required String kind,
  });
  Future<void> finishEditorDraftClose(EditorDraftClosePreparation preparation);
  EditorDraftCommitAttempt beginEditorDraftCommit(
    EditorDraftOwner owner,
    Idea template,
  );
}

/// Holds complete local state, original operations, import decisions and the
/// capture session beyond the lifetime of a replaceable dialog. Host journals
/// remain authoritative. Nothing in a widget's dispose method retires these.
final class EditorDraftOwner {
  EditorDraftOwner({
    required this.workspace,
    required EditorDraftControl control,
    required this.editor,
    required EditorDraftAssetCatalog catalog,
    required EditorDraftSnapshot initial,
    required this.isCurrent,
    required this.operationFactory,
    this.baseline,
  }) : _catalog = catalog {
    final identity = editor;
    if (identity is! WorkbenchEditorOperationIdentity) {
      throw UnsupportedError('Original editor operation identity is required');
    }
    session = EditorDraftSession.newSession(
      control: control,
      cardId: catalog.cardId,
      draftId: catalog.draftId,
      sourceRevision: catalog.sourceRevision,
      sourceKind: catalog.sourceKind,
      predecessorOperation: catalog.predecessorOperation,
      predecessorDigest: catalog.predecessorDigest,
      commitOperation:
          (identity as WorkbenchEditorOperationIdentity).commitOperation,
      initialSnapshot: EditorDraftSnapshot(
        values: initial.values,
        assets: initial.assets,
        captureScope: captureIdentity.scope,
        captureEvents: captureIdentity.events,
      ),
      operationFactory: operationFactory,
      isCurrent: isCurrent,
    );
    session.addListener(_settlePins);
    workspace.retain(session);
  }

  final EditorDraftWorkspace workspace;
  final bool Function() isCurrent;
  final String Function() operationFactory;
  final WorkbenchEditorSession editor;
  final Idea? baseline;
  late final EditorDraftSession session;
  EditorDraftAssetCatalog _catalog;
  EditorDraftRecord? _adopted;
  List<EditorDraftAssetView>? _adoptedViews;
  final _imports = <EditorDraftImportSession>[];
  final _pendingSelections = <IdeaAttachment>[];
  bool _importAdmitted = false;
  bool _importing = false;
  Object? _assetFailure;
  EditorDraftImportSession? _pendingImport;
  EditorDraftRecord? _pendingImportParent;
  Future<EditorDraftAssetView> Function(EditorDraftImportRecord)?
  _pendingExport;
  Future<void>? _close;
  bool _captureClosed = false;
  PasteInsertion? _pasteIntent;
  bool _pasteBusy = false;
  Object? _pasteFailure;
  EditorDraftBinding? _view;
  EditorDraftCommitAttempt? _commit;
  EditorDraftCommitAttempt? _incoming;
  bool _retired = false;
  EditorDraftRetirementReservation? _retirement;
  Future<void>? _retirementFinish;

  EditorDraftRetirementReservation reserveRetirement(
    EditorDraftRecord original,
    String operation,
  ) {
    if (commitFrozen ||
        _retired ||
        captureClosed ||
        _close != null ||
        importing ||
        hasUnresolvedImports ||
        !captureIdentity.complete ||
        _pasteIntent != null ||
        !isCurrent() ||
        workspace.preparingClose ||
        workspace.disposed ||
        workspace.hasView(session) ||
        _view?.attached == true ||
        !identical(workspace.find(session.cardId, session.draftId), session)) {
      throw StateError('Original editor resources cannot begin retirement');
    }
    return _retirement = session.reserveRetirement(original, operation);
  }

  Future<void> finishRetirement(
    EditorDraftRetirementReservation reservation,
    EditorDraftRecord retired,
  ) {
    if (!identical(_retirement, reservation)) {
      throw StateError('Retirement belongs to another original editor');
    }
    return _retirementFinish ??= Future<void>.sync(() async {
      workspace.releaseRetiredDraft(session, reservation, retired);
      _retired = true;
      // The same close future owns an ambiguous reply. Inspection never calls
      // close again or treats a disposed local cache as a capture close proof.
      await closeCaptureAfterDrafts();
    });
  }

  EditorDraftCommitAttempt? get commit => _commit;
  EditorDraftCommitAttempt? get pendingCommit =>
      _commit ?? (_incoming?.finished == false ? _incoming : null);
  bool get commitFrozen =>
      session.retirementFrozen ||
      _commit != null ||
      (_incoming != null && !_incoming!.finished);
  bool get retired => _retired;

  EditorDraftOwner._handoff({
    required this.workspace,
    required EditorDraftControl control,
    required this.editor,
    required EditorDraftAssetCatalog catalog,
    required EditorDraftRecord record,
    required List<EditorDraftAssetView> views,
    required this.isCurrent,
    required this.operationFactory,
    required this.baseline,
  }) : _catalog = catalog.afterConfirmedDraft(record, views) {
    session = EditorDraftSession.restore(
      control: control,
      record: record,
      operationFactory: operationFactory,
      isCurrent: isCurrent,
    );
    session.adoptConfirmedAssetPins(record);
    _adopted = record;
    _adoptedViews = List.unmodifiable(views);
    session.addListener(_settlePins);
    workspace.retain(session);
  }

  /// Attach only the freshly created resume child to its still-live original
  /// editor. Restoring persisted scope strings never creates this capability.
  EditorDraftOwner.resumed({
    required this.workspace,
    required EditorDraftControl control,
    required this.editor,
    required EditorDraftRecord record,
    required List<EditorDraftAssetView> views,
    required this.isCurrent,
    required this.operationFactory,
    required this.baseline,
  }) : _catalog = EditorDraftAssetCatalog.restore(
         record,
         views,
         promoteConfirmedPins: true,
       ) {
    final identity = editor;
    final capture = captureIdentity;
    if (record.resumeLink == null ||
        record.parentLink != null ||
        record.generation != BigInt.one ||
        record.currentGeneration != BigInt.one ||
        !record.active ||
        !record.currentActive ||
        identity is! WorkbenchEditorOperationIdentity ||
        (identity as WorkbenchEditorOperationIdentity).commitOperation !=
            record.request.commitOperation ||
        (identity as WorkbenchEditorOperationIdentity).sourceRevision !=
            record.request.sourceRevision ||
        editor.targetId != record.request.cardId ||
        !capture.complete ||
        capture.scope != record.request.captureScope ||
        capture.events != 0 ||
        (record.request.sourceKind == EditorDraftSourceKind.newCard
            ? baseline != null
            : baseline == null ||
                  baseline!.id != record.request.cardId ||
                  baseline!.contentRevision != record.request.sourceRevision)) {
      throw StateError(
        'Resume requires its original fresh editor and exact first receipt',
      );
    }
    session = EditorDraftSession.restore(
      control: control,
      record: record,
      operationFactory: operationFactory,
      isCurrent: isCurrent,
    );
    session.adoptConfirmedAssetPins(record);
    _adopted = record;
    _adoptedViews = List.unmodifiable(views);
    session.addListener(_settlePins);
    workspace.retain(session);
  }

  EditorDraftAssetCatalog get catalog => _catalog;
  bool get importing => _importAdmitted;
  bool get hasUnresolvedImports =>
      _assetFailure != null ||
      _imports.any((value) => value.unknown || value.busy);
  bool get captureClosed => _captureClosed;
  List<EditorDraftImportSession> get imports => List.unmodifiable(_imports);
  ({String scope, int events, bool complete}) get captureIdentity {
    final original = editor;
    final actual = original is WorkbenchEditorDraftCaptureIdentity
        ? (original as WorkbenchEditorDraftCaptureIdentity).draftCapture
        : (scope: '', events: 0, complete: true);
    return (
      scope: actual.scope,
      events: actual.events,
      complete: actual.complete && !_pasteBusy && _pasteFailure == null,
    );
  }

  void _settlePins() {
    if (session.retirementFrozen) return;
    final record = session.confirmed;
    if (session.saving ||
        session.unknown ||
        session.conflicted ||
        record == null ||
        identical(record, _adopted) ||
        _assetFailure != null) {
      return;
    }
    try {
      final views = _catalog.viewsForSelections(record.request.assets);
      final next = _catalog.afterConfirmedDraft(record, views);
      // Catalog and session adoption are synchronous; no view can observe a
      // half-promoted set. The receipt retains its original immutable origins.
      session.adoptConfirmedAssetPins(record);
      _catalog = next;
      _adopted = record;
      _adoptedViews = views;
    } catch (error) {
      _assetFailure = error;
      session.markCaptureIncomplete(error);
    }
  }

  EditorDraftMetadata metadata({
    required String category,
    required String stage,
    required List<IdeaAttachment> selected,
  }) {
    if (!isCurrent() ||
        _assetFailure != null ||
        _imports.any((value) => value.unknown)) {
      throw StateError('Complete draft attachment metadata is unavailable');
    }
    final complete = <IdeaAttachment>[
      ...selected,
      for (final attachment in _pendingSelections)
        if (!selected.contains(attachment)) attachment,
    ];
    final metadata = EditorDraftMetadata(
      category: category,
      stage: stage,
      assets: _catalog.selectionsFor(complete),
      captureScope: captureIdentity.scope,
      captureEvents: captureIdentity.events,
      incomplete: _importing || !captureIdentity.complete
          ? StateError('Original import or capture is still unresolved')
          : null,
    );
    // An import belongs to the root before the route receives its result. A
    // detached route cannot silently drop a Ready selection. Once a view has
    // observed it, subsequent explicit selection changes may remove it.
    _pendingSelections.removeWhere(selected.contains);
    return metadata;
  }

  EditorDraftViewLease attach(EditorDraftBinding binding) {
    if (session.retirementFrozen) {
      throw StateError('Original draft retirement blocks new views');
    }
    if (!identical(binding.session, session)) {
      throw StateError('Draft binding belongs to another owner');
    }
    binding.attach();
    try {
      final lease = workspace.attach(session, capture: binding.capture);
      if (!binding.capture()) {
        lease.release();
        throw StateError('Complete draft view capture failed');
      }
      _view = binding;
      return lease;
    } catch (_) {
      binding.detach();
      rethrow;
    }
  }

  EditorDraftTextValue _field(EditorDraftValues values, String field) =>
      switch (field) {
        'title' => values.title,
        'description' => values.description,
        'hypothesis' => values.hypothesis,
        'conclusion' => values.conclusion,
        'todos' => values.todos,
        _ => throw const FormatException('Unknown draft field'),
      };

  /// Retain the original paste intent before dispatch. A view may disappear
  /// during the host call; its result still belongs to this root owner.
  Future<EditorDraftTextValue> recordPaste(PasteInsertion insertion) async {
    if (commitFrozen ||
        _pasteIntent != null ||
        !captureIdentity.complete ||
        importing ||
        hasUnresolvedImports ||
        !isCurrent() ||
        workspace.preparingClose) {
      throw StateError('Original draft paste is unavailable');
    }
    if (_field(session.current.values, insertion.field).text !=
        insertion.before) {
      throw StateError('Draft changed before paste admission');
    }
    _pasteIntent = insertion;
    _pasteBusy = true;
    final paused = session.autoSavePaused;
    session.pauseAutoSave();
    try {
      await editor.recordPaste(insertion);
      if (!isCurrent()) throw StateError('Original paste workspace changed');
      final values = session.current.values;
      if (_field(values, insertion.field).text != insertion.before) {
        throw StateError('Raw input changed during the original paste');
      }
      final offset =
          insertion.startUtf16 +
          insertion.after.length -
          (insertion.before.length -
              (insertion.endUtf16 - insertion.startUtf16));
      final applied = EditorDraftTextValue(
        text: insertion.after,
        selectionBase: offset,
        selectionExtent: offset,
        affinity: 1,
        directional: false,
        composingStart: -1,
        composingEnd: -1,
      );
      final actual = captureIdentity;
      final view = _view;
      if (view?.attached == true) {
        view!.applyPaste(insertion.field, insertion.before, applied);
      }
      _pasteBusy = false;
      session.observe(
        EditorDraftSnapshot(
          values: EditorDraftValues(
            title: insertion.field == 'title' ? applied : values.title,
            description: insertion.field == 'description'
                ? applied
                : values.description,
            hypothesis: insertion.field == 'hypothesis'
                ? applied
                : values.hypothesis,
            conclusion: insertion.field == 'conclusion'
                ? applied
                : values.conclusion,
            todos: insertion.field == 'todos' ? applied : values.todos,
            category: values.category,
            stage: values.stage,
          ),
          assets: session.current.assets,
          captureScope: actual.scope,
          captureEvents: actual.events,
        ),
      );
      if (view?.attached == true && !view!.capture()) {
        throw StateError('Complete pasted view could not be captured');
      }
      _pasteIntent = null;
      return applied;
    } catch (error) {
      _pasteFailure = error;
      session.markCaptureIncomplete(error);
      rethrow;
    } finally {
      _pasteBusy = false;
      if (!paused && isCurrent() && !session.disposed) session.resumeAutoSave();
    }
  }

  /// The immutable import request is kept by this owner even if the view or
  /// its source file disappears. Unknown is inspected explicitly, never retried
  /// by a timer. Only an exported, hash-verified Ready view enters the catalog.
  Future<EditorDraftAssetView> importAsset({
    required EditorDraftImportControl control,
    required Future<EditorDraftImportRequest> Function(EditorDraftRecord)
    prepare,
    required String selectedPath,
    required Future<EditorDraftAssetView> Function(EditorDraftImportRecord)
    export,
  }) async {
    if (commitFrozen ||
        _importAdmitted ||
        hasUnresolvedImports ||
        !isCurrent() ||
        workspace.preparingClose ||
        workspace.disposed) {
      throw StateError('Draft import is unavailable');
    }
    // Reserve before the first await; two imports must not share a generation.
    // Existing metadata remains capturable while the prerequisite save drains.
    _importAdmitted = true;
    final paused = session.autoSavePaused;
    session.pauseAutoSave();
    try {
      await session.ensureJournal();
      final current = (await session.flushLatestVisible(
        timeout: const Duration(seconds: 20),
      ))!;
      _importing = true;
      final request = await prepare(current);
      if (!isCurrent() ||
          request.cardId != session.cardId ||
          request.draftId != session.draftId ||
          request.expectedGeneration != current.generation) {
        throw StateError('Draft import original scope changed');
      }
      final importing = EditorDraftImportSession(
        control: control,
        request: request,
        isCurrent: isCurrent,
      );
      _imports.add(importing);
      _pendingImport = importing;
      _pendingImportParent = current;
      _pendingExport = export;
      await importing.begin();
      final reply = await importing.complete(selectedPath: selectedPath);
      final ready = reply.record;
      if (ready == null || ready.phase != EditorDraftImportPhase.ready) {
        throw StateError('Draft import has no confirmed bytes');
      }
      final view = await export(ready);
      _adoptReadyImport(current, ready, view);
      return view;
    } catch (error) {
      // Even a known Ready result can fail export or view validation. Preserve
      // that unresolved selection; ordinary text capture is not abandonment.
      _assetFailure = error;
      session.markCaptureIncomplete(error);
      rethrow;
    } finally {
      _importing = false;
      _importAdmitted = false;
      if (!paused && isCurrent() && !session.disposed) session.resumeAutoSave();
    }
  }

  void _adoptReadyImport(
    EditorDraftRecord parent,
    EditorDraftImportRecord ready,
    EditorDraftAssetView view,
  ) {
    if (!isCurrent() ||
        session.confirmed?.generation != parent.generation ||
        session.unknown ||
        session.saving ||
        session.conflicted) {
      throw StateError('Draft import original generation changed');
    }
    final catalog = _catalog.withReadyImport(parent, ready, view);
    final selected =
        catalog
            .viewsForSelections(session.current.assets)
            .map((value) => value.attachment)
            .toList()
          ..add(view.attachment);
    final complete = EditorDraftSnapshot(
      values: session.current.values,
      assets: catalog.selectionsFor(selected),
      captureScope: captureIdentity.scope,
      captureEvents: captureIdentity.events,
    );
    _catalog = catalog;
    _pendingSelections.add(view.attachment);
    _assetFailure = null;
    _pendingImport = null;
    _pendingImportParent = null;
    _pendingExport = null;
    if (captureIdentity.complete) {
      session.observe(complete);
    } else {
      session.observeIncomplete(
        complete,
        StateError('Original paste remains unresolved'),
      );
    }
  }

  /// Resolve only the retained original import and export its host-owned bytes.
  /// No source file is reopened and no old mutation is submitted again.
  Future<EditorDraftAssetView> inspectOriginalImport() async {
    final original = _pendingImport;
    final parent = _pendingImportParent;
    final export = _pendingExport;
    if (original == null ||
        parent == null ||
        export == null ||
        _importAdmitted ||
        commitFrozen ||
        !isCurrent() ||
        workspace.preparingClose ||
        workspace.disposed) {
      throw StateError('No original unresolved draft import can be inspected');
    }
    _importAdmitted = true;
    _importing = true;
    final paused = session.autoSavePaused;
    session.pauseAutoSave();
    try {
      final ready = await original.inspectReady();
      final view = await export(ready);
      _adoptReadyImport(parent, ready, view);
      return view;
    } catch (error) {
      _assetFailure = error;
      session.markCaptureIncomplete(error);
      rethrow;
    } finally {
      _importAdmitted = false;
      _importing = false;
      if (isCurrent() && !session.disposed) {
        if (_view?.attached == true) _view!.capture();
        if (!paused) session.resumeAutoSave();
      }
    }
  }

  /// Called only after the workspace's capture/flush/detach/finish sequence.
  Future<void> closeCaptureAfterDrafts() {
    if (_close case final original?) return original;
    if ((!workspace.disposed && !_retired) ||
        !session.disposed ||
        importing ||
        hasUnresolvedImports ||
        !captureIdentity.complete) {
      throw StateError('Draft owner still has unretired work');
    }
    return _close ??= editor.close().then<void>((_) {
      _captureClosed = true;
    });
  }
}
