import 'dart:async';
import 'package:flutter/material.dart';
import 'package:morrow_i18n/morrow_i18n.dart';
import 'plugins/editor_draft.dart';
import 'plugins/editor_draft_review.dart';

/// Discovery and selection only. This route does not create a live editor or
/// decide whether a previous business operation may be submitted again.
class EditorDraftRecoveryDialog extends StatefulWidget {
  const EditorDraftRecoveryDialog({
    super.key,
    required this.runtime,
    required this.isCurrent,
  });
  final WorkbenchEditorDraftRecovery runtime;
  final bool Function() isCurrent;
  @override
  State<EditorDraftRecoveryDialog> createState() =>
      _EditorDraftRecoveryDialogState();
}

class _EditorDraftRecoveryDialogState extends State<EditorDraftRecoveryDialog> {
  List<EditorDraftRecord> _records = const [];
  bool _busy = false, _failed = false;
  Object? lastError;
  bool get current => mounted && widget.isCurrent();
  @override
  void initState() {
    super.initState();
    unawaited(_refresh());
  }

  Future<void> _refresh() async {
    if (!current || _busy) return;
    setState(() {
      _busy = true;
      _failed = false;
      lastError = null;
    });
    try {
      final summaries = await widget.runtime.editorDrafts.list();
      final records = <EditorDraftRecord>[];
      for (final row in summaries.where((row) => row.active)) {
        if (!current) return;
        final record = await widget.runtime.editorDrafts.read(
          row.cardId,
          row.draftId,
        );
        if (record == null ||
            !record.currentActive ||
            record.generation != row.generation ||
            record.currentGeneration != row.generation) {
          throw StateError('Draft inventory changed during review');
        }
        records.add(record);
      }
      if (current) setState(() => _records = List.unmodifiable(records));
    } catch (error) {
      if (current) {
        setState(() {
          lastError = error;
          _failed = true;
        });
      }
    } finally {
      if (current) setState(() => _busy = false);
    }
  }

  Future<void> _inspect(EditorDraftRecord record) async {
    if (!current || _busy) return;
    setState(() {
      _busy = true;
      _failed = false;
      lastError = null;
    });
    try {
      final review = await widget.runtime.inspectEditorDraft(
        EditorDraftSummary(
          cardId: record.request.cardId,
          draftId: record.request.draftId,
          generation: record.generation,
          active: record.currentActive,
        ),
      );
      if (mounted && current) Navigator.pop(context, review);
    } catch (error) {
      if (current) {
        setState(() {
          lastError = error;
          _failed = true;
        });
      }
    } finally {
      if (current) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final l = L10n.of(context);
    return AlertDialog(
      title: Text(l.mainDraftJournalTitle),
      content: SizedBox(
        width: 600,
        height: 440,
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Text(l.mainDraftJournalReview),
            if (_busy) const LinearProgressIndicator(),
            if (_failed) Text(l.mainDraftHandoffRecoveryFailed),
            Expanded(
              child: ListView(
                children: [
                  if (!_busy && !_failed && _records.isEmpty)
                    Text(l.mainDraftJournalEmpty),
                  for (final record in _records)
                    ListTile(
                      key: ValueKey('draft-journal-${record.request.draftId}'),
                      title: Text(
                        record.request.values.title.text.isEmpty
                            ? l.mainIdeaNameHint
                            : record.request.values.title.text,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                      ),
                      subtitle: Text(
                        record.request.values.description.text,
                        maxLines: 2,
                        overflow: TextOverflow.ellipsis,
                      ),
                      trailing: const Icon(Icons.visibility_outlined),
                      onTap: _busy ? null : () => _inspect(record),
                    ),
                ],
              ),
            ),
          ],
        ),
      ),
      actions: [
        TextButton(
          onPressed: _busy ? null : _refresh,
          child: Text(l.pluginsRefreshList),
        ),
        TextButton(
          onPressed: () => Navigator.pop(context),
          child: Text(l.mainDraftImportRecoveryClose),
        ),
      ],
    );
  }
}
