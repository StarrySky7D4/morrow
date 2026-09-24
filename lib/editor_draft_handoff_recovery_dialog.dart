import 'package:flutter/material.dart';
import 'package:morrow_i18n/morrow_i18n.dart';

import 'plugins/editor_draft.dart';
import 'plugins/editor_draft_handoff_coordinator.dart';

/// The workspace owns the coordinator. Dismissing this view neither cancels
/// a sent operation nor releases the only locally held uncertain identity.
class EditorDraftHandoffRecoveryDialog extends StatefulWidget {
  const EditorDraftHandoffRecoveryDialog({
    super.key,
    required this.coordinator,
    required this.isCurrent,
    required this.canWrite,
  });
  final EditorDraftHandoffCoordinator coordinator;
  final bool Function() isCurrent, canWrite;

  @override
  State<EditorDraftHandoffRecoveryDialog> createState() =>
      _EditorDraftHandoffRecoveryDialogState();
}

class _EditorDraftHandoffRecoveryDialogState
    extends State<EditorDraftHandoffRecoveryDialog> {
  bool _confirming = false;
  DialogRoute<bool>? _confirmationRoute;
  bool get _current {
    try {
      return mounted && widget.isCurrent();
    } catch (_) {
      return false;
    }
  }

  bool get _writable {
    try {
      return _current && widget.canWrite();
    } catch (_) {
      return false;
    }
  }

  @override
  void initState() {
    super.initState();
    // Discovery is read-only; an already running workspace operation survives
    // closing/reopening this view and must not be started a second time.
    if (!widget.coordinator.busy) {
      _run(widget.coordinator.refresh);
    }
  }

  @override
  void dispose() {
    final route = _confirmationRoute;
    _confirmationRoute = null;
    if (route != null) {
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (route.isActive) route.navigator?.removeRoute(route, false);
      });
    }
    super.dispose();
  }

  Future<void> _run(Future<Object?> Function() action) async {
    if (!_current) return;
    try {
      await action();
    } catch (_) {
      // The coordinator retains the typed result and uncertainty. Raw errors
      // may contain host paths and are not suitable for this surface.
    }
  }

  String _status(AppLocalizations l, EditorDraftHandoffProposalStatus status) =>
      switch (status) {
        EditorDraftHandoffProposalStatus.pending =>
          l.mainDraftHandoffRecoveryPending,
        EditorDraftHandoffProposalStatus.childCommitted =>
          l.mainDraftHandoffRecoveryChildCommitted,
        EditorDraftHandoffProposalStatus.parentRetired =>
          l.mainDraftHandoffRecoveryParentRetired,
        EditorDraftHandoffProposalStatus.cancelled =>
          l.mainDraftHandoffRecoveryCancelled,
        EditorDraftHandoffProposalStatus.conflict =>
          l.mainDraftHandoffRecoveryConflict,
      };

  Future<void> _confirm(
    EditorDraftHandoffProposalRecord observed,
    EditorDraftHandoffAction action,
  ) async {
    if (!_writable ||
        widget.coordinator.busy ||
        _confirming ||
        widget.coordinator.uncertain) {
      return;
    }
    setState(() => _confirming = true);
    try {
      final l = L10n.of(context);
      final body = switch (action) {
        EditorDraftHandoffAction.complete =>
          l.mainDraftHandoffRecoveryCompleteBody,
        EditorDraftHandoffAction.retire => l.mainDraftHandoffRecoveryRetireBody,
        EditorDraftHandoffAction.cancel => l.mainDraftHandoffRecoveryCancelBody,
      };
      final label = switch (action) {
        EditorDraftHandoffAction.complete => l.mainDraftHandoffRecoveryComplete,
        EditorDraftHandoffAction.retire => l.mainDraftHandoffRecoveryRetire,
        EditorDraftHandoffAction.cancel => l.mainDraftHandoffRecoveryCancel,
      };
      final confirmation = DialogRoute<bool>(
        context: context,
        builder: (context) => AlertDialog(
          title: Text(l.mainDraftHandoffRecoveryConfirmTitle),
          content: Text(body),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(context, false),
              child: Text(l.commonCancel),
            ),
            FilledButton(
              key: const ValueKey('draft-handoff-confirm'),
              onPressed: () => Navigator.pop(context, true),
              child: Text(label),
            ),
          ],
        ),
      );
      _confirmationRoute = confirmation;
      final accepted = await Navigator.of(
        context,
        rootNavigator: true,
      ).push(confirmation);
      if (identical(_confirmationRoute, confirmation)) {
        _confirmationRoute = null;
      }
      if (accepted == true && _writable) {
        await _run(() => widget.coordinator.act(observed, action));
      }
    } finally {
      if (mounted) setState(() => _confirming = false);
    }
  }

  Widget _field(String label, String value) => Padding(
    padding: const EdgeInsets.only(bottom: 12),
    child: Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Text(label, style: Theme.of(context).textTheme.labelLarge),
        const SizedBox(height: 4),
        // Preserve the original value; the bounded viewport scrolls long fields
        // without truncating the recovery preview or rendering Markdown links.
        ConstrainedBox(
          constraints: const BoxConstraints(maxHeight: 120),
          child: SingleChildScrollView(child: SelectableText(value)),
        ),
      ],
    ),
  );

  Widget _detail(AppLocalizations l, EditorDraftHandoffProposalRecord record) {
    final c = widget.coordinator;
    final raw = record.proposal.handoff.request.values;
    final status = record.summary.status;
    final enabled = _writable && !c.busy && !_confirming && !c.uncertain;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Text(_status(l, status), style: Theme.of(context).textTheme.titleSmall),
        const SizedBox(height: 10),
        Wrap(
          spacing: 8,
          runSpacing: 8,
          children: [
            if (status == EditorDraftHandoffProposalStatus.pending)
              FilledButton.tonal(
                key: const ValueKey('draft-handoff-complete'),
                onPressed: enabled
                    ? () => _confirm(record, EditorDraftHandoffAction.complete)
                    : null,
                child: Text(l.mainDraftHandoffRecoveryComplete),
              ),
            if (status == EditorDraftHandoffProposalStatus.childCommitted)
              FilledButton.tonal(
                key: const ValueKey('draft-handoff-retire'),
                onPressed: enabled
                    ? () => _confirm(record, EditorDraftHandoffAction.retire)
                    : null,
                child: Text(l.mainDraftHandoffRecoveryRetire),
              ),
            if (status == EditorDraftHandoffProposalStatus.pending ||
                status == EditorDraftHandoffProposalStatus.conflict)
              OutlinedButton(
                key: const ValueKey('draft-handoff-cancel'),
                onPressed: enabled
                    ? () => _confirm(record, EditorDraftHandoffAction.cancel)
                    : null,
                child: Text(l.mainDraftHandoffRecoveryCancel),
              ),
          ],
        ),
        const SizedBox(height: 16),
        Text(
          l.mainDraftHandoffRecoveryFields,
          style: Theme.of(context).textTheme.titleMedium,
        ),
        const SizedBox(height: 8),
        _field(l.mainIdeaNameHint, raw.title.text),
        _field(l.mainMarkdownBody, raw.description.text),
        _field(l.mainHypothesisPrompt, raw.hypothesis.text),
        _field(l.mainObservationsPrompt, raw.conclusion.text),
        _field(l.mainTodosPrompt, raw.todos.text),
        _field(l.mainCategoryPrompt, raw.category),
        _field(l.mainTaskStagePrompt, raw.stage),
        Text(
          l.mainDraftHandoffRecoveryAssets,
          style: Theme.of(context).textTheme.titleSmall,
        ),
        for (final asset in record.proposal.handoff.request.assets)
          Padding(
            padding: const EdgeInsets.only(top: 6),
            child: SelectableText(
              asset.aliases.isEmpty ? asset.assetId : asset.aliases.join(' · '),
            ),
          ),
      ],
    );
  }

  @override
  Widget build(BuildContext context) => AnimatedBuilder(
    animation: widget.coordinator,
    builder: (context, _) {
      final l = L10n.of(context);
      final c = widget.coordinator;
      final blocked = c.busy || _confirming || !_current;
      return AlertDialog(
        title: Text(l.mainDraftHandoffRecoveryTitle),
        content: SizedBox(
          width: 620,
          height: (MediaQuery.sizeOf(context).height * .65).clamp(180, 620),
          child: CustomScrollView(
            slivers: [
              SliverToBoxAdapter(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
                    Text(l.mainDraftHandoffRecoveryIntro),
                    const SizedBox(height: 12),
                    if (!_writable) Text(l.mainDraftHandoffRecoveryReadOnly),
                    if (c.busy) const LinearProgressIndicator(),
                    if (c.uncertain)
                      Text(l.mainDraftHandoffRecoveryUnknown)
                    else if (c.lastError != null)
                      Text(l.mainDraftHandoffRecoveryFailed)
                    else if (c.lastConfirmed != null)
                      Text(l.mainDraftHandoffRecoveryConfirmed),
                    if (!c.busy && c.summaries.isEmpty)
                      Text(l.mainDraftHandoffRecoveryEmpty),
                  ],
                ),
              ),
              SliverList.builder(
                itemCount: c.summaries.length,
                itemBuilder: (context, index) {
                  final item = c.summaries[index];
                  return Padding(
                    key: ValueKey(item.childOperation),
                    padding: const EdgeInsets.only(top: 12),
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.stretch,
                      children: [
                        Text(
                          item.cardId,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                        ),
                        Text(_status(l, item.status)),
                        Align(
                          alignment: AlignmentDirectional.centerStart,
                          child: TextButton(
                            key: ValueKey(
                              'draft-handoff-inspect-${item.childOperation}',
                            ),
                            onPressed: blocked
                                ? null
                                : () => _run(() => c.inspect(item)),
                            child: Text(l.mainDraftHandoffRecoveryInspect),
                          ),
                        ),
                      ],
                    ),
                  );
                },
              ),
              SliverToBoxAdapter(
                child: Padding(
                  padding: const EdgeInsets.only(top: 16),
                  child: c.selected == null
                      ? Text(l.mainDraftHandoffRecoverySelection)
                      : _detail(l, c.selected!),
                ),
              ),
            ],
          ),
        ),
        actions: [
          TextButton(
            key: const ValueKey('draft-handoff-refresh'),
            onPressed: blocked ? null : () => _run(c.refresh),
            child: Text(l.mainRetry),
          ),
          TextButton(
            key: const ValueKey('draft-handoff-close'),
            onPressed: () => Navigator.pop(context),
            child: Text(l.mainDraftImportRecoveryClose),
          ),
        ],
      );
    },
  );
}
