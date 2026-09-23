import 'package:morrow_i18n/morrow_i18n.dart';
import 'dart:async';
import 'package:flutter/material.dart';
import 'appearance.dart';
import 'music/music_controller.dart';

// Compatibility default for callers without a locale; visible UI uses the context helper.
List<String> get cornerTips => _cornerTips(L10n.forLocale(const Locale('zh')));
// Compatibility default for callers without a locale; visible UI uses the context helper.
List<String> get footerTips => _footerTips(L10n.forLocale(const Locale('zh')));

List<String> localizedCornerTips(BuildContext context) =>
    _cornerTips(L10n.of(context));

List<String> _cornerTips(AppLocalizations labels) => [
  labels.visualCornerTips1,
  labels.visualCornerTips2,
  labels.visualCornerTips3,
  labels.visualCornerTips4,
  labels.visualCornerTips5,
  labels.visualCornerTips6,
  labels.visualCornerTips7,
  labels.visualCornerTips8,
];

List<String> localizedFooterTips(BuildContext context) =>
    _footerTips(L10n.of(context));

List<String> _footerTips(AppLocalizations labels) => [
  labels.visualFooterTips1,
  labels.visualFooterTips2,
  labels.visualFooterTips3,
  labels.visualFooterTips4,
  labels.visualFooterTips5,
  labels.visualFooterTips6,
  labels.visualFooterTips7,
  labels.visualFooterTips8,
  labels.visualFooterTips9,
  labels.visualFooterTips10,
];

class RotatingTip extends StatefulWidget {
  const RotatingTip({
    super.key,
    required this.lines,
    this.textOverride,
    this.style,
    this.interval = const Duration(seconds: 14),
  });
  final List<String> lines;
  final String? textOverride;
  final TextStyle? style;
  final Duration interval;
  @override
  State<RotatingTip> createState() => _RotatingTipState();
}

class _RotatingTipState extends State<RotatingTip> {
  int index = 0;
  Timer? timer;
  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    timer?.cancel();
    if (!MediaQuery.disableAnimationsOf(context)) {
      timer = Timer.periodic(widget.interval, (_) {
        if (mounted &&
            TickerMode.valuesOf(context).enabled &&
            widget.textOverride == null) {
          setState(() => index = (index + 1) % widget.lines.length);
        }
      });
    }
  }

  @override
  void dispose() {
    timer?.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final text =
        widget.textOverride ?? widget.lines[index % widget.lines.length];
    // Keep one accessibility node while the visual text crossfades. Native
    // Windows readers may query a child as its outgoing animation is removed.
    return Semantics(
      container: true,
      label: text,
      child: ExcludeSemantics(
        child: AnimatedSwitcher(
          duration: motionDuration(context, 700),
          layoutBuilder: (current, previous) => Stack(
            alignment: Alignment.centerLeft,
            children: [
              ...previous.map((w) => ExcludeSemantics(child: w)),
              ?current,
            ],
          ),
          child: Text(
            text,
            key: ValueKey(text),
            maxLines: 3,
            overflow: TextOverflow.ellipsis,
            style: widget.style,
          ),
        ),
      ),
    );
  }
}

/// Tips float over the canvas. Material is opt-in using the existing per-
/// component editor; the default has no blur, fill, shadow or border.
class FooterOverlay extends StatelessWidget {
  const FooterOverlay({super.key, required this.music});
  final MusicController music;

  @override
  Widget build(BuildContext context) {
    final p = AppearanceScope.of(context);
    final content = Padding(
      padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 4),
      child: MusicFooter(music: music),
    );
    return Padding(
      key: const ValueKey('footer-dock'),
      padding: const EdgeInsets.only(top: 8),
      child: p.surfaces.resolveComponent('footer')?.enabled == true
          ? Glass(componentId: 'footer', p: p, radius: 14, child: content)
          : content,
    );
  }
}

class MusicFooter extends StatelessWidget {
  const MusicFooter({super.key, required this.music});
  final MusicController music;
  @override
  Widget build(BuildContext context) {
    final p = AppearanceScope.of(context);
    return ListenableBuilder(
      listenable: music,
      builder: (context, _) => Row(
        children: [
          Icon(
            music.playing ? Icons.music_note_rounded : Icons.circle,
            size: music.playing ? 15 : 6,
            color: p.accent,
          ),
          const SizedBox(width: 8),
          Expanded(
            child: RotatingTip(
              key: const ValueKey('footer-tips'),
              lines: localizedFooterTips(context),
              textOverride: music.playing && music.showLyrics
                  ? (music.lyricForDisplay(
                          untimedLabel: L10n.of(context).visualNoTimeline,
                        ) ??
                        L10n.of(
                          context,
                        ).visualNoLyricsTitle(music.current?.title ?? ''))
                  : null,
              style: TextStyle(color: p.muted, fontSize: 11, height: 1.7),
            ),
          ),
          IconButton(
            key: const ValueKey('footer-lyrics-toggle'),
            tooltip: music.showLyrics
                ? L10n.of(context).visualFooterTips
                : L10n.of(context).visualFooterLyrics,
            onPressed: music.playing
                ? () => music.setShowLyrics(!music.showLyrics)
                : null,
            icon: Icon(
              Icons.lyrics_outlined,
              size: 17,
              color: music.showLyrics ? p.accent : p.muted,
            ),
          ),
        ],
      ),
    );
  }
}
