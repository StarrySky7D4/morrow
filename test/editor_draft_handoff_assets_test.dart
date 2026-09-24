import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/attachments/attachment.dart';
import 'package:morrow_studio/media/texture_source.dart';
import 'package:morrow_studio/plugins/editor_draft.dart';
import 'package:morrow_studio/plugins/editor_draft_assets.dart';
import 'package:morrow_studio/plugins/editor_recovery.dart';
import 'package:morrow_studio/plugins/versioned_content.dart';

final _hash = List<int>.filled(32, 7);
final _commitHash = List<int>.filled(32, 8);
final _requestHash = List<int>.filled(32, 9);

IdeaAttachment _attachment(String id, String name) => IdeaAttachment.versioned(
  source: TextureSource(
    location: 'C:/preview/$id',
    name: name,
    kind: TextureKind.file,
    local: true,
  ),
  byteLength: BigInt.from(4),
  pluginId: id,
);

EditorDraftAssetView _view(
  IdeaAttachment attachment, {
  List<String> aliases = const [],
}) => EditorDraftAssetView(
  attachment: attachment,
  aliases: aliases,
  sha256: _hash,
  mediaType: 'application/octet-stream',
);

EditorDraftTextValue _text(String value) => EditorDraftTextValue(
  text: value,
  selectionBase: value.length,
  selectionExtent: value.length,
  affinity: 0,
  directional: false,
  composingStart: -1,
  composingEnd: -1,
);

EditorDraftValues _values({String title = 'raw parent'}) => EditorDraftValues(
  title: _text(title),
  description: _text('raw description'),
  hypothesis: _text(''),
  conclusion: _text(''),
  todos: _text(''),
  category: 'category',
  stage: 'stage',
);

EditorDraftAssetSelection _selection(
  EditorDraftAssetOrigin origin,
  String id, {
  List<String> aliases = const [],
}) => EditorDraftAssetSelection(origin: origin, assetId: id, aliases: aliases);

EditorDraftStoredAsset _stored(EditorDraftAssetSelection selected) =>
    EditorDraftStoredAsset(
      selection: selected,
      name: '${selected.assetId}.bin',
      mediaType: 'application/octet-stream',
      bytes: BigInt.from(4),
      sha256: _hash,
    );

VersionedContentRecord _source(BigInt revision, {bool deleted = false}) =>
    VersionedContentRecord(
      id: 'card',
      title: 'card',
      revision: revision,
      formatVersion: revision == BigInt.one ? 1 : 2,
      description: '',
      category: '',
      stage: '',
      hypothesis: '',
      conclusion: '',
      favorite: false,
      assets: const [],
      icon: 0,
      color: 0,
      deleted: deleted,
      deletedAt: BigInt.zero,
      todos: const [],
      completed: const [],
      tasks: const [],
      origin: null,
      retiredTaskIds: const [],
      projectedStage: '',
      completeCount: 0,
      incompleteCount: 0,
      ambiguousCount: 0,
    );

EditorDraftCommitEvidence _committed(
  BigInt sourceRevision,
  BigInt currentRevision,
) {
  if (sourceRevision == BigInt.zero) {
    // Structural new-card proof only; the current host does not expose an
    // EditorRecovery for a V1 first Create.
    return EditorDraftCommitEvidence(
      id: 'card',
      operation: 'commit-s1',
      digest: _commitHash,
      sourceRevision: sourceRevision,
      committedRevision: currentRevision,
    );
  }
  return EditorDraftCommitEvidence.fromRecovery(
    EditorRecovery(
      id: 'card',
      title: 'card',
      operation: 'commit-s1',
      digest: _commitHash,
      sourceRevision: sourceRevision,
      currentRevision: currentRevision,
      status: EditorRecoveryStatus.committed,
    ),
  );
}

EditorDraftRecord _parent({
  bool newCard = false,
  bool current = true,
  List<EditorDraftAssetSelection>? selections,
}) {
  final assets =
      selections ??
      [
        _selection(
          EditorDraftAssetOrigin.staged,
          'a',
          aliases: const ['original-a'],
        ),
        _selection(EditorDraftAssetOrigin.staged, 'b'),
      ];
  final request = EditorDraftWriteRequest(
    cardId: 'card',
    draftId: 'parent',
    operation: 'parent-save',
    expectedGeneration: BigInt.zero,
    sourceRevision: newCard ? BigInt.zero : BigInt.one,
    sourceKind: newCard
        ? EditorDraftSourceKind.newCard
        : EditorDraftSourceKind.existingCard,
    predecessorOperation: '',
    predecessorDigest: const [],
    values: _values(),
    assets: assets,
  );
  return EditorDraftRecord(
    request: request,
    generation: BigInt.one,
    active: true,
    currentGeneration: current ? BigInt.one : BigInt.two,
    currentActive: current,
    repeated: !current,
    sourceFormat: newCard ? 0 : 2,
    sourceRevision: request.sourceRevision,
    sourceSha256: newCard ? const [] : _hash,
    predecessorRevision: BigInt.zero,
    predecessorSha256: const [],
    assets: [for (final item in assets) _stored(item)],
    requestSha256: _requestHash,
  );
}

EditorDraftParentLink _link({List<int>? requestHash, List<int>? commitHash}) =>
    EditorDraftParentLink(
      parentDraftId: 'parent',
      parentGeneration: BigInt.one,
      parentSaveOperation: 'parent-save',
      parentRequestSha256: requestHash ?? _requestHash,
      committedOperation: 'commit-s1',
      committedSha256: commitHash ?? _commitHash,
      childOperation: 'child-save',
    );

EditorDraftAssetCatalog _catalog(
  EditorDraftRecord parent,
  List<EditorDraftAssetView> views, {
  VersionedContentRecord? source,
  EditorDraftCommitEvidence? committed,
  EditorDraftParentLink? link,
}) => EditorDraftAssetCatalog.handoff(
  parent: parent,
  parentLink: link ?? _link(),
  childDraftId: 'child',
  source: source ?? _source(parent.request.sourceRevision + BigInt.one),
  committed:
      committed ??
      _committed(
        parent.request.sourceRevision,
        parent.request.sourceRevision + BigInt.one,
      ),
  parentViews: views,
);

EditorDraftRecord _child(
  EditorDraftRecord parent,
  EditorDraftParentLink link,
  List<EditorDraftAssetSelection> selections, {
  String title = 'raw parent',
  String operation = 'child-save',
  bool linked = true,
}) {
  final request = EditorDraftWriteRequest(
    cardId: 'card',
    draftId: 'child',
    operation: operation,
    expectedGeneration: BigInt.zero,
    sourceRevision: parent.request.sourceRevision + BigInt.one,
    predecessorOperation: '',
    predecessorDigest: const [],
    values: _values(title: title),
    assets: selections,
  );
  return EditorDraftRecord(
    request: request,
    generation: BigInt.one,
    active: true,
    currentGeneration: BigInt.one,
    currentActive: true,
    repeated: false,
    sourceFormat: request.sourceRevision == BigInt.one ? 1 : 2,
    sourceRevision: request.sourceRevision,
    sourceSha256: _hash,
    predecessorRevision: BigInt.zero,
    predecessorSha256: const [],
    assets: [for (final item in selections) _stored(item)],
    requestSha256: _requestHash,
    parentLink: linked ? link : null,
  );
}

void main() {
  test('existing-card handoff keeps exact parent pins and view identity', () {
    final a = _attachment('a', 'a.bin');
    final b = _attachment('b', 'b.bin');
    final views = [
      _view(a, aliases: const ['original-a']),
      _view(b),
    ];
    final parent = _parent();
    final catalog = _catalog(parent, views);
    expect(catalog.cardId, 'card');
    expect(catalog.draftId, 'child');
    expect(catalog.sourceKind, EditorDraftSourceKind.existingCard);
    expect(catalog.sourceRevision, BigInt.two);
    expect(catalog.confirmedGeneration, BigInt.zero);
    final selections = catalog.selectionsFor([a, b]);
    expect(
      selections.map((item) => item.origin),
      everyElement(EditorDraftAssetOrigin.parentDraft),
    );
    expect(selections.first.aliases, ['original-a']);
    expect(() => catalog.selectionsFor([a]), throwsFormatException);
    expect(() => catalog.selectionsFor([b, a]), throwsFormatException);
    expect(
      () => catalog.selectionsFor([_attachment('a', 'a.bin'), b]),
      throwsFormatException,
    );

    final confirmed = _child(parent, _link(), selections);
    final next = catalog.afterConfirmedDraft(confirmed, views);
    expect(next.confirmedGeneration, BigInt.one);
    expect(
      next.selectionsFor([a, b]).map((s) => s.origin),
      everyElement(EditorDraftAssetOrigin.previousDraft),
    );
    expect(next.selectionsFor([a]).single.aliases, ['original-a']);
    expect(identical(views.first.attachment, a), isTrue);
  });

  test('V1 first Create is not mislabeled as EditorRecovery proof', () {
    expect(
      () => EditorDraftCommitEvidence.fromRecovery(
        EditorRecovery(
          id: 'card',
          title: 'card',
          operation: 'commit-s1',
          digest: _commitHash,
          sourceRevision: BigInt.zero,
          currentRevision: BigInt.one,
          status: EditorRecoveryStatus.committed,
        ),
      ),
      throwsFormatException,
    );
  });

  test('new-card parent first commit yields existing-card child baseline', () {
    final a = _attachment('a', 'a.bin');
    final b = _attachment('b', 'b.bin');
    final parent = _parent(newCard: true);
    final views = [
      _view(a, aliases: const ['original-a']),
      _view(b),
    ];
    final catalog = _catalog(parent, views);
    expect(catalog.sourceRevision, BigInt.one);
    expect(catalog.sourceKind, EditorDraftSourceKind.existingCard);
    final selected = catalog.selectionsFor([a, b]);
    final next = catalog.afterConfirmedDraft(
      _child(parent, _link(), selected),
      views,
    );
    expect(
      next.selectionsFor([b]).single.origin,
      EditorDraftAssetOrigin.previousDraft,
    );
  });

  test('later child save drops unreselected pins permanently', () {
    final a = _attachment('a', 'a.bin');
    final b = _attachment('b', 'b.bin');
    final views = [
      _view(a, aliases: const ['original-a']),
      _view(b),
    ];
    final parent = _parent();
    final first = _catalog(parent, views);
    final selected = first.selectionsFor([a, b]);
    final second = first.afterConfirmedDraft(
      _child(parent, _link(), selected),
      views,
    );
    final kept = second.selectionsFor([a]);
    final later = EditorDraftRecord(
      request: EditorDraftWriteRequest(
        cardId: 'card',
        draftId: 'child',
        operation: 'child-update',
        expectedGeneration: BigInt.one,
        sourceRevision: BigInt.two,
        predecessorOperation: '',
        predecessorDigest: const [],
        values: _values(title: 'later edit'),
        assets: kept,
      ),
      generation: BigInt.two,
      active: true,
      currentGeneration: BigInt.two,
      currentActive: true,
      repeated: false,
      sourceFormat: 2,
      sourceRevision: BigInt.two,
      sourceSha256: _hash,
      predecessorRevision: BigInt.zero,
      predecessorSha256: const [],
      assets: [_stored(kept.single)],
      requestSha256: _requestHash,
      parentLink: _link(),
    );
    final third = second.afterConfirmedDraft(later, [views.first]);
    expect(
      third.selectionsFor([a]).single.origin,
      EditorDraftAssetOrigin.previousDraft,
    );
    expect(() => third.selectionsFor([b]), throwsFormatException);
  });

  test('handoff rejects stale parent and changed source proof', () {
    final a = _attachment('a', 'a.bin');
    final b = _attachment('b', 'b.bin');
    final views = [
      _view(a, aliases: const ['original-a']),
      _view(b),
    ];
    expect(
      () => _catalog(_parent(current: false), views),
      throwsFormatException,
    );
    expect(
      () => _catalog(
        _parent(),
        views,
        link: _link(requestHash: List<int>.filled(32, 1)),
      ),
      throwsFormatException,
    );
    expect(
      () => _catalog(
        _parent(),
        views,
        committed: _committed(BigInt.two, BigInt.from(3)),
      ),
      throwsFormatException,
    );
    expect(
      () => _catalog(_parent(), views, source: _source(BigInt.from(3))),
      throwsFormatException,
    );
    expect(
      () => _catalog(_parent(), [views.last, views.first]),
      throwsFormatException,
    );
    expect(
      () => EditorDraftAssetCatalog.handoff(
        parent: _parent(),
        parentLink: _link(),
        childDraftId: 'bad/child',
        source: _source(BigInt.two),
        committed: _committed(BigInt.one, BigInt.two),
        parentViews: views,
      ),
      throwsFormatException,
    );
  });

  test('first child receipt must prove complete unchanged snapshot', () {
    final a = _attachment('a', 'a.bin');
    final b = _attachment('b', 'b.bin');
    final views = [
      _view(a, aliases: const ['original-a']),
      _view(b),
    ];
    final parent = _parent();
    final catalog = _catalog(parent, views);
    final selected = catalog.selectionsFor([a, b]);
    expect(
      () => catalog.afterConfirmedDraft(
        _child(parent, _link(), selected, title: 'changed'),
        views,
      ),
      throwsFormatException,
    );
    expect(
      () => catalog.afterConfirmedDraft(
        _child(parent, _link(), selected, linked: false),
        views,
      ),
      throwsFormatException,
    );
    expect(
      () => catalog.afterConfirmedDraft(
        _child(parent, _link(), selected, operation: 'other-save'),
        views,
      ),
      throwsFormatException,
    );
    expect(
      () => catalog.afterConfirmedDraft(
        _child(parent, _link(), selected.sublist(0, 1)),
        [views.first],
      ),
      throwsFormatException,
    );
  });
}
