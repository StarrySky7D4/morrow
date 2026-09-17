import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_i18n/morrow_i18n.dart';
import 'package:morrow_studio/attachments/clipboard_import.dart';
import 'package:morrow_studio/attachments/import_notices.dart';
import 'package:super_clipboard/super_clipboard.dart';

class _Item extends ClipboardDataReader {
  _Item(this.values, {this.error});
  final Map<DataFormat, Object> values;
  final String? error;
  @override
  List<String> get platformFormats => ['fixture'];
  @override
  List<DataFormat> getFormats(List<DataFormat> allFormats) =>
      allFormats.where(values.containsKey).toList();
  @override
  Future<T?> readValue<T extends Object>(ValueFormat<T> format) async {
    if (error case final message?) throw FormatException(message);
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
  final en = L10n.forLocale(const Locale('en'));
  final zh = L10n.forLocale(const Locale('zh'));
  test(
    'per-call paste language changes notices only and never pasted data',
    () async {
      const text = '名称\t值\n用户中文\t42';
      final reader = ClipboardReader([
        _Item({Formats.plainText: text}),
      ]);
      final results = await Future.wait([
        readPaste(reader, null, en),
        readPaste(reader, null, zh),
      ]);
      expect(results[0].text, text);
      expect(results[1].text, text);
      expect(results[0].markdown, results[1].markdown);
      expect(results[0].warnings, contains(en.importsTableConverted));
      expect(results[1].warnings, contains(zh.importsTableConverted));
      expect(await results[0].files.single.readAsString(), text);
      final fallback = await readPaste(reader);
      expect(fallback.warnings, contains(zh.importsTableConverted));
    },
  );
  test(
    'size and item limits show current language and keep readable content',
    () async {
      final reader = ClipboardReader([
        _Item({Formats.plainText: 'x' * (2 * 1024 * 1024 + 1)}),
        ...List.generate(20, (i) => _Item({Formats.plainText: '原文$i'})),
      ]);
      final result = await readPaste(reader, null, en);
      expect(result.warnings, contains(en.importsTextTooLarge));
      expect(result.warnings, contains(en.importsItemLimit));
      expect(result.text, contains('原文0'));
      expect(result.text, isNot(contains('原文19')));
    },
  );
  test(
    'known rich import diagnostics localize but unknown messages stay exact',
    () async {
      final reader = ClipboardReader([
        _Item({
          Formats.htmlText: '<table><tr><td colspan="2">用户内容</td></tr></table>',
        }),
        _Item({}, error: '来自外部工具的原始错误: 用户值'),
      ]);
      final result = await readPaste(reader, null, en);
      expect(result.warnings, contains(en.importsMergedTable));
      expect(result.warnings, contains('来自外部工具的原始错误: 用户值'));
      expect(result.markdown, contains('用户内容'));
    },
  );
  test(
    'all existing native Office notices have explicit compatibility mappings',
    () {
      final notices = {
        'Office 嵌入对象已保留为原始附件；图表、公式和版式可用原软件继续编辑。': en.importsOfficeEmbeddedKept,
        '有一个 Office 对象超过限制或无法导出，请在原软件保存后导入。': en.importsOfficeExportFailed,
        'Office 剪贴板暂不可用。': en.importsOfficeUnavailable,
        '剪贴板已变化，请重新粘贴。': en.importsClipboardChanged,
        '读取期间剪贴板发生变化，请重新粘贴。': en.importsClipboardChanged,
        '剪贴板正被其他应用占用，Office 对象未读取。': en.importsOfficeBusy,
        'Office 内容读取失败，其他剪贴板内容仍可使用。': en.importsOfficeReadFailed,
      };
      for (final entry in notices.entries) {
        expect(localizeImportNotice(entry.key, en), entry.value);
      }
      expect(localizeImportNotice('用户自己的中文说明', en), '用户自己的中文说明');
    },
  );
}
