import 'dart:io';
import 'dart:ui' as ui;
import 'package:flutter/rendering.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_i18n/morrow_i18n.dart';
import 'package:morrow_studio/main.dart';
import 'package:morrow_studio/component_material_page.dart';
import 'package:morrow_studio/little_tips.dart';
import 'package:morrow_studio/storage.dart';
import 'package:morrow_studio/tip_preferences.dart';
import 'package:morrow_studio/music/music_controller.dart';
import 'package:morrow_studio/theme_plugins/theme_plugin_controller.dart';
import 'context_menu_workflow_test.dart' show secondaryClick, chooseMenu;
import 'music_test.dart' show FakeAudio, track;
import 'theme_plugin_fixture.dart';

class FailingTipStore extends MemoryTipStore {
  bool fail = false;
  @override
  Future<void> write(String value) async {
    if (fail) throw StateError('disk unavailable');
    await super.write(value);
  }
}

Finder tipInputs() => find.byWidgetPredicate(
  (widget) =>
      widget is TextField &&
      widget.key is ValueKey<String> &&
      (widget.key as ValueKey<String>).value.startsWith('tip-item-'),
);

Future<void> setTipRows(WidgetTester t, List<String> texts) async {
  while (tipInputs().evaluate().length > texts.length) {
    await click(t, 'tip-remove-${tipInputs().evaluate().length - 1}');
  }
  while (tipInputs().evaluate().length < texts.length) {
    await click(t, 'tip-add');
  }
  for (final (i, text) in texts.indexed) {
    final field = find.byKey(ValueKey('tip-item-$i'));
    await t.ensureVisible(field);
    await t.pumpAndSettle();
    await t.enterText(field, text);
    await t.pumpAndSettle();
  }
}

Future<void> click(WidgetTester t, String key) async {
  final f = find.byKey(ValueKey(key));
  await t.ensureVisible(f);
  await t.pumpAndSettle();
  await t.tap(f);
  await t.pumpAndSettle();
}

void main() {
  test(
    'serialized local tip saves survive reopen, isolate IDs and validate limits',
    () async {
      final store = MemoryTipStore();
      final tips = TipPreferences(store);
      await tips.restore();
      await Future.wait([
        tips.save('footer', ' one \r\n\n two '),
        tips.save('corner-tips', '边栏'),
      ]);
      final restored = TipPreferences(store);
      await restored.restore();
      expect(restored.customLines('footer'), ['one', 'two']);
      expect(restored.customLines('corner-tips'), ['边栏']);
      expect(
        () => tips.save('footer', List.filled(101, 'tip').join('\n')),
        throwsFormatException,
      );
      await restored.save('footer', '   \n');
      expect(restored.lines('footer', ['localized']), ['localized']);
      expect(restored.customLines('corner-tips'), ['边栏']);
      expect(
        LocalTipStore(libraryDirectory: 'A').key,
        isNot(LocalTipStore(libraryDirectory: 'B').key),
      );
      tips.dispose();
      restored.dispose();
    },
  );

  Future<void> mount(
    WidgetTester t,
    TipPreferences tips, {
    ThemePluginController? theme,
    Locale locale = const Locale('en'),
    MemoryStorage? storage,
  }) async {
    t.view.physicalSize = const Size(1440, 1000);
    t.view.devicePixelRatio = 1;
    addTearDown(t.view.reset);
    await t.pumpWidget(
      MorrowApp(
        storage: storage ?? MemoryStorage(),
        tipPreferences: tips,
        themePlugins: theme,
        initialLocale: locale,
      ),
    );
    await t.pumpAndSettle();
  }

  Future<void> openTip(WidgetTester t, String id) async {
    await secondaryClick(
      t,
      id == 'daily'
          ? find.text(L10n.forLocale(const Locale('en')).mainDaily)
          : find.byKey(ValueKey(id == 'footer' ? 'footer-tips' : id)),
    );
    await chooseMenu(
      t,
      L10n.forLocale(const Locale('en')).mainComponentSettings,
    );
    expect(
      t.widget<ComponentMaterialPage>(find.byType(ComponentMaterialPage)).id,
      id,
    );
  }

  testWidgets(
    'daily edits, reordering and duplicate labels keep completion on stable IDs',
    (t) async {
      final store = MemoryTipStore();
      final tips = TipPreferences(store);
      final storage = MemoryStorage();
      final en = L10n.forLocale(const Locale('en'));
      await mount(t, tips, storage: storage);
      await t.ensureVisible(find.text(en.mainDailyWater));
      await t.pumpAndSettle();
      await t.tap(find.text(en.mainDailyWater));
      await t.pumpAndSettle();
      expect(
        storage.data!['completed'],
        contains(TipPreferences.defaultDailyIds.first),
      );
      await openTip(t, 'daily');
      expect(find.byKey(const ValueKey('tip-default-text')), findsNothing);
      await t.enterText(find.byKey(const ValueKey('tip-item-0')), 'Stretch');
      await t.enterText(find.byKey(const ValueKey('tip-item-1')), 'Stretch');
      await click(t, 'tip-down-0');
      expect(
        t
            .widget<TextField>(find.byKey(const ValueKey('tip-item-1')))
            .controller!
            .text,
        'Stretch',
      );
      await click(t, 'tip-add');
      final added = find.byKey(const ValueKey('tip-item-3'));
      expect(added.hitTestable(), findsOneWidget);
      expect(t.widget<TextField>(added).focusNode!.hasFocus, isTrue);
      await t.ensureVisible(added);
      await t.pumpAndSettle();
      await t.enterText(added, 'Look outside');
      await click(t, 'tip-remove-2');
      final caption = find.byKey(const ValueKey('tip-daily-caption'));
      await t.ensureVisible(caption);
      await t.pumpAndSettle();
      await t.enterText(caption, 'Small steps matter');
      await click(t, 'component-apply');
      final defaults = [
        en.mainDailyWater,
        en.mainDailyIdea,
        en.mainDailyExplore,
      ];
      final items = tips.items('daily', defaults);
      expect(items.map((item) => item.text), [
        'Stretch',
        'Stretch',
        'Look outside',
      ]);
      expect(items[1].id, TipPreferences.defaultDailyIds[0]);
      expect(
        t
            .widget<LittleTask>(
              find.byKey(ValueKey('daily-task:${items[1].id}')),
            )
            .done,
        isTrue,
      );
      expect(
        t
            .widget<LittleTask>(
              find.byKey(ValueKey('daily-task:${items[0].id}')),
            )
            .done,
        isFalse,
      );
      expect(find.text('Small steps matter'), findsOneWidget);
      await t.pumpWidget(const SizedBox());
      tips.dispose();
      final restored = TipPreferences(store);
      await restored.restore();
      await mount(t, restored, storage: storage);
      expect(
        restored.items('daily', defaults).map((item) => item.id),
        items.map((item) => item.id),
      );
      expect(find.text('Small steps matter'), findsOneWidget);
      await secondaryClick(t, find.text(en.mainDaily));
      await chooseMenu(t, en.mainTaskMarkComplete);
      expect(
        (storage.data!['completed'] as List).toSet(),
        items.map((item) => item.id).toSet(),
      );
      await openTip(t, 'daily');
      await click(t, 'tip-text-reset');
      await click(t, 'component-apply');
      expect(restored.customLines('daily'), isNull);
      expect(find.text(en.mainSlowProgress), findsOneWidget);
      await t.pumpWidget(const SizedBox());
      restored.dispose();
    },
  );

  testWidgets(
    'editing a daily item leaves its untouched closing note localized',
    (t) async {
      final tips = TipPreferences(MemoryTipStore());
      await mount(t, tips);
      await openTip(t, 'daily');
      await t.enterText(
        find.byKey(const ValueKey('tip-item-0')),
        'Custom daily item',
      );
      await click(t, 'component-apply');
      expect(tips.customLines('daily')!.first, 'Custom daily item');
      expect(tips.customLines('daily-caption'), isNull);
      await t.pumpWidget(const SizedBox());
      await mount(t, tips, locale: const Locale('zh'));
      expect(
        find.text(L10n.forLocale(const Locale('zh')).mainSlowProgress),
        findsOneWidget,
      );
      await t.pumpWidget(const SizedBox());
      tips.dispose();
    },
  );

  testWidgets(
    'card editor fits desktop and narrow layouts with persistent save bar',
    (t) async {
      final boundary = GlobalKey();
      final tips = TipPreferences(MemoryTipStore());
      t.view.devicePixelRatio = 1;
      t.view.physicalSize = const Size(1280, 900);
      addTearDown(t.view.reset);
      await t.pumpWidget(
        RepaintBoundary(
          key: boundary,
          child: MorrowApp(
            storage: MemoryStorage(),
            tipPreferences: tips,
            initialLocale: const Locale('zh'),
          ),
        ),
      );
      await t.pumpAndSettle();
      await click(t, 'component-settings');
      await click(t, 'component-entry:daily');
      for (final size in [const Size(1280, 900), const Size(390, 844)]) {
        t.view.physicalSize = size;
        await t.pumpAndSettle();
        expect(t.takeException(), isNull);
        expect(
          find.byKey(const ValueKey('component-apply')).hitTestable(),
          findsOneWidget,
        );
        final render =
            boundary.currentContext!.findRenderObject()!
                as RenderRepaintBoundary;
        await t.runAsync(() async {
          final image = await render.toImage();
          final bytes = await image.toByteData(format: ui.ImageByteFormat.png);
          final file = File('build/tips-cards-${size.width.toInt()}.png');
          await file.parent.create(recursive: true);
          await file.writeAsBytes(bytes!.buffer.asUint8List());
          image.dispose();
        });
      }
      await t.pumpWidget(const SizedBox());
      tips.dispose();
    },
  );

  testWidgets(
    'sidebar and footer edit separately; cancel, restart and reset localized defaults',
    (t) async {
      final store = MemoryTipStore();
      final tips = TipPreferences(store);
      await tips.restore();
      await mount(t, tips);
      await openTip(t, 'footer');
      final input = find.byKey(const ValueKey('tip-item-0'));
      expect(t.widget<TextField>(input).controller!.text, contains(' '));
      await setTipRows(t, ['First custom tip', 'Second custom tip']);
      await click(t, 'component-apply');
      expect(tips.customLines('footer'), [
        'First custom tip',
        'Second custom tip',
      ]);
      expect(
        t
            .widget<Studio>(find.byType(Studio))
            .palette
            .surfaces
            .components
            .containsKey('footer'),
        isFalse,
        reason: 'Text-only edits must not submit a business material change',
      );
      expect(
        t.widget<RotatingTip>(find.byKey(const ValueKey('footer-tips'))).lines,
        ['First custom tip', 'Second custom tip'],
      );
      await openTip(t, 'corner-tips');
      await setTipRows(t, ['Custom sidebar']);
      await click(t, 'component-apply');
      await openTip(t, 'footer');
      await t.enterText(input, 'Discard this');
      final cancel = find.text(L10n.forLocale(const Locale('en')).visualCancel);
      await t.ensureVisible(cancel);
      await t.pumpAndSettle();
      await t.tap(cancel);
      await t.pumpAndSettle();
      expect(tips.customLines('footer')!.first, 'First custom tip');
      await t.pumpWidget(const SizedBox());
      tips.dispose();
      final reopened = TipPreferences(store);
      await reopened.restore();
      await mount(t, reopened);
      expect(
        t.widget<RotatingTip>(find.byKey(const ValueKey('corner-tips'))).lines,
        ['Custom sidebar'],
      );
      await openTip(t, 'footer');
      await click(t, 'tip-text-reset');
      await click(t, 'component-apply');
      expect(reopened.customLines('footer'), isNull);
      expect(reopened.customLines('corner-tips'), ['Custom sidebar']);
      await t.pumpWidget(const SizedBox());
      await mount(t, reopened, locale: const Locale('zh'));
      expect(
        t.widget<RotatingTip>(find.byKey(const ValueKey('footer-tips'))).lines,
        footerTips,
      );
      expect(t.takeException(), isNull);
      await t.pumpWidget(const SizedBox());
      reopened.dispose();
    },
  );

  testWidgets(
    'failed save stays in editor with draft and confirmed text intact',
    (t) async {
      final store = FailingTipStore();
      final tips = TipPreferences(store);
      await tips.save('footer', 'Confirmed');
      await mount(t, tips);
      await openTip(t, 'footer');
      await t.enterText(find.byKey(const ValueKey('tip-item-0')), 'Draft');
      store.fail = true;
      await click(t, 'component-apply');
      expect(find.byType(ComponentMaterialPage), findsOneWidget);
      expect(tips.customLines('footer'), ['Confirmed']);
      expect(
        t
            .widget<TextField>(find.byKey(const ValueKey('tip-item-0')))
            .controller!
            .text,
        'Draft',
      );
      store.fail = false;
      await click(t, 'component-apply');
      expect(tips.customLines('footer'), ['Draft']);
      await t.pumpWidget(const SizedBox());
      tips.dispose();
    },
  );

  testWidgets(
    'full theme keeps text settings available while hiding material controls',
    (t) async {
      final tips = TipPreferences(MemoryTipStore());
      final theme = ThemePluginController(
        store: MemoryThemePluginStore(),
        backend: ThemeBackend(),
      );
      await theme.restore();
      await theme.activate(themeId);
      await theme.setFullOverride(true);
      await mount(t, tips, theme: theme);
      await click(t, 'component-settings');
      expect(find.byKey(const ValueKey('component-entry:hero')), findsNothing);
      await click(t, 'component-entry:footer');
      expect(
        find.byKey(const ValueKey('component-custom-toggle')),
        findsNothing,
      );
      await setTipRows(t, ['Under theme']);
      await click(t, 'component-apply');
      expect(tips.customLines('footer'), ['Under theme']);
      await t.pumpWidget(const SizedBox());
      tips.dispose();
      theme.dispose();
    },
  );

  testWidgets(
    'custom tips rotate and lyrics take priority, then return to custom text',
    (t) async {
      final tips = TipPreferences(MemoryTipStore());
      await tips.save('footer', 'One\nTwo');
      final audio = FakeAudio();
      final music = MusicController(
        tracks: [track('song', lyrics: '[00:00]Lyric')],
        createTransport: () => audio,
        onSave: () {},
      );
      await t.pumpWidget(
        TipPreferencesScope(
          controller: tips,
          child: MaterialApp(
            locale: const Locale('en'),
            localizationsDelegates: AppLocalizations.localizationsDelegates,
            supportedLocales: AppLocalizations.supportedLocales,
            home: AppearanceScope(
              palette: const Palette(StudioTheme.white, GlassMode.frosted),
              child: Scaffold(body: MusicFooter(music: music)),
            ),
          ),
        ),
      );
      await t.pumpAndSettle();
      expect(find.text('One'), findsOneWidget);
      await t.pump(const Duration(seconds: 14));
      await t.pumpAndSettle();
      expect(find.text('Two'), findsOneWidget);
      await music.toggle();
      music.setShowLyrics(true);
      await t.pumpAndSettle();
      expect(find.text('Lyric'), findsOneWidget);
      music.setShowLyrics(false);
      await t.pumpAndSettle();
      expect(find.text('Two'), findsOneWidget);
      await t.pumpWidget(const SizedBox());
      music.dispose();
      tips.dispose();
    },
  );
}
