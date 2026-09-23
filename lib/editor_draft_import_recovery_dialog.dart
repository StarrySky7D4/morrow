import 'package:flutter/material.dart';
import 'package:morrow_i18n/morrow_i18n.dart';

import 'plugins/editor_draft_import.dart';
import 'plugins/editor_draft_import_decision_coordinator.dart';

/// Reviews durable attachment abandonment decisions without submitting work
/// during discovery or when the dialog is dismissed.
class EditorDraftImportRecoveryDialog extends StatefulWidget {
  const EditorDraftImportRecoveryDialog({
    super.key,
    required this.coordinator,
    required this.writable,
    required this.isCurrent,
  });

  final EditorDraftImportDecisionCoordinator coordinator;
  final bool writable;
  final bool Function() isCurrent;

  @override
  State<EditorDraftImportRecoveryDialog> createState() =>
      _EditorDraftImportRecoveryDialogState();
}

class _EditorDraftImportRecoveryDialogState
    extends State<EditorDraftImportRecoveryDialog> {
  List<EditorDraftImportDecision> _decisions = const [];
  bool _busy = true;
  bool _failed = false;
  bool _confirmedRefreshFailed = false;
  int _serial = 0;

  @override
  void initState() {
    super.initState();
    _load();
  }

  @override
  void dispose() {
    ++_serial;
    widget.coordinator.dispose();
    super.dispose();
  }

  bool get _current {
    if (!mounted) return false;
    try {
      return widget.isCurrent();
    } catch (_) {
      return false;
    }
  }

  Future<void> _load({bool preserveFailure = false}) async {
    if (!_current) return;
    final serial = ++_serial;
    setState(() {
      _busy = true;
      if (!preserveFailure) {
        _failed = false;
      }
    });
    try {
      final decisions = await widget.coordinator.refresh();
      if (!_current || serial != _serial) return;
      setState(() {
        _decisions = decisions;
        _busy = false;
        _confirmedRefreshFailed = false;
      });
    } catch (_) {
      if (!_current || serial != _serial) return;
      setState(() {
        _busy = false;
        _failed = true;
      });
    }
  }

  Future<void> _act(
    EditorDraftImportDecision observed, {
    required bool cancel,
  }) async {
    if (_busy || _confirmedRefreshFailed || !widget.writable || !_current) {
      return;
    }
    if (cancel) {
      if (observed.status != EditorDraftImportDecisionStatus.pending &&
          observed.status != EditorDraftImportDecisionStatus.conflict) {
        return;
      }
      final l = L10n.of(context);
      final accepted = await showDialog<bool>(
        context: context,
        builder: (context) => AlertDialog(
          title: Text(l.mainDraftImportRecoveryCancelTitle),
          content: Text(l.mainDraftImportRecoveryCancelBody),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(context, false),
              child: Text(l.commonCancel),
            ),
            FilledButton(
              key: const ValueKey('draft-import-recovery-cancel-confirm'),
              onPressed: () => Navigator.pop(context, true),
              child: Text(l.mainDraftImportRecoveryCancelDecision),
            ),
          ],
        ),
      );
      if (accepted != true || !_current) return;
    } else if (observed.status != EditorDraftImportDecisionStatus.pending) {
      return;
    }

    setState(() {
      _busy = true;
      _failed = false;
      _confirmedRefreshFailed = false;
    });
    try {
      if (cancel) {
        await widget.coordinator.cancel(observed);
      } else {
        await widget.coordinator.retry(observed);
      }
      if (!_current) return;
      setState(() {
        _decisions = widget.coordinator.decisions;
        _busy = false;
      });
    } catch (error) {
      if (!_current) return;
      if (error is EditorDraftImportDecisionRefreshFailure) {
        setState(() {
          _busy = false;
          _confirmedRefreshFailed = true;
        });
        return;
      }
      setState(() => _failed = true);
      await _load(preserveFailure: true);
    }
  }

  @override
  Widget build(BuildContext context) {
    final l = L10n.of(context);
    return AlertDialog(
      title: Text(l.mainDraftImportRecoveryTitle),
      content: SizedBox(
        width: 420,
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxHeight: 460),
          child: SingleChildScrollView(
            child: Column(
              mainAxisSize: MainAxisSize.min,
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                if (_busy) const LinearProgressIndicator(),
                if (_confirmedRefreshFailed)
                  Padding(
                    padding: const EdgeInsets.only(bottom: 12),
                    child: Text(
                      l.mainDraftImportRecoveryConfirmedRefreshFailed,
                    ),
                  ),
                if (_failed)
                  Padding(
                    padding: const EdgeInsets.only(bottom: 12),
                    child: Text(l.mainDraftImportRecoveryFailed),
                  ),
                if (!_busy && _decisions.isEmpty)
                  Text(l.mainDraftImportRecoveryEmpty),
                for (final item in _decisions)
                  Padding(
                    padding: const EdgeInsets.only(bottom: 18),
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.stretch,
                      children: [
                        Text(
                          item.request.name,
                          maxLines: 2,
                          overflow: TextOverflow.ellipsis,
                          style: Theme.of(context).textTheme.titleSmall,
                        ),
                        const SizedBox(height: 8),
                        Text(switch (item.status) {
                          EditorDraftImportDecisionStatus.pending =>
                            l.mainDraftImportRecoveryPending,
                          EditorDraftImportDecisionStatus.committed =>
                            l.mainDraftImportRecoveryCommitted,
                          EditorDraftImportDecisionStatus.cancelled =>
                            l.mainDraftImportRecoveryCancelled,
                          EditorDraftImportDecisionStatus.conflict =>
                            l.mainDraftImportRecoveryConflict,
                        }),
                        if (item.status ==
                            EditorDraftImportDecisionStatus.pending)
                          OutlinedButton(
                            key: ValueKey(
                              'draft-import-recovery-retry-${item.operation}',
                            ),
                            onPressed:
                                _busy ||
                                    _confirmedRefreshFailed ||
                                    !widget.writable ||
                                    !_current
                                ? null
                                : () => _act(item, cancel: false),
                            child: Text(l.mainDraftImportRecoveryRetry),
                          ),
                        if ((item.status ==
                                EditorDraftImportDecisionStatus.pending ||
                            item.status ==
                                EditorDraftImportDecisionStatus.conflict))
                          TextButton(
                            key: ValueKey(
                              'draft-import-recovery-cancel-${item.operation}',
                            ),
                            onPressed:
                                _busy ||
                                    _confirmedRefreshFailed ||
                                    !widget.writable ||
                                    !_current
                                ? null
                                : () => _act(item, cancel: true),
                            child: Text(
                              l.mainDraftImportRecoveryCancelDecision,
                            ),
                          ),
                      ],
                    ),
                  ),
              ],
            ),
          ),
        ),
      ),
      actions: [
        TextButton(
          key: const ValueKey('draft-import-recovery-refresh'),
          onPressed: _busy || !_current ? null : () => _load(),
          child: Text(l.mainRetry),
        ),
        TextButton(
          onPressed: () => Navigator.pop(context),
          child: Text(l.mainDraftImportRecoveryClose),
        ),
      ],
    );
  }
}
