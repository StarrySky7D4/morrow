import 'dart:io';
import 'dart:ui' as ui;
import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/main.dart';
import 'package:morrow_studio/storage.dart';

// Explicit local visual audit, excluded from the normal test/ suite.
void main() {
  testWidgets('Render new capture and liquid glass for visual review', (
    tester,
  ) async {
    await tester.runAsync(() async {
      for (final font in {
        'MaterialIcons':
            'C:/flutter/bin/cache/artifacts/material_fonts/materialicons-regular.otf',
        'Segoe UI': 'C:/Windows/Fonts/segoeui.ttf',
        'Microsoft YaHei': 'C:/Windows/Fonts/msyh.ttc',
      }.entries) {
        final loader = FontLoader(font.key)
          ..addFont(
            File(font.value).readAsBytes().then((b) => ByteData.sublistView(b)),
          );
        await loader.load();
      }
    });
    tester.view.physicalSize = const Size(1440, 1000);
    tester.view.devicePixelRatio = 1;
    final boundaryKey = GlobalKey();
    final storage = MemoryStorage();
    await tester.pumpWidget(
      RepaintBoundary(
        key: boundaryKey,
        child: MorrowApp(storage: storage),
      ),
    );
    await tester.pumpAndSettle();
    Future<void> capture(String name) async {
      final boundary =
          boundaryKey.currentContext!.findRenderObject()!
              as RenderRepaintBoundary;
      await tester.runAsync(() async {
        final image = await boundary.toImage();
        final bytes = await image.toByteData(format: ui.ImageByteFormat.png);
        final file = File('build/feature-review/$name.png');
        await file.parent.create(recursive: true);
        await file.writeAsBytes(bytes!.buffer.asUint8List());
        image.dispose();
      });
    }

    await tester.tap(find.byKey(const ValueKey('mode-liquid')));
    await tester.pumpAndSettle();
    await capture('liquid-desktop');
    await tester.tap(find.text('新建灵感'));
    await tester.pumpAndSettle();
    await tester.enterText(
      find.byKey(const ValueKey('idea-title')),
      '把周末，留给一点新想法',
    );
    await tester.enterText(
      find.byKey(const ValueKey('idea-description')),
      '# 一个安静的数字花园\n\n收集 **值得留下的片刻**，也给尚未成形的念头留一点空间。\n\n- 记录一束光\n- 做一次小实验\n\n| 灵感 | 下一小步 |\n| --- | --- |\n| 桌面小助手 | 画出第一张草图 |\n| 周末散步 | 带上相机 |',
    );
    await tester.tap(find.byKey(const ValueKey('idea-preview-toggle')));
    await tester.pumpAndSettle();
    expect(tester.takeException(), isNull);
    await capture('new-idea-desktop');
    tester.view.physicalSize = const Size(390, 844);
    await tester.pumpAndSettle();
    expect(tester.takeException(), isNull);
    await capture('new-idea-mobile');
    await tester.pumpWidget(const SizedBox());
    tester.view.resetPhysicalSize();
    tester.view.resetDevicePixelRatio();
  });
}
