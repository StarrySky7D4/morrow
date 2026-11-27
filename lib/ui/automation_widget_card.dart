import 'package:material_ui/material_ui.dart';

import '../appearance.dart';
import '../widgets_suite/automation_engine.dart';
import 'workbench_controls.dart';
import 'workbench_strings.dart';

class WorkbenchAutomationCard extends StatelessWidget {
  const WorkbenchAutomationCard({
    super.key,
    required this.palette,
    required this.preferences,
    required this.onCreateRule,
    required this.onRunRuleNow,
    this.embedded = false,
    this.onFloat,
    this.onDetach,
  });

  final Palette palette;
  final AutomationPreferences preferences;
  final VoidCallback onCreateRule;
  final ValueChanged<AutomationRule> onRunRuleNow;
  final bool embedded;
  final VoidCallback? onFloat;
  final VoidCallback? onDetach;

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
    final rules = preferences.rules;
    final logs = preferences.logs;

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
                Icon(Icons.bolt_rounded, size: 16, color: p.accent),
                const SizedBox(width: 7),
                Expanded(
                  child: Row(
                    children: [
                      Flexible(
                        child: Text(
                          s.automationTitle,
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
                          s.automationEnabledBadge(preferences.enabledCount),
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
                if (onFloat != null)
                  _iconBtn(
                    key: const ValueKey('widget-float-automation'),
                    icon: Icons.picture_in_picture_alt_rounded,
                    tooltip: s.widgetFloatTooltip,
                    color: p.muted,
                    onTap: onFloat!,
                    iconSize: 14,
                  ),
                if (onDetach != null)
                  _iconBtn(
                    key: const ValueKey('widget-detach-automation'),
                    icon: Icons.open_in_new_rounded,
                    tooltip: s.widgetDetachTooltip,
                    color: p.muted,
                    onTap: onDetach!,
                    iconSize: 14,
                  ),
                _iconBtn(
                  key: const ValueKey('automation-add-button'),
                  icon: Icons.add_rounded,
                  tooltip: s.automationAdd,
                  color: p.accent,
                  onTap: onCreateRule,
                  iconSize: 17,
                ),
              ],
            ),
          if (!embedded) const SizedBox(height: 10),
          for (final rule in rules)
            Container(
              key: ValueKey('automation-rule-${rule.id}'),
              margin: const EdgeInsets.only(bottom: 6),
              padding: const EdgeInsets.fromLTRB(10, 7, 6, 7),
              decoration: BoxDecoration(
                color: p.surface.withValues(alpha: p.dark ? 0.25 : 0.45),
                borderRadius: p.borderRadius(11),
                border: Border.all(color: p.line),
              ),
              child: Row(
                children: [
                  Expanded(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(
                          rule.name,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: TextStyle(
                            fontSize: 12,
                            fontWeight: FontWeight.w600,
                            color: rule.enabled ? p.ink : p.muted,
                          ),
                        ),
                        const SizedBox(height: 2),
                        Text(
                          '${s.triggerLabel(rule.trigger)}  ->  ${s.actionLabel(rule.action)}',
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: TextStyle(fontSize: 10.5, color: p.accent),
                        ),
                      ],
                    ),
                  ),
                  InkWell(
                    key: ValueKey('automation-run-${rule.id}'),
                    borderRadius: p.borderRadius(6),
                    onTap: () => onRunRuleNow(rule),
                    child: Padding(
                      padding: const EdgeInsets.symmetric(
                        horizontal: 6,
                        vertical: 4,
                      ),
                      child: Text(
                        s.automationTestRun,
                        style: TextStyle(
                          fontSize: 10.5,
                          fontWeight: FontWeight.w600,
                          color: p.accent,
                        ),
                      ),
                    ),
                  ),
                  WorkbenchCompactSwitch(
                    key: ValueKey('automation-toggle-${rule.id}'),
                    palette: p,
                    value: rule.enabled,
                    semanticLabel: rule.name,
                    onChanged: (_) => preferences.toggleRule(rule.id),
                  ),
                  if (!rule.builtIn)
                    InkWell(
                      key: ValueKey('automation-delete-${rule.id}'),
                      borderRadius: p.borderRadius(6),
                      onTap: () => preferences.removeRule(rule.id),
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
            ),
          if (logs.isNotEmpty) ...[
            const SizedBox(height: 6),
            Text(
              s.automationRecentLogs,
              style: TextStyle(
                fontSize: 10.5,
                fontWeight: FontWeight.w600,
                color: p.muted,
              ),
            ),
            const SizedBox(height: 4),
            for (final log in logs.take(3))
              Padding(
                padding: const EdgeInsets.only(bottom: 2),
                child: Text(
                  '${log.timestamp.hour.toString().padLeft(2, '0')}:${log.timestamp.minute.toString().padLeft(2, '0')} · ${log.summary}',
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(fontSize: 10.5, color: p.muted),
                ),
              ),
          ],
        ],
      ),
    );

    if (embedded) return body;
    return Glass(componentId: 'automation', p: p, radius: 20, child: body);
  }
}

Future<AutomationRule?> showAutomationRuleDialog({
  required BuildContext context,
  required Palette palette,
}) {
  final nameController = TextEditingController();
  final msgController = TextEditingController();
  var trigger = AutomationTrigger.timerCompleted;
  var action = AutomationAction.footerReminder;

  return showStudioDialog<AutomationRule>(
    context: context,
    builder: (dialogContext) => StatefulBuilder(
      builder: (_, refresh) {
        final s = WorkbenchStrings.of(dialogContext);
        return StudioDialog(
          icon: Icons.bolt_rounded,
          title: s.automationAdd,
          content: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              TextField(
                key: const ValueKey('automation-name-input'),
                controller: nameController,
                autofocus: true,
                maxLength: 80,
                decoration: InputDecoration(
                  hintText: s.automationRuleNameHint,
                  counterText: '',
                ),
              ),
              const SizedBox(height: 12),
              DropdownButtonFormField<AutomationTrigger>(
                key: const ValueKey('automation-trigger-picker'),
                initialValue: trigger,
                isExpanded: true,
                decoration: InputDecoration(
                  labelText: s.automationTriggerLabel,
                ),
                items: [
                  for (final t in AutomationTrigger.values)
                    DropdownMenuItem(value: t, child: Text(s.triggerLabel(t))),
                ],
                onChanged: (v) {
                  if (v != null) refresh(() => trigger = v);
                },
              ),
              const SizedBox(height: 12),
              DropdownButtonFormField<AutomationAction>(
                key: const ValueKey('automation-action-picker'),
                initialValue: action,
                isExpanded: true,
                decoration: InputDecoration(
                  labelText: s.automationActionLabel,
                ),
                items: [
                  for (final a in AutomationAction.values)
                    DropdownMenuItem(value: a, child: Text(s.actionLabel(a))),
                ],
                onChanged: (v) {
                  if (v != null) refresh(() => action = v);
                },
              ),
              const SizedBox(height: 12),
              TextField(
                controller: msgController,
                maxLength: 120,
                decoration: InputDecoration(
                  hintText: s.automationCustomMsgHint,
                  counterText: '',
                ),
              ),
            ],
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(dialogContext),
              child: Text(s.scheduleCancel),
            ),
            FilledButton(
              key: const ValueKey('automation-save-button'),
              onPressed: () {
                final cleanName = nameController.text.trim();
                final effectiveName = cleanName.isNotEmpty
                    ? cleanName
                    : '${s.triggerLabel(trigger)} -> ${s.actionLabel(action)}';
                Navigator.pop(
                  dialogContext,
                  AutomationRule(
                    id: AutomationRule.nextId(),
                    name: effectiveName,
                    trigger: trigger,
                    action: action,
                    customMessage: msgController.text.trim(),
                  ),
                );
              },
              child: Text(s.scheduleSave),
            ),
          ],
        );
      },
    ),
  );
}
