import 'dart:convert';
import 'dart:io';
import 'package:characters/characters.dart';
import 'package:crypto/crypto.dart';
import 'package:flutter_test/flutter_test.dart';
import 'file:///C:/Users/Administrator/AppData/Local/Pub/Cache/hosted/pub.dev/characters-1.4.1/test/src/unicode_grapheme_tests.dart' as corpus;

// Fresh execution against the actual installed Flutter Characters package.
// Boundaries store a generator and complete identities, not huge source blobs.
bool scalarValid(String text) {
  for (var i = 0; i < text.length; i++) {
    final code = text.codeUnitAt(i);
    if (code >= 0xd800 && code <= 0xdbff) {
      if (++i >= text.length) return false;
      final low = text.codeUnitAt(i);
      if (low < 0xdc00 || low > 0xdfff) return false;
    } else if (code >= 0xdc00 && code <= 0xdfff) { return false; }
  }
  return true;
}
void main() {
  test('actual Characters Unicode16 corpus and all five field boundaries', () {
    final records = <Map<String, Object>>[];
    const limits = {'title':60,'todos':1000,'hypothesis':5000,'conclusion':10000,'description':20000};
    void capture(String field, String sample, int repeats, String origin) {
      final text = sample * repeats;
      expect(scalarValid(text), isTrue);
      final bytes = utf8.encode(text), count = text.characters.length;
      records.add({'field':field,'codepoints':sample.runes.toList(),'repeats':repeats,'origin':origin,
        'grapheme_count':count,'utf16_length':text.length,'utf8_length':bytes.length,
        'sha256':sha256.convert(bytes).toString(),'ok':count <= limits[field]!});
    }
    for (final entry in limits.entries) {
      for (final sample in ['a','中','😀','👨‍👩‍👧‍👦','🇨🇳','e\u0301','\u0915\u094d\u0937']) {
        expect(sample.characters.length,1);
        for (final extra in [-1,0,1]) { capture(entry.key,sample,entry.value+extra,'field-boundary'); }
      }
    }
    var corpusCount = 0;
    for (final split in corpus.splitTests) {
      final sample = split.join();
      expect(scalarValid(sample),isTrue);
      expect(sample.characters.toList(),split);
      capture('description',sample,1,'actual-characters-split-corpus');
      corpusCount++;
    }
    final malformed = [String.fromCharCode(0xd800),String.fromCharCode(0xdc00),
      String.fromCharCodes([0xd800,0xd800]),String.fromCharCodes([0xd800,0x61]),String.fromCharCodes([0x61,0xdc00])];
    final path = Platform.environment['HMOS_EDITOR_FIELD_FLUTTER_REFERENCE']!;
    File(path).writeAsStringSync('${jsonEncode(records)}\n');
    File('$path.meta.json').writeAsStringSync('${jsonEncode({'characters_version':'1.4.1',
      'unicode_version':'16.0.0','field_boundary_records':105,'actual_split_corpus_records':corpusCount,
      'records':records.length,'malformed_utf16_explicit_strict_boundary':malformed.map((text)=>
        {'utf16_units':text.codeUnits,'flutter_characters':text.characters.length,'scalar_valid':scalarValid(text)}).toList()})}\n');
    expect(records.length,greaterThan(1000));
    print('Actual Flutter Characters capture: ${records.length} complete identities; $corpusCount corpus / 105 field boundaries.');
  });
}
