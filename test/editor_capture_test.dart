import 'dart:async';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:super_clipboard/super_clipboard.dart';
import 'package:morrow_studio/attachments/attachment.dart';
import 'package:morrow_studio/attachments/clipboard_import.dart';
import 'package:morrow_studio/content/rich_content.dart';
import 'package:morrow_studio/main.dart';
import 'package:morrow_studio/media/texture_source.dart';
import 'package:morrow_studio/plugins/editor_session.dart';
import 'package:morrow_studio/plugins/studio_backend.dart';
import 'rich_capture_test.dart' show FixtureItem;

class TicketStudio implements StudioBackend {
  final calls = <String>[];
  @override
  Future<RichFragment> capture(
    String format,
    String source, {
    String imagePrefix = 'clipboard',
    String? parentTicket,
  }) async {
    calls.add('$format:$parentTicket');
    if (format == 'html' && source == 'broken') {
      throw const FormatException('broken');
    }
    return RichFragment(
      format == 'plain' ? plainTextToMarkdown(source) : '**rich**',
      ticket: 'ticket-${calls.length}',
    );
  }

  @override
  dynamic noSuchMethod(Invocation invocation) => throw UnimplementedError();
}

class EditorFixture implements WorkbenchEditorSession {
  @override
  final targetId = 'editor-card';
  @override
  final studio = TicketStudio();
  final events = <PasteInsertion>[];
  final drafts = <Idea>[];
  final fields = <EditorFields>[];
  final attempts = <Completer<Idea>>[];
  int closed = 0;
  @override
  Future<void> recordPaste(PasteInsertion event) async => events.add(event);
  @override
  Future<Idea> save(Idea draft, EditorFields snapshot) {
    drafts.add(draft);
    fields.add(snapshot);
    final done = Completer<Idea>();
    attempts.add(done);
    return done.future;
  }

  @override
  Future<void> close() async {
    closed++;
  }
}

Idea initial() => Idea(
  'Original',
  'A😀B',
  '灵感',
  Idea.icons[0],
  const Color(0xff8877aa),
  id: 'editor-card',
  attachments: [
    const IdeaAttachment(
      source: TextureSource(
        location: 'synthetic.bin',
        name: 'synthetic.bin',
        kind: TextureKind.file,
        local: true,
      ),
      size: 4,
    ),
  ],
);
Future<void> open(
  WidgetTester tester,
  EditorFixture editor, {
  Future<PastedContent> Function()? clipboard,
}) async {
  await tester.pumpWidget(
    MaterialApp(
      home: Builder(
        builder: (context) => Scaffold(
          body: TextButton(
            onPressed: () => showDialog<Idea>(
              context: context,
              builder: (_) => NewIdeaDialog(
                initialIdea: initial(),
                editor: editor,
                readClipboard: clipboard,
              ),
            ),
            child: const Text('open'),
          ),
        ),
      ),
    ),
  );
  await tester.tap(find.text('open'));
  await tester.pumpAndSettle();
}

void main() {
  test(
    'plain input and selected rich output retain distinct ticket parts',
    () async {
      final plugin = TicketStudio();
      final content = await readPaste(
        ClipboardReader([
          FixtureItem({Formats.plainText: 'A\tB\n1\t2'}),
          FixtureItem({
            Formats.plainText: 'raw html text',
            Formats.htmlText: '<b>rich</b>',
          }),
        ]),
        plugin,
      );
      expect(content.textParts.map((p) => p.ticket), ['ticket-1', '', '']);
      expect(content.textParts.first.selection, 'inputPlainText');
      expect(content.markdownParts.map((p) => p.ticket), [
        'ticket-1',
        '',
        'ticket-2',
      ]);
      expect(content.markdownParts[1].literal, '\n\n');
      expect(content.markdownParts.last.selection, 'outputMarkdown');
    },
  );
  test(
    'failed HTML conversion contributes only the adopted fallback ticket',
    () async {
      final plugin = TicketStudio();
      final content = await readPaste(
        ClipboardReader([
          FixtureItem({
            Formats.plainText: 'fallback',
            Formats.htmlText: 'broken',
          }),
        ]),
        plugin,
      );
      expect(plugin.calls.length, 2);
      expect(content.markdownParts.single.ticket, 'ticket-2');
      expect(content.textParts.single.ticket, 'ticket-2');
      expect(content.markdown, 'fallback');
    },
  );
  testWidgets(
    'UTF16 insertion, manual edit, failed save and stable retry preserve the editor',
    (tester) async {
      final editor = EditorFixture();
      await open(
        tester,
        editor,
        clipboard: () async => const PastedContent(
          text: 'x',
          markdown: '**x**',
          markdownParts: [PastePart.ticket('adopted')],
        ),
      );
      final field = find.byKey(const ValueKey('idea-description'));
      await tester.tap(field);
      await tester.pump();
      final control = tester.widget<TextField>(field).controller!;
      control.selection = const TextSelection(baseOffset: 1, extentOffset: 3);
      Actions.invoke(
        tester.element(
          find.descendant(of: field, matching: find.byType(EditableText)),
        ),
        const PasteTextIntent(SelectionChangedCause.keyboard),
      );
      await tester.pumpAndSettle();
      expect(control.text, 'A**x**B');
      final event = editor.events.single;
      expect(
        [
          event.field,
          event.before,
          event.startUtf16,
          event.endUtf16,
          event.after,
        ],
        ['description', 'A😀B', 1, 3, 'A**x**B'],
      );
      expect(event.parts.single.ticket, 'adopted');
      await tester.enterText(field, 'A**x**B + manual');
      await tester.ensureVisible(find.byKey(const ValueKey('idea-save')));
      await tester.tap(find.byKey(const ValueKey('idea-save')));
      await tester.pump();
      expect(editor.attempts.length, 1);
      expect(
        tester
            .widget<IconButton>(
              find.byWidgetPredicate(
                (w) => w is IconButton && w.tooltip == '关闭弹窗',
              ),
            )
            .onPressed,
        isNull,
      );
      expect(await Navigator.of(tester.element(field)).maybePop(), isTrue);
      await tester.pump();
      expect(find.byType(NewIdeaDialog), findsOneWidget);
      editor.attempts.first.completeError(StateError('lost acknowledgement'));
      await tester.pumpAndSettle();
      expect(editor.closed, 0);
      expect(find.byKey(const ValueKey('idea-save-error')), findsOneWidget);
      expect(tester.widget<TextField>(field).readOnly, isFalse);
      expect(editor.fields.single.description, 'A**x**B + manual');
      expect(editor.drafts.single.attachments.length, 1);
      await tester.tap(find.byKey(const ValueKey('idea-save')));
      await tester.pump();
      expect(identical(editor.drafts[0], editor.drafts[1]), isTrue);
      expect(identical(editor.fields[0], editor.fields[1]), isTrue);
      editor.attempts[1].complete(editor.drafts[1]);
      await tester.pumpAndSettle();
      expect(find.byType(NewIdeaDialog), findsNothing);
      expect(editor.closed, 1);
      expect(tester.takeException(), isNull);
    },
  );
  testWidgets('cancel releases its scope without saving', (tester) async {
    final editor = EditorFixture();
    await open(tester, editor);
    await tester.tap(find.text('再想想'));
    await tester.pumpAndSettle();
    expect(editor.closed, 1);
    expect(editor.attempts, isEmpty);
  });
  testWidgets('late clipboard completion cannot insert after editor disposal', (
    tester,
  ) async {
    final editor = EditorFixture();
    final clipboard = Completer<PastedContent>();
    await open(tester, editor, clipboard: () => clipboard.future);
    await tester.tap(find.byKey(const ValueKey('idea-paste')));
    await tester.pump();
    await tester.pumpWidget(const SizedBox());
    await tester.pump();
    clipboard.complete(
      const PastedContent(
        text: 'late',
        markdown: 'late',
        markdownParts: [PastePart.ticket('late-ticket')],
      ),
    );
    await tester.pumpAndSettle();
    expect(editor.closed, 1);
    expect(editor.events, isEmpty);
    expect(tester.takeException(), isNull);
  });
  testWidgets(
    'known pre-submission error restores editable draft without losing attachments',
    (tester) async {
      final editor = EditorFixture();
      await open(tester, editor);
      await tester.tap(find.byKey(const ValueKey('idea-save')));
      await tester.pump();
      editor.attempts[0].completeError(
        const EditorPreparationException(FormatException('shorten first')),
      );
      await tester.pumpAndSettle();
      final field = find.byKey(const ValueKey('idea-description'));
      expect(tester.widget<TextField>(field).readOnly, isFalse);
      await tester.enterText(field, 'corrected');
      await tester.tap(find.byKey(const ValueKey('idea-save')));
      await tester.pump();
      expect(editor.drafts[1].description, 'corrected');
      expect(editor.drafts[1].attachments.length, 1);
      editor.attempts[1].complete(editor.drafts[1]);
      await tester.pumpAndSettle();
      expect(editor.closed, 1);
    },
  );
}
