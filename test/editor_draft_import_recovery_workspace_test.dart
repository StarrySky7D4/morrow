import 'dart:io';

import 'package:crypto/crypto.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/editor_draft_import_recovery_dialog.dart';
import 'package:morrow_studio/main.dart';
import 'package:morrow_studio/plugins/editor_draft.dart';
import 'package:morrow_studio/plugins/editor_draft_import.dart';
import 'package:morrow_studio/plugins/studio_storage.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';

final _host = Platform.environment['MORROW_WORKBENCH_HOST'];
final _package = Platform.environment['MORROW_WORKBENCH_PACKAGE'];
const _card = 'inactive-recovery-ui-card';
const _draft = 'inactive-recovery-ui-draft';
const _op = 'inactive-ui-abandon';
EditorDraftTextValue _text() => const EditorDraftTextValue(
  text: '',
  selectionBase: -1,
  selectionExtent: -1,
  affinity: 0,
  directional: false,
  composingStart: -1,
  composingEnd: -1,
);
Future<void> _wait(WidgetTester t, bool Function() ready) async {
  for (var i = 0; i < 100; i++) {
    await t.runAsync(
      () => Future<void>.delayed(const Duration(milliseconds: 20)),
    );
    await t.pump(const Duration(milliseconds: 50));
    if (ready()) return;
  }
  expect(ready(), isTrue, reason: 'Native recovery UI did not settle');
}

void main() {
  testWidgets(
    'workspace recovery menu discovers discarded draft after reopen and cancels only on confirmation',
    (t) async {
      t.view.devicePixelRatio = 1;
      t.view.physicalSize = const Size(1180, 850);
      addTearDown(t.view.reset);
      Directory? directory;
      RustWorkbench? backend;
      RustStudioStorage? storage;
      final intent = EditorDraftImportAbandon(
        cardId: _card,
        draftId: _draft,
        importOperation: 'inactive-ui-import',
        operation: _op,
        currentGeneration: BigInt.one,
      );
      try {
        await t.runAsync(() async {
          directory = await Directory.systemTemp.createTemp(
            'morrow-import-recovery-ui-',
          );
          backend = await RustWorkbench.open(
            executable: _host!,
            package: _package!,
            directory: directory!,
          );
          await backend!.editorDrafts.save(
            EditorDraftWriteRequest(
              cardId: _card,
              draftId: _draft,
              operation: 'inactive-ui-draft',
              sourceKind: EditorDraftSourceKind.newCard,
              sourceRevision: BigInt.zero,
              expectedGeneration: BigInt.zero,
              predecessorOperation: '',
              predecessorDigest: [],
              values: EditorDraftValues(
                title: _text(),
                description: _text(),
                hypothesis: _text(),
                conclusion: _text(),
                todos: _text(),
                category: '',
                stage: '',
              ),
              assets: [],
            ),
          );
          final source = File('${directory!.path}/original.txt');
          const bytes = [65, 66, 67];
          await source.writeAsBytes(bytes);
          await backend!.editorDraftImports.complete(
            EditorDraftImportRequest(
              cardId: _card,
              draftId: _draft,
              operation: intent.importOperation,
              expectedGeneration: BigInt.one,
              name: 'Recovered attachment.txt',
              kind: 'file',
              bytes: BigInt.from(bytes.length),
              sha256: sha256.convert(bytes).bytes,
            ),
            selectedPath: source.path,
          );
          await source.delete();
          await backend!.editorDraftImports.prepareDecision(intent);
          await backend!.editorDrafts.discard(
            _card,
            _draft,
            BigInt.one,
            'inactive-ui-discard',
          );
          await backend!.close();
          backend = await RustWorkbench.open(
            executable: _host!,
            package: _package!,
            directory: directory!,
          );
          expect(await backend!.editorDrafts.list(), isEmpty);
          storage = await RustStudioStorage.open(backend!);
        });
        await t.pumpWidget(
          MorrowApp(
            storage: storage,
            workbench: backend,
            initialLocale: const Locale('en'),
          ),
        );
        await _wait(
          t,
          () => find
              .byKey(const ValueKey('editor-recovery-open'))
              .evaluate()
              .isNotEmpty,
        );
        await t.tap(find.byKey(const ValueKey('editor-recovery-open')));
        // Native background queries can keep an indicator animated; advance the
        // route while allowing actual process I/O rather than draining fake time.
        await t.pump();
        await t.runAsync(
          () => Future<void>.delayed(const Duration(milliseconds: 40)),
        );
        await t.pump(const Duration(milliseconds: 500));
        expect(
          find.byKey(const ValueKey('editor-recovery-menu-edits')),
          findsOneWidget,
        );
        await t.tap(find.byKey(const ValueKey('editor-recovery-menu-imports')));
        await _wait(
          t,
          () => find.text('Recovered attachment.txt').evaluate().isNotEmpty,
        );
        expect(find.byType(EditorDraftImportRecoveryDialog), findsOneWidget);
        expect(t.takeException(), isNull);
        await t.runAsync(() async {
          final current = await backend!.editorDraftImports.inspectDecision(
            intent,
          );
          expect(
            current.decision!.status,
            EditorDraftImportDecisionStatus.conflict,
          );
          expect(current.decision!.decisionRevision, BigInt.one);
        });
        // Dismissing the review does not cancel or replay any decision.
        Navigator.of(
          t.element(find.byType(EditorDraftImportRecoveryDialog)),
        ).pop();
        // Native background queries can keep an indicator animated; advance the
        // route while allowing actual process I/O rather than draining fake time.
        await t.pump();
        await t.runAsync(
          () => Future<void>.delayed(const Duration(milliseconds: 40)),
        );
        await t.pump(const Duration(milliseconds: 500));
        await t.runAsync(() async {
          expect(
            (await backend!.editorDraftImports.inspectDecision(
              intent,
            )).decision!.status,
            EditorDraftImportDecisionStatus.conflict,
          );
        });
        await t.tap(find.byKey(const ValueKey('editor-recovery-open')));
        // Native background queries can keep an indicator animated; advance the
        // route while allowing actual process I/O rather than draining fake time.
        await t.pump();
        await t.runAsync(
          () => Future<void>.delayed(const Duration(milliseconds: 40)),
        );
        await t.pump(const Duration(milliseconds: 500));
        await t.tap(find.byKey(const ValueKey('editor-recovery-menu-imports')));
        await _wait(
          t,
          () => find.text('Recovered attachment.txt').evaluate().isNotEmpty,
        );
        final cancel = find.byKey(
          const ValueKey('draft-import-recovery-cancel-$_op'),
        );
        await t.ensureVisible(cancel);
        await t.tap(cancel);
        // Native background queries can keep an indicator animated; advance the
        // route while allowing actual process I/O rather than draining fake time.
        await t.pump();
        await t.runAsync(
          () => Future<void>.delayed(const Duration(milliseconds: 40)),
        );
        await t.pump(const Duration(milliseconds: 500));
        await t.runAsync(() async {
          expect(
            (await backend!.editorDraftImports.inspectDecision(
              intent,
            )).decision!.status,
            EditorDraftImportDecisionStatus.conflict,
          );
        });
        await t.tap(
          find.byKey(const ValueKey('draft-import-recovery-cancel-confirm')),
        );
        await _wait(t, () => cancel.evaluate().isEmpty);
        await t.runAsync(() async {
          final current = await backend!.editorDraftImports.inspectDecision(
            intent,
          );
          expect(
            current.decision!.status,
            EditorDraftImportDecisionStatus.cancelled,
          );
          expect(current.decision!.decisionRevision, BigInt.two);
          await expectLater(
            backend!.editorDraftImports.abandon(intent),
            throwsA(isA<EditorDraftImportFailure>()),
          );
        });
        expect(t.takeException(), isNull);
      } finally {
        await t.pumpWidget(const SizedBox());
        await t.runAsync(() async {
          await backend?.close();
          if (directory != null) await directory!.delete(recursive: true);
        });
      }
    },
    skip: !Platform.isWindows || _host == null || _package == null,
    timeout: const Timeout(Duration(minutes: 4)),
  );
}
