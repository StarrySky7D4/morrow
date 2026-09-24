import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/main.dart';
import 'package:morrow_studio/plugins/editor_draft.dart';
import 'dart:math';
import 'package:morrow_studio/plugins/editor_recovery.dart';
import 'package:morrow_studio/plugins/editor_session.dart';
import 'package:morrow_studio/plugins/versioned_editor_adapter.dart';
import 'package:morrow_studio/plugins/workbench_backend.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';

EditorDraftTextValue _text(
  String text, {
  int? base,
  int? extent,
  int composingStart = -1,
  int composingEnd = -1,
}) => EditorDraftTextValue(
  text: text,
  selectionBase: base ?? text.length,
  selectionExtent: extent ?? text.length,
  affinity: 1,
  directional: true,
  composingStart: composingStart,
  composingEnd: composingEnd,
);

EditorDraftValues _values(String title) => EditorDraftValues(
  title: _text(title),
  description: _text(
    'Uncommitted S2 😀 body',
    base: 17,
    extent: 18,
    composingStart: 12,
    composingEnd: 18,
  ),
  hypothesis: _text(''),
  conclusion: _text(''),
  todos: _text(''),
  category: '进行中',
  stage: '计划中',
);

EditorDraftWriteRequest _write({
  required String cardId,
  required String draftId,
  required String operation,
  required BigInt generation,
  required BigInt sourceRevision,
  required EditorDraftValues values,
  required List<EditorDraftAssetSelection> assets,
}) => EditorDraftWriteRequest(
  cardId: cardId,
  draftId: draftId,
  operation: operation,
  expectedGeneration: generation,
  sourceRevision: sourceRevision,
  predecessorOperation: '',
  predecessorDigest: const [],
  values: values,
  assets: assets,
);

List<EditorDraftAssetSelection> _withOrigin(
  List<EditorDraftAssetSelection> assets,
  EditorDraftAssetOrigin origin,
) => [
  for (final asset in assets)
    EditorDraftAssetSelection(
      origin: origin,
      assetId: asset.assetId,
      aliases: asset.aliases,
    ),
];

EditorDraftParentLink _link(
  EditorDraftRecord parent,
  EditorDraftWriteRequest child,
  EditorRecovery committed,
) => EditorDraftParentLink.fromParentRecord(
  parent: parent,
  committedOperation: committed.operation,
  committedSha256: committed.digest,
  childOperation: child.operation,
);

Future<BigInt> _seed(RustWorkbench host, String id) async {
  await host.apply(
    PluginAction.create,
    Idea(
      'Source title',
      'Source body',
      '进行中',
      Idea.icons[0],
      const Color(0xff8866aa),
      id: id,
      stage: '计划中',
    ),
  );
  final plan = await host.versionedContent.planMigration(id);
  return (await host.versionedContent.migrate(plan)).current.revision;
}

Future<EditorRecovery> _commitS1(
  RustWorkbench host,
  String id,
  BigInt sourceRevision, {
  String title = 'Committed S1',
  String description = 'Committed business body',
}) async {
  final baselineRecord = await host.versionedContent.read(id);
  expect(baselineRecord.revision, sourceRevision);
  final baseline = await host.workspaceRecord(baselineRecord);
  final session = await host.openVersionedEditor(
    id,
    expectedRevision: sourceRevision,
  );
  final adapter = VersionedWorkbenchEditorAdapter(
    session,
    baseline.versioned!,
    host.workspaceRecord,
  );
  try {
    final proposed = Idea(
      title,
      description,
      baseline.category,
      baseline.icon,
      baseline.color,
      id: id,
      stage: baseline.stage,
      favorite: baseline.favorite,
      hypothesis: baseline.hypothesis,
      conclusion: baseline.conclusion,
      attachments: baseline.attachments,
      contentRevision: baseline.contentRevision,
      contentOwner: baseline.contentOwner,
      versioned: baseline.versioned,
    );
    final saved = await adapter.save(
      proposed,
      EditorFields(
        title: title,
        description: description,
        hypothesis: '',
        conclusion: '',
        todos: '',
      ),
    );
    expect(saved.contentRevision, sourceRevision + BigInt.one);
    final evidence = (adapter as EditorDraftPredecessorSource).draftPredecessor;
    expect(evidence, isNotNull);
    expect(evidence!.operation, isNotEmpty);
    expect(evidence.digest, hasLength(32));
    expect(evidence.status, EditorRecoveryStatus.committed);
    return evidence;
  } finally {
    await adapter.close();
  }
}

const proposalBytes = [2, 7, 13, 255, 0, 91];
String proposalRawTitle() => 'Uncommitted S2 😀 ' * 6000;
String freshProposalId(String prefix) =>
    '$prefix-${DateTime.now().microsecondsSinceEpoch}-${Random.secure().nextInt(1 << 32)}';

Future<EditorDraftHandoffProposal> seedHandoffProposal(
  RustWorkbench host,
  Directory directory, {
  void Function(EditorRecovery)? onCommitted,
}) async {
  final card = freshProposalId('card');
  final parentId = freshProposalId('parent');
  final childId = freshProposalId('child');
  final revision = await _seed(host, card);
  final file = File('${directory.path}/source.bin');
  await file.writeAsBytes(proposalBytes);
  final asset = await host.editorDrafts.importAsset(
    card,
    parentId,
    BigInt.zero,
    file.path,
    'source.bin',
    'file',
    BigInt.from(proposalBytes.length),
  );
  final parent = await host.editorDrafts.save(
    _write(
      cardId: card,
      draftId: parentId,
      operation: freshProposalId('parent-save'),
      generation: BigInt.zero,
      sourceRevision: revision,
      values: _values(proposalRawTitle()),
      assets: [
        EditorDraftAssetSelection(
          origin: EditorDraftAssetOrigin.staged,
          assetId: asset.id,
          aliases: const ['source.bin', 'raw attachment'],
        ),
      ],
    ),
  );
  await file.delete();
  final committed = await _commitS1(host, card, revision);
  onCommitted?.call(committed);
  final child = _write(
    cardId: card,
    draftId: childId,
    operation: freshProposalId('child-save'),
    generation: BigInt.zero,
    sourceRevision: revision + BigInt.one,
    values: parent.request.values,
    assets: _withOrigin(
      parent.request.assets,
      EditorDraftAssetOrigin.parentDraft,
    ),
  );
  return EditorDraftHandoffProposal(
    handoff: EditorDraftHandoffRequest(
      request: child,
      parentLink: _link(parent, child, committed),
    ),
    retirementOperation: freshProposalId('retire'),
  );
}
