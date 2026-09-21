import 'dart:async';
import 'package:file_selector/file_selector.dart';
import 'package:flutter/material.dart';
import 'package:morrow_i18n/morrow_i18n.dart';

import 'service_run_control.dart';

/// A per-view choice; only a successfully checked immutable value leaves it.
class ServiceTlsPicker extends StatefulWidget {
  const ServiceTlsPicker({
    super.key,
    required this.backend,
    required this.enabled,
    required this.onChanged,
    required this.ink,
    required this.muted,
    required this.line,
    required this.radius,
    this.chooseFile,
    this.now,
  });
  final WorkbenchServiceTlsControl backend;
  final bool enabled;
  final ValueChanged<ServiceTlsSelection?> onChanged;
  final Color ink, muted, line;
  final BorderRadius radius;
  @visibleForTesting
  final Future<String?> Function()? chooseFile;
  @visibleForTesting
  final DateTime Function()? now;
  @override
  State<ServiceTlsPicker> createState() => _ServiceTlsPickerState();
}

class _ServiceTlsPickerState extends State<ServiceTlsPicker> {
  DateTime get _now => widget.now?.call() ?? DateTime.now();
  String _certificate = '', _privateKey = '';
  ServiceTlsSelection? _checked;
  bool _busy = false, _failed = false;
  int _generation = 0;
  Timer? _timer;
  bool _accepted = false;
  @override
  void initState() {
    super.initState();
    _timer = Timer.periodic(const Duration(seconds: 1), (_) {
      final validity = _checked?.validity;
      if (validity == null) return;
      if (_accepted && !validity.validAt(_now)) {
        _accepted = false;
        widget.onChanged(null);
      }
      setState(() {});
    });
  }

  @override
  void dispose() {
    _timer?.cancel();
    super.dispose();
  }

  @override
  void didUpdateWidget(covariant ServiceTlsPicker oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (!identical(oldWidget.backend, widget.backend)) {
      _generation++;
      _certificate = _privateKey = '';
      _checked = null;
      _accepted = false;
      _busy = _failed = false;
    }
  }

  Future<void> _pick(bool certificate) async {
    if (!widget.enabled || _busy) return;
    final generation = ++_generation;
    setState(() {
      _busy = true;
      _failed = false;
      _checked = null;
    });
    widget.onChanged(null);
    try {
      final selected = widget.chooseFile != null
          ? await widget.chooseFile!()
          : (await openFile(
              acceptedTypeGroups: const [
                XTypeGroup(
                  label: 'PEM',
                  extensions: ['pem', 'crt', 'cer', 'key'],
                ),
              ],
            ))?.path;
      if (!mounted || generation != _generation) return;
      if (selected != null) {
        ServiceRunValidation.tlsPath(selected);
        setState(() {
          if (certificate) {
            _certificate = selected;
          } else {
            _privateKey = selected;
          }
        });
      }
    } catch (_) {
      if (mounted && generation == _generation) setState(() => _failed = true);
    } finally {
      if (mounted && generation == _generation) setState(() => _busy = false);
    }
  }

  Future<void> _inspect() async {
    if (!widget.enabled ||
        _busy ||
        _certificate.isEmpty ||
        _privateKey.isEmpty) {
      return;
    }
    final generation = ++_generation;
    final certificate = _certificate, privateKey = _privateKey;
    setState(() {
      _busy = true;
      _failed = false;
      _checked = null;
    });
    widget.onChanged(null);
    try {
      final checked = await widget.backend.inspectServiceTls(
        certificatePath: certificate,
        privateKeyPath: privateKey,
      );
      if (!mounted || generation != _generation) return;
      ServiceRunValidation.tlsSelection(checked);
      final validity = checked.validity;
      if (validity == null) throw const FormatException('Missing TLS validity');
      if (checked.certificatePath != certificate ||
          checked.privateKeyPath != privateKey) {
        throw const FormatException('TLS selection changed');
      }
      setState(() => _checked = checked);
      _accepted = validity.validAt(_now);
      widget.onChanged(_accepted ? checked : null);
    } catch (_) {
      if (mounted && generation == _generation) setState(() => _failed = true);
    } finally {
      if (mounted && generation == _generation) setState(() => _busy = false);
    }
  }

  Widget _button(
    String text,
    String key,
    VoidCallback action, {
    bool ready = true,
  }) => OutlinedButton(
    key: ValueKey('service-tls-$key'),
    onPressed: widget.enabled && !_busy && ready ? action : null,
    style: OutlinedButton.styleFrom(
      foregroundColor: widget.ink,
      side: BorderSide(color: widget.line),
      shape: RoundedRectangleBorder(borderRadius: widget.radius),
    ),
    child: Text(text, style: const TextStyle(fontSize: 11)),
  );

  @override
  Widget build(BuildContext context) {
    final l = L10n.of(context);
    final checked = _checked;
    final validity = checked?.validity;
    Widget note(String text, {String? key}) => Padding(
      padding: const EdgeInsets.symmetric(vertical: 5),
      child: Text(
        text,
        key: key == null ? null : ValueKey(key),
        style: TextStyle(color: widget.muted, fontSize: 11),
      ),
    );
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        note(l.pluginsServiceTlsHint),
        Wrap(
          spacing: 8,
          runSpacing: 6,
          children: [
            _button(
              l.pluginsServiceTlsCertificate,
              'certificate',
              () => _pick(true),
            ),
            _button(
              l.pluginsServiceTlsPrivateKey,
              'private-key',
              () => _pick(false),
            ),
            _button(
              l.pluginsServiceTlsInspect,
              'inspect',
              _inspect,
              ready: _certificate.isNotEmpty && _privateKey.isNotEmpty,
            ),
          ],
        ),
        if (_certificate.isNotEmpty)
          note(_certificate, key: 'service-tls-certificate-path'),
        if (_privateKey.isNotEmpty)
          note(_privateKey, key: 'service-tls-private-key-path'),
        if (_busy) note(l.pluginsServiceTlsChecking),
        if (_failed) note(l.pluginsServiceTlsFailed, key: 'service-tls-failed'),
        if (checked != null) ...[
          note(l.pluginsServiceTlsChecked),
          if (validity != null) ...[
            note(
              l.pluginsServiceTlsValidity(
                DateTime.fromMillisecondsSinceEpoch(
                  validity.notAfterSeconds * 1000,
                  isUtc: true,
                ).toIso8601String(),
                DateTime.fromMillisecondsSinceEpoch(
                  validity.notBeforeSeconds * 1000,
                  isUtc: true,
                ).toIso8601String(),
              ),
            ),
            if (!validity.validAt(_now))
              note(
                l.pluginsServiceTlsOutsideValidity,
                key: 'service-tls-outside-validity',
              ),
            if (!_accepted) note(l.pluginsServiceTlsRecheck),
          ],
          SelectionArea(
            child: Text(
              checked.certificateSha256
                  .map((v) => v.toRadixString(16).padLeft(2, '0'))
                  .join(),
              key: const ValueKey('service-tls-fingerprint'),
              style: TextStyle(color: widget.muted, fontSize: 11, height: 1.6),
            ),
          ),
        ],
      ],
    );
  }
}
