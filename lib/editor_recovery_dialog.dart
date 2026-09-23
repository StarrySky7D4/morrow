import 'package:flutter/material.dart';
import 'package:morrow_i18n/morrow_i18n.dart';
import 'plugins/editor_recovery.dart';

class EditorRecoveryDialog extends StatefulWidget {
  const EditorRecoveryDialog({
    super.key,
    required this.control,
    required this.writable,
    required this.presentCurrent,
    required this.isCurrent,
  });
  final WorkbenchEditorRecovery control;
  final bool writable;
  final Future<void> Function(String id) presentCurrent;
  final bool Function() isCurrent;

  @override
  State<EditorRecoveryDialog> createState() => _EditorRecoveryDialogState();
}

class _EditorRecoveryDialogState extends State<EditorRecoveryDialog> {
  List<EditorRecovery> _entries = const [];
  bool _busy = true, _failed = false;
  @override
  void initState() {
    super.initState();
    _load();
  }

  void _checkCurrent() {
    if (!mounted || !widget.isCurrent()) {
      throw StateError('Recovery workspace changed');
    }
  }

  Future<void> _load() async {
    try {
      _checkCurrent();
      final entries = await widget.control.inspectEditorRecoveries();
      _checkCurrent();
      if (mounted) {
        setState(() {
          _entries = entries;
          _busy = false;
        });
      }
    } catch (_) {
      if (mounted) {
        setState(() {
          _busy = false;
          _failed = true;
        });
      }
    }
  }

  Future<void> _resolve(EditorRecovery observed, {bool abandon = false}) async {
    if (_busy) return;
    setState(() {
      _busy = true;
      _failed = false;
    });
    try {
      _checkCurrent();
      if (abandon) {
        if (!widget.writable ||
            observed.status == EditorRecoveryStatus.committed) {
          throw StateError('Recovery cannot be abandoned');
        }
        final l = L10n.of(context);
        final confirmed = await showDialog<bool>(
          context: context,
          builder: (context) => AlertDialog(
            title: Text(l.mainEditorRecoveryAbandonTitle),
            content: Text(l.mainEditorRecoveryAbandonBody),
            actions: [
              TextButton(
                onPressed: () => Navigator.pop(context, false),
                child: Text(l.commonCancel),
              ),
              FilledButton(
                key: const ValueKey('editor-recovery-abandon-confirm'),
                onPressed: () => Navigator.pop(context, true),
                child: Text(l.mainEditorRecoveryAbandonTitle),
              ),
            ],
          ),
        );
        _checkCurrent();
        if (confirmed != true) {
          setState(() => _busy = false);
          return;
        }
      }
      final rows = await widget.control.inspectEditorRecoveries(
        id: observed.id,
      );
      _checkCurrent();
      if (rows.length != 1) throw StateError('Recovery changed');
      final current = rows.single;
      if (current.id != observed.id ||
          current.operation != observed.operation ||
          current.sourceRevision != observed.sourceRevision ||
          current.digest.length != observed.digest.length ||
          !List.generate(
            current.digest.length,
            (i) => current.digest[i] == observed.digest[i],
          ).every((v) => v)) {
        throw StateError('Recovery changed');
      }
      if (abandon) {
        if (current.status == EditorRecoveryStatus.committed) {
          throw StateError('Committed recovery cannot be abandoned');
        }
        await widget.control.abandonEditorRecovery(current);
        await _load();
        return;
      }
      if (current.status == EditorRecoveryStatus.conflict) {
        throw StateError('Recovery conflict');
      }
      if (current.status == EditorRecoveryStatus.pending) {
        if (!widget.writable) throw StateError('Recovery is read only');
        await widget.control.resumeEditorRecovery(current);
      }
      _checkCurrent();
      await widget.presentCurrent(current.id);
      _checkCurrent();
      await widget.control.acknowledgeEditorRecovery(current);
      await _load();
    } catch (_) {
      if (mounted) {
        setState(() {
          _failed = true;
        });
      }
      await _load();
    }
  }

  @override
  Widget build(BuildContext context) {
    final l = L10n.of(context);
    return AlertDialog(
      title: Text(l.mainEditorRecoveryTitle),
      content: SizedBox(
        width: 440,
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxHeight: 460),
          child: SingleChildScrollView(
            child: Column(
              mainAxisSize: MainAxisSize.min,
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                if (_busy) const LinearProgressIndicator(),
                if (_failed)
                  Padding(
                    padding: const EdgeInsets.only(bottom: 12),
                    child: Text(l.mainEditorRecoveryFailed),
                  ),
                if (!_busy && _entries.isEmpty) Text(l.mainEditorRecoveryEmpty),
                for (final item in _entries)
                  Padding(
                    padding: const EdgeInsets.only(bottom: 18),
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.stretch,
                      children: [
                        Text(
                          item.title,
                          maxLines: 2,
                          overflow: TextOverflow.ellipsis,
                          style: Theme.of(context).textTheme.titleSmall,
                        ),
                        const SizedBox(height: 8),
                        Text(switch (item.status) {
                          EditorRecoveryStatus.pending =>
                            l.mainEditorRecoveryPending,
                          EditorRecoveryStatus.committed =>
                            l.mainEditorRecoveryCommitted,
                          EditorRecoveryStatus.conflict =>
                            l.mainEditorRecoveryConflict,
                        }),
                        const SizedBox(height: 8),
                        OutlinedButton(
                          key: ValueKey('editor-recovery-resolve-${item.id}'),
                          onPressed:
                              _busy ||
                                  item.status ==
                                      EditorRecoveryStatus.conflict ||
                                  (item.status ==
                                          EditorRecoveryStatus.pending &&
                                      !widget.writable)
                              ? null
                              : () => _resolve(item),
                          child: Text(
                            item.status == EditorRecoveryStatus.committed
                                ? l.mainEditorRecoveryConfirm
                                : l.mainEditorRecoveryResume,
                          ),
                        ),
                        if (item.status != EditorRecoveryStatus.committed &&
                            widget.writable)
                          TextButton(
                            key: ValueKey('editor-recovery-abandon-${item.id}'),
                            onPressed: _busy
                                ? null
                                : () => _resolve(item, abandon: true),
                            child: Text(l.mainEditorRecoveryAbandonTitle),
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
          onPressed: _busy
              ? null
              : () {
                  setState(() {
                    _busy = true;
                    _failed = false;
                  });
                  _load();
                },
          child: Text(l.mainRetry),
        ),
        TextButton(
          onPressed: () => Navigator.pop(context),
          child: Text(l.commonCancel),
        ),
      ],
    );
  }
}
