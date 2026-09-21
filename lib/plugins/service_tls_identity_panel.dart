import 'dart:async';
import 'package:flutter/material.dart';
import 'package:morrow_i18n/morrow_i18n.dart';
import 'service_run_control.dart';
import 'service_tls_identity_session.dart';
import 'service_tls_picker.dart';
import 'session_view_state.dart';

class ServiceTlsIdentityPanel extends StatefulWidget {
  const ServiceTlsIdentityPanel({
    super.key,
    required this.backend,
    required this.inspector,
    required this.selectionEnabled,
    required this.onChanged,
    required this.ink,
    required this.muted,
    required this.line,
    required this.radius,
    this.chooseFile,
  });
  final WorkbenchTlsIdentityControl backend;
  final WorkbenchServiceTlsControl inspector;
  final bool selectionEnabled;
  final void Function(ServiceTlsSelection?, ServiceTlsIdentityChoice?)
  onChanged;
  final Color ink, muted, line;
  final BorderRadius radius;
  @visibleForTesting
  final Future<String?> Function()? chooseFile;
  @override
  State<ServiceTlsIdentityPanel> createState() =>
      _ServiceTlsIdentityPanelState();
}

class _ServiceTlsIdentityPanelState extends State<ServiceTlsIdentityPanel>
    with SessionViewState<ServiceTlsIdentityPanel> {
  late TlsIdentitySession session;
  bool _queued = false;
  void _attach() {
    session = TlsIdentitySession.forBackend(widget.backend);
    session.addListener(_changed);
    if (!session.trusted && !session.busy) unawaited(session.refresh());
    _changed();
  }

  void _changed() {
    markSessionViewDirty();
    if (_queued) return;
    _queued = true;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      _queued = false;
      if (!mounted) return;
      widget.onChanged(
        session.canUse && !session.useSaved ? session.draft.checked : null,
        session.canUse && session.useSaved ? session.selected : null,
      );
    });
  }

  @override
  void initState() {
    super.initState();
    _attach();
  }

  @override
  void didUpdateWidget(covariant ServiceTlsIdentityPanel oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (!identical(oldWidget.backend, widget.backend)) {
      session.removeListener(_changed);
      _attach();
    }
  }

  @override
  void dispose() {
    session.removeListener(_changed);
    super.dispose();
  }

  Widget _button(String label, String key, VoidCallback? action) =>
      OutlinedButton(
        key: ValueKey('tls-identities-$key'),
        onPressed: action,
        style: OutlinedButton.styleFrom(
          foregroundColor: widget.ink,
          side: BorderSide(color: widget.line),
          shape: RoundedRectangleBorder(borderRadius: widget.radius),
        ),
        child: Text(label, style: const TextStyle(fontSize: 11)),
      );
  Widget _note(String text, {String? key}) => Padding(
    padding: const EdgeInsets.symmetric(vertical: 5),
    child: Text(
      text,
      key: key == null ? null : ValueKey(key),
      style: TextStyle(color: widget.muted, fontSize: 11, height: 1.6),
    ),
  );
  @override
  Widget build(BuildContext context) {
    final l = L10n.of(context), s = session;
    final checked =
        s.draft.accepted &&
        s.draft.checked?.validity?.validAt(DateTime.now()) == true;
    final selected = s.selected;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        _note(l.pluginsTlsIdentitiesTitle),
        _note(l.pluginsTlsIdentitiesHint),
        Wrap(
          spacing: 8,
          runSpacing: 6,
          children: [
            _button(
              l.pluginsServiceRefresh,
              'refresh',
              s.busy ? null : () => unawaited(s.refresh()),
            ),
            _button(
              l.pluginsTlsIdentitiesUseFile,
              'use-file',
              widget.selectionEnabled && s.canWrite ? s.useFile : null,
            ),
          ],
        ),
        if (s.busy) const LinearProgressIndicator(minHeight: 2),
        if (s.notice == 'loadFailed') _note(l.pluginsServiceLoadFailed),
        if (s.notice == 'invalid') _note(l.pluginsServiceInvalid),
        if (s.notice == 'saved') _note(l.pluginsTlsIdentitiesSaved),
        if (s.uncertain) ...[
          _note(l.pluginsServiceWriteUnknown, key: 'tls-identities-unknown'),
          _note(l.pluginsTlsIdentitiesUnknownHint),
          _button(
            l.pluginsServiceAcknowledgeUncertain,
            'acknowledge',
            s.trusted && !s.busy ? s.acknowledgeUncertain : null,
          ),
        ],
        _note(
          s.useSaved
              ? l.pluginsTlsIdentitiesSavedMode
              : l.pluginsTlsIdentitiesFileMode,
        ),
        if (s.useSaved && selected != null) ...[
          _note(
            '${selected.key} · r${selected.revision}',
            key: 'tls-identities-selection',
          ),
          if (!s.current(selected))
            _note(l.pluginsTlsIdentitiesStale, key: 'tls-identities-stale'),
        ],
        if (s.trusted && s.records.isEmpty) _note(l.pluginsTlsIdentitiesEmpty),
        for (final row in s.records)
          Container(
            margin: const EdgeInsets.symmetric(vertical: 5),
            padding: const EdgeInsets.all(8),
            decoration: BoxDecoration(
              border: Border.all(color: widget.line),
              borderRadius: widget.radius,
            ),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                _note(
                  '${row.choice.key} · r${row.choice.revision}${row.disabled ? " · ${l.pluginsServiceDisabled}" : ""}',
                ),
                SelectionArea(
                  child: _note(
                    row.choice.certificateSha256
                        .map((v) => v.toRadixString(16).padLeft(2, '0'))
                        .join(),
                  ),
                ),
                Wrap(
                  spacing: 8,
                  runSpacing: 6,
                  children: [
                    _button(
                      l.pluginsTlsIdentitiesSelect,
                      'select-${row.choice.key}',
                      widget.selectionEnabled && s.canWrite && !row.disabled
                          ? () => s.select(row.choice)
                          : null,
                    ),
                    _button(
                      l.pluginsTlsIdentitiesReplace,
                      'replace-${row.choice.key}',
                      s.canWrite && checked
                          ? () => unawaited(s.save(previous: row))
                          : null,
                    ),
                    _button(
                      l.pluginsTlsIdentitiesDisable,
                      'disable-${row.choice.key}',
                      s.canWrite && !row.disabled
                          ? () => unawaited(s.disable(row))
                          : null,
                    ),
                  ],
                ),
              ],
            ),
          ),
        _note(l.pluginsTlsIdentitiesImport),
        ServiceTlsPicker(
          backend: widget.inspector,
          enabled: s.canWrite,
          draft: s.draft,
          chooseFile: widget.chooseFile,
          onChanged: (_) => s.changed(),
          ink: widget.ink,
          muted: widget.muted,
          line: widget.line,
          radius: widget.radius,
        ),
        _button(
          l.pluginsTlsIdentitiesSave,
          'save',
          s.canWrite && checked ? () => unawaited(s.save()) : null,
        ),
      ],
    );
  }
}
