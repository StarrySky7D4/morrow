import 'dart:math';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:morrow_i18n/morrow_i18n.dart';

import 'appearance.dart';
import 'hold_reorder.dart';
import 'component_context_menu.dart';
import 'neumorphic_controls.dart';
import 'plugins/versioned_content.dart';
import 'plugins/versioned_idea_view.dart';
import 'plugins/workbench_ids.dart';
import 'plugins/workbench_labels.dart';

/// TaskId controls for format-2 cards. Format-1 tasks remain read-only until
/// the caller performs an explicit migration.
class VersionedTaskPanel extends StatefulWidget {
  const VersionedTaskPanel({
    super.key,
    required this.view,
    required this.writable,
    required this.busy,
    required this.onCommand,
  });

  final VersionedIdeaView view;
  final bool writable;
  final bool busy;

  /// Successful completion must be followed by the parent's accepted view.
  /// Until that view arrives, the panel keeps new operations disabled.
  final Future<void> Function(TaskEditCommand) onCommand;

  @override
  State<VersionedTaskPanel> createState() => _VersionedTaskPanelState();
}

class _VersionedTaskPanelState extends State<VersionedTaskPanel> {
  final _newTask = TextEditingController();
  late String _selectedStage = widget.view.stage;
  bool _submitting = false;
  BigInt? _awaitingRevision;
  VersionedIdeaView? _awaitingAcceptedView;
  final _retryByCard = <String, TaskEditCommand>{};

  TaskEditCommand? get _retryCommand => _retryByCard[widget.view.id];

  List<WorkbenchStage> get _stageOptions => switch (widget.view.category) {
    '灵感' => const [WorkbenchStage.unsorted, WorkbenchStage.organized],
    '进行中' => const [
      WorkbenchStage.planned,
      WorkbenchStage.active,
      WorkbenchStage.completed,
    ],
    '实验' => const [
      WorkbenchStage.unverified,
      WorkbenchStage.verifying,
      WorkbenchStage.recorded,
    ],
    _ => const [],
  };

  bool get _validStage =>
      _stageOptions.any((stage) => WorkbenchV1.stage(stage) == _selectedStage);

  String _stageLabel(BuildContext context, String value) {
    for (final stage in _stageOptions) {
      if (WorkbenchV1.stage(stage) == value) {
        return WorkbenchLabelsScope.of(context).filter(StageFilter(stage));
      }
    }
    return value;
  }

  bool get _canEdit =>
      widget.view.formatVersion == 2 &&
      !widget.view.deleted &&
      widget.writable &&
      !widget.busy &&
      !_submitting &&
      _retryCommand == null &&
      _awaitingAcceptedView == null &&
      _awaitingRevision != widget.view.revision;

  bool get _canRetry =>
      _retryCommand != null &&
      widget.view.formatVersion == 2 &&
      !widget.view.deleted &&
      widget.writable &&
      !widget.busy &&
      !_submitting;

  @override
  void didUpdateWidget(covariant VersionedTaskPanel oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.view.id != widget.view.id) {
      _newTask.clear();
      _selectedStage = widget.view.stage;
      _awaitingRevision = null;
      _awaitingAcceptedView = null;
    } else if (oldWidget.view.revision != widget.view.revision ||
        oldWidget.view.category != widget.view.category) {
      if (_selectedStage == oldWidget.view.stage ||
          oldWidget.view.category != widget.view.category) {
        _selectedStage = widget.view.stage;
      }
      _awaitingRevision = null;
    }
    // A successful callback can complete before the parent's accepted card
    // has rebuilt this panel. A new view also acknowledges a historical retry
    // whose accepted revision equals the currently displayed revision.
    if (_awaitingAcceptedView != null &&
        !identical(widget.view, _awaitingAcceptedView)) {
      _awaitingAcceptedView = null;
      _awaitingRevision = null;
    }
  }

  @override
  void dispose() {
    _newTask.dispose();
    super.dispose();
  }

  Future<bool> _submit(TaskEditCommand command, {bool retry = false}) async {
    if (retry ? !_canRetry || !identical(command, _retryCommand) : !_canEdit) {
      return false;
    }
    final submittedRevision = widget.view.revision;
    final submittedView = widget.view;
    final cardId = widget.view.id;
    var released = false;
    var acceptedViewDelivered = false;
    setState(() => _submitting = true);
    try {
      await widget.onCommand(command);
      _retryByCard.remove(cardId);
      if (mounted && widget.view.id == cardId) {
        // onCommand may have delivered its accepted view before completing.
        acceptedViewDelivered = !identical(widget.view, submittedView);
        if (!acceptedViewDelivered) _awaitingAcceptedView = submittedView;
      }
      return true;
    } on VersionedMutationNoCommit {
      released = true;
      _retryByCard.remove(cardId);
      if (mounted) {
        ScaffoldMessenger.maybeOf(context)?.showSnackBar(
          SnackBar(content: Text(L10n.of(context).mainSaveNotSubmitted)),
        );
      }
      return false;
    } on VersionedMutationNotSubmitted {
      // A locally unsent retry says nothing about the earlier unknown result.
      if (!retry) {
        released = true;
        _retryByCard.remove(cardId);
      }
      if (mounted) {
        ScaffoldMessenger.maybeOf(context)?.showSnackBar(
          SnackBar(
            content: Text(
              retry
                  ? L10n.of(context).mainSaveUnknown
                  : L10n.of(context).mainSaveNotSubmitted,
            ),
          ),
        );
      }
      return false;
    } catch (_) {
      _retryByCard[cardId] = command;
      if (mounted) {
        ScaffoldMessenger.maybeOf(context)?.showSnackBar(
          SnackBar(content: Text(L10n.of(context).mainSaveUnknown)),
        );
      }
      return false;
    } finally {
      if (mounted) {
        setState(() {
          _submitting = false;
          if (widget.view.id == cardId) {
            // The panel cannot mint another operation against this snapshot.
            _awaitingRevision = released || acceptedViewDelivered
                ? null
                : submittedRevision;
          }
        });
      }
    }
  }

  String _newTaskId() {
    final used = {
      ...widget.view.retiredTaskIds,
      for (final task in widget.view.tasks)
        if (task is IdentifiedIdeaTaskView) task.taskId,
    };
    final random = Random.secure();
    while (true) {
      final bytes = List.generate(
        16,
        (_) => random.nextInt(256).toRadixString(16).padLeft(2, '0'),
      ).join();
      final id = 'task-$bytes';
      if (!used.contains(id)) return id;
    }
  }

  Future<void> _add() async {
    final text = _newTask.text.trim();
    if (text.isEmpty) return;
    final cardId = widget.view.id;
    if (await _submit(TaskEditCommand.add(_newTaskId(), text)) &&
        mounted &&
        widget.view.id == cardId) {
      _newTask.clear();
    }
  }

  Future<void> _rename(IdentifiedIdeaTaskView task) async {
    if (!_canEdit) return;
    await showDialog<void>(
      context: context,
      builder: (_) => _RenameTaskDialog(
        task: task,
        onSubmit: (text) => _submit(TaskEditCommand.rename(task.taskId, text)),
      ),
    );
  }

  Future<bool> _confirm(String message, {String? confirmLabel}) async {
    if (!_canEdit) return false;
    final result = await showDialog<bool>(
      context: context,
      builder: (dialogContext) {
        final l = L10n.of(dialogContext);
        return AlertDialog(
          content: Text(message),
          actions: [
            TextButton(
              onPressed: () => Navigator.of(dialogContext).pop(false),
              child: Text(l.visualCancel),
            ),
            TextButton(
              key: const ValueKey('task-confirm-action'),
              onPressed: () => Navigator.of(dialogContext).pop(true),
              child: Text(confirmLabel ?? l.mainDone),
            ),
          ],
        );
      },
    );
    return result == true && mounted && _canEdit;
  }

  Future<void> _remove(IdentifiedIdeaTaskView task) async {
    final l = L10n.of(context);
    if (await _confirm(
      l.mainTaskRemoveConfirm(task.text),
      confirmLabel: l.mainDelete,
    )) {
      await _submit(TaskEditCommand.remove(task.taskId));
    }
  }

  Future<void> _reorder(int index, int delta) async {
    if (!_canEdit) return;
    final order = [
      for (final task in widget.view.tasks)
        (task as IdentifiedIdeaTaskView).taskId,
    ];
    final next = index + delta;
    if (next < 0 || next >= order.length) return;
    final moved = order.removeAt(index);
    order.insert(next, moved);
    await _submit(TaskEditCommand.reorder(order));
  }

  Future<void> _setStage() async {
    if (_validStage && _selectedStage != widget.view.stage) {
      await _submit(TaskEditCommand.setStage(_selectedStage));
    }
  }

  Future<void> _completeAll() async {
    if (!_validStage) return;
    if (await _confirm(
      L10n.of(
        context,
      ).mainTaskCompleteAllConfirm(_stageLabel(context, _selectedStage)),
    )) {
      await _submit(TaskEditCommand.completeAllAndSetStage(_selectedStage));
    }
  }

  Widget _legacyRow(LegacyIdeaTaskView task) => Row(
    children: [
      Icon(
        task.completedByName
            ? Icons.check_circle_rounded
            : Icons.circle_outlined,
        size: 18,
      ),
      const SizedBox(width: 8),
      Expanded(child: Text(task.text)),
    ],
  );

  Widget _identifiedRow(IdentifiedIdeaTaskView task, int index) {
    return ComponentContextMenu(
      key: ValueKey('task-menu-${task.taskId}'),
      actions: () {
        final l = L10n.of(context);
        final openedView = widget.view;
        bool canSelect() =>
            mounted && _canEdit && identical(widget.view, openedView);
        return [
          for (final done
              in task.needsExplicitDecision
                  ? [true, false]
                  : [task.completion != VersionedTaskCompletion.complete])
            ComponentMenuAction(
              label: done ? l.mainTaskMarkComplete : l.mainTaskMarkIncomplete,
              icon: done ? Icons.check_circle_outline : Icons.circle_outlined,
              isEnabled: canSelect,
              onSelected: () =>
                  _submit(TaskEditCommand.setCompletion(task.taskId, done)),
            ),
          ComponentMenuAction(
            label: l.mainTaskRename,
            icon: Icons.edit_outlined,
            isEnabled: canSelect,
            onSelected: () => _rename(task),
          ),
          ComponentMenuAction(
            label: MaterialLocalizations.of(context).copyButtonLabel,
            icon: Icons.copy_outlined,
            onSelected: () => Clipboard.setData(ClipboardData(text: task.text)),
          ),
          ComponentMenuAction(
            label: l.mainTaskMoveUp,
            icon: Icons.arrow_upward,
            enabled: index > 0,
            isEnabled: canSelect,
            onSelected: () => _reorder(index, -1),
          ),
          ComponentMenuAction(
            label: l.mainTaskMoveDown,
            icon: Icons.arrow_downward,
            enabled: index < widget.view.tasks.length - 1,
            isEnabled: canSelect,
            onSelected: () => _reorder(index, 1),
          ),
          ComponentMenuAction(
            label: l.mainDelete,
            icon: Icons.delete_outline,
            isEnabled: canSelect,
            onSelected: () => _remove(task),
          ),
        ];
      },
      child: HoldReorder(
        scope: this,
        id: task.taskId,
        revision: widget.view,
        label: task.text,
        enabled: _canEdit,
        onMove: (source, target, after) {
          if (!_canEdit) return;
          final order = [
            for (final row in widget.view.tasks)
              (row as IdentifiedIdeaTaskView).taskId,
          ];
          _submit(
            TaskEditCommand.reorder(
              moveRelative(order, source as String, target as String, after),
            ),
          );
        },
        builder: (handle) => _identifiedRowBody(task, index, handle),
      ),
    );
  }

  Widget _identifiedRowBody(
    IdentifiedIdeaTaskView task,
    int index,
    Widget handle,
  ) {
    final l = L10n.of(context);
    final enabled = _canEdit;
    return Padding(
      key: ValueKey('task-row-${task.taskId}'),
      padding: const EdgeInsets.symmetric(vertical: 3),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              handle,
              if (task.needsExplicitDecision)
                const Icon(Icons.help_outline, size: 20)
              else
                NeumorphicCheckbox(
                  key: ValueKey('task-checkbox-${task.taskId}'),
                  palette: AppearanceScope.of(context),
                  value: task.completion == VersionedTaskCompletion.complete,
                  onChanged: enabled
                      ? (value) {
                          if (value != null) {
                            _submit(
                              TaskEditCommand.setCompletion(task.taskId, value),
                            );
                          }
                        }
                      : null,
                ),
              const SizedBox(width: 5),
              Expanded(child: Text(task.text)),
            ],
          ),
          Align(
            alignment: Alignment.centerRight,
            child: Wrap(
              alignment: WrapAlignment.end,
              children: [
                IconButton(
                  key: ValueKey('task-rename-${task.taskId}'),
                  tooltip: l.mainTaskRename,
                  onPressed: enabled ? () => _rename(task) : null,
                  icon: const Icon(Icons.edit_outlined),
                ),
                IconButton(
                  key: ValueKey('task-up-${task.taskId}'),
                  tooltip: l.mainTaskMoveUp,
                  onPressed: enabled && index > 0
                      ? () => _reorder(index, -1)
                      : null,
                  icon: const Icon(Icons.arrow_upward),
                ),
                IconButton(
                  key: ValueKey('task-down-${task.taskId}'),
                  tooltip: l.mainTaskMoveDown,
                  onPressed: enabled && index < widget.view.tasks.length - 1
                      ? () => _reorder(index, 1)
                      : null,
                  icon: const Icon(Icons.arrow_downward),
                ),
                IconButton(
                  key: ValueKey('task-remove-${task.taskId}'),
                  tooltip: l.mainDelete,
                  onPressed: enabled ? () => _remove(task) : null,
                  icon: const Icon(Icons.delete_outline),
                ),
              ],
            ),
          ),
          if (task.needsExplicitDecision) ...[
            Text(l.mainTaskAmbiguousDecision),
            Wrap(
              children: [
                TextButton(
                  key: ValueKey('task-ambiguous-complete-${task.taskId}'),
                  onPressed: enabled
                      ? () => _submit(
                          TaskEditCommand.setCompletion(task.taskId, true),
                        )
                      : null,
                  child: Text(l.mainTaskMarkComplete),
                ),
                TextButton(
                  key: ValueKey('task-ambiguous-incomplete-${task.taskId}'),
                  onPressed: enabled
                      ? () => _submit(
                          TaskEditCommand.setCompletion(task.taskId, false),
                        )
                      : null,
                  child: Text(l.mainTaskMarkIncomplete),
                ),
              ],
            ),
          ],
        ],
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final l = L10n.of(context);
    if (widget.view.formatVersion == 1) {
      return Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            l.mainProgress(widget.view.completeCount, widget.view.tasks.length),
          ),
          Text(l.mainTaskLegacyReadOnly),
          for (final task in widget.view.tasks)
            _legacyRow(task as LegacyIdeaTaskView),
        ],
      );
    }
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          l.mainTaskProgressThreeWay(
            widget.view.ambiguousCount,
            widget.view.completeCount,
            widget.view.incompleteCount,
          ),
        ),
        if (_retryCommand != null) ...[
          Text(l.mainSaveUnknown),
          TextButton(
            key: const ValueKey('task-retry-original'),
            onPressed: _canRetry
                ? () => _submit(_retryCommand!, retry: true)
                : null,
            child: Text(l.mainRetry),
          ),
        ],
        for (var i = 0; i < widget.view.tasks.length; i++)
          _identifiedRow(widget.view.tasks[i] as IdentifiedIdeaTaskView, i),
        TextField(
          key: const ValueKey('task-add-input'),
          controller: _newTask,
          enabled: _canEdit,
          decoration: InputDecoration(labelText: l.mainTaskTextPrompt),
          onSubmitted: (_) => _add(),
        ),
        Align(
          alignment: Alignment.centerRight,
          child: TextButton(
            key: const ValueKey('task-add'),
            onPressed: _canEdit ? _add : null,
            child: Text(l.mainTaskAdd),
          ),
        ),
        if (_stageOptions.isNotEmpty) ...[
          SizedBox(
            width: double.infinity,
            child: DropdownButtonFormField<String>(
              key: ValueKey(
                'task-stage-select-${widget.view.id}-${widget.view.category}-${widget.view.revision}',
              ),
              isExpanded: true,
              initialValue: _validStage ? _selectedStage : null,
              decoration: InputDecoration(labelText: l.mainTaskStagePrompt),
              items: [
                for (final stage in _stageOptions)
                  DropdownMenuItem(
                    value: WorkbenchV1.stage(stage),
                    child: Text(
                      WorkbenchLabelsScope.of(
                        context,
                      ).filter(StageFilter(stage)),
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                    ),
                  ),
              ],
              onChanged: _canEdit
                  ? (value) {
                      if (value != null) {
                        setState(() => _selectedStage = value);
                      }
                    }
                  : null,
            ),
          ),
          SizedBox(
            width: double.infinity,
            child: TextButton(
              key: const ValueKey('task-set-stage'),
              onPressed: _canEdit && _validStage ? _setStage : null,
              child: Text(l.mainTaskSetStage, textAlign: TextAlign.center),
            ),
          ),
          SizedBox(
            width: double.infinity,
            child: TextButton(
              key: const ValueKey('task-complete-all-stage'),
              onPressed: _canEdit && _validStage ? _completeAll : null,
              child: Text(
                l.mainTaskCompleteAllAndSetStage,
                textAlign: TextAlign.center,
              ),
            ),
          ),
        ],
      ],
    );
  }
}

class _RenameTaskDialog extends StatefulWidget {
  const _RenameTaskDialog({required this.task, required this.onSubmit});
  final IdentifiedIdeaTaskView task;
  final Future<bool> Function(String text) onSubmit;

  @override
  State<_RenameTaskDialog> createState() => _RenameTaskDialogState();
}

class _RenameTaskDialogState extends State<_RenameTaskDialog> {
  late final _controller = TextEditingController(text: widget.task.text);
  bool _submitting = false;

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  Future<void> _save() async {
    final text = _controller.text.trim();
    if (_submitting || text.isEmpty || text == widget.task.text) return;
    setState(() => _submitting = true);
    final saved = await widget.onSubmit(text);
    if (!mounted) return;
    if (saved) {
      Navigator.of(context).pop();
    } else {
      setState(() => _submitting = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final l = L10n.of(context);
    return AlertDialog(
      title: Text(l.mainTaskRename),
      content: TextField(
        controller: _controller,
        autofocus: true,
        decoration: InputDecoration(labelText: l.mainTaskTextPrompt),
      ),
      actions: [
        TextButton(
          onPressed: _submitting ? null : () => Navigator.of(context).pop(),
          child: Text(l.visualCancel),
        ),
        TextButton(
          key: ValueKey('task-rename-confirm-${widget.task.taskId}'),
          onPressed: _submitting ? null : _save,
          child: Text(l.mainDone),
        ),
      ],
    );
  }
}
