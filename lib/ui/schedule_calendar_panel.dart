import 'package:material_ui/material_ui.dart';

import '../appearance.dart';
import '../schedule/schedule_store.dart';
import 'workbench_controls.dart';
import 'workbench_strings.dart';

/// Lightweight card metadata exposed to the schedule widget and dialog.
@immutable
class ScheduleCardOption {
  const ScheduleCardOption({
    required this.id,
    required this.title,
    required this.categoryLabel,
    required this.icon,
    required this.color,
  });

  final String id;
  final String title;
  final String categoryLabel;
  final IconData icon;
  final Color color;
}

class WorkbenchScheduleCard extends StatefulWidget {
  const WorkbenchScheduleCard({
    super.key,
    required this.palette,
    required this.preferences,
    required this.cards,
    required this.selectedDate,
    required this.onSelectDate,
    required this.onOpenCard,
    required this.onCreateEntry,
    required this.onEditEntry,
    this.embedded = false,
    this.onFloat,
    this.onDetach,
    this.onStartFocusForEntry,
    this.nowProvider,
  });

  final Palette palette;
  final SchedulePreferences preferences;
  final List<ScheduleCardOption> cards;
  final DateTime selectedDate;
  final ValueChanged<DateTime> onSelectDate;
  final ValueChanged<String> onOpenCard;
  final VoidCallback onCreateEntry;
  final ValueChanged<ScheduleEntry> onEditEntry;
  final bool embedded;
  final VoidCallback? onFloat;
  final VoidCallback? onDetach;
  final ValueChanged<ScheduleEntry>? onStartFocusForEntry;
  final DateTime Function()? nowProvider;

  @override
  State<WorkbenchScheduleCard> createState() => _WorkbenchScheduleCardState();
}

class _WorkbenchScheduleCardState extends State<WorkbenchScheduleCard> {
  late DateTime _visibleMonth;

  DateTime get _today =>
      ScheduleEntry.dateOnly((widget.nowProvider ?? DateTime.now)());

  @override
  void initState() {
    super.initState();
    _visibleMonth = DateTime(
      widget.selectedDate.year,
      widget.selectedDate.month,
    );
  }

  @override
  void didUpdateWidget(WorkbenchScheduleCard oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (ScheduleEntry.formatDateKey(oldWidget.selectedDate) !=
        ScheduleEntry.formatDateKey(widget.selectedDate)) {
      _visibleMonth = DateTime(
        widget.selectedDate.year,
        widget.selectedDate.month,
      );
    }
  }

  void _shiftMonth(int delta) {
    setState(() {
      _visibleMonth = DateTime(
        _visibleMonth.year,
        _visibleMonth.month + delta,
      );
    });
  }

  List<DateTime> _calendarGridDays(DateTime month) {
    final first = DateTime(month.year, month.month, 1);
    final leading = first.weekday - 1; // Monday = 0
    final gridStart = DateTime(first.year, first.month, 1 - leading);
    final daysInMonth = DateTime(month.year, month.month + 1, 0).day;
    final totalCells = leading + daysInMonth > 35 ? 42 : 35;
    return List.generate(
      totalCells,
      (i) => DateTime(gridStart.year, gridStart.month, gridStart.day + i),
      growable: false,
    );
  }

  Widget _flatIconAction({
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
    final s = WorkbenchStrings.of(context);
    final today = _today;
    final todayKey = ScheduleEntry.formatDateKey(today);
    final selectedKey = ScheduleEntry.formatDateKey(widget.selectedDate);
    final isTodaySelected = selectedKey == todayKey;
    final isTodayMonth =
        _visibleMonth.year == today.year && _visibleMonth.month == today.month;

    final monthCounts = widget.preferences.entryCountsForMonth(
      _visibleMonth.year,
      _visibleMonth.month,
    );
    final linkedDates = widget.preferences.linkedDatesForMonth(
      _visibleMonth.year,
      _visibleMonth.month,
    );
    final pendingToday = widget.preferences.pendingCountForDate(today);
    final dayEntries = widget.preferences.entriesForDate(widget.selectedDate);
    final cardMap = {for (final card in widget.cards) card.id: card};
    final cells = _calendarGridDays(_visibleMonth);

    final content = Padding(
      padding: widget.embedded
          ? const EdgeInsets.fromLTRB(12, 8, 12, 12)
          : const EdgeInsets.fromLTRB(14, 14, 14, 12),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          if (!widget.embedded)
            Row(
              children: [
                Icon(Icons.calendar_month_rounded, size: 16, color: p.accent),
                const SizedBox(width: 7),
                Expanded(
                  child: Row(
                    children: [
                      Flexible(
                        child: Text(
                          s.scheduleTitle,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: TextStyle(
                            fontSize: 13,
                            fontWeight: FontWeight.w600,
                            color: p.ink,
                          ),
                        ),
                      ),
                      if (pendingToday > 0) ...[
                        const SizedBox(width: 6),
                        Container(
                          padding: const EdgeInsets.symmetric(
                            horizontal: 6,
                            vertical: 2,
                          ),
                          decoration: BoxDecoration(
                            color: p.accent.withValues(alpha: 0.14),
                            borderRadius: p.borderRadius(8),
                          ),
                          child: Text(
                            s.scheduleTodayPending(pendingToday),
                            style: TextStyle(
                              fontSize: 10.5,
                              fontWeight: FontWeight.w600,
                              color: p.accent,
                            ),
                          ),
                        ),
                      ],
                    ],
                  ),
                ),
                if (!isTodaySelected || !isTodayMonth)
                  Padding(
                    padding: const EdgeInsets.only(right: 4),
                    child: InkWell(
                      key: const ValueKey('schedule-jump-today'),
                      borderRadius: p.borderRadius(8),
                      onTap: () {
                        setState(
                          () => _visibleMonth = DateTime(today.year, today.month),
                        );
                        widget.onSelectDate(today);
                      },
                      child: Container(
                        padding: const EdgeInsets.symmetric(
                          horizontal: 7,
                          vertical: 4,
                        ),
                        decoration: BoxDecoration(
                          color: p.accent.withValues(alpha: 0.10),
                          borderRadius: p.borderRadius(8),
                        ),
                        child: Text(
                          s.scheduleToday,
                          style: TextStyle(
                            fontSize: 11,
                            fontWeight: FontWeight.w600,
                            color: p.accent,
                          ),
                        ),
                      ),
                    ),
                  ),
                if (widget.onFloat != null)
                  _flatIconAction(
                    key: const ValueKey('widget-float-schedule'),
                    icon: Icons.picture_in_picture_alt_rounded,
                    tooltip: s.widgetFloatTooltip,
                    color: p.muted,
                    onTap: widget.onFloat!,
                    size: 26,
                    iconSize: 14,
                  ),
                if (widget.onDetach != null)
                  _flatIconAction(
                    key: const ValueKey('widget-detach-schedule'),
                    icon: Icons.open_in_new_rounded,
                    tooltip: s.widgetDetachTooltip,
                    color: p.muted,
                    onTap: widget.onDetach!,
                    size: 26,
                    iconSize: 14,
                  ),
                _flatIconAction(
                  key: const ValueKey('schedule-add-button'),
                  icon: Icons.add_rounded,
                  tooltip: s.scheduleAdd,
                  color: p.accent,
                  onTap: widget.onCreateEntry,
                  iconSize: 17,
                ),
              ],
            ),
          if (!widget.embedded) const SizedBox(height: 10),
            Row(
              children: [
                _flatIconAction(
                  key: const ValueKey('schedule-prev-month'),
                  icon: Icons.chevron_left_rounded,
                  tooltip: '<',
                  color: p.muted,
                  onTap: () => _shiftMonth(-1),
                  size: 26,
                  iconSize: 16,
                ),
                Expanded(
                  child: Text(
                    s.scheduleMonthLabel(
                      _visibleMonth.year,
                      _visibleMonth.month,
                    ),
                    key: const ValueKey('schedule-month-label'),
                    textAlign: TextAlign.center,
                    style: TextStyle(
                      fontSize: 12,
                      fontWeight: FontWeight.w600,
                      color: p.ink,
                    ),
                  ),
                ),
                _flatIconAction(
                  key: const ValueKey('schedule-next-month'),
                  icon: Icons.chevron_right_rounded,
                  tooltip: '>',
                  color: p.muted,
                  onTap: () => _shiftMonth(1),
                  size: 26,
                  iconSize: 16,
                ),
              ],
            ),
            const SizedBox(height: 6),
            Row(
              children: [
                for (final wd in s.scheduleWeekdays)
                  Expanded(
                    child: Center(
                      child: Text(
                        wd,
                        style: TextStyle(
                          fontSize: 10.5,
                          fontWeight: FontWeight.w600,
                          color: p.muted,
                        ),
                      ),
                    ),
                  ),
              ],
            ),
            const SizedBox(height: 4),
            GridView.builder(
              shrinkWrap: true,
              physics: const NeverScrollableScrollPhysics(),
              itemCount: cells.length,
              gridDelegate: const SliverGridDelegateWithFixedCrossAxisCount(
                crossAxisCount: 7,
                mainAxisSpacing: 2,
                crossAxisSpacing: 2,
                childAspectRatio: 1.08,
              ),
              itemBuilder: (context, index) {
                final day = cells[index];
                final key = ScheduleEntry.formatDateKey(day);
                final inMonth =
                    day.month == _visibleMonth.month &&
                    day.year == _visibleMonth.year;
                final isSelected = key == selectedKey;
                final isToday = key == todayKey;
                final count = monthCounts[key] ?? 0;
                final hasLinked = linkedDates.contains(key);

                final bgColor = isSelected
                    ? p.accent
                    : isToday
                    ? p.accent.withValues(alpha: 0.14)
                    : Colors.transparent;
                final fgColor = isSelected
                    ? p.onAccent
                    : !inMonth
                    ? p.muted.withValues(alpha: 0.40)
                    : isToday
                    ? p.accent
                    : p.ink;

                return Material(
                  color: Colors.transparent,
                  child: InkWell(
                    key: ValueKey('schedule-day-$key'),
                    borderRadius: p.borderRadius(8),
                    onTap: () {
                      if (!inMonth) {
                        setState(
                          () => _visibleMonth = DateTime(day.year, day.month),
                        );
                      }
                      widget.onSelectDate(day);
                    },
                    child: Container(
                      decoration: BoxDecoration(
                        color: bgColor,
                        borderRadius: p.borderRadius(8),
                        border: !isSelected && isToday
                            ? Border.all(color: p.accent.withValues(alpha: 0.45))
                            : null,
                      ),
                      child: Column(
                        mainAxisAlignment: MainAxisAlignment.center,
                        children: [
                          Text(
                            '${day.day}',
                            style: TextStyle(
                              fontSize: 10,
                              fontWeight: isSelected || isToday
                                  ? FontWeight.w700
                                  : FontWeight.w500,
                              color: fgColor,
                            ),
                          ),
                          const SizedBox(height: 2),
                          Container(
                            width: hasLinked ? 8 : 4,
                            height: 3,
                            decoration: BoxDecoration(
                              color: count > 0
                                  ? (isSelected ? p.onAccent : p.accent)
                                  : Colors.transparent,
                              borderRadius: p.borderRadius(2),
                            ),
                          ),
                        ],
                      ),
                    ),
                  ),
                );
              },
            ),
            const SizedBox(height: 10),
            Divider(height: 1, color: p.line),
            const SizedBox(height: 10),
            Row(
              children: [
                Text(
                  s.scheduleDaySummary(
                    widget.selectedDate.month,
                    widget.selectedDate.day,
                    isTodaySelected,
                  ),
                  style: TextStyle(
                    fontSize: 11,
                    fontWeight: FontWeight.w600,
                    color: p.ink,
                  ),
                ),
                const SizedBox(width: 6),
                Container(
                  padding: const EdgeInsets.symmetric(
                    horizontal: 6,
                    vertical: 1,
                  ),
                  decoration: BoxDecoration(
                    color: p.accent.withValues(alpha: 0.12),
                    borderRadius: p.borderRadius(6),
                  ),
                  child: Text(
                    '${dayEntries.length}',
                    style: TextStyle(
                      fontSize: 10.5,
                      fontWeight: FontWeight.w600,
                      color: p.accent,
                    ),
                  ),
                ),
              ],
            ),
            const SizedBox(height: 8),
            if (dayEntries.isEmpty)
              InkWell(
                key: const ValueKey('schedule-empty-add'),
                borderRadius: p.borderRadius(10),
                onTap: widget.onCreateEntry,
                child: Container(
                  padding: const EdgeInsets.symmetric(
                    horizontal: 10,
                    vertical: 10,
                  ),
                  decoration: BoxDecoration(
                    color: p.surface.withValues(alpha: p.dark ? 0.22 : 0.40),
                    borderRadius: p.borderRadius(10),
                    border: Border.all(color: p.line),
                  ),
                  child: Row(
                    mainAxisAlignment: MainAxisAlignment.center,
                    children: [
                      Icon(
                        Icons.event_available_rounded,
                        size: 13,
                        color: p.muted,
                      ),
                      const SizedBox(width: 6),
                      Flexible(
                        child: Text(
                          s.scheduleEmptyDay,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: TextStyle(fontSize: 11.5, color: p.muted),
                        ),
                      ),
                    ],
                  ),
                ),
              )
            else
              Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  for (final entry in dayEntries)
                    _ScheduleAgendaTile(
                      key: ValueKey('schedule-item-${entry.id}'),
                      palette: p,
                      strings: s,
                      entry: entry,
                      linkedCard: entry.linkedCardId == null
                          ? null
                          : cardMap[entry.linkedCardId],
                      onToggle: () =>
                          widget.preferences.toggleCompleted(entry.id),
                      onEdit: () => widget.onEditEntry(entry),
                      onDelete: () => widget.preferences.remove(entry.id),
                      onOpenCard: widget.onOpenCard,
                    ),
                ],
              ),
          ],
        ),
      );
    if (widget.embedded) return content;
    return Glass(componentId: 'schedule', p: p, radius: 20, child: content);
  }
}

class _ScheduleAgendaTile extends StatelessWidget {
  const _ScheduleAgendaTile({
    super.key,
    required this.palette,
    required this.strings,
    required this.entry,
    required this.linkedCard,
    required this.onToggle,
    required this.onEdit,
    required this.onDelete,
    required this.onOpenCard,
  });

  final Palette palette;
  final WorkbenchStrings strings;
  final ScheduleEntry entry;
  final ScheduleCardOption? linkedCard;
  final VoidCallback onToggle;
  final VoidCallback onEdit;
  final VoidCallback onDelete;
  final ValueChanged<String> onOpenCard;

  @override
  Widget build(BuildContext context) {
    final p = palette;
    final s = strings;
    return Container(
      margin: const EdgeInsets.only(bottom: 6),
      padding: const EdgeInsets.fromLTRB(8, 7, 6, 7),
      decoration: BoxDecoration(
        color: p.surface.withValues(alpha: p.dark ? 0.25 : 0.45),
        borderRadius: p.borderRadius(11),
        border: Border.all(color: p.line),
      ),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          InkWell(
            key: ValueKey('schedule-toggle-${entry.id}'),
            borderRadius: p.borderRadius(8),
            onTap: onToggle,
            child: Padding(
              padding: const EdgeInsets.all(2),
              child: Icon(
                entry.completed
                    ? Icons.check_circle_rounded
                    : Icons.radio_button_unchecked_rounded,
                size: 16,
                color: entry.completed ? p.accent : p.muted,
              ),
            ),
          ),
          const SizedBox(width: 6),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Row(
                  children: [
                    Container(
                      padding: const EdgeInsets.symmetric(
                        horizontal: 5,
                        vertical: 1,
                      ),
                      decoration: BoxDecoration(
                        color: p.accent.withValues(alpha: 0.12),
                        borderRadius: p.borderRadius(5),
                      ),
                      child: Text(
                        entry.isAllDay ? s.scheduleAllDay : entry.timeText,
                        style: TextStyle(
                          fontSize: 10.5,
                          fontWeight: FontWeight.w600,
                          color: p.accent,
                        ),
                      ),
                    ),
                    const SizedBox(width: 6),
                    Expanded(
                      child: Text(
                        entry.title,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: TextStyle(
                          fontSize: 12,
                          fontWeight: FontWeight.w600,
                          color: entry.completed ? p.muted : p.ink,
                          decoration: entry.completed
                              ? TextDecoration.lineThrough
                              : null,
                        ),
                      ),
                    ),
                  ],
                ),
                if (entry.note.isNotEmpty) ...[
                  const SizedBox(height: 3),
                  Text(
                    entry.note,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: TextStyle(fontSize: 10, color: p.muted),
                  ),
                ],
                if (entry.hasLinkedCard) ...[
                  const SizedBox(height: 5),
                  if (linkedCard != null)
                    InkWell(
                      key: ValueKey('schedule-linked-card-${entry.id}'),
                      borderRadius: p.borderRadius(6),
                      onTap: () => onOpenCard(linkedCard!.id),
                      child: Container(
                        padding: const EdgeInsets.symmetric(
                          horizontal: 6,
                          vertical: 2,
                        ),
                        decoration: BoxDecoration(
                          color: p.accent.withValues(alpha: 0.10),
                          borderRadius: p.borderRadius(6),
                        ),
                        child: Row(
                          mainAxisSize: MainAxisSize.min,
                          children: [
                            Icon(
                              linkedCard!.icon,
                              size: 11,
                              color: p.accent,
                            ),
                            const SizedBox(width: 4),
                            Flexible(
                              child: Text(
                                linkedCard!.title,
                                maxLines: 1,
                                overflow: TextOverflow.ellipsis,
                                style: TextStyle(
                                  fontSize: 9,
                                  fontWeight: FontWeight.w600,
                                  color: p.accent,
                                ),
                              ),
                            ),
                            const SizedBox(width: 3),
                            Icon(
                              Icons.open_in_new_rounded,
                              size: 10,
                              color: p.accent,
                            ),
                          ],
                        ),
                      ),
                    )
                  else
                    Text(
                      s.scheduleMissingCard,
                      style: TextStyle(fontSize: 9, color: p.muted),
                    ),
                ],
              ],
            ),
          ),
          const SizedBox(width: 2),
          InkWell(
            key: ValueKey('schedule-edit-${entry.id}'),
            borderRadius: p.borderRadius(6),
            onTap: onEdit,
            child: Padding(
              padding: const EdgeInsets.all(4),
              child: Icon(Icons.edit_outlined, size: 13, color: p.muted),
            ),
          ),
          InkWell(
            key: ValueKey('schedule-delete-${entry.id}'),
            borderRadius: p.borderRadius(6),
            onTap: onDelete,
            child: Padding(
              padding: const EdgeInsets.all(4),
              child: Icon(Icons.close_rounded, size: 13, color: p.muted),
            ),
          ),
        ],
      ),
    );
  }
}

Future<ScheduleEntry?> showScheduleEntryDialog({
  required BuildContext context,
  required Palette palette,
  required List<ScheduleCardOption> cards,
  required DateTime initialDate,
  ScheduleEntry? initialEntry,
  String? initialLinkedCardId,
  String? initialTitle,
  DateTime Function()? nowProvider,
}) {
  final isEditing = initialEntry != null;
  final today = ScheduleEntry.dateOnly((nowProvider ?? DateTime.now)());
  final titleController = TextEditingController(
    text: initialEntry?.title ?? initialTitle ?? '',
  );
  final timeController = TextEditingController(
    text: initialEntry?.timeText ?? '',
  );
  final noteController = TextEditingController(text: initialEntry?.note ?? '');
  var selectedDate = initialEntry?.date ?? ScheduleEntry.dateOnly(initialDate);
  var selectedCardId = initialEntry != null
      ? initialEntry.linkedCardId
      : initialLinkedCardId;
  if (selectedCardId != null && !cards.any((c) => c.id == selectedCardId)) {
    selectedCardId = null;
  }
  String? validationError;

  return showStudioDialog<ScheduleEntry>(
    context: context,
    builder: (dialogContext) => StatefulBuilder(
      builder: (_, refresh) {
        final s = WorkbenchStrings.of(dialogContext);
        final p = palette;
        return StudioDialog(
          icon: Icons.event_note_rounded,
          title: isEditing ? s.scheduleEdit : s.scheduleAdd,
          content: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              TextField(
                key: const ValueKey('schedule-title-input'),
                controller: titleController,
                autofocus: true,
                maxLength: ScheduleEntry.maxTitleLength,
                decoration: InputDecoration(
                  hintText: s.scheduleTitleHint,
                  counterText: '',
                ),
              ),
              const SizedBox(height: 14),
              Text(
                s.scheduleDateLabel,
                style: TextStyle(
                  fontSize: 11,
                  fontWeight: FontWeight.w600,
                  color: p.muted,
                ),
              ),
              const SizedBox(height: 6),
              Row(
                children: [
                  IconButton(
                    key: const ValueKey('schedule-dialog-prev-day'),
                    tooltip: '-1d',
                    onPressed: () => refresh(
                      () => selectedDate = selectedDate.subtract(
                        const Duration(days: 1),
                      ),
                    ),
                    icon: const Icon(Icons.chevron_left_rounded, size: 18),
                  ),
                  Expanded(
                    child: Text(
                      ScheduleEntry.formatDateKey(selectedDate),
                      key: const ValueKey('schedule-dialog-date'),
                      textAlign: TextAlign.center,
                      style: TextStyle(
                        fontSize: 13,
                        fontWeight: FontWeight.w600,
                        color: p.ink,
                      ),
                    ),
                  ),
                  IconButton(
                    key: const ValueKey('schedule-dialog-next-day'),
                    tooltip: '+1d',
                    onPressed: () => refresh(
                      () => selectedDate = selectedDate.add(
                        const Duration(days: 1),
                      ),
                    ),
                    icon: const Icon(Icons.chevron_right_rounded, size: 18),
                  ),
                ],
              ),
              const SizedBox(height: 6),
              Wrap(
                spacing: 6,
                runSpacing: 6,
                children: [
                  for (final offset in const [0, 1, 3, 7])
                    ChoiceChip(
                      label: Text(
                        s.scheduleQuickDays(offset),
                        style: const TextStyle(fontSize: 10),
                      ),
                      selected:
                          ScheduleEntry.formatDateKey(selectedDate) ==
                          ScheduleEntry.formatDateKey(
                            today.add(Duration(days: offset)),
                          ),
                      onSelected: (_) => refresh(
                        () => selectedDate = today.add(Duration(days: offset)),
                      ),
                    ),
                ],
              ),
              const SizedBox(height: 14),
              TextField(
                key: const ValueKey('schedule-time-input'),
                controller: timeController,
                decoration: InputDecoration(
                  labelText: s.scheduleTimeLabel,
                  hintText: '09:30',
                ),
              ),
              const SizedBox(height: 6),
              Wrap(
                spacing: 6,
                runSpacing: 6,
                children: [
                  for (final preset in const ['', '09:30', '14:00', '20:00'])
                    ActionChip(
                      label: Text(
                        preset.isEmpty ? s.scheduleAllDay : preset,
                        style: const TextStyle(fontSize: 10),
                      ),
                      onPressed: () =>
                          refresh(() => timeController.text = preset),
                    ),
                ],
              ),
              const SizedBox(height: 14),
              DropdownButtonFormField<String?>(
                key: const ValueKey('schedule-card-picker'),
                initialValue: selectedCardId,
                isExpanded: true,
                decoration: InputDecoration(labelText: s.scheduleLinkCardLabel),
                items: [
                  DropdownMenuItem<String?>(
                    value: null,
                    child: Text(s.scheduleNoLinkedCard),
                  ),
                  for (final card in cards)
                    DropdownMenuItem<String?>(
                      value: card.id,
                      child: Row(
                        children: [
                          Icon(card.icon, size: 15, color: card.color),
                          const SizedBox(width: 8),
                          Expanded(
                            child: Text(
                              '[${card.categoryLabel}] ${card.title}',
                              maxLines: 1,
                              overflow: TextOverflow.ellipsis,
                            ),
                          ),
                        ],
                      ),
                    ),
                ],
                onChanged: (value) => refresh(() => selectedCardId = value),
              ),
              const SizedBox(height: 14),
              TextField(
                key: const ValueKey('schedule-note-input'),
                controller: noteController,
                maxLines: 2,
                maxLength: ScheduleEntry.maxNoteLength,
                decoration: InputDecoration(
                  labelText: s.scheduleNoteLabel,
                  counterText: '',
                ),
              ),
              if (validationError != null) ...[
                const SizedBox(height: 8),
                Text(
                  validationError!,
                  style: TextStyle(
                    fontSize: 11,
                    color: Theme.of(dialogContext).colorScheme.error,
                  ),
                ),
              ],
            ],
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(dialogContext),
              child: Text(s.scheduleCancel),
            ),
            FilledButton(
              key: const ValueKey('schedule-save-button'),
              onPressed: () {
                final cleanTitle = titleController.text.trim();
                final cleanTime = timeController.text.trim();
                if (cleanTitle.isEmpty ||
                    cleanTitle.contains(RegExp(r'[\r\n]'))) {
                  refresh(() => validationError = s.scheduleTitleError);
                  return;
                }
                if (!ScheduleEntry.isValidTimeText(cleanTime)) {
                  refresh(() => validationError = s.scheduleTimeError);
                  return;
                }
                final dateKey = ScheduleEntry.formatDateKey(selectedDate);
                final result =
                    (initialEntry ??
                            ScheduleEntry(
                              id: ScheduleEntry.nextId(),
                              title: cleanTitle,
                              dateKey: dateKey,
                              createdAtMs:
                                  DateTime.now().millisecondsSinceEpoch,
                            ))
                        .copyWith(
                          title: cleanTitle,
                          dateKey: dateKey,
                          timeText: cleanTime,
                          note: noteController.text.trim(),
                          linkedCardId: selectedCardId,
                          clearLinkedCard: selectedCardId == null,
                        )
                        .validated();
                Navigator.pop(dialogContext, result);
              },
              child: Text(s.scheduleSave),
            ),
          ],
        );
      },
    ),
  );
}
