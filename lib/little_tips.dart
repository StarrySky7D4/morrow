import 'dart:async';
import 'package:flutter/material.dart';
import 'appearance.dart';
import 'music/music_controller.dart';

const cornerTips = [
  '不必每个想法都有用\n有些只是让今天更有趣。',
  '先写下来，再慢慢想\n灵感不必一次就完整。',
  '留一点空白给自己\n好奇心也需要呼吸。',
  '今天试一点新东西\n小小的偏离，也有惊喜。',
  '走神也可能有收获\n给思绪一条散步的小路。',
  '给喜欢的事一点时间\n不用急着证明它的意义。',
  '进度可以很小\n愿意开始就已经很好。',
  '偶尔抬头看看窗外\n生活也是灵感的来源。',
];
const footerTips = [
  '没有紧迫的事。给好奇心一点时间。',
  '想到什么就记一点，不用马上整理。',
  '把大的想法，拆成今天的一小步。',
  '伸个懒腰，让眼睛休息一会儿。',
  '允许一个想法暂时没有答案。',
  '有些收获，会在慢下来以后出现。',
  '收藏一个细节，也是在照顾灵感。',
  '今天的随手一记，可能是明天的开始。',
  '走一会儿神，再回到喜欢的事情。',
  '不用填满每一分钟。留一点余地。',
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
              lines: footerTips,
              textOverride: music.playing && music.showLyrics
                  ? (music.lyric ?? '♪ ${music.current?.title ?? ''} · 暂无歌词')
                  : null,
              style: TextStyle(color: p.muted, fontSize: 11, height: 1.7),
            ),
          ),
          IconButton(
            key: const ValueKey('footer-lyrics-toggle'),
            tooltip: music.showLyrics ? '底部显示提示语' : '底部显示歌词',
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
