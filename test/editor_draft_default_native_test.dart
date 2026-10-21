import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_i18n/morrow_i18n.dart';
import 'package:morrow_studio/main.dart';
import 'package:morrow_studio/plugins/studio_storage.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';
import 'package:morrow_studio/plugins/workbench_backend.dart';
import 'package:morrow_studio/plugins/versioned_editor_adapter.dart';

final hostPath = Platform.environment['MORROW_WORKBENCH_HOST'];
final packagePath = Platform.environment['MORROW_WORKBENCH_PACKAGE'];

Future<void> until(WidgetTester t, bool Function() ready) async {
  final end = DateTime.now().add(const Duration(seconds: 40));
  while (!ready()) {
    if (DateTime.now().isAfter(end)) {
      fail('Native default editor did not settle');
    }
    await t.runAsync(
      () => Future<void>.delayed(const Duration(milliseconds: 15)),
    );
    await t.pump(const Duration(milliseconds: 20));
    final error = t.takeException();
    if (error != null) throw error;
  }
}

Future<T> native<T>(WidgetTester t, Future<T> request) async {
  var done = false;
  late T result;
  Object? failure;
  request.then(
    (value) {
      result = value;
      done = true;
    },
    onError: (Object error) {
      failure = error;
      done = true;
    },
  );
  await until(t, () => done);
  if (failure != null) throw failure!;
  return result;
}

Future<void> tap(WidgetTester t, Finder finder) async {
  await until(t, () => finder.hitTestable().evaluate().length == 1);
  await t.tap(finder);
  await t.pump();
}

void main() {
  testWidgets(
    'saved-draft menu resumes the retained blank-title owner',
    (t) async {
      t.view.devicePixelRatio = 1;
      t.view.physicalSize = const Size(1280, 1000);
      addTearDown(t.view.reset);
      late Directory root;
      late RustWorkbench host;
      late RustStudioStorage storage;
      var closed = false;
      await t.runAsync(() async {
        root = await Directory.systemTemp.createTemp(
          'morrow-resume-live-draft-',
        );
        host = await RustWorkbench.open(
          executable: hostPath!,
          package: packagePath!,
          directory: Directory('${root.path}/store'),
        );
        storage = await RustStudioStorage.open(host);
      });
      final l = L10n.forLocale(const Locale('en'));
      try {
        await t.pumpWidget(
          MorrowApp(
            storage: storage,
            workbench: host,
            initialLocale: const Locale('en'),
            libraryDirectory: root.path,
          ),
        );
        await until(t, () => find.text(l.mainNewIdea).evaluate().isNotEmpty);
        await tap(t, find.text(l.mainNewIdea).first);
        await until(t, () => find.byType(NewIdeaDialog).evaluate().length == 1);
        final owner = t
            .widget<NewIdeaDialog>(find.byType(NewIdeaDialog))
            .draftOwner!;
        const value = TextEditingValue(
          text: 'blank-title 😀 draft',
          selection: TextSelection(
            baseOffset: 12,
            extentOffset: 14,
            isDirectional: true,
            affinity: TextAffinity.upstream,
          ),
          composing: TextRange(start: 12, end: 14),
        );
        t
                .widget<TextField>(
                  find.byKey(const ValueKey('idea-description')),
                )
                .controller!
                .value =
            value;
        await native(
          t,
          owner.session.flushLatestVisible(
            timeout: const Duration(seconds: 20),
          ),
        );
        await tap(t, find.text(l.mainNotNow));
        await until(t, () => host.editorDraftWorkspace.attachedViews == 0);
        await tap(t, find.byKey(const ValueKey('editor-recovery-open')));
        await tap(t, find.byKey(const ValueKey('editor-recovery-menu-drafts')));
        await tap(
          t,
          find.byKey(ValueKey('draft-journal-${owner.session.draftId}')),
        );
        await until(t, () => find.byType(NewIdeaDialog).evaluate().length == 1);
        expect(
          t.widget<NewIdeaDialog>(find.byType(NewIdeaDialog)).savedDraft,
          isNotNull,
        );
        await tap(t, find.byKey(const ValueKey('idea-resume-live-draft')));
        await until(
          t,
          () =>
              find.byType(NewIdeaDialog).evaluate().length == 1 &&
              t.widget<NewIdeaDialog>(find.byType(NewIdeaDialog)).draftOwner !=
                  null,
        );
        final restored = t.widget<NewIdeaDialog>(find.byType(NewIdeaDialog));
        expect(restored.draftOwner, same(owner));
        expect(
          t
              .widget<TextField>(find.byKey(const ValueKey('idea-description')))
              .controller!
              .value,
          value,
        );
        expect(
          t
              .widget<TextField>(find.byKey(const ValueKey('idea-title')))
              .controller!
              .text,
          isEmpty,
        );
        expect(host.editorDraftWorkspace.sessions, [owner.session]);
        expect(await native(t, host.loadWorkspaceContent()), isEmpty);
        await tap(t, find.text(l.mainNotNow));
        await until(t, () => host.editorDraftWorkspace.attachedViews == 0);
        final preparation = await native(t, host.prepareEditorDraftClose());
        await t.pumpWidget(const SizedBox.shrink());
        await native(t, host.finishEditorDraftClose(preparation));
        await native(t, host.close());
        closed = true;
      } finally {
        await t.pumpWidget(const SizedBox.shrink());
        if (closed) await t.runAsync(() => root.delete(recursive: true));
      }
    },
    skip: !Platform.isWindows || hostPath == null || packagePath == null,
  );
  for (final versioned in [false, true]) {
    testWidgets(
      'default ${versioned ? 'V2 edit' : 'new'} route retains the original draft and S2',
      (t) async {
        t.view.devicePixelRatio = 1;
        t.view.physicalSize = const Size(1280, 1000);
        addTearDown(t.view.reset);
        late Directory root;
        late RustWorkbench host;
        late RustStudioStorage storage;
        var closed = false;
        await t.runAsync(() async {
          root = await Directory.systemTemp.createTemp('morrow-default-draft-');
          host = await RustWorkbench.open(
            executable: hostPath!,
            package: packagePath!,
            directory: Directory('${root.path}/store'),
          );
          if (versioned) {
            await host.apply(
              PluginAction.create,
              Idea(
                'V2 baseline',
                'baseline body',
                '实验',
                Idea.icons[0],
                const Color(0xff8866aa),
                id: 'default-v2',
              ),
            );
            await host.versionedContent.migrate(
              await host.versionedContent.planMigration('default-v2'),
            );
          }
          storage = await RustStudioStorage.open(host);
        });
        final l = L10n.forLocale(const Locale('en'));
        try {
          await t.pumpWidget(
            MorrowApp(
              storage: storage,
              workbench: host,
              initialLocale: const Locale('en'),
              libraryDirectory: root.path,
            ),
          );
          if (versioned) {
            await until(
              t,
              () => find.text('V2 baseline').evaluate().isNotEmpty,
            );
            await tap(t, find.text('V2 baseline').first);
            await tap(t, find.text(l.mainEdit));
          } else {
            await until(
              t,
              () => find.text(l.mainNewIdea).evaluate().isNotEmpty,
            );
            await tap(t, find.text(l.mainNewIdea).first);
          }
          await until(
            t,
            () => find.byType(NewIdeaDialog).evaluate().length == 1,
          );
          final dialog = t.widget<NewIdeaDialog>(find.byType(NewIdeaDialog));
          final parent = dialog.draftOwner!;
          expect(dialog.draftRuntime, same(host));
          await native(t, parent.session.ensureJournal());
          expect(
            parent.session.confirmed!.request.values.title.text,
            versioned ? 'V2 baseline' : isEmpty,
          );
          if (versioned) {
            expect(
              (dialog.editor as VersionedWorkbenchEditorAdapter)
                  .deferRecoveryAcknowledgement,
              isTrue,
            );
          }
          TextEditingController field(String key) =>
              t.widget<TextField>(find.byKey(ValueKey(key))).controller!;
          field('idea-title').text = 'Default S1';
          field('idea-description').text = 'Default S1 body';
          t
              .widget<FilledButton>(find.byKey(const ValueKey('idea-save')))
              .onPressed!();
          await until(
            t,
            () =>
                parent.commit?.stage == 'business' ||
                parent.commit?.failure != null,
          );
          expect(parent.commit!.failure, isNull);
          const s2 = TextEditingValue(
            text: 'S2 😀 retained after route close',
            selection: TextSelection(
              baseOffset: 3,
              extentOffset: 5,
              affinity: TextAffinity.upstream,
              isDirectional: true,
            ),
            composing: TextRange(start: 3, end: 5),
          );
          field('idea-description').value = s2;
          await until(
            t,
            () => parent.commit!.finished || parent.commit!.failure != null,
          );
          expect(parent.commit!.failure, isNull);
          final child = parent.commit!.child!;
          await until(
            t,
            () =>
                t
                    .widget<Text>(
                      find.descendant(
                        of: find.byKey(const ValueKey('idea-save')),
                        matching: find.byType(Text),
                      ),
                    )
                    .data ==
                l.mainSaveIdea,
          );
          expect(field('idea-description').value, s2);
          await tap(t, find.text(l.mainNotNow));
          await until(t, () => find.byType(NewIdeaDialog).evaluate().isEmpty);
          await until(t, () => host.editorDraftWorkspace.attachedViews == 0);
          expect(host.retainedEditorDraft(parent.session.cardId), same(child));
          // The ordinary card details -> Edit route must reuse the retained child.
          await until(t, () => find.text('Default S1').evaluate().isNotEmpty);
          await tap(t, find.text('Default S1').first);
          await tap(t, find.text(l.mainEdit));
          await until(
            t,
            () => find.byType(NewIdeaDialog).evaluate().length == 1,
          );
          expect(
            t.widget<NewIdeaDialog>(find.byType(NewIdeaDialog)).draftOwner,
            same(child),
          );
          expect(field('idea-description').value, s2);
          expect(host.editorDraftWorkspace.sessions, [child.session]);
          expect(
            (await native(
              t,
              host.versionedContent.read(parent.session.cardId),
            )).revision,
            parent.session.sourceRevision + BigInt.one,
          );
          await tap(t, find.text(l.mainNotNow));
          await until(t, () => host.editorDraftWorkspace.attachedViews == 0);
          final preparation = await native(t, host.prepareEditorDraftClose());
          await t.pumpWidget(const SizedBox.shrink());
          await native(t, host.finishEditorDraftClose(preparation));
          await native(t, host.close());
          closed = true;
        } finally {
          await t.pumpWidget(const SizedBox.shrink());
          if (closed) await t.runAsync(() => root.delete(recursive: true));
        }
      },
      skip: !Platform.isWindows || hostPath == null || packagePath == null,
    );
  }
}
