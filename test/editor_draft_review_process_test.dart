// Run seed and recover in separate Flutter invocations. Only a fresh store
// directory crosses that process boundary; recovery discovers identities there.
import 'dart:convert';
import 'dart:io';

import 'package:crypto/crypto.dart';
import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_i18n/morrow_i18n.dart';
import 'package:morrow_studio/main.dart';
import 'package:morrow_studio/editor_draft_recovery_dialog.dart';
import 'package:morrow_studio/editor_draft_history_dialog.dart';
import 'package:morrow_studio/card_tips_editor.dart';
import 'package:morrow_studio/plugins/editor_draft.dart';
import 'package:morrow_studio/plugins/editor_draft_binding.dart';
import 'package:morrow_studio/plugins/editor_draft_codec.dart';
import 'package:morrow_studio/plugins/editor_draft_owner.dart';
import 'package:morrow_studio/plugins/editor_draft_import.dart';
import 'package:morrow_studio/plugins/editor_draft_session.dart';
import 'package:morrow_studio/plugins/editor_session.dart';
import 'package:morrow_studio/plugins/studio_storage.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';

final hostPath = Platform.environment['MORROW_WORKBENCH_HOST'];
final packagePath = Platform.environment['MORROW_WORKBENCH_PACKAGE'];
final rootPath = Platform.environment['MORROW_DRAFT_REVIEW_ROOT'];
final phase = Platform.environment['MORROW_DRAFT_REVIEW_PHASE'];
final available =
    Platform.isWindows &&
    hostPath != null &&
    packagePath != null &&
    rootPath != null &&
    [
      'seed',
      'recover',
      'retire',
      'archive',
      'history',
      'live-retire',
    ].contains(phase);

Future<void> waitFor(WidgetTester t, bool Function() ready) async {
  final end = DateTime.now().add(const Duration(seconds: 45));
  while (!ready()) {
    for (final type in [EditorDraftRecoveryDialog, EditorDraftHistoryDialog]) {
      final dialog = find.byType(type);
      if (dialog.evaluate().isNotEmpty) {
        final dynamic state = t.state(dialog);
        if (state.lastError != null) {
          throw StateError('Fixture recovery read failed: ${state.lastError}');
        }
      }
    }
    if (DateTime.now().isAfter(end)) {
      debugPrint(
        'Visible fixture text: ${find.byType(Text).evaluate().map((e) => (e.widget as Text).data).whereType<String>().join(' | ')}',
      );
      fail('Draft review did not settle');
    }
    await t.runAsync(
      () => Future<void>.delayed(const Duration(milliseconds: 20)),
    );
    await t.pump(const Duration(milliseconds: 30));
    final error = t.takeException();
    if (error != null) throw error;
  }
}

Future<T> native<T>(WidgetTester t, Future<T> request) async {
  var done = false;
  late T value;
  Object? failure;
  request.then(
    (v) {
      value = v;
      done = true;
    },
    onError: (Object e) {
      failure = e;
      done = true;
    },
  );
  await waitFor(t, () => done);
  if (failure != null) throw failure!;
  return value;
}

Future<void> tapVisible(WidgetTester t, String key) async {
  final target = find.byKey(ValueKey(key));
  await waitFor(t, () => target.hitTestable().evaluate().length == 1);
  await t.tap(target);
  await t.pump();
}

EditorDraftTextValue text(String value) => EditorDraftTextValue(
  text: value,
  selectionBase: 0,
  selectionExtent: value.length,
  affinity: 0,
  directional: true,
  composingStart: -1,
  composingEnd: -1,
);
EditorDraftSnapshot initial() => EditorDraftSnapshot(
  values: EditorDraftValues(
    title: text(''),
    description: text(''),
    hypothesis: text(''),
    conclusion: text(''),
    todos: text(''),
    category: '实验',
    stage: '待验证',
  ),
  assets: const [],
);
List<TextEditingController> controllers(WidgetTester t) => [
  for (final key in [
    'idea-title',
    'idea-description',
    'experiment-hypothesis',
    'experiment-conclusion',
  ])
    t.widget<TextField>(find.byKey(ValueKey(key))).controller!,
  t.widget<CardTipsEditor>(find.byKey(const ValueKey('idea-todos'))).controller,
];

void main({Future<void> Function(WidgetTester)? prepareWindow}) {
  testWidgets(
    'formal draft $phase in an independent Flutter client',
    (t) async {
      if (prepareWindow == null) {
        t.view.devicePixelRatio = 1;
        t.view.physicalSize = const Size(1180, 900);
        addTearDown(t.view.reset);
      } else {
        await prepareWindow(t);
      }
      final freshOwner = phase == 'seed' || phase == 'live-retire';
      final root = Directory(rootPath!);
      final store = Directory('${root.path}/store');
      RustWorkbench? host;
      EditorDraftOwner? owner;
      late RustStudioStorage storage;
      await t.runAsync(() async {
        if (freshOwner) {
          expect(await store.exists(), isFalse);
          await root.create(recursive: true);
        } else {
          expect(await store.exists(), isTrue);
        }
        host = await RustWorkbench.open(
          executable: hostPath!,
          package: packagePath!,
          directory: store,
        );
        if (freshOwner) {
          final editor = await host!.openEditor(
            'formal-process-blank-draft',
            create: true,
          );
          owner = await host!.createEditorDraftOwner(
            editor: editor,
            initial: initial(),
            attachments: const [],
            create: true,
          );
          final selected = File('${root.path}/selected.bin');
          await selected.writeAsBytes([0, 255, 42, 9]);
          await host!.importEditorDraftFile(
            owner!,
            path: selected.path,
            name: 'selected.bin',
            kind: 'file',
          );
          await selected.delete();
        } else {
          storage = await RustStudioStorage.open(host!);
        }
      });
      if (phase == 'archive' || phase == 'history') {
        await _historyMaintenance(t, root, host!);
        return;
      }
      if (freshOwner) {
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
        fields[1].text = 'x😀y raw [asset](attachment:selected.bin)';
        fields[2].text = '  hypothesis\n';
        fields[3].text = '\nconclusion  ';
        fields[4].text = 'one\n\none\ntwo';
        final capture = await native(
          t,
          owner!.editor.studio.capture('html', '<strong>captured</strong>'),
        );
        final before = fields[1].text;
        await native(
          t,
          owner!.recordPaste(
            PasteInsertion(
              id: 'original-process-rich-paste',
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
        for (final field in fields) {
          field.value = field.value.copyWith(
            selection: TextSelection(
              baseOffset: 0,
              extentOffset: field.text.length,
              affinity: TextAffinity.upstream,
              isDirectional: true,
            ),
          );
        }
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
        if (phase == 'live-retire') {
          await _retireLiveOwner(t, root, store, host!, owner!);
          return;
        }
        final prepared = await native(t, host!.prepareEditorDraftClose());
        final stored = owner!.session.confirmed!;
        expect(stored.request.values.title.text, isEmpty);
        expect(stored.request.captureEvents, 1);
        expect(stored.assets, hasLength(1));
        await t.pumpWidget(const SizedBox());
        await native(t, host!.finishEditorDraftClose(prepared));
        // A durable but unselected Ready import belongs to the journal too.
        // Restarted retirement must release it under its original staging owner.
        final unselected = File('${root.path}/unselected.bin');
        await t.runAsync(() => unselected.writeAsBytes([7, 8, 9], flush: true));
        final import = EditorDraftImportRequest(
          cardId: stored.request.cardId,
          draftId: stored.request.draftId,
          operation: 'retirement-unselected-import',
          expectedGeneration: stored.generation,
          name: 'unselected.bin',
          kind: 'file',
          bytes: BigInt.from(3),
          sha256: sha256.convert([7, 8, 9]).bytes,
        );
        await native(t, host!.editorDraftImports.begin(import));
        await native(
          t,
          host!.editorDraftImports.complete(
            import,
            selectedPath: unselected.path,
          ),
        );
        await t.runAsync(unselected.delete);
        await native(t, host!.close());
        await t.runAsync(
          () => File('${root.path}/seed.json').writeAsString(
            jsonEncode({
              'pid': pid,
              'journal_sha256': sha256
                  .convert(EditorDraftCodec.encodeWrite(stored.request))
                  .toString(),
              'complete': true,
              'business_operations': 0,
            }),
            flush: true,
          ),
        );
      } else {
        final rows = await native(t, host!.editorDrafts.list());
        final original = rows.singleWhere((row) => row.active);
        final before = (await native(
          t,
          host!.editorDrafts.read(original.cardId, original.draftId),
        ))!;
        await native(t, host!.inspectEditorDraft(original));
        debugPrint('independent recovery: native inspection verified');
        await t.pumpWidget(
          MorrowApp(
            storage: storage,
            workbench: host,
            initialLocale: const Locale('en'),
            libraryDirectory: store.path,
          ),
        );
        await waitFor(
          t,
          () => find
              .byKey(const ValueKey('editor-recovery-open'))
              .evaluate()
              .isNotEmpty,
        );
        // Progress real I/O while routes animate. pumpAndSettle can advance a
        // transport timeout before the native process has received CPU time.
        await tapVisible(t, 'editor-recovery-open');
        await tapVisible(t, 'editor-recovery-menu-drafts');
        debugPrint('independent recovery: journal menu selected');
        await waitFor(
          t,
          () => find
              .byKey(ValueKey('draft-journal-${original.draftId}'))
              .evaluate()
              .isNotEmpty,
        );
        await tapVisible(t, 'draft-journal-${original.draftId}');
        debugPrint('independent recovery: journal selected');
        await waitFor(
          t,
          () => find.byType(NewIdeaDialog).evaluate().isNotEmpty,
        );
        await t.pump(const Duration(milliseconds: 300));
        final fields = controllers(t);
        final raw = before.request.values;
        final expected = [
          raw.title,
          raw.description,
          raw.hypothesis,
          raw.conclusion,
          raw.todos,
        ];
        for (var i = 0; i < fields.length; i++) {
          expect(fields[i].value, EditorDraftBinding.editingValue(expected[i]));
        }
        expect(find.byKey(const ValueKey('idea-save')), findsNothing);
        for (final key in [
          'idea-title',
          'idea-description',
          'experiment-hypothesis',
          'experiment-conclusion',
        ]) {
          expect(
            t.widget<TextField>(find.byKey(ValueKey(key))).readOnly,
            isTrue,
          );
        }
        expect(host!.editorDraftWorkspace.sessions, isEmpty);
        final review = await native(t, host!.inspectEditorDraft(original));
        expect(review.commit, isNull);
        expect(
          review.views.single.attachment.source.location,
          isNot(contains('selected.bin')),
        );
        final output = await native(
          t,
          host!.exportEditorDraft(review, root.path),
        );
        await t.runAsync(() async {
          expect(await File('${root.path}/selected.bin').exists(), isFalse);
          expect(await File('$output/asset-0.bin').readAsBytes(), [
            0,
            255,
            42,
            9,
          ]);
          final manifest =
              jsonDecode(await File('$output/manifest.json').readAsString())
                  as Map;
          expect(manifest['complete'], isTrue);
          expect(manifest['commit_confirmed'], isFalse);
          for (final item in manifest['files'] as List) {
            final bytes = await File('$output/${item['file']}').readAsBytes();
            expect(bytes.length, item['bytes']);
            expect(sha256.convert(bytes).toString(), item['sha256']);
          }
          expect(
            await File('$output/capture.pb').readAsBytes(),
            await File(review.captureArchive!).readAsBytes(),
          );
          final seed =
              jsonDecode(await File('${root.path}/seed.json').readAsString())
                  as Map;
          expect(seed['pid'], isNot(pid));
          expect(
            seed['journal_sha256'],
            sha256
                .convert(EditorDraftCodec.encodeWrite(before.request))
                .toString(),
          );
        });
        final after = (await native(
          t,
          host!.editorDrafts.read(original.cardId, original.draftId),
        ))!;
        expect(
          EditorDraftCodec.encodeWrite(after.request),
          EditorDraftCodec.encodeWrite(before.request),
        );
        expect(await native(t, host!.loadWorkspaceContent()), isEmpty);
        if (phase == 'retire') {
          await tapVisible(t, 'idea-retire-saved-draft');
          await tapVisible(t, 'idea-confirm-retire-draft');
          await waitFor(t, () => find.byType(NewIdeaDialog).evaluate().isEmpty);
          final retired = (await native(
            t,
            host!.editorDrafts.read(original.cardId, original.draftId),
          ))!;
          expect(retired.discardOperation, isNotEmpty);
          expect(retired.active, isFalse);
          expect(retired.generation, before.generation + BigInt.one);
          expect(
            EditorDraftCodec.encodeWrite(retired.request),
            EditorDraftCodec.encodeWrite(before.request),
          );
          final imports = await native(
            t,
            host!.editorDraftImports.list(original.cardId, original.draftId),
          );
          expect(imports.records, isEmpty);
          final oldImport = await native(
            t,
            host!.editorDraftImports.inspect(
              original.cardId,
              original.draftId,
              'retirement-unselected-import',
            ),
          );
          expect(oldImport.record!.phase, EditorDraftImportPhase.retired);
          expect(oldImport.record!.bytesRetained, isFalse);
          expect(await native(t, host!.editorDrafts.list()), isEmpty);
          expect(await native(t, host!.loadWorkspaceContent()), isEmpty);
          await t.runAsync(
            () => File('${root.path}/retire.json').writeAsString(
              jsonEncode({
                'pid': pid,
                'complete': true,
                'operation': retired.discardOperation,
                'generation': retired.generation.toString(),
                'imports_closed': true,
                'business_operations': 0,
                'complete_export': output,
              }),
              flush: true,
            ),
          );
        }
        await t.pumpWidget(const SizedBox());
        await native(t, host!.close());
        await t.runAsync(
          () => File('${root.path}/${phase!}-review.json').writeAsString(
            jsonEncode({
              'pid': pid,
              'complete': true,
              'journal_unchanged': true,
              'read_only_formal_dialog': true,
              'complete_export': output,
              'business_operations': 0,
              'capture_scope_recreated': false,
            }),
            flush: true,
          ),
        );
      }
    },
    skip: !available,
    timeout: const Timeout(Duration(minutes: 3)),
  );
}

Future<void> _historyMaintenance(
  WidgetTester t,
  Directory root,
  RustWorkbench host,
) async {
  final history = host.editorDrafts as EditorDraftHistoryControl;
  final page = await native(t, history.history());
  expect(page.rows, hasLength(1));
  expect(page.unarchivedCount, phase == 'archive' ? 1 : 0);
  expect(page.archivedCount, phase == 'archive' ? 0 : 1);
  final row = page.rows.single;
  final before = (await native(
    t,
    host.editorDrafts.read(row.cardId, row.draftId),
  ))!;
  final snapshot = EditorDraftCodec.encodeWrite(before.request);
  expect(before.active, isFalse);
  expect(before.currentActive, isFalse);
  expect(before.request.values.title.text, isEmpty);
  expect(row.title, isEmpty);
  expect(row.preview, isNotEmpty);
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
        body: EditorDraftRecoveryDialog(runtime: host, isCurrent: () => true),
      ),
    ),
  );
  await waitFor(
    t,
    () =>
        find.text('Draft history').hitTestable().evaluate().length == 1 &&
        find.byType(LinearProgressIndicator).evaluate().isEmpty,
  );
  await t.tap(find.text('Draft history'));
  await t.pump();
  await waitFor(
    t,
    () => find.byType(EditorDraftHistoryDialog).evaluate().length == 1,
  );
  await tapVisible(t, 'draft-history-${row.draftId}');
  await waitFor(
    t,
    () =>
        find.byType(LinearProgressIndicator).evaluate().isEmpty &&
        find.byType(SelectableText).evaluate().isNotEmpty,
  );
  expect(find.text(row.draftId), findsNothing);
  expect(find.text('Title'), findsOneWidget);
  final archiveButton = find.byKey(const ValueKey('draft-history-archive'));
  if (phase == 'archive') {
    expect(t.widget<TextButton>(archiveButton).onPressed, isNotNull);
    await t.tap(archiveButton);
    await t.pump();
    await waitFor(
      t,
      () =>
          find.byType(LinearProgressIndicator).evaluate().isEmpty &&
          find.text('Archived').evaluate().isNotEmpty,
    );
  }
  expect(t.widget<TextButton>(archiveButton).onPressed, isNull);
  final receipt = (await native(
    t,
    history.inspectArchive(row.cardId, row.draftId),
  ))!;
  expect(receipt.generation, before.generation);
  expect(receipt.requestSha256, before.requestSha256);
  final after = (await native(
    t,
    host.editorDrafts.read(row.cardId, row.draftId),
  ))!;
  expect(EditorDraftCodec.encodeWrite(after.request), snapshot);
  expect(after.generation, before.generation);
  expect(after.discardOperation, before.discardOperation);
  final repeated = await native(t, history.archive(after));
  expect(repeated.operation, receipt.operation);
  expect(repeated.repeated, isTrue);
  final exported = File('${root.path}/history-$phase.json');
  await native(t, history.exportHistory(after, exported.path));
  await t.runAsync(() async {
    final data = jsonDecode(await exported.readAsString()) as Map;
    expect(data['active'], isFalse);
    expect(data['released_blobs_included'], isFalse);
    expect(data['capture_authority'], isFalse);
    expect(data['business_replay_authority'], isFalse);
    expect((data['values'] as Map)['title']['text'], '');
    expect(
      data['request_sha256'],
      receipt.requestSha256
          .map((b) => b.toRadixString(16).padLeft(2, '0'))
          .join(),
    );
    expect(data['archive_operation'], receipt.operation);
    final seed =
        jsonDecode(await File('${root.path}/seed.json').readAsString()) as Map;
    expect(seed['pid'], isNot(pid));
    if (phase == 'history') {
      final previous =
          jsonDecode(await File('${root.path}/archive.json').readAsString())
              as Map;
      expect(previous['pid'], isNot(pid));
      expect(previous['operation'], receipt.operation);
      expect(previous['request_sha256'], data['request_sha256']);
    }
  });
  final finalPage = await native(t, history.history());
  expect((finalPage.unarchivedCount, finalPage.archivedCount), (0, 1));
  expect(await native(t, host.loadWorkspaceContent()), isEmpty);
  expect(host.editorDraftWorkspace.sessions, isEmpty);
  await t.pumpWidget(const SizedBox());
  await native(t, host.close());
  await t.runAsync(
    () => File('${root.path}/$phase.json').writeAsString(
      jsonEncode({
        'pid': pid,
        'complete': true,
        'operation': receipt.operation,
        'generation': receipt.generation.toString(),
        'request_sha256': receipt.requestSha256
            .map((b) => b.toRadixString(16).padLeft(2, '0'))
            .join(),
        'original_journal_unchanged': true,
        'formal_history_dialog': true,
        'unarchived_count': finalPage.unarchivedCount,
        'archived_count': finalPage.archivedCount,
        'history_export': exported.path,
        'capture_authority_restored': false,
        'released_blobs_restored': false,
        'business_operations': 0,
      }),
      flush: true,
    ),
  );
}

// The first retirement goes through the formal recovery menu while the same
// client still owns the capture. Additional fresh owners exercise slot reuse.
Future<void> _retireLiveOwner(
  WidgetTester t,
  Directory root,
  Directory store,
  RustWorkbench host,
  EditorDraftOwner owner,
) async {
  final before = (await native(
    t,
    owner.session.flushLatestVisible(timeout: const Duration(seconds: 20)),
  ))!;
  expect(before.assets, hasLength(1));
  expect(before.request.captureEvents, 1);
  await t.pumpWidget(const SizedBox());
  expect(owner.session.disposed, isFalse);
  expect(owner.captureClosed, isFalse);
  expect(host.editorDraftWorkspace.hasView(owner.session), isFalse);
  final storage = await native(t, RustStudioStorage.open(host));
  await t.pumpWidget(
    MorrowApp(
      storage: storage,
      workbench: host,
      initialLocale: const Locale('en'),
      libraryDirectory: store.path,
    ),
  );
  await tapVisible(t, 'editor-recovery-open');
  await tapVisible(t, 'editor-recovery-menu-drafts');
  await tapVisible(t, 'draft-journal-${owner.session.draftId}');
  await waitFor(t, () => find.byType(NewIdeaDialog).evaluate().isNotEmpty);
  expect(
    t.widget<NewIdeaDialog>(find.byType(NewIdeaDialog)).canResumeSavedDraft,
    isTrue,
  );
  await tapVisible(t, 'idea-retire-saved-draft');
  expect(owner.session.retirementFrozen, isFalse);
  await tapVisible(t, 'idea-confirm-retire-draft');
  await waitFor(t, () => find.byType(NewIdeaDialog).evaluate().isEmpty);
  final retired = (await native(
    t,
    host.editorDrafts.read(owner.session.cardId, owner.session.draftId),
  ))!;
  expect(retired.discardOperation, isNotEmpty);
  expect(retired.currentActive, isFalse);
  expect(retired.generation, before.generation + BigInt.one);
  expect(
    EditorDraftCodec.encodeWrite(retired.request),
    EditorDraftCodec.encodeWrite(before.request),
  );
  expect(owner.retired, isTrue);
  expect(owner.captureClosed, isTrue);
  expect(owner.session.disposed, isTrue);
  expect(host.editorDraftWorkspace.sessions, isEmpty);
  final imports = await native(
    t,
    host.editorDraftImports.list(owner.session.cardId, owner.session.draftId),
  );
  expect(imports.records, isEmpty);
  await t.pumpWidget(const SizedBox());
  for (var index = 0; index < 17; index++) {
    final editor = await native(
      t,
      host.openEditor('live-retire-$index', create: true),
    );
    final next = await native(
      t,
      host.createEditorDraftOwner(
        editor: editor,
        initial: initial(),
        attachments: const [],
        create: true,
      ),
    );
    final record = await native(t, next.session.ensureJournal());
    final review = await native(
      t,
      host.inspectEditorDraft(
        EditorDraftSummary(
          cardId: record.request.cardId,
          draftId: record.request.draftId,
          generation: record.generation,
          active: true,
        ),
      ),
    );
    final attempt = host.retirementFor(review);
    await native(t, attempt.start());
    expect(next.captureClosed, isTrue);
    expect(next.session.disposed, isTrue);
    expect(next.retired, isTrue);
    expect(host.editorDraftWorkspace.sessions, isEmpty);
    expect(await native(t, host.editorDrafts.list()), isEmpty);
  }
  expect(await native(t, host.loadWorkspaceContent()), isEmpty);
  final closing = await native(t, host.prepareEditorDraftClose());
  await native(t, host.finishEditorDraftClose(closing));
  await native(t, host.close());
  await t.runAsync(
    () => File('${root.path}/live-retire.json').writeAsString(
      jsonEncode({
        'pid': pid,
        'complete': true,
        'live_retirements': 18,
        'formal_dialog_retirements': 1,
        'capture_closed': true,
        'imports_closed': true,
        'business_operations': 0,
        'operation': retired.discardOperation,
        'generation': retired.generation.toString(),
      }),
      flush: true,
    ),
  );
}
