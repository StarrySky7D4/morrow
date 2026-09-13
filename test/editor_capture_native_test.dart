import 'dart:io';
import 'package:flutter/material.dart';
import 'package:super_clipboard/super_clipboard.dart';
import 'package:morrow_studio/attachments/clipboard_import.dart';
import 'rich_capture_test.dart' show FixtureItem;
import 'plugin_tools_native_test.dart' show pumpHost;
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/main.dart';
import 'package:morrow_studio/attachments/attachment.dart';
import 'package:morrow_studio/media/texture_source.dart';
import 'package:morrow_studio/plugins/editor_session.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';

Future<T> hostFuture<T>(WidgetTester tester, Future<T> request) async {
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
  await pumpHost(tester, () => done);
  if (failure != null) throw failure!;
  return value;
}

void main() {
  final executable = Platform.environment['MORROW_WORKBENCH_HOST'];
  final package = Platform.environment['MORROW_WORKBENCH_PACKAGE'];
  test(
    'actual scoped paste saves UTF16 edits, Dart normalization, aliases and stable historical retry',
    () async {
      final directory = await Directory.systemTemp.createTemp(
        'morrow-editor-capture-',
      );
      RustWorkbench? backend;
      WorkbenchEditorSession? editor;
      try {
        backend = await RustWorkbench.open(
          executable: executable!,
          package: package!,
          directory: directory,
        );
        editor = await backend.openEditor('capture-native', create: true);
        final converted = await editor.studio.capture('plain', 'A\tB\n1\t2');
        expect(converted.ticket, isNotEmpty);
        const before = 'A😀B';
        final after = before.replaceRange(1, 3, converted.markdown);
        await editor.recordPaste(
          PasteInsertion(
            id: 'paste-emoji',
            field: 'description',
            before: before,
            startUtf16: 1,
            endUtf16: 3,
            parts: [PastePart.ticket(converted.ticket!)],
            after: after,
          ),
        );
        const filename = "原件 !~*'().bin";
        final file = File('${directory.path}/source file.bin');
        await file.writeAsBytes([0, 1, 127, 255]);
        final rawDescription =
            '\ufeff\u00a0\u0085$after + manual\n'
            '[original](attachment:${Uri.encodeComponent(file.path)})\n'
            'attachment:$filename\u00a0keep\nattachment:$filename\u0085keep\ufeff';
        final fields = EditorFields(
          title: '\ufeff\u00a0\u0085Native\u0085',
          description: rawDescription,
          hypothesis: '\u0085 hyp \ufeff',
          conclusion: '\u00a0 result \u0085',
          todos: '\ufeff one \u0085\none\n\u00a0 two \u00a0\n\none',
        );
        final draft = Idea(
          fields.title.trim(),
          fields.description.trim(),
          '灵感',
          Idea.icons[0],
          const Color(0xff8877aa),
          id: editor.targetId,
          hypothesis: fields.hypothesis.trim(),
          conclusion: fields.conclusion.trim(),
          todos: fields.todos
              .split('\n')
              .map((v) => v.trim())
              .where((v) => v.isNotEmpty)
              .toSet()
              .toList(),
          attachments: [
            IdeaAttachment(
              source: TextureSource(
                location: file.path,
                name: filename,
                kind: TextureKind.file,
                local: true,
              ),
              size: 4,
            ),
          ],
        );
        final saved = await editor.save(draft, fields);
        expect(saved.title, 'Native');
        expect(saved.hypothesis, 'hyp');
        expect(saved.conclusion, 'result');
        expect(saved.todos, ['one', 'two']);
        final asset = saved.attachments.single.pluginId!;
        expect(saved.description, startsWith('$after + manual'));
        expect(saved.description, contains('[original](attachment:$asset)'));
        expect(saved.description, contains('attachment:$asset\u00a0keep'));
        expect(saved.description, contains('attachment:$filename\u0085keep'));
        final retry = await editor.save(draft, fields);
        expect(retry.description, saved.description);
        expect(await backend.load(), hasLength(1));
        draft.title = 'different intention';
        await expectLater(editor.save(draft, fields), throwsStateError);
        await editor.close();
        editor = await backend.openEditor('capture-native', create: false);
        final rtf = await editor.studio.capture('rtf', r'{\rtf1\ansi Alpha}');
        final plain = await editor.studio.capture(
          'plain',
          rtf.markdown,
          parentTicket: rtf.ticket,
        );
        expect(plain.ticket, isNotEmpty);
        await expectLater(
          editor.studio.capture('plain', 'different', parentTicket: rtf.ticket),
          throwsStateError,
        );
        await editor.recordPaste(
          PasteInsertion(
            id: 'paste-rtf-chain',
            field: 'title',
            before: saved.title,
            startUtf16: 0,
            endUtf16: saved.title.length,
            parts: [
              PastePart.ticket(plain.ticket!, selection: 'inputPlainText'),
            ],
            after: rtf.markdown,
          ),
        );
        final editedFields = EditorFields(
          title: rtf.markdown,
          description: saved.description,
          hypothesis: saved.hypothesis,
          conclusion: saved.conclusion,
          todos: saved.todos.join('\n'),
        );
        final edited = Idea(
          editedFields.title.trim(),
          editedFields.description.trim(),
          saved.category,
          saved.icon,
          saved.color,
          id: saved.id,
          attachments: saved.attachments,
          hypothesis: saved.hypothesis,
          conclusion: saved.conclusion,
          todos: saved.todos,
          stage: saved.stage,
        );
        final result = await editor.save(edited, editedFields);
        expect(result.title, 'Alpha');
        await editor.close();
        editor = await backend.openEditor('cancelled-target', create: true);
        await editor.studio.capture('plain', 'cancelled conversion');
        await editor.close();
        await expectLater(
          editor.recordPaste(
            const PasteInsertion(
              id: 'late',
              field: 'title',
              before: '',
              startUtf16: 0,
              endUtf16: 0,
              parts: [PastePart.literal('late')],
              after: 'late',
            ),
          ),
          throwsStateError,
        );
        expect(await backend.load(), hasLength(1));
      } finally {
        try {
          await editor?.close();
        } catch (_) {}
        await backend?.close();
        expect(directory.path.startsWith(Directory.systemTemp.path), isTrue);
        expect(
          directory.uri.pathSegments.where((v) => v.isNotEmpty).last,
          startsWith('morrow-editor-capture-'),
        );
        await directory.delete(recursive: true);
      }
    },
    skip: !Platform.isWindows || executable == null || package == null,
    timeout: const Timeout(Duration(minutes: 3)),
  );
  testWidgets(
    'production dialog reads converted paste and commits through its real editor session',
    (tester) async {
      Directory? directory;
      RustWorkbench? backend;
      WorkbenchEditorSession? editor;
      Idea? returned;
      try {
        await tester.runAsync(() async {
          directory = await Directory.systemTemp.createTemp(
            'morrow-editor-dialog-',
          );
          backend = await RustWorkbench.open(
            executable: executable!,
            package: package!,
            directory: directory!,
          );
          editor = await backend!.openEditor('dialog-native', create: true);
        });
        await tester.pumpWidget(
          MaterialApp(
            home: Builder(
              builder: (context) => Scaffold(
                body: TextButton(
                  onPressed: () async {
                    returned = await showDialog<Idea>(
                      context: context,
                      builder: (_) => NewIdeaDialog(
                        editor: editor,
                        importAttachment: (file) async {
                          final path = File('${directory!.path}/${file.name}');
                          await path.writeAsBytes(await file.readAsBytes());
                          return IdeaAttachment(
                            source: TextureSource(
                              location: path.path,
                              name: file.name,
                              kind: TextureKind.file,
                              local: true,
                            ),
                            size: await path.length(),
                          );
                        },
                        readClipboard: () => readPaste(
                          ClipboardReader([
                            FixtureItem({Formats.plainText: 'A\tB\n1\t2'}),
                          ]),
                          editor!.studio,
                        ),
                      ),
                    );
                  },
                  child: const Text('open native editor'),
                ),
              ),
            ),
          ),
        );
        await tester.tap(find.text('open native editor'));
        await tester.pumpAndSettle();
        await tester.enterText(
          find.byKey(const ValueKey('idea-title')),
          '\u0085 Dialog \ufeff',
        );
        final field = find.byKey(const ValueKey('idea-description'));
        await tester.enterText(field, 'start😀end');
        final controller = tester.widget<TextField>(field).controller!;
        controller.selection = const TextSelection(
          baseOffset: 5,
          extentOffset: 7,
        );
        Actions.invoke(
          tester.element(
            find.descendant(of: field, matching: find.byType(EditableText)),
          ),
          const PasteTextIntent(SelectionChangedCause.keyboard),
        );
        await tester.pump();
        await pumpHost(tester, () => controller.text != 'start😀end');
        expect(controller.text, contains('| A | B |'));
        await pumpHost(
          tester,
          () => find.byType(LinearProgressIndicator).evaluate().isEmpty,
        );
        final finalDescription = '${controller.text}\nmanual addition';
        await tester.enterText(field, finalDescription);
        await tester.ensureVisible(find.byKey(const ValueKey('idea-save')));
        await tester.tap(find.byKey(const ValueKey('idea-save')));
        await tester.pump();
        await pumpHost(
          tester,
          () =>
              returned != null ||
              find
                  .byKey(const ValueKey('idea-save-error'))
                  .evaluate()
                  .isNotEmpty,
        );
        expect(returned, isNotNull);
        await tester.pumpAndSettle();
        expect(find.byType(NewIdeaDialog), findsNothing);
        expect(returned!.title, 'Dialog');
        expect(returned!.attachments, hasLength(1));
        expect(returned!.description, finalDescription);
        final stored = await hostFuture(tester, backend!.load());
        expect(stored, hasLength(1));
        expect(stored.single.id, 'dialog-native');
        expect(stored.single.description, finalDescription);
        expect(tester.takeException(), isNull);
      } finally {
        await tester.pumpWidget(const SizedBox());
        if (editor != null) {
          try {
            await hostFuture(tester, editor!.close());
          } catch (_) {}
        }
        await tester.runAsync(() async {
          await backend?.close();
          if (directory != null) {
            expect(
              directory!.path.startsWith(Directory.systemTemp.path),
              isTrue,
            );
            expect(
              directory!.uri.pathSegments.where((v) => v.isNotEmpty).last,
              startsWith('morrow-editor-dialog-'),
            );
            await directory!.delete(recursive: true);
          }
        });
      }
    },
    skip: !Platform.isWindows || executable == null || package == null,
    timeout: const Timeout(Duration(minutes: 3)),
  );
  test(
    'committed edit with failed attachment materialization cannot advance stale UI revision',
    () async {
      final directory = await Directory.systemTemp.createTemp(
        'morrow-editor-recovery-',
      );
      RustWorkbench? backend;
      WorkbenchEditorSession? editor;
      try {
        backend = await RustWorkbench.open(
          executable: executable!,
          package: package!,
          directory: directory,
        );
        final source = File('${directory.path}/source.bin');
        await source.writeAsBytes([4, 3, 2, 1]);
        editor = await backend.openEditor('recover-card', create: true);
        const fields = EditorFields(
          title: 'original',
          description: 'original body',
          hypothesis: '',
          conclusion: '',
          todos: '',
        );
        final draft = Idea(
          'original',
          'original body',
          '灵感',
          Idea.icons[0],
          const Color(0xff8877aa),
          id: 'recover-card',
          attachments: [
            IdeaAttachment(
              source: TextureSource(
                location: source.path,
                name: 'source.bin',
                kind: TextureKind.file,
                local: true,
              ),
              size: 4,
            ),
          ],
        );
        final original = await editor.save(draft, fields);
        await editor.close();
        editor = await backend.openEditor('recover-card', create: false);
        await source.delete();
        expect(original.attachments.single.source.location, isNot(source.path));
        expect(
          await File(original.attachments.single.source.location).readAsBytes(),
          [4, 3, 2, 1],
        );
        await File(original.attachments.single.source.location).delete();
        // This entire directory is test-owned. A file at the preview-directory path makes
        // the post-commit export fail; the host's authoritative blob remains untouched.
        await backend.cache.delete();
        final obstruction = File(backend.cache.path);
        await obstruction.writeAsString('blocked cache');
        const changedFields = EditorFields(
          title: 'committed edit',
          description: 'new body',
          hypothesis: '',
          conclusion: '',
          todos: '',
        );
        final changed = Idea(
          'committed edit',
          'new body',
          '灵感',
          original.icon,
          original.color,
          id: original.id,
          attachments: original.attachments,
        );
        await expectLater(
          editor.save(changed, changedFields),
          throwsStateError,
        );
        await editor.close();
        editor = null;
        // If a failed _idea had advanced _revisions, this old UI would receive a fresh scope.
        await expectLater(
          backend.openEditor('recover-card', create: false),
          throwsStateError,
        );
        await obstruction.delete();
        await backend.cache.create();
        final refreshed = await backend.refreshEditorContent();
        expect(refreshed, hasLength(1));
        expect(refreshed.single.title, 'committed edit');
        final restored = File(
          refreshed.single.attachments.single.source.location,
        );
        expect(await restored.readAsBytes(), [4, 3, 2, 1]);
        expect(restored.path, isNot(source.path));
        editor = await backend.openEditor('recover-card', create: false);
        await editor.close();
        editor = null;
        expect((await backend.load()).single.title, 'committed edit');
      } finally {
        try {
          await editor?.close();
        } catch (_) {}
        await backend?.close();
        expect(directory.path.startsWith(Directory.systemTemp.path), isTrue);
        expect(
          directory.uri.pathSegments.where((v) => v.isNotEmpty).last,
          startsWith('morrow-editor-recovery-'),
        );
        await directory.delete(recursive: true);
      }
    },
    skip: !Platform.isWindows || executable == null || package == null,
    timeout: const Timeout(Duration(minutes: 3)),
  );

  test(
    'proven pre-submission capacity error allows a corrected draft in the same editor scope',
    () async {
      final directory = await Directory.systemTemp.createTemp(
        'morrow-editor-preflight-',
      );
      RustWorkbench? backend;
      WorkbenchEditorSession? editor;
      try {
        backend = await RustWorkbench.open(
          executable: executable!,
          package: package!,
          directory: directory,
        );
        editor = await backend.openEditor('preflight-card', create: true);
        final tooLarge = '中' * 22000;
        final draft = Idea(
          'title',
          tooLarge,
          '灵感',
          Idea.icons[0],
          const Color(0xff8877aa),
          id: editor.targetId,
        );
        await expectLater(
          editor.save(
            draft,
            EditorFields(
              title: 'title',
              description: tooLarge,
              hypothesis: '',
              conclusion: '',
              todos: '',
            ),
          ),
          throwsA(isA<EditorPreparationException>()),
        );
        expect(await backend.load(), isEmpty);
        draft.description = 'corrected';
        final saved = await editor.save(
          draft,
          const EditorFields(
            title: 'title',
            description: 'corrected',
            hypothesis: '',
            conclusion: '',
            todos: '',
          ),
        );
        expect(saved.description, 'corrected');
        expect(await backend.load(), hasLength(1));
      } finally {
        try {
          await editor?.close();
        } catch (_) {}
        await backend?.close();
        expect(directory.path.startsWith(Directory.systemTemp.path), isTrue);
        expect(
          directory.uri.pathSegments.where((v) => v.isNotEmpty).last,
          startsWith('morrow-editor-preflight-'),
        );
        await directory.delete(recursive: true);
      }
    },
    skip: !Platform.isWindows || executable == null || package == null,
    timeout: const Timeout(Duration(minutes: 3)),
  );
}
