import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'package:file_selector/file_selector.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_i18n/morrow_i18n.dart';
import 'package:morrow_studio/attachments/clipboard_import.dart';
import 'package:morrow_studio/main.dart';
import 'package:morrow_studio/plugins/editor_draft.dart';
import 'package:morrow_studio/plugins/editor_draft_codec.dart';
import 'package:morrow_studio/plugins/editor_draft_session.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';

final executable = Platform.environment['MORROW_WORKBENCH_HOST'];
final package = Platform.environment['MORROW_WORKBENCH_PACKAGE'];
final rootPath = Platform.environment['MORROW_CLIPBOARD_TEST_ROOT'];

Future<void> until(WidgetTester tester, bool Function() done) async {
  final deadline = DateTime.now().add(const Duration(seconds: 30));
  while (!done()) {
    if (DateTime.now().isAfter(deadline)) {
      throw StateError('Original clipboard operation did not settle');
    }
    await tester.runAsync(
      () => Future<void>.delayed(const Duration(milliseconds: 20)),
    );
    await tester.pump(const Duration(milliseconds: 20));
  }
}

Future<T> original<T>(WidgetTester tester, Future<T> request) async {
  var done = false;
  late T value;
  Object? failure;
  request.then(
    (result) {
      value = result;
      done = true;
    },
    onError: (Object error) {
      failure = error;
      done = true;
    },
  );
  await until(tester, () => done);
  if (failure != null) throw failure!;
  return value;
}

EditorDraftTextValue empty() => const EditorDraftTextValue(
  text: '',
  selectionBase: 0,
  selectionExtent: 0,
  affinity: 0,
  directional: false,
  composingStart: -1,
  composingEnd: -1,
);

void main() {
  testWidgets(
    'formal title and body pastes retain memory attachments through autosave and restart',
    (tester) async {
      final root = Directory(rootPath!);
      expect(root.existsSync(), isFalse);
      await tester.runAsync(() => root.create());
      try {
        final host = await original(
          tester,
          RustWorkbench.open(
            executable: executable!,
            package: package!,
            directory: Directory('${root.path}/store'),
          ),
        );
        final editor = await original(
          tester,
          host.openEditor('memory-clipboard', create: true),
        );
        final owner = await original(
          tester,
          host.createEditorDraftOwner(
            editor: editor,
            initial: EditorDraftSnapshot(
              values: EditorDraftValues(
                title: empty(),
                description: empty(),
                hypothesis: empty(),
                conclusion: empty(),
                todos: empty(),
                category: '实验',
                stage: '待验证',
              ),
              assets: [],
            ),
            attachments: [],
            create: true,
          ),
        );
        const titleText = 'Windows 原生输入 😀';
        const bodyText = '  第一行 😀\nsecond line  ';
        final titleBytes = utf8.encode('<p>$titleText</p>');
        final bodyBytes = utf8.encode('<p>$bodyText</p>');
        var clipboardReads = 0;
        await tester.pumpWidget(
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
                readClipboard: () async {
                  clipboardReads++;
                  final isTitle = clipboardReads == 1;
                  return PastedContent(
                    text: isTitle ? titleText : bodyText,
                    files: [
                      XFile.fromData(
                        isTitle ? titleBytes : bodyBytes,
                        name: isTitle
                            ? 'title-original.html'
                            : 'body-original.html',
                        path: isTitle
                            ? 'title-original.html'
                            : 'body-original.html',
                      ),
                    ],
                  );
                },
              ),
            ),
          ),
        );
        final title = find.byKey(const ValueKey('idea-title'));
        await tester.enterText(title, 'hello');
        await until(
          tester,
          () => !owner.session.dirty && !owner.session.saving,
        );
        final titleController = tester.widget<TextField>(title).controller!;
        titleController.selection = TextSelection(
          baseOffset: 0,
          extentOffset: titleController.text.length,
        );
        Actions.invoke(
          tester.element(title),
          const PasteTextIntent(SelectionChangedCause.keyboard),
        );
        await until(
          tester,
          () =>
              clipboardReads == 1 &&
              titleController.text == titleText &&
              owner.session.current.assets.length == 1 &&
              !owner.importing &&
              !owner.session.dirty &&
              !owner.session.saving,
        );
        expect(owner.session.captureFailure, isNull);
        expect(owner.hasUnresolvedImports, isFalse);
        final bodyField = find.byKey(const ValueKey('idea-description'));
        await tester.ensureVisible(bodyField);
        await tester.tap(bodyField);
        await tester.pump();
        final body = tester.widget<TextField>(bodyField).controller!;
        Actions.invoke(
          tester.element(bodyField),
          const PasteTextIntent(SelectionChangedCause.keyboard),
        );
        await until(
          tester,
          () =>
              clipboardReads == 2 &&
              body.text == bodyText &&
              owner.session.current.assets.length == 2 &&
              !owner.importing &&
              !owner.session.dirty &&
              !owner.session.saving,
        );
        expect(body.selection.extentOffset, bodyText.length);
        await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
        await tester.sendKeyEvent(LogicalKeyboardKey.home);
        await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
        await tester.pump();
        expect(body.selection, const TextSelection.collapsed(offset: 0));
        await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
        await tester.sendKeyEvent(LogicalKeyboardKey.numpad1);
        await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
        expect(body.selection.extentOffset, bodyText.length);
        await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
        await tester.sendKeyEvent(LogicalKeyboardKey.numpad7);
        await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
        await tester.pump();
        expect(body.selection, const TextSelection.collapsed(offset: 0));
        await tester.sendKeyDownEvent(LogicalKeyboardKey.shiftLeft);
        await tester.sendKeyEvent(LogicalKeyboardKey.arrowRight);
        await tester.sendKeyUpEvent(LogicalKeyboardKey.shiftLeft);
        await tester.pump();
        expect(body.selection.baseOffset, 0);
        expect(body.selection.extentOffset, 1);
        await until(
          tester,
          () => !owner.session.dirty && !owner.session.saving,
        );
        final saved = await original(
          tester,
          host.editorDrafts.read('memory-clipboard', owner.session.draftId),
        );
        expect(saved!.request.values.title.text, titleText);
        expect(saved.request.values.description.text, bodyText);
        expect(saved.request.values.description.selectionBase, 0);
        expect(saved.request.values.description.selectionExtent, 1);
        expect(saved.assets, hasLength(2));
        final views = owner.catalog.viewsForSelections(
          owner.session.current.assets,
        );
        for (final expected in {
          'title-original.html': titleBytes,
          'body-original.html': bodyBytes,
        }.entries) {
          final view = views.singleWhere(
            (view) => view.attachment.source.name == expected.key,
          );
          expect(
            await tester.runAsync(
              () => File(view.attachment.source.location).readAsBytes(),
            ),
            expected.value,
          );
        }
        final inputs = await tester.runAsync(
          () => host.cache
              .list()
              .where(
                (file) =>
                    file is File &&
                    file.uri.pathSegments.last.startsWith('draft-input-'),
              )
              .toList(),
        );
        expect(inputs, hasLength(2));
        final prepared = await original(tester, host.prepareEditorDraftClose());
        await tester.pumpWidget(const SizedBox());
        await original(tester, host.finishEditorDraftClose(prepared));
        await original(tester, host.close());
        expect(inputs!.every((file) => !file.existsSync()), isTrue);
        final reopened = await original(
          tester,
          RustWorkbench.open(
            executable: executable!,
            package: package!,
            directory: Directory('${root.path}/store'),
          ),
        );
        final restored = await original(
          tester,
          reopened.editorDrafts.read('memory-clipboard', saved.request.draftId),
        );
        expect(restored!.generation, saved.generation);
        expect(
          EditorDraftCodec.encodeWrite(restored.request),
          EditorDraftCodec.encodeWrite(saved.request),
        );
        expect(
          await original(tester, reopened.loadWorkspaceContent()),
          isEmpty,
        );
        await original(tester, reopened.close());
        await tester.runAsync(
          () => File('${root.path}/result.json').writeAsString(
            jsonEncode({
              'status':
                  'PASS_FORMAL_DRAFT_MEMORY_CLIPBOARD_AUTOSAVE_AND_RESTART',
              'memory_source_path_not_opened': true,
              'original_owner_closed': true,
              'input_files_removed_after_host_close': true,
              'exact_request_recovered': true,
              'system_clipboard_read': false,
              'framework_input': true,
              'title_and_body_paste_accepted': true,
              'framework_ctrl_home_and_shift_right_accepted': true,
              'framework_keypad_ctrl_home_and_end_accepted': true,
              'native_keyboard_accepted': false,
              'business_submissions': 0,
              'full_P3_accepted': false,
            }),
            flush: true,
          ),
        );
      } catch (error) {
        await tester.runAsync(
          () => File('${root.path}/failure.json').writeAsString(
            jsonEncode({
              'status': 'RETAINED_NOT_ACCEPTED',
              'reason': error.toString(),
              'original_owner_retained': true,
              'replay_authorized': false,
            }),
            flush: true,
          ),
        );
        // Actual host ownership is retained for qualification diagnosis; do not
        // turn a failed assertion into a fabricated close or automatic replay.
        await Completer<void>().future;
      }
    },
    skip:
        !Platform.isWindows ||
        executable == null ||
        package == null ||
        rootPath == null,
    timeout: Timeout.none,
    variant: TargetPlatformVariant.only(TargetPlatform.windows),
  );
}
