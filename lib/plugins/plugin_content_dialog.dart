import 'dart:convert';

import 'package:material_ui/material_ui.dart';
import 'package:morrow_i18n/morrow_i18n.dart';

import 'generated/host.capnp.dart' as wire;
import 'plugin_content_task.dart';
import 'plugin_library.dart' show PluginLibraryEntry;
import 'workbench_backend.dart' show newQueryOperationId;

class PluginContentDialog extends StatefulWidget {
  const PluginContentDialog({
    super.key,
    required this.backend,
    required this.entry,
    required this.revision,
    required this.isCurrent,
    required this.onCommitted,
  });
  final PluginContentBackend backend;
  final PluginLibraryEntry entry;
  final BigInt revision;
  final bool Function() isCurrent;
  final VoidCallback onCommitted;
  @override
  State<PluginContentDialog> createState() => _PluginContentDialogState();
}

class _PluginContentDialogState extends State<PluginContentDialog> {
  final _card = TextEditingController(),
      _title = TextEditingController(),
      _body = TextEditingController(),
      _preview = TextEditingController(),
      _type = TextEditingController(text: 'org.morrow.sdk.text'),
      _revision = TextEditingController(text: '1'),
      _offset = TextEditingController(text: '0'),
      _operation = TextEditingController();
  late final _actions = PluginContentAction.values
      .where((a) => widget.entry.approved.contains(a.capability))
      .toList();
  late PluginContentAction _action = _actions.first;
  bool _busy = false;
  String? _error;
  PluginContentReceipt? _receipt;
  PluginContentRequest? _uncertain;
  @override
  void dispose() {
    for (final c in [
      _card,
      _title,
      _body,
      _preview,
      _type,
      _revision,
      _offset,
      _operation,
    ]) {
      c.dispose();
    }
    super.dispose();
  }

  String _label(AppLocalizations l, PluginContentAction a) => switch (a) {
    PluginContentAction.create => l.pluginsCreate,
    PluginContentAction.edit => l.pluginsEdit,
    PluginContentAction.read => l.pluginsRead,
    PluginContentAction.lookup => l.pluginsOperation,
  };
  bool get _writing =>
      _action == PluginContentAction.create ||
      _action == PluginContentAction.edit;
  Widget _field(
    String key,
    String label,
    TextEditingController c, {
    bool multiline = false,
  }) => TextField(
    key: ValueKey('content-$key'),
    controller: c,
    enabled: !_busy,
    maxLines: multiline ? 3 : 1,
    decoration: InputDecoration(labelText: label),
  );
  PluginContentRequest _request() => PluginContentRequest(
    action: _action,
    card: _card.text,
    requestId: newQueryOperationId(),
    typeId: _type.text,
    title: _title.text,
    preview: _preview.text,
    revision:
        _action == PluginContentAction.edit ||
            _action == PluginContentAction.read
        ? BigInt.parse(_revision.text)
        : BigInt.zero,
    offset: _action == PluginContentAction.read
        ? BigInt.parse(_offset.text)
        : BigInt.zero,
    body: _writing ? utf8.encode(_body.text) : const [],
    operation: _operation.text,
  );
  String _review(PluginContentRequest q, AppLocalizations l) => [
    widget.entry.name,
    widget.entry.id,
    '${l.pluginsEndpointDigest}\n${widget.entry.digest.map((b) => b.toRadixString(16).padLeft(2, '0')).join()}',
    '${l.pluginsServiceCardId}\n${q.card}',
    _label(l, q.action),
    if (q.action == PluginContentAction.edit ||
        q.action == PluginContentAction.read)
      '${l.pluginsAgentRevision}  ${q.revision}',
    if (q.action == PluginContentAction.read)
      '${l.pluginsCardOffset}  ${q.offset} (+${q.length})',
    if (q.action == PluginContentAction.create)
      '${l.pluginsCardType}  ${q.typeId}',
    if (_writing) ...[
      '${l.mainDraftHistoryTitleField}\n${q.title}',
      '${l.pluginsCardBody}\n${utf8.decode(q.body)}',
    ],
    if (q.action == PluginContentAction.edit)
      '${l.pluginsCardPreview}\n${q.preview}',
    '${l.pluginsCardOperationId}\n${q.action == PluginContentAction.lookup ? q.operation : q.requestId}',
  ].join('\n\n');

  Future<void> _run() async {
    if (_busy) return;
    final l = L10n.of(context);
    if (!widget.isCurrent()) {
      setState(() => _error = l.pluginsSettingsUnknown);
      return;
    }
    late final PluginContentRequest request;
    try {
      request = _request();
    } on FormatException {
      setState(() => _error = l.pluginsCardInvalid);
      return;
    }
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      final yes = await showDialog<bool>(
        context: context,
        builder: (dialog) => AlertDialog(
          title: Text(l.pluginsCardReview),
          content: SizedBox(
            width: 480,
            child: SingleChildScrollView(
              child: SelectableText(_review(request, l)),
            ),
          ),
          actions: [
            TextButton(
              key: const ValueKey('content-cancel'),
              onPressed: () => Navigator.pop(dialog, false),
              child: Text(l.commonCancel),
            ),
            FilledButton(
              key: const ValueKey('content-confirm'),
              onPressed: () => Navigator.pop(dialog, true),
              child: Text(l.pluginsCardRun),
            ),
          ],
        ),
      );
      if (yes != true || !mounted) return;
      if (!widget.isCurrent()) {
        setState(() => _error = l.pluginsSettingsUnknown);
        return;
      }
      final writing =
          request.action == PluginContentAction.create ||
          request.action == PluginContentAction.edit;
      if (writing) {
        _operation.text = request.requestId;
        setState(() {
          _uncertain = request;
          _receipt = null;
        });
      }
      final receipt = await widget.backend.runPluginContent(
        widget.entry,
        widget.revision,
        request,
      );
      if (!mounted || !widget.isCurrent()) return;
      setState(() {
        _receipt = receipt;
        if (writing ||
            (receipt.status == wire.PluginContentStatus.committed &&
                _uncertain?.card == receipt.card &&
                _uncertain?.requestId == receipt.operation)) {
          _uncertain = null;
        }
        if (receipt.status == wire.PluginContentStatus.committed) {
          _revision.text = receipt.revision.toString();
        }
      });
      if (writing && receipt.status == wire.PluginContentStatus.committed) {
        widget.onCommitted();
      }
    } catch (error) {
      if (mounted) {
        setState(() {
          _receipt = null;
          _error = error.toString();
        });
      }
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  String _result(PluginContentReceipt r, AppLocalizations l) {
    final body = switch (r.status) {
      wire.PluginContentStatus.committed =>
        '${l.pluginsCardCommitted}\n${l.pluginsAgentRevision}  ${r.revision}\n${l.pluginsCardOperationId}\n${r.operation}\n${l.pluginsAgentEventId}\n${r.event}',
      wire.PluginContentStatus.chunk =>
        '${l.pluginsRead}  ${r.offset} + ${r.bytes.length} / ${r.totalLength}\n${_displayBytes(r.bytes)}',
      wire.PluginContentStatus.absent =>
        '${l.pluginsCardAbsent}\n${r.operation}',
      _ => '${l.pluginsCardRejected}  ${r.failure}',
    };
    return '${r.card}\n$body';
  }

  String _displayBytes(List<int> bytes) {
    try {
      return utf8.decode(bytes);
    } on FormatException {
      return 'Base64\n${base64.encode(bytes)}';
    }
  }

  @override
  Widget build(BuildContext context) {
    final l = L10n.of(context);
    return PopScope(
      canPop: !_busy,
      child: AlertDialog(
        title: Text(l.pluginsCardTask),
        content: SizedBox(
          width: 500,
          child: SingleChildScrollView(
            child: Column(
              mainAxisSize: MainAxisSize.min,
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(widget.entry.name),
                DropdownButton<PluginContentAction>(
                  key: const ValueKey('content-action'),
                  value: _action,
                  items: _actions
                      .map(
                        (a) => DropdownMenuItem(
                          value: a,
                          child: Text(_label(l, a)),
                        ),
                      )
                      .toList(),
                  onChanged: _busy
                      ? null
                      : (a) {
                          if (a != null) {
                            setState(() {
                              _action = a;
                              _error = null;
                            });
                          }
                        },
                ),
                _field('card', l.pluginsServiceCardId, _card),
                if (_action == PluginContentAction.create)
                  _field('type', l.pluginsCardType, _type),
                if (_action == PluginContentAction.edit ||
                    _action == PluginContentAction.read)
                  _field('revision', l.pluginsAgentRevision, _revision),
                if (_writing) ...[
                  _field('title', l.mainDraftHistoryTitleField, _title),
                  _field('body', l.pluginsCardBody, _body, multiline: true),
                ],
                if (_action == PluginContentAction.edit)
                  _field('preview', l.pluginsCardPreview, _preview),
                if (_action == PluginContentAction.read)
                  _field('offset', l.pluginsCardOffset, _offset),
                if (_action == PluginContentAction.lookup)
                  _field('operation', l.pluginsCardOperationId, _operation),
                if (_uncertain case final q?)
                  SelectableText(
                    '${l.pluginsCardUnknown}\n${q.card}\n${q.requestId}',
                    key: const ValueKey('content-unknown'),
                  ),
                if (_error case final message?)
                  SelectableText(message, key: const ValueKey('content-error')),
                if (_receipt case final r?)
                  SelectableText(
                    _result(r, l),
                    key: const ValueKey('content-result'),
                  ),
              ],
            ),
          ),
        ),
        actions: [
          TextButton(
            onPressed: _busy ? null : () => Navigator.pop(context),
            child: Text(l.visualClose),
          ),
          FilledButton(
            key: const ValueKey('content-review'),
            onPressed: _busy || (_writing && _uncertain != null) ? null : _run,
            child: Text(l.pluginsCardReview),
          ),
        ],
      ),
    );
  }
}
