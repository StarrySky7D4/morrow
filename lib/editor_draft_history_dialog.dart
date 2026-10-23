import 'dart:async';
import 'package:file_selector/file_selector.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:morrow_i18n/morrow_i18n.dart';
import 'plugins/editor_draft.dart';

/// Maintenance is limited to already retired records. Active owners and their
/// attachments are managed by the original editor retirement flow.
class EditorDraftHistoryDialog extends StatefulWidget {
  const EditorDraftHistoryDialog({
    super.key,
    required this.control,
    required this.history,
    required this.isCurrent,
  });
  final EditorDraftControl control;
  final EditorDraftHistoryControl history;
  final bool Function() isCurrent;
  @override
  State<EditorDraftHistoryDialog> createState() =>
      _EditorDraftHistoryDialogState();
}

class _EditorDraftHistoryDialogState extends State<EditorDraftHistoryDialog> {
  EditorDraftHistoryPage? _page;
  EditorDraftRecord? _selected;
  EditorDraftArchiveReceipt? _archiveReceipt;
  EditorDraftArchiveMaintenanceReceipt? _maintenance;
  bool _maintenanceSubmitted = false;
  EditorDraftArchiveMaintenanceControl? get _maintenanceControl =>
      widget.history is EditorDraftArchiveMaintenanceControl
      ? widget.history as EditorDraftArchiveMaintenanceControl
      : null;
  String _cursor = '', _archiveOperation = '', _exportedPath = '';
  bool _busy = false, _failed = false, _archiveSubmitted = false;
  Object? lastError;
  bool get current => mounted && widget.isCurrent();

  @override
  void initState() {
    super.initState();
    unawaited(_refresh(''));
  }

  Future<void> _run(Future<void> Function() action) async {
    if (!current || _busy) return;
    setState(() {
      _busy = true;
      _failed = false;
      lastError = null;
    });
    try {
      await action();
    } catch (error) {
      if (current) {
        setState(() {
          _failed = true;
          lastError = error;
        });
      }
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _refresh(String cursor) => _run(() async {
    final page = await widget.history.history(cursor: cursor);
    if (!current) return;
    final selected = _selected;
    if (selected != null) {
      final archive = await widget.history.inspectArchive(
        selected.request.cardId,
        selected.request.draftId,
      );
      if (!current) return;
      if (archive != null &&
          (archive.generation != selected.generation ||
              !listEquals(archive.requestSha256, selected.requestSha256))) {
        throw StateError('Archive inspection changed the selected retirement');
      }
      _archiveOperation = archive?.operation ?? '';
      _archiveReceipt = archive;
      final control = _maintenanceControl;
      if (archive?.archiveVersion == 1 && control != null) {
        final receipt = await control.inspectArchiveMaintenance(
          selected.request.cardId,
          selected.request.draftId,
        );
        if (!current) return;
        if (receipt != null) _checkMaintenance(receipt, archive!);
        _maintenance = receipt;
      }
    }
    setState(() {
      _page = page;
      _cursor = cursor;
    });
  });

  Future<void> _select(EditorDraftHistoryRow row) => _run(() async {
    final record = await widget.control.read(row.cardId, row.draftId);
    if (!current) return;
    if (record == null ||
        record.active ||
        record.currentActive ||
        record.repeated ||
        record.generation != row.generation ||
        record.currentGeneration != row.generation ||
        !listEquals(record.requestSha256, row.requestSha256)) {
      throw StateError('Retired draft changed during selection');
    }
    final archive = await widget.history.inspectArchive(
      row.cardId,
      row.draftId,
    );
    if (!current) return;
    if ((archive?.operation ?? '') != row.archiveOperation ||
        (archive != null &&
            (archive.cardId != row.cardId ||
                archive.draftId != row.draftId ||
                archive.generation != record.generation ||
                !listEquals(archive.requestSha256, record.requestSha256)))) {
      throw StateError('Selected archive changed during inspection');
    }
    final control = _maintenanceControl;
    final maintenance = archive?.archiveVersion == 1 && control != null
        ? await control.inspectArchiveMaintenance(row.cardId, row.draftId)
        : null;
    if (!current) return;
    if (maintenance != null) _checkMaintenance(maintenance, archive!);
    setState(() {
      _selected = record;
      _archiveOperation = row.archiveOperation;
      _archiveReceipt = archive;
      _maintenance = maintenance;
      _maintenanceSubmitted = false;
      _archiveSubmitted = false;
      _exportedPath = '';
    });
  });

  Future<void> _archive() => _run(() async {
    final original = _selected;
    if (original == null || _archiveOperation.isNotEmpty || _archiveSubmitted) {
      return;
    }
    setState(() => _archiveSubmitted = true);
    final receipt = await widget.history.archive(original);
    if (!current) return;
    if (receipt.generation != original.generation ||
        receipt.cardId != original.request.cardId ||
        receipt.draftId != original.request.draftId ||
        !listEquals(receipt.requestSha256, original.requestSha256)) {
      throw StateError('Archive completion changed selected retirement');
    }
    setState(() {
      _archiveOperation = receipt.operation;
      _archiveReceipt = receipt;
    });
    final page = await widget.history.history(cursor: _cursor);
    if (current) setState(() => _page = page);
  });

  void _checkMaintenance(
    EditorDraftArchiveMaintenanceReceipt receipt,
    EditorDraftArchiveReceipt archive,
  ) {
    if (receipt.cardId != archive.cardId ||
        receipt.draftId != archive.draftId ||
        receipt.generation != archive.generation ||
        receipt.archiveOperation != archive.operation ||
        !listEquals(receipt.archiveSha256, archive.archiveSha256) ||
        !listEquals(receipt.requestSha256, archive.requestSha256)) {
      throw StateError('Maintenance changed the selected archive');
    }
  }

  Future<void> _maintain() => _run(() async {
    final original = _selected,
        archive = _archiveReceipt,
        control = _maintenanceControl;
    if (original == null ||
        archive == null ||
        control == null ||
        archive.archiveVersion != 1 ||
        archive.legacyAuxiliaryCount == 0 ||
        _maintenance != null ||
        _maintenanceSubmitted) {
      return;
    }
    setState(() => _maintenanceSubmitted = true);
    final receipt = await control.maintainArchive(original, archive);
    if (!current) return;
    _checkMaintenance(receipt, archive);
    final page = await widget.history.history(cursor: _cursor);
    if (current) {
      setState(() {
        _maintenance = receipt;
        _page = page;
      });
    }
  });

  Future<void> _export() => _run(() async {
    final original = _selected;
    if (original == null) return;
    final target = await getSaveLocation(
      suggestedName: 'morrow-draft-history.json',
      acceptedTypeGroups: const [
        XTypeGroup(label: 'JSON', extensions: ['json']),
      ],
    );
    if (!current || target == null) return;
    await widget.history.exportHistory(original, target.path);
    if (current) setState(() => _exportedPath = target.path);
  });

  @override
  Widget build(BuildContext context) {
    final l = L10n.of(context), page = _page, selected = _selected;
    final values = selected?.request.values;
    return AlertDialog(
      title: Text(l.mainDraftHistoryTitle),
      content: SizedBox(
        width: 760,
        height: 490,
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Text(l.mainDraftHistoryNotice),
            if (page != null)
              Text(
                l.mainDraftHistoryCapacity(
                  page.unarchivedCount,
                  page.archivedCount,
                ),
              ),
            if (_busy) const LinearProgressIndicator(),
            if (_failed) Text(l.mainDraftHandoffRecoveryFailed),
            if (_archiveSubmitted && _archiveOperation.isEmpty)
              Text(l.mainDraftHistoryUnconfirmed),
            if (_exportedPath.isNotEmpty)
              SelectableText('${l.mainDraftJournalExported}\n$_exportedPath'),
            Expanded(
              child: Row(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  Expanded(
                    child: ListView(
                      children: [
                        if (page != null && page.rows.isEmpty)
                          Text(l.mainDraftJournalEmpty),
                        for (final row
                            in page?.rows ?? <EditorDraftHistoryRow>[])
                          ListTile(
                            key: ValueKey('draft-history-${row.draftId}'),
                            title: Text(
                              row.title.isEmpty
                                  ? l.mainIdeaNameHint
                                  : row.title,
                              maxLines: 1,
                              overflow: TextOverflow.ellipsis,
                            ),
                            subtitle: Text(
                              '${row.archiveOperation.isEmpty ? l.mainDraftHistoryRetired : l.mainDraftHistoryArchived}\n${row.preview}',
                              maxLines: 3,
                              overflow: TextOverflow.ellipsis,
                            ),
                            selected:
                                selected?.request.cardId == row.cardId &&
                                selected?.request.draftId == row.draftId,
                            onTap: _busy || !current
                                ? null
                                : () => _select(row),
                          ),
                      ],
                    ),
                  ),
                  if (values != null)
                    Expanded(
                      child: SingleChildScrollView(
                        child: Column(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            for (final field in [
                              (l.mainDraftHistoryTitleField, values.title.text),
                              (
                                l.mainDraftHistoryDescriptionField,
                                values.description.text,
                              ),
                              (l.mainHypothesis, values.hypothesis.text),
                              (
                                l.mainObservationsPrompt,
                                values.conclusion.text,
                              ),
                              (l.mainTodosPrompt, values.todos.text),
                              (
                                l.mainDraftHistoryCategoryField,
                                values.category,
                              ),
                              (l.mainDraftHistoryStageField, values.stage),
                            ])
                              Padding(
                                padding: const EdgeInsets.symmetric(
                                  vertical: 6,
                                ),
                                child: Column(
                                  crossAxisAlignment: CrossAxisAlignment.start,
                                  children: [
                                    Text(
                                      field.$1,
                                      style: Theme.of(
                                        context,
                                      ).textTheme.labelLarge,
                                    ),
                                    SelectableText(field.$2),
                                  ],
                                ),
                              ),
                            for (final asset in selected!.assets)
                              Text(asset.name),
                            if (_archiveOperation.isNotEmpty)
                              Text(l.mainDraftHistoryArchived),
                            if (_maintenance != null)
                              Text(l.mainDraftHistoryMaintained)
                            else if (_archiveReceipt?.archiveVersion == 1 &&
                                (_archiveReceipt?.legacyAuxiliaryCount ?? 0) >
                                    0)
                              Text(l.mainDraftHistoryMaintainHelp),
                          ],
                        ),
                      ),
                    ),
                ],
              ),
            ),
            Wrap(
              spacing: 8,
              children: [
                TextButton(
                  onPressed: _busy || _cursor.isEmpty
                      ? null
                      : () => _refresh(''),
                  child: Text(l.mainDraftHistoryFirst),
                ),
                TextButton(
                  onPressed: _busy || page == null || page.nextCursor.isEmpty
                      ? null
                      : () => _refresh(page.nextCursor),
                  child: Text(l.mainDraftHistoryNext),
                ),
              ],
            ),
          ],
        ),
      ),
      actions: [
        if (_archiveReceipt?.archiveVersion == 1 &&
            (_archiveReceipt?.legacyAuxiliaryCount ?? 0) > 0 &&
            _maintenanceControl != null)
          TextButton(
            key: const ValueKey('draft-history-maintain'),
            onPressed: _busy || _maintenance != null || _maintenanceSubmitted
                ? null
                : _maintain,
            child: Text(l.mainDraftHistoryMaintain),
          ),
        TextButton(
          onPressed: _busy || selected == null ? null : _export,
          child: Text(l.mainDraftHistoryExport),
        ),
        TextButton(
          key: const ValueKey('draft-history-archive'),
          onPressed:
              _busy ||
                  selected == null ||
                  _archiveSubmitted ||
                  _archiveOperation.isNotEmpty
              ? null
              : _archive,
          child: Text(l.mainDraftHistoryArchive),
        ),
        TextButton(
          onPressed: _busy ? null : () => _refresh(_cursor),
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
