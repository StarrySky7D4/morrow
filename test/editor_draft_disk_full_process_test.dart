// Real ERROR_DISK_FULL on an isolated virtual volume, original host writes,
// formal recovery buttons and a separate native client verifying the commit.
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
import 'package:morrow_studio/plugins/workbench_native.dart';
import 'editor_draft_review_process_test.dart' as fixture;
import 'editor_draft_resume_process_test.dart' show controllers;
import 'fixtures/editor_draft_reply_faults.dart';

final rootPath = fixture.rootPath, phase = fixture.phase;
final python = Platform.environment['MORROW_CLOSE_TEST_PYTHON'];
final volumePlan = Platform.environment['MORROW_DRAFT_VOLUME_PLAN'];
final available =
    Platform.isWindows &&
    fixture.hostPath != null &&
    fixture.packagePath != null &&
    rootPath != null &&
    python != null &&
    volumePlan != null &&
    ['full', 'verify'].contains(phase);
const target = 'formal-draft-disk-full';

String digest(EditorDraftWriteRequest request) =>
    sha256.convert(EditorDraftCodec.encodeWrite(request)).toString();

Future<Map<String, dynamic>> volumeAction(String action) async {
  final result = await Process.run(python!, [
    '-B',
    File('test/fixtures/editor_draft_disk_full.py').absolute.path,
    action,
    volumePlan!,
  ]);
  // This future is observed while the tester pumps frames. Do not call a
  // guarded test API from its asynchronous process-completion callback.
  if (result.exitCode != 0) {
    throw StateError(
      'Private volume helper failed: ${result.stdout}\n${result.stderr}',
    );
  }
  return jsonDecode(
        await File(
          '${File(volumePlan!).parent.path}/$action.json',
        ).readAsString(),
      )
      as Map<String, dynamic>;
}

void main({Future<void> Function(WidgetTester)? prepareWindow}) {
  testWidgets(
    'formal draft disk-full $phase with original operation recovery',
    (t) async {
      if (prepareWindow != null) {
        await prepareWindow(t);
      } else {
        t.view.devicePixelRatio = 1;
        t.view.physicalSize = const Size(1180, 900);
        addTearDown(t.view.reset);
      }
      final root = Directory(rootPath!);
      final plan =
          jsonDecode(
                await File(volumePlan!).readAsString().then(
                  (value) => value.replaceFirst('\ufeff', ''),
                ),
              )
              as Map;
      final store = Directory('${plan['mount']}/store');
      final result = <String, Object?>{
        'pid': pid,
        'phase': phase,
        'complete': false,
      };
      late RustWorkbench host;
      await t.runAsync(() async {
        expect(await store.exists(), phase != 'full');
        await root.create(recursive: true);
        host = await RustWorkbench.open(
          executable: fixture.hostPath!,
          package: fixture.packagePath!,
          directory: store,
        );
      });
      if (phase == 'full') {
        final original = await fixture.native(
          t,
          host.openEditor(target, create: true),
        );
        final editor = LostBusinessReply(original, loseReply: false);
        final owner = await fixture.native(
          t,
          host.createEditorDraftOwner(
            editor: editor,
            initial: fixture.initial(),
            attachments: [],
            create: true,
          ),
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
                draftRuntime: host,
                initialCategory: '实验',
              ),
            ),
          ),
        );
        await t.pumpAndSettle();
        final fields = controllers(t);
        owner.session.pauseAutoSave();
        fields[0].text = 'Original disk-full draft';
        fields[1].text = 'S1 confirmed before the volume fills';
        await t.pump();
        final before = (await fixture.native(
          t,
          owner.session.flushLatestVisible(
            timeout: const Duration(seconds: 20),
          ),
        ))!;
        expect(before.request.values.description.text, fields[1].text);
        final full = await fixture.native(t, volumeAction('fill'));
        expect(full['after']['available'], lessThan(4096));
        expect((full['errors'] as List).last['winerror'], 112);
        final s2 = TextEditingValue(
          text:
              'S2 requires new SQLite pages 😀 ${List.filled(4000, '草稿内容').join()}',
          selection: const TextSelection(
            baseOffset: 1,
            extentOffset: 3,
            affinity: TextAffinity.upstream,
            isDirectional: true,
          ),
          composing: const TextRange(start: 1, end: 3),
        );
        fields[1].value = s2;
        await t.pump();
        await fixture.tapVisible(t, 'idea-save');
        await fixture.waitFor(
          t,
          () =>
              owner.commit?.failure != null &&
              find
                  .byKey(const ValueKey('idea-inspect-draft-write'))
                  .evaluate()
                  .isNotEmpty,
        );
        expect(owner.commit!.stage, 'parent');
        expect(owner.commit!.finished, false);
        expect(owner.session.unknown, true);
        final actualHostFailure = owner.session.lastFailure!.cause.toString();
        // Windows ERROR_DISK_FULL is independently observed by the bounded
        // volume fixture. A failed SQLite commit is correctly Unknown at the
        // product boundary; it must not be reclassified as a proven rollback.
        expect(actualHostFailure, 'Bad state: CommitUnknown');
        expect(editor.saves, 0);
        final pending = owner.session.pendingRequest!;
        final pendingHash = digest(pending);
        expect(pending.values.description.text, s2.text);
        final afterFailure = (await fixture.native(
          t,
          host.editorDrafts.read(target, owner.session.draftId),
        ))!;
        expect(digest(afterFailure.request), digest(before.request));
        expect(afterFailure.generation, before.generation);

        // A newer live edit must survive both inspection and the exact frozen
        // retry. Neither action may submit a business Save.
        final s3 = s2.copyWith(
          text: '${s2.text}\nS3 typed after the failed write',
        );
        fields[1].value = s3;
        await t.pump();
        await fixture.tapVisible(t, 'idea-inspect-draft-write');
        await fixture.waitFor(
          t,
          () =>
              !owner.session.saving &&
              t
                      .widget<TextButton>(
                        find.byKey(const ValueKey('idea-inspect-draft-write')),
                      )
                      .onPressed !=
                  null,
        );
        expect(owner.session.unknown, true);
        expect(identical(owner.session.pendingRequest, pending), true);
        expect(digest(owner.session.pendingRequest!), pendingHash);
        expect(fields[1].value, s3);
        expect(editor.saves, 0);
        final released = await fixture.native(t, volumeAction('release'));
        expect(released['complete'], true);
        await fixture.tapVisible(t, 'idea-retry-draft-write');
        await fixture.waitFor(
          t,
          () => !owner.session.unknown && !owner.session.saving,
        );
        expect(digest(owner.session.confirmed!.request), pendingHash);
        expect(owner.session.pendingRequest, isNull);
        expect(owner.session.current.values.description.text, s3.text);
        expect(fields[1].value, s3);
        expect(owner.session.dirty, true);
        expect(owner.commit!.stage, 'parent');
        expect(editor.saves, 0);
        final retried = (await fixture.native(
          t,
          host.editorDrafts.read(target, owner.session.draftId),
        ))!;
        expect(digest(retried.request), pendingHash);
        expect(retried.generation, before.generation + BigInt.one);
        final operation = owner.session.commitOperation;
        await fixture.tapVisible(t, 'idea-save');
        await fixture.waitFor(t, () => owner.commit?.finished == true);
        expect(owner.commit!.failure, isNull);
        expect(editor.saves, 1);
        final content = await fixture.native(
          t,
          host.versionedContent.read(target),
        );
        final shown = await fixture.native(t, host.workspaceRecord(content));
        expect(shown.description, s3.text);
        expect(content.revision, BigInt.one);
        final proof = await fixture.native(
          t,
          host.inspectEditorCommit(id: target, operation: operation),
        );
        expect(proof.committedRevision, BigInt.one);
        result.addAll({
          'volume_guid': plan['volume_guid'],
          'actual_fill_winerror': 112,
          'actual_host_failure': actualHostFailure,
          'confirmed_S1_sha256': digest(before.request),
          'failed_S2_sha256': pendingHash,
          'exact_original_retry': true,
          'S3_preserved_after_inspect_and_retry': true,
          'business_submissions': editor.saves,
          'business_operation': operation,
          'exact_commit_evidence': proof.digest,
          'business_revision': '1',
          'final_description_sha256': sha256
              .convert(utf8.encode(s3.text))
              .toString(),
        });
      } else {
        final previous =
            jsonDecode(await File('${root.path}/full.json').readAsString())
                as Map;
        expect(previous['pid'], isNot(pid));
        final content = await fixture.native(
          t,
          host.versionedContent.read(target),
        );
        final shown = await fixture.native(t, host.workspaceRecord(content));
        expect(content.revision, BigInt.one);
        expect(
          sha256.convert(utf8.encode(shown.description)).toString(),
          previous['final_description_sha256'],
        );
        expect(
          (await fixture.native(t, host.loadWorkspaceContent())).length,
          1,
        );
        final proof = await fixture.native(
          t,
          host.inspectEditorCommit(
            id: target,
            operation: previous['business_operation'] as String,
          ),
        );
        expect(proof.committedRevision, BigInt.one);
        expect(proof.digest, previous['exact_commit_evidence']);
        result.addAll({
          'previous_pid': previous['pid'],
          'business_submissions': 0,
          'business_revision': '1',
          'exact_commit_evidence': proof.digest,
          'description_sha256': previous['final_description_sha256'],
        });
      }
      final prepared = await fixture.native(t, host.prepareEditorDraftClose());
      await t.pumpWidget(const SizedBox());
      await fixture.native(t, host.finishEditorDraftClose(prepared));
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
