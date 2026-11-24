import 'dart:io';
import 'package:flutter/services.dart';
import 'package:material_ui/material_ui.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/main.dart';
import 'package:morrow_studio/storage.dart';
import 'package:morrow_studio/fonts/font_choice.dart';
import 'package:morrow_studio/fonts/font_repository.dart';
import 'package:morrow_studio/fonts/font_storage_native.dart' as native_fonts;
import 'package:morrow_studio/fonts/font_settings.dart';
import 'package:morrow_studio/fonts/system_fonts.dart';
import 'package:morrow_studio/settings_surface.dart';

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
        final tab = find.byKey(const ValueKey('inspector-tab-appearance'));
        if (tab.evaluate().isNotEmpty) {
          await t.tap(tab);
          await t.pumpAndSettle();
        }
        await t.ensureVisible(find.byKey(const ValueKey('font-settings')));
        await t.tap(find.byKey(const ValueKey('font-settings')));
        await t.pumpAndSettle();
      }

      await open();
      expect(
        find.ancestor(
          of: find.byType(FontSettingsPage),
          matching: find.byType(CanvasNavigation),
        ),
        findsOneWidget,
      );
      expect(find.byType(Studio), findsOneWidget);
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
  test(
    'system font catalog normalizes, deduplicates, filters invalid names, and refreshes cache',
    () async {
      TestWidgetsFlutterBinding.ensureInitialized();
      SystemFontCatalog.clearCache();
      var calls = 0;
      final messenger =
          TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger;
      messenger.setMockMethodCallHandler(SystemFontCatalog.channel, (
        call,
      ) async {
        calls++;
        expect(call.method, 'listSystemFonts');
        return [
          {'family': 'Microsoft YaHei', 'displayName': '微软雅黑'},
          {'family': 'microsoft yahei', 'displayName': 'Duplicate'},
          {'family': 'bad\nfont', 'displayName': 'Bad'},
          {'family': 'Consolas', 'displayName': 'Consolas'},
        ];
      });
      addTearDown(() {
        messenger.setMockMethodCallHandler(SystemFontCatalog.channel, null);
        SystemFontCatalog.clearCache();
      });

      final first = await SystemFontCatalog.load(locale: 'zh-CN');
      expect(calls, 1);
      expect(first.length, 2);
      expect(
        first,
        containsAll([
          const SystemFontEntry(family: 'Microsoft YaHei', displayName: '微软雅黑'),
          const SystemFontEntry(family: 'Consolas', displayName: 'Consolas'),
        ]),
      );

      final cached = await SystemFontCatalog.load(locale: 'zh-CN');
      expect(calls, 1);
      expect(cached, first);

      await SystemFontCatalog.load(locale: 'zh-CN', refresh: true);
      expect(calls, 2);
    },
  );
  testWidgets(
    'system font picker filters, applies selected font, and restores default',
    (t) async {
      SystemFontCatalog.clearCache();
      final messenger =
          TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger;
      messenger.setMockMethodCallHandler(SystemFontCatalog.channel, (
        call,
      ) async {
        return [
          {'family': 'Microsoft YaHei', 'displayName': '微软雅黑'},
          {'family': 'Consolas', 'displayName': 'Consolas'},
          {'family': 'Georgia', 'displayName': 'Georgia'},
        ];
      });
      addTearDown(() {
        messenger.setMockMethodCallHandler(SystemFontCatalog.channel, null);
        SystemFontCatalog.clearCache();
      });

      t.view.devicePixelRatio = 1;
      t.view.physicalSize = const Size(1440, 1100);
      addTearDown(t.view.reset);

      final storage = MemoryStorage();
      await t.pumpWidget(
        MorrowApp(storage: storage, initialLocale: const Locale('zh')),
      );
      await t.pumpAndSettle();

      final tab = find.byKey(const ValueKey('inspector-tab-appearance'));
      if (tab.evaluate().isNotEmpty) {
        await t.tap(tab);
        await t.pumpAndSettle();
      }
      await t.ensureVisible(find.byKey(const ValueKey('font-settings')));
      await t.tap(find.byKey(const ValueKey('font-settings')));
      await t.pumpAndSettle();

      expect(find.byKey(const ValueKey('system-font-picker')), findsOneWidget);
      expect(find.byKey(const ValueKey('system-font-list')), findsOneWidget);

      await t.enterText(
        find.byKey(const ValueKey('system-font-search')),
        '微软雅黑',
      );
      await t.pumpAndSettle();

      expect(
        find.byKey(const ValueKey('system-font-Microsoft YaHei')),
        findsOneWidget,
      );
      expect(find.byKey(const ValueKey('system-font-Consolas')), findsNothing);

      await t.tap(find.byKey(const ValueKey('system-font-Microsoft YaHei')));
      await t.pumpAndSettle();

      expect(storage.data!['uiFont']['family'], 'Microsoft YaHei');
      expect(
        Theme.of(
          t.element(find.byType(FontSettingsPage)),
        ).textTheme.bodyMedium!.fontFamily,
        'Microsoft YaHei',
      );
      expect(
        t.widget<TextField>(find.byKey(const ValueKey('font-family'))).controller!
            .text,
        'Microsoft YaHei',
      );

      await t.enterText(find.byKey(const ValueKey('system-font-search')), '');
      await t.pumpAndSettle();

      await t.tap(find.byKey(const ValueKey('system-font-default')));
      await t.pumpAndSettle();

      expect(storage.data!['uiFont']['family'], '');
      expect(t.takeException(), isNull);
    },
  );
}
