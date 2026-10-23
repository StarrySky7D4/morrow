// Real original host transactions, formal Save and explicit Inspect, with a
// selected acknowledgement withheld after its actual effect. Evidence remains.
import 'dart:convert';
import 'dart:io';
import 'package:crypto/crypto.dart';
import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_i18n/morrow_i18n.dart';
import 'package:morrow_studio/main.dart';
import 'package:morrow_studio/plugins/editor_draft.dart';
import 'package:morrow_studio/plugins/editor_draft_codec.dart';
import 'package:morrow_studio/plugins/editor_draft_owner.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';
import 'editor_draft_review_process_test.dart' as fixture;
import 'editor_draft_resume_process_test.dart' show controllers;
import 'fixtures/editor_draft_reply_faults.dart';

final rootPath = fixture.rootPath, phase = fixture.phase;
final faultStage = phase == 'context' ? 'business' : phase;
final available =
    Platform.isWindows &&
    fixture.hostPath != null &&
    fixture.packagePath != null &&
    rootPath != null &&
    [
      'business',
      'prepare',
      'complete',
      'child',
      'retire',
      'imports',
      'context',
    ].contains(phase);

void main({Future<void> Function(WidgetTester)? prepareWindow}) {
  testWidgets(
    'formal editor inspects original lost $faultStage receipt ($phase)',
    (t) async {
      if (prepareWindow != null) {
        await prepareWindow(t);
      } else {
        t.view.devicePixelRatio = 1;
        t.view.physicalSize = const Size(1180, 900);
        addTearDown(t.view.reset);
      }
      final root = Directory(rootPath!), store = Directory('$rootPath/store');
      final target = 'formal-lost-$phase';
      late RustWorkbench host;
      late EditorDraftOwner owner;
      late LostBusinessReply editor;
      late FaultDraftRuntime runtime;
      late List<TextEditingController> fields;
      final contextChanges = <Map<String, String>>[];
      final afterSubmit = TextEditingValue(
        text: 'S2 during original S1 😀 [asset](attachment:selected.bin)',
        selection: const TextSelection(
          baseOffset: 1,
          extentOffset: 3,
          affinity: TextAffinity.upstream,
          isDirectional: true,
        ),
        composing: const TextRange(start: 1, end: 3),
      );
      await t.runAsync(() async {
        expect(await store.exists(), false);
        await root.create(recursive: true);
        host = await RustWorkbench.open(
          executable: fixture.hostPath!,
          package: fixture.packagePath!,
          directory: store,
        );
        final original = await host.openEditor(target, create: true);
        editor = LostBusinessReply(
          original,
          loseReply: faultStage == 'business',
          afterActualSave: () => fields[1].value = afterSubmit,
        );
        owner = await host.createEditorDraftOwner(
          editor: editor,
          initial: fixture.initial(),
          attachments: [],
          create: true,
        );
        final selected = File('${root.path}/selected.bin');
        await selected.writeAsBytes([31, 4, 255, 7], flush: true);
        await host.importEditorDraftFile(
          owner,
          path: selected.path,
          name: 'selected.bin',
          kind: 'file',
        );
        await selected.delete();
        runtime = FaultDraftRuntime(host, faultStage!);
      });
      Widget view({Locale locale = const Locale('zh'), String? font}) =>
          MaterialApp(
            locale: locale,
            theme: font == null ? null : ThemeData(fontFamily: font),
            supportedLocales: AppLocalizations.supportedLocales,
            localizationsDelegates: const [
              AppLocalizations.delegate,
              GlobalMaterialLocalizations.delegate,
              GlobalWidgetsLocalizations.delegate,
              GlobalCupertinoLocalizations.delegate,
            ],
            home: Scaffold(
              body: NewIdeaDialog(
                key: ValueKey(target),
                editor: owner.editor,
                draftOwner: owner,
                draftRuntime: runtime,
                initialCategory: '实验',
              ),
            ),
          );
      await t.pumpWidget(view());
      await t.pumpAndSettle();
      fields = controllers(t);
      fields[0].text = 'Formal original S1';
      fields[1].text = 'S1 body [asset](attachment:selected.bin)';
      fields[2].text = '  hypothesis\n';
      fields[3].text = '\nconclusion  ';
      fields[4].text = 'one\n\none\ntwo';
      await t.pump();
      await fixture.tapVisible(t, 'idea-save');
      await fixture.waitFor(
        t,
        () =>
            owner.commit?.failure != null &&
            find.byKey(const ValueKey('idea-save-error')).evaluate().isNotEmpty,
      );
      final attempt = owner.commit!;
      expect(attempt.stage, faultStage);
      expect(editor.saves, 1);
      expect(runtime.begins, 1);
      expect(runtime.acknowledgements, 0);
      await fixture.native(
        t,
        expectLater(host.prepareEditorDraftClose(), throwsStateError),
      );
      await fixture.native(t, expectLater(host.close(), throwsStateError));
      expect(host.editorDraftWorkspace.disposed, false);
      final actualS1 = await fixture.native(
        t,
        host.versionedContent.read(target),
      );
      expect(actualS1.revision, BigInt.one);
      final proof = await fixture.native(
        t,
        host.inspectEditorCommit(
          id: target,
          operation: owner.session.commitOperation,
        ),
      );
      expect(proof.committedRevision, BigInt.one);
      // Inspecting the durable original is read-only and leaves the live UI intact.
      expect(fields[1].value, afterSubmit);
      if (phase == 'context') {
        final originalControllers = List<TextEditingController>.of(fields);
        final originalRequest = EditorDraftCodec.encodeWrite(
          attempt.frozenParent!.request,
        );
        for (final configuration in const [
          {'locale': 'en', 'font': 'Courier New'},
          {'locale': 'ja', 'font': 'Arial'},
        ]) {
          await t.pumpWidget(
            view(
              locale: Locale(configuration['locale']!),
              font: configuration['font']!,
            ),
          );
          await t.pumpAndSettle();
          final context = t.element(find.byType(NewIdeaDialog));
          expect(
            Localizations.localeOf(context).languageCode,
            configuration['locale'],
          );
          expect(
            Theme.of(context).textTheme.bodyMedium!.fontFamily,
            configuration['font'],
          );
          final rebuilt = controllers(t);
          expect(rebuilt.length, originalControllers.length);
          for (var i = 0; i < rebuilt.length; i++) {
            expect(rebuilt[i], same(originalControllers[i]));
          }
          expect(rebuilt[1].value, afterSubmit);
          expect(owner.commit, same(attempt));
          expect(
            EditorDraftCodec.encodeWrite(attempt.frozenParent!.request),
            originalRequest,
          );
          expect(host.editorDraftWorkspace.attachedViews, 1);
          expect(editor.saves, 1);
          expect(runtime.begins, 1);
          expect(runtime.acknowledgements, 0);
          expect(runtime.proposals.calls, isEmpty);
          expect(t.takeException(), isNull);
          contextChanges.add(configuration);
        }
      }
      fields[1].value = afterSubmit.copyWith(
        text: 'S2 after lost $phase 😀 [asset](attachment:selected.bin)',
      );
      final latest = fields[1].value;
      await t.pump();
      // Same production Save control now resolves its retained original attempt.
      await fixture.tapVisible(t, 'idea-save');
      await fixture.waitFor(t, () => attempt.finished);
      expect(attempt.failure, isNull);
      expect(editor.saves, 1);
      expect(runtime.begins, 1);
      expect(runtime.acknowledgements, 1);
      expect(runtime.proposals.calls, {
        'prepare': 1,
        'complete': 1,
        'retire': 1,
      });
      if (phase == 'imports') {
        expect(runtime.importCleanup.reconciles, 1);
        expect(runtime.importCleanup.inspections, 1);
      }
      final child = attempt.child!;
      expect(host.editorDraftWorkspace.hasView(child.session), true);
      expect(controllers(t)[1].value, latest);
      final saved = (await fixture.native(
        t,
        host.editorDrafts.read(target, child.session.draftId),
      ))!;
      expect(saved.request.values.description.text, latest.text);
      expect(
        saved.request.values.description.composingEnd,
        latest.composing.end,
      );
      expect(saved.assets, hasLength(1));
      final parent = (await fixture.native(
        t,
        host.editorDrafts.read(target, owner.session.draftId),
      ))!;
      expect(parent.currentActive, false);
      expect(parent.retirement!.childDraftId, child.session.draftId);
      final original = attempt.frozenParent!.request;
      await fixture.native(
        t,
        expectLater(
          host.editorDrafts.save(
            EditorDraftWriteRequest(
              cardId: original.cardId,
              draftId: original.draftId,
              operation: 'late-parent-after-$phase',
              expectedGeneration: attempt.frozenParent!.generation,
              sourceRevision: original.sourceRevision,
              sourceKind: original.sourceKind,
              commitOperation: original.commitOperation,
              captureScope: original.captureScope,
              captureEvents: original.captureEvents,
              predecessorOperation: original.predecessorOperation,
              predecessorDigest: original.predecessorDigest,
              values: original.values,
              assets: original.assets,
            ),
          ),
          throwsA(anything),
        ),
      );
      if (phase == 'child') {
        expect(runtime.childWrites.operations, hasLength(2));
        expect(runtime.childWrites.operations.toSet(), hasLength(2));
      }
      final prepared = await fixture.native(t, host.prepareEditorDraftClose());
      await t.pumpWidget(const SizedBox());
      await fixture.native(t, host.finishEditorDraftClose(prepared));
      await fixture.native(t, host.close());
      // A fresh actual host reopens only the durable library, not the old owner.
      final reopened = await fixture.native(
        t,
        RustWorkbench.open(
          executable: fixture.hostPath!,
          package: fixture.packagePath!,
          directory: store,
        ),
      );
      final restored = (await fixture.native(
        t,
        reopened.editorDrafts.read(target, child.session.draftId),
      ))!;
      expect(
        EditorDraftCodec.encodeWrite(restored.request),
        EditorDraftCodec.encodeWrite(saved.request),
      );
      final business = await fixture.native(
        t,
        reopened.versionedContent.read(target),
      );
      expect(business.revision, BigInt.one);
      final shown = await fixture.native(t, reopened.workspaceRecord(business));
      expect(shown.attachments, hasLength(1));
      await t.runAsync(() async {
        expect(
          await File(shown.attachments.single.source.location).readAsBytes(),
          [31, 4, 255, 7],
        );
        expect(await File('${root.path}/selected.bin').exists(), false);
      });
      final restoredProof = await fixture.native(
        t,
        reopened.inspectEditorCommit(
          id: target,
          operation: original.commitOperation,
        ),
      );
      expect(restoredProof.digest, proof.digest);
      await fixture.native(t, reopened.close());
      await t.runAsync(
        () => File('${root.path}/$phase.json').writeAsString(
          jsonEncode({
            'pid': pid,
            'complete': true,
            'phase': phase,
            'fault_stage': faultStage,
            'context_changes': contextChanges,
            'raw_controllers_preserved_during_context_changes':
                phase == 'context',
            'card_id': target,
            'parent_draft_id': original.draftId,
            'child_draft_id': saved.request.draftId,
            'business_operation': original.commitOperation,
            'business_submissions': editor.saves,
            'business_revision': business.revision.toString(),
            'commit_digest': proof.digest,
            'proposal_calls': runtime.proposals.calls,
            'import_cleanup_dispatches': runtime.importCleanup.reconciles,
            'import_cleanup_inspections': runtime.importCleanup.inspections,
            'acknowledgements': runtime.acknowledgements,
            'child_write_operations': runtime.childWrites.operations,
            'child_request_sha256': sha256
                .convert(EditorDraftCodec.encodeWrite(saved.request))
                .toString(),
            'latest_raw_text': latest.text,
            'parent_retired': true,
            'late_parent_refused': true,
            'original_file_deleted': true,
            'host_reopened_and_verified': true,
            'fault': 'withheld_actual_returned_receipt',
          }),
          flush: true,
        ),
      );
    },
    skip: !available,
    timeout: const Timeout(Duration(minutes: 5)),
  );
}
