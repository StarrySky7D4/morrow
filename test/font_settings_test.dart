import 'dart:io';
import 'dart:typed_data';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/main.dart';
import 'package:morrow_studio/storage.dart';
import 'package:morrow_studio/fonts/font_choice.dart';
import 'package:morrow_studio/fonts/font_repository.dart';
import 'package:morrow_studio/fonts/font_storage_native.dart' as native_fonts;
import 'package:morrow_studio/fonts/font_settings.dart';

void main() {
  test(
    'migration library fonts load locally with bounded size and safe IDs',
    () async {
      final directory = await Directory.systemTemp.createTemp(
        'morrow-migrated-font-',
      );
      addTearDown(() => directory.delete(recursive: true));
      final fonts = await Directory('${directory.path}/fonts').create();
      final id = 'a' * 64;
      final file = File('${fonts.path}/$id.sfnt');
      await file.writeAsBytes([1, 2, 3, 4]);
      expect(await native_fonts.read(id, 4, libraryDirectory: directory.path), [
        1,
        2,
        3,
        4,
      ]);
      await expectLater(
        native_fonts.read(id, 3, libraryDirectory: directory.path),
        throwsFormatException,
      );
      await expectLater(
        native_fonts.read('../outside', 100, libraryDirectory: directory.path),
        throwsFormatException,
      );
      expect((await directory.list().toList()).length, 1);
    },
  );
  test('font descriptors and SFNT table boundaries reject malformed input', () {
    expect(
      () => const FontChoice(asset: '../outside', name: 'x').validate(),
      throwsFormatException,
    );
    expect(
      () => const FontChoice(family: 'bad\nname').validate(),
      throwsFormatException,
    );
    expect(FontChoice.fromJson(null), const FontChoice());
    expect(
      FontChoice.fromJson(const FontChoice(family: 'Arial').toJson()).family,
      'Arial',
    );
    expect(
      () => FontRepository.validateBytes(Uint8List(12)),
      throwsFormatException,
    );
    final bytes = Uint8List(28);
    final data = ByteData.sublistView(bytes)
      ..setUint32(0, 0x00010000)
      ..setUint16(4, 1)
      ..setUint32(20, 28)
      ..setUint32(24, 1);
    expect(() => FontRepository.validateBytes(bytes), throwsFormatException);
    data.setUint32(24, 0);
    FontRepository.validateBytes(bytes);
    data.setUint16(4, 257);
    expect(() => FontRepository.validateBytes(bytes), throwsFormatException);
  });
  testWidgets(
    'font settings persist, restore and reset without replacing workspace',
    (t) async {
      t.view.devicePixelRatio = 1;
      t.view.physicalSize = const Size(1440, 1100);
      addTearDown(t.view.reset);
      final storage = MemoryStorage();
      await t.pumpWidget(
        MorrowApp(storage: storage, initialLocale: const Locale('en')),
      );
      await t.pumpAndSettle();
      final studio = t.state(find.byType(Studio));
      Future<void> open() async {
        await t.ensureVisible(find.byKey(const ValueKey('font-settings')));
        await t.tap(find.byKey(const ValueKey('font-settings')));
        await t.pumpAndSettle();
      }

      await open();
      await t.enterText(find.byKey(const ValueKey('font-family')), 'Arial');
      await t.tap(find.byKey(const ValueKey('font-apply')));
      await t.pumpAndSettle();
      expect(storage.data!['uiFont']['family'], 'Arial');
      expect(
        Theme.of(
          t.element(find.byType(FontSettingsPage)),
        ).textTheme.bodyMedium!.fontFamily,
        'Arial',
      );
      expect(t.state(find.byType(Studio, skipOffstage: false)), same(studio));
      await t.pageBack();
      await t.pumpAndSettle();
      await t.pumpWidget(const SizedBox());
      await t.pumpWidget(
        MorrowApp(storage: storage, initialLocale: const Locale('en')),
      );
      await t.pumpAndSettle();
      expect(
        Theme.of(
          t.element(find.byType(Studio)),
        ).textTheme.bodyMedium!.fontFamily,
        'Arial',
      );
      await open();
      await t.tap(find.byKey(const ValueKey('font-reset')));
      await t.pumpAndSettle();
      expect(storage.data!['uiFont']['family'], '');
      expect(t.takeException(), isNull);
    },
  );
  testWidgets('font changes preserve an open editor, IME and selection', (
    t,
  ) async {
    t.view.devicePixelRatio = 1;
    t.view.physicalSize = const Size(1440, 1100);
    addTearDown(t.view.reset);
    await t.pumpWidget(
      MorrowApp(storage: MemoryStorage(), initialLocale: const Locale('en')),
    );
    await t.pumpAndSettle();
    final scope = FontScope.of(t.element(find.byType(Studio)));
    await t.tap(find.text('New idea').first);
    await t.pumpAndSettle();
    final finder = find.byKey(const ValueKey('idea-title'));
    final controller = t.widget<TextField>(finder).controller!;
    const draft = TextEditingValue(
      text: 'draft 拼音',
      selection: TextSelection(baseOffset: 2, extentOffset: 4),
      composing: TextRange(start: 6, end: 8),
    );
    controller.value = draft;
    await scope.onChanged(const FontChoice(family: 'Arial'));
    await t.pumpAndSettle();
    expect(t.widget<TextField>(finder).controller, same(controller));
    expect(controller.value, draft);
    expect(t.takeException(), isNull);
  });
}
