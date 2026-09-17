import 'package:morrow_i18n/morrow_i18n.dart';
import 'dart:async';
import 'package:flutter/material.dart';
import 'package:media_kit/media_kit.dart';
import 'package:media_kit_video/media_kit_video.dart';
import 'texture_repository.dart';
import 'texture_source.dart';

class TextureBackdrop extends StatefulWidget {
  const TextureBackdrop({
    super.key,
    required this.source,
    required this.playing,
    required this.onError,
    this.muted = true,
    this.onAudible,
  });
  final TextureSource source;
  final bool playing;
  final ValueChanged<String?> onError;
  final bool muted;
  final ValueChanged<bool>? onAudible;
  @override
  State<TextureBackdrop> createState() => _TextureBackdropState();
}

class _TextureBackdropState extends State<TextureBackdrop>
    with WidgetsBindingObserver {
  ResolvedTexture? resolved;
  Player? player;
  VideoController? video;
  StreamSubscription<String>? errors;
  StreamSubscription<bool>? playback;
  bool lastAudible = false;
  void reportAudible() {
    final audible =
        !widget.muted &&
        !failed &&
        shouldPlay &&
        (player?.state.playing ?? false);
    if (audible == lastAudible) return;
    lastAudible = audible;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) widget.onAudible?.call(audible);
    });
  }

  bool active = true, failed = false;
  bool get shouldPlay =>
      widget.playing && active && !MediaQuery.disableAnimationsOf(context);
  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    load();
  }

  void report(String? message) {
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) widget.onError(message);
    });
  }

  void failure() {
    if (!mounted || failed) return;
    failed = true;
    reportAudible();
    report(L10n.of(context).visualTextureFailure);
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) setState(() {});
    });
  }

  Future<void> load() async {
    try {
      final result = await TextureRepository.resolve(widget.source);
      if (!mounted) {
        result.release?.call();
        return;
      }
      resolved = result;
      if (widget.source.kind == TextureKind.video) {
        MediaKit.ensureInitialized();
        final controller = Player();
        player = controller;
        video = VideoController(controller);
        errors = controller.stream.error.listen((_) => failure());
        playback = controller.stream.playing.listen((_) => reportAudible());
        await controller.setVolume(0);
        if (!mounted) return;
        await controller.setPlaylistMode(PlaylistMode.single);
        if (!mounted) return;
        await controller
            .open(Media(result.uri), play: false)
            .timeout(const Duration(seconds: 20));
        if (!mounted) return;
        await syncPlayback();
      }
      if (mounted) {
        setState(() {});
        if (!failed) report(null);
      }
    } catch (_) {
      failure();
    }
  }

  Future<void> syncPlayback() async {
    try {
      if (player == null || failed) return;
      await player!.setVolume(widget.muted ? 0 : 100);
      if (shouldPlay) {
        await player!.play();
      } else {
        await player!.pause();
      }
      reportAudible();
    } catch (_) {
      failure();
    }
  }

  @override
  void didUpdateWidget(TextureBackdrop old) {
    super.didUpdateWidget(old);
    if (old.playing != widget.playing || old.muted != widget.muted) {
      syncPlayback();
    }
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    if (failed) report(L10n.of(context).visualTextureFailure);
    syncPlayback();
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    active = state == AppLifecycleState.resumed;
    syncPlayback();
    if (mounted) setState(() {});
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    errors?.cancel();
    playback?.cancel();
    final release = resolved?.release;
    final controller = player;
    if (controller == null) {
      release?.call();
    } else {
      controller.dispose().whenComplete(() => release?.call());
    }
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    if (failed || resolved == null) return const SizedBox.expand();
    if (video != null) {
      return IgnorePointer(
        child: Video(
          controller: video!,
          fit: BoxFit.cover,
          controls: NoVideoControls,
          fill: Colors.transparent,
          wakelock: false,
          pauseUponEnteringBackgroundMode: false,
        ),
      );
    }
    Widget error(BuildContext context, Object exception, StackTrace? stack) {
      failure();
      return const SizedBox.expand();
    }

    return IgnorePointer(
      child: TickerMode(
        enabled: shouldPlay,
        child: resolved!.bytes != null
            ? Image.memory(
                resolved!.bytes!,
                fit: BoxFit.cover,
                gaplessPlayback: true,
                errorBuilder: error,
              )
            : Image.network(
                resolved!.uri,
                fit: BoxFit.cover,
                gaplessPlayback: true,
                errorBuilder: error,
              ),
      ),
    );
  }
}
