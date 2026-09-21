import 'dart:async';
import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:morrow_i18n/morrow_i18n.dart';

class StoredCredential {
  const StoredCredential({
    required this.reference,
    required this.revision,
    required this.createdMs,
    required this.expiresMs,
    required this.disabled,
  });
  final Uint8List reference;
  final BigInt revision, createdMs, expiresMs;
  final bool disabled;
}

class CredentialPage {
  const CredentialPage({
    required this.entries,
    required this.snapshot,
    this.next,
  });
  final List<StoredCredential> entries;
  final Uint8List snapshot;
  final Uint8List? next;
}

abstract interface class WorkbenchCredentialControl {
  Future<CredentialPage> credentialPage({
    Uint8List? after,
    Uint8List? snapshot,
  });
  Future<StoredCredential> saveCredential({
    required Uint8List reference,
    required BigInt expectedRevision,
    required String headerName,
    required String headerValue,
    required int lifetimeDays,
  });
  Future<StoredCredential> disableCredential(StoredCredential expected);
}

/// This panel never reads stored secrets or grants endpoint/plugin permissions.
class CredentialManager extends StatefulWidget {
  const CredentialManager({
    super.key,
    required this.backend,
    required this.ink,
    required this.muted,
    required this.line,
    required this.radius,
  });
  final WorkbenchCredentialControl backend;
  final Color ink, muted, line;
  final BorderRadius radius;
  @override
  State<CredentialManager> createState() => _CredentialManagerState();
}

class _CredentialManagerState extends State<CredentialManager> {
  final _header = TextEditingController(text: 'authorization');
  final _secret = TextEditingController();
  List<StoredCredential> _entries = [];
  StoredCredential? _replacing;
  bool _busy = false, _trusted = false, _form = false;
  int _epoch = 0, _days = 7, _formEpoch = 0;
  String Function(AppLocalizations)? _message;

  bool _current(WorkbenchCredentialControl backend, int epoch) =>
      mounted && identical(widget.backend, backend) && epoch == _epoch;

  @override
  void initState() {
    super.initState();
    unawaited(_refresh());
  }

  @override
  void didUpdateWidget(covariant CredentialManager oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (!identical(oldWidget.backend, widget.backend)) {
      _epoch++;
      _secret.clear();
      _header.text = 'authorization';
      _form = false;
      _replacing = null;
      _entries = [];
      _trusted = false;
      _busy = false;
      _message = null;
      unawaited(_refresh());
    }
  }

  @override
  void dispose() {
    _epoch++;
    _secret.clear();
    _secret.dispose();
    _header.dispose();
    super.dispose();
  }

  static int _compare(List<int> left, List<int> right) {
    for (var i = 0; i < left.length && i < right.length; i++) {
      if (left[i] != right[i]) return left[i].compareTo(right[i]);
    }
    return left.length.compareTo(right.length);
  }

  static String _hex(List<int> reference) =>
      reference.map((v) => v.toRadixString(16).padLeft(2, '0')).join();
  static String _short(List<int> reference) {
    final hex = _hex(reference);
    return '${hex.substring(0, 8)}…${hex.substring(56)}';
  }

  static StoredCredential _checked(StoredCredential entry) {
    if (entry.reference.length != 32 ||
        entry.reference.every((v) => v == 0) ||
        entry.revision <= BigInt.zero ||
        entry.createdMs <= BigInt.zero ||
        entry.expiresMs <= entry.createdMs ||
        entry.expiresMs > BigInt.from(253402300799999)) {
      throw const FormatException('Invalid credential metadata');
    }
    return StoredCredential(
      reference: Uint8List.fromList(entry.reference),
      revision: entry.revision,
      createdMs: entry.createdMs,
      expiresMs: entry.expiresMs,
      disabled: entry.disabled,
    );
  }

  Future<void> _refresh() async {
    if (_busy) return;
    final backend = widget.backend;
    final epoch = ++_epoch;
    _secret.clear();
    setState(() {
      _busy = true;
      _trusted = false;
      _form = false;
      _replacing = null;
      _entries = [];
      _message = null;
    });
    try {
      final entries = <StoredCredential>[];
      Uint8List? after, snapshot;
      for (var pageIndex = 0; pageIndex < 512; pageIndex++) {
        final page = await backend.credentialPage(
          after: after == null ? null : Uint8List.fromList(after),
          snapshot: snapshot == null ? null : Uint8List.fromList(snapshot),
        );
        if (!_current(backend, epoch)) return;
        if (page.snapshot.length != 32 ||
            (snapshot != null && _compare(snapshot, page.snapshot) != 0) ||
            page.entries.length > 16 ||
            entries.length + page.entries.length > 512) {
          throw const FormatException('Inconsistent credential page');
        }
        snapshot ??= Uint8List.fromList(page.snapshot);
        for (final raw in page.entries) {
          final entry = _checked(raw);
          if ((after != null && _compare(entry.reference, after) <= 0) ||
              (entries.isNotEmpty &&
                  _compare(entry.reference, entries.last.reference) <= 0)) {
            throw const FormatException('Invalid credential ordering');
          }
          entries.add(entry);
        }
        final next = page.next;
        if (next == null) {
          setState(() {
            _entries = entries;
            _trusted = true;
          });
          return;
        }
        // Pages cover the shared outbound table. An endpoint-only page may
        // contain no credentials; its cursor still has to advance monotonically.
        if (next.length != 32 ||
            (after != null && _compare(next, after) <= 0) ||
            (entries.isNotEmpty &&
                _compare(next, entries.last.reference) < 0)) {
          throw const FormatException('Invalid credential cursor');
        }
        after = Uint8List.fromList(next);
      }
      throw const FormatException('Credential page bound');
    } catch (_) {
      if (_current(backend, epoch)) {
        setState(() => _message = (l) => l.pluginsCredentialLoadFailed);
      }
    } finally {
      if (_current(backend, epoch)) setState(() => _busy = false);
    }
  }

  void _open([StoredCredential? replacing]) {
    if (_busy || !_trusted) return;
    _secret.clear();
    _header.text = 'authorization';
    setState(() {
      _replacing = replacing;
      _formEpoch++;
      _days = 7;
      _form = true;
      _message = null;
    });
  }

  void _close() {
    _secret.clear();
    setState(() {
      _form = false;
      _replacing = null;
    });
  }

  // Keep plaintext out of the asynchronous state machine and clear the editing
  // buffer before invoking the backend, including locally rejected attempts.
  Future<StoredCredential>? _takeSubmission(
    WorkbenchCredentialControl backend,
    StoredCredential? expected,
  ) {
    final secret = _secret.text;
    final name = _header.text.trim().toLowerCase();
    _secret.clear();
    if (!RegExp(r"^[a-z0-9!#$%&'*+.^_`|~-]{1,128}$").hasMatch(name) ||
        secret.isEmpty ||
        secret.length > 8192 ||
        !secret.codeUnits.every((v) => v >= 0x20 && v <= 0x7e)) {
      setState(() => _message = (l) => l.pluginsCredentialInvalid);
      return null;
    }
    return backend.saveCredential(
      reference: expected == null
          ? Uint8List(0)
          : Uint8List.fromList(expected.reference),
      expectedRevision: expected?.revision ?? BigInt.zero,
      headerName: name,
      headerValue: secret,
      lifetimeDays: _days,
    );
  }

  Future<void> _save() async {
    if (_busy || !_trusted || !_form) return;
    final backend = widget.backend;
    final expected = _replacing;
    final epoch = ++_epoch;
    setState(() {
      _busy = true;
      _message = null;
    });
    try {
      final pending = _takeSubmission(backend, expected);
      if (pending == null) return;
      final saved = _checked(await pending);
      if (!_current(backend, epoch)) return;
      if (saved.disabled ||
          saved.revision != (expected?.revision ?? BigInt.zero) + BigInt.one ||
          (expected != null &&
              _compare(saved.reference, expected.reference) != 0)) {
        throw const FormatException('Unexpected credential acknowledgement');
      }
      setState(() {
        _entries = [
          ..._entries.where((e) => _compare(e.reference, saved.reference) != 0),
          saved,
        ]..sort((a, b) => _compare(a.reference, b.reference));
        _form = false;
        _replacing = null;
        _message = (l) => l.pluginsCredentialSaved;
      });
    } catch (_) {
      if (_current(backend, epoch)) _unknown();
    } finally {
      if (_current(backend, epoch)) setState(() => _busy = false);
    }
  }

  void _unknown() {
    _secret.clear();
    setState(() {
      _trusted = false;
      _form = false;
      _replacing = null;
      _message = (l) => l.pluginsCredentialUnknown;
    });
  }

  Future<void> _disable(StoredCredential expected) async {
    if (_busy || !_trusted) return;
    final backend = widget.backend;
    final epoch = ++_epoch;
    _secret.clear();
    setState(() {
      _busy = true;
      _message = null;
    });
    try {
      final saved = _checked(await backend.disableCredential(expected));
      if (!_current(backend, epoch)) return;
      if (!saved.disabled ||
          saved.revision != expected.revision + BigInt.one ||
          _compare(saved.reference, expected.reference) != 0) {
        throw const FormatException('Unexpected disable acknowledgement');
      }
      setState(() {
        _entries = _entries
            .map((e) => _compare(e.reference, saved.reference) == 0 ? saved : e)
            .toList();
        _message = (l) => l.pluginsCredentialDisabledDone;
        _form = false;
        _replacing = null;
      });
    } catch (_) {
      if (_current(backend, epoch)) _unknown();
    } finally {
      if (_current(backend, epoch)) setState(() => _busy = false);
    }
  }

  Widget _note(String text) => Padding(
    padding: const EdgeInsets.symmetric(vertical: 5),
    child: Text(
      text,
      style: TextStyle(color: widget.muted, fontSize: 11, height: 1.6),
    ),
  );
  Widget _button(
    String text,
    String key,
    IconData icon,
    VoidCallback? action,
  ) => OutlinedButton.icon(
    key: ValueKey(key),
    onPressed: _busy ? null : action,
    style: OutlinedButton.styleFrom(
      foregroundColor: widget.ink,
      minimumSize: const Size(0, 38),
      side: BorderSide(color: widget.line),
      shape: RoundedRectangleBorder(borderRadius: widget.radius),
    ),
    icon: Icon(icon, size: 16),
    label: Text(text, style: const TextStyle(fontSize: 11)),
  );

  @override
  Widget build(BuildContext context) {
    final l = L10n.of(context);
    final material = MaterialLocalizations.of(context);
    return Padding(
      padding: const EdgeInsets.all(19),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            l.pluginsCredentialTitle,
            style: TextStyle(
              color: widget.ink,
              fontSize: 13,
              fontWeight: FontWeight.w600,
            ),
          ),
          _note(l.pluginsCredentialDetails),
          Wrap(
            spacing: 8,
            runSpacing: 8,
            children: [
              _button(
                l.pluginsCredentialNew,
                'credential-new',
                Icons.add,
                _trusted ? _open : null,
              ),
              _button(
                l.pluginsCredentialRefresh,
                'credential-refresh',
                Icons.refresh,
                _refresh,
              ),
            ],
          ),
          if (_busy) _note(l.pluginsCredentialReading),
          if (_message != null) _note(_message!(l)),
          if (_trusted && _entries.isEmpty) _note(l.pluginsCredentialEmpty),
          if (_form) ...[
            const SizedBox(height: 12),
            Text(
              _replacing == null
                  ? l.pluginsCredentialCreateTitle
                  : l.pluginsCredentialReplaceTitle(
                      _short(_replacing!.reference),
                    ),
              style: TextStyle(color: widget.ink, fontSize: 12),
            ),
            const SizedBox(height: 16),
            TextField(
              key: const ValueKey('credential-header'),
              controller: _header,
              enabled: !_busy,
              autocorrect: false,
              enableSuggestions: false,
              decoration: InputDecoration(labelText: l.pluginsCredentialHeader),
            ),
            const SizedBox(height: 16),
            TextField(
              key: const ValueKey('credential-secret'),
              controller: _secret,
              enabled: !_busy,
              obscureText: true,
              autocorrect: false,
              enableSuggestions: false,
              autofillHints: const [],
              keyboardType: TextInputType.visiblePassword,
              smartDashesType: SmartDashesType.disabled,
              smartQuotesType: SmartQuotesType.disabled,
              decoration: InputDecoration(labelText: l.pluginsCredentialSecret),
            ),
            const SizedBox(height: 16),
            DropdownButtonFormField<int>(
              key: ValueKey('credential-days-$_formEpoch'),
              initialValue: _days,
              isExpanded: true,
              decoration: InputDecoration(
                labelText: l.pluginsCredentialLifetime,
              ),
              items: [
                for (var i = 1; i <= 30; i++)
                  DropdownMenuItem(
                    value: i,
                    child: Text(l.pluginsCredentialDays(i)),
                  ),
              ],
              onChanged: _busy
                  ? null
                  : (value) {
                      if (value != null) setState(() => _days = value);
                    },
            ),
            const SizedBox(height: 16),
            Wrap(
              spacing: 8,
              runSpacing: 8,
              children: [
                _button(
                  l.pluginsCredentialSave,
                  'credential-save',
                  Icons.key_outlined,
                  _save,
                ),
                _button(
                  l.pluginsCredentialCancel,
                  'credential-close',
                  Icons.close,
                  _close,
                ),
              ],
            ),
          ],
          for (final entry in _entries) ...[
            const SizedBox(height: 12),
            Text(
              l.pluginsCredentialReference(_short(entry.reference)),
              key: ValueKey('credential-row-${_hex(entry.reference)}'),
              style: TextStyle(color: widget.ink, fontSize: 12),
            ),
            _note(
              entry.disabled
                  ? l.pluginsCredentialDisabled
                  : entry.expiresMs <=
                        BigInt.from(DateTime.now().millisecondsSinceEpoch)
                  ? l.pluginsCredentialExpired
                  : l.pluginsCredentialStored,
            ),
            _note(
              l.pluginsCredentialExpires(
                '${material.formatMediumDate(DateTime.fromMillisecondsSinceEpoch(entry.expiresMs.toInt()).toLocal())} ${material.formatTimeOfDay(TimeOfDay.fromDateTime(DateTime.fromMillisecondsSinceEpoch(entry.expiresMs.toInt()).toLocal()))}',
              ),
            ),
            Wrap(
              spacing: 8,
              runSpacing: 8,
              children: [
                _button(
                  l.pluginsCredentialReplace,
                  'credential-replace-${_hex(entry.reference)}',
                  Icons.sync,
                  _trusted ? () => _open(entry) : null,
                ),
                _button(
                  l.pluginsCredentialDisable,
                  'credential-disable-${_hex(entry.reference)}',
                  Icons.block_outlined,
                  _trusted && !entry.disabled ? () => _disable(entry) : null,
                ),
              ],
            ),
          ],
        ],
      ),
    );
  }
}
