import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:super_clipboard/super_clipboard.dart';
import 'package:morrow_studio/attachments/clipboard_import.dart';
import 'package:morrow_studio/collapsible_panel.dart';
import 'package:morrow_studio/content/rich_content.dart';
import 'package:morrow_studio/content/idea_markdown.dart';
import 'package:morrow_studio/main.dart';
import 'package:morrow_studio/storage.dart';

class FixtureItem extends ClipboardDataReader {
  FixtureItem(this.values, {this.broken = false});
  final Map<DataFormat, Object> values;
  final bool broken;
  @override
  List<String> get platformFormats => ['fixture'];
  @override
  List<DataFormat> getFormats(List<DataFormat> allFormats) =>
      allFormats.where(values.containsKey).toList();
  @override
  Future<T?> readValue<T extends Object>(ValueFormat<T> format) async {
    if (broken) throw const FormatException('无法读取此项');
    return values[format] as T?;
  }

  @override
  Future<String?> getSuggestedName() async => null;
  @override
  ReadProgress? getFile(
    FileFormat? format,
    AsyncValueChanged<DataReaderFile> onFile, {
    ValueChanged<Object>? onError,
    bool allowVirtualFiles = true,
    bool synthesizeFilesFromURIs = true,
  }) => null;
  @override
  dynamic noSuchMethod(Invocation invocation) => super.noSuchMethod(invocation);
}

void main() {
  test('Word HTML keeps structure and rejects active content', () {
    final rich = htmlToMarkdown(
      '<h2>研究</h2><p><b>重点</b> 与 <i>观察</i></p><ul><li>一步</li></ul>'
      '<script>alert(1)</script><a href="javascript:alert(1)">危险链接</a>',
    );
    expect(rich.markdown, contains('## 研究'));
    expect(rich.markdown, contains('**重点**'));
    expect(rich.markdown, contains('*观察*'));
    expect(rich.markdown, contains('- 一步'));
    expect(rich.markdown, isNot(contains('alert')));
    expect(rich.markdown, contains('危险链接'));
  });
  test('Excel table values and formulas remain readable without execution', () {
    final rich = spreadsheetToMarkdown(
      '<Workbook xmlns:ss="urn:schemas-microsoft-com:office:spreadsheet">'
      '<Worksheet><Table><Row><Cell><Data>名称</Data></Cell><Cell><Data>结果</Data></Cell></Row>'
      '<Row><Cell><Data>合计</Data></Cell><Cell ss:Formula="=SUM(R[-1]C:R[-1]C)"><Data>42</Data></Cell></Row>'
      '</Table></Worksheet></Workbook>',
    );
    expect(rich.markdown, contains('| 名称 | 结果 |'));
    expect(rich.markdown, contains('42 (公式: =SUM'));
    expect(rich.warnings, isNotEmpty);
    final merged = htmlToMarkdown(
      '<table><tr><td rowspan="2">跨行</td><td>A</td></tr><tr><td>B</td></tr></table>',
    );
    expect(merged.markdown, contains('|  | B |'));
    expect(
      plainTextToMarkdown('A\tB\n1\t2'),
      '| A | B |\n| --- | --- |\n| 1 | 2 |',
    );
    expect(
      markdownTable([
        ['a|b', 'c'],
        ['1'],
      ]),
      contains(r'a\|b'),
    );
  });
  test(
    'RTF unicode, groups and paragraph fallback exclude embedded payloads',
    () {
      expect(
        rtfToPlainText(
          r'{\rtf1\ansi\uc1 {\fonttbl secret}\u20013?\u25991?\par next{\*\objdata payload}}',
        ),
        '中文\nnext',
      );
      expect(
        decodeClipboardText([255, 254, 0x2d, 0x4e, 0x87, 0x65, 0, 0]),
        '中文',
      );
    },
  );
  test(
    'Inline images are persisted and external/local URLs are constrained',
    () async {
      final rich = htmlToMarkdown(
        '<p><img src="data:image/png;base64,AQID" alt="图"></p>',
      );
      expect(await rich.files.single.readAsBytes(), [1, 2, 3]);
      expect(rich.markdown, contains('attachment:clipboard-1.png'));
      for (final link in [
        'javascript:alert(1)',
        'file:///C:/secret',
        'data:text/html,hello',
        'https://user:pass@example.com',
      ]) {
        expect(safeContentLink(link), isNull);
      }
      expect(safeContentLink('https://example.com'), 'https://example.com');
      expect(
        () => htmlToMarkdown('x' * (2 * 1024 * 1024 + 1)),
        throwsFormatException,
      );
    },
  );
  test(
    'Clipboard aggregates rich and plain items and preserves original HTML',
    () async {
      final content = await readPaste(
        ClipboardReader([
          FixtureItem({
            Formats.plainText: '标题',
            Formats.htmlText: '<h1>标题</h1>',
          }),
          FixtureItem({}, broken: true),
          FixtureItem({Formats.plainText: 'A\tB\n1\t2'}),
        ]),
      );
      expect(content.markdown, contains('# 标题'));
      expect(content.markdown, contains('| 1 | 2 |'));
      expect(content.text, contains('标题'));
      expect(content.files.length, 2);
      final htmlFile = content.files.firstWhere(
        (f) => f.name.endsWith('.html'),
      );
      expect(content.files.any((f) => f.name.endsWith('.tsv')), isTrue);
      expect(await htmlFile.readAsString(), '<h1>标题</h1>');
      expect(content.warnings, contains('无法读取此项'));
    },
  );
  test(
    'Malformed rich text falls back to plain text with a visible warning',
    () async {
      final content = await readPaste(
        ClipboardReader([
          FixtureItem({
            Formats.plainText: '可读文字',
            Formats.htmlText: 'x' * (2 * 1024 * 1024 + 1),
          }),
        ]),
      );
      expect(content.markdown, '可读文字');
      expect(content.warnings, isNotEmpty);
    },
  );
  testWidgets(
    'Panels animate through intermediate widths and preserve editor state on reversal',
    (tester) async {
      var expanded = true;
      late StateSetter change;
      final controller = TextEditingController(text: '保留输入');
      addTearDown(controller.dispose);
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: StatefulBuilder(
              builder: (context, setState) {
                change = setState;
                return Row(
                  children: [
                    CollapsiblePanel(
                      key: const ValueKey('panel'),
                      expanded: expanded,
                      axis: Axis.horizontal,
                      extent: 220,
                      child: TextField(controller: controller),
                    ),
                    const Expanded(child: SizedBox()),
                  ],
                );
              },
            ),
          ),
        ),
      );
      final panel = find.byKey(const ValueKey('panel'));
      expect(tester.getSize(panel).width, 220);
      change(() => expanded = false);
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 150));
      expect(tester.getSize(panel).width, inExclusiveRange(0, 220));
      change(() => expanded = true);
      await tester.pumpAndSettle();
      expect(tester.getSize(panel).width, 220);
      expect(controller.text, '保留输入');
      change(() => expanded = false);
      await tester.pumpAndSettle();
      expect(tester.getSize(panel).width, 0);
      expect(
        tester.widget<ExcludeFocus>(find.byType(ExcludeFocus).last).excluding,
        isTrue,
      );
      expect(tester.takeException(), isNull);
    },
  );
  testWidgets(
    'Rich paste edits Markdown at the caret and renders a live preview on narrow screens',
    (tester) async {
      tester.view.physicalSize = const Size(390, 844);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: NewIdeaDialog(
              readClipboard: () async =>
                  const PastedContent(text: '标题', markdown: '**标题**'),
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();
      final editor = find.byKey(const ValueKey('idea-description'));
      await tester.enterText(editor, '开始结束');
      final control = tester.widget<TextField>(editor).controller!;
      control.selection = const TextSelection.collapsed(offset: 2);
      Actions.invoke(
        tester.element(
          find.descendant(of: editor, matching: find.byType(EditableText)),
        ),
        const PasteTextIntent(SelectionChangedCause.keyboard),
      );
      await tester.pumpAndSettle();
      expect(control.text, '开始**标题**结束');
      expect(find.byType(IdeaMarkdown), findsOneWidget);
      expect(tester.takeException(), isNull);
    },
  );
  testWidgets('Paste cannot bypass title character limit', (tester) async {
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: NewIdeaDialog(
            readClipboard: () async => PastedContent(text: 'x' * 61),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();
    final title = find.byKey(const ValueKey('idea-title'));
    await tester.enterText(title, '原题');
    Actions.invoke(
      tester.element(
        find.descendant(of: title, matching: find.byType(EditableText)),
      ),
      const PasteTextIntent(SelectionChangedCause.keyboard),
    );
    await tester.pumpAndSettle();
    expect(tester.widget<TextField>(title).controller!.text, '原题');
    expect(find.textContaining('最多 60'), findsOneWidget);
  });
  testWidgets(
    'Sidebar, settings and canvas liquid preferences survive restore',
    (tester) async {
      tester.view.physicalSize = const Size(1440, 1000);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      final storage = MemoryStorage();
      await tester.pumpWidget(MorrowApp(storage: storage));
      await tester.pumpAndSettle();
      await tester.tap(find.byKey(const ValueKey('sidebar-toggle')));
      await tester.pumpAndSettle();
      final transparent = find.byKey(const ValueKey('background-transparent'));
      await tester.ensureVisible(transparent);
      await tester.tap(transparent);
      await tester.pumpAndSettle();
      final liquid = find.byKey(const ValueKey('canvas-liquid-toggle'));
      await tester.ensureVisible(liquid);
      await tester.tap(liquid);
      await tester.pumpAndSettle();
      expect(storage.data!['sidebarExpanded'], false);
      expect(storage.data!['liquidCanvas'], true);
      await tester.tap(find.byKey(const ValueKey('appearance-toggle')));
      await tester.pumpAndSettle();
      expect(storage.data!['appearanceExpanded'], false);
      await tester.pumpWidget(const SizedBox());
      await tester.pumpWidget(MorrowApp(storage: storage));
      await tester.pumpAndSettle();
      expect(
        tester.getSize(find.byKey(const ValueKey('sidebar-panel'))).width,
        0,
      );
      expect(
        tester.getSize(find.byKey(const ValueKey('settings-side-panel'))).width,
        0,
      );
      expect(
        tester.widget<Studio>(find.byType(Studio)).palette.liquidCanvas,
        true,
      );
      expect(tester.takeException(), isNull);
    },
  );
}
