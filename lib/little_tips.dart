import 'package:morrow_i18n/morrow_i18n.dart';
import 'dart:async';
import 'package:material_ui/material_ui.dart';
import 'appearance.dart';
import 'schedule/schedule_store.dart';
import 'tip_preferences.dart';
import 'music/music_controller.dart';
import 'ui/workbench_strings.dart';
import 'widgets_suite/countdown_store.dart';
import 'widgets_suite/footer_reminder_controller.dart';
import 'widgets_suite/timer_controller.dart';

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

List<String> localizedDailyTips(BuildContext context) {
  final l = L10n.of(context);
  return [l.mainDailyWater, l.mainDailyIdea, l.mainDailyExplore];
}

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
  void didUpdateWidget(covariant RotatingTip oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.lines.join('\n') != widget.lines.join('\n')) index = 0;
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
  const FooterOverlay({
    super.key,
    required this.music,
    this.timer,
    this.schedule,
    this.countdown,
    this.reminders,
    this.onTapTimer,
    this.onTapSchedule,
    this.onTapCard,
  });
  final MusicController music;
  final TimerController? timer;
  final SchedulePreferences? schedule;
  final CountdownPreferences? countdown;
  final FooterReminderController? reminders;
  final VoidCallback? onTapTimer;
  final ValueChanged<DateTime>? onTapSchedule;
  final ValueChanged<String>? onTapCard;

  @override
  Widget build(BuildContext context) {
    final p = AppearanceScope.of(context);
    final customFooter = p.surfaces.resolveComponent('footer')?.enabled == true;
    final footerRadius = p.componentBorderRadius(22, componentId: 'footer');
    final content = Padding(
      padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 3),
      child: MusicFooter(
        music: music,
        timer: timer,
        schedule: schedule,
        countdown: countdown,
        reminders: reminders,
        onTapTimer: onTapTimer,
        onTapSchedule: onTapSchedule,
        onTapCard: onTapCard,
      ),
    );
    return Center(
      child: Padding(
        key: const ValueKey('footer-dock'),
        padding: const EdgeInsets.only(top: 8),
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: 640),
          child: customFooter
              ? Glass(
                  componentId: 'footer',
                  p: p,
                  radius: 22,
                  useBackdropGroup: false,
                  child: content,
                )
              : AnimatedContainer(
                  duration: motionDuration(context, 280),
                  curve: Curves.easeOutCubic,
                  decoration: BoxDecoration(
                    color: p.surface.withValues(alpha: p.dark ? 0.68 : 0.84),
                    borderRadius: footerRadius,
                    border: Border.all(color: p.line),
                    boxShadow: [
                      BoxShadow(
                        color: Colors.black.withValues(
                          alpha: p.dark ? 0.34 : 0.11,
                        ),
                        blurRadius: 20,
                        offset: const Offset(0, 8),
                      ),
                      BoxShadow(
                        color: Colors.black.withValues(
                          alpha: p.dark ? 0.18 : 0.05,
                        ),
                        blurRadius: 5,
                        offset: const Offset(0, 2),
                      ),
                    ],
                  ),
                  child: content,
                ),
        ),
      ),
    );
  }
}

class MusicFooter extends StatelessWidget {
  const MusicFooter({
    super.key,
    required this.music,
    this.timer,
    this.schedule,
    this.countdown,
    this.reminders,
    this.onTapTimer,
    this.onTapSchedule,
    this.onTapCard,
  });
  final MusicController music;
  final TimerController? timer;
  final SchedulePreferences? schedule;
  final CountdownPreferences? countdown;
  final FooterReminderController? reminders;
  final VoidCallback? onTapTimer;
  final ValueChanged<DateTime>? onTapSchedule;
  final ValueChanged<String>? onTapCard;

  @override
  Widget build(BuildContext context) {
    final p = AppearanceScope.of(context);
    final s = WorkbenchStrings.of(context);
    final listenables = <Listenable>[
      music,
      ?timer,
      ?schedule,
      ?countdown,
      ?reminders,
    ];
    return ListenableBuilder(
      listenable: Listenable.merge(listenables),
      builder: (context, _) {
        final now = DateTime.now();
        final alert = reminders?.activeAlert;
        final todayEntries = (schedule?.entriesForDate(now) ?? const <ScheduleEntry>[])
            .where((e) => !e.completed)
            .toList(growable: false);
        final primarySchedule = todayEntries.isEmpty ? null : todayEntries.first;
        final primaryCountdown = countdown?.primaryForFooter(now);
        final showTimerPill =
            timer != null && (timer!.running || timer!.elapsedSeconds > 0);

        final baseLines =
            TipPreferencesScope.maybeOf(
              context,
            )?.lines('footer', localizedFooterTips(context)) ??
            localizedFooterTips(context);
        final dynamicLines = <String>[
          if (primarySchedule != null)
            s.footerTodayScheduleLine(
              primarySchedule.isAllDay
                  ? primarySchedule.title
                  : '${primarySchedule.timeText} ${primarySchedule.title}',
              todayEntries.length,
            ),
          if (primaryCountdown != null)
            s.countdownFooterLine(primaryCountdown, now),
          ...baseLines,
        ];

        final isLyricOverride = music.playing && music.showLyrics;
        final textOverride = alert != null
            ? '${alert.badge} · ${alert.message}'
            : isLyricOverride
            ? (music.lyricForDisplay(
                    untimedLabel: L10n.of(context).visualNoTimeline,
                  ) ??
                  L10n.of(
                    context,
                  ).visualNoLyricsTitle(music.current?.title ?? ''))
            : null;

        VoidCallback? textTapAction;
        if (alert != null) {
          if (alert.linkedCardId != null && onTapCard != null) {
            textTapAction = () => onTapCard!(alert.linkedCardId!);
          } else if (alert.scheduleDate != null && onTapSchedule != null) {
            textTapAction = () => onTapSchedule!(alert.scheduleDate!);
          }
        } else if (!isLyricOverride && primarySchedule != null && onTapSchedule != null) {
          textTapAction = () => onTapSchedule!(primarySchedule.date);
        }

        return Row(
          children: [
            if (music.current != null)
              IconButton(
                visualDensity: VisualDensity.compact,
                constraints: const BoxConstraints(minWidth: 28, minHeight: 28),
                padding: EdgeInsets.zero,
                onPressed: music.toggle,
                icon: Icon(
                  music.playing
                      ? Icons.pause_circle_outline_rounded
                      : Icons.play_circle_outline_rounded,
                  size: 18,
                  color: p.accent,
                ),
              )
            else ...[
              Icon(
                music.playing ? Icons.music_note_rounded : Icons.circle,
                size: music.playing ? 15 : 6,
                color: p.accent,
              ),
              const SizedBox(width: 8),
            ],
            if (showTimerPill) ...[
              const SizedBox(width: 4),
              InkWell(
                key: const ValueKey('footer-timer-pill'),
                borderRadius: p.borderRadius(8),
                onTap: onTapTimer,
                child: Container(
                  padding: const EdgeInsets.symmetric(
                    horizontal: 7,
                    vertical: 3,
                  ),
                  decoration: BoxDecoration(
                    color: p.accent.withValues(alpha: 0.14),
                    borderRadius: p.borderRadius(8),
                  ),
                  child: Row(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      Icon(
                        timer!.running
                            ? Icons.timer_rounded
                            : Icons.timer_outlined,
                        size: 12,
                        color: p.accent,
                      ),
                      const SizedBox(width: 4),
                      Text(
                        timer!.formattedTime,
                        style: TextStyle(
                          fontSize: 10,
                          fontWeight: FontWeight.w600,
                          color: p.accent,
                          fontFeatures: const [FontFeature.tabularFigures()],
                        ),
                      ),
                    ],
                  ),
                ),
              ),
            ],
            const SizedBox(width: 6),
            Expanded(
              child: GestureDetector(
                behavior: HitTestBehavior.translucent,
                onTap: textTapAction,
                child: Container(
                  key: alert != null ? const ValueKey('footer-alert-banner') : null,
                  padding: alert != null
                      ? const EdgeInsets.symmetric(horizontal: 6, vertical: 2)
                      : EdgeInsets.zero,
                  decoration: alert != null
                      ? BoxDecoration(
                          color: p.accent.withValues(alpha: 0.12),
                          borderRadius: p.borderRadius(7),
                        )
                      : null,
                  child: RotatingTip(
                    key: const ValueKey('footer-tips'),
                    lines: dynamicLines,
                    textOverride: textOverride,
                    style: TextStyle(
                      color: alert != null ? p.ink : p.muted,
                      fontWeight: alert != null ? FontWeight.w600 : FontWeight.w400,
                      fontSize: 11,
                      height: 1.6,
                    ),
                  ),
                ),
              ),
            ),
            if (alert != null)
              IconButton(
                key: const ValueKey('footer-alert-dismiss'),
                visualDensity: VisualDensity.compact,
                constraints: const BoxConstraints(minWidth: 24, minHeight: 24),
                padding: EdgeInsets.zero,
                tooltip: L10n.of(context).visualCloseDialog,
                onPressed: reminders?.dismiss,
                icon: Icon(Icons.close_rounded, size: 14, color: p.muted),
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
        );
      },
    );
  }
}
