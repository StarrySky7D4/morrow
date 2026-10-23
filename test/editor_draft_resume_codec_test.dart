import 'dart:typed_data';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/editor_draft.dart';
import 'package:morrow_studio/plugins/editor_draft_codec.dart';

EditorDraftResumeRequest proposal({
  bool create = false,
  String operation = 'old-business',
  String scope = 'fresh-scope',
  int events = 0,
  String retirement = 'retire-parent',
  String business = 'new-business',
  String parent = 'parent',
  int hashLength = 32,
}) {
  const raw = EditorDraftTextValue(
    text: 'A😀B',
    selectionBase: 2,
    selectionExtent: 1,
    affinity: 1,
    directional: true,
    composingStart: 1,
    composingEnd: 2,
  );
  const blank = EditorDraftTextValue(
    text: '',
    selectionBase: 0,
    selectionExtent: 0,
    affinity: 0,
    directional: false,
    composingStart: -1,
    composingEnd: -1,
  );
  return EditorDraftResumeRequest(
    request: EditorDraftWriteRequest(
      cardId: 'card',
      draftId: 'child',
      operation: operation,
      expectedGeneration: BigInt.zero,
      sourceRevision: create ? BigInt.zero : BigInt.two,
      sourceKind: create
          ? EditorDraftSourceKind.newCard
          : EditorDraftSourceKind.existingCard,
      commitOperation: business,
      captureScope: scope,
      captureEvents: events,
      predecessorOperation: '',
      predecessorDigest: [],
      values: const EditorDraftValues(
        title: blank,
        description: raw,
        hypothesis: blank,
        conclusion: blank,
        todos: blank,
        category: '实验',
        stage: '计划中',
      ),
      assets: [
        EditorDraftAssetSelection(
          origin: EditorDraftAssetOrigin.parentDraft,
          assetId: 'pin',
          aliases: ['deleted.bin'],
        ),
      ],
    ),
    resumeLink: EditorDraftResumeLink(
      parentDraftId: parent,
      parentGeneration: BigInt.from(7),
      parentSaveOperation: 'parent-save',
      parentRequestSha256: List.filled(hashLength, 9),
      parentBusinessOperation: 'old-business',
      retirementOperation: retirement,
    ),
  );
}

void main() {
  test(
    'fresh resume preserves raw UTF-16 and all parent pins for both source formats',
    () {
      for (final create in [false, true]) {
        final original = proposal(create: create);
        final bytes = EditorDraftCodec.encodeResume(original);
        final actual = EditorDraftCodec.decodeResume(bytes);
        expect(EditorDraftCodec.encodeResume(actual), bytes);
        expect(actual.request.values.title.text, '');
        expect(actual.request.values.description.selectionBase, 2);
        expect(actual.request.values.description.composingEnd, 2);
        expect(actual.request.values.description.directional, true);
        expect(
          actual.request.assets.single.origin,
          EditorDraftAssetOrigin.parentDraft,
        );
        expect(actual.request.sourceKind, original.request.sourceKind);
        expect(actual.resumeLink.parentGeneration, BigInt.from(7));
      }
    },
  );
  test(
    'resume rejects reused operations, dead scopes, forged digests and capture replay',
    () {
      for (final invalid in [
        proposal(operation: 'replacement'),
        proposal(scope: ''),
        proposal(events: 1),
        proposal(retirement: 'old-business'),
        proposal(retirement: 'parent-save'),
        proposal(business: 'old-business'),
        proposal(business: 'retire-parent'),
        proposal(parent: 'child'),
        proposal(hashLength: 31),
      ]) {
        expect(
          () => EditorDraftCodec.encodeResume(invalid),
          throwsFormatException,
        );
      }
    },
  );
  test('resume decoder rejects truncated or appended wire bytes', () {
    final bytes = EditorDraftCodec.encodeResume(proposal());
    expect(
      () => EditorDraftCodec.decodeResume(
        Uint8List.sublistView(bytes, 0, bytes.length - 8),
      ),
      throwsFormatException,
    );
    expect(
      () => EditorDraftCodec.decodeResume(
        Uint8List.fromList([...bytes, ...List.filled(8, 0)]),
      ),
      throwsFormatException,
    );
  });
}
