import 'dart:async';
import 'dart:io';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/main.dart';
import 'package:morrow_studio/editor_draft_handoff_recovery_dialog.dart';
import 'package:morrow_studio/plugins/editor_draft.dart';
import 'package:morrow_studio/plugins/studio_storage.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';
import 'fixtures/editor_draft_handoff_proposal_fixture.dart';

final hostPath = Platform.environment['MORROW_WORKBENCH_HOST'];
final packagePath = Platform.environment['MORROW_WORKBENCH_PACKAGE'];
Future<void> waitFor(WidgetTester t, bool Function() ready) async {
  for (var i = 0; i < 120; i++) {
    await t.runAsync(
      () => Future<void>.delayed(const Duration(milliseconds: 20)),
    );
    await t.pump(const Duration(milliseconds: 50));
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
  await t.ensureVisible(target);
  await t.tap(target);
  await t.pump();
  await t.runAsync(
    () => Future<void>.delayed(const Duration(milliseconds: 50)),
  );
  await t.pump(const Duration(milliseconds: 500));
}

void main() {
  testWidgets(
    'real workspace discovers a persisted handoff and explicitly completes then retires it',
    (t) async {
      t.view.devicePixelRatio = 1;
      t.view.physicalSize = const Size(1180, 850);
      addTearDown(t.view.reset);
      RustWorkbench? backend;
      RustStudioStorage? storage;
      late EditorDraftHandoffProposal proposal;
      try {
        await t.runAsync(() async {
          final directory = await Directory.systemTemp.createTemp(
            'morrow-handoff-recovery-ui-',
          );
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
        // Switching backend while the confirmation is open removes both owned
        // routes. It must not leave old-library raw text over the new workspace.
        await t.pumpWidget(
          MorrowApp(storage: storage, initialLocale: const Locale('en')),
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
      } finally {
        await t.pumpWidget(const SizedBox());
        await native(t, () async {
          await backend?.close();
        });
      }
    },
    skip: !Platform.isWindows || hostPath == null || packagePath == null,
    timeout: const Timeout(Duration(minutes: 4)),
  );
}
