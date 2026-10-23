// Actual Windows SendInput is supplied externally; the fixture never assigns
// editor values or sends framework text-input events.
import 'dart:convert';
import 'dart:io';
import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_i18n/morrow_i18n.dart';
import 'package:morrow_studio/main.dart';
import 'package:morrow_studio/plugins/editor_draft_codec.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';
import 'editor_draft_review_process_test.dart' as fixture;

final rootPath = fixture.rootPath, phase = fixture.phase;
final available =
    Platform.isWindows &&
    fixture.hostPath != null &&
    fixture.packagePath != null &&
    rootPath != null &&
    ['os-edit', 'os-verify'].contains(phase);
const target = 'native-os-input-original';
const nativeTitle = 'Windows 原生输入 😀';
const nativeBody = '  第一行 😀\nsecond line  ';

Widget view(Widget child) => MaterialApp(
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

void main({Future<void> Function(WidgetTester)? prepareWindow}) {
  testWidgets(
    'formal editor receives native OS input and survives restart ($phase)',
    (t) async {
      if (prepareWindow == null) {
        throw StateError('Native Windows wrapper required');
      }
      await prepareWindow(t);
      final root = Directory(rootPath!), store = Directory('$rootPath/store');
      late RustWorkbench host;
      await t.runAsync(() async {
        expect(await store.exists(), phase == 'os-verify');
        await root.create(recursive: true);
        host = await RustWorkbench.open(
          executable: fixture.hostPath!,
          package: fixture.packagePath!,
          directory: store,
        );
      });
      final result = <String, Object?>{
        'pid': pid,
        'phase': phase,
        'complete': false,
        'input': 'external_windows_sendinput',
        'framework_text_injection': false,
        'hardware_keyboard_accepted': false,
        'ime_composition_accepted': false,
        'business_submissions': 0,
      };
      if (phase == 'os-edit') {
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
        await t.pumpWidget(
          view(
            NewIdeaDialog(
              key: const ValueKey(target),
              editor: owner.editor,
              draftOwner: owner,
              draftRuntime: host,
              initialCategory: '实验',
            ),
          ),
        );
        await t.pumpAndSettle();
        final fields = fixture.controllers(t);
        expect(fields[0].text, '');
        expect(fields[1].text, '');
        await t.runAsync(
          () => File('$rootPath/native-input-ready.json').writeAsString(
            jsonEncode({
              'pid': pid,
              'phase': phase,
              'title': nativeTitle,
              'body': nativeBody,
              'description_selection_base': 0,
              'description_selection_extent': 1,
              'complete': false,
            }),
            flush: true,
          ),
        );
        final deadline = DateTime.now().add(const Duration(minutes: 15));
        while (!File('$rootPath/native-input-finished.json').existsSync()) {
          if (DateTime.now().isAfter(deadline)) {
            throw StateError(
              'Native input not completed; preserve original run',
            );
          }
          await t.runAsync(
            () => Future<void>.delayed(const Duration(milliseconds: 50)),
          );
          await t.pump(const Duration(milliseconds: 50));
          final error = t.takeException();
          if (error != null) {
            throw error;
          }
        }
        final confirmation =
            jsonDecode(
                  File(
                    '$rootPath/native-input-finished.json',
                  ).readAsStringSync(),
                )
                as Map;
        expect(confirmation['pid'], pid);
        expect(confirmation['input'], 'windows_sendinput_via_computer_use');
        expect(fields[0].text, nativeTitle);
        expect(fields[1].text, nativeBody);
        expect(fields[1].selection.baseOffset, 0);
        expect(fields[1].selection.extentOffset, 1);
        expect(fields[1].value.composing, TextRange.empty);
        // Natural autosave and its original owner must settle; no injected Save
        // or controller assignment is used to manufacture this persisted state.
        await fixture.waitFor(
          t,
          () =>
              !owner.session.dirty &&
              !owner.session.saving &&
              owner.session.confirmed != null,
        );
        final original = owner.session.confirmed!;
        expect(original.request.values.title.text, nativeTitle);
        expect(original.request.values.description.text, nativeBody);
        expect(original.request.values.description.selectionBase, 0);
        expect(original.request.values.description.selectionExtent, 1);
        final stored = (await fixture.native(
          t,
          host.editorDrafts.read(target, owner.session.draftId),
        ))!;
        expect(
          EditorDraftCodec.encodeWrite(stored.request),
          EditorDraftCodec.encodeWrite(original.request),
        );
        final prepared = await fixture.native(
          t,
          host.prepareEditorDraftClose(),
        );
        result.addAll({
          'draft_id': stored.request.draftId,
          'generation': stored.generation.toString(),
          'request_base64': base64Encode(
            EditorDraftCodec.encodeWrite(stored.request),
          ),
          'native_os_input_accepted': true,
          'original_commit_operation': stored.request.commitOperation,
        });
        await t.pumpWidget(const SizedBox());
        await fixture.native(t, host.finishEditorDraftClose(prepared));
      } else {
        final previous =
            jsonDecode(File('$rootPath/os-edit.json').readAsStringSync())
                as Map;
        expect(previous['complete'], true);
        expect(previous['pid'], isNot(pid));
        final stored = (await fixture.native(
          t,
          host.editorDrafts.read(target, previous['draft_id'] as String),
        ))!;
        expect(stored.currentActive, true);
        expect(stored.generation.toString(), previous['generation']);
        expect(
          base64Encode(EditorDraftCodec.encodeWrite(stored.request)),
          previous['request_base64'],
        );
        expect(
          stored.request.commitOperation,
          previous['original_commit_operation'],
        );
        expect(stored.request.values.title.text, nativeTitle);
        expect(stored.request.values.description.text, nativeBody);
        expect(stored.request.values.description.selectionBase, 0);
        expect(stored.request.values.description.selectionExtent, 1);
        await t.pumpWidget(
          view(
            Column(
              children: [
                Text(stored.request.values.title.text),
                Text(stored.request.values.description.text),
              ],
            ),
          ),
        );
        await t.pumpAndSettle();
        result.addAll({
          'previous_pid': previous['pid'],
          'original_request_preserved': true,
          'draft_id': stored.request.draftId,
          'generation': stored.generation.toString(),
          'request_base64': base64Encode(
            EditorDraftCodec.encodeWrite(stored.request),
          ),
        });
      }
      await fixture.native(
        t,
        expectLater(host.versionedContent.read(target), throwsA(anything)),
      );
      await t.pumpWidget(const SizedBox());
      await fixture.native(t, host.close());
      result['complete'] = true;
      await t.runAsync(
        () => File(
          '$rootPath/$phase.json',
        ).writeAsString(jsonEncode(result), flush: true),
      );
    },
    skip: !available,
    timeout: const Timeout(Duration(minutes: 18)),
  );
}
