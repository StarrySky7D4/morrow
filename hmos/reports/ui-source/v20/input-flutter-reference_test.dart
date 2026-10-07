import 'dart:convert';
import 'dart:io';
import 'package:characters/characters.dart';
import 'package:crypto/crypto.dart';
import 'package:flutter/material.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

const limits = {'title':60,'todos':1000,'hypothesis':5000,'conclusion':10000,'description':20000};
const budget = 512 * 1024;
String hash(String text) => sha256.convert(utf8.encode(text)).toString();
Map<String,Object> spec(String cluster,int repeats,[String suffix='']) => {
  'codepoints':cluster.runes.toList(),'repeats':repeats,'suffix_codepoints':suffix.runes.toList()};
String source(Map<String,Object> s) => String.fromCharCodes(s['codepoints'] as List<int>) * (s['repeats'] as int)
  + String.fromCharCodes(s['suffix_codepoints'] as List<int>);
TextEditingValue edit(String text,{int? base,int? extent,int affinity=1,bool directional=false,int cs=-1,int ce=-1}) =>
  TextEditingValue(text:text,selection:TextSelection(baseOffset:base??text.length,extentOffset:extent??text.length,
    affinity:TextAffinity.values[affinity],isDirectional:directional),composing:TextRange(start:cs,end:ce));
Map<String,Object> flat(TextEditingValue v) => {'text':v.text,'selection_base':v.selection.baseOffset,
  'selection_extent':v.selection.extentOffset,'affinity':v.selection.affinity.index,
  'directional':v.selection.isDirectional,'composing_start':v.composing.start,'composing_end':v.composing.end};

void main() {
  test('actual installed Flutter complete Windows formatter values and todo row chain',() {
    expect(LengthLimitingTextInputFormatter.getDefaultMaxLengthEnforcement(TargetPlatform.windows),MaxLengthEnforcement.enforced);
    final records=<Map<String,Object>>[];
    var bounded=0, wireDifferences=0;
    void capture(String name,String field,Map<String,Object> os,Map<String,Object> ns,
      TextEditingValue old,TextEditingValue next,{String mode='field',int? rowLimit}) {
      expect(old.text,source(os)); expect(next.text,source(ns));
      final limit=rowLimit??limits[field]!;
      final applied=old.text!=next.text || (!old.composing.isCollapsed && next.composing.isCollapsed);
      var result=next,action='accepted';
      if(applied) {
        if(mode=='todo_row') {
          if(limit==0) {
            final guard=TextInputFormatter.withFunction((previous,value)=>
              value.text.characters.length<=previous.text.characters.length?value:previous);
            result=guard.formatEditUpdate(old,result);
            if(identical(result,old)) action='retained';
          }
          result=FilteringTextInputFormatter.deny(RegExp(r'[\r\n]'),replacementString:' ').formatEditUpdate(old,result);
        }
        if(limit>0) {
          final before=result;
          result=LengthLimitingTextInputFormatter(limit,maxLengthEnforcement:MaxLengthEnforcement.enforced)
            .formatEditUpdate(old,result);
          if(identical(result,old)) { action='retained'; }
          else if(result.text!=before.text) { action='truncated'; }
        }
      }
      final request={'field':field,'mode':mode,if(mode=='todo_row')'limit':limit,'old_value':flat(old),'new_value':flat(next)};
      final requestText=jsonEncode(request),requestBytes=utf8.encode(requestText).length;
      final wire={'ok':true,'error':'','field':field,'mode':mode,'limit':limit,'action':action,'format_applied':applied,
        'old_value':flat(old),'new_value':flat(next),'value':flat(result),
        'old_grapheme_count':old.text.characters.length,'old_utf16_length':old.text.length,'old_utf8_length':utf8.encode(old.text).length,
        'new_grapheme_count':next.text.characters.length,'new_utf16_length':next.text.length,'new_utf8_length':utf8.encode(next.text).length,
        'grapheme_count':result.text.characters.length,'utf16_length':result.text.length,'utf8_length':utf8.encode(result.text).length,
        'unicode_version':'16.0.0','request_sha256':hash(requestText)};
      final error=requestBytes>budget?'EditorInputRequestBytesLimit':
        utf8.encode(jsonEncode(wire)).length>budget?'EditorInputReplyBytesLimit':'';
      if(error.isEmpty) { bounded++; } else { wireDifferences++; }
      // Store lossless generators plus the actual complete prefix's hash, so
      // the committed capture does not duplicate megabytes of repeated text.
      final candidates={'new':next.text,'old':old.text,'row_new':next.text.replaceAll(RegExp(r'[\r\n]'),' '),
        'row_old':old.text.replaceAll(RegExp(r'[\r\n]'),' ')};
      final origin=candidates.entries.firstWhere((e)=>e.value.startsWith(result.text)).key;
      final oldFlat=flat(old)..remove('text'),newFlat=flat(next)..remove('text'),valueFlat=flat(result)..remove('text');
      records.add({'name':name,'field':field,'mode':mode,'limit':limit,
        'old_value':{...oldFlat,'text_spec':os},'new_value':{...newFlat,'text_spec':ns},'value':valueFlat,
        'old_text_sha256':hash(old.text),'new_text_sha256':hash(next.text),'value_text_sha256':hash(result.text),
        'result_from':origin,'result_prefix_utf16':result.text.length,'action':action,'format_applied':applied,
        'old_grapheme_count':wire['old_grapheme_count']!,'old_utf16_length':wire['old_utf16_length']!,'old_utf8_length':wire['old_utf8_length']!,
        'new_grapheme_count':wire['new_grapheme_count']!,'new_utf16_length':wire['new_utf16_length']!,'new_utf8_length':wire['new_utf8_length']!,
        'grapheme_count':result.text.characters.length,'utf16_length':result.text.length,'utf8_length':utf8.encode(result.text).length,
        'expected_native_error':error,'actual_request_utf8_bytes':requestBytes,'actual_reply_utf8_bytes':utf8.encode(jsonEncode(wire)).length});
    }
    for(final entry in limits.entries) {
      final n=entry.value,field=entry.key;
      for(final cluster in ['a','中','😀','👨‍👩‍👧‍👦','🇨🇳','e\u0301','\u0915\u094d\u0937']) {
        final empty=spec('',0),exact=spec(cluster,n),over=spec(cluster,n+1),under=spec(cluster,n-1);
        capture('$field/$cluster/under',field,empty,under,edit(''),edit(source(under)));
        capture('$field/$cluster/exact',field,empty,exact,edit(''),edit(source(exact)));
        capture('$field/$cluster/truncate',field,empty,over,edit(''),edit(source(over),base:source(over).length,extent:1,affinity:0,directional:true,cs:1,ce:source(over).length));
        capture('$field/$cluster/retained-composing',field,exact,over,edit(source(exact),affinity:0,directional:true,cs:0,ce:source(exact).length),edit(source(over),cs:1,ce:source(over).length));
        capture('$field/$cluster/selected-replace',field,exact,over,edit(source(exact),base:source(exact).length,extent:0),edit(source(over),base:1,extent:source(over).length,cs:source(exact).length,ce:source(over).length));
        capture('$field/$cluster/deletion',field,exact,under,edit(source(exact)),edit(source(under),base:source(under).length,extent:0,affinity:0,directional:true));
      }
    }
    for(final cluster in ['a','😀','👨‍👩‍👧‍👦','🇨🇳','e\u0301','\u0915\u094d\u0937']) {
      final s=spec(cluster,65),text=source(s);
      capture('gate/$cluster/selection-only','title',s,s,edit(text,cs:1,ce:3),edit(text,base:3,extent:1,affinity:0,directional:true,cs:2,ce:4));
      capture('gate/$cluster/composition-commit','title',s,s,edit(text,cs:1,ce:3),edit(text,cs:2,ce:2));
      capture('gate/$cluster/collapsed-not-commit','title',s,s,edit(text,cs:1,ce:1),edit(text,cs:-1,ce:-1));
      final empty=spec('',0),over=spec(cluster,61);
      for(final c in [[-1,-1],[-1,1],[1,-1],[60*cluster.length,61*cluster.length],[0,0]]) {
        capture('compose/$cluster/${c.join(',')}','title',empty,over,edit(''),edit(source(over),base:-1,extent:1,affinity:0,directional:true,cs:c[0],ce:c[1]));
      }
    }
    for(final limit in [0,1,2,4,1000]) {
      for(final text in ['a','abc','\r\n','a\r\nb','e\u0301\r\n','😀\n🇨🇳']) {
        final os=spec('ab',1),ns=spec(text,1);
        capture('row/$limit/${text.codeUnits}','todos',os,ns,edit('ab'),edit(text,base:-1,extent:text.length,affinity:0,directional:true,cs:0,ce:0),mode:'todo_row',rowLimit:limit);
      }
      final s=spec('ab',1);
      capture('row/$limit/selection-only','todos',s,s,edit('ab'),edit('ab',base:0,extent:1,cs:1,ce:1),mode:'todo_row',rowLimit:limit);
    }
    capture('row/0/retained-collapsed-composing','todos',spec('ab',1),spec('abc',1),
      edit('ab',cs:1,ce:1),edit('abc'),mode:'todo_row',rowLimit:0);
    capture('row/0/retained-invalid-selection','todos',spec('ab',1),spec('abc',1),
      edit('ab',base:-1,extent:1,affinity:0,directional:true),edit('abc'),mode:'todo_row',rowLimit:0);
    capture('row/0/retained-old-crlf','todos',spec('a\r\nb',1),spec('abcde',1),
      edit('a\r\nb'),edit('abcde'),mode:'todo_row',rowLimit:0);
    final control=spec(' \u0000\ufffd e\u0301 ',1);
    capture('scalar-control-preservation','title',spec('',0),control,edit(''),edit(source(control)));
    final path=Platform.environment['HMOS_EDITOR_INPUT_FLUTTER_REFERENCE']!;
    File(path).writeAsStringSync('${jsonEncode(records)}\n');
    File('$path.meta.json').writeAsStringSync('${jsonEncode({'characters_version':'1.4.1','unicode_version':'16.0.0',
      'target_platform':'windows','enforcement':'enforced','records':records.length,'complete_value_matches_within_wire':bounded,
      'explicit_native_request_reply_budget_differences':wireDifferences,'gate':'EditableText textChanged || textCommitted'})}\n');
    expect(records.length,greaterThanOrEqualTo(200));
    print('Actual Flutter complete editing capture: ${records.length} identities; $bounded within wire / $wireDifferences explicit wire differences.');
  });

  testWidgets('actual EditableText Windows gate preserves raw selection and noncommit composition', (tester) async {
    final original=debugDefaultTargetPlatformOverride;
    debugDefaultTargetPlatformOverride=TargetPlatform.windows;
    try {
      final controller=TextEditingController();
      final widgetRecords=<Map<String,Object>>[];
      await tester.pumpWidget(MaterialApp(home:Scaffold(body:TextField(controller:controller,maxLength:60))));
      await tester.showKeyboard(find.byType(TextField));
      for(final commit in [false,true]) {
        final old=edit('x'*65,cs:1,ce:3);
        controller.value=old; await tester.pump();
        final next=edit('x'*65,base:3,extent:2,affinity:0,directional:true,cs:commit?-1:2,ce:commit?-1:4);
        tester.testTextInput.updateEditingValue(next); await tester.pump();
        final expected=commit?LengthLimitingTextInputFormatter(60,maxLengthEnforcement:MaxLengthEnforcement.enforced).formatEditUpdate(old,next):next;
        expect(flat(controller.value),flat(expected));
        widgetRecords.add({'name':commit?'compose-commit':'noncommit-selection-within-compose',
          'old_value':flat(old),'new_value':flat(next),'value':flat(controller.value),'formatter_result':flat(expected),
          'controller_matches_formatter':true});
      }
      final old=edit('x'*65,cs:1,ce:3);
      controller.value=old; await tester.pump();
      final outside=edit('x'*65,base:3,extent:1,affinity:0,directional:true,cs:2,ce:4);
      tester.testTextInput.updateEditingValue(outside); await tester.pump();
      final postSelection=outside.copyWith(composing:TextRange.empty);
      expect(flat(controller.value),flat(postSelection));
      widgetRecords.add({'name':'selection-outside-compose-controller-postprocessing',
        'old_value':flat(old),'new_value':flat(outside),'value':flat(controller.value),'formatter_result':flat(outside),
        'controller_matches_formatter':false,
        'boundary':'TextEditingController.selection setter clears composing outside selection after formatter stage'});
      final path=Platform.environment['HMOS_EDITOR_INPUT_FLUTTER_REFERENCE']!;
      File('$path.widget.json').writeAsStringSync('${jsonEncode(widgetRecords)}\n');
      await tester.pumpWidget(const SizedBox.shrink());
      controller.dispose();
    } finally { debugDefaultTargetPlatformOverride=original; }
  });
}
