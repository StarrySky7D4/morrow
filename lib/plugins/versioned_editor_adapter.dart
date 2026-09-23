import 'dart:async';
import 'dart:collection';
import 'dart:convert';

import '../attachments/attachment.dart';
import '../main.dart' show Idea;
import 'editor_session.dart';
import 'editor_recovery.dart';
import 'studio_backend.dart';
import 'versioned_content.dart';
import 'versioned_editor.dart';
import 'versioned_idea_view.dart';
import 'workbench_backend.dart';

/// A confirmed V2 commit whose current record could not be presented.
final class VersionedEditorPresentationFailure
    extends WorkbenchCommittedRefreshFailure {
  const VersionedEditorPresentationFailure(this.receipt, Object cause)
    : super(cause);

  final VersionedCommitReceipt receipt;
}

/// Adapts the legacy dialog shape at its boundary without submitting a V1
/// mutation or converting TaskIds into legacy checklist text.
final class VersionedWorkbenchEditorAdapter
    implements
        WorkbenchEditorSession,
        WorkbenchEditorContinuation,
        EditorDraftPredecessorSource {
  VersionedWorkbenchEditorAdapter(
    this._session,
    this._source,
    this._present, {
    this.isCurrent,
    this.reopen,
  }) {
    if (_source.formatVersion != 2 ||
        _source.deleted ||
        _session.targetId != _source.id) {
      throw const FormatException('Invalid versioned editor baseline');
    }
  }

  final VersionedEditorSession _session;
  final VersionedIdeaView _source;
  final Future<Idea> Function(VersionedContentRecord) _present;
  final bool Function()? isCurrent;
  final Future<WorkbenchEditorSession> Function(Idea confirmed)? reopen;
  bool get _active => !_closed && !_continued && (isCurrent?.call() ?? true);
  final Map<IdeaAttachment, Future<VersionedAsset>> _staged =
      HashMap<IdeaAttachment, Future<VersionedAsset>>.identity();

  String? _intent;
  _FrozenEdit? _frozen;
  Future<Idea>? _inFlight;
  Future<WorkbenchEditorSession>? _continuation;
  Future<void>? _closing;
  ({String id, BigInt revision, String operation})? _accepted;
  bool _stageStarted = false;
  bool _closed = false;
  bool _continued = false;

  @override
  String get targetId => _session.targetId;

  @override
  EditorRecovery? get draftPredecessor {
    final accepted = _accepted;
    if (accepted == null || _session is! EditorDraftPredecessorSource) {
      return null;
    }
    final observed =
        (_session as EditorDraftPredecessorSource).draftPredecessor;
    return observed != null &&
            observed.id == accepted.id &&
            observed.operation == accepted.operation &&
            observed.digest.length == 32 &&
            observed.sourceRevision == _source.revision &&
            observed.status == EditorRecoveryStatus.committed
        ? observed
        : null;
  }

  @override
  StudioBackend get studio => _session.studio;

  @override
  Future<void> recordPaste(PasteInsertion insertion) =>
      _session.recordPaste(insertion);

  @override
  Future<void> close() {
    if (_closing != null) return _closing!;
    _closed = true;
    return _closing = _session.close();
  }

  String _signature(Idea draft, EditorFields editor) => jsonEncode([
    draft.id,
    draft.versioned?.id,
    draft.versioned?.revision.toString(),
    draft.contentRevision?.toString(),
    draft.title,
    draft.description,
    draft.category,
    draft.stage,
    draft.favorite,
    Idea.icons.indexOf(draft.icon),
    draft.color.toARGB32(),
    draft.hypothesis,
    draft.conclusion,
    draft.todos,
    draft.completed.toList()..sort(),
    [
      for (final attachment in draft.attachments)
        [
          attachment.pluginId,
          attachment.source.location,
          attachment.source.name,
          attachment.source.kind.name,
          attachment.source.local,
          attachment.byteLength.toString(),
        ],
    ],
    editor.title,
    editor.description,
    editor.hypothesis,
    editor.conclusion,
    editor.todos,
  ]);

  _FrozenEdit _prepare(Idea draft, EditorFields editor) {
    if (draft.id != _source.id ||
        draft.versioned?.id != _source.id ||
        draft.versioned?.formatVersion != 2 ||
        draft.versioned?.revision != _source.revision ||
        (draft.contentRevision != null &&
            draft.contentRevision != _source.revision) ||
        draft.category != _source.category ||
        draft.stage != _source.stage ||
        draft.favorite != _source.favorite ||
        draft.todos.isNotEmpty ||
        draft.completed.isNotEmpty ||
        editor.todos.isNotEmpty) {
      throw const FormatException('V2 editor source or task fields changed');
    }
    final icon = Idea.icons.indexOf(draft.icon);
    if (icon < 0 ||
        draft.title != editor.title.trim() ||
        draft.hypothesis != editor.hypothesis.trim() ||
        draft.conclusion != editor.conclusion.trim()) {
      throw const FormatException('V2 editor common fields changed');
    }
    final known = {for (final asset in _source.assets) asset.id: asset};
    final seenIds = <String>{};
    final seenLocal = HashSet<IdeaAttachment>.identity();
    for (final attachment in draft.attachments) {
      final id = attachment.pluginId;
      if (id == null) {
        if (!attachment.source.local || !seenLocal.add(attachment)) {
          throw const FormatException('Invalid selected local attachment');
        }
        continue;
      }
      final original = known[id];
      if (!seenIds.add(id) ||
          original == null ||
          original.name != attachment.source.name ||
          original.kind != attachment.source.kind.name ||
          original.bytes != attachment.byteLength) {
        throw const FormatException('Existing V2 attachment changed identity');
      }
    }
    return _FrozenEdit(
      title: draft.title,
      description: draft.description,
      hypothesis: draft.hypothesis,
      conclusion: draft.conclusion,
      icon: icon,
      color: draft.color.toARGB32(),
      attachments: List.unmodifiable(draft.attachments),
      editor: EditorFields(
        title: editor.title,
        description: editor.description,
        hypothesis: editor.hypothesis,
        conclusion: editor.conclusion,
        todos: editor.todos,
      ),
    );
  }

  @override
  Future<Idea> save(Idea draft, EditorFields editor) {
    if (_continuation != null) {
      return Future.error(StateError('The successor editor is opening'));
    }
    if (!_active) {
      return Future.error(StateError('The editor is no longer current'));
    }
    final signature = _signature(draft, editor);
    if (_intent != null && signature != _intent) {
      return Future.error(
        StateError('Retry the original pending V2 edit without changing it'),
      );
    }
    if (_intent == null) {
      try {
        _frozen = _prepare(draft, editor);
      } catch (error) {
        return Future.error(EditorPreparationException(error));
      }
      _intent = signature;
    }
    if (_inFlight != null) return _inFlight!;
    _accepted = null;
    return _inFlight = _saveFrozen(_frozen!).whenComplete(() {
      _inFlight = null;
    });
  }

  Future<VersionedAsset> _stageOnce(IdeaAttachment attachment) {
    final existing = _staged[attachment];
    if (existing != null) return existing;
    _stageStarted = true;
    final pending = _session.stageAttachment(attachment);
    _staged[attachment] = pending;
    unawaited(
      pending.then<void>(
        (_) {},
        onError: (Object _) {
          if (identical(_staged[attachment], pending)) {
            _staged.remove(attachment);
          }
        },
      ),
    );
    return pending;
  }

  String _normalizedDescription(
    String raw,
    List<({IdeaAttachment source, VersionedAsset asset})> staged,
  ) {
    var description = raw.trim();
    if (description.isEmpty) description = '从一个小小的念头开始。';
    for (final item in staged) {
      final source = item.source.source;
      final names = {
        source.location,
        source.name,
        Uri.encodeComponent(source.location),
        Uri.encodeComponent(source.name),
      }.toList()..sort((a, b) => b.length.compareTo(a.length));
      for (final name in names) {
        description = description.replaceAll(
          RegExp(
            'attachment:${RegExp.escape(name)}'
            r'(?=[\s)>]|$)',
          ),
          'attachment:${item.asset.id}',
        );
      }
    }
    return description;
  }

  Future<Idea> _saveFrozen(_FrozenEdit input) async {
    final assets = <VersionedAsset>[];
    final selectedIds = <String>{};
    final aliases = <({IdeaAttachment source, VersionedAsset asset})>[];
    try {
      // Await every selected stage before the first captured save. A successful
      // stage is retained across same-intent retries, including later failures.
      for (final attachment in input.attachments) {
        final id = attachment.pluginId;
        if (id != null) {
          if (!selectedIds.add(id)) {
            throw const FormatException('Duplicate selected V2 attachment');
          }
          assets.add(_source.assets.firstWhere((asset) => asset.id == id));
        } else {
          final staged = await _stageOnce(attachment);
          if (staged.id.isEmpty ||
              !selectedIds.add(staged.id) ||
              staged.name != attachment.source.name ||
              staged.kind != attachment.source.kind.name ||
              staged.bytes != attachment.byteLength) {
            throw const FormatException(
              'Staged V2 attachment metadata changed',
            );
          }
          aliases.add((source: attachment, asset: staged));
          assets.add(staged);
        }
      }
      if (!_active) throw StateError('The editor changed while staging');
      final description = _normalizedDescription(
        input.editor.description,
        aliases,
      );
      if (_normalizedDescription(input.description, aliases) != description) {
        throw const FormatException('Editor description changed');
      }
      final fields = CardEditFields(
        title: input.title,
        description: description,
        hypothesis: input.hypothesis,
        conclusion: input.conclusion,
        icon: input.icon,
        color: input.color,
        assets: assets,
      );
      if (!_active) throw StateError('The editor is no longer current');
      final result = await _session.save(fields, input.editor);
      if (result.current.id != _source.id ||
          result.current.formatVersion != 2 ||
          result.current.revision < result.receipt.revision) {
        throw VersionedEditorPresentationFailure(
          result.receipt,
          const FormatException('Versioned editor current record changed'),
        );
      }
      try {
        if (!_active) throw StateError('The editor is no longer current');
        final presented = await _present(result.current);
        if (!_active) throw StateError('The editor is no longer current');
        final shown = presented.versioned;
        if (presented.id != _source.id ||
            presented.historicalReceipt ||
            shown == null ||
            shown.id != _source.id ||
            shown.formatVersion != 2 ||
            shown.revision < result.current.revision ||
            shown.revision != presented.contentRevision ||
            shown.deleted != presented.contentDeleted) {
          throw const FormatException('Versioned editor presentation changed');
        }
        if (_session case VersionedEditorAcknowledgement confirmation) {
          await confirmation.acknowledgePresented(result.receipt);
          if (result.receipt.id == _source.id &&
              result.receipt.operation.isNotEmpty &&
              result.receipt.revision == _source.revision + BigInt.one &&
              shown.revision == result.receipt.revision &&
              !shown.deleted &&
              !presented.contentDeleted) {
            _accepted = (
              id: presented.id,
              revision: shown.revision,
              operation: result.receipt.operation,
            );
          }
        }
        return presented;
      } catch (error) {
        throw VersionedEditorPresentationFailure(result.receipt, error);
      }
    } on EditorPreparationException catch (error) {
      if (!_stageStarted) {
        _intent = null;
        _frozen = null;
        rethrow;
      }
      throw StateError('The staged V2 edit remains frozen: $error');
    }
  }

  bool _matchesAccepted(Idea confirmed) {
    final accepted = _accepted;
    final view = confirmed.versioned;
    return accepted != null &&
        confirmed.id == accepted.id &&
        confirmed.contentRevision == accepted.revision &&
        !confirmed.contentDeleted &&
        !confirmed.historicalReceipt &&
        view != null &&
        view.id == accepted.id &&
        view.formatVersion == 2 &&
        view.revision == accepted.revision &&
        !view.deleted;
  }

  @override
  Future<WorkbenchEditorSession> continueAfterCommit(Idea confirmed) {
    if (!_active) {
      return Future.error(StateError('The editor is no longer current'));
    }
    if (_inFlight != null || !_matchesAccepted(confirmed)) {
      return Future.error(
        StateError('The original edit is not confirmed at this revision'),
      );
    }
    if (reopen == null) {
      return Future.error(UnsupportedError('Editor continuation unavailable'));
    }
    final ongoing = _continuation;
    if (ongoing != null) return ongoing;
    final pending = Completer<WorkbenchEditorSession>();
    _continuation = pending.future;
    unawaited(
      _openSuccessor(confirmed).then(
        pending.complete,
        onError: (Object error, StackTrace stack) {
          _continuation = null;
          pending.completeError(error, stack);
        },
      ),
    );
    return pending.future;
  }

  Future<WorkbenchEditorSession> _openSuccessor(Idea confirmed) async {
    final successor = await reopen!(confirmed);
    if (!_active ||
        !_matchesAccepted(confirmed) ||
        successor.targetId != targetId ||
        identical(successor, this)) {
      if (!identical(successor, this)) {
        try {
          await successor.close();
        } catch (_) {
          // The original editor remains frozen and cannot be replaced.
        }
      }
      throw StateError('The successor editor is no longer current');
    }
    _continued = true;
    try {
      await close();
    } catch (_) {
      // The successor is valid; the old local session is already closed.
    }
    return successor;
  }
}

final class _FrozenEdit {
  const _FrozenEdit({
    required this.title,
    required this.description,
    required this.hypothesis,
    required this.conclusion,
    required this.icon,
    required this.color,
    required this.attachments,
    required this.editor,
  });

  final String title;
  final String description;
  final String hypothesis;
  final String conclusion;
  final int icon;
  final int color;
  final List<IdeaAttachment> attachments;
  final EditorFields editor;
}
