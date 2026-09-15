import 'dart:io';
import 'package:flutter/services.dart';
import 'package:morrow_studio/desktop_frame.dart';
import 'package:morrow_studio/main.dart';
import 'package:morrow_studio/media/texture_backdrop.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/storage.dart';
import 'package:shared_preferences/shared_preferences.dart';
import 'package:morrow_studio/media/texture_source.dart';

void main() {
  testWidgets(
    'Header controls remain at the right inset on desktop and mobile',
    (tester) async {
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      for (final width in [1440.0, 1050.0, 800.0, 760.0, 390.0]) {
        tester.view.physicalSize = Size(width, 1000);
        await tester.pumpWidget(const MorrowApp(initialLocale: Locale('zh')));
        await tester.pumpAndSettle();
        final settings = find.byKey(const ValueKey('appearance-toggle'));
        final search = find.byKey(const ValueKey('header-search'));
        final inset = width >= 1050 ? 24.0 : 12.0;
        expect(tester.getRect(settings).right, closeTo(width - inset, .1));
        expect(
          tester.getRect(search).right,
          closeTo(tester.getRect(settings).left - 8, .1),
        );
        await tester.tap(settings);
        await tester.pumpAndSettle();
        if (width < 1050) {
          expect(settings, findsNothing);
          expect(
            find.byKey(const ValueKey('compact-settings-back')),
            findsOneWidget,
          );
          expect(find.byTooltip('关闭设置'), findsNothing);
        } else {
          expect(tester.getRect(settings).right, closeTo(width - inset, .1));
        }
        expect(tester.takeException(), isNull);
        await tester.pumpWidget(const SizedBox());
      }
    },
  );

  testWidgets(
    'Desktop caption inherits custom color, opacity and transparency',
    (tester) async {
      const channel = MethodChannel('window_manager');
      tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        channel,
        (call) async => false,
      );
      addTearDown(
        () => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
          channel,
          null,
        ),
      );
      const color = Color(0xff2468ab);
      for (final backdrop in BackgroundMode.values) {
        final palette = Palette(
          StudioTheme.dark,
          GlassMode.frosted,
          backdrop,
          0,
          .2,
          color,
        );
        await tester.pumpWidget(
          MaterialApp(
            home: DesktopFrame(
              palette: palette,
              child: const SizedBox.expand(),
            ),
          ),
        );
        await tester.pumpAndSettle();
        final caption = tester.widget<AnimatedContainer>(
          find.byKey(const ValueKey('desktop-caption')),
        );
        final captionColor = (caption.decoration! as BoxDecoration).color;
        if (backdrop == BackgroundMode.transparent ||
            backdrop == BackgroundMode.texture) {
          expect(captionColor, Colors.transparent);
        } else if (backdrop == BackgroundMode.solid) {
          expect(captionColor, color.withValues(alpha: .2));
        }
        if (backdrop == BackgroundMode.transparent) {
          final outline = tester.widget<AnimatedContainer>(
            find.byKey(const ValueKey('desktop-outline')),
          );
          expect(
            ((outline.decoration! as BoxDecoration).border! as Border)
                .top
                .color,
            Colors.transparent,
          );
        }
        expect(tester.takeException(), isNull);
      }
      await tester.pumpWidget(const SizedBox());
    },
  );

  Future<void> launch(WidgetTester tester, Size size) async {
    tester.view.physicalSize = size;
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);
    final previousHandler = FlutterError.onError;
    FlutterError.onError = (details) {
      FlutterError.dumpErrorToConsole(details);
      previousHandler?.call(details);
    };
    await tester.pumpWidget(const MorrowApp(initialLocale: Locale('zh')));
    await tester.pumpAndSettle();
  }

  testWidgets('All 36 appearance combinations render on desktop and mobile', (
    tester,
  ) async {
    await launch(tester, const Size(1440, 1000));
    for (final size in [const Size(1440, 1000), const Size(390, 844)]) {
      tester.view.physicalSize = size;
      await tester.pumpAndSettle();
      if (size.width < 1050) {
        await tester.tap(find.byKey(const ValueKey('appearance-toggle')));
        await tester.pumpAndSettle();
      }
      for (final theme in StudioTheme.values) {
        for (final mode in GlassMode.values) {
          final themeControl = find.byKey(ValueKey('theme-${theme.name}'));
          await tester.ensureVisible(themeControl);
          await tester.tap(themeControl);
          await tester.pumpAndSettle();
          final modeControl = find.byKey(ValueKey('mode-${mode.name}'));
          await tester.ensureVisible(modeControl);
          await tester.tap(modeControl);
          await tester.pumpAndSettle();
          final studio = tester.widget<Studio>(find.byType(Studio));
          expect(studio.palette.theme, theme);
          expect(studio.palette.mode, mode);
          expect(tester.takeException(), isNull);
          for (final background in BackgroundMode.values) {
            final control = find.byKey(
              ValueKey('background-${background.name}'),
            );
            await tester.ensureVisible(control);
            await tester.tap(control);
            await tester.pumpAndSettle();
            expect(
              tester.widget<Studio>(find.byType(Studio)).palette.backdrop,
              background,
            );
            expect(tester.takeException(), isNull);
          }
        }
      }
    }
  });

  testWidgets('Search, favorites and new ideas work together', (tester) async {
    await launch(tester, const Size(1440, 1100));
    await tester.enterText(find.byType(TextField).first, '数字花园');
    await tester.pumpAndSettle();
    expect(find.text('一个安静的数字花园'), findsOneWidget);
    expect(find.text('给灵感一个容器'), findsNothing);
    await tester.tap(find.byTooltip('清空搜索'));
    await tester.pumpAndSettle();
    await tester.tap(find.byTooltip('收藏 周末，做点无用的东西'));
    await tester.tap(find.text('已收藏'));
    await tester.pumpAndSettle();
    expect(find.text('周末，做点无用的东西'), findsOneWidget);
    expect(find.text('我的桌面小助手'), findsNothing);
    await tester.tap(find.text('新建灵感'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('保存灵感'));
    await tester.pumpAndSettle();
    expect(find.text('先写下你的想法吧'), findsOneWidget);
    await tester.enterText(
      find.byKey(const ValueKey('idea-title')),
      '测试一闪而过的灵感',
    );
    await tester.enterText(
      find.byKey(const ValueKey('idea-description')),
      '值得留下的细节',
    );
    await tester.tap(find.text('保存灵感'));
    await tester.pumpAndSettle();
    expect(find.text('测试一闪而过的灵感'), findsOneWidget);
    await tester.tap(find.text('测试一闪而过的灵感'));
    await tester.pumpAndSettle();
    expect(find.text('值得留下的细节'), findsNWidgets(2));
    expect(tester.takeException(), isNull);
  });

  testWidgets('Page navigation crossfades and respects reduced motion', (
    tester,
  ) async {
    await launch(tester, const Size(1440, 1000));
    await tester.tap(find.text('小项目'));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 100));
    expect(
      find.byKey(const ValueKey('page-workbench.page.overview')),
      findsOneWidget,
    );
    expect(
      find.byKey(const ValueKey('page-workbench.page.projects')),
      findsOneWidget,
    );
    await tester.pumpAndSettle();
    expect(
      find.byKey(const ValueKey('page-workbench.page.overview')),
      findsNothing,
    );
    await tester.tap(find.text('实验室').first);
    await tester.pump();
    await tester.tap(find.text('已收藏').first);
    await tester.pumpAndSettle();
    expect(
      find.byKey(const ValueKey('page-workbench.page.favorites')),
      findsOneWidget,
    );
    expect(tester.takeException(), isNull);
    tester.platformDispatcher.accessibilityFeaturesTestValue =
        const FakeAccessibilityFeatures(disableAnimations: true);
    addTearDown(tester.platformDispatcher.clearAccessibilityFeaturesTestValue);
    await tester.pumpAndSettle();
    await tester.tap(find.text('概览').first);
    await tester.pumpAndSettle();
    final transition = tester.widget<AnimatedSwitcher>(
      find
          .ancestor(
            of: find.byKey(const ValueKey('page-workbench.page.overview')),
            matching: find.byType(AnimatedSwitcher),
          )
          .first,
    );
    expect(transition.duration, Duration.zero);
  });

  testWidgets('Edit, tasks, delete undo and appearance survive a fresh app', (
    tester,
  ) async {
    SharedPreferences.setMockInitialValues({});
    final storage = LocalStorage(await SharedPreferences.getInstance());
    tester.view.physicalSize = const Size(1440, 1100);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);
    await tester.pumpWidget(
      MorrowApp(initialLocale: const Locale('zh'), storage: storage),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('一个安静的数字花园'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('整理第一批收藏'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('编辑'));
    await tester.pumpAndSettle();
    await tester.enterText(
      find.byKey(const ValueKey('idea-title')),
      '数字花园 · 第二版',
    );
    await tester.tap(find.text('保存灵感'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('数字花园 · 第二版'));
    await tester.pumpAndSettle();
    expect(find.text('小小的进展 · 1/3'), findsOneWidget);
    await tester.tap(find.text('删除'));
    await tester.pumpAndSettle();
    expect(find.text('数字花园 · 第二版'), findsNothing);
    await tester.tap(find.text('撤销'));
    await tester.pumpAndSettle();
    expect(find.text('数字花园 · 第二版'), findsOneWidget);
    for (final key in [
      'theme-dark',
      'mode-clear',
      'background-solid',
      'tint-2',
    ]) {
      final control = find.byKey(ValueKey(key));
      await tester.ensureVisible(control);
      await tester.tap(control);
      await tester.pumpAndSettle();
    }
    await tester.pumpWidget(const SizedBox());
    await tester.pumpWidget(
      MorrowApp(
        initialLocale: const Locale('zh'),
        storage: LocalStorage(await SharedPreferences.getInstance()),
      ),
    );
    await tester.pumpAndSettle();
    final palette = tester.widget<Studio>(find.byType(Studio)).palette;
    expect(palette.theme, StudioTheme.dark);
    expect(palette.mode, GlassMode.clear);
    expect(palette.backdrop, BackgroundMode.solid);
    expect(palette.solidTint, 2);
    await tester.tap(find.text('数字花园 · 第二版'));
    await tester.pumpAndSettle();
    expect(find.text('小小的进展 · 1/3'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });

  testWidgets(
    'Opacity lower bound and compass color persist; dialogs follow appearance',
    (tester) async {
      final storage = MemoryStorage();
      tester.view.physicalSize = const Size(1440, 1100);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      await tester.pumpWidget(
        MorrowApp(initialLocale: const Locale('zh'), storage: storage),
      );
      await tester.pumpAndSettle();
      final slider = find.byKey(const ValueKey('frosted-opacity'));
      await tester.ensureVisible(slider);
      await tester.drag(slider, const Offset(-500, 0));
      await tester.pumpAndSettle();
      expect(
        tester.widget<Studio>(find.byType(Studio)).palette.frostedOpacity,
        .2,
      );
      expect(storage.data!['frostedOpacity'], .2);
      final solid = find.byKey(const ValueKey('background-solid'));
      await tester.ensureVisible(solid);
      await tester.tap(solid);
      await tester.pumpAndSettle();
      final compass = find.byKey(const ValueKey('custom-color'));
      await tester.ensureVisible(compass);
      await tester.tap(compass);
      await tester.pumpAndSettle();
      final wheel = find.byKey(const ValueKey('color-wheel'));
      await tester.drag(wheel, const Offset(45, 20));
      await tester.pumpAndSettle();
      await tester.enterText(
        find.byKey(const ValueKey('color-hex')),
        '#XYZZZZ',
      );
      await tester.tap(find.text('应用颜色'));
      await tester.pumpAndSettle();
      expect(find.text('请输入 6 位十六进制色值'), findsOneWidget);
      await tester.enterText(
        find.byKey(const ValueKey('color-hex')),
        '#2468AB',
      );
      await tester.tap(find.text('应用颜色'));
      await tester.pumpAndSettle();
      expect(
        tester.widget<Studio>(find.byType(Studio)).palette.solidColor,
        const Color(0xFF2468AB),
      );
      await tester.pumpWidget(const SizedBox());
      await tester.pumpWidget(
        MorrowApp(initialLocale: const Locale('zh'), storage: storage),
      );
      await tester.pumpAndSettle();
      expect(
        tester.widget<Studio>(find.byType(Studio)).palette.customColor,
        const Color(0xFF2468AB),
      );
      expect(
        tester.widget<Studio>(find.byType(Studio)).palette.frostedOpacity,
        .2,
      );
      for (final mode in [BackgroundMode.solid, BackgroundMode.texture]) {
        final control = find.byKey(ValueKey('background-${mode.name}'));
        await tester.ensureVisible(control);
        await tester.tap(control);
        await tester.pumpAndSettle();
        await tester.tap(find.text('新建灵感'));
        await tester.pumpAndSettle();
        final surface = tester
            .widgetList<Glass>(find.byType(Glass))
            .singleWhere((glass) => glass.dialog);
        expect(surface.p.backdrop, mode);
        expect(surface.p.frostedOpacity, .2);
        expect(
          tester.widget<Dialog>(find.byType(Dialog)).backgroundColor,
          Colors.transparent,
        );
        await tester.tap(find.byTooltip('关闭弹窗'));
        await tester.pumpAndSettle();
      }
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets(
    'Compact dialogs remain usable and invalid media addresses are rejected',
    (tester) async {
      await launch(tester, const Size(390, 844));
      await tester.tap(find.byKey(const ValueKey('appearance-toggle')));
      await tester.pumpAndSettle();
      await tester.ensureVisible(
        find.byKey(const ValueKey('background-texture')),
      );
      await tester.tap(find.byKey(const ValueKey('background-texture')));
      await tester.pumpAndSettle();
      await tester.ensureVisible(find.byKey(const ValueKey('texture-link')));
      await tester.tap(find.byKey(const ValueKey('texture-link')));
      await tester.pumpAndSettle();
      await tester.enterText(
        find.byKey(const ValueKey('texture-url')),
        'file:///C:/private.txt',
      );
      await tester.tap(find.text('应用素材'));
      await tester.pumpAndSettle();
      expect(find.text('请输入有效且不含登录信息的 HTTP / HTTPS 地址'), findsOneWidget);
      expect(tester.takeException(), isNull);
      await tester.tap(find.byTooltip('关闭弹窗'));
      await tester.pumpAndSettle();
      await tester.tap(find.byKey(const ValueKey('compact-settings-back')));
      await tester.pumpAndSettle();
      await tester.ensureVisible(find.text('新建灵感'));
      await tester.tap(find.text('新建灵感'));
      await tester.pumpAndSettle();
      expect(find.byKey(const ValueKey('idea-title')), findsOneWidget);
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets(
    'Restored image fills the canvas and survives background toggles',
    (tester) async {
      tester.view.physicalSize = const Size(1440, 1000);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      final storage = MemoryStorage()
        ..data = {
          'theme': 'white',
          'glass': 'frosted',
          'background': 'texture',
          'ideas': <dynamic>[],
          'texture': TextureSource(
            location: File('test/fixtures/texture.png').absolute.path,
            name: 'texture.png',
            kind: TextureKind.image,
            local: true,
          ).toJson(),
        };
      await tester.runAsync(() async {
        await tester.pumpWidget(
          MorrowApp(initialLocale: const Locale('zh'), storage: storage),
        );
        await tester.pumpAndSettle();
        await precacheImage(
          FileImage(File('test/fixtures/texture.png')),
          tester.element(find.byType(Studio)),
        );
      });
      await tester.pumpAndSettle();
      final image = find.descendant(
        of: find.byType(TextureBackdrop),
        matching: find.byType(Image),
      );
      expect(image, findsOneWidget);
      expect(tester.getSize(image), const Size(1440, 1000));
      for (final mode in ['ambient', 'texture']) {
        final control = find.byKey(ValueKey('background-$mode'));
        await tester.ensureVisible(control);
        await tester.tap(control);
        await tester.pumpAndSettle();
      }
      expect(find.byType(TextureBackdrop), findsOneWidget);
      expect(tester.takeException(), isNull);
    },
  );

  test(
    'Texture references preserve type and reject non-web or credential URLs',
    () {
      const source = TextureSource(
        location: 'asset-key',
        name: 'movie.webm',
        kind: TextureKind.video,
        local: true,
      );
      expect(TextureSource.fromJson(source.toJson()).kind, TextureKind.video);
      expect(TextureSource.kindFor('Loop.GIF'), TextureKind.gif);
      expect(
        TextureSource.validUrl('https://example.com/image.png?size=1'),
        isTrue,
      );
      for (final value in [
        'javascript:alert(1)',
        'file:///tmp/x',
        'data:image/png,a',
        'https://user:password@example.com/image.png',
      ]) {
        expect(TextureSource.validUrl(value), isFalse);
      }
    },
  );
}
