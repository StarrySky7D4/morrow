import 'dart:collection';

import '../attachments/attachment.dart';
import '../media/texture_source.dart';
import 'editor_draft.dart';
import 'editor_draft_codec.dart';
import 'editor_draft_import.dart';
import 'editor_recovery.dart';
import 'editor_commit_proof.dart';
export 'editor_commit_proof.dart' show EditorDraftCommitEvidence;
import 'versioned_content.dart';

/// A view explicitly paired with host metadata. A local path or pluginId alone
/// never proves which source owns an attachment.
final class EditorDraftAssetView {
  EditorDraftAssetView({
    required this.attachment,
    required List<String> aliases,
    List<int>? sha256,
    this.mediaType,
  }) : aliases = List.unmodifiable(aliases),
       sha256 = sha256 == null ? null : List.unmodifiable(sha256);

  final IdeaAttachment attachment;
  final List<String> aliases;
  final List<int>? sha256;
  final String? mediaType;
}

final class _BoundAsset {
  const _BoundAsset(this.view, this.selection);
  final EditorDraftAssetView view;
  final EditorDraftAssetSelection selection;
}

bool _bytesEqual(List<int> a, List<int> b) {
  if (a.length != b.length) return false;
  for (var i = 0; i < a.length; i++) {
    if (a[i] != b[i]) return false;
  }
  return true;
}

bool _aliasesEqual(List<String> a, List<String> b) {
  if (a.length != b.length) return false;
  for (var i = 0; i < a.length; i++) {
    if (a[i] != b[i]) return false;
  }
  return true;
}

bool _selectionEqual(
  EditorDraftAssetSelection a,
  EditorDraftAssetSelection b,
) =>
    a.origin == b.origin &&
    a.assetId == b.assetId &&
    _aliasesEqual(a.aliases, b.aliases);
bool _textEqual(EditorDraftTextValue a, EditorDraftTextValue b) =>
    a.text == b.text &&
    a.selectionBase == b.selectionBase &&
    a.selectionExtent == b.selectionExtent &&
    a.affinity == b.affinity &&
    a.directional == b.directional &&
    a.composingStart == b.composingStart &&
    a.composingEnd == b.composingEnd;

bool _valuesEqual(EditorDraftValues a, EditorDraftValues b) =>
    _textEqual(a.title, b.title) &&
    _textEqual(a.description, b.description) &&
    _textEqual(a.hypothesis, b.hypothesis) &&
    _textEqual(a.conclusion, b.conclusion) &&
    _textEqual(a.todos, b.todos) &&
    a.category == b.category &&
    a.stage == b.stage;

bool _linkEqual(EditorDraftParentLink a, EditorDraftParentLink b) =>
    a.parentDraftId == b.parentDraftId &&
    a.parentGeneration == b.parentGeneration &&
    a.parentSaveOperation == b.parentSaveOperation &&
    _bytesEqual(a.parentRequestSha256, b.parentRequestSha256) &&
    a.committedOperation == b.committedOperation &&
    _bytesEqual(a.committedSha256, b.committedSha256) &&
    a.childOperation == b.childOperation;

bool _mediaMatches(
  TextureKind kind,
  String mediaType, {
  bool allowUnrecognized = false,
}) {
  final bare = mediaType.split(';').first.trim().toLowerCase();
  if (!bare.contains('/') || bare.startsWith('/') || bare.endsWith('/')) {
    return false;
  }
  if (bare == 'image/gif') return kind == TextureKind.gif;
  if (bare.startsWith('image/')) return kind == TextureKind.image;
  if (bare.startsWith('video/')) return kind == TextureKind.video;
  if (bare.startsWith('audio/')) return kind == TextureKind.audio;
  if (bare.startsWith('text/') || bare.startsWith('application/')) {
    return kind == TextureKind.file;
  }
  return allowUnrecognized;
}

void _checkView(
  EditorDraftAssetView view,
  String id,
  String name,
  String kind,
  BigInt bytes, {
  List<int>? sha256,
  bool isMediaType = false,
}) {
  final attachment = view.attachment;
  if (id.isEmpty ||
      attachment.pluginId != id ||
      attachment.source.name != name ||
      (isMediaType
          ? ((view.mediaType != null && view.mediaType != kind) ||
                !_mediaMatches(
                  attachment.source.kind,
                  kind,
                  allowUnrecognized: view.mediaType == kind,
                ))
          : attachment.source.kind.name != kind) ||
      attachment.byteLength != bytes ||
      view.aliases.length > 8 ||
      view.aliases.any((alias) => alias.isEmpty) ||
      (sha256 != null &&
          (sha256.length != 32 ||
              view.sha256 == null ||
              !_bytesEqual(view.sha256!, sha256)))) {
    throw const FormatException('Draft asset view differs from host metadata');
  }
}

/// A bounded, immutable inventory of explicit source bindings for one draft.
/// It is a UI projection of host-verified records, never a write grant.
final class EditorDraftAssetCatalog {
  EditorDraftAssetCatalog._(
    this.cardId,
    this.draftId,
    this.sourceKind,
    this.sourceRevision,
    this.predecessorOperation,
    List<int> predecessorDigest,
    this.confirmedGeneration,
    List<_BoundAsset> entries, {
    this._handoffLink,
    this._handoffParentRequest,
  }) : predecessorDigest = List.unmodifiable(predecessorDigest),
       _entries = List.unmodifiable(entries);

  final String cardId, draftId, predecessorOperation;
  final EditorDraftSourceKind sourceKind;
  final BigInt sourceRevision, confirmedGeneration;
  final List<int> predecessorDigest;
  final List<_BoundAsset> _entries;
  final EditorDraftParentLink? _handoffLink;
  final EditorDraftWriteRequest? _handoffParentRequest;

  static List<_BoundAsset> _versioned(
    VersionedContentRecord record,
    List<EditorDraftAssetView> views,
    EditorDraftAssetOrigin origin,
  ) {
    if (record.deleted || views.length != record.assets.length) {
      throw const FormatException('Versioned source asset count changed');
    }
    return [
      for (var i = 0; i < views.length; i++)
        (() {
          final asset = record.assets[i];
          final view = views[i];
          _checkView(view, asset.id, asset.name, asset.kind, asset.bytes);
          return _BoundAsset(
            view,
            EditorDraftAssetSelection(
              origin: origin,
              assetId: asset.id,
              aliases: view.aliases,
            ),
          );
        })(),
    ];
  }

  static List<_BoundAsset> _stored(
    EditorDraftRecord record,
    List<EditorDraftAssetView> views, {
    required bool promote,
  }) {
    if (!record.active ||
        !record.currentActive ||
        record.generation != record.currentGeneration ||
        views.length != record.assets.length ||
        record.assets.length != record.request.assets.length) {
      throw const FormatException('Draft asset generation is not current');
    }
    return [
      for (var i = 0; i < views.length; i++)
        (() {
          final stored = record.assets[i];
          final selected = record.request.assets[i];
          final view = views[i];
          if (!_selectionEqual(stored.selection, selected) ||
              !_aliasesEqual(view.aliases, selected.aliases)) {
            throw const FormatException('Stored draft asset selection changed');
          }
          _checkView(
            view,
            selected.assetId,
            stored.name,
            stored.mediaType,
            stored.bytes,
            sha256: stored.sha256,
            isMediaType: true,
          );
          return _BoundAsset(
            view,
            promote
                ? EditorDraftAssetSelection(
                    origin: EditorDraftAssetOrigin.previousDraft,
                    assetId: selected.assetId,
                    aliases: selected.aliases,
                  )
                : selected,
          );
        })(),
    ];
  }

  static List<_BoundAsset> _ready(
    String cardId,
    String draftId,
    EditorDraftRecord? previous,
    List<EditorDraftImportRecord> imports,
    List<EditorDraftAssetView> views,
  ) {
    if (imports.length != views.length ||
        (imports.isNotEmpty && previous == null)) {
      throw const FormatException('Ready draft import has no bound journal');
    }
    return [
      for (var i = 0; i < imports.length; i++)
        (() {
          final record = imports[i];
          final request = record.request;
          final view = views[i];
          if (request.cardId != cardId ||
              request.draftId != draftId ||
              request.expectedGeneration > previous!.generation ||
              record.currentGeneration != previous.generation ||
              !record.mainActive ||
              !record.currentActive ||
              !record.bytesRetained ||
              record.phase != EditorDraftImportPhase.ready) {
            throw const FormatException(
              'Draft import is not ready for this generation',
            );
          }
          _checkView(
            view,
            record.assetId,
            request.name,
            request.kind,
            request.bytes,
            sha256: request.sha256,
          );
          return _BoundAsset(
            view,
            EditorDraftAssetSelection(
              origin: EditorDraftAssetOrigin.staged,
              assetId: record.assetId,
              aliases: view.aliases,
            ),
          );
        })(),
    ];
  }

  factory EditorDraftAssetCatalog.existingCard({
    required String cardId,
    required String draftId,
    required VersionedContentRecord source,
    required List<EditorDraftAssetView> sourceViews,
    VersionedContentRecord? predecessor,
    EditorRecovery? predecessorEvidence,
    List<EditorDraftAssetView> predecessorViews = const [],
    EditorDraftRecord? previousDraft,
    List<EditorDraftAssetView> previousViews = const [],
    List<EditorDraftImportRecord> readyImports = const [],
    List<EditorDraftAssetView> readyViews = const [],
  }) {
    if (cardId.isEmpty ||
        draftId.isEmpty ||
        source.id != cardId ||
        source.revision <= BigInt.zero ||
        source.deleted ||
        (predecessor == null) != (predecessorEvidence == null)) {
      throw const FormatException('Draft source identity changed');
    }
    var predecessorOperation = '';
    List<int> predecessorDigest = const [];
    if (predecessor != null) {
      final evidence = predecessorEvidence!;
      if (predecessor.id != cardId ||
          predecessor.deleted ||
          predecessor.revision != source.revision + BigInt.one ||
          evidence.id != cardId ||
          evidence.sourceRevision != source.revision ||
          evidence.currentRevision != predecessor.revision ||
          evidence.status != EditorRecoveryStatus.committed ||
          evidence.operation.isEmpty ||
          evidence.digest.length != 32) {
        throw const FormatException('Draft predecessor evidence changed');
      }
      predecessorOperation = evidence.operation;
      predecessorDigest = evidence.digest;
    } else if (predecessorViews.isNotEmpty) {
      throw const FormatException('Unexpected predecessor views');
    }
    _checkPrevious(
      previousDraft,
      cardId,
      draftId,
      EditorDraftSourceKind.existingCard,
      source.revision,
      predecessorOperation,
      predecessorDigest,
      previousViews,
    );
    return EditorDraftAssetCatalog._(
      cardId,
      draftId,
      EditorDraftSourceKind.existingCard,
      source.revision,
      predecessorOperation,
      predecessorDigest,
      previousDraft?.generation ?? BigInt.zero,
      [
        ..._versioned(source, sourceViews, EditorDraftAssetOrigin.source).where(
          (entry) => !previousViews.any(
            (view) => identical(view.attachment, entry.view.attachment),
          ),
        ),
        if (predecessor != null)
          ..._versioned(
            predecessor,
            predecessorViews,
            EditorDraftAssetOrigin.predecessor,
          ).where(
            (entry) => !previousViews.any(
              (view) => identical(view.attachment, entry.view.attachment),
            ),
          ),
        if (previousDraft != null)
          ..._stored(previousDraft, previousViews, promote: true),
        ..._ready(cardId, draftId, previousDraft, readyImports, readyViews),
      ],
    );
  }

  factory EditorDraftAssetCatalog.newCard({
    required String cardId,
    required String draftId,
    EditorDraftRecord? previousDraft,
    List<EditorDraftAssetView> previousViews = const [],
    List<EditorDraftImportRecord> readyImports = const [],
    List<EditorDraftAssetView> readyViews = const [],
  }) {
    if (cardId.isEmpty || draftId.isEmpty) {
      throw const FormatException('Missing new-card draft identity');
    }
    _checkPrevious(
      previousDraft,
      cardId,
      draftId,
      EditorDraftSourceKind.newCard,
      BigInt.zero,
      '',
      const [],
      previousViews,
    );
    return EditorDraftAssetCatalog._(
      cardId,
      draftId,
      EditorDraftSourceKind.newCard,
      BigInt.zero,
      '',
      const [],
      previousDraft?.generation ?? BigInt.zero,
      [
        if (previousDraft != null)
          ..._stored(previousDraft, previousViews, promote: true),
        ..._ready(cardId, draftId, previousDraft, readyImports, readyViews),
      ],
    );
  }

  static void _checkPrevious(
    EditorDraftRecord? previous,
    String cardId,
    String draftId,
    EditorDraftSourceKind kind,
    BigInt sourceRevision,
    String predecessorOperation,
    List<int> predecessorDigest,
    List<EditorDraftAssetView> views,
  ) {
    if (previous == null) {
      if (views.isNotEmpty) {
        throw const FormatException('Draft views have no confirmed generation');
      }
      return;
    }
    final request = previous.request;
    if (request.cardId != cardId ||
        request.draftId != draftId ||
        request.sourceKind != kind ||
        request.sourceRevision != sourceRevision ||
        request.predecessorOperation != predecessorOperation ||
        !_bytesEqual(request.predecessorDigest, predecessorDigest)) {
      throw const FormatException(
        'Confirmed draft belongs to another baseline',
      );
    }
  }

  /// Cross-draft handoff from one exact current parent save and one verified
  /// committed business source. The first child save must carry every parent
  /// pin in order; no source-card asset is inferred from a path or plugin ID.
  factory EditorDraftAssetCatalog.handoff({
    required EditorDraftRecord parent,
    required EditorDraftParentLink parentLink,
    required String childDraftId,
    required VersionedContentRecord source,
    required EditorDraftCommitEvidence committed,
    required List<EditorDraftAssetView> parentViews,
  }) {
    final request = parent.request;
    EditorDraftCodec.validateHandoffProposalIdentity(
      request.cardId,
      request.draftId,
      request.operation,
    );
    EditorDraftCodec.validateHandoffProposalIdentity(
      request.cardId,
      childDraftId,
      parentLink.childOperation,
    );
    EditorDraftCodec.validateHandoffProposalIdentity(
      request.cardId,
      request.draftId,
      parentLink.committedOperation,
    );
    final hasPredecessor = request.predecessorOperation.isNotEmpty;
    final sameCommit =
        hasPredecessor &&
        request.predecessorOperation == parentLink.committedOperation;
    final effectiveRevision =
        request.sourceKind == EditorDraftSourceKind.newCard
        ? BigInt.zero
        : (hasPredecessor && !sameCommit
              ? parent.predecessorRevision
              : request.sourceRevision);
    if (request.cardId.isEmpty ||
        request.draftId.isEmpty ||
        childDraftId.isEmpty ||
        childDraftId == request.draftId ||
        request.operation.isEmpty ||
        parentLink.childOperation.isEmpty ||
        source.id != request.cardId ||
        source.deleted ||
        source.formatVersion < 1 ||
        source.formatVersion > 2 ||
        source.revision != effectiveRevision + BigInt.one ||
        (request.sourceKind == EditorDraftSourceKind.newCard &&
            (request.sourceRevision != BigInt.zero ||
                hasPredecessor ||
                parent.sourceFormat != 0 ||
                source.formatVersion != 1)) ||
        (request.sourceKind == EditorDraftSourceKind.existingCard &&
            (request.sourceRevision <= BigInt.zero ||
                parent.sourceFormat < 1 ||
                parent.sourceFormat > 2)) ||
        parent.sourceRevision != request.sourceRevision ||
        (hasPredecessor &&
            (parent.predecessorRevision !=
                    request.sourceRevision + BigInt.one ||
                parent.predecessorSha256.length != 32)) ||
        (!hasPredecessor &&
            (parent.predecessorRevision != BigInt.zero ||
                parent.predecessorSha256.isNotEmpty)) ||
        !parent.active ||
        !parent.currentActive ||
        parent.generation != parent.currentGeneration ||
        parent.generation != request.expectedGeneration + BigInt.one ||
        parent.requestSha256.length != 32 ||
        parentLink.parentDraftId != request.draftId ||
        parentLink.parentGeneration != parent.generation ||
        parentLink.parentSaveOperation != request.operation ||
        parentLink.parentRequestSha256.length != 32 ||
        !_bytesEqual(parentLink.parentRequestSha256, parent.requestSha256) ||
        parentLink.committedSha256.length != 32 ||
        parentLink.committedOperation.isEmpty ||
        parentLink.committedOperation == parentLink.childOperation ||
        parentLink.committedOperation == request.operation ||
        (sameCommit &&
            !_bytesEqual(
              request.predecessorDigest,
              parentLink.committedSha256,
            )) ||
        committed.id != request.cardId ||
        committed.operation != parentLink.committedOperation ||
        committed.digest.length != 32 ||
        !_bytesEqual(committed.digest, parentLink.committedSha256) ||
        committed.sourceRevision != effectiveRevision ||
        committed.committedRevision != source.revision) {
      throw const FormatException('Draft handoff source proof changed');
    }
    final pinned = _stored(parent, parentViews, promote: false);
    return EditorDraftAssetCatalog._(
      request.cardId,
      childDraftId,
      EditorDraftSourceKind.existingCard,
      source.revision,
      '',
      const [],
      BigInt.zero,
      [
        for (final entry in pinned)
          _BoundAsset(
            entry.view,
            EditorDraftAssetSelection(
              origin: EditorDraftAssetOrigin.parentDraft,
              assetId: entry.selection.assetId,
              aliases: entry.selection.aliases,
            ),
          ),
      ],
      handoffLink: parentLink,
      handoffParentRequest: request,
    );
  }

  /// Restore the exact original selections from a current host journal. Every
  /// view must be supplied in stored order with a measured SHA-256 digest.
  factory EditorDraftAssetCatalog.restore(
    EditorDraftRecord record,
    List<EditorDraftAssetView> views,
  ) {
    final request = record.request;
    return EditorDraftAssetCatalog._(
      request.cardId,
      request.draftId,
      request.sourceKind,
      request.sourceRevision,
      request.predecessorOperation,
      request.predecessorDigest,
      record.generation,
      _stored(record, views, promote: false),
    );
  }

  /// Convert the exact selected view objects, in UI order, to complete host
  /// selections. Neither an unregistered pluginId nor a matching local path
  /// can introduce an attachment. One asset ID cannot be selected twice.
  List<EditorDraftAssetSelection> selectionsFor(List<IdeaAttachment> selected) {
    if (selected.length > 20) {
      throw const FormatException('Too many selected draft assets');
    }
    final byView = HashMap<IdeaAttachment, List<_BoundAsset>>.identity();
    for (final entry in _entries) {
      (byView[entry.view.attachment] ??= []).add(entry);
    }
    final usedIds = <String>{};
    final usedViews = HashSet<IdeaAttachment>.identity();
    final result = <EditorDraftAssetSelection>[];
    if (_handoffLink != null && confirmedGeneration == BigInt.zero) {
      if (selected.length != _entries.length) {
        throw const FormatException(
          'First child save must inherit every parent pin',
        );
      }
      for (var i = 0; i < selected.length; i++) {
        if (!identical(selected[i], _entries[i].view.attachment)) {
          throw const FormatException('First child pin order changed');
        }
      }
    }
    for (final attachment in selected) {
      final bound = byView[attachment];
      if (bound == null ||
          bound.length != 1 ||
          !usedViews.add(attachment) ||
          !usedIds.add(bound.single.selection.assetId)) {
        throw const FormatException(
          'Unknown or ambiguous draft attachment source',
        );
      }
      final selection = bound.single.selection;
      result.add(
        EditorDraftAssetSelection(
          origin: selection.origin,
          assetId: selection.assetId,
          aliases: selection.aliases,
        ),
      );
    }
    return List<EditorDraftAssetSelection>.unmodifiable(result);
  }

  /// Once a save has an exact confirmed record, its pinned assets may serve
  /// as previousDraft inputs for the same card and draft at the next generation.
  /// This deliberately cannot inherit pins from another draft ID.
  EditorDraftAssetCatalog afterConfirmedDraft(
    EditorDraftRecord record,
    List<EditorDraftAssetView> views,
  ) {
    final request = record.request;
    if (request.cardId != cardId ||
        request.draftId != draftId ||
        request.sourceKind != sourceKind ||
        request.sourceRevision != sourceRevision ||
        request.predecessorOperation != predecessorOperation ||
        !_bytesEqual(request.predecessorDigest, predecessorDigest) ||
        request.expectedGeneration != confirmedGeneration ||
        record.generation != confirmedGeneration + BigInt.one ||
        views.length != record.assets.length) {
      throw const FormatException('Confirmed draft changed asset baseline');
    }
    final selected = selectionsFor([for (final view in views) view.attachment]);
    if (selected.length != request.assets.length) {
      throw const FormatException('Confirmed draft dropped an attachment');
    }
    for (var i = 0; i < selected.length; i++) {
      if (!_selectionEqual(selected[i], request.assets[i])) {
        throw const FormatException('Confirmed draft changed asset selection');
      }
    }
    final link = _handoffLink;
    if (link != null) {
      final parentRequest = _handoffParentRequest!;
      if (record.generation != BigInt.one ||
          !record.active ||
          !record.currentActive ||
          record.currentGeneration != record.generation ||
          record.requestSha256.length != 32 ||
          request.operation != link.childOperation ||
          record.parentLink == null ||
          !_linkEqual(record.parentLink!, link) ||
          !_valuesEqual(request.values, parentRequest.values) ||
          request.assets.length != parentRequest.assets.length ||
          request.assets.any(
            (asset) => asset.origin != EditorDraftAssetOrigin.parentDraft,
          )) {
        throw const FormatException('First child save changed parent snapshot');
      }
      return EditorDraftAssetCatalog._(
        cardId,
        draftId,
        sourceKind,
        sourceRevision,
        predecessorOperation,
        predecessorDigest,
        record.generation,
        _stored(record, views, promote: true),
      );
    }
    return EditorDraftAssetCatalog._(
      cardId,
      draftId,
      sourceKind,
      sourceRevision,
      predecessorOperation,
      predecessorDigest,
      record.generation,
      [
        ..._entries.where(
          (entry) =>
              (entry.selection.origin == EditorDraftAssetOrigin.source ||
                  entry.selection.origin ==
                      EditorDraftAssetOrigin.predecessor ||
                  entry.selection.origin == EditorDraftAssetOrigin.staged) &&
              !views.any(
                (view) => identical(view.attachment, entry.view.attachment),
              ) &&
              !(entry.selection.origin == EditorDraftAssetOrigin.staged &&
                  selected.any(
                    (item) => item.assetId == entry.selection.assetId,
                  )),
        ),
        ..._stored(record, views, promote: true),
      ],
    );
  }
}
