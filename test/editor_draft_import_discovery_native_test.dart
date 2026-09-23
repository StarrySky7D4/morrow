import 'dart:io';

import 'package:crypto/crypto.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/editor_draft.dart';
import 'package:morrow_studio/plugins/editor_draft_import.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';

final _host = Platform.environment['MORROW_WORKBENCH_HOST'];
final _package = Platform.environment['MORROW_WORKBENCH_PACKAGE'];
final _available = Platform.isWindows && _host != null && _package != null;

EditorDraftTextValue _text(String value) => EditorDraftTextValue(
  text: value,
  selectionBase: value.length,
  selectionExtent: value.length,
  affinity: 0,
  directional: false,
  composingStart: -1,
  composingEnd: -1,
);

EditorDraftWriteRequest _draft(String id) => EditorDraftWriteRequest(
  cardId: 'discovery-card-$id',
  draftId: 'discovery-draft-$id',
  operation: 'create-draft-$id',
  sourceKind: EditorDraftSourceKind.newCard,
  sourceRevision: BigInt.zero,
  expectedGeneration: BigInt.zero,
  predecessorOperation: '',
  predecessorDigest: const [],
  values: EditorDraftValues(
    title: _text('draft $id'),
    description: _text('pending input'),
    hypothesis: _text(''),
    conclusion: _text(''),
    todos: _text(''),
    category: '',
    stage: '',
  ),
  assets: const [],
);

EditorDraftImportRequest _import(String id) => EditorDraftImportRequest(
  cardId: 'discovery-card-$id',
  draftId: 'discovery-draft-$id',
  operation: 'import-$id',
  expectedGeneration: BigInt.one,
  name: 'asset.bin',
  kind: 'file',
  bytes: BigInt.one,
  sha256: sha256.convert([42]).bytes,
);

EditorDraftImportAbandon _decision(String id, {String? operation}) =>
    EditorDraftImportAbandon(
      cardId: 'discovery-card-$id',
      draftId: 'discovery-draft-$id',
      importOperation: 'import-$id',
      operation: operation ?? 'abandon-$id',
      currentGeneration: BigInt.one,
    );

void main() {
  test(
    'global decision scan shares the draft queue and includes inactive scopes',
    () async {
      final directory = await Directory.systemTemp.createTemp(
        'morrow-decision-discovery-',
      );
      RustWorkbench? workbench;
      try {
        workbench = await RustWorkbench.open(
          executable: _host!,
          package: _package!,
          directory: directory,
        );
        for (final id in ['a', 'b']) {
          await workbench.editorDrafts.save(_draft(id));
          await workbench.editorDraftImports.begin(_import(id));
          await workbench.editorDraftImports.prepareDecision(_decision(id));
        }
        // One scope spans two pages. Each cancelled decision remains audited,
        // while a new decision for the same original import can be prepared.
        await workbench.editorDraftImports.cancelDecision(_decision('a'));
        for (var index = 1; index < 33; index++) {
          final intent = _decision('a', operation: 'abandon-a-$index');
          await workbench.editorDraftImports.prepareDecision(intent);
          await workbench.editorDraftImports.cancelDecision(intent);
        }
        await workbench.editorDrafts.discard(
          'discovery-card-b',
          'discovery-draft-b',
          BigInt.one,
          'discard-draft-b',
        );

        final scan = workbench.editorDraftImports.discoverDecisions();
        final queuedIntent = _decision('a', operation: 'abandon-a-queued');
        final queuedPrepare = workbench.editorDraftImports.prepareDecision(
          queuedIntent,
        );
        final queuedRead = workbench.editorDrafts.read(
          'discovery-card-a',
          'discovery-draft-a',
        );
        final first = await scan;
        expect(first, hasLength(34));
        expect(
          first.any((decision) => decision.operation == queuedIntent.operation),
          isFalse,
        );
        await queuedPrepare;
        expect(await queuedRead, isA<EditorDraftRecord>());
        expect(
          first
              .singleWhere((decision) => decision.operation == 'abandon-b')
              .mainActive,
          isFalse,
        );
        expect(() => first.clear(), throwsUnsupportedError);

        final second = await workbench.editorDraftImports.discoverDecisions();
        expect(second, hasLength(35));
        expect(
          second.any(
            (decision) => decision.operation == queuedIntent.operation,
          ),
          isTrue,
        );
        final scopes = await workbench.editorDraftImports.listDecisionScopes();
        expect(scopes.scopes, hasLength(2));
      } finally {
        await workbench?.close();
        await directory.delete(recursive: true);
      }
    },
    skip: !_available,
    timeout: const Timeout(Duration(minutes: 4)),
  );
}
