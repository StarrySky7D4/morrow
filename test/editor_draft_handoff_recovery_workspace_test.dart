import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'package:crypto/crypto.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/main.dart';
import 'package:morrow_studio/editor_draft_handoff_recovery_dialog.dart';
import 'package:morrow_studio/plugins/editor_draft.dart';
import 'package:morrow_studio/plugins/editor_draft_codec.dart';
import 'package:morrow_studio/plugins/studio_storage.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';
import 'fixtures/editor_draft_handoff_proposal_fixture.dart';

final hostPath = Platform.environment['MORROW_WORKBENCH_HOST'];
final packagePath = Platform.environment['MORROW_WORKBENCH_PACKAGE'];
final rootPath = Platform.environment['MORROW_DRAFT_REVIEW_ROOT'];
final phase = Platform.environment['MORROW_DRAFT_REVIEW_PHASE'];
final available = Platform.isWindows && hostPath != null && packagePath != null;
Future<void> waitFor(WidgetTester t, bool Function() ready) async {
  final deadline = DateTime.now().add(const Duration(seconds: 45));
  while (DateTime.now().isBefore(deadline)) {
    await t.runAsync(
      () => Future<void>.delayed(const Duration(milliseconds: 20)),
    );
    await t.pump(const Duration(milliseconds: 50));
    final error = t.takeException();
    if (error != null) throw error;
    if (ready()) return;
  }
  expect(ready(), isTrue, reason: 'Native handoff recovery UI did not settle');
}

Future<T> native<T>(WidgetTester t, Future<T> Function() action) async {
  late T value;
  // Host work queued by a widget may await microtasks in Flutter's fake zone.
  // Keep pumping that zone while real process I/O progresses, instead of
  // waiting inside runAsync on work whose predecessor cannot advance.
  var done = false;
  Object? failure;
  StackTrace? stack;
  unawaited(
    Future.sync(action).then<void>(
      (result) {
        value = result;
        done = true;
      },
      onError: (Object error, StackTrace trace) {
        failure = error;
        stack = trace;
        done = true;
      },
    ),
  );
  await waitFor(t, () => done);
  if (failure != null) Error.throwWithStackTrace(failure!, stack!);
  return value;
}

Future<void> tapNative(WidgetTester t, String key) async {
  final target = find.byKey(ValueKey(key));
  // A retained detail can be visible while read-only refresh still owns the
  // coordinator. Click only when the real button is enabled, not after a fixed
  // sleep that varies with host startup and parallel test load.
  await waitFor(t, () {
    if (target.evaluate().length != 1) return false;
    final widget = t.widget(target);
    return widget is! ButtonStyleButton || widget.onPressed != null;
  });
  await t.ensureVisible(target);
  await t.tap(target);
  await t.pump();
  await t.runAsync(
    () => Future<void>.delayed(const Duration(milliseconds: 50)),
  );
  await t.pump(const Duration(milliseconds: 500));
}

void main({Future<void> Function(WidgetTester)? prepareWindow}) {
  testWidgets(
    'real workspace discovers a persisted handoff and explicitly completes then retires it',
    (t) async {
      if (prepareWindow != null) {
        await prepareWindow(t);
      } else {
        t.view.devicePixelRatio = 1;
        t.view.physicalSize = const Size(1180, 850);
        addTearDown(t.view.reset);
      }
      RustWorkbench? backend;
      RustWorkbench? other;
      RustStudioStorage? storage;
      RustStudioStorage? otherStorage;
      late Directory directory;
      late EditorDraftHandoffProposal proposal;
      var completed = false;
      try {
        await t.runAsync(() async {
          directory = rootPath == null
              ? await Directory.systemTemp.createTemp(
                  'morrow-handoff-recovery-ui-',
                )
              : Directory('$rootPath/store-a');
          if (rootPath != null) {
            expect(await directory.exists(), false);
            await directory.create(recursive: true);
          }
          backend = await RustWorkbench.open(
            executable: hostPath!,
            package: packagePath!,
            directory: directory,
          );
          proposal = await seedHandoffProposal(backend!, directory);
          await backend!.editorDraftHandoffProposals.prepare(proposal);
          await backend!.close();
          backend = await RustWorkbench.open(
            executable: hostPath!,
            package: packagePath!,
            directory: directory,
          );
          storage = await RustStudioStorage.open(backend!);
          final otherDirectory = Directory('${directory.path}-other');
          expect(await otherDirectory.exists(), false);
          other = await RustWorkbench.open(
            executable: hostPath!,
            package: packagePath!,
            directory: otherDirectory,
          );
          otherStorage = await RustStudioStorage.open(other!);
          expect(await other!.loadWorkspaceContent(), isEmpty);
        });
        await t.pumpWidget(
          MorrowApp(
            storage: storage,
            workbench: backend,
            initialLocale: const Locale('en'),
          ),
        );
        await waitFor(
          t,
          () => find
              .byKey(const ValueKey('editor-recovery-open'))
              .evaluate()
              .isNotEmpty,
        );
        await tapNative(t, 'editor-recovery-open');
        await tapNative(t, 'editor-recovery-menu-handoffs');
        final inspectKey =
            'draft-handoff-inspect-${proposal.handoff.request.operation}';
        await waitFor(
          t,
          () => find.byKey(ValueKey(inspectKey)).evaluate().isNotEmpty,
        );
        await tapNative(t, inspectKey);
        await waitFor(
          t,
          () => find
              .byKey(const ValueKey('draft-handoff-complete'))
              .evaluate()
              .isNotEmpty,
        );
        expect(
          await native(
            t,
            () => backend!.editorDrafts.read(
              proposal.handoff.request.cardId,
              proposal.handoff.request.draftId,
            ),
          ),
          isNull,
        );
        await tapNative(t, 'draft-handoff-close');
        await tapNative(t, 'editor-recovery-open');
        await tapNative(t, 'editor-recovery-menu-handoffs');
        await waitFor(
          t,
          () => find
              .byKey(const ValueKey('draft-handoff-complete'))
              .evaluate()
              .isNotEmpty,
        );
        await tapNative(t, 'draft-handoff-complete');
        expect(
          await native(
            t,
            () => backend!.editorDrafts.read(
              proposal.handoff.request.cardId,
              proposal.handoff.request.draftId,
            ),
          ),
          isNull,
        );
        await tapNative(t, 'draft-handoff-confirm');
        await waitFor(
          t,
          () => find
              .byKey(const ValueKey('draft-handoff-retire'))
              .evaluate()
              .isNotEmpty,
        );
        expect(
          (await native(
            t,
            () => backend!.editorDrafts.read(
              proposal.handoff.request.cardId,
              proposal.handoff.parentLink.parentDraftId,
            ),
          ))!.active,
          isTrue,
        );
        await tapNative(t, 'draft-handoff-retire');
        expect(
          (await native(
            t,
            () => backend!.editorDrafts.read(
              proposal.handoff.request.cardId,
              proposal.handoff.request.draftId,
            ),
          ))!.active,
          isTrue,
        );
        // Switching backend while the confirmation is open removes both owned
        // routes. It must not leave old-library raw text over the new workspace.
        await t.pumpWidget(
          MorrowApp(
            storage: otherStorage,
            workbench: other,
            initialLocale: const Locale('en'),
          ),
        );
        expect(await native(t, () => other!.loadWorkspaceContent()), isEmpty);
        expect(
          await native(
            t,
            () => other!.editorDrafts.read(
              proposal.handoff.request.cardId,
              proposal.handoff.request.draftId,
            ),
          ),
          isNull,
        );
        await waitFor(
          t,
          () =>
              find
                  .byType(EditorDraftHandoffRecoveryDialog)
                  .evaluate()
                  .isEmpty &&
              find
                  .byKey(const ValueKey('draft-handoff-confirm'))
                  .evaluate()
                  .isEmpty,
        );
        expect(
          (await native(
            t,
            () => backend!.editorDrafts.read(
              proposal.handoff.request.cardId,
              proposal.handoff.parentLink.parentDraftId,
            ),
          ))!.active,
          isTrue,
        );
        await t.pumpWidget(
          MorrowApp(
            storage: storage,
            workbench: backend,
            initialLocale: const Locale('en'),
          ),
        );
        await tapNative(t, 'editor-recovery-open');
        await tapNative(t, 'editor-recovery-menu-handoffs');
        await waitFor(
          t,
          () => find.byKey(ValueKey(inspectKey)).evaluate().isNotEmpty,
        );
        await tapNative(t, inspectKey);
        await waitFor(
          t,
          () => find
              .byKey(const ValueKey('draft-handoff-retire'))
              .evaluate()
              .isNotEmpty,
        );
        await tapNative(t, 'draft-handoff-retire');
        await tapNative(t, 'draft-handoff-confirm');
        await waitFor(
          t,
          () => find
              .byKey(const ValueKey('draft-handoff-retire'))
              .evaluate()
              .isEmpty,
        );
        final record = await native(
          t,
          () => backend!.editorDraftHandoffProposals.inspect(
            cardId: proposal.handoff.request.cardId,
            parentDraftId: proposal.handoff.parentLink.parentDraftId,
            childOperation: proposal.handoff.request.operation,
          ),
        );
        expect(
          record!.summary.status,
          EditorDraftHandoffProposalStatus.parentRetired,
        );
        final child = await native(
          t,
          () => backend!.editorDrafts.read(
            proposal.handoff.request.cardId,
            proposal.handoff.request.draftId,
          ),
        );
        expect(child!.request.values.title.text, proposalRawTitle());
        final source = await native(
          t,
          () => backend!.versionedContent.read(proposal.handoff.request.cardId),
        );
        expect(source.revision, proposal.handoff.request.sourceRevision);
        expect(t.takeException(), isNull);
        final childBytes = EditorDraftCodec.encodeWrite(child.request);
        await t.pumpWidget(const SizedBox());
        await native(t, () => backend!.close());
        backend = await native(
          t,
          () => RustWorkbench.open(
            executable: hostPath!,
            package: packagePath!,
            directory: directory,
          ),
        );
        final reopenedChild = await native(
          t,
          () => backend!.editorDrafts.read(
            proposal.handoff.request.cardId,
            proposal.handoff.request.draftId,
          ),
        );
        final reopenedParent = await native(
          t,
          () => backend!.editorDrafts.read(
            proposal.handoff.request.cardId,
            proposal.handoff.parentLink.parentDraftId,
          ),
        );
        expect(
          EditorDraftCodec.encodeWrite(reopenedChild!.request),
          childBytes,
        );
        expect(reopenedChild.active, isTrue);
        expect(reopenedParent!.active, isFalse);
        expect(
          reopenedParent.retirement!.childDraftId,
          reopenedChild.request.draftId,
        );
        expect(
          (await native(
            t,
            () => backend!.versionedContent.read(source.id),
          )).revision,
          source.revision,
        );
        expect(await native(t, () => other!.loadWorkspaceContent()), isEmpty);
        completed = true;
      } finally {
        // A failed qualification retains its owners and library for diagnosis;
        // only the completed original scenario closes these transports.
        if (completed) {
          await t.pumpWidget(const SizedBox());
          await native(t, () async {
            await backend?.close();
            await other?.close();
          });
        }
      }
      if (rootPath != null) {
        await t.runAsync(
          () => File('$rootPath/$phase.json').writeAsString(
            jsonEncode({
              'pid': pid,
              'complete': true,
              'phase': phase,
              'two_real_workspaces': true,
              'confirmation_removed_on_switch': true,
              'other_workspace_content_unchanged': true,
              'parent_and_child_active_before_switch': true,
              'parent_retired_only_after_return_and_confirmation': true,
              'original_child_request_sha256': sha256
                  .convert(
                    EditorDraftCodec.encodeWrite(proposal.handoff.request),
                  )
                  .toString(),
              'reopened_raw_child_verified': true,
              'business_revision_unchanged': true,
              'both_original_transports_closed': true,
            }),
            flush: true,
          ),
        );
      }
    },
    skip: !available,
    timeout: const Timeout(Duration(minutes: 4)),
  );
}
