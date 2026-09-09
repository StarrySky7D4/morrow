import 'dart:async';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:daemon_studio/little_tips.dart';
import 'package:daemon_studio/main.dart';
import 'package:daemon_studio/media/texture_source.dart';
import 'package:daemon_studio/music/music_controller.dart';
import 'package:daemon_studio/music/music_panel.dart';

class FakeAudio implements MusicTransport {
  final playingEvents = StreamController<bool>.broadcast(sync: true);
  final completedEvents = StreamController<bool>.broadcast(sync: true);
  final positionEvents = StreamController<Duration>.broadcast(sync: true);
  final durationEvents = StreamController<Duration>.broadcast(sync: true);
  final errorEvents = StreamController<String>.broadcast(sync: true);
  String? uri;
  int playCount = 0;
  @override
  Stream<bool> get playing => playingEvents.stream;
  @override
  Stream<bool> get completed => completedEvents.stream;
  @override
  Stream<Duration> get position => positionEvents.stream;
  @override
  Stream<Duration> get duration => durationEvents.stream;
  @override
  Stream<String> get errors => errorEvents.stream;
  @override
  Future<void> open(String uri) async {
    this.uri = uri;
    durationEvents.add(const Duration(seconds: 10));
  }

  @override
  Future<void> play() async {
    playCount++;
    playingEvents.add(true);
  }

  @override
  Future<void> pause() async {
    playingEvents.add(false);
  }

  @override
  Future<void> stop() async {
    playingEvents.add(false);
  }

  @override
  Future<void> seek(Duration value) async {
    positionEvents.add(value);
  }

  @override
  Future<void> dispose() async {
    await Future.wait([
      playingEvents.close(),
      completedEvents.close(),
      positionEvents.close(),
      durationEvents.close(),
      errorEvents.close(),
    ]);
  }
}

MusicTrack track(String name, {String lyrics = ''}) => MusicTrack(
  source: TextureSource(
    location: name,
    name: '$name.wav',
    kind: TextureKind.audio,
  ),
  lyrics: lyrics,
);
void main() {
  test(
    'LRC supports fractions, repeated timestamps, offset and empty interludes',
    () {
      final lyrics = parseLyrics(
        '[offset:100]\n[00:01.2][00:04.230]first\n[00:02.00]\n[ar:artist]',
      );
      expect(lyrics.map((line) => line.time.inMilliseconds), [
        1100,
        1900,
        4130,
      ]);
      expect(lyrics.map((line) => line.text), ['first', '', 'first']);
    },
  );
  test(
    'Playlist wraps, skips automatically, seeks lyrics and releases media',
    () async {
      final audio = FakeAudio();
      var releases = 0;
      final music = MusicController(
        tracks: [
          track('a', lyrics: '[00:01]第一行\n[00:03]第二行'),
          track('b'),
        ],
        createTransport: () => audio,
        resolve: (source) async =>
            ResolvedTexture(uri: source.location, release: () => releases++),
      );
      await music.toggle();
      expect(music.playing, isTrue);
      expect(audio.uri, 'a');
      await music.seek(const Duration(seconds: 3));
      expect(music.lyric, '第二行');
      await music.previous();
      expect(music.index, 1);
      expect(audio.uri, 'b');
      audio.completedEvents.add(true);
      await Future<void>.delayed(Duration.zero);
      expect(music.index, 0);
      expect(audio.uri, 'a');
      expect(releases, 2);
      await music.remove(0);
      expect(music.current!.title, 'b');
      await music.remove(0);
      expect(music.current, isNull);
      expect(music.playing, isFalse);
      music.dispose();
    },
  );
  test(
    'Background sound cancels playback requested while song is loading',
    () async {
      final audio = FakeAudio(), loaded = Completer<ResolvedTexture>();
      final music = MusicController(
        tracks: [track('a')],
        createTransport: () => audio,
        resolve: (_) => loaded.future,
      );
      final open = music.toggle();
      await Future<void>.delayed(Duration.zero);
      final pause = music.setBlocked(true);
      loaded.complete(ResolvedTexture(uri: 'a'));
      await Future.wait([open, pause]);
      expect(audio.playCount, 0);
      expect(music.playing, isFalse);
      await music.setBlocked(false);
      expect(audio.playCount, 0);
      await music.toggle();
      expect(music.playing, isTrue);
      music.dispose();
    },
  );
  test(
    'Rapid next presses discard obsolete loads; failures remain recoverable',
    () async {
      final audio = FakeAudio();
      final loaded = Completer<ResolvedTexture>();
      var released = false;
      final music = MusicController(
        tracks: [track('a'), track('b'), track('c')],
        createTransport: () => audio,
        resolve: (source) async {
          if (source.location == 'a') return loaded.future;
          if (source.location == 'b') throw StateError('missing');
          return ResolvedTexture(uri: source.location);
        },
      );
      final first = music.toggle();
      await Future<void>.delayed(Duration.zero);
      final second = music.next();
      final third = music.next();
      loaded.complete(
        ResolvedTexture(uri: 'a', release: () => released = true),
      );
      await Future.wait([first, second, third]);
      expect(audio.uri, 'c');
      expect(released, isTrue);
      await music.select(1);
      expect(music.error, isNotNull);
      expect(music.playing, isFalse);
      await music.next();
      expect(music.error, isNull);
      expect(music.playing, isTrue);
      music.dispose();
    },
  );
  testWidgets(
    'Player starts collapsed and footer switches between synced lyrics and tips',
    (tester) async {
      final audio = FakeAudio();
      final music = MusicController(
        tracks: [track('a', lyrics: '[00:01]这是一行测试歌词')],
        createTransport: () => audio,
      );
      await tester.pumpWidget(
        MaterialApp(
          home: AppearanceScope(
            palette: const Palette(StudioTheme.white, GlassMode.frosted),
            child: Scaffold(
              body: Column(
                children: [
                  SizedBox(width: 252, child: MusicPanel(controller: music)),
                  MusicFooter(music: music),
                ],
              ),
            ),
          ),
        ),
      );
      expect(find.byKey(const ValueKey('music-playlist')), findsNothing);
      await tester.tap(find.byKey(const ValueKey('music-list-toggle')));
      await tester.pumpAndSettle();
      expect(find.byKey(const ValueKey('music-playlist')), findsOneWidget);
      await music.toggle();
      music.setShowLyrics(true);
      await music.seek(const Duration(seconds: 2));
      await tester.pumpAndSettle();
      expect(find.text('这是一行测试歌词'), findsOneWidget);
      await tester.tap(find.byKey(const ValueKey('footer-lyrics-toggle')));
      await tester.pumpAndSettle();
      expect(find.text(footerTips.first), findsOneWidget);
      await tester.pumpWidget(const SizedBox());
      music.dispose();
    },
  );
  testWidgets('Tips fade to new content and honor reduced motion', (
    tester,
  ) async {
    Widget host(bool reduced) => MaterialApp(
      home: MediaQuery(
        data: MediaQueryData(disableAnimations: reduced),
        child: const RotatingTip(lines: ['first', 'second']),
      ),
    );
    final semantics = tester.ensureSemantics();
    await tester.pumpWidget(host(false));
    final initialNode = tester.getSemantics(find.bySemanticsLabel('first')).id;
    await tester.pump(const Duration(seconds: 14));
    await tester.pump(const Duration(milliseconds: 800));
    expect(find.text('second'), findsOneWidget);
    expect(
      tester.getSemantics(find.bySemanticsLabel('second')).id,
      initialNode,
    );
    await tester.pumpWidget(host(true));
    await tester.pump(const Duration(seconds: 28));
    expect(find.text('second'), findsOneWidget);
    semantics.dispose();
    await tester.pumpWidget(const SizedBox());
  });
}
