// Actual host quotas, formal editor/history UI, and an independent client restart.
import 'dart:convert';
import 'dart:io';
import 'package:crypto/crypto.dart';
import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_i18n/morrow_i18n.dart';
import 'package:morrow_studio/main.dart';
import 'package:morrow_studio/editor_draft_history_dialog.dart';
import 'package:morrow_studio/plugins/editor_draft.dart';
import 'package:morrow_studio/plugins/editor_draft_codec.dart';
import 'package:morrow_studio/plugins/editor_draft_owner.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';
import 'editor_draft_review_process_test.dart' as fixture;
import 'editor_draft_resume_process_test.dart' show controllers;

final rootPath = fixture.rootPath, phase = fixture.phase;
final available =
    Platform.isWindows &&
    fixture.hostPath != null &&
    fixture.packagePath != null &&
    rootPath != null &&
    ['capacity', 'verify'].contains(phase);
const target = 'formal-capacity-original';

EditorDraftWriteRequest filler(int index) => EditorDraftWriteRequest(
  cardId: 'capacity-card-$index',
  draftId: 'capacity-draft-$index',
  operation: 'capacity-save-$index',
  expectedGeneration: BigInt.zero,
  sourceRevision: BigInt.zero,
  sourceKind: EditorDraftSourceKind.newCard,
  commitOperation: 'capacity-unsubmitted-create-$index',
  predecessorOperation: '',
  predecessorDigest: const [],
  values: fixture.initial().values,
  assets: const [],
);

Widget window(Widget child) => MaterialApp(
  locale: const Locale('en'),
  supportedLocales: AppLocalizations.supportedLocales,
  localizationsDelegates: const [
    AppLocalizations.delegate,
    GlobalMaterialLocalizations.delegate,
    GlobalWidgetsLocalizations.delegate,
    GlobalCupertinoLocalizations.delegate,
  ],
  home: Scaffold(body: child),
);

Future<void> rejected(
  WidgetTester t,
  RustWorkbench host,
  int id,
  String expected,
) async {
  Object? failure;
  try {
    await fixture.native(t, host.editorDrafts.save(filler(id)));
  } catch (error) {
    failure = error;
  }
  expect(
    failure,
    isNotNull,
    reason: 'Original configured quota must reject admission',
  );
  expect(failure, isA<EditorDraftSaveFailure>());
  expect(
    (failure as EditorDraftSaveFailure).cause.toString(),
    contains(expected),
  );
  expect(
    await fixture.native(
      t,
      host.editorDrafts.read(filler(id).cardId, filler(id).draftId),
    ),
    isNull,
  );
}

Future<EditorDraftWriteRequest> formalRefusal(
  WidgetTester t,
  RustWorkbench host,
  EditorDraftOwner owner,
  String expected,
) async {
  Object? failure;
  try {
    await fixture.native(
      t,
      owner.session.flushLatestVisible(timeout: const Duration(seconds: 20)),
    );
  } catch (error) {
    failure = error;
  }
  expect(failure, isA<EditorDraftSaveFailure>());
  expect(owner.session.lastFailure!.cause.toString(), contains(expected));
  // A generic failed finish reply does not prove rollback to the client.
  // Inspection preserves the exact pending operation until an explicit retry.
  expect(owner.session.unknown, true);
  expect(owner.session.dirty, true);
  final pending = owner.session.pendingRequest!;
  final bytes = EditorDraftCodec.encodeWrite(pending);
  await t.pump();
  expect(
    find.byKey(const ValueKey('idea-durable-draft-status')),
    findsOneWidget,
  );
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
  expect(EditorDraftCodec.encodeWrite(owner.session.pendingRequest!), bytes);
  expect(
    await fixture.native(
      t,
      host.editorDrafts.read(pending.cardId, pending.draftId),
    ),
    isNull,
  );
  expect(owner.commit, isNull);
  return pending;
}

Future<void> explicitOriginalRetry(
  WidgetTester t,
  EditorDraftOwner owner,
  EditorDraftWriteRequest pending,
) async {
  await fixture.tapVisible(t, 'idea-retry-draft-write');
  await fixture.waitFor(
    t,
    () => !owner.session.unknown && !owner.session.saving,
  );
  expect(owner.session.pendingRequest, isNull);
  expect(
    EditorDraftCodec.encodeWrite(owner.session.confirmed!.request),
    EditorDraftCodec.encodeWrite(pending),
  );
  expect(owner.commit, isNull);
}

void main({Future<void> Function(WidgetTester)? prepareWindow}) {
  testWidgets(
    'formal configured capacity $phase and retained original history',
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
      await t.runAsync(() async {
        expect(await store.exists(), phase != 'capacity');
        await root.create(recursive: true);
        host = await RustWorkbench.open(
          executable: fixture.hostPath!,
          package: fixture.packagePath!,
          directory: store,
        );
      });
      final history = host.editorDrafts as EditorDraftHistoryControl;
      final result = <String, Object?>{
        'pid': pid,
        'phase': phase,
        'complete': false,
      };
      String? currentDraft;
      String? overflowDraft;
      if (phase == 'capacity') {
        final active = <EditorDraftRecord>[];
        for (var i = 0; i < 16; i++) {
          active.add(
            await fixture.native(t, host.editorDrafts.save(filler(i))),
          );
        }
        expect(
          await fixture.native(t, host.editorDrafts.list()),
          hasLength(16),
        );
        await rejected(t, host, 9990, 'draft active capacity reached');
        final editor = await fixture.native(
          t,
          host.openEditor(target, create: true),
        );
        final owner = await fixture.native(
          t,
          host.createEditorDraftOwner(
            editor: editor,
            initial: fixture.initial(),
            attachments: [],
            create: true,
          ),
        );
        owner.session.pauseAutoSave();
        await t.pumpWidget(
          window(
            NewIdeaDialog(
              key: ValueKey(owner.session.draftId),
              editor: editor,
              draftOwner: owner,
              draftRuntime: host,
              initialCategory: '实验',
            ),
          ),
        );
        await t.pumpAndSettle();
        final fields = controllers(t);
        owner.session.pauseAutoSave();
        fields[1].text = 'capacity frozen S1';
        await t.pump();
        final refusedActive = await formalRefusal(
          t,
          host,
          owner,
          'draft active capacity reached',
        );
        fields[1].value = const TextEditingValue(
          text: 'capacity 原始 S2 🧪',
          selection: TextSelection(
            baseOffset: 2,
            extentOffset: 9,
            isDirectional: true,
          ),
          composing: TextRange(start: 2, end: 5),
        );
        await t.pump();
        for (final record in active) {
          await fixture.native(
            t,
            host.editorDrafts.discard(
              record.request.cardId,
              record.request.draftId,
              record.generation,
              'capacity-retire-${record.request.draftId}',
            ),
          );
        }
        expect(await fixture.native(t, host.editorDrafts.list()), isEmpty);
        await explicitOriginalRetry(t, owner, refusedActive);
        expect(owner.session.dirty, true);
        expect(owner.session.current.values.description.text, fields[1].text);
        final first = (await fixture.native(
          t,
          owner.session.flushLatestVisible(
            timeout: const Duration(seconds: 20),
          ),
        ))!;
        currentDraft = first.request.draftId;
        expect(first.request.values.title.text, '');
        expect(first.request.values.description.text, fields[1].text);
        for (var i = 16; i < 255; i++) {
          final record = await fixture.native(
            t,
            host.editorDrafts.save(filler(i)),
          );
          await fixture.native(
            t,
            host.editorDrafts.discard(
              record.request.cardId,
              record.request.draftId,
              record.generation,
              'capacity-retire-${record.request.draftId}',
            ),
          );
        }
        expect(
          (await fixture.native(t, history.history())).unarchivedCount,
          256,
        );
        await rejected(
          t,
          host,
          9991,
          'draft journal identity capacity reached',
        );
        final before = (await fixture.native(
          t,
          host.editorDrafts.read(target, currentDraft),
        ))!;
        expect(
          EditorDraftCodec.encodeWrite(before.request),
          EditorDraftCodec.encodeWrite(first.request),
        );
        final overflowEditor = await fixture.native(
          t,
          host.openEditor('formal-capacity-overflow', create: true),
        );
        final overflow = await fixture.native(
          t,
          host.createEditorDraftOwner(
            editor: overflowEditor,
            initial: fixture.initial(),
            attachments: [],
            create: true,
          ),
        );
        await t.pumpWidget(
          window(
            NewIdeaDialog(
              key: ValueKey(overflow.session.draftId),
              editor: overflowEditor,
              draftOwner: overflow,
              draftRuntime: host,
              initialCategory: '实验',
            ),
          ),
        );
        await t.pumpAndSettle();
        overflow.session.pauseAutoSave();
        controllers(t)[1].text =
            'independent draft retained while journal quota is full';
        await t.pump();
        final refusedJournal = await formalRefusal(
          t,
          host,
          overflow,
          'draft journal identity capacity reached',
        );
        overflowDraft = refusedJournal.draftId;
        final page = await fixture.native(t, history.history());
        final row = page.rows.first;
        final original = (await fixture.native(
          t,
          host.editorDrafts.read(row.cardId, row.draftId),
        ))!;
        expect(original.currentActive, false);
        final originalBytes = EditorDraftCodec.encodeWrite(original.request);
        await t.pumpWidget(
          window(
            EditorDraftHistoryDialog(
              control: host.editorDrafts,
              history: history,
              isCurrent: () => true,
            ),
          ),
        );
        await fixture.tapVisible(t, 'draft-history-${row.draftId}');
        await fixture.waitFor(
          t,
          () =>
              find.byType(LinearProgressIndicator).evaluate().isEmpty &&
              t
                      .widget<TextButton>(
                        find.byKey(const ValueKey('draft-history-archive')),
                      )
                      .onPressed !=
                  null,
        );
        await fixture.tapVisible(t, 'draft-history-archive');
        await fixture.waitFor(
          t,
          () =>
              find.byType(LinearProgressIndicator).evaluate().isEmpty &&
              find.text('Archived').evaluate().isNotEmpty,
        );
        final receipt = await fixture.native(
          t,
          history.inspectArchive(row.cardId, row.draftId),
        );
        expect(receipt, isNotNull);
        expect(receipt!.requestSha256, original.requestSha256);
        expect(
          (await fixture.native(t, history.history())).unarchivedCount,
          255,
        );
        await fixture.native(
          t,
          history.exportHistory(original, '$rootPath/capacity-export.json'),
        );
        late Map exported;
        await t.runAsync(() async {
          exported =
              jsonDecode(
                    await File('$rootPath/capacity-export.json').readAsString(),
                  )
                  as Map;
        });
        expect(
          exported['request_sha256'],
          original.requestSha256
              .map((b) => b.toRadixString(16).padLeft(2, '0'))
              .join(),
        );
        final slotHex = exported['canonical_slot_protobuf_hex'] as String;
        expect(RegExp(r'^(?:[a-f0-9]{2})+$').hasMatch(slotHex), true);
        final slot = [
          for (var offset = 0; offset < slotHex.length; offset += 2)
            int.parse(slotHex.substring(offset, offset + 2), radix: 16),
        ];
        final slotDigest = sha256.convert(slot).toString();
        expect(slotDigest, exported['slot_sha256']);
        expect(
          slotDigest,
          receipt.slotSha256
              .map((b) => b.toRadixString(16).padLeft(2, '0'))
              .join(),
        );
        await t.runAsync(() async {
          result['export_sha256'] = sha256
              .convert(
                await File('$rootPath/capacity-export.json').readAsBytes(),
              )
              .toString();
        });
        expect(exported['business_replay_authority'], false);
        expect(overflow.session.unknown, true);
        expect(
          identical(overflow.session.pendingRequest, refusedJournal),
          true,
        );
        await t.pumpWidget(
          window(
            NewIdeaDialog(
              key: ValueKey(overflow.session.draftId),
              editor: overflowEditor,
              draftOwner: overflow,
              draftRuntime: host,
              initialCategory: '实验',
            ),
          ),
        );
        await t.pumpAndSettle();
        overflow.session.pauseAutoSave();
        await explicitOriginalRetry(t, overflow, refusedJournal);
        final extra = (await fixture.native(
          t,
          host.editorDrafts.read(refusedJournal.cardId, refusedJournal.draftId),
        ))!;
        expect(extra.currentActive, true);
        final repeated = await fixture.native(t, history.archive(original));
        expect(repeated.operation, receipt.operation);
        expect(repeated.repeated, true);
        await rejected(
          t,
          host,
          9993,
          'draft journal identity capacity reached',
        );
        final after = await fixture.native(t, history.history());
        expect(after.unarchivedCount, 256);
        expect(after.archivedCount, 1);
        final preserved = (await fixture.native(
          t,
          host.editorDrafts.read(row.cardId, row.draftId),
        ))!;
        expect(EditorDraftCodec.encodeWrite(preserved.request), originalBytes);
        expect(await fixture.native(t, host.loadWorkspaceContent()), isEmpty);
        result.addAll({
          'active_limit': 16,
          'journal_limit': 256,
          'active_refused': true,
          'formal_active_refusal_recovered': true,
          'formal_journal_refusal_recovered': true,
          'original_pending_operations_preserved': true,
          'overflow_draft': overflowDraft,
          'journal_refused': true,
          'archive_card': row.cardId,
          'archive_draft': row.draftId,
          'archive_operation': receipt.operation,
          'archive_request_base64': base64Encode(originalBytes),
          'current_draft': currentDraft,
          'archive_repetition_gave_extra_credit': false,
          'business_submissions': 0,
        });
      } else {
        late Map previous;
        await t.runAsync(() async {
          previous =
              jsonDecode(await File('$rootPath/capacity.json').readAsString())
                  as Map;
        });
        expect(previous['pid'], isNot(pid));
        currentDraft = previous['current_draft'] as String;
        overflowDraft = previous['overflow_draft'] as String;
        final overflow = (await fixture.native(
          t,
          host.editorDrafts.read('formal-capacity-overflow', overflowDraft),
        ))!;
        expect(
          base64Encode(EditorDraftCodec.encodeWrite(overflow.request)),
          previous['overflow_latest_request_base64'],
        );
        expect(overflow.currentActive, true);
        final saved = (await fixture.native(
          t,
          host.editorDrafts.read(target, currentDraft),
        ))!;
        expect(
          base64Encode(EditorDraftCodec.encodeWrite(saved.request)),
          previous['latest_request_base64'],
        );
        expect(saved.currentActive, true);
        expect(saved.request.values.title.text, '');
        final receipt = await fixture.native(
          t,
          history.inspectArchive(
            previous['archive_card'] as String,
            previous['archive_draft'] as String,
          ),
        );
        expect(receipt!.operation, previous['archive_operation']);
        final original = (await fixture.native(
          t,
          host.editorDrafts.read(
            previous['archive_card'] as String,
            previous['archive_draft'] as String,
          ),
        ))!;
        expect(
          base64Encode(EditorDraftCodec.encodeWrite(original.request)),
          previous['archive_request_base64'],
        );
        await fixture.native(
          t,
          history.exportHistory(
            original,
            '$rootPath/capacity-export-reopened.json',
          ),
        );
        await t.runAsync(() async {
          final before = await File(
            '$rootPath/capacity-export.json',
          ).readAsBytes();
          final reopened = await File(
            '$rootPath/capacity-export-reopened.json',
          ).readAsBytes();
          expect(sha256.convert(before).toString(), previous['export_sha256']);
          expect(reopened, before);
        });
        final page = await fixture.native(t, history.history());
        expect(page.unarchivedCount, 256);
        expect(page.archivedCount, 1);
        await rejected(
          t,
          host,
          9994,
          'draft journal identity capacity reached',
        );
        expect(await fixture.native(t, host.loadWorkspaceContent()), isEmpty);
        result.addAll({
          'previous_pid': previous['pid'],
          'current_draft': currentDraft,
          'original_request_preserved': true,
          'original_archive_preserved': true,
          'journal_limit_still_enforced': true,
          'business_submissions': 0,
        });
      }
      final prepared = await fixture.native(t, host.prepareEditorDraftClose());
      final latest = (await fixture.native(
        t,
        host.editorDrafts.read(target, currentDraft),
      ))!;
      result['latest_request_base64'] = base64Encode(
        EditorDraftCodec.encodeWrite(latest.request),
      );
      final overflowLatest = (await fixture.native(
        t,
        host.editorDrafts.read('formal-capacity-overflow', overflowDraft),
      ))!;
      result['overflow_latest_request_base64'] = base64Encode(
        EditorDraftCodec.encodeWrite(overflowLatest.request),
      );
      await t.pumpWidget(const SizedBox());
      await fixture.native(t, host.finishEditorDraftClose(prepared));
      await fixture.native(t, host.close());
      result['complete'] = true;
      await t.runAsync(
        () => File(
          '$rootPath/$phase.json',
        ).writeAsString(jsonEncode(result), flush: true),
      );
    },
    skip: !available,
    timeout: const Timeout(Duration(minutes: 12)),
  );
}
