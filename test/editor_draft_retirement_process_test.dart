// Actual SQLite retirement rejection, transport loss and independent client
// recovery through the formal handoff and saved-draft menus.
import 'dart:convert';
import 'dart:io';
import 'package:crypto/crypto.dart';
import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_i18n/morrow_i18n.dart';
import 'package:morrow_studio/main.dart';
import 'package:morrow_studio/editor_draft_handoff_recovery_dialog.dart';
import 'package:morrow_studio/plugins/editor_draft.dart';
import 'package:morrow_studio/plugins/editor_draft_codec.dart';
import 'package:morrow_studio/plugins/editor_draft_owner.dart';
import 'package:morrow_studio/plugins/studio_storage.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';
import 'package:morrow_studio/plugins/workbench_channel_native.dart';
import 'editor_draft_review_process_test.dart' as fixture;
import 'editor_draft_resume_process_test.dart' show controllers, assertRaw;
import 'fixtures/editor_draft_reply_faults.dart';

final rootPath = fixture.rootPath, phase = fixture.phase;
final python = Platform.environment['MORROW_CLOSE_TEST_PYTHON'];
final available =
    Platform.isWindows &&
    fixture.hostPath != null &&
    fixture.packagePath != null &&
    rootPath != null &&
    python != null &&
    ['fail', 'recover', 'verify'].contains(phase);
const target = 'formal-parent-retirement-failure';

Future<void> inject(String action, String parent) async {
  final result = await Process.run(python!, [
    '-B',
    File('test/fixtures/editor_draft_retire_fault.py').absolute.path,
    action,
    rootPath!,
    target,
    parent,
  ]);
  expect(result.exitCode, 0, reason: '${result.stdout}\n${result.stderr}');
}

String digest(EditorDraftRecord record) =>
    sha256.convert(EditorDraftCodec.encodeWrite(record.request)).toString();

void main({Future<void> Function(WidgetTester)? prepareWindow}) {
  testWidgets(
    'formal parent retirement $phase across process boundary',
    (t) async {
      if (prepareWindow != null) {
        await prepareWindow(t);
      } else {
        t.view.devicePixelRatio = 1;
        t.view.physicalSize = const Size(1180, 900);
        addTearDown(t.view.reset);
      }
      final root = Directory(rootPath!), store = Directory('$rootPath/store');
      late RustWorkbench host;
      final result = <String, Object?>{
        'pid': pid,
        'complete': false,
        'phase': phase,
      };
      late Map previous;
      if (phase != 'fail') {
        previous = (await t.runAsync(
          () async =>
              jsonDecode(
                    await File(
                      '${root.path}/${phase == 'recover' ? 'fail' : 'recover'}.json',
                    ).readAsString(),
                  )
                  as Map,
        ))!;
        expect(previous['pid'], isNot(pid));
        if (phase == 'recover') {
          await fixture.native(
            t,
            inject('disarm', previous['parent_draft_id'] as String),
          );
        }
      }
      await t.runAsync(() async {
        expect(await store.exists(), phase != 'fail');
        await root.create(recursive: true);
        host = await RustWorkbench.open(
          executable: fixture.hostPath!,
          package: fixture.packagePath!,
          directory: store,
        );
      });
      if (phase == 'fail') {
        late EditorDraftOwner owner;
        late List<TextEditingController> fields;
        final original = await fixture.native(
          t,
          host.openEditor(target, create: true),
        );
        final editor = LostBusinessReply(
          original,
          loseReply: false,
          afterActualSave: () {
            fields[1].value = const TextEditingValue(
              text:
                  'S2 survives failed retirement 😀 [asset](attachment:selected.bin)',
              selection: TextSelection(
                baseOffset: 1,
                extentOffset: 3,
                affinity: TextAffinity.upstream,
                isDirectional: true,
              ),
              composing: TextRange(start: 1, end: 3),
            );
          },
        );
        owner = await fixture.native(
          t,
          host.createEditorDraftOwner(
            editor: editor,
            initial: fixture.initial(),
            attachments: [],
            create: true,
          ),
        );
        await t.runAsync(() async {
          final file = File('${root.path}/selected.bin');
          await file.writeAsBytes([44, 3, 0, 255], flush: true);
          await host.importEditorDraftFile(
            owner,
            path: file.path,
            name: 'selected.bin',
            kind: 'file',
          );
          await file.delete();
        });
        final runtime = FaultDraftRuntime(
          host,
          'actual-retirement-abort',
          beforeRetire: () => inject('arm', owner.session.draftId),
        );
        await t.pumpWidget(
          MaterialApp(
            locale: const Locale('zh'),
            supportedLocales: AppLocalizations.supportedLocales,
            localizationsDelegates: const [
              AppLocalizations.delegate,
              GlobalMaterialLocalizations.delegate,
              GlobalWidgetsLocalizations.delegate,
              GlobalCupertinoLocalizations.delegate,
            ],
            home: Scaffold(
              body: NewIdeaDialog(
                editor: editor,
                draftOwner: owner,
                draftRuntime: runtime,
                initialCategory: '实验',
              ),
            ),
          ),
        );
        await t.pumpAndSettle();
        fields = controllers(t);
        fields[0].text = 'Original committed S1';
        fields[1].text = 'S1 body [asset](attachment:selected.bin)';
        await t.pump();
        await fixture.tapVisible(t, 'idea-save');
        await fixture.waitFor(
          t,
          () =>
              owner.commit?.failure != null &&
              find
                  .byKey(const ValueKey('idea-save-error'))
                  .evaluate()
                  .isNotEmpty,
        );
        final attempt = owner.commit!;
        expect(attempt.stage, 'retire');
        expect(attempt.finished, false);
        expect(editor.saves, 1);
        expect(runtime.acknowledgements, 0);
        expect(runtime.proposals.calls, {
          'prepare': 1,
          'complete': 1,
          'retire': 1,
        });
        final parent = (await fixture.native(
          t,
          host.editorDrafts.read(target, owner.session.draftId),
        ))!;
        final child = (await fixture.native(
          t,
          host.editorDrafts.read(target, attempt.child!.session.draftId),
        ))!;
        expect(parent.currentActive && child.currentActive, true);
        expect(child.request.values.description.text, fields[1].text);
        expect(child.request.values.description.composingEnd, 3);
        expect(child.assets, hasLength(1));
        for (final record in [parent, child]) {
          final path = '${root.path}/before-${record.request.draftId}.bin';
          await fixture.native(
            t,
            host.editorDrafts.exportAsset(
              target,
              record.request.draftId,
              record.generation,
              record.assets.single.selection.assetId,
              path,
            ),
          );
          await t.runAsync(
            () async => expect(await File(path).readAsBytes(), [44, 3, 0, 255]),
          );
        }
        final business = await fixture.native(
          t,
          host.versionedContent.read(target),
        );
        expect(business.revision, BigInt.one);
        await fixture.native(
          t,
          expectLater(host.prepareEditorDraftClose(), throwsStateError),
        );
        await fixture.native(t, expectLater(host.close(), throwsStateError));
        result.addAll({
          'parent_draft_id': parent.request.draftId,
          'child_draft_id': child.request.draftId,
          'child_operation': attempt.proposal!.handoff.request.operation,
          'retirement_operation': attempt.proposal!.retirementOperation,
          'business_operation': owner.session.commitOperation,
          'parent_request_sha256': digest(parent),
          'child_request_sha256': digest(child),
          'business_submissions': editor.saves,
          'business_revision': '1',
          'parent_active': true,
          'child_active': true,
          'normal_close_accepted': false,
          'retirement_error_type': attempt.failure.runtimeType.toString(),
          'fault':
              'exact_parent_SQLite_UPDATE_ABORT_then_original_transport_EOF',
        });
        await t.pumpWidget(const SizedBox());
        // Intentional failure injection through this fixture's original channel.
        // It is not normal workspace close or a retirement proof. No retry occurs.
        final channel = host.channel as NativeWorkbenchChannel;
        result['original_host_pid'] = channel.process.pid;
        await fixture.native(t, channel.closeInput());
        result['original_transport_exit'] = await fixture.native(
          t,
          channel.exitCode,
        );
        expect(result['original_transport_exit'], 0);
      } else {
        final observedId = previous['child_draft_id'] as String;
        final child = (await fixture.native(
          t,
          host.editorDrafts.read(target, observedId),
        ))!;
        expect(digest(child), previous['child_request_sha256']);
        final storage = await fixture.native(t, RustStudioStorage.open(host));
        await t.pumpWidget(
          MorrowApp(
            storage: storage,
            workbench: host,
            initialLocale: const Locale('en'),
            libraryDirectory: store.path,
          ),
        );
        if (phase == 'recover') {
          final parentId = previous['parent_draft_id'] as String;
          final parent = (await fixture.native(
            t,
            host.editorDrafts.read(target, parentId),
          ))!;
          expect(parent.currentActive && child.currentActive, true);
          expect(digest(parent), previous['parent_request_sha256']);
          await fixture.tapVisible(t, 'editor-recovery-open');
          await fixture.tapVisible(t, 'editor-recovery-menu-handoffs');
          await fixture.tapVisible(
            t,
            'draft-handoff-inspect-${previous['child_operation']}',
          );
          await fixture.tapVisible(t, 'draft-handoff-retire');
          await fixture.tapVisible(t, 'draft-handoff-confirm');
          final coordinator = t
              .widget<EditorDraftHandoffRecoveryDialog>(
                find.byType(EditorDraftHandoffRecoveryDialog),
              )
              .coordinator;
          await fixture.waitFor(t, () => !coordinator.busy);
          expect(coordinator.lastError, isNull);
          expect(
            coordinator.lastConfirmed!.summary.status,
            EditorDraftHandoffProposalStatus.parentRetired,
          );
          expect(
            coordinator.lastConfirmed!.proposal.retirementOperation,
            previous['retirement_operation'],
          );
          await fixture.tapVisible(t, 'draft-handoff-close');
          final retired = (await fixture.native(
            t,
            host.editorDrafts.read(target, parentId),
          ))!;
          expect(retired.currentActive, false);
          expect(retired.retirement!.childDraftId, observedId);
        }
        await fixture.tapVisible(t, 'editor-recovery-open');
        await fixture.tapVisible(t, 'editor-recovery-menu-drafts');
        await fixture.tapVisible(t, 'draft-journal-$observedId');
        await fixture.waitFor(
          t,
          () => find.byType(NewIdeaDialog).evaluate().isNotEmpty,
        );
        await t.pump(const Duration(milliseconds: 300));
        assertRaw(t, child.request.values);
        EditorDraftRecord latest = child;
        if (phase == 'recover') {
          await fixture.tapVisible(t, 'idea-resume-live-draft');
          await fixture.waitFor(
            t,
            () => find.byKey(const ValueKey('idea-save')).evaluate().isNotEmpty,
          );
          final resumed = t
              .widget<NewIdeaDialog>(find.byType(NewIdeaDialog))
              .draftOwner!;
          expect(resumed.session.draftId, isNot(observedId));
          assertRaw(t, child.request.values);
          final fields = controllers(t);
          fields[1].value = fields[1].value.copyWith(
            text: '${fields[1].text}\nEdited after failed retirement recovery',
          );
          await t.pump();
          final prepared = await fixture.native(
            t,
            host.prepareEditorDraftClose(),
          );
          latest = resumed.session.confirmed!;
          await t.pumpWidget(const SizedBox());
          await fixture.native(t, host.finishEditorDraftClose(prepared));
        } else {
          await t.pumpWidget(const SizedBox());
        }
        final business = await fixture.native(
          t,
          host.versionedContent.read(target),
        );
        expect(business.revision, BigInt.one);
        final proof = await fixture.native(
          t,
          host.inspectEditorCommit(
            id: target,
            operation: previous['business_operation'] as String,
          ),
        );
        expect(proof.committedRevision, BigInt.one);
        final assetPath = '${root.path}/$phase-export.bin';
        await fixture.native(
          t,
          host.editorDrafts.exportAsset(
            target,
            latest.request.draftId,
            latest.generation,
            latest.assets.single.selection.assetId,
            assetPath,
          ),
        );
        await t.runAsync(
          () async =>
              expect(await File(assetPath).readAsBytes(), [44, 3, 0, 255]),
        );
        await fixture.native(t, host.close());
        result.addAll({
          'prior_pid': previous['pid'],
          'child_draft_id': latest.request.draftId,
          'child_request_sha256': digest(latest),
          'business_operation': previous['business_operation'],
          'business_revision': '1',
          'original_retirement_operation':
              previous['retirement_operation'] ??
              previous['original_retirement_operation'],
          'business_replayed': false,
          'durable_attachment_verified': true,
          'normal_close_accepted': true,
        });
      }
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
