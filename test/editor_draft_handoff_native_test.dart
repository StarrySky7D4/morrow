import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/main.dart';
import 'package:morrow_studio/plugins/editor_draft.dart';
import 'package:morrow_studio/plugins/editor_draft_session.dart';
import 'package:morrow_studio/plugins/editor_recovery.dart';
import 'package:morrow_studio/plugins/editor_session.dart';
import 'package:morrow_studio/plugins/versioned_editor_adapter.dart';
import 'package:morrow_studio/plugins/workbench_backend.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';

final _executable = Platform.environment['MORROW_WORKBENCH_HOST'];
final _package = Platform.environment['MORROW_WORKBENCH_PACKAGE'];
final _available =
    Platform.isWindows && _executable != null && _package != null;

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

void main() {
  test(
    'real captured S1 hands complete selected S2 to durable child; retirement and old retries survive restart',
    () async {
      final directory = await Directory.systemTemp.createTemp(
        'morrow-editor-handoff-native-',
      );
      RustWorkbench? host;
      EditorDraftSession? restoredSession;
      try {
        host = await RustWorkbench.open(
          executable: _executable!,
          package: _package!,
          directory: directory,
        );
        const card = 'handoff-native-card';
        const parentDraft = 'handoff-parent';
        const childDraft = 'handoff-child';
        final sourceRevision = await _seed(host, card);

        final firstFile = File('${directory.path}/first.bin');
        final secondFile = File('${directory.path}/second.bin');
        final firstBytes = List<int>.generate(38000, (i) => i % 251);
        final secondBytes = List<int>.generate(41000, (i) => (i * 7) % 253);
        await firstFile.writeAsBytes(firstBytes);
        await secondFile.writeAsBytes(secondBytes);
        final firstAsset = await host.editorDrafts.importAsset(
          card,
          parentDraft,
          BigInt.zero,
          firstFile.path,
          'first.bin',
          'file',
          BigInt.from(firstBytes.length),
        );
        final secondAsset = await host.editorDrafts.importAsset(
          card,
          parentDraft,
          BigInt.zero,
          secondFile.path,
          'second.bin',
          'file',
          BigInt.from(secondBytes.length),
        );
        // Order and aliases are part of the raw selected snapshot, including
        // selections not committed to the business card by S1.
        final parentSelections = [
          EditorDraftAssetSelection(
            origin: EditorDraftAssetOrigin.staged,
            assetId: secondAsset.id,
            aliases: const ['second.bin', 'second:alternate'],
          ),
          EditorDraftAssetSelection(
            origin: EditorDraftAssetOrigin.staged,
            assetId: firstAsset.id,
            aliases: const ['first.bin'],
          ),
        ];
        final original = _write(
          cardId: card,
          draftId: parentDraft,
          operation: 'native-parent-first',
          generation: BigInt.zero,
          sourceRevision: sourceRevision,
          values: _values('Uncommitted S2'),
          assets: parentSelections,
        );
        final first = await host.editorDrafts.save(original);
        expect(first.generation, BigInt.one);
        expect(first.requestSha256, hasLength(32));
        expect(first.request.values.description.selectionBase, 17);
        expect(first.request.values.description.composingEnd, 18);
        await firstFile.delete();
        await secondFile.delete();

        final committed = await _commitS1(host, card, sourceRevision);
        expect(
          (await host.versionedContent.read(card)).revision,
          sourceRevision + BigInt.one,
        );
        final staleChild = _write(
          cardId: card,
          draftId: childDraft,
          operation: 'native-child-handoff',
          generation: BigInt.zero,
          sourceRevision: sourceRevision + BigInt.one,
          values: first.request.values,
          assets: _withOrigin(
            first.request.assets,
            EditorDraftAssetOrigin.parentDraft,
          ),
        );
        final stale = EditorDraftHandoffRequest(
          request: staleChild,
          parentLink: _link(first, staleChild, committed),
        );

        // S2 changed locally after S1. The old parent generation must not be
        // treated as the current complete snapshot.
        final parentNext = _write(
          cardId: card,
          draftId: parentDraft,
          operation: 'native-parent-latest',
          generation: BigInt.one,
          sourceRevision: sourceRevision,
          values: _values('Latest uncommitted S2'),
          assets: _withOrigin(
            first.request.assets,
            EditorDraftAssetOrigin.previousDraft,
          ),
        );
        final parent = await host.editorDrafts.save(parentNext);
        expect(parent.generation, BigInt.two);
        expect(parent.requestSha256, hasLength(32));
        await expectLater(
          host.editorDraftHandoffs.handoff(stale),
          throwsA(isA<EditorDraftHandoffFailure>()),
        );
        expect(await host.editorDrafts.read(card, childDraft), isNull);

        final child = _write(
          cardId: card,
          draftId: childDraft,
          operation: staleChild.operation,
          generation: BigInt.zero,
          sourceRevision: sourceRevision + BigInt.one,
          values: parent.request.values,
          assets: _withOrigin(
            parent.request.assets,
            EditorDraftAssetOrigin.parentDraft,
          ),
        );
        final link = _link(parent, child, committed);
        final handoff = EditorDraftHandoffRequest(
          request: child,
          parentLink: link,
        );
        final changedLink = EditorDraftHandoffRequest(
          request: child,
          parentLink: EditorDraftParentLink(
            parentDraftId: link.parentDraftId,
            parentGeneration: link.parentGeneration,
            parentSaveOperation: link.parentSaveOperation,
            parentRequestSha256: List<int>.filled(32, 0),
            committedOperation: link.committedOperation,
            committedSha256: link.committedSha256,
            childOperation: link.childOperation,
          ),
        );
        await expectLater(
          host.editorDraftHandoffs.handoff(changedLink),
          throwsA(isA<EditorDraftHandoffFailure>()),
        );
        await expectLater(
          host.editorDrafts.save(child),
          throwsA(isA<EditorDraftSaveFailure>()),
        );
        expect(await host.editorDrafts.read(card, childDraft), isNull);

        final savedChild = await host.editorDraftHandoffs.handoff(handoff);
        expect(savedChild.generation, BigInt.one);
        expect(savedChild.active, isTrue);
        expect(savedChild.requestSha256, hasLength(32));
        expect(
          savedChild.parentLink!.parentRequestSha256,
          parent.requestSha256,
        );
        expect(savedChild.request.values.title.text, 'Latest uncommitted S2');
        expect(savedChild.request.assets.map((a) => a.origin), [
          EditorDraftAssetOrigin.parentDraft,
          EditorDraftAssetOrigin.parentDraft,
        ]);
        expect(savedChild.request.assets.map((a) => a.assetId), [
          secondAsset.id,
          firstAsset.id,
        ]);
        expect(savedChild.request.assets.map((a) => a.aliases), [
          ['second.bin', 'second:alternate'],
          ['first.bin'],
        ]);
        expect(
          (await host.versionedContent.read(card)).revision,
          sourceRevision + BigInt.one,
        );
        expect((await host.versionedContent.read(card)).title, 'Committed S1');
        final firstPage = await host.editorDraftHandoffs.listLineages(limit: 1);
        expect(firstPage.lineages, hasLength(1));
        expect(firstPage.lineages.single.childDraftId, childDraft);
        expect(firstPage.lineages.single.parentActive, isTrue);
        expect(firstPage.lineages.single.childActive, isTrue);
        expect(await host.editorDraftHandoffs.discoverLineages(), hasLength(1));
        await host.editorDrafts.exportAsset(
          card,
          childDraft,
          BigInt.one,
          secondAsset.id,
          '${directory.path}/export-second.bin',
        );
        expect(
          await File('${directory.path}/export-second.bin').readAsBytes(),
          secondBytes,
        );

        // Listing does not need an enabled guest package or a capture scope.
        await host.close();
        host = null;
        host = await RustWorkbench.open(
          executable: _executable!,
          package: '${directory.path}/missing-plugin.morrowplugin',
          directory: directory,
        );
        expect((await host.pluginState()).writable, isFalse);
        final readOnly = await host.editorDraftHandoffs.listLineages(limit: 1);
        expect(readOnly.lineages, hasLength(1));
        expect(
          readOnly.lineages.single.parentLink.committedOperation,
          committed.operation,
        );
        expect(await host.editorDraftHandoffs.discoverLineages(), hasLength(1));
        await host.close();
        host = null;
        host = await RustWorkbench.open(
          executable: _executable!,
          package: _package!,
          directory: directory,
        );

        await expectLater(
          host.editorDrafts.save(
            _write(
              cardId: card,
              draftId: parentDraft,
              operation: 'native-parent-forbidden-after-child',
              generation: BigInt.two,
              sourceRevision: sourceRevision,
              values: parent.request.values,
              assets: _withOrigin(
                parent.request.assets,
                EditorDraftAssetOrigin.previousDraft,
              ),
            ),
          ),
          throwsA(isA<EditorDraftSaveFailure>()),
        );
        final oldParent = await host.editorDrafts.save(original);
        expect(oldParent.repeated, isTrue);
        expect(oldParent.currentGeneration, BigInt.two);
        final retirement = EditorDraftRetirementRequest(
          cardId: card,
          childDraftId: childDraft,
          parentDraftId: parentDraft,
          childOperation: child.operation,
          parentGeneration: parent.generation,
          operation: 'native-parent-retirement',
        );
        final retired = await host.editorDraftHandoffs.retireParent(retirement);
        expect(retired.active, isFalse);
        expect(retired.generation, BigInt.from(3));
        expect(retired.retirement!.childOperation, child.operation);
        expect(
          (await host.editorDrafts.read(card, parentDraft))!.active,
          isFalse,
        );

        await host.close();
        host = null;
        host = await RustWorkbench.open(
          executable: _executable!,
          package: _package!,
          directory: directory,
        );
        final lineage = await host.editorDraftHandoffs.discoverLineages();
        expect(lineage, hasLength(1));
        expect(lineage.single.parentActive, isFalse);
        expect(lineage.single.parentGeneration, BigInt.from(3));
        expect(lineage.single.childActive, isTrue);
        final historicalHandoff = await host.editorDraftHandoffs.handoff(
          handoff,
        );
        expect(historicalHandoff.repeated, isTrue);
        expect(historicalHandoff.generation, BigInt.one);
        expect(historicalHandoff.currentGeneration, BigInt.one);
        final historicalRetirement = await host.editorDraftHandoffs
            .retireParent(retirement);
        expect(historicalRetirement.repeated, isTrue);
        expect(historicalRetirement.generation, BigInt.from(3));
        await expectLater(
          host.editorDraftHandoffs.handoff(changedLink),
          throwsA(isA<EditorDraftHandoffFailure>()),
        );
        final liveChild = (await host.editorDrafts.read(card, childDraft))!;
        expect(liveChild.parentLink, isNotNull);
        restoredSession = EditorDraftSession.restore(
          control: host.editorDrafts,
          record: liveChild,
          operationFactory: () => 'native-child-next',
          isCurrent: () => true,
          debounce: const Duration(hours: 1),
        );
        restoredSession.adoptConfirmedAssetPins(liveChild);
        expect(
          restoredSession.current.assets.map((a) => a.origin),
          everyElement(EditorDraftAssetOrigin.previousDraft),
        );
        restoredSession.observe(
          EditorDraftSnapshot(
            values: _values('Further uncommitted S2'),
            assets: restoredSession.current.assets,
          ),
        );
        final nextChild = (await restoredSession.flush())!;
        expect(nextChild.generation, BigInt.two);
        expect(nextChild.request.assets.map((a) => a.origin), [
          EditorDraftAssetOrigin.previousDraft,
          EditorDraftAssetOrigin.previousDraft,
        ]);
        expect(nextChild.parentLink, isNotNull);
        expect(
          (await host.versionedContent.read(card)).revision,
          sourceRevision + BigInt.one,
        );
        final replay = await host.editorDraftHandoffs.handoff(handoff);
        expect(replay.repeated, isTrue);
        expect(replay.generation, BigInt.one);
        expect(replay.currentGeneration, BigInt.two);
        expect(
          (await host.editorDrafts.read(card, childDraft))!.generation,
          BigInt.two,
        );
        await host.editorDrafts.exportAsset(
          card,
          childDraft,
          BigInt.two,
          firstAsset.id,
          '${directory.path}/export-first.bin',
        );
        expect(
          await File('${directory.path}/export-first.bin').readAsBytes(),
          firstBytes,
        );

        // B can itself be a parent after A has retired. Its retirement
        // record must retain both its incoming A→B link and outgoing B→C
        // marker, including after a process restart.
        restoredSession.dispose();
        restoredSession = null;
        final secondCommit = await _commitS1(
          host,
          card,
          sourceRevision + BigInt.one,
          title: 'Committed S2',
          description: 'Second committed business body',
        );
        const grandchildDraft = 'handoff-grandchild';
        final grandchild = _write(
          cardId: card,
          draftId: grandchildDraft,
          operation: 'native-grandchild-handoff',
          generation: BigInt.zero,
          sourceRevision: sourceRevision + BigInt.from(2),
          values: nextChild.request.values,
          assets: _withOrigin(
            nextChild.request.assets,
            EditorDraftAssetOrigin.parentDraft,
          ),
        );
        final secondHandoff = EditorDraftHandoffRequest(
          request: grandchild,
          parentLink: _link(nextChild, grandchild, secondCommit),
        );
        final savedGrandchild = await host.editorDraftHandoffs.handoff(
          secondHandoff,
        );
        expect(savedGrandchild.generation, BigInt.one);
        expect(savedGrandchild.parentLink, isNotNull);
        expect(savedGrandchild.request.assets.map((a) => a.assetId), [
          secondAsset.id,
          firstAsset.id,
        ]);
        expect(
          (await host.versionedContent.read(card)).revision,
          sourceRevision + BigInt.from(2),
        );

        final pageOne = await host.editorDraftHandoffs.listLineages(limit: 1);
        expect(pageOne.lineages, hasLength(1));
        expect(pageOne.nextCursor, isNotEmpty);
        final pageTwo = await host.editorDraftHandoffs.listLineages(
          cursor: pageOne.nextCursor,
          limit: 1,
        );
        expect(pageTwo.lineages, hasLength(1));
        expect(pageTwo.nextCursor, isEmpty);
        expect(pageTwo.requestCursor, pageOne.nextCursor);
        expect(
          {
            pageOne.lineages.single.childDraftId,
            pageTwo.lineages.single.childDraftId,
          },
          {childDraft, grandchildDraft},
        );
        expect(
          (await host.editorDraftHandoffs.discoverLineages())
              .map((row) => row.childDraftId)
              .toSet(),
          {childDraft, grandchildDraft},
        );

        final retireChild = EditorDraftRetirementRequest(
          cardId: card,
          childDraftId: grandchildDraft,
          parentDraftId: childDraft,
          childOperation: grandchild.operation,
          parentGeneration: nextChild.generation,
          operation: 'native-child-retirement',
        );
        final retiredChild = await host.editorDraftHandoffs.retireParent(
          retireChild,
        );
        expect(retiredChild.generation, BigInt.from(3));
        expect(retiredChild.active, isFalse);
        expect(retiredChild.parentLink, isNotNull);
        expect(retiredChild.retirement!.childDraftId, grandchildDraft);

        await host.close();
        host = null;
        host = await RustWorkbench.open(
          executable: _executable!,
          package: _package!,
          directory: directory,
        );
        final middle = (await host.editorDrafts.read(card, childDraft))!;
        expect(middle.active, isFalse);
        expect(middle.parentLink!.parentDraftId, parentDraft);
        expect(middle.retirement!.childDraftId, grandchildDraft);
        expect(
          (await host.editorDraftHandoffs.discoverLineages())
              .map((row) => row.childDraftId)
              .toSet(),
          {childDraft, grandchildDraft},
        );
        final repeatedMiddle = await host.editorDraftHandoffs.retireParent(
          retireChild,
        );
        expect(repeatedMiddle.repeated, isTrue);
        expect(repeatedMiddle.parentLink, isNotNull);
        expect(repeatedMiddle.retirement, isNotNull);
        final oldMiddleHandoff = await host.editorDraftHandoffs.handoff(
          handoff,
        );
        expect(oldMiddleHandoff.repeated, isTrue);
        expect(oldMiddleHandoff.generation, BigInt.one);
        expect(oldMiddleHandoff.currentGeneration, BigInt.from(3));
        expect(oldMiddleHandoff.currentActive, isFalse);
        expect(
          (await host.editorDrafts.read(card, grandchildDraft))!.generation,
          BigInt.one,
        );
      } finally {
        restoredSession?.dispose();
        await host?.close();
        await directory.delete(recursive: true);
      }
    },
    skip: !_available,
    timeout: const Timeout(Duration(minutes: 6)),
  );
}
