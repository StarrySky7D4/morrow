import '../content/rich_content.dart' show RichFragment;
import 'capture_native.dart';
import 'dart:typed_data';
import 'package:capnproto_dart/capnproto_dart.dart';
import '../music/music_controller.dart' show LyricLine;
import '../music/lyrics_service.dart' show LyricMatch;
import 'studio_backend.dart';
import 'workbench_native.dart';
import 'generated/studio.capnp.dart' as wire;
import 'generated/identity.dart' as contract;

class RustStudioPlugin implements StudioBackend {
  RustStudioPlugin(this.host);
  final RustWorkbench host;
  @override
  Future<RichFragment> capture(
    String format,
    String source, {
    String imagePrefix = "clipboard",
  }) => captureWithPlugin(host, format, source, imagePrefix);
  Future<wire.ServiceResponseReader> _call(
    wire.ServiceAction action,
    void Function(wire.ServiceRequestBuilder) configure,
  ) async {
    final message = MessageBuilder();
    final r = message.initRoot(wire.serviceRequestFactory);
    r.version = 1;
    r.digest = Uint8List.fromList(contract.studioDigest);
    r.action = action;
    configure(r);
    final request = message.serialize();
    if (request.length > 65536) {
      throw const FormatException('插件消息容量不足');
    }
    final bytes = await host.service(request);
    final reply = RustWorkbench.readMessage(
      bytes,
    ).getRoot(wire.serviceResponseFactory);
    if (reply.version != 1 ||
        reply.digest == null ||
        reply.digest!.length != contract.studioDigest.length ||
        List.generate(
          contract.studioDigest.length,
          (i) => reply.digest![i] != contract.studioDigest[i],
        ).any((v) => v)) {
      throw const FormatException('插件服务版本不匹配');
    }
    return reply;
  }

  @override
  Future<List<LyricLine>> parseLyrics(String text) async {
    final r = await _call(wire.ServiceAction.lyrics, (r) => r.text = text);
    return [
      for (final line in r.lines ?? <wire.LyricLineReader>[])
        LyricLine(Duration(milliseconds: line.timeMs), line.text ?? ''),
    ];
  }

  @override
  Future<int?> matchLyrics(
    List<LyricMatch> candidates,
    String title,
    String artist,
    double duration,
  ) async {
    final r = await _call(wire.ServiceAction.lyricMatch, (r) {
      r.title = title;
      r.artist = artist;
      r.duration = duration;
      final list = r.initCandidates(candidates.length);
      for (var i = 0; i < candidates.length; i++) {
        final item = list[i];
        final candidate = candidates[i];
        item.title = candidate.title;
        item.artist = candidate.artist;
        item.duration = candidate.duration;
        item.synced = candidate.synced;
        item.hasLyrics = candidate.lyrics.trim().isNotEmpty;
      }
    });
    return r.matchIndex < 0 ? null : r.matchIndex;
  }

  @override
  Future<void> validateImport(
    String kind,
    int size, {
    String url = '',
    bool attachment = false,
  }) async {
    await _call(wire.ServiceAction.importPolicy, (r) {
      r.kind = kind;
      r.size = size;
      r.text = url;
      r.flag = attachment;
    });
  }

  @override
  Future<PlaybackDecision> playback(
    String action, {
    required int count,
    required int index,
    required bool playing,
    required bool blocked,
    int positionMs = 0,
    int durationMs = 0,
    int value = 0,
    bool flag = false,
  }) async {
    final r = await _call(wire.ServiceAction.playback, (r) {
      r.musicAction = wire.MusicAction.values.byName(action);
      r.value = value;
      r.flag = flag;
      final p = r.initPlayback();
      p.index = index;
      p.playing = playing;
      p.blocked = blocked;
      p.positionMs = positionMs;
      p.durationMs = durationMs;
      final ids = p.initIds(count);
      for (var i = 0; i < count; i++) {
        ids[i] = 'track-$i';
      }
    });
    final p = r.playback!;
    return PlaybackDecision(
      p.index,
      p.playing,
      p.positionMs,
      r.effect ?? 'none',
    );
  }
}
