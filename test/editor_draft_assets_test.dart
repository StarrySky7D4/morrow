import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/attachments/attachment.dart';
import 'package:morrow_studio/media/texture_source.dart';
import 'package:morrow_studio/plugins/editor_draft.dart';
import 'package:morrow_studio/plugins/editor_draft_assets.dart';
import 'package:morrow_studio/plugins/editor_draft_import.dart';
import 'package:morrow_studio/plugins/editor_recovery.dart';
import 'package:morrow_studio/plugins/versioned_content.dart';

final _hash = List<int>.filled(32, 9);

IdeaAttachment _attachment(
  String id,
  String name, {
  int bytes = 4,
  TextureKind kind = TextureKind.file,
}) => IdeaAttachment.versioned(
  source: TextureSource(
    location: 'C:/preview/$id',
    name: name,
    kind: kind,
    local: true,
  ),
  byteLength: BigInt.from(bytes),
  pluginId: id,
);

EditorDraftAssetView _view(
  IdeaAttachment attachment, {
  List<String> aliases = const [],
  List<int>? hash,
  String? mediaType,
}) => EditorDraftAssetView(
  attachment: attachment,
  aliases: aliases,
  sha256: hash,
  mediaType: mediaType,
);
VersionedContentRecord _card(BigInt revision, List<VersionedAsset> assets) =>
    VersionedContentRecord(
      id: 'card',
      title: 'card',
      revision: revision,
      formatVersion: 2,
      description: '',
      category: '',
      stage: '',
      hypothesis: '',
      conclusion: '',
      favorite: false,
      assets: assets,
      icon: 0,
      color: 0,
      deleted: false,
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

EditorDraftTextValue _text() => EditorDraftTextValue(
  text: '',
  selectionBase: -1,
  selectionExtent: -1,
  affinity: 0,
  directional: false,
  composingStart: -1,
  composingEnd: -1,
);

EditorDraftValues _values() => EditorDraftValues(
  title: _text(),
  description: _text(),
  hypothesis: _text(),
  conclusion: _text(),
  todos: _text(),
  category: '',
  stage: '',
);

EditorDraftRecord _record({
  String draftId = 'draft',
  BigInt? generation,
  List<EditorDraftAssetSelection> selections = const [],
  List<EditorDraftStoredAsset> assets = const [],
}) {
  final gen = generation ?? BigInt.one;
  return EditorDraftRecord(
    request: EditorDraftWriteRequest(
      cardId: 'card',
      draftId: draftId,
      operation: 'save-$gen',
      expectedGeneration: gen - BigInt.one,
      sourceRevision: BigInt.zero,
      sourceKind: EditorDraftSourceKind.newCard,
      predecessorOperation: '',
      predecessorDigest: const [],
      values: _values(),
      assets: selections,
    ),
    generation: gen,
    active: true,
    currentGeneration: gen,
    currentActive: true,
    repeated: false,
    sourceFormat: 0,
    sourceRevision: BigInt.zero,
    sourceSha256: const [],
    predecessorRevision: BigInt.zero,
    predecessorSha256: const [],
    assets: assets,
  );
}

EditorDraftStoredAsset _stored(
  EditorDraftAssetSelection selection, {
  String name = 'one.bin',
  int bytes = 4,
  String mediaType = 'application/octet-stream',
}) => EditorDraftStoredAsset(
  selection: selection,
  name: name,
  mediaType: mediaType,
  bytes: BigInt.from(bytes),
  sha256: _hash,
);
void main() {
  test('source and canonical S1 with same ID remain explicit', () {
    final old = _attachment('shared', 'one.bin');
    final canonical = _attachment('shared', 'one.bin');
    final asset = VersionedAsset(
      id: 'shared',
      name: 'one.bin',
      kind: 'file',
      bytes: BigInt.from(4),
    );
    final catalog = EditorDraftAssetCatalog.existingCard(
      cardId: 'card',
      draftId: 'draft',
      source: _card(BigInt.one, [asset]),
      sourceViews: [_view(old)],
      predecessor: _card(BigInt.two, [asset]),
      predecessorEvidence: EditorRecovery(
        id: 'card',
        title: 'card',
        operation: 's1',
        digest: _hash,
        sourceRevision: BigInt.one,
        currentRevision: BigInt.two,
        status: EditorRecoveryStatus.committed,
      ),
      predecessorViews: [_view(canonical)],
    );
    expect(
      catalog.selectionsFor([old]).single.origin,
      EditorDraftAssetOrigin.source,
    );
    expect(
      catalog.selectionsFor([canonical]).single.origin,
      EditorDraftAssetOrigin.predecessor,
    );
    expect(
      () => catalog.selectionsFor([old, canonical]),
      throwsFormatException,
    );
    expect(
      () => catalog.selectionsFor([_attachment('shared', 'one.bin')]),
      throwsFormatException,
    );
  });

  test('same view object bound to source and predecessor is ambiguous', () {
    final shared = _attachment('shared', 'one.bin');
    final asset = VersionedAsset(
      id: 'shared',
      name: 'one.bin',
      kind: 'file',
      bytes: BigInt.from(4),
    );
    final catalog = EditorDraftAssetCatalog.existingCard(
      cardId: 'card',
      draftId: 'draft',
      source: _card(BigInt.one, [asset]),
      sourceViews: [_view(shared)],
      predecessor: _card(BigInt.two, [asset]),
      predecessorEvidence: EditorRecovery(
        id: 'card',
        title: 'card',
        operation: 's1',
        digest: _hash,
        sourceRevision: BigInt.one,
        currentRevision: BigInt.two,
        status: EditorRecoveryStatus.committed,
      ),
      predecessorViews: [_view(shared)],
    );
    expect(() => catalog.selectionsFor([shared]), throwsFormatException);
  });

  test('restore keeps exact order, aliases, length and SHA', () {
    final a = _attachment('a', 'one.bin');
    final b = _attachment('b', 'two.bin', bytes: 5);
    final selected = [
      EditorDraftAssetSelection(
        origin: EditorDraftAssetOrigin.staged,
        assetId: 'a',
        aliases: const ['old-a'],
      ),
      EditorDraftAssetSelection(
        origin: EditorDraftAssetOrigin.previousDraft,
        assetId: 'b',
        aliases: const [],
      ),
    ];
    final record = _record(
      generation: BigInt.two,
      selections: selected,
      assets: [
        _stored(selected[0]),
        _stored(selected[1], name: 'two.bin', bytes: 5),
      ],
    );
    final views = [
      _view(a, aliases: const ['old-a'], hash: _hash),
      _view(b, hash: _hash),
    ];
    final restored = EditorDraftAssetCatalog.restore(record, views);
    expect(restored.selectionsFor([b, a]).map((s) => s.assetId), ['b', 'a']);
    expect(restored.selectionsFor([a]).single.aliases, ['old-a']);
    expect(
      () => EditorDraftAssetCatalog.restore(record, [views[1], views[0]]),
      throwsFormatException,
    );
    expect(
      () => EditorDraftAssetCatalog.restore(record, [views[0]]),
      throwsFormatException,
    );
    expect(
      () => EditorDraftAssetCatalog.restore(record, [
        views[0],
        _view(b, hash: List<int>.filled(32, 8)),
      ]),
      throwsFormatException,
    );
  });
  test('ready import promotes only after same-draft confirmed save', () {
    final empty = _record();
    final asset = _attachment('logical-import', 'one.bin');
    final view = _view(asset, aliases: const ['local-name'], hash: _hash);
    final imported = EditorDraftImportRecord(
      request: EditorDraftImportRequest(
        cardId: 'card',
        draftId: 'draft',
        operation: 'import-op',
        expectedGeneration: BigInt.one,
        name: 'one.bin',
        kind: 'file',
        bytes: BigInt.from(4),
        sha256: _hash,
      ),
      assetId: 'logical-import',
      phase: EditorDraftImportPhase.ready,
      currentActive: true,
      bytesRetained: true,
      stagingRevision: BigInt.two,
      repeated: false,
      currentGeneration: BigInt.one,
      mainActive: true,
    );
    final catalog = EditorDraftAssetCatalog.newCard(
      cardId: 'card',
      draftId: 'draft',
      previousDraft: empty,
      readyImports: [imported],
      readyViews: [view],
    );
    final selected = catalog.selectionsFor([asset]);
    expect(selected.single.origin, EditorDraftAssetOrigin.staged);
    final confirmed = _record(
      generation: BigInt.two,
      selections: selected,
      assets: [_stored(selected.single)],
    );
    final next = catalog.afterConfirmedDraft(confirmed, [view]);
    expect(next.confirmedGeneration, BigInt.two);
    expect(
      next.selectionsFor([asset]).single.origin,
      EditorDraftAssetOrigin.previousDraft,
    );
    expect(
      () => catalog.afterConfirmedDraft(
        _record(
          draftId: 'other-draft',
          generation: BigInt.two,
          selections: selected,
          assets: [_stored(selected.single)],
        ),
        [view],
      ),
      throwsFormatException,
    );
    expect(
      () => EditorDraftAssetCatalog.newCard(
        cardId: 'card',
        draftId: 'other-draft',
        previousDraft: empty,
      ),
      throwsFormatException,
    );
  });
  test('explicit previous view shadows its old source binding', () {
    final shared = _attachment('shared', 'one.bin');
    final distinctOld = _attachment('shared', 'one.bin');
    final asset = VersionedAsset(
      id: 'shared',
      name: 'one.bin',
      kind: 'file',
      bytes: BigInt.from(4),
    );
    final selection = EditorDraftAssetSelection(
      origin: EditorDraftAssetOrigin.source,
      assetId: 'shared',
      aliases: const [],
    );
    final previous = EditorDraftRecord(
      request: EditorDraftWriteRequest(
        cardId: 'card',
        draftId: 'draft',
        operation: 'first-save',
        expectedGeneration: BigInt.zero,
        sourceRevision: BigInt.one,
        sourceKind: EditorDraftSourceKind.existingCard,
        predecessorOperation: '',
        predecessorDigest: const [],
        values: _values(),
        assets: [selection],
      ),
      generation: BigInt.one,
      active: true,
      currentGeneration: BigInt.one,
      currentActive: true,
      repeated: false,
      sourceFormat: 2,
      sourceRevision: BigInt.one,
      sourceSha256: _hash,
      predecessorRevision: BigInt.zero,
      predecessorSha256: const [],
      assets: [_stored(selection)],
    );
    final catalog = EditorDraftAssetCatalog.existingCard(
      cardId: 'card',
      draftId: 'draft',
      source: _card(BigInt.one, [asset]),
      sourceViews: [_view(shared)],
      previousDraft: previous,
      previousViews: [_view(shared, hash: _hash)],
    );
    expect(
      catalog.selectionsFor([shared]).single.origin,
      EditorDraftAssetOrigin.previousDraft,
    );
    final separate = EditorDraftAssetCatalog.existingCard(
      cardId: 'card',
      draftId: 'draft',
      source: _card(BigInt.one, [asset]),
      sourceViews: [_view(distinctOld)],
      previousDraft: previous,
      previousViews: [_view(shared, hash: _hash)],
    );
    expect(
      separate.selectionsFor([distinctOld]).single.origin,
      EditorDraftAssetOrigin.source,
    );
    expect(
      separate.selectionsFor([shared]).single.origin,
      EditorDraftAssetOrigin.previousDraft,
    );
    expect(
      () => separate.selectionsFor([distinctOld, shared]),
      throwsFormatException,
    );
  });

  test(
    'S1 confirmation retains a different unselected Ready import for S2',
    () {
      final empty = _record();
      final first = _attachment('logical-a', 'a.bin');
      final second = _attachment('logical-b', 'b.bin');
      final firstView = _view(first, hash: _hash);
      final secondView = _view(second, hash: _hash);
      EditorDraftImportRecord ready(String id, String name) =>
          EditorDraftImportRecord(
            request: EditorDraftImportRequest(
              cardId: 'card',
              draftId: 'draft',
              operation: 'import-$id',
              expectedGeneration: BigInt.one,
              name: name,
              kind: 'file',
              bytes: BigInt.from(4),
              sha256: _hash,
            ),
            assetId: id,
            phase: EditorDraftImportPhase.ready,
            currentActive: true,
            bytesRetained: true,
            stagingRevision: BigInt.two,
            repeated: false,
            currentGeneration: BigInt.one,
            mainActive: true,
          );
      final catalog = EditorDraftAssetCatalog.newCard(
        cardId: 'card',
        draftId: 'draft',
        previousDraft: empty,
        readyImports: [
          ready('logical-a', 'a.bin'),
          ready('logical-b', 'b.bin'),
        ],
        readyViews: [firstView, secondView],
      );
      final submittedS1 = catalog.selectionsFor([first]);
      final confirmed = _record(
        generation: BigInt.two,
        selections: submittedS1,
        assets: [_stored(submittedS1.single, name: 'a.bin')],
      );
      final next = catalog.afterConfirmedDraft(confirmed, [firstView]);
      expect(
        next.selectionsFor([first]).single.origin,
        EditorDraftAssetOrigin.previousDraft,
      );
      expect(
        next.selectionsFor([second]).single.origin,
        EditorDraftAssetOrigin.staged,
      );
      expect(next.selectionsFor([second, first]).map((s) => s.assetId), [
        'logical-b',
        'logical-a',
      ]);
    },
  );
  test('stored host MIME mappings and concrete source MIME stay typed', () {
    final cases = <(TextureKind, String)>[
      (TextureKind.image, 'image/*'),
      (TextureKind.gif, 'image/gif'),
      (TextureKind.video, 'video/*'),
      (TextureKind.audio, 'audio/*'),
      (TextureKind.file, 'application/octet-stream'),
      (TextureKind.image, 'image/png'),
      (TextureKind.file, 'text/plain'),
      (TextureKind.file, 'application/pdf'),
    ];
    for (final (kind, mediaType) in cases) {
      final selection = EditorDraftAssetSelection(
        origin: EditorDraftAssetOrigin.previousDraft,
        assetId: 'same-id',
        aliases: const [],
      );
      final record = _record(
        selections: [selection],
        assets: [_stored(selection, mediaType: mediaType)],
      );
      final view = _view(
        _attachment('same-id', 'one.bin', kind: kind),
        hash: _hash,
      );
      expect(
        EditorDraftAssetCatalog.restore(record, [
          view,
        ]).selectionsFor([view.attachment]).single.assetId,
        'same-id',
      );
    }

    final selection = EditorDraftAssetSelection(
      origin: EditorDraftAssetOrigin.previousDraft,
      assetId: 'same-id',
      aliases: const [],
    );
    final imageRecord = _record(
      selections: [selection],
      assets: [_stored(selection, mediaType: 'image/png')],
    );
    expect(
      () => EditorDraftAssetCatalog.restore(imageRecord, [
        _view(_attachment('same-id', 'one.bin'), hash: _hash),
      ]),
      throwsFormatException,
    );
    final unusual = _record(
      selections: [selection],
      assets: [_stored(selection, mediaType: 'model/gltf-binary')],
    );
    final file = _attachment('same-id', 'one.bin');
    expect(
      () =>
          EditorDraftAssetCatalog.restore(unusual, [_view(file, hash: _hash)]),
      throwsFormatException,
    );
    expect(
      EditorDraftAssetCatalog.restore(unusual, [
        _view(file, hash: _hash, mediaType: 'model/gltf-binary'),
      ]).selectionsFor([file]).single.assetId,
      'same-id',
    );
    expect(
      () => EditorDraftAssetCatalog.restore(unusual, [
        _view(file, hash: _hash, mediaType: 'model/gltf+json'),
      ]),
      throwsFormatException,
    );
  });
}
