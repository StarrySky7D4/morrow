import 'dart:io';
import 'package:morrow_studio/attachments/attachment.dart';
import 'package:morrow_studio/media/texture_repository.dart';
import 'package:file_selector/file_selector.dart';
import 'package:morrow_studio/desktop_frame.dart';
import 'package:morrow_studio/main.dart';
import 'package:morrow_studio/storage.dart';
import 'package:morrow_studio/window_effects.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:window_manager/window_manager.dart';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  testWidgets('Native attachment import preserves original bytes and names', (
    tester,
  ) async {
    final folder = await Directory.systemTemp.createTemp(
      'morrow-attachment-test-',
    );
    try {
      for (final name in ['设计.dwg', '模型.blend', '截图.png', '片段.mp4', '声音.wav']) {
        final original = File('${folder.path}/$name');
        final bytes = List.generate(256, (i) => i);
        await original.writeAsBytes(bytes);
        final attachment = await IdeaAttachment.import(XFile(original.path));
        try {
          expect(attachment.source.name, name);
          expect(attachment.size, bytes.length);
          expect(attachment.source.location, isNot(original.path));
          expect(await File(attachment.source.location).readAsBytes(), bytes);
          final restored = IdeaAttachment.fromJson(attachment.toJson());
          expect(await File(restored.source.location).readAsBytes(), bytes);
          expect(await original.readAsBytes(), bytes);
        } finally {
          await TextureRepository.remove(attachment.source);
        }
        expect(await File(attachment.source.location).exists(), false);
      }
    } finally {
      await folder.delete(recursive: true);
    }
  });
  testWidgets(
    'All four native corners clip, including after resize and maximize',
    (tester) async {
      await initializeDesktopFrame();
      await tester.pumpWidget(
        MorrowApp(
          storage: MemoryStorage(),
          nativeBackground: DesktopBackground(),
        ),
      );
      await tester.pumpAndSettle();
      const channel = MethodChannel('morrow/window_shape');
      for (final radius in [32.0, 0.0, 20.0]) {
        final slider = tester.widget<Slider>(
          find.byKey(const ValueKey('window-radius')),
        );
        slider.onChanged!(radius);
        slider.onChangeEnd!(radius);
        await tester.pumpAndSettle();
        final geometry = await channel.invokeMapMethod<String, dynamic>(
          'inspectRegion',
        );
        expect(geometry!['cornersVisible'], List.filled(4, radius == 0));
        expect(geometry['centerVisible'], true);
      }
      await windowManager.setSize(const Size(1120, 760));
      await tester.pumpAndSettle();
      expect(
        (await channel.invokeMapMethod<String, dynamic>(
          'inspectRegion',
        ))!['cornersVisible'],
        [false, false, false, false],
      );
      await windowManager.maximize();
      await tester.pumpAndSettle();
      expect(
        (await channel.invokeMapMethod<String, dynamic>(
          'inspectRegion',
        ))!['cornersVisible'],
        [true, true, true, true],
      );
      await windowManager.unmaximize();
      await tester.pumpAndSettle();
      expect(
        (await channel.invokeMapMethod<String, dynamic>(
          'inspectRegion',
        ))!['cornersVisible'],
        [false, false, false, false],
      );
    },
  );
}
