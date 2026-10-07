import 'dart:convert';
import 'dart:io';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/content/rich_content.dart';

void main() {
  test('fresh actual Flutter rich-content reference outputs', () async {
    const word = '<h2>研究</h2><p><b>重点</b> 与 <i>观察</i></p><ul><li>一步</li></ul><script>alert(1)</script><a href="javascript:alert(1)">危险链接</a>';
    const merged = '<table><tr><td rowspan="2">跨行</td><td>A</td></tr><tr><td>B</td></tr></table>';
    const xml = '<Workbook xmlns:ss="urn:schemas-microsoft-com:office:spreadsheet"><Worksheet><Table><Row><Cell><Data>名称</Data></Cell><Cell><Data>结果</Data></Cell></Row><Row><Cell><Data>合计</Data></Cell><Cell ss:Formula="=SUM(R[-1]C:R[-1]C)"><Data>42</Data></Cell></Row></Table></Worksheet></Workbook>';
    const rtf = r'{\rtf1\ansi\uc1 {\fonttbl secret}\u20013?\u25991?\par next{\*\objdata payload}}';
    const tsv = 'A\tB\n1\t2';
    final values = [
      {'format':'html','source':word,'paste_text':htmlToMarkdown(word).markdown},
      {'format':'html','source':merged,'paste_text':htmlToMarkdown(merged).markdown},
      {'format':'xml','source':xml,'paste_text':spreadsheetToMarkdown(xml).markdown},
      {'format':'rtf','source':rtf,'paste_text':rtfToPlainText(rtf)},
      {'format':'plain','source':tsv,'paste_text':plainTextToMarkdown(tsv)},
    ];
    expect(values.length, 5);
    final destination = Platform.environment['HMOS_CLIPBOARD_FLUTTER_REFERENCE'];
    if (destination == null || destination.isEmpty) {
      throw StateError('HMOS_CLIPBOARD_FLUTTER_REFERENCE must name a test output JSON');
    }
    await File(destination).writeAsString(const JsonEncoder.withIndent('  ').convert(values));
  });
}
