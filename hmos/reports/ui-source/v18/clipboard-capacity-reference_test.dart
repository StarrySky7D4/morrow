import 'dart:convert';
import 'dart:io';
import 'package:crypto/crypto.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/content/rich_content.dart';

// Executes actual Flutter fallback converters. The JSON records complete
// source identities and outcomes without checking multi-megabyte payloads in.
void main() {
  test('actual Flutter UTF16 source capacity boundaries', () async {
    final results = <Map<String, Object>>[];
    final shells = <(String, String, String, int)>[
      ('html', "<p title='", "'>ok</p>", 2 * 1024 * 1024),
      ('xml', "<Table note='", "'><Row><Cell><Data>ok</Data></Cell></Row></Table>", 2 * 1024 * 1024),
      ('rtf', r'{\rtf1\ansi\ansicpg65001{\info ', '}ok}', 8 * 1024 * 1024),
    ];
    for (final (format, prefix, suffix, limit) in shells) {
      for (final extra in [0, 1]) {
        final source = '$prefix${'中' * (limit + extra - prefix.length - suffix.length)}$suffix';
        final bytes = utf8.encode(source);
        var accepted = false, text = '', error = '';
        try {
          text = switch (format) {
            'html' => htmlToMarkdown(source).markdown,
            'xml' => spreadsheetToMarkdown(source).markdown,
            _ => rtfToPlainText(source),
          };
          accepted = true;
        } catch (failure) {
          error = failure.toString();
        }
        expect(accepted, extra == 0);
        if (accepted) expect(text, format == 'xml' ? '| ok |\n| --- |' : 'ok');
        results.add({
          'format': format, 'source_utf16': source.length, 'source_byte_length': bytes.length,
          'source_sha256': sha256.convert(bytes).toString(), 'accepted': accepted,
          'paste_text': text, 'error': error,
        });
      }
    }
    final destination = Platform.environment['HMOS_CLIPBOARD_FLUTTER_CAPACITY_REFERENCE'];
    if (destination == null || destination.isEmpty) {
      throw StateError('HMOS_CLIPBOARD_FLUTTER_CAPACITY_REFERENCE must name a test output JSON');
    }
    await File(destination).writeAsString(const JsonEncoder.withIndent('  ').convert(results));
  });
}
