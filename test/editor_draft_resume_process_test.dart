// Run each phase in a distinct real Windows application. Only the isolated
// library and evidence files cross process boundaries; no capture owner does.
import 'dart:convert';
import 'dart:io';
import 'package:crypto/crypto.dart';
import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_i18n/morrow_i18n.dart';
import 'package:morrow_studio/main.dart';
import 'package:morrow_studio/card_tips_editor.dart';
import 'package:morrow_studio/fonts/font_choice.dart';
import 'package:morrow_studio/plugins/editor_draft.dart';
import 'package:morrow_studio/plugins/editor_draft_binding.dart';
import 'package:morrow_studio/plugins/editor_draft_codec.dart';
import 'package:morrow_studio/plugins/editor_draft_owner.dart';
import 'package:morrow_studio/plugins/editor_session.dart';
import 'package:morrow_studio/plugins/studio_storage.dart';
import 'package:morrow_studio/plugins/versioned_editor_adapter.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';
import 'package:morrow_studio/plugins/workbench_backend.dart';
import 'editor_draft_review_process_test.dart' as fixture;

final rootPath = fixture.rootPath, phase = fixture.phase;
final available =
    Platform.isWindows &&
    fixture.hostPath != null &&
    fixture.packagePath != null &&
    rootPath != null &&
    [
      'new-seed',
      'new-edit',
      'new-commit',
      'new-verify',
      'v2-seed',
      'v2-edit',
      'v2-commit',
      'v2-verify',
    ].contains(phase);
List<TextEditingController> controllers(WidgetTester t) => [
  for (final key in [
    'idea-title',
    'idea-description',
    'experiment-hypothesis',
    'experiment-conclusion',
  ])
    t.widget<TextField>(find.byKey(ValueKey(key))).controller!,
  if (find.byKey(const ValueKey('idea-todos')).evaluate().isNotEmpty)
    t
        .widget<CardTipsEditor>(find.byKey(const ValueKey('idea-todos')))
        .controller,
];

void assertRaw(WidgetTester t, EditorDraftValues expected) {
  final fields = controllers(t);
  final raw = [
    expected.title,
    expected.description,
    expected.hypothesis,
    expected.conclusion,
    expected.todos,
  ];
  for (var i = 0; i < fields.length; i++) {
    expect(
      fields[i].value,
      EditorDraftBinding.editingValue(raw[i]),
      reason: 'raw field $i changed',
    );
  }
}

void main({Future<void> Function(WidgetTester)? prepareWindow}) {
  testWidgets(
    'formal editable resume $phase across process restart',
    (t) async {
      if (prepareWindow != null) {
        await prepareWindow(t);
      } else {
        t.view.devicePixelRatio = 1;
        t.view.physicalSize = const Size(1180, 900);
        addTearDown(t.view.reset);
      }
      final root = Directory(rootPath!), store = Directory('$rootPath/store');
      final versioned = phase!.startsWith('v2-');
      final action = phase!.split('-').last;
      final target = versioned ? 'resume-formal-v2' : 'resume-formal-new';
      final prefix = versioned ? 'v2' : 'new';
      late RustWorkbench host;
      late RustStudioStorage storage;
      EditorDraftOwner? owner;
      Idea? baseline;
      await t.runAsync(() async {
        expect(await store.exists(), action != 'seed');
        host = await RustWorkbench.open(
          executable: fixture.hostPath!,
          package: fixture.packagePath!,
          directory: store,
        );
        if (action == 'seed') {
          late WorkbenchEditorSession editor;
          if (versioned) {
            await host.apply(
              PluginAction.create,
              Idea(
                'V2 baseline',
                'original body',
                '实验',
                Idea.icons[0],
                const Color(0xff8866aa),
                id: target,
                stage: '待验证',
              ),
            );
            await host.versionedContent.migrate(
              await host.versionedContent.planMigration(target),
            );
            final source = await host.versionedContent.read(target);
            baseline = await host.workspaceRecord(source);
            editor = VersionedWorkbenchEditorAdapter(
              await host.openVersionedEditor(
                target,
                expectedRevision: source.revision,
              ),
              baseline!.versioned!,
              host.workspaceRecord,
              deferRecoveryAcknowledgement: true,
            );
          } else {
            editor = await host.openEditor(target, create: true);
          }
          owner = await host.createEditorDraftOwner(
            editor: editor,
            initial: fixture.initial(),
            attachments: [],
            create: !versioned,
          );
          final selected = File('${root.path}/selected.bin');
          await selected.writeAsBytes([0, 255, 42, 9], flush: true);
          await host.importEditorDraftFile(
            owner!,
            path: selected.path,
            name: 'selected.bin',
            kind: 'file',
          );
          await selected.delete();
        }
        storage = await RustStudioStorage.open(host);
      });
      final result = <String, Object?>{
        'pid': pid,
        'complete': false,
        'phase': phase,
        'versioned': versioned,
      };
      if (action == 'seed') {
        await t.pumpWidget(
          MaterialApp(
            locale: const Locale('en'),
            supportedLocales: AppLocalizations.supportedLocales,
            localizationsDelegates: const [
              AppLocalizations.delegate,
              GlobalMaterialLocalizations.delegate,
              GlobalWidgetsLocalizations.delegate,
              GlobalCupertinoLocalizations.delegate,
            ],
            home: Scaffold(
              body: NewIdeaDialog(
                initialIdea: baseline,
                editor: owner!.editor,
                draftOwner: owner,
                draftRuntime: host,
                initialCategory: '实验',
              ),
            ),
          ),
        );
        await t.pumpAndSettle();
        final fields = controllers(t);
        fields[0].text = '';
        fields[1].text = 'x😀y raw [asset](attachment:selected.bin)';
        fields[2].text = '  hypothesis\n';
        fields[3].text = '\nconclusion  ';
        if (!versioned) fields[4].text = 'one\n\none\ntwo';
        final capture = await fixture.native(
          t,
          owner!.editor.studio.capture('html', '<strong>captured</strong>'),
        );
        final before = fields[1].text;
        await fixture.native(
          t,
          owner!.recordPaste(
            PasteInsertion(
              id: 'resume-formal-rich-paste',
              field: 'description',
              before: before,
              startUtf16: before.length,
              endUtf16: before.length,
              parts: [
                const PastePart.literal('\n'),
                PastePart.ticket(capture.ticket!),
              ],
              after: '$before\n${capture.markdown}',
            ),
          ),
        );
        fields[1].value = fields[1].value.copyWith(
          selection: const TextSelection(
            baseOffset: 1,
            extentOffset: 3,
            affinity: TextAffinity.upstream,
            isDirectional: true,
          ),
          composing: const TextRange(start: 1, end: 3),
        );
        await t.pump();
        final prepared = await fixture.native(
          t,
          host.prepareEditorDraftClose(),
        );
        final saved = owner!.session.confirmed!;
        expect(saved.request.values.title.text, '');
        expect(saved.request.captureEvents, 1);
        expect(saved.assets, hasLength(1));
        result.addAll({
          'draft_id': saved.request.draftId,
          'save_operation': saved.request.operation,
          'business_operation': saved.request.commitOperation,
          'capture_scope': saved.request.captureScope,
          'generation': saved.generation.toString(),
          'request_sha256': sha256
              .convert(EditorDraftCodec.encodeWrite(saved.request))
              .toString(),
          'journal_business_submissions': 0,
        });
        await t.pumpWidget(const SizedBox());
        await fixture.native(t, host.finishEditorDraftClose(prepared));
      } else if (action == 'verify') {
        final committed =
            jsonDecode(
                  await File('${root.path}/$prefix-commit.json').readAsString(),
                )
                as Map;
        expect(committed['pid'], isNot(pid));
        final current = await fixture.native(
          t,
          host.versionedContent.read(target),
        );
        final shown = await fixture.native(t, host.workspaceRecord(current));
        expect(shown.title, 'Saved after two restarts');
        expect(current.revision, BigInt.from(versioned ? 3 : 1));
        expect(shown.attachments, hasLength(1));
        await t.runAsync(() async {
          expect(
            await File(shown.attachments.single.source.location).readAsBytes(),
            [0, 255, 42, 9],
          );
        });
        final proof = await fixture.native(
          t,
          host.inspectEditorCommit(
            id: target,
            operation: committed['business_operation'] as String,
          ),
        );
        expect(proof.committedRevision, current.revision);
        expect(
          (await fixture.native(t, host.loadWorkspaceContent())).length,
          1,
        );
        result.addAll({
          'business_operation': proof.operation,
          'business_revision': current.revision.toString(),
          'exact_commit_evidence': proof.digest,
          'attachment_sha256': sha256.convert([0, 255, 42, 9]).toString(),
          'journal_business_submissions': 0,
          'prior_commit_pid': committed['pid'],
        });
      } else {
        final rows = await fixture.native(t, host.editorDrafts.list());
        final original = rows.singleWhere((row) => row.active);
        final before = (await fixture.native(
          t,
          host.editorDrafts.read(original.cardId, original.draftId),
        ))!;
        final priorFile = File(
          '${root.path}/$prefix-${action == 'edit' ? 'seed' : 'edit'}.json',
        );
        final prior = jsonDecode(await priorFile.readAsString()) as Map;
        expect(prior['pid'], isNot(pid));
        expect(prior['draft_id'], before.request.draftId);
        expect(
          prior['request_sha256'],
          sha256
              .convert(EditorDraftCodec.encodeWrite(before.request))
              .toString(),
        );
        await t.pumpWidget(
          MorrowApp(
            storage: storage,
            workbench: host,
            initialLocale: Locale(action == 'edit' ? 'zh' : 'en'),
            libraryDirectory: store.path,
          ),
        );
        await fixture.tapVisible(t, 'editor-recovery-open');
        await fixture.tapVisible(t, 'editor-recovery-menu-drafts');
        await fixture.tapVisible(t, 'draft-journal-${original.draftId}');
        await fixture.waitFor(
          t,
          () => find.byType(NewIdeaDialog).evaluate().isNotEmpty,
        );
        await t.pump(const Duration(milliseconds: 300));
        assertRaw(t, before.request.values);
        expect(find.byKey(const ValueKey('idea-save')), findsNothing);
        await fixture.tapVisible(t, 'idea-resume-live-draft');
        await fixture.waitFor(
          t,
          () => find.byKey(const ValueKey('idea-save')).evaluate().isNotEmpty,
        );
        owner = t.widget<NewIdeaDialog>(find.byType(NewIdeaDialog)).draftOwner!;
        expect(owner!.session.draftId, isNot(original.draftId));
        expect(
          owner!.captureIdentity.scope,
          isNot(before.request.captureScope),
        );
        expect(owner!.captureIdentity.events, 0);
        assertRaw(t, before.request.values);
        final parent = (await fixture.native(
          t,
          host.editorDrafts.read(original.cardId, original.draftId),
        ))!;
        expect(parent.currentActive, false);
        expect(parent.retirement!.childDraftId, owner!.session.draftId);
        final views = owner!.catalog.viewsForSelections(
          owner!.session.current.assets,
        );
        await t.runAsync(() async {
          expect(await File('${root.path}/selected.bin').exists(), false);
          expect(
            await File(views.single.attachment.source.location).readAsBytes(),
            [0, 255, 42, 9],
          );
        });
        final fields = controllers(t);
        if (action == 'edit') {
          fields[1].value = fields[1].value.copyWith(
            text: '${fields[1].text}\nEdited after restart',
            selection: const TextSelection(
              baseOffset: 1,
              extentOffset: 3,
              affinity: TextAffinity.upstream,
              isDirectional: true,
            ),
            composing: const TextRange(start: 1, end: 3),
          );
          await t.pump();
          // Change actual native preferences while the same original editor is
          // retained, then verify the complete draft after another process start.
          await fixture.native(
            t,
            host.saveUiFont(const FontChoice(family: 'Arial')),
          );
          await fixture.native(t, host.saveUiLocale('ja'));
          expect(owner!.session.current.values.title.text, '');
          final prepared = await fixture.native(
            t,
            host.prepareEditorDraftClose(),
          );
          final saved = owner!.session.confirmed!;
          result.addAll({
            'draft_id': saved.request.draftId,
            'save_operation': saved.request.operation,
            'business_operation': saved.request.commitOperation,
            'capture_scope': saved.request.captureScope,
            'generation': saved.generation.toString(),
            'request_sha256': sha256
                .convert(EditorDraftCodec.encodeWrite(saved.request))
                .toString(),
            'parent_draft_id': original.draftId,
            'parent_retired': true,
            'prior_pid': prior['pid'],
            'journal_business_submissions': 0,
            'font_preference': 'Arial',
            'locale_preference': 'ja',
          });
          await t.pumpWidget(const SizedBox());
          await fixture.native(t, host.finishEditorDraftClose(prepared));
        } else {
          fields[0].text = 'Saved after two restarts';
          await t.pump();
          final operation = owner!.session.commitOperation;
          await fixture.tapVisible(t, 'idea-save');
          await fixture.waitFor(t, () => owner!.commit?.finished == true);
          expect(owner!.commit!.failure, isNull);
          final current = await fixture.native(
            t,
            host.versionedContent.read(target),
          );
          expect(current.revision, BigInt.from(versioned ? 3 : 1));
          final proof = await fixture.native(
            t,
            host.inspectEditorCommit(id: target, operation: operation),
          );
          expect(proof.committedRevision, current.revision);
          final shown = await fixture.native(t, host.workspaceRecord(current));
          expect(shown.title, 'Saved after two restarts');
          expect(shown.attachments, hasLength(1));
          await t.runAsync(() async {
            expect(
              await File(
                shown.attachments.single.source.location,
              ).readAsBytes(),
              [0, 255, 42, 9],
            );
          });
          final prepared = await fixture.native(
            t,
            host.prepareEditorDraftClose(),
          );
          await t.pumpWidget(const SizedBox());
          await fixture.native(t, host.finishEditorDraftClose(prepared));
          result.addAll({
            'draft_id': owner!.session.draftId,
            'business_operation': operation,
            'business_revision': current.revision.toString(),
            'exact_commit_evidence': proof.digest,
            'parent_retired': true,
            'prior_pid': prior['pid'],
            'journal_business_submissions': 1,
          });
        }
      }
      await fixture.native(t, host.close());
      result['complete'] = true;
      await t.runAsync(
        () => File(
          '${root.path}/$phase.json',
        ).writeAsString(jsonEncode(result), flush: true),
      );
    },
    skip: !available,
    timeout: const Timeout(Duration(minutes: 5)),
  );
}
