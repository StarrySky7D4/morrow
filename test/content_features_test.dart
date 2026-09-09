import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'package:morrow_studio/attachments/attachment.dart';
import 'package:morrow_studio/attachments/clipboard_import.dart';
import 'package:morrow_studio/main.dart';
import 'package:morrow_studio/media/texture_source.dart';
import 'package:morrow_studio/music/embedded_tags.dart';
import 'package:morrow_studio/music/lyrics_service.dart';
import 'package:morrow_studio/music/music_controller.dart';
import 'package:morrow_studio/music/track_metadata_native.dart';
import 'package:morrow_studio/storage.dart';
import 'package:file_selector/file_selector.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart';

Uint8List taggedAudio() {
  List<int> frame(String id, List<int> value) => [
    ...ascii.encode(id),
    0,
    0,
    value.length >> 8,
    value.length & 255,
    0,
    0,
    ...value,
  ];
  final frames = [
    ...frame('TIT2', [3, ...utf8.encode('Test Song')]),
    ...frame('TPE1', [3, ...utf8.encode('Test Artist')]),
    ...frame('USLT', [
      3,
      ...ascii.encode('eng'),
      0,
      ...utf8.encode('[00:01.00]内嵌测试歌词'),
    ]),
  ];
  return Uint8List.fromList([
    ...ascii.encode('ID3'),
    3,
    0,
    0,
    0,
    0,
    frames.length >> 7,
    frames.length & 127,
    ...frames,
  ]);
}

void main() {
  test(
    'FLAC and M4A embedded lyrics survive metadata parsing and truncation',
    () {
      List<int> le(int n) => [
        n & 255,
        n >> 8 & 255,
        n >> 16 & 255,
        n >> 24 & 255,
      ];
      List<int> be(int n) => le(n).reversed.toList();
      final comments = [
        'TITLE=Test Song',
        'ARTIST=Test Artist',
        'LYRICS=没有时间轴的歌词',
      ];
      final block = [
        0,
        0,
        0,
        0,
        ...le(comments.length),
        for (final comment in comments) ...[
          ...le(utf8.encode(comment).length),
          ...utf8.encode(comment),
        ],
      ];
      final flac = Uint8List.fromList([
        ...ascii.encode('fLaC'),
        132,
        ...be(block.length).skip(1),
        ...block,
      ]);
      final tags = readEmbeddedTags(flac);
      expect(tags.title, 'Test Song');
      expect(tags.embeddedLyrics, '没有时间轴的歌词');
      expect(
        readEmbeddedTags(flac.sublist(0, flac.length - 1)).embeddedLyrics,
        '',
      );
      List<int> atom(String type, List<int> body) => [
        ...be(body.length + 8),
        ...latin1.encode(type),
        ...body,
      ];
      List<int> field(String name, String text) => atom(
        name,
        atom('data', [0, 0, 0, 1, 0, 0, 0, 0, ...utf8.encode(text)]),
      );
      final mp4 = Uint8List.fromList([
        ...atom('ftyp', ascii.encode('M4A ')),
        ...atom(
          'moov',
          atom(
            'udta',
            atom('meta', [
              0,
              0,
              0,
              0,
              ...atom('ilst', [
                ...field('©nam', 'Test Song'),
                ...field('©ART', 'Test Artist'),
                ...field('©lyr', '[00:01]内置歌词'),
              ]),
            ]),
          ),
        ),
      ]);
      final m4a = readEmbeddedTags(mp4);
      expect(m4a.artist, 'Test Artist');
      expect(m4a.embeddedLyrics, '[00:01]内置歌词');
      expect(readEmbeddedTags(mp4.sublist(0, 25)).embeddedLyrics, '');
    },
  );

  test(
    'Embedded ID3 lyrics and metadata decode without inventing timestamps',
    () {
      final metadata = readEmbeddedTags(taggedAudio());
      expect(metadata.title, 'Test Song');
      expect(metadata.artist, 'Test Artist');
      expect(metadata.embeddedLyrics, '[00:01.00]内嵌测试歌词');
      expect(
        readEmbeddedTags(Uint8List.fromList([1, 2, 3])).embeddedLyrics,
        '',
      );
      expect(readEmbeddedTags(taggedAudio().sublist(0, 18)).embeddedLyrics, '');
    },
  );

  test(
    'Native metadata detects same-name LRC and embedded lyrics independently',
    () async {
      final folder = await Directory('build').createTemp('lyrics-test-');
      addTearDown(() => folder.delete(recursive: true));
      final mp3 = File('${folder.path}/track.mp3');
      await mp3.writeAsBytes(taggedAudio());
      await File('${folder.path}/track.LRC').writeAsString('[00:01.00]文件优先');
      final metadata = await readTrackMetadata(XFile(mp3.path));
      expect(metadata.fileLyrics, '[00:01.00]文件优先');
      expect(metadata.embeddedLyrics, contains('内嵌测试歌词'));
    },
  );

  test(
    'Online matching rejects ambiguous versions and prefers synchronized exact results',
    () {
      const sync = LyricMatch(
        title: 'Song',
        artist: 'Artist',
        lyrics: '[00:01]line',
        synced: true,
        duration: 100,
      );
      const plain = LyricMatch(
        title: 'Song',
        artist: 'Artist',
        lyrics: 'line',
        duration: 100,
      );
      const other = LyricMatch(
        title: 'Song',
        artist: 'Someone',
        lyrics: 'line',
        duration: 100,
      );
      expect(
        LyricsService.exactMatch([plain, sync], 'Song', 'Artist', 100),
        sync,
      );
      expect(LyricsService.exactMatch([sync, other], 'Song', '', 100), isNull);
      expect(LyricsService.exactMatch([sync], 'Song', 'Artist', 115), isNull);
    },
  );

  test(
    'Lyric file wins over late network results and results stay with their track',
    () async {
      final pending = Completer<http.Response>();
      final client = MockClient((request) {
        expect(request.url.host, 'lrclib.net');
        expect(request.url.queryParameters['track_name'], 'Song');
        return pending.future;
      });
      final first = MusicTrack(
        source: const TextureSource(
          location: 'song',
          name: 'Song.mp3',
          kind: TextureKind.audio,
        ),
        artist: 'Artist',
        metadataRead: true,
      );
      final second = MusicTrack(
        source: const TextureSource(
          location: 'other',
          name: 'Other.mp3',
          kind: TextureKind.audio,
        ),
        metadataRead: true,
      );
      final music = MusicController(
        tracks: [first, second],
        onlineLyrics: true,
        lyricsService: LyricsService(client: client),
      );
      final task = music.loadLyrics(first);
      music.index = 1;
      music.setLyrics('[00:01]文件歌词', track: first);
      pending.complete(
        http.Response(
          jsonEncode([
            {
              'trackName': 'Song',
              'artistName': 'Artist',
              'syncedLyrics': '[00:01]network',
            },
          ]),
          200,
        ),
      );
      await task;
      expect(first.lyrics, '[00:01]文件歌词');
      expect(first.lyricSource, '歌词文件');
      expect(second.lyrics, '');
      music.dispose();
      client.close();
    },
  );

  test(
    'Offline lyrics avoid requests; untimed lyrics remain readable after restore',
    () async {
      final track = MusicTrack(
        source: const TextureSource(
          location: 'song',
          name: 'Song.flac',
          kind: TextureKind.audio,
        ),
        lyrics: '第一行\n第二行',
        lyricSource: '音频内嵌',
      );
      final client = MockClient(
        (_) => throw StateError('Network must not be used'),
      );
      final music = MusicController(
        tracks: [track],
        lyricsService: LyricsService(client: client),
      );
      await music.loadLyrics(track);
      expect(music.lyric, contains('无时间轴'));
      expect(MusicTrack.fromJson(track.toJson()).lyrics, '第一行\n第二行');
      music.dispose();
      client.close();
    },
  );

  test(
    'CAD and Blender attachments preserve file metadata in idea snapshots',
    () {
      for (final name in [
        'assembly.dwg',
        'scene.blend',
        'model.step',
        'drawing.dxf',
      ]) {
        final attachment = IdeaAttachment(
          source: TextureSource(
            location: 'stored',
            name: name,
            kind: IdeaAttachment.kindFor(name),
            local: true,
          ),
          size: 1024,
        );
        final idea = Idea(
          '设计',
          '',
          '实验',
          Icons.auto_awesome_outlined,
          Colors.purple,
          attachments: [attachment],
          hypothesis: '验证结构',
          conclusion: '尺寸合适',
          stage: '已记录',
        );
        final restored = Idea.fromJson(idea.toJson());
        expect(restored.attachments.single.source.kind, TextureKind.file);
        expect(restored.attachments.single.source.name, name);
        expect(restored.hypothesis, '验证结构');
        expect(restored.conclusion, '尺寸合适');
        expect(restored.stage, '已记录');
      }
    },
  );

  testWidgets(
    'Rich paste inserts text at selection, keeps mixed attachments and allows file-only records',
    (tester) async {
      tester.view.physicalSize = const Size(1100, 1000);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      Idea? saved;
      await tester.pumpWidget(
        MaterialApp(
          home: Builder(
            builder: (context) => Scaffold(
              body: TextButton(
                onPressed: () async {
                  saved = await showStudioDialog<Idea>(
                    context: context,
                    builder: (_) => NewIdeaDialog(
                      readClipboard: () async => PastedContent(
                        text: '替换文本',
                        files: [
                          XFile.fromData(
                            Uint8List(2),
                            path: 'scene.blend',
                            name: 'scene.blend',
                          ),
                          XFile.fromData(
                            Uint8List(2),
                            path: 'photo.png',
                            name: 'photo.png',
                          ),
                        ],
                      ),
                      importAttachment: (file) async => IdeaAttachment(
                        source: TextureSource(
                          location: 'fixture',
                          name: file.name,
                          kind: IdeaAttachment.kindFor(file.name),
                        ),
                        size: 2,
                      ),
                    ),
                  );
                },
                child: const Text('open'),
              ),
            ),
          ),
        ),
      );
      await tester.tap(find.text('open'));
      await tester.pumpAndSettle();
      final description = find.byKey(const ValueKey('idea-description'));
      await tester.enterText(description, '开头选中结尾');
      final field = tester.widget<TextField>(description);
      field.controller!.selection = const TextSelection(
        baseOffset: 2,
        extentOffset: 4,
      );
      final editable = find.descendant(
        of: description,
        matching: find.byType(EditableText),
      );
      Actions.invoke(
        tester.element(editable),
        const PasteTextIntent(SelectionChangedCause.keyboard),
      );
      await tester.pumpAndSettle();
      expect(field.controller!.text, '开头替换文本结尾');
      expect(find.text('scene.blend'), findsNWidgets(2));
      expect(find.text('photo.png'), findsOneWidget);
      await tester.tap(find.text('保存灵感'));
      await tester.pumpAndSettle();
      expect(saved!.attachments.length, 2);
      expect(saved!.title, 'scene.blend');
      expect(saved!.description, '开头替换文本结尾');
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets('Paste respects the experiment field and preserves other text', (
    tester,
  ) async {
    tester.view.physicalSize = const Size(1100, 1000);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: NewIdeaDialog(
            initialCategory: '实验',
            readClipboard: () async => const PastedContent(text: '观察结果'),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();
    final description = find.byKey(const ValueKey('idea-description'));
    await tester.enterText(description, '保留原文');
    for (final key in ['experiment-hypothesis', 'experiment-conclusion']) {
      final field = find.byKey(ValueKey(key));
      await tester.ensureVisible(field);
      await tester.enterText(field, '');
      Actions.invoke(
        tester.element(
          find.descendant(of: field, matching: find.byType(EditableText)),
        ),
        const PasteTextIntent(SelectionChangedCause.keyboard),
      );
      await tester.pumpAndSettle();
      expect(tester.widget<TextField>(field).controller!.text, '观察结果');
    }
    expect(tester.widget<TextField>(description).controller!.text, '保留原文');
    expect(tester.takeException(), isNull);
  });

  testWidgets(
    'Pages have distinct tools, project promotion and progress persist',
    (tester) async {
      tester.view.physicalSize = const Size(1440, 1000);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      final storage = MemoryStorage();
      await tester.pumpWidget(MorrowApp(storage: storage));
      await tester.pumpAndSettle();
      await tester.tap(find.text('灵感收件箱'));
      await tester.pumpAndSettle();
      expect(find.byKey(const ValueKey('inbox-list')), findsOneWidget);
      final promote = find.text('转为项目').first;
      await tester.ensureVisible(promote);
      await tester.tap(promote);
      await tester.pumpAndSettle();
      await tester.tap(find.text('小项目').first);
      await tester.pumpAndSettle();
      expect(find.byKey(const ValueKey('project-board')), findsOneWidget);
      expect(find.text('给灵感一个容器'), findsOneWidget);
      final task = find.text('整理第一批收藏');
      await tester.ensureVisible(task);
      await tester.tap(task);
      await tester.pumpAndSettle();
      expect(
        (storage.data!['ideas'] as List).any(
          (i) => (i['completed'] as List).contains('整理第一批收藏'),
        ),
        true,
      );
      await tester.tap(find.text('实验室').first);
      await tester.pumpAndSettle();
      expect(find.byKey(const ValueKey('experiment-journal')), findsOneWidget);
      expect(find.text('假设 / 想试什么'), findsWidgets);
      await tester.tap(find.text('已收藏').first);
      await tester.pumpAndSettle();
      expect(find.byKey(const ValueKey('favorites-library')), findsOneWidget);
      expect(find.text('音视频'), findsOneWidget);
      expect(tester.takeException(), isNull);
      await tester.pumpWidget(const SizedBox());
      await tester.pumpWidget(MorrowApp(storage: storage));
      await tester.pumpAndSettle();
      await tester.tap(find.text('小项目').first);
      await tester.pumpAndSettle();
      expect(find.text('给灵感一个容器'), findsOneWidget);
      for (final page in ['灵感收件箱', '小项目', '实验室', '已收藏']) {
        await tester.tap(find.text(page).first);
        await tester.pumpAndSettle();
        tester.view.physicalSize = const Size(390, 844);
        await tester.pumpAndSettle();
        expect(tester.takeException(), isNull, reason: '$page narrow layout');
        tester.view.physicalSize = const Size(1440, 1000);
        await tester.pumpAndSettle();
      }
      await tester.pumpWidget(const SizedBox());
    },
  );
}
