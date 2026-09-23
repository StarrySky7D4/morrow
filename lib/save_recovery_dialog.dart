import 'package:flutter/material.dart';
import 'package:morrow_i18n/morrow_i18n.dart';
import 'save_recovery.dart';

enum SaveRecoveryResult { resolved, retry }

class SaveRecoveryDialog extends StatefulWidget {
  const SaveRecoveryDialog({super.key, required this.storage});
  final SaveRecoveryStorage storage;
  @override
  State<SaveRecoveryDialog> createState() => _SaveRecoveryDialogState();
}

class _SaveRecoveryDialogState extends State<SaveRecoveryDialog> {
  SaveRecovery? proposal;
  bool busy = true, failed = false;
  @override
  void initState() {
    super.initState();
    _load();
  }

  Future<void> _load() async {
    try {
      final current = await widget.storage.inspectSaveRecovery();
      if (mounted) {
        setState(() {
          proposal = current;
          busy = false;
        });
      }
    } catch (_) {
      if (mounted) {
        setState(() {
          failed = true;
          busy = false;
        });
      }
    }
  }

  Future<void> _resolve(bool abandon) async {
    final observed = proposal;
    if (busy || observed == null) return;
    final l = L10n.of(context);
    if (abandon) {
      final accepted = await showDialog<bool>(
        context: context,
        builder: (context) => AlertDialog(
          title: Text(l.mainSaveRecoveryAbandon),
          content: Text(l.mainSaveRecoveryAbandonConfirm),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(context, false),
              child: Text(l.commonCancel),
            ),
            FilledButton(
              key: const ValueKey('save-recovery-confirm-abandon'),
              onPressed: () => Navigator.pop(context, true),
              child: Text(l.mainSaveRecoveryAbandon),
            ),
          ],
        ),
      );
      if (accepted != true || !mounted) return;
    }
    setState(() {
      busy = true;
      failed = false;
    });
    try {
      await widget.storage.resolveSaveRecovery(observed, abandon: abandon);
      if (mounted) Navigator.pop(context, SaveRecoveryResult.resolved);
    } catch (_) {
      if (mounted) {
        setState(() {
          failed = true;
        });
        await _load();
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    final l = L10n.of(context), current = proposal;
    return AlertDialog(
      title: Text(l.mainSaveRecoveryTitle),
      content: SizedBox(
        width: 420,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            if (busy) const LinearProgressIndicator(),
            if (!busy)
              Text(
                current == null
                    ? l.mainSaveRecoveryEmpty
                    : current.committed
                    ? l.mainSaveRecoveryCommitted
                    : current.conflict
                    ? l.mainSaveRecoveryConflict
                    : l.mainSaveRecoveryPending,
              ),
            if (failed)
              Padding(
                padding: const EdgeInsets.only(top: 12),
                child: Text(l.mainSaveFailed),
              ),
          ],
        ),
      ),
      actions: [
        TextButton(
          onPressed: busy ? null : () => Navigator.pop(context),
          child: Text(l.commonCancel),
        ),
        if (current == null && !failed)
          FilledButton(
            onPressed: busy
                ? null
                : () => Navigator.pop(context, SaveRecoveryResult.retry),
            child: Text(l.mainRetry),
          ),
        if (current != null && !current.committed)
          TextButton(
            key: const ValueKey('save-recovery-abandon'),
            onPressed: busy ? null : () => _resolve(true),
            child: Text(l.mainSaveRecoveryAbandon),
          ),
        if (current != null && !current.conflict)
          FilledButton(
            key: const ValueKey('save-recovery-resolve'),
            onPressed: busy ? null : () => _resolve(false),
            child: Text(l.mainSaveRecoveryResolve),
          ),
      ],
    );
  }
}
