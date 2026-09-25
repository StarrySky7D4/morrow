import 'dart:io';
import 'dart:typed_data';
import 'dart:ui' as ui;
import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import '../main.dart' show Idea;
import '../music/music_controller.dart';
import '../media/texture_source.dart';
import 'workbench_backend.dart';
import 'workbench_native.dart';
import 'studio_storage.dart';

/// Explicit qualification mode uses a fresh, caller-selected data directory.
Future<void> seedQualification(RustWorkbench host, Directory directory) async {
  for (final (id, title, category, body) in [
    (
      'demo-inbox',
      '把零散想法留在这里',
      '灵感',
      '# 今日灵感\n\n**随时记录**，再把想法整理成下一步。\n\n- Markdown 正文\n- 原始附件随卡片保存',
    ),
    ('demo-project', '制作一份自己的灵感地图', '进行中', '收集、整理、动手做。每一步都值得留下。'),
    ('demo-lab', '试试不同的玻璃与色彩', '实验', '保留假设和观察，让试验可以继续。'),
  ]) {
    var idea = await host.apply(
      PluginAction.create,
      Idea(
        title,
        body,
        category,
        Idea.icons[0],
        const Color(0xff987acb),
        id: id,
        todos: category == '进行中' ? ['收集线索', '整理素材', '完成第一版'] : [],
        hypothesis: category == '实验' ? '更清晰的层次是否更容易阅读？' : '',
        conclusion: category == '实验' ? '用真实内容观察，不只比较空白面板。' : '',
      ),
    );
    if (category == '进行中') {
      idea = await host.apply(
        PluginAction.todo,
        idea,
        text: '收集线索',
        flag: true,
      );
    }
    if (category == '灵感') {
      await host.apply(PluginAction.favorite, idea, flag: true);
    }
  }
  // A silent PCM fixture proves the actual bundled decoder and playback clock
  // without generating an unexpected audible tone on the user's desktop.
  final wave = ByteData(44 + 48000 * 2 * 2);
  void ascii(int offset, String text) {
    for (var i = 0; i < text.length; i++) {
      wave.setUint8(offset + i, text.codeUnitAt(i));
    }
  }

  ascii(0, 'RIFF');
  wave.setUint32(4, wave.lengthInBytes - 8, Endian.little);
  ascii(8, 'WAVEfmt ');
  wave.setUint32(16, 16, Endian.little);
  wave.setUint16(20, 1, Endian.little);
  wave.setUint16(22, 1, Endian.little);
  wave.setUint32(24, 48000, Endian.little);
  wave.setUint32(28, 96000, Endian.little);
  wave.setUint16(32, 2, Endian.little);
  wave.setUint16(34, 16, Endian.little);
  ascii(36, 'data');
  wave.setUint32(40, wave.lengthInBytes - 44, Endian.little);
  final file = File('${directory.path}/qualification.wav');
  await file.writeAsBytes(wave.buffer.asUint8List());
  final storage = await RustStudioStorage.open(host);
  await storage.write({
    ...storage.read(),
    'theme': 'custom',
    'themeColor': 0xff846ab5,
    'glass': 'clear',
    'background': 'ambient',
    'liquidCanvas': true,
    'completed': ['喝水'],
    'music': {
      'index': 0,
      'showLyrics': true,
      'onlineLyrics': false,
      'tracks': [
        MusicTrack(
          source: TextureSource(
            location: file.path,
            name: '静音播放验证.wav',
            kind: TextureKind.audio,
            local: true,
          ),
          trackTitle: '留给下一次灵感',
          metadataRead: true,
          lyrics: '[00:00.00]留住此刻\n[00:01.00]继续向前',
        ).toJson(),
      ],
    },
  });
}

Future<void> finishQualification(
  GlobalKey boundary,
  RustWorkbench host,
  RustStudioStorage storage,
  String output,
  List<String> errors,
) async {
  final notes = <String>[];
  try {
    const channel = MethodChannel('morrow/window_shape');
    for (final blur in [0.0, 1.0, 12.0, 40.0, 0.0]) {
      await channel.invokeMethod<void>('setCanvasBlur', blur);
      await Future<void>.delayed(const Duration(milliseconds: 320));
    }
    notes.add(
      'PASS: native desktop composition initialized, accepted blur 0/1/12/40 and disabled cleanly; this is an API check, not a desktop pixel comparison.',
    );
    final track = MusicTrack.fromJson(
      (storage.read()['music']['tracks'] as List).first as Map<String, dynamic>,
    );
    final player = MusicController(tracks: [track], plugin: host.studio);
    try {
      if (player.playing) throw StateError('Unexpected playback on restore');
      await player.select(0);
      // Decoder startup and the first position event vary by audio device.
      // Wait for evidence with a deadline instead of assuming 500 ms is enough.
      final playbackDeadline = Stopwatch()..start();
      while (player.error == null &&
          (player.position == Duration.zero ||
              player.duration == Duration.zero) &&
          playbackDeadline.elapsed < const Duration(seconds: 5)) {
        await Future<void>.delayed(const Duration(milliseconds: 100));
      }
      if (player.error != null ||
          player.position.inMilliseconds <= 0 ||
          player.duration.inMilliseconds <= 0) {
        throw StateError(
          'Native playback clock: ${player.error}, ${player.position}, ${player.duration}',
        );
      }
      notes.add(
        'PASS: bundled Windows audio decoder opened the silent WAV and advanced its playback clock.',
      );
      await player.seek(const Duration(milliseconds: 1000));
      await Future<void>.delayed(const Duration(milliseconds: 150));
      if (player.position.inMilliseconds < 900) {
        throw StateError('Seek did not advance');
      }
      await player.setBlocked(true);
      await Future<void>.delayed(const Duration(milliseconds: 100));
      if (player.playing) {
        throw StateError('Music continued after background audio block');
      }
      notes.add('PASS: seek, playback exclusion and no autoplay on restore.');
    } finally {
      player.dispose();
    }
    await Future<void>.delayed(const Duration(milliseconds: 700));
    await WidgetsBinding.instance.endOfFrame;
    final render =
        boundary.currentContext!.findRenderObject()! as RenderRepaintBoundary;
    final image = await render.toImage(pixelRatio: 1);
    final png = await image.toByteData(format: ui.ImageByteFormat.png);
    image.dispose();
    await File('$output.png').writeAsBytes(png!.buffer.asUint8List());
    if (errors.isNotEmpty) throw StateError(errors.join('\n'));
    notes.add(
      'PASS: actual Windows Release application rendered the Rust-backed workspace without Flutter errors.',
    );
    await File(
      '$output.md',
    ).writeAsString('# Windows qualification\n\n${notes.join('\n\n')}\n');
    await host.close();
    exit(0);
  } catch (e, stack) {
    await File('$output.md').writeAsString(
      '# Windows qualification failed\n\n$e\n\n$stack\n\n${errors.join('\n')}\n',
    );
    await host.close();
    exit(1);
  }
}
