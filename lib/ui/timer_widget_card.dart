import 'package:flutter/services.dart';
import 'package:material_ui/material_ui.dart';

import '../appearance.dart';
import '../widgets_suite/timer_controller.dart';
import 'schedule_calendar_panel.dart';
import 'workbench_controls.dart';
import 'workbench_strings.dart';

class WorkbenchTimerCard extends StatefulWidget {
  const WorkbenchTimerCard({
    super.key,
    required this.palette,
    required this.controller,
    required this.cards,
    this.embedded = false,
    this.onFloat,
    this.onDetach,
  });

  final Palette palette;
  final TimerController controller;
  final List<ScheduleCardOption> cards;
  final bool embedded;
  final VoidCallback? onFloat;
  final VoidCallback? onDetach;

  @override
  State<WorkbenchTimerCard> createState() => _WorkbenchTimerCardState();
}

class _WorkbenchTimerCardState extends State<WorkbenchTimerCard> {
  bool _editingCustom = false;
  late final TextEditingController _minutesCtrl;
  late final TextEditingController _secondsCtrl;

  @override
  void initState() {
    super.initState();
    _minutesCtrl = TextEditingController(text: '25');
    _secondsCtrl = TextEditingController(text: '00');
  }

  @override
  void dispose() {
    _minutesCtrl.dispose();
    _secondsCtrl.dispose();
    super.dispose();
  }

  void _openCustomEditor() {
    final c = widget.controller;
    final total = c.mode == WorkbenchTimerMode.custom
        ? c.customTotalSeconds
        : (c.mode == WorkbenchTimerMode.stopwatch
              ? c.customTotalSeconds
              : c.remainingSeconds);
    _minutesCtrl.text = (total ~/ 60).toString();
    _secondsCtrl.text = (total % 60).toString().padLeft(2, '0');
    setState(() => _editingCustom = true);
  }

  void _applyCustomDuration() {
    final mins = int.tryParse(_minutesCtrl.text.trim()) ?? 0;
    final secs = int.tryParse(_secondsCtrl.text.trim()) ?? 0;
    widget.controller.setCustomDuration(
      minutes: mins <= 0 && secs <= 0 ? 1 : mins,
      seconds: secs,
    );
    setState(() => _editingCustom = false);
  }

  Widget _iconBtn({
    required Key key,
    required IconData icon,
    required String tooltip,
    required Color color,
    required VoidCallback onTap,
    double size = 28,
    double iconSize = 15,
  }) => WorkbenchMiniIconButton(
    key: key,
    palette: widget.palette,
    icon: icon,
    tooltip: tooltip,
    color: color,
    onTap: onTap,
    size: size,
    iconSize: iconSize,
  );

  @override
  Widget build(BuildContext context) {
    final p = widget.palette;
    final controller = widget.controller;
    final cards = widget.cards;
    final s = WorkbenchStrings.of(context);
    final selectedCard = controller.linkedCardId != null &&
            cards.any((c) => c.id == controller.linkedCardId)
        ? controller.linkedCardId
        : null;

    final body = Padding(
      padding: widget.embedded
          ? const EdgeInsets.fromLTRB(12, 8, 12, 12)
          : const EdgeInsets.fromLTRB(14, 14, 14, 12),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          if (!widget.embedded)
            Row(
              children: [
                Icon(Icons.timer_outlined, size: 16, color: p.accent),
                const SizedBox(width: 7),
                Expanded(
                  child: Row(
                    children: [
                      Flexible(
                        child: Text(
                          s.timerTitle,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: TextStyle(
                            fontSize: 13,
                            fontWeight: FontWeight.w600,
                            color: p.ink,
                          ),
                        ),
                      ),
                      const SizedBox(width: 6),
                      Container(
                        padding: const EdgeInsets.symmetric(
                          horizontal: 6,
                          vertical: 2,
                        ),
                        decoration: BoxDecoration(
                          color: p.accent.withValues(alpha: 0.12),
                          borderRadius: p.borderRadius(7),
                        ),
                        child: Text(
                          s.timerTodayStats(
                            controller.completedSessionsToday,
                            controller.focusMinutesToday,
                          ),
                          style: TextStyle(
                            fontSize: 10.5,
                            fontWeight: FontWeight.w600,
                            color: p.accent,
                          ),
                        ),
                      ),
                    ],
                  ),
                ),
                if (widget.onFloat != null)
                  _iconBtn(
                    key: const ValueKey('widget-float-timer'),
                    icon: Icons.picture_in_picture_alt_rounded,
                    tooltip: s.widgetFloatTooltip,
                    color: p.muted,
                    onTap: widget.onFloat!,
                  ),
                if (widget.onDetach != null)
                  _iconBtn(
                    key: const ValueKey('widget-detach-timer'),
                    icon: Icons.open_in_new_rounded,
                    tooltip: s.widgetDetachTooltip,
                    color: p.muted,
                    onTap: widget.onDetach!,
                  ),
              ],
            ),
          if (!widget.embedded) const SizedBox(height: 10),
          Container(
            padding: const EdgeInsets.fromLTRB(12, 12, 12, 10),
            decoration: BoxDecoration(
              color: p.surface.withValues(alpha: p.dark ? 0.25 : 0.45),
              borderRadius: p.borderRadius(14),
              border: Border.all(color: p.line),
            ),
            child: Column(
              children: [
                Row(
                  children: [
                    Expanded(
                      child: Tooltip(
                        message: s.timerCustomTooltip,
                        child: InkWell(
                          borderRadius: p.borderRadius(8),
                          onTap: controller.running ||
                                  controller.mode == WorkbenchTimerMode.stopwatch
                              ? null
                              : () {
                                  if (_editingCustom) {
                                    setState(() => _editingCustom = false);
                                  } else {
                                    _openCustomEditor();
                                  }
                                },
                          child: Align(
                            alignment: Alignment.centerLeft,
                            child: FittedBox(
                              fit: BoxFit.scaleDown,
                              alignment: Alignment.centerLeft,
                              child: Text(
                                controller.formattedTime,
                                key: const ValueKey('timer-display'),
                                style: TextStyle(
                                  fontSize: 26,
                                  fontWeight: FontWeight.w700,
                                  color: p.ink,
                                  fontFeatures: const [
                                    FontFeature.tabularFigures(),
                                  ],
                                ),
                              ),
                            ),
                          ),
                        ),
                      ),
                    ),
                    _iconBtn(
                      key: const ValueKey('timer-custom-edit'),
                      icon: _editingCustom
                          ? Icons.close_rounded
                          : Icons.tune_rounded,
                      tooltip: s.timerCustomTooltip,
                      color: _editingCustom ? p.accent : p.muted,
                      onTap: () {
                        if (_editingCustom) {
                          setState(() => _editingCustom = false);
                        } else {
                          _openCustomEditor();
                        }
                      },
                      size: 24,
                      iconSize: 13,
                    ),
                    const SizedBox(width: 4),
                    InkWell(
                      key: const ValueKey('timer-play-pause'),
                      borderRadius: p.borderRadius(10),
                      onTap: controller.toggle,
                      child: Container(
                        padding: const EdgeInsets.symmetric(
                          horizontal: 10,
                          vertical: 6,
                        ),
                        decoration: BoxDecoration(
                          color: controller.running
                              ? p.accent.withValues(alpha: 0.16)
                              : p.accent,
                          borderRadius: p.borderRadius(10),
                        ),
                        child: Row(
                          mainAxisSize: MainAxisSize.min,
                          children: [
                            Icon(
                              controller.running
                                  ? Icons.pause_rounded
                                  : Icons.play_arrow_rounded,
                              size: 16,
                              color: controller.running ? p.accent : p.onAccent,
                            ),
                            const SizedBox(width: 3),
                            Text(
                              controller.running ? s.timerPause : s.timerStart,
                              style: TextStyle(
                                fontSize: 11,
                                fontWeight: FontWeight.w600,
                                color: controller.running
                                    ? p.accent
                                    : p.onAccent,
                              ),
                            ),
                          ],
                        ),
                      ),
                    ),
                    const SizedBox(width: 2),
                    _iconBtn(
                      key: const ValueKey('timer-reset'),
                      icon: Icons.refresh_rounded,
                      tooltip: s.timerReset,
                      color: p.muted,
                      onTap: controller.reset,
                    ),
                  ],
                ),
                if (_editingCustom) ...[
                  const SizedBox(height: 8),
                  Row(
                    children: [
                      Expanded(
                        child: TextField(
                          key: const ValueKey('timer-custom-minutes'),
                          controller: _minutesCtrl,
                          keyboardType: TextInputType.number,
                          inputFormatters: [
                            FilteringTextInputFormatter.digitsOnly,
                          ],
                          textAlign: TextAlign.center,
                          style: TextStyle(fontSize: 12, color: p.ink),
                          decoration: InputDecoration(
                            isDense: true,
                            suffixText: s.timerCustomMinutesLabel,
                            suffixStyle: TextStyle(fontSize: 9, color: p.muted),
                            contentPadding: const EdgeInsets.symmetric(
                              horizontal: 6,
                              vertical: 6,
                            ),
                          ),
                          onSubmitted: (_) => _applyCustomDuration(),
                        ),
                      ),
                      Padding(
                        padding: const EdgeInsets.symmetric(horizontal: 4),
                        child: Text(
                          ':',
                          style: TextStyle(
                            fontSize: 13,
                            fontWeight: FontWeight.w700,
                            color: p.muted,
                          ),
                        ),
                      ),
                      Expanded(
                        child: TextField(
                          key: const ValueKey('timer-custom-seconds'),
                          controller: _secondsCtrl,
                          keyboardType: TextInputType.number,
                          inputFormatters: [
                            FilteringTextInputFormatter.digitsOnly,
                          ],
                          textAlign: TextAlign.center,
                          style: TextStyle(fontSize: 12, color: p.ink),
                          decoration: InputDecoration(
                            isDense: true,
                            suffixText: s.timerCustomSecondsLabel,
                            suffixStyle: TextStyle(fontSize: 9, color: p.muted),
                            contentPadding: const EdgeInsets.symmetric(
                              horizontal: 6,
                              vertical: 6,
                            ),
                          ),
                          onSubmitted: (_) => _applyCustomDuration(),
                        ),
                      ),
                      const SizedBox(width: 6),
                      InkWell(
                        key: const ValueKey('timer-custom-apply'),
                        borderRadius: p.borderRadius(8),
                        onTap: _applyCustomDuration,
                        child: Container(
                          padding: const EdgeInsets.symmetric(
                            horizontal: 10,
                            vertical: 6,
                          ),
                          decoration: BoxDecoration(
                            color: p.accent,
                            borderRadius: p.borderRadius(8),
                          ),
                          child: Text(
                            s.timerCustomApply,
                            style: TextStyle(
                              fontSize: 10,
                              fontWeight: FontWeight.w600,
                              color: p.onAccent,
                            ),
                          ),
                        ),
                      ),
                    ],
                  ),
                  const SizedBox(height: 6),
                  Wrap(
                    spacing: 4,
                    runSpacing: 4,
                    children: [
                      for (final preset in const [1, 10, 30, 45, 60, 90])
                        InkWell(
                          key: ValueKey('timer-preset-$preset'),
                          borderRadius: p.borderRadius(6),
                          onTap: () {
                            controller.setCustomDuration(minutes: preset);
                            setState(() => _editingCustom = false);
                          },
                          child: Container(
                            padding: const EdgeInsets.symmetric(
                              horizontal: 6,
                              vertical: 3,
                            ),
                            decoration: BoxDecoration(
                              borderRadius: p.borderRadius(6),
                              border: Border.all(color: p.line),
                            ),
                            child: Text(
                              '${preset}m',
                              style: TextStyle(fontSize: 9, color: p.muted),
                            ),
                          ),
                        ),
                    ],
                  ),
                ],
                const SizedBox(height: 8),
                ClipRRect(
                  borderRadius: p.borderRadius(3),
                  child: LinearProgressIndicator(
                    value: controller.progress,
                    minHeight: 4,
                    backgroundColor: p.line,
                    color: p.accent,
                  ),
                ),
              ],
            ),
          ),
          const SizedBox(height: 10),
          Wrap(
            spacing: 6,
            runSpacing: 6,
            children: [
              for (final mode in const [
                WorkbenchTimerMode.focus,
                WorkbenchTimerMode.shortBreak,
                WorkbenchTimerMode.longBreak,
                WorkbenchTimerMode.custom,
                WorkbenchTimerMode.stopwatch,
              ])
                InkWell(
                  key: ValueKey('timer-mode-${mode.name}'),
                  borderRadius: p.borderRadius(8),
                  onTap: () {
                    controller.selectMode(mode);
                    if (mode == WorkbenchTimerMode.custom) {
                      _openCustomEditor();
                    } else if (_editingCustom) {
                      setState(() => _editingCustom = false);
                    }
                  },
                  child: Container(
                    padding: const EdgeInsets.symmetric(
                      horizontal: 8,
                      vertical: 4,
                    ),
                    decoration: BoxDecoration(
                      color: controller.mode == mode
                          ? p.accent.withValues(alpha: 0.15)
                          : Colors.transparent,
                      borderRadius: p.borderRadius(8),
                      border: Border.all(
                        color: controller.mode == mode
                            ? p.accent.withValues(alpha: 0.45)
                            : p.line,
                      ),
                    ),
                    child: Text(
                      s.timerModeLabel(mode),
                      style: TextStyle(
                        fontSize: 11,
                        fontWeight: controller.mode == mode
                            ? FontWeight.w600
                            : FontWeight.w400,
                        color: controller.mode == mode ? p.accent : p.muted,
                      ),
                    ),
                  ),
                ),
              if (controller.mode != WorkbenchTimerMode.stopwatch) ...[
                InkWell(
                  key: const ValueKey('timer-minus-5'),
                  borderRadius: p.borderRadius(8),
                  onTap: () => controller.adjustMinutes(-5),
                  child: Container(
                    padding: const EdgeInsets.symmetric(
                      horizontal: 7,
                      vertical: 4,
                    ),
                    decoration: BoxDecoration(
                      borderRadius: p.borderRadius(8),
                      border: Border.all(color: p.line),
                    ),
                    child: Text(
                      '-5m',
                      style: TextStyle(fontSize: 11, color: p.muted),
                    ),
                  ),
                ),
                InkWell(
                  key: const ValueKey('timer-plus-5'),
                  borderRadius: p.borderRadius(8),
                  onTap: () => controller.adjustMinutes(5),
                  child: Container(
                    padding: const EdgeInsets.symmetric(
                      horizontal: 7,
                      vertical: 4,
                    ),
                    decoration: BoxDecoration(
                      borderRadius: p.borderRadius(8),
                      border: Border.all(color: p.line),
                    ),
                    child: Text(
                      '+5m',
                      style: TextStyle(fontSize: 11, color: p.muted),
                    ),
                  ),
                ),
              ],
            ],
          ),
          const SizedBox(height: 10),
          DropdownButtonFormField<String?>(
            key: const ValueKey('timer-card-picker'),
            initialValue: selectedCard,
            isExpanded: true,
            style: TextStyle(fontSize: 11, color: p.ink),
            decoration: InputDecoration(
              isDense: true,
              labelText: s.timerLinkCard,
              labelStyle: TextStyle(fontSize: 10, color: p.muted),
              contentPadding: const EdgeInsets.symmetric(
                horizontal: 10,
                vertical: 8,
              ),
            ),
            items: [
              DropdownMenuItem<String?>(
                value: null,
                child: Text(
                  s.timerNoCard,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                ),
              ),
              for (final card in cards)
                DropdownMenuItem<String?>(
                  value: card.id,
                  child: Row(
                    children: [
                      Icon(card.icon, size: 13, color: card.color),
                      const SizedBox(width: 6),
                      Expanded(
                        child: Text(
                          '${card.title}${controller.focusMinutesForCard(card.id) > 0 ? ' (${controller.focusMinutesForCard(card.id)}m)' : ''}',
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                        ),
                      ),
                    ],
                  ),
                ),
            ],
            onChanged: (value) {
              if (value == null) {
                controller.bindCard(null);
              } else {
                final card = cards.firstWhere((c) => c.id == value);
                controller.bindCard(card.id, label: card.title);
              }
            },
          ),
        ],
      ),
    );

    if (widget.embedded) return body;
    return Glass(componentId: 'timer', p: p, radius: 20, child: body);
  }
}
