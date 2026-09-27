import 'dart:convert';
import 'dart:io';
import 'dart:ui' as ui;
import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/main.dart';
import 'package:morrow_studio/storage.dart';
void main() {
 testWidgets('Capture Flutter responsive workspace for HMOS', (tester) async {
  await tester.runAsync(() async {
   for (final f in {'MaterialIcons':'C:/flutter/bin/cache/artifacts/material_fonts/materialicons-regular.otf','Segoe UI':'C:/Windows/Fonts/segoeui.ttf','Microsoft YaHei':'C:/Windows/Fonts/msyh.ttc'}.entries) {
    await (FontLoader(f.key)..addFont(File(f.value).readAsBytes().then(ByteData.sublistView))).load();
   }
  });
  tester.view.physicalSize = const Size(440, 676);
  tester.view.devicePixelRatio = 1;
  final key = GlobalKey();
  await tester.pumpWidget(RepaintBoundary(key:key, child:MorrowApp(storage:MemoryStorage(), initialLocale:const Locale('zh'))));
  await tester.pumpAndSettle();
  Future<void> shot(String name) async {
   await tester.runAsync(() async {
    final image = await (key.currentContext!.findRenderObject()! as RenderRepaintBoundary).toImage(pixelRatio:3);
    final bytes = await image.toByteData(format:ui.ImageByteFormat.png);
    await File('C:/Users/Administrator/Desktop/CodeXProjext/morrow/hmos/reports/ui-source/v6/flutter-$name.png').writeAsBytes(bytes!.buffer.asUint8List());
    image.dispose();
   });
  }

  final metrics = <String, Map<String, double>>{};
  for (final view in {'phone':const Size(440,676),'tablet':const Size(880,1408),'wide':const Size(1488,832)}.entries) {
    tester.view.physicalSize = view.value;
    await tester.pumpAndSettle();
    final greeting=find.text('让想法，自由生长。');
    expect(greeting,findsOneWidget);
    final rect=tester.getRect(greeting);
    metrics[view.key]={'width':view.value.width,'height':view.value.height,'greetingX':rect.left,'greetingY':rect.top};
    expect(rect.top,lessThan(150),reason:'Reference workspace starts below its header');
    await shot(view.key);
    expect(tester.takeException(),isNull);
  }
  await tester.runAsync(() async {
    await File('C:/Users/Administrator/Desktop/CodeXProjext/morrow/hmos/reports/ui-source/v6/flutter-layout.json').writeAsString(const JsonEncoder.withIndent('  ').convert(metrics));
  });
  await tester.pumpWidget(const SizedBox());
  tester.view.resetPhysicalSize(); tester.view.resetDevicePixelRatio();
 });
}
