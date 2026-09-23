import 'dart:io';
import 'dart:ui' as ui;
import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/main.dart';
import 'package:morrow_studio/storage.dart';
void main() {
 testWidgets('Capture actual Flutter reference for HMOS', (tester) async {
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
    await File('C:/Users/Administrator/Desktop/CodeXProjext/morrow/hmos/reports/ui-source/v3/flutter-$name.png').writeAsBytes(bytes!.buffer.asUint8List());
    image.dispose();
   });
  }
  await shot('mobile');
  await tester.tap(find.text('新建灵感'));
  await tester.pumpAndSettle();
  await shot('editor');
  await tester.tap(find.text('再想想').last);
  await tester.pumpAndSettle();
  tester.view.physicalSize = const Size(1280,900);
  await tester.pumpAndSettle();
  await shot('desktop');
  for (final entry in {'灵感收件箱':'inbox','小项目':'projects','实验室':'lab','已收藏':'favorites'}.entries) {
    await tester.tap(find.text(entry.key).first);
    await tester.pumpAndSettle();
    tester.view.physicalSize = const Size(440,676);
    await tester.pumpAndSettle();
    await shot(entry.value);
    tester.view.physicalSize = const Size(1280,900);
    await tester.pumpAndSettle();
  }
  await tester.tap(find.byKey(const ValueKey('settings-expand')));
  await tester.pumpAndSettle();
  tester.view.physicalSize = const Size(440,676);
  await tester.pumpAndSettle();
  await shot('settings');
  await tester.tap(find.byKey(const ValueKey('font-settings')));
  await tester.pumpAndSettle();
  await shot('font');
  expect(tester.takeException(),isNull);
  await tester.pumpWidget(const SizedBox());
  tester.view.resetPhysicalSize();
  tester.view.resetDevicePixelRatio();
 });
}

