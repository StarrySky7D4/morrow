// Formal attachment removal/import, actual durable generations, independent
// Windows-client restart. File choices are framework supplied, not physical.
import 'dart:convert';
import 'dart:io';
import 'package:crypto/crypto.dart';
import 'package:file_selector/file_selector.dart';
import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_i18n/morrow_i18n.dart';
import 'package:morrow_studio/main.dart';
import 'package:morrow_studio/attachments/attachment_view.dart';
import 'package:morrow_studio/editor_draft_history_dialog.dart';
import 'package:morrow_studio/plugins/editor_draft.dart';
import 'package:morrow_studio/plugins/editor_draft_codec.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';
import 'editor_draft_review_process_test.dart' as fixture;
import 'editor_draft_resume_process_test.dart' show controllers;
import 'fixtures/editor_draft_reply_faults.dart';

final rootPath = fixture.rootPath, phase = fixture.phase;
final available =
    Platform.isWindows &&
    fixture.hostPath != null &&
    fixture.packagePath != null &&
    rootPath != null &&
    [
      'reselect',
      'archive',
      'verify',
      'maintain',
      'maintenance-verify',
    ].contains(phase);
const target = 'formal-attachment-reselection';
const originalBytes = [31, 4, 255, 7];
const newBytes = [94, 17, 0, 255, 9, 8];

void main({Future<void> Function(WidgetTester)? prepareWindow}) {
  testWidgets(
    'formal released attachment $phase',
    (t) async {
      if (prepareWindow != null) {
        await prepareWindow(t);
      } else {
        t.view.devicePixelRatio = 1;
        t.view.physicalSize = const Size(1180, 900);
        addTearDown(t.view.reset);
      }
      final root = Directory(rootPath!), store = Directory('$rootPath/store');
      final selected = File('$rootPath/selected.bin');
      final result = <String, Object?>{
        'pid': pid,
        'phase': phase,
        'complete': false,
      };
      late RustWorkbench host;
      await t.runAsync(() async {
        expect(await store.exists(), phase != 'reselect');
        await root.create(recursive: true);
        host = await RustWorkbench.open(
          executable: fixture.hostPath!,
          package: fixture.packagePath!,
          directory: store,
        );
      });
      if (phase == 'reselect') {
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
        fields[0].text = 'Reselected attachment';
        fields[1].text = 'Explicit new selection after original pin retirement';
        final dynamic ui = t.state(find.byType(NewIdeaDialog));
        await t.runAsync(
          () => selected.writeAsBytes(originalBytes, flush: true),
        );
        await fixture.native<void>(
          t,
          ui.importFiles([XFile(selected.path)]) as Future<void>,
        );
        await t.pump();
        final oldView = t
            .widget<AttachmentTile>(find.byType(AttachmentTile))
            .attachment;
        final first = (await fixture.native(
          t,
          owner.session.flushLatestVisible(
            timeout: const Duration(seconds: 20),
          ),
        ))!;
        expect(first.assets, hasLength(1));
        final oldId = first.assets.single.selection.assetId;
        final firstHash = sha256
            .convert(EditorDraftCodec.encodeWrite(first.request))
            .toString();
        final remove = find.descendant(
          of: find.byType(AttachmentTile),
          matching: find.byIcon(Icons.close),
        );
        await t.ensureVisible(remove);
        await t.tap(remove);
        await t.pump();
        expect(find.byType(AttachmentTile), findsNothing);
        final removed = (await fixture.native(
          t,
          owner.session.flushLatestVisible(
            timeout: const Duration(seconds: 20),
          ),
        ))!;
        expect(removed.generation, first.generation + BigInt.one);
        expect(removed.assets, isEmpty);
        expect(owner.catalog.confirmedGeneration, removed.generation);
        expect(
          () => owner.catalog.selectionsFor([oldView]),
          throwsFormatException,
        );
        final durable = (await fixture.native(
          t,
          host.editorDrafts.read(target, owner.session.draftId),
        ))!;
        expect(durable.assets, isEmpty);
        expect(
          EditorDraftCodec.encodeWrite(durable.request),
          EditorDraftCodec.encodeWrite(removed.request),
        );
        bool oldExportRejected = false;
        try {
          await fixture.native(
            t,
            host.editorDrafts.exportAsset(
              target,
              owner.session.draftId,
              first.generation,
              oldId,
              '$rootPath/stale-export.bin',
            ),
          );
        } catch (_) {
          oldExportRejected = true;
        }
        expect(oldExportRejected, true);
        expect(
          await t.runAsync(() => File('$rootPath/stale-export.bin').exists()),
          false,
        );
        // This is a fresh explicit user selection through the dialog's original
        // import handler. The same path now contains different bytes.
        await t.runAsync(() => selected.writeAsBytes(newBytes, flush: true));
        await fixture.native<void>(
          t,
          ui.importFiles([XFile(selected.path)]) as Future<void>,
        );
        await t.pump();
        final newView = t
            .widget<AttachmentTile>(find.byType(AttachmentTile))
            .attachment;
        expect(identical(newView, oldView), false);
        final fresh = (await fixture.native(
          t,
          owner.session.flushLatestVisible(
            timeout: const Duration(seconds: 20),
          ),
        ))!;
        expect(fresh.assets, hasLength(1));
        final freshId = fresh.assets.single.selection.assetId;
        expect(freshId, isNot(oldId));
        expect(fresh.generation, removed.generation + BigInt.one);
        expect(
          () => owner.catalog.selectionsFor([oldView]),
          throwsFormatException,
        );
        await fixture.native(
          t,
          host.editorDrafts.exportAsset(
            target,
            owner.session.draftId,
            fresh.generation,
            freshId,
            '$rootPath/new-export.bin',
          ),
        );
        await t.runAsync(() async {
          expect(
            await File('$rootPath/new-export.bin').readAsBytes(),
            newBytes,
          );
          await selected.delete();
        });
        final operation = owner.session.commitOperation;
        await fixture.tapVisible(t, 'idea-save');
        await fixture.waitFor(t, () => owner.commit?.finished == true);
        expect(editor.saves, 1);
        expect(owner.commit!.failure, isNull);
        final content = await fixture.native(
          t,
          host.versionedContent.read(target),
        );
        final shown = await fixture.native(t, host.workspaceRecord(content));
        expect(content.revision, BigInt.one);
        expect(shown.attachments, hasLength(1));
        await t.runAsync(() async {
          expect(
            await File(shown.attachments.single.source.location).readAsBytes(),
            newBytes,
          );
        });
        final proof = await fixture.native(
          t,
          host.inspectEditorCommit(id: target, operation: operation),
        );
        result.addAll({
          'old_asset_id': oldId,
          'parent_draft_id': owner.session.draftId,
          'new_asset_id': freshId,
          'old_view_rejected': true,
          'old_generation_export_rejected': oldExportRejected,
          'first_request_sha256': firstHash,
          'removed_generation': removed.generation.toString(),
          'fresh_generation': fresh.generation.toString(),
          'business_submissions': editor.saves,
          'business_operation': operation,
          'business_revision': '1',
          'commit_evidence': proof.digest,
          'attachment_sha256': sha256.convert(newBytes).toString(),
          'selected_source_deleted_before_commit': true,
        });
      } else if (phase == 'archive') {
        final previous =
            jsonDecode(await File('$rootPath/reselect.json').readAsString())
                as Map;
        expect(previous['pid'], isNot(pid));
        final parentId = previous['parent_draft_id'] as String;
        final original = (await fixture.native(
          t,
          host.editorDrafts.read(target, parentId),
        ))!;
        expect(original.currentActive, false);
        expect(original.retirement, isNotNull);
        final history = host.editorDrafts as EditorDraftHistoryControl;
        final before = await fixture.native(t, history.history());
        expect(before.rows.any((row) => row.draftId == parentId), true);
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
              body: EditorDraftHistoryDialog(
                control: host.editorDrafts,
                history: history,
                isCurrent: () => true,
              ),
            ),
          ),
        );
        await fixture.tapVisible(t, 'draft-history-$parentId');
        // Selection reads both the retired record and its archive receipt from
        // the original host. A visible disabled button is still hit-testable.
        await fixture.waitFor(
          t,
          () =>
              find.byType(LinearProgressIndicator).evaluate().isEmpty &&
              find.byType(SelectableText).evaluate().isNotEmpty,
        );
        final archiveButton = find.byKey(
          const ValueKey('draft-history-archive'),
        );
        expect(t.widget<TextButton>(archiveButton).onPressed, isNotNull);
        await fixture.tapVisible(t, 'draft-history-archive');
        await fixture.waitFor(
          t,
          () => find.byType(LinearProgressIndicator).evaluate().isEmpty,
        );
        final archived = await fixture.native(
          t,
          history.inspectArchive(target, parentId),
        );
        await t.pump();
        final dynamic ui = t.state(find.byType(EditorDraftHistoryDialog));
        expect(
          ui.lastError,
          isNull,
          reason: 'Completed original handoff must be archivable',
        );
        expect(archived, isNotNull);
        expect(archived!.generation, original.generation);
        expect(archived.requestSha256, original.requestSha256);
        final after = await fixture.native(t, history.history());
        expect(after.unarchivedCount, before.unarchivedCount - 1);
        expect(after.archivedCount, before.archivedCount + 1);
        final preserved = (await fixture.native(
          t,
          host.editorDrafts.read(target, parentId),
        ))!;
        expect(
          EditorDraftCodec.encodeWrite(preserved.request),
          EditorDraftCodec.encodeWrite(original.request),
        );
        expect(preserved.retirement!.operation, original.retirement!.operation);
        final exportedPath = '$rootPath/archived-parent.json';
        await fixture.native(t, history.exportHistory(preserved, exportedPath));
        late List auxiliaries;
        await t.runAsync(() async {
          final exported =
              jsonDecode(await File(exportedPath).readAsString()) as Map;
          auxiliaries = exported['auxiliary_journals'] as List;
          expect(auxiliaries, hasLength(2));
          expect(auxiliaries.map((entry) => entry['kind']).toSet(), {
            'import',
            'proposal',
          });
          for (final raw in auxiliaries) {
            final entry = raw as Map;
            expect(entry['closed'], true);
            expect(entry['replay_authority'], false);
            final hex = entry['canonical_card_protobuf_hex'] as String;
            final bytes = [
              for (var i = 0; i < hex.length; i += 2)
                int.parse(hex.substring(i, i + 2), radix: 16),
            ];
            expect(bytes.length.toString(), entry['bytes']);
            expect(sha256.convert(bytes).toString(), entry['sha256']);
          }
        });
        result.addAll({
          'previous_pid': previous['pid'],
          'parent_draft_id': parentId,
          'archive_operation': archived.operation,
          'business_submissions': 0,
          'parent_retirement_preserved': true,
          'auxiliary_journals_exported': auxiliaries.length,
          'unarchived_count': after.unarchivedCount,
          'archived_count': after.archivedCount,
        });
      } else if (phase == 'maintain' || phase == 'maintenance-verify') {
        final previous =
            jsonDecode(await File('$rootPath/reselect.json').readAsString())
                as Map;
        final originalArchive =
            jsonDecode(await File('$rootPath/archive.json').readAsString())
                as Map;
        expect(previous['pid'], isNot(pid));
        expect(originalArchive['pid'], isNot(pid));
        final parentId = originalArchive['parent_draft_id'] as String;
        final history = host.editorDrafts as EditorDraftHistoryControl;
        final maintenance =
            host.editorDrafts as EditorDraftArchiveMaintenanceControl;
        final original = (await fixture.native(
          t,
          host.editorDrafts.read(target, parentId),
        ))!;
        final archive = (await fixture.native(
          t,
          history.inspectArchive(target, parentId),
        ))!;
        expect(original.currentActive, false);
        expect(original.retirement, isNotNull);
        expect(archive.archiveVersion, 1);
        expect(archive.legacyAuxiliaryCount, 2);
        expect(archive.operation, originalArchive['archive_operation']);
        expect(archive.requestSha256, original.requestSha256);
        final before = await fixture.native(t, history.history());
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
              body: EditorDraftHistoryDialog(
                control: host.editorDrafts,
                history: history,
                isCurrent: () => true,
              ),
            ),
          ),
        );
        await fixture.tapVisible(t, 'draft-history-$parentId');
        final currentExport = '$rootPath/$phase-history.json';
        await fixture.native(t, history.exportHistory(original, currentExport));
        late EditorDraftArchiveMaintenanceReceipt receipt;
        if (phase == 'maintain') {
          expect(
            await fixture.native(
              t,
              maintenance.inspectArchiveMaintenance(target, parentId),
            ),
            isNull,
          );
          await fixture.tapVisible(t, 'draft-history-maintain');
          await t.pump();
          final dynamic ui = t.state(find.byType(EditorDraftHistoryDialog));
          expect(ui.lastError, isNull);
          receipt = (await fixture.native(
            t,
            maintenance.inspectArchiveMaintenance(target, parentId),
          ))!;
          expect(receipt.auxiliaryCount, 2);
          expect(receipt.auxiliaryBytes, greaterThan(BigInt.zero));
          final after = await fixture.native(t, history.history());
          expect(after.unarchivedCount, before.unarchivedCount);
          expect(after.archivedCount, before.archivedCount);
          expect(
            after.archivedBytes,
            greaterThan(before.archivedBytes + receipt.auxiliaryBytes),
          );
          final repeated = await fixture.native(
            t,
            maintenance.maintainArchive(original, archive),
          );
          expect(repeated.repeated, true);
          expect(repeated.operation, receipt.operation);
          expect(repeated.auxiliarySha256, receipt.auxiliarySha256);
          expect(
            (await fixture.native(t, history.history())).archivedBytes,
            after.archivedBytes,
          );
          result['archived_bytes_after'] = after.archivedBytes.toString();
        } else {
          final saved =
              jsonDecode(await File('$rootPath/maintain.json').readAsString())
                  as Map;
          expect(saved['pid'], isNot(pid));
          expect(saved['complete'], true);
          receipt = (await fixture.native(
            t,
            maintenance.inspectArchiveMaintenance(target, parentId),
          ))!;
          expect(receipt.operation, saved['maintenance_operation']);
          expect(receipt.auxiliarySha256, saved['auxiliary_sha256']);
          expect(receipt.auxiliaryBytes.toString(), saved['auxiliary_bytes']);
          expect(receipt.archiveSha256, saved['archive_sha256']);
          expect(
            before.archivedBytes.toString(),
            saved['archived_bytes_after'],
          );
          final button = t.widget<TextButton>(
            find.byKey(const ValueKey('draft-history-maintain')),
          );
          expect(button.onPressed, isNull);
          final repeated = await fixture.native(
            t,
            maintenance.maintainArchive(original, archive),
          );
          expect(repeated.repeated, true);
          expect(repeated.operation, receipt.operation);
          expect(
            (await fixture.native(t, history.history())).archivedBytes,
            before.archivedBytes,
          );
          result['maintaining_pid'] = saved['pid'];
        }
        expect(receipt.archiveOperation, archive.operation);
        expect(receipt.archiveSha256, archive.archiveSha256);
        final unchangedArchive = (await fixture.native(
          t,
          history.inspectArchive(target, parentId),
        ))!;
        expect(unchangedArchive.archiveVersion, 1);
        expect(unchangedArchive.archiveSha256, archive.archiveSha256);
        expect(unchangedArchive.slotSha256, archive.slotSha256);
        expect(unchangedArchive.operation, archive.operation);
        final preserved = (await fixture.native(
          t,
          host.editorDrafts.read(target, parentId),
        ))!;
        expect(
          EditorDraftCodec.encodeWrite(preserved.request),
          EditorDraftCodec.encodeWrite(original.request),
        );
        expect(preserved.retirement!.operation, original.retirement!.operation);
        final afterExport = '$rootPath/$phase-after-history.json';
        await fixture.native(t, history.exportHistory(preserved, afterExport));
        await t.runAsync(() async {
          final exported = await File(currentExport).readAsBytes();
          expect(await File(afterExport).readAsBytes(), exported);
          if (phase == 'maintenance-verify') {
            expect(
              exported,
              await File('$rootPath/maintain-history.json').readAsBytes(),
            );
          }
          final payload = jsonDecode(utf8.decode(exported)) as Map;
          final records = payload['auxiliary_journals'] as List;
          expect(records, hasLength(2));
          expect(records.map((record) => record['kind']).toSet(), {
            'import',
            'proposal',
          });
          for (final entry in records) {
            expect(entry['closed'], true);
            expect(entry['replay_authority'], false);
            final hex = entry['canonical_card_protobuf_hex'] as String;
            final bytes = [
              for (var i = 0; i < hex.length; i += 2)
                int.parse(hex.substring(i, i + 2), radix: 16),
            ];
            expect(bytes.length.toString(), entry['bytes']);
            expect(sha256.convert(bytes).toString(), entry['sha256']);
          }
        });
        final content = await fixture.native(
          t,
          host.versionedContent.read(target),
        );
        expect(content.revision, BigInt.one);
        final proof = await fixture.native(
          t,
          host.inspectEditorCommit(
            id: target,
            operation: previous['business_operation'] as String,
          ),
        );
        expect(proof.digest, previous['commit_evidence']);
        result.addAll({
          'previous_pid': previous['pid'],
          'parent_draft_id': parentId,
          'archive_operation': archive.operation,
          'archive_sha256': archive.archiveSha256,
          'maintenance_operation': receipt.operation,
          'auxiliary_count': receipt.auxiliaryCount,
          'auxiliary_bytes': receipt.auxiliaryBytes.toString(),
          'auxiliary_sha256': receipt.auxiliarySha256,
          'original_archive_version_preserved': 1,
          'original_history_bytes_preserved': true,
          'business_submissions': 0,
          'business_revision': '1',
          'new_accounting_only': true,
        });
      } else {
        final previous =
            jsonDecode(await File('$rootPath/reselect.json').readAsString())
                as Map;
        expect(previous['pid'], isNot(pid));
        final content = await fixture.native(
          t,
          host.versionedContent.read(target),
        );
        final shown = await fixture.native(t, host.workspaceRecord(content));
        expect(content.revision, BigInt.one);
        expect(shown.attachments, hasLength(1));
        await t.runAsync(() async {
          expect(await selected.exists(), false);
          expect(
            await File(shown.attachments.single.source.location).readAsBytes(),
            newBytes,
          );
        });
        final proof = await fixture.native(
          t,
          host.inspectEditorCommit(
            id: target,
            operation: previous['business_operation'] as String,
          ),
        );
        expect(proof.digest, previous['commit_evidence']);
        expect(
          (await fixture.native(t, host.loadWorkspaceContent())).length,
          1,
        );
        final archiveFile = File('$rootPath/archive.json');
        if (await archiveFile.exists()) {
          final archived = jsonDecode(await archiveFile.readAsString()) as Map;
          expect(archived['pid'], isNot(pid));
          final history = host.editorDrafts as EditorDraftHistoryControl;
          final receipt = await fixture.native(
            t,
            history.inspectArchive(
              target,
              archived['parent_draft_id'] as String,
            ),
          );
          expect(receipt!.operation, archived['archive_operation']);
          result['archive_verified_after_restart'] = true;
        }
        result.addAll({
          'previous_pid': previous['pid'],
          'business_submissions': 0,
          'business_revision': '1',
          'commit_evidence': proof.digest,
          'attachment_sha256': sha256.convert(newBytes).toString(),
        });
      }
      final prepared = await fixture.native(t, host.prepareEditorDraftClose());
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
    timeout: const Timeout(Duration(minutes: 5)),
  );
}
