import 'package:material_ui/material_ui.dart';

import '../appearance.dart';
import '../schedule/schedule_store.dart';
import '../widgets_suite/countdown_store.dart';
import 'schedule_calendar_panel.dart';
import 'workbench_controls.dart';
import 'workbench_strings.dart';

class WorkbenchCountdownCard extends StatelessWidget {
  const WorkbenchCountdownCard({
    super.key,
    required this.palette,
    required this.preferences,
    required this.cards,
    required this.onCreate,
    required this.onEdit,
    required this.onOpenCard,
    this.embedded = false,
    this.onFloat,
    this.onDetach,
    this.nowProvider,
  });

  final Palette palette;
  final CountdownPreferences preferences;
  final List<ScheduleCardOption> cards;
  final VoidCallback onCreate;
  final ValueChanged<CountdownItem> onEdit;
  final ValueChanged<String> onOpenCard;
  final bool embedded;
  final VoidCallback? onFloat;
  final VoidCallback? onDetach;
  final DateTime Function()? nowProvider;

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
    palette: palette,
    icon: icon,
    tooltip: tooltip,
    color: color,
    onTap: onTap,
    size: size,
    iconSize: iconSize,
  );

  @override
  Widget build(BuildContext context) {
    final p = palette;
    final s = WorkbenchStrings.of(context);
    final now = (nowProvider ?? DateTime.now)();
    final items = preferences.entries;
    final cardMap = {for (final c in cards) c.id: c};

    final body = Padding(
      padding: embedded
          ? const EdgeInsets.fromLTRB(12, 8, 12, 12)
          : const EdgeInsets.fromLTRB(14, 14, 14, 12),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          if (!embedded)
            Row(
              children: [
                Icon(Icons.hourglass_bottom_rounded, size: 16, color: p.accent),
                const SizedBox(width: 7),
                Expanded(
                  child: Text(
                    s.countdownTitle,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: TextStyle(
                      fontSize: 13,
                      fontWeight: FontWeight.w600,
                      color: p.ink,
                    ),
                  ),
                ),
                if (onFloat != null)
                  _iconBtn(
                    key: const ValueKey('widget-float-countdown'),
                    icon: Icons.picture_in_picture_alt_rounded,
                    tooltip: s.widgetFloatTooltip,
                    color: p.muted,
                    onTap: onFloat!,
                    iconSize: 14,
                  ),
                if (onDetach != null)
                  _iconBtn(
                    key: const ValueKey('widget-detach-countdown'),
                    icon: Icons.open_in_new_rounded,
                    tooltip: s.widgetDetachTooltip,
                    color: p.muted,
                    onTap: onDetach!,
                    iconSize: 14,
                  ),
                _iconBtn(
                  key: const ValueKey('countdown-add-button'),
                  icon: Icons.add_rounded,
                  tooltip: s.countdownAdd,
                  color: p.accent,
                  onTap: onCreate,
                  iconSize: 17,
                ),
              ],
            ),
          if (!embedded) const SizedBox(height: 10),
          if (items.isEmpty)
            InkWell(
              key: const ValueKey('countdown-empty-add'),
              borderRadius: p.borderRadius(10),
              onTap: onCreate,
              child: Container(
                padding: const EdgeInsets.symmetric(
                  horizontal: 10,
                  vertical: 11,
                ),
                decoration: BoxDecoration(
                  color: p.surface.withValues(alpha: p.dark ? 0.22 : 0.40),
                  borderRadius: p.borderRadius(10),
                  border: Border.all(color: p.line),
                ),
                child: Row(
                  mainAxisAlignment: MainAxisAlignment.center,
                  children: [
                    Icon(Icons.flag_outlined, size: 14, color: p.muted),
                    const SizedBox(width: 6),
                    Flexible(
                      child: Text(
                        s.countdownEmpty,
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
                for (final item in items) ...[
                  Builder(
                    builder: (context) {
                      final days = item.daysFrom(now);
                      final linkedCard = item.linkedCardId == null
                          ? null
                          : cardMap[item.linkedCardId];
                      return Container(
                        key: ValueKey('countdown-item-${item.id}'),
                        margin: const EdgeInsets.only(bottom: 7),
                        padding: const EdgeInsets.fromLTRB(10, 8, 6, 8),
                        decoration: BoxDecoration(
                          color: p.surface.withValues(alpha: p.dark ? 0.25 : 0.45),
                          borderRadius: p.borderRadius(12),
                          border: Border.all(color: p.line),
                        ),
                        child: Row(
                          children: [
                            Container(
                              padding: const EdgeInsets.symmetric(
                                horizontal: 8,
                                vertical: 5,
                              ),
                              decoration: BoxDecoration(
                                color: days >= 0
                                    ? p.accent.withValues(alpha: 0.14)
                                    : p.line.withValues(alpha: 0.5),
                                borderRadius: p.borderRadius(8),
                              ),
                              child: Text(
                                s.countdownDaysBadge(days),
                                style: TextStyle(
                                  fontSize: 11,
                                  fontWeight: FontWeight.w700,
                                  color: days >= 0 ? p.accent : p.muted,
                                ),
                              ),
                            ),
                            const SizedBox(width: 8),
                            Expanded(
                              child: Column(
                                crossAxisAlignment: CrossAxisAlignment.start,
                                children: [
                                  Text(
                                    item.title,
                                    maxLines: 1,
                                    overflow: TextOverflow.ellipsis,
                                    style: TextStyle(
                                      fontSize: 12,
                                      fontWeight: FontWeight.w600,
                                      color: p.ink,
                                    ),
                                  ),
                                  const SizedBox(height: 2),
                                  Wrap(
                                    spacing: 6,
                                    crossAxisAlignment:
                                        WrapCrossAlignment.center,
                                    children: [
                                      Text(
                                        item.targetDateKey,
                                        style: TextStyle(
                                          fontSize: 10.5,
                                          color: p.muted,
                                        ),
                                      ),
                                      if (linkedCard != null)
                                        InkWell(
                                          key: ValueKey(
                                            'countdown-linked-${item.id}',
                                          ),
                                          borderRadius: p.borderRadius(5),
                                          onTap: () =>
                                              onOpenCard(linkedCard.id),
                                          child: Row(
                                            mainAxisSize: MainAxisSize.min,
                                            children: [
                                              Icon(
                                                linkedCard.icon,
                                                size: 11,
                                                color: p.accent,
                                              ),
                                              const SizedBox(width: 3),
                                              Text(
                                                linkedCard.title,
                                                style: TextStyle(
                                                  fontSize: 10.5,
                                                  fontWeight: FontWeight.w600,
                                                  color: p.accent,
                                                ),
                                              ),
                                            ],
                                          ),
                                        ),
                                    ],
                                  ),
                                ],
                              ),
                            ),
                            InkWell(
                              key: ValueKey('countdown-pin-${item.id}'),
                              borderRadius: p.borderRadius(6),
                              onTap: () => preferences.togglePinned(item.id),
                              child: Padding(
                                padding: const EdgeInsets.all(4),
                                child: Icon(
                                  item.pinned
                                      ? Icons.push_pin_rounded
                                      : Icons.push_pin_outlined,
                                  size: 13,
                                  color: item.pinned ? p.accent : p.muted,
                                ),
                              ),
                            ),
                            InkWell(
                              key: ValueKey('countdown-edit-${item.id}'),
                              borderRadius: p.borderRadius(6),
                              onTap: () => onEdit(item),
                              child: Padding(
                                padding: const EdgeInsets.all(4),
                                child: Icon(
                                  Icons.edit_outlined,
                                  size: 13,
                                  color: p.muted,
                                ),
                              ),
                            ),
                            InkWell(
                              key: ValueKey('countdown-delete-${item.id}'),
                              borderRadius: p.borderRadius(6),
                              onTap: () => preferences.remove(item.id),
                              child: Padding(
                                padding: const EdgeInsets.all(4),
                                child: Icon(
                                  Icons.close_rounded,
                                  size: 13,
                                  color: p.muted,
                                ),
                              ),
                            ),
                          ],
                        ),
                      );
                    },
                  ),
                ],
              ],
            ),
        ],
      ),
    );

    if (embedded) return body;
    return Glass(componentId: 'countdown', p: p, radius: 20, child: body);
  }
}

Future<CountdownItem?> showCountdownDialog({
  required BuildContext context,
  required Palette palette,
  required List<ScheduleCardOption> cards,
  CountdownItem? initialItem,
  String? initialLinkedCardId,
  DateTime Function()? nowProvider,
}) {
  final isEditing = initialItem != null;
  final today = ScheduleEntry.dateOnly((nowProvider ?? DateTime.now)());
  final titleController = TextEditingController(
    text: initialItem?.title ?? '',
  );
  final noteController = TextEditingController(text: initialItem?.note ?? '');
  var targetDate = initialItem?.targetDate ?? today.add(const Duration(days: 7));
  var pinned = initialItem?.pinned ?? true;
  var selectedCardId =
      initialItem != null ? initialItem.linkedCardId : initialLinkedCardId;
  if (selectedCardId != null && !cards.any((c) => c.id == selectedCardId)) {
    selectedCardId = null;
  }
  String? validationError;

  return showStudioDialog<CountdownItem>(
    context: context,
    builder: (dialogContext) => StatefulBuilder(
      builder: (_, refresh) {
        final s = WorkbenchStrings.of(dialogContext);
        final p = palette;
        return StudioDialog(
          icon: Icons.hourglass_bottom_rounded,
          title: isEditing ? s.countdownEdit : s.countdownAdd,
          content: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              TextField(
                key: const ValueKey('countdown-title-input'),
                controller: titleController,
                autofocus: true,
                maxLength: CountdownItem.maxTitleLength,
                decoration: InputDecoration(
                  hintText: s.countdownTitleHint,
                  counterText: '',
                ),
              ),
              const SizedBox(height: 14),
              Row(
                children: [
                  IconButton(
                    key: const ValueKey('countdown-prev-day'),
                    onPressed: () => refresh(
                      () => targetDate = targetDate.subtract(
                        const Duration(days: 1),
                      ),
                    ),
                    icon: const Icon(Icons.chevron_left_rounded, size: 18),
                  ),
                  Expanded(
                    child: Text(
                      '${ScheduleEntry.formatDateKey(targetDate)} (${s.countdownDaysBadge(targetDate.difference(today).inDays)})',
                      key: const ValueKey('countdown-dialog-date'),
                      textAlign: TextAlign.center,
                      style: TextStyle(
                        fontSize: 12,
                        fontWeight: FontWeight.w600,
                        color: p.ink,
                      ),
                    ),
                  ),
                  IconButton(
                    key: const ValueKey('countdown-next-day'),
                    onPressed: () => refresh(
                      () => targetDate = targetDate.add(const Duration(days: 1)),
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
                  for (final offset in const [0, 3, 7, 30, 100])
                    ActionChip(
                      label: Text(
                        s.scheduleQuickDays(offset),
                        style: const TextStyle(fontSize: 10),
                      ),
                      onPressed: () => refresh(
                        () => targetDate = today.add(Duration(days: offset)),
                      ),
                    ),
                ],
              ),
              const SizedBox(height: 12),
              DropdownButtonFormField<String?>(
                key: const ValueKey('countdown-card-picker'),
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
                onChanged: (v) => refresh(() => selectedCardId = v),
              ),
              const SizedBox(height: 10),
              SwitchListTile.adaptive(
                contentPadding: EdgeInsets.zero,
                title: Text(
                  s.countdownPinFooter,
                  style: TextStyle(fontSize: 12, color: p.ink),
                ),
                value: pinned,
                onChanged: (v) => refresh(() => pinned = v),
              ),
              TextField(
                controller: noteController,
                maxLines: 2,
                maxLength: CountdownItem.maxNoteLength,
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
              key: const ValueKey('countdown-save-button'),
              onPressed: () {
                final cleanTitle = titleController.text.trim();
                if (cleanTitle.isEmpty ||
                    cleanTitle.contains(RegExp(r'[\r\n]'))) {
                  refresh(() => validationError = s.scheduleTitleError);
                  return;
                }
                final dateKey = ScheduleEntry.formatDateKey(targetDate);
                final result =
                    (initialItem ??
                            CountdownItem(
                              id: CountdownItem.nextId(),
                              title: cleanTitle,
                              targetDateKey: dateKey,
                              createdAtMs:
                                  DateTime.now().millisecondsSinceEpoch,
                            ))
                        .copyWith(
                          title: cleanTitle,
                          targetDateKey: dateKey,
                          note: noteController.text.trim(),
                          pinned: pinned,
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
