import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:morrow_studio/desktop_frame.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_i18n/morrow_i18n.dart';
import 'package:morrow_studio/appearance.dart';
import 'package:morrow_studio/color_compass.dart';
import 'package:morrow_studio/component_material_page.dart';
import 'package:morrow_studio/attachments/attachment.dart';
import 'package:morrow_studio/attachments/attachment_view.dart';
import 'package:morrow_studio/content/idea_markdown.dart';
import 'package:morrow_studio/media/texture_link_dialog.dart';
import 'package:morrow_studio/media/texture_source.dart';
import 'package:morrow_studio/music/lyrics_dialog.dart';
import 'package:morrow_studio/music/lyrics_service.dart';
import 'package:morrow_studio/music/music_controller.dart';
import 'package:morrow_studio/music/music_panel.dart';
import 'package:morrow_studio/little_tips.dart';

Widget host(String locale, Widget child) => MaterialApp(
  locale: Locale(locale),
  supportedLocales: L10n.supportedLocales,
  localizationsDelegates: const [
    L10n.delegate,
    ...GlobalMaterialLocalizations.delegates,
  ],
  home: AppearanceScope(
    palette: const Palette(StudioTheme.white, GlassMode.clear),
    child: Scaffold(body: child),
  ),
);

class FixedLyrics extends LyricsService {
  bool fail = false;
  @override
  Future<List<LyricMatch>> search(String title, String artist) async {
    if (fail) throw const FormatException('private diagnostic');
    return const [
      LyricMatch(
        title: '歌曲原名',
        artist: '原歌手',
        lyrics: '歌词文件',
        album: '原专辑',
        duration: 1,
      ),
      LyricMatch(
        title: '第二首',
        artist: '原歌手',
        lyrics: '另一行',
        album: '第二张',
        duration: 2,
        synced: true,
      ),
    ];
  }
}

void main() {
  testWidgets('Color compass changes language without resetting edited color', (
    tester,
  ) async {
    const dialog = ColorCompassDialog(initial: Color(0xff123456));
    await tester.pumpWidget(host('zh', dialog));
    await tester.pumpAndSettle();
    expect(find.text('给空间一点颜色'), findsOneWidget);
    await tester.enterText(find.byKey(const ValueKey('color-hex')), '#abcdef');
    await tester.pumpWidget(host('en', dialog));
    await tester.pumpAndSettle();
    expect(find.text('Add color to your space'), findsOneWidget);
    expect(find.text('Apply color'), findsOneWidget);
    expect(
      tester
          .widget<TextField>(find.byKey(const ValueKey('color-hex')))
          .controller!
          .text,
      '#abcdef',
    );
    for (final code in L10n.nativeNames.keys) {
      await tester.pumpWidget(host(code, dialog));
      await tester.pumpAndSettle();
      expect(
        tester
            .widget<TextField>(find.byKey(const ValueKey('color-hex')))
            .controller!
            .text,
        '#abcdef',
      );
      expect(tester.takeException(), isNull, reason: code);
    }
    expect(tester.takeException(), isNull);
  });

  testWidgets(
    'Component material labels translate while stable component keys survive saving',
    (tester) async {
      SurfaceSettings? saved;
      await tester.pumpWidget(
        host(
          'en',
          ComponentMaterialListPage(
            palette: const Palette(StudioTheme.white, GlassMode.clear),
            entries: const {'summary:工作台': '用户卡片名'},
            onChanged: (value) => saved = value,
          ),
        ),
      );
      await tester.pumpAndSettle();
      expect(find.text('Components and cards'), findsOneWidget);
      await tester.tap(
        find.byKey(const ValueKey('component-entry:summary:工作台')),
      );
      await tester.pumpAndSettle();
      await tester.tap(find.byKey(const ValueKey('component-custom-toggle')));
      await tester.pumpAndSettle();
      expect(find.text('Frosting'), findsOneWidget);
      await tester.ensureVisible(find.byKey(const ValueKey('component-apply')));
      await tester.tap(find.byKey(const ValueKey('component-apply')));
      await tester.pumpAndSettle();
      expect(saved!.components.keys, ['summary:工作台']);
      expect(saved!.components['summary:工作台']!.enabled, isTrue);
      expect(find.text('用户卡片名'), findsOneWidget);
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets(
    'Media link validation follows locale and preserves entered URL',
    (tester) async {
      const dialog = TextureLinkDialog();
      await tester.pumpWidget(host('zh', dialog));
      await tester.pumpAndSettle();
      await tester.enterText(
        find.byKey(const ValueKey('texture-url')),
        'https://user:pass@example.com/原图.png',
      );
      await tester.tap(find.text('应用素材'));
      await tester.pumpAndSettle();
      await tester.pumpWidget(host('en', dialog));
      await tester.pumpAndSettle();
      expect(
        find.text(
          'Enter a valid HTTP or HTTPS address without login information.',
        ),
        findsOneWidget,
      );
      expect(
        tester
            .widget<TextField>(find.byKey(const ValueKey('texture-url')))
            .controller!
            .text,
        'https://user:pass@example.com/原图.png',
      );
      for (final code in L10n.nativeNames.keys) {
        await tester.pumpWidget(host(code, dialog));
        await tester.pumpAndSettle();
        expect(
          tester
              .widget<TextField>(find.byKey(const ValueKey('texture-url')))
              .controller!
              .text,
          'https://user:pass@example.com/原图.png',
        );
        expect(find.byType(TextField), findsWidgets);
        expect(tester.takeException(), isNull, reason: code);
      }
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets(
    'Markdown translates controls without translating user text or alt text',
    (tester) async {
      await tester.pumpWidget(
        host(
          'en',
          const IdeaMarkdown(
            data: '用户正文：关闭\n\n![图片原名](https://example.com/image.png)',
          ),
        ),
      );
      await tester.pumpAndSettle();
      expect(find.textContaining('用户正文：关闭'), findsWidgets);
      expect(find.text('Load image · 图片原名'), findsOneWidget);
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets(
    'Lyrics matches use English plurals but preserve returned metadata',
    (tester) async {
      final service = FixedLyrics();
      await tester.pumpWidget(
        host(
          'en',
          LyricsSearchDialog(title: '歌曲原名', artist: '原歌手', service: service),
        ),
      );
      await tester.pumpAndSettle();
      await tester.tap(find.text('Search'));
      await tester.pumpAndSettle();
      expect(find.text('歌曲原名 · 原歌手'), findsOneWidget);
      expect(find.text('原专辑\nPlain-text lyrics · 1 second'), findsOneWidget);
      expect(find.text('第二张\nSynced lyrics · 2 seconds'), findsOneWidget);
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets(
    'Stored lyrics error rerenders in new locale without retrying network',
    (tester) async {
      final service = FixedLyrics()..fail = true;
      final dialog = LyricsSearchDialog(
        title: '原歌名',
        artist: '',
        service: service,
      );
      await tester.pumpWidget(host('zh', dialog));
      await tester.pumpAndSettle();
      await tester.tap(find.text('搜索'));
      await tester.pumpAndSettle();
      await tester.pumpWidget(host('en', dialog));
      await tester.pumpAndSettle();
      expect(
        find.text(
          'Could not connect to the lyric service. Try again later or import local lyrics.',
        ),
        findsOneWidget,
      );
      expect(find.text('private diagnostic'), findsNothing);
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets(
    'Music labels and untimed suffix localize without rewriting persisted lyrics',
    (tester) async {
      final track = MusicTrack(
        source: const TextureSource(
          location: 'synthetic.mp3',
          name: '原文件.mp3',
          kind: TextureKind.audio,
        ),
        lyrics: '歌词文件\n关闭',
        lyricSource: '歌词文件',
      );
      final music = MusicController(
        tracks: [
          track,
          MusicTrack(
            source: const TextureSource(
              location: "second.mp3",
              name: "第二首.mp3",
              kind: TextureKind.audio,
            ),
          ),
        ],
      );
      final original = music.toJson();
      await tester.pumpWidget(
        host(
          'en',
          Builder(
            builder: (context) => Column(
              children: [
                Text(musicSourceLabel(context, track.lyricSource)),
                Text(musicSourceLabel(context, '自定义来源')),
                Text(
                  music.lyricForDisplay(
                    untimedLabel: L10n.of(context).visualNoTimeline,
                  )!,
                ),
                Text(localizedCornerTips(context).first),
                SizedBox(width: 252, child: MusicPanel(controller: music)),
              ],
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();
      expect(find.text('Lyric file'), findsOneWidget);
      expect(find.text('自定义来源'), findsOneWidget);
      expect(find.text('歌词文件 · No timing data'), findsOneWidget);
      expect(find.text('1 / 2 · Paused'), findsOneWidget);
      expect(music.toJson(), original);
      expect(music.lyric, '歌词文件 · 无时间轴');
      await tester.pumpWidget(const SizedBox());
      music.dispose();
    },
  );

  testWidgets(
    'Attachment actions translate while filename and stable attachment ID remain intact',
    (tester) async {
      const attachment = IdeaAttachment(
        source: TextureSource(
          location: 'synthetic.file',
          name: '用户附件.txt',
          kind: TextureKind.file,
        ),
        size: 1024,
        pluginId: '附件:stable',
      );
      final original = attachment.toJson();
      await tester.pumpWidget(
        host('en', const AttachmentTile(attachment: attachment)),
      );
      await tester.pumpAndSettle();
      expect(find.text('用户附件.txt'), findsOneWidget);
      expect(find.byTooltip('Save attachment as'), findsOneWidget);
      expect(find.text('TXT · 1 KB · Open with default app'), findsOneWidget);
      expect(attachment.toJson(), original);
    },
  );

  testWidgets(
    'Legacy MaterialApp without delegates keeps Chinese visual defaults',
    (tester) async {
      await tester.pumpWidget(
        const MaterialApp(home: ColorCompassDialog(initial: Colors.blue)),
      );
      expect(find.text('给空间一点颜色'), findsOneWidget);
      expect(find.text('应用颜色'), findsOneWidget);
      expect(tester.takeException(), isNull);
    },
  );
  testWidgets(
    'English music panel fits the compact sidebar without untranslated controls',
    (tester) async {
      final music = MusicController();
      await tester.pumpWidget(
        host(
          'en',
          SingleChildScrollView(
            child: SizedBox(width: 252, child: MusicPanel(controller: music)),
          ),
        ),
      );
      await tester.pumpAndSettle();
      expect(find.text('Music player'), findsOneWidget);
      expect(find.byTooltip('Import music'), findsOneWidget);
      await tester.tap(find.byKey(const ValueKey('music-list-toggle')));
      await tester.pumpAndSettle();
      expect(find.text('Your playlist is empty'), findsOneWidget);
      expect(
        find.text('Find missing lyrics online automatically'),
        findsOneWidget,
      );
      expect(tester.takeException(), isNull);
      await tester.pumpWidget(const SizedBox());
      music.dispose();
    },
  );

  testWidgets(
    'Window caption labels follow locale while commands retain native names',
    (tester) async {
      final calls = <String>[];
      const channel = MethodChannel('window_manager');
      tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(channel, (
        call,
      ) async {
        calls.add(call.method);
        if (call.method == 'isMaximized' || call.method == 'isMinimized') {
          return false;
        }
        return null;
      });
      addTearDown(
        () => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
          channel,
          null,
        ),
      );
      Widget framedApp(String locale) => MaterialApp(
        locale: Locale(locale),
        supportedLocales: L10n.supportedLocales,
        localizationsDelegates: const [
          L10n.delegate,
          ...GlobalMaterialLocalizations.delegates,
        ],
        builder: (context, navigator) => DesktopFrame(
          palette: const Palette(StudioTheme.white, GlassMode.clear),
          child: navigator!,
        ),
        home: const Scaffold(body: Text('Window body')),
      );
      await tester.pumpWidget(framedApp('en'));
      await tester.pumpAndSettle();
      expect(find.byTooltip('Close window'), findsOneWidget);
      await tester.tap(find.byTooltip('Minimize'));
      await tester.pumpAndSettle();
      expect(calls, contains('minimize'));
      await tester.pumpWidget(framedApp('zh'));
      await tester.pumpAndSettle();
      expect(find.byTooltip('关闭窗口'), findsOneWidget);
      expect(find.text('Morrow'), findsOneWidget);
      expect(tester.takeException(), isNull);
    },
  );
}
