import 'dart:convert';
import 'dart:io';
import 'package:characters/characters.dart';
import 'package:crypto/crypto.dart';
void main() {
  final samples=<String,String>{
    'empty':'','ascii':' beta \nalpha\nbeta\n\n',
    'crlf':'a\r\nb\r\na\r\n', 'interior-cr':'one\rtwo',
    'bom':'\uFEFF alpha \uFEFF\nalpha\n\uFEFF',
    'normalization':'e\u0301\né\ne\u0301',
    'interior-line-separator':'one\u2028two',
    'family-flags':' 👨‍👩‍👧‍👦 \n🇨🇳\n👨‍👩‍👧‍👦',
    'indic':'क्‍ष\nक्ष\nक्‍ष', 'nul':'a\u0000b\na\u0000b',
    'rows100':List.filled(100,'a').join('\n'),
    'rows101':List.filled(101,'a').join('\n'),
    'blank100':List.filled(100,'').join('\n'),
    'blank101':List.filled(101,'').join('\n'),
    'cluster1000':List.filled(1000,'e\u0301').join(),
    'cluster1001':List.filled(1001,'e\u0301').join(),
    'astral1000':List.filled(1000,'😀').join(),
    'astral1001':List.filled(1001,'😀').join(),
  };
  final whitespace=[...List.generate(5,(i)=>9+i),32,133,160,5760,...List.generate(11,(i)=>8192+i),8232,8233,8239,8287,12288,65279];
  final nonWhitespace=[0,8,14,6158,8203,8288];
  for(final point in [...whitespace,...nonWhitespace]) {
    final w=String.fromCharCode(point);
    samples['trim-'+point.toRadixString(16)]='$w label $w\n$w\nlabel';
  }
  final records=[for(final entry in samples.entries) (() {
    final raw=entry.value;
    final normalized=raw.split('\n').map((line)=>line.trim()).where((line)=>line.isNotEmpty).toSet().toList();
    final count=raw.characters.length;
    final rows=raw.isEmpty?0:raw.split('\n').length;
    return {'id':entry.key,'raw':raw,'normalized':normalized,'graphemes':count,'raw_rows':rows,
      'utf16_units':raw.length,'utf8_bytes':utf8.encode(raw).length,'sha256':sha256.convert(utf8.encode(raw)).toString(),
      'field_and_rows_accepted':count<=1000&&rows<=100};
  })()];
  stdout.write(jsonEncode({'dart_version':Platform.version,'characters_version':'1.4.1','unicode_version':'16.0.0','records':records}));
}
