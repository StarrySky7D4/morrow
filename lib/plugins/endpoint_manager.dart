import 'dart:async';
import 'dart:typed_data';

import 'package:file_selector/file_selector.dart';
import 'package:flutter/material.dart';
import 'package:morrow_i18n/morrow_i18n.dart';

import 'credential_manager.dart';
import 'endpoint_control.dart';
import 'plugin_library.dart';

/// Edits persisted approvals only. No operation here opens a network connection.
class EndpointManager extends StatefulWidget {
  const EndpointManager({
    super.key,
    required this.backend,
    required this.plugins,
    required this.registryRevision,
    required this.ink,
    required this.muted,
    required this.line,
    required this.radius,
    this.credentialBackend,
    this.pickCertificate,
  });
  final WorkbenchEndpointControl backend;
  final List<PluginLibraryEntry> plugins;
  final BigInt registryRevision;
  final WorkbenchCredentialControl? credentialBackend;
  final Color ink, muted, line;
  final BorderRadius radius;
  final Future<XFile?> Function()? pickCertificate;

  @override
  State<EndpointManager> createState() => _EndpointManagerState();
}

class _EndpointManagerState extends State<EndpointManager> {
  final _origin = TextEditingController();
  final _days = TextEditingController(text: '7');
  final _limits = <String, TextEditingController>{
    'request': TextEditingController(text: '65536'),
    'response': TextEditingController(text: '65536'),
    'headers': TextEditingController(text: '16384'),
    'concurrent': TextEditingController(text: '1'),
    'timeout': TextEditingController(text: '10000'),
    'frame': TextEditingController(text: '131072'),
  };
  List<StoredEndpoint> _entries = [];
  List<StoredCredential> _credentials = [];
  StoredEndpoint? _replacing;
  String? _packageId;
  String _credential = '';
  Uint8List _root = Uint8List(0);
  Set<String> _methods = {'GET'};
  int _profile = 1, _epoch = 0, _formEpoch = 0;
  bool _busy = false, _trusted = false, _form = false, _writing = false;
  late String _directory;
  String Function(AppLocalizations)? _message;

  static String _hex(List<int> bytes) =>
      bytes.map((v) => v.toRadixString(16).padLeft(2, '0')).join();
  static int _compare(List<int> a, List<int> b) {
    for (var i = 0; i < a.length && i < b.length; i++) {
      if (a[i] != b[i]) return a[i].compareTo(b[i]);
    }
    return a.length.compareTo(b.length);
  }

  static bool _reference(List<int> bytes) =>
      bytes.length == 32 && bytes.any((v) => v != 0);
  static String _fingerprint(EndpointManager widget) => [
    widget.registryRevision.toString(),
    ...widget.plugins.map(
      (p) =>
          '${p.id}:${_hex(p.digest)}:${p.available}:${p.declaredIo.join(',')}:${p.approvedIo.join(',')}',
    ),
  ].join('|');
  bool _current(WorkbenchEndpointControl backend, int epoch) =>
      mounted && epoch == _epoch && identical(widget.backend, backend);
  bool _canUse(PluginLibraryEntry p) =>
      p.available &&
      p.digest.length == 32 &&
      p.declaredIo.contains('http-request') &&
      p.approvedIo.contains('http-request');
  bool _canUseCredential(PluginLibraryEntry p) =>
      p.declaredIo.contains('credential-use') &&
      p.approvedIo.contains('credential-use');
  static bool _loopback(String host) {
    if (const ['localhost', '::1', '[::1]', '0:0:0:0:0:0:0:1'].contains(host)) {
      return true;
    }
    final octets = host.split('.');
    return octets.length == 4 &&
        octets.first == '127' &&
        octets.every((v) {
          final n = int.tryParse(v);
          return n != null && n >= 0 && n <= 255;
        });
  }

  List<PluginLibraryEntry> get _choices => widget.plugins
      .where(
        (p) =>
            _canUse(p) &&
            (_replacing == null || p.id == _replacing!.policy.packageId),
      )
      .toList();
  PluginLibraryEntry? get _selected {
    for (final p in _choices) {
      if (p.id == _packageId) return p;
    }
    return null;
  }

  List<StoredCredential> get _validCredentials => _credentials
      .where(
        (c) =>
            !c.disabled &&
            c.expiresMs > BigInt.from(DateTime.now().millisecondsSinceEpoch),
      )
      .toList();

  @override
  void initState() {
    super.initState();
    _directory = _fingerprint(widget);
    unawaited(_refresh());
  }

  @override
  void didUpdateWidget(covariant EndpointManager oldWidget) {
    super.didUpdateWidget(oldWidget);
    final directory = _fingerprint(widget);
    if (!identical(oldWidget.backend, widget.backend) ||
        !identical(oldWidget.credentialBackend, widget.credentialBackend) ||
        directory != _directory) {
      _directory = directory;
      _epoch++;
      _formEpoch++;
      _busy = false;
      _writing = false;
      _form = false;
      _trusted = false;
      _entries = [];
      _credentials = [];
      _replacing = null;
      _message = null;
      unawaited(_refresh());
    }
  }

  @override
  void dispose() {
    _epoch++;
    _origin.dispose();
    _days.dispose();
    for (final c in _limits.values) {
      c.dispose();
    }
    super.dispose();
  }

  static EndpointPolicy _copyPolicy(EndpointPolicy p) => EndpointPolicy(
    packageId: p.packageId,
    packageDigest: Uint8List.fromList(p.packageDigest),
    origin: p.origin,
    profile: p.profile,
    methods: List.of(p.methods),
    credentialReference: Uint8List.fromList(p.credentialReference),
    rootCertificate: Uint8List.fromList(p.rootCertificate),
    maxRequestBytes: p.maxRequestBytes,
    maxResponseBytes: p.maxResponseBytes,
    maxHeaderBytes: p.maxHeaderBytes,
    maxConcurrent: p.maxConcurrent,
    timeoutMs: p.timeoutMs,
    maxFrameBytes: p.maxFrameBytes,
  );
  static StoredEndpoint _checked(StoredEndpoint e) {
    if (!_reference(e.reference) ||
        e.revision <= BigInt.zero ||
        e.createdMs <= BigInt.zero ||
        e.expiresMs <= e.createdMs ||
        e.expiresMs > BigInt.from(253402300799999) ||
        e.policy.packageId.isEmpty ||
        e.policy.packageDigest.length != 32 ||
        (e.policy.credentialReference.isNotEmpty &&
            !_reference(e.policy.credentialReference)) ||
        e.policy.rootCertificate.length > 32768) {
      throw const FormatException('Invalid endpoint metadata');
    }
    return StoredEndpoint(
      reference: Uint8List.fromList(e.reference),
      revision: e.revision,
      createdMs: e.createdMs,
      expiresMs: e.expiresMs,
      disabled: e.disabled,
      policy: _copyPolicy(e.policy),
    );
  }

  Future<List<StoredCredential>> _loadCredentials(
    WorkbenchEndpointControl endpointBackend,
    int epoch,
  ) async {
    final backend = widget.credentialBackend;
    if (backend == null) return [];
    final entries = <StoredCredential>[];
    Uint8List? after, snapshot;
    for (var i = 0; i < 512; i++) {
      final page = await backend.credentialPage(
        after: after == null ? null : Uint8List.fromList(after),
        snapshot: snapshot == null ? null : Uint8List.fromList(snapshot),
      );
      if (!_current(endpointBackend, epoch)) return [];
      if (page.snapshot.length != 32 ||
          (snapshot != null && _compare(snapshot, page.snapshot) != 0) ||
          page.entries.length > 16 ||
          entries.length + page.entries.length > 512) {
        throw const FormatException('Invalid credential page');
      }
      snapshot ??= Uint8List.fromList(page.snapshot);
      for (final c in page.entries) {
        if (!_reference(c.reference) ||
            c.revision <= BigInt.zero ||
            c.createdMs <= BigInt.zero ||
            c.expiresMs <= c.createdMs ||
            c.expiresMs > BigInt.from(253402300799999) ||
            (after != null && _compare(c.reference, after) <= 0) ||
            (entries.isNotEmpty &&
                _compare(c.reference, entries.last.reference) <= 0)) {
          throw const FormatException('Invalid credential metadata');
        }
        entries.add(
          StoredCredential(
            reference: Uint8List.fromList(c.reference),
            revision: c.revision,
            createdMs: c.createdMs,
            expiresMs: c.expiresMs,
            disabled: c.disabled,
          ),
        );
      }
      if (page.next == null) return entries;
      final next = page.next!;
      if (!_reference(next) ||
          (after != null && _compare(next, after) <= 0) ||
          (entries.isNotEmpty && _compare(next, entries.last.reference) < 0)) {
        throw const FormatException('Invalid credential cursor');
      }
      after = Uint8List.fromList(next);
    }
    throw const FormatException('Credential page bound');
  }

  Future<void> _refresh() async {
    if (_busy) return;
    final backend = widget.backend;
    final epoch = ++_epoch;
    _formEpoch++;
    setState(() {
      _busy = true;
      _trusted = false;
      _form = false;
      _writing = false;
      _replacing = null;
      _entries = [];
      _credentials = [];
      _message = null;
    });
    try {
      final entries = <StoredEndpoint>[];
      Uint8List? after, snapshot;
      var complete = false;
      for (var i = 0; i < 512; i++) {
        final page = await backend.endpointPage(
          after: after == null ? null : Uint8List.fromList(after),
          snapshot: snapshot == null ? null : Uint8List.fromList(snapshot),
        );
        if (!_current(backend, epoch)) return;
        if (page.snapshot.length != 32 ||
            (snapshot != null && _compare(snapshot, page.snapshot) != 0) ||
            page.entries.length > 2 ||
            entries.length + page.entries.length > 512) {
          throw const FormatException('Inconsistent endpoint page');
        }
        snapshot ??= Uint8List.fromList(page.snapshot);
        for (final raw in page.entries) {
          final e = _checked(raw);
          if ((after != null && _compare(e.reference, after) <= 0) ||
              (entries.isNotEmpty &&
                  _compare(e.reference, entries.last.reference) <= 0)) {
            throw const FormatException('Invalid endpoint ordering');
          }
          entries.add(e);
        }
        final next = page.next;
        if (next == null) {
          complete = true;
          break;
        }
        // The shared table also contains credentials: empty pages can advance.
        if (!_reference(next) ||
            (after != null && _compare(next, after) <= 0) ||
            (entries.isNotEmpty &&
                _compare(next, entries.last.reference) < 0)) {
          throw const FormatException('Invalid endpoint cursor');
        }
        after = Uint8List.fromList(next);
      }
      if (!complete) throw const FormatException('Endpoint page bound');
      List<StoredCredential> credentials = [];
      var credentialFailure = false;
      try {
        credentials = await _loadCredentials(backend, epoch);
      } catch (_) {
        credentialFailure = true;
      }
      if (!_current(backend, epoch)) return;
      setState(() {
        _entries = entries;
        _credentials = credentials;
        _trusted = true;
        if (credentialFailure) {
          _message = (l) => l.pluginsEndpointCredentialsFailed;
        }
      });
    } catch (_) {
      if (_current(backend, epoch)) {
        setState(() => _message = (l) => l.pluginsEndpointLoadFailed);
      }
    } finally {
      if (_current(backend, epoch)) setState(() => _busy = false);
    }
  }

  void _open([StoredEndpoint? old]) {
    if (_busy || !_trusted) return;
    _formEpoch++;
    _replacing = old;
    final p = old?.policy;
    _packageId = p?.packageId ?? (_choices.isEmpty ? null : _choices.first.id);
    _origin.text = p?.origin ?? '';
    _days.text = '7';
    _methods = p == null ? {'GET'} : p.methods.toSet();
    _profile = p?.profile ?? 1;
    _credential = p == null ? '' : _hex(p.credentialReference);
    _root = Uint8List.fromList(p?.rootCertificate ?? []);
    final values = [
      p?.maxRequestBytes ?? 65536,
      p?.maxResponseBytes ?? 65536,
      p?.maxHeaderBytes ?? 16384,
      p?.maxConcurrent ?? 1,
      p?.timeoutMs ?? 10000,
      p?.maxFrameBytes ?? 131072,
    ];
    var i = 0;
    for (final c in _limits.values) {
      c.text = '${values[i++]}';
    }
    setState(() {
      _form = true;
      _message = null;
    });
  }

  void _close() {
    _formEpoch++;
    setState(() {
      _form = false;
      _replacing = null;
      if (_writing) {
        // Closing an in-flight write invalidates its reply and requires a read.
        _epoch++;
        _busy = false;
        _writing = false;
        _trusted = false;
        _message = (l) => l.pluginsEndpointUnknown;
      }
    });
  }

  Future<void> _pickRoot() async {
    final backend = widget.backend, epoch = _epoch, formEpoch = _formEpoch;
    try {
      final file =
          await (widget.pickCertificate?.call() ??
              openFile(
                acceptedTypeGroups: const [
                  XTypeGroup(label: 'DER', extensions: ['der', 'cer']),
                ],
              ));
      if (!_current(backend, epoch) ||
          formEpoch != _formEpoch ||
          !_form ||
          file == null) {
        return;
      }
      if (!RegExp(r'\.(der|cer)$', caseSensitive: false).hasMatch(file.name) ||
          await file.length() > 32768) {
        throw const FormatException('Invalid certificate');
      }
      final length = await file.length();
      if (length < 1 || length > 32768) {
        throw const FormatException('Invalid certificate size');
      }
      final builder = BytesBuilder(copy: false);
      await for (final chunk in file.openRead(0, length)) {
        if (builder.length + chunk.length > 32768) {
          throw const FormatException('Certificate too large');
        }
        builder.add(chunk);
      }
      final bytes = builder.takeBytes();
      if (bytes.length != length || await file.length() != length) {
        throw const FormatException('Certificate changed while reading');
      }
      if (!_singleDerCertificate(bytes)) {
        throw const FormatException('Invalid DER');
      }
      if (_current(backend, epoch) &&
          formEpoch == _formEpoch &&
          _form &&
          !_busy) {
        setState(() {
          _root = bytes;
          _message = null;
        });
      }
    } catch (_) {
      if (_current(backend, epoch) &&
          formEpoch == _formEpoch &&
          _form &&
          !_busy) {
        setState(() => _message = (l) => l.pluginsEndpointCertificateInvalid);
      }
    }
  }

  EndpointPolicy? _takePolicy() {
    final p = _selected;
    final days = int.tryParse(_days.text);
    final limits = _limits.values.map((c) => int.tryParse(c.text)).toList();
    final uri = Uri.tryParse(_origin.text.trim());
    final isLocal = uri != null && _loopback(uri.host);
    if (p == null ||
        days == null ||
        days < 1 ||
        days > 30 ||
        limits.indexed.any(
          (e) =>
              e.$2 == null ||
              e.$2! < 1 ||
              e.$2! > const [65536, 65536, 16384, 128, 30000, 131072][e.$1],
        ) ||
        _methods.isEmpty ||
        _methods.any(
          (m) => !const [
            'GET',
            'HEAD',
            'POST',
            'PUT',
            'PATCH',
            'DELETE',
            'OPTIONS',
          ].contains(m),
        ) ||
        uri == null ||
        !uri.hasAuthority ||
        uri.host.isEmpty ||
        uri.userInfo.isNotEmpty ||
        uri.authority.contains('@') ||
        uri.port < 1 ||
        uri.port > 65535 ||
        uri.hasQuery ||
        uri.hasFragment ||
        (uri.path.isNotEmpty && uri.path != '/') ||
        ![1, 2, 3].contains(_profile) ||
        (_profile == 1 && (uri.scheme != 'https' || isLocal)) ||
        (_profile == 2 && (uri.scheme != 'http' || !isLocal)) ||
        (_profile == 3 && (uri.scheme != 'https' || !isLocal)) ||
        (_root.isNotEmpty &&
            (_profile == 2 || !_singleDerCertificate(_root)))) {
      return null;
    }
    Uint8List credential = Uint8List(0);
    if (_credential.isNotEmpty) {
      final matches = _validCredentials.where(
        (c) => _hex(c.reference) == _credential,
      );
      if (!_canUseCredential(p) || matches.isEmpty) return null;
      // Preserve the requested duration. The server rejects a shorter credential.
      credential = Uint8List.fromList(matches.single.reference);
    }
    return EndpointPolicy(
      packageId: p.id,
      packageDigest: Uint8List.fromList(p.digest),
      origin: uri.origin,
      profile: _profile,
      methods: _methods.toList()..sort(),
      credentialReference: credential,
      rootCertificate: Uint8List.fromList(_root),
      maxRequestBytes: limits[0]!,
      maxResponseBytes: limits[1]!,
      maxHeaderBytes: limits[2]!,
      maxConcurrent: limits[3]!,
      timeoutMs: limits[4]!,
      maxFrameBytes: limits[5]!,
    );
  }

  static bool _samePolicy(EndpointPolicy a, EndpointPolicy b) =>
      a.packageId == b.packageId &&
      _compare(a.packageDigest, b.packageDigest) == 0 &&
      a.origin == b.origin &&
      a.profile == b.profile &&
      (a.methods.toList()..sort()).join(',') ==
          (b.methods.toList()..sort()).join(',') &&
      _compare(a.credentialReference, b.credentialReference) == 0 &&
      _compare(a.rootCertificate, b.rootCertificate) == 0 &&
      a.maxRequestBytes == b.maxRequestBytes &&
      a.maxResponseBytes == b.maxResponseBytes &&
      a.maxHeaderBytes == b.maxHeaderBytes &&
      a.maxConcurrent == b.maxConcurrent &&
      a.timeoutMs == b.timeoutMs &&
      a.maxFrameBytes == b.maxFrameBytes;

  void _unknown() => setState(() {
    _trusted = false;
    _form = false;
    _replacing = null;
    _formEpoch++;
    _message = (l) => l.pluginsEndpointUnknown;
  });
  Future<void> _save() async {
    if (_busy || !_trusted || !_form) return;
    final policy = _takePolicy();
    if (policy == null) {
      setState(() => _message = (l) => l.pluginsEndpointInvalid);
      return;
    }
    final backend = widget.backend, epoch = ++_epoch, formEpoch = _formEpoch;
    final old = _replacing;
    setState(() {
      _busy = true;
      _writing = true;
      _message = null;
    });
    try {
      final raw = await backend.saveEndpoint(
        reference: Uint8List.fromList(old?.reference ?? []),
        expectedRevision: old?.revision ?? BigInt.zero,
        registryRevision: widget.registryRevision,
        lifetimeDays: int.parse(_days.text),
        policy: policy,
      );
      if (!_current(backend, epoch) || formEpoch != _formEpoch) return;
      final saved = _checked(raw);
      if (saved.disabled ||
          saved.revision != (old?.revision ?? BigInt.zero) + BigInt.one ||
          (old != null && _compare(saved.reference, old.reference) != 0) ||
          (old == null &&
              _entries.any(
                (e) => _compare(e.reference, saved.reference) == 0,
              )) ||
          !_samePolicy(saved.policy, policy)) {
        throw const FormatException('Unexpected endpoint acknowledgement');
      }
      setState(() {
        _entries = [
          ..._entries.where((e) => _compare(e.reference, saved.reference) != 0),
          saved,
        ]..sort((a, b) => _compare(a.reference, b.reference));
        _form = false;
        _replacing = null;
        _formEpoch++;
        _message = (l) => l.pluginsEndpointSaved;
      });
    } catch (_) {
      if (_current(backend, epoch) && formEpoch == _formEpoch) _unknown();
    } finally {
      if (_current(backend, epoch)) {
        setState(() {
          _busy = false;
          _writing = false;
        });
      }
    }
  }

  Future<void> _disable(StoredEndpoint expected) async {
    if (_busy || !_trusted || expected.disabled) return;
    final backend = widget.backend, epoch = ++_epoch;
    _formEpoch++;
    setState(() {
      _busy = true;
      _writing = true;
      _form = false;
      _replacing = null;
      _message = null;
    });
    try {
      final raw = await backend.disableEndpoint(expected);
      if (!_current(backend, epoch)) return;
      final saved = _checked(raw);
      if (!saved.disabled ||
          saved.revision != expected.revision + BigInt.one ||
          _compare(saved.reference, expected.reference) != 0 ||
          !_samePolicy(saved.policy, expected.policy) ||
          saved.createdMs != expected.createdMs ||
          saved.expiresMs != expected.expiresMs) {
        throw const FormatException('Unexpected disable acknowledgement');
      }
      setState(() {
        _entries = _entries
            .map((e) => _compare(e.reference, saved.reference) == 0 ? saved : e)
            .toList();
        _message = (l) => l.pluginsEndpointDisabledDone;
      });
    } catch (_) {
      if (_current(backend, epoch)) _unknown();
    } finally {
      if (_current(backend, epoch)) {
        setState(() {
          _busy = false;
          _writing = false;
        });
      }
    }
  }

  Widget _note(String text) => Padding(
    padding: const EdgeInsets.symmetric(vertical: 5),
    child: Text(
      text,
      style: TextStyle(color: widget.muted, fontSize: 11, height: 1.6),
    ),
  );
  Widget _button(String label, String key, VoidCallback? action) =>
      OutlinedButton(
        key: ValueKey(key),
        onPressed: _busy ? null : action,
        style: OutlinedButton.styleFrom(
          foregroundColor: widget.ink,
          minimumSize: const Size(0, 38),
          side: BorderSide(color: widget.line),
          shape: RoundedRectangleBorder(borderRadius: widget.radius),
        ),
        child: Text(label, style: const TextStyle(fontSize: 11)),
      );
  Widget _field(
    String label,
    String key,
    TextEditingController controller, {
    bool numeric = false,
  }) => Padding(
    padding: const EdgeInsets.symmetric(vertical: 6),
    child: TextField(
      key: ValueKey('endpoint-$key'),
      controller: controller,
      enabled: !_busy,
      keyboardType: numeric ? TextInputType.number : TextInputType.url,
      decoration: InputDecoration(labelText: label),
      style: TextStyle(color: widget.ink, fontSize: 12),
    ),
  );

  Widget _editor(AppLocalizations l) {
    final selected = _selected;
    final credentials = _validCredentials;
    final credentialKnown =
        _credential.isEmpty ||
        credentials.any((c) => _hex(c.reference) == _credential);
    final labels = [
      l.pluginsEndpointRequestBytes,
      l.pluginsEndpointResponseBytes,
      l.pluginsEndpointHeaderBytes,
      l.pluginsEndpointConcurrency,
      l.pluginsEndpointTimeout,
      l.pluginsEndpointFrameBytes,
    ];
    var i = 0;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        _note(
          _replacing == null
              ? l.pluginsEndpointCreateTitle
              : l.pluginsEndpointReplaceTitle,
        ),
        DropdownButtonFormField<String>(
          key: ValueKey('endpoint-package-$_formEpoch'),
          initialValue: selected?.id,
          isExpanded: true,
          decoration: InputDecoration(labelText: l.pluginsEndpointPackage),
          items: _choices
              .map(
                (p) => DropdownMenuItem(
                  value: p.id,
                  child: Text(
                    '${p.name} (${p.id})',
                    overflow: TextOverflow.ellipsis,
                  ),
                ),
              )
              .toList(),
          onChanged: _busy || _replacing != null
              ? null
              : (id) => setState(() {
                  _packageId = id;
                  _credential = '';
                }),
        ),
        if (selected == null)
          _note(l.pluginsEndpointPackageUnavailable)
        else
          _note('${l.pluginsEndpointDigest}: ${_hex(selected.digest)}'),
        _field(l.pluginsEndpointOrigin, 'origin', _origin),
        DropdownButtonFormField<int>(
          key: ValueKey('endpoint-profile-$_formEpoch'),
          initialValue: [1, 2, 3].contains(_profile) ? _profile : null,
          isExpanded: true,
          decoration: InputDecoration(labelText: l.pluginsEndpointProfile),
          items: [
            DropdownMenuItem(
              value: 1,
              child: Text(l.pluginsEndpointPublicHttps),
            ),
            DropdownMenuItem(value: 2, child: Text(l.pluginsEndpointLocalHttp)),
            DropdownMenuItem(
              value: 3,
              child: Text(l.pluginsEndpointLocalHttps),
            ),
          ],
          onChanged: _busy ? null : (v) => setState(() => _profile = v!),
        ),
        _note(l.pluginsEndpointMethods),
        Wrap(
          spacing: 6,
          runSpacing: 6,
          children:
              {
                    'GET',
                    'HEAD',
                    'POST',
                    'PUT',
                    'PATCH',
                    'DELETE',
                    'OPTIONS',
                    ..._methods,
                  }
                  .map(
                    (m) => FilterChip(
                      key: ValueKey('endpoint-method-$m'),
                      label: Text(m),
                      selected: _methods.contains(m),
                      onSelected: _busy
                          ? null
                          : (v) => setState(() {
                              if (v) {
                                _methods.add(m);
                              } else {
                                _methods.remove(m);
                              }
                            }),
                    ),
                  )
                  .toList(),
        ),
        _field(l.pluginsEndpointLifetime, 'days', _days, numeric: true),
        _note(l.pluginsEndpointCredentialLifetime),
        DropdownButtonFormField<String>(
          key: ValueKey('endpoint-credential-$_formEpoch-$_packageId'),
          initialValue: credentialKnown ? _credential : null,
          isExpanded: true,
          decoration: InputDecoration(labelText: l.pluginsEndpointCredential),
          items: [
            DropdownMenuItem(
              value: '',
              child: Text(l.pluginsEndpointNoCredential),
            ),
            ...credentials.map(
              (c) => DropdownMenuItem(
                value: _hex(c.reference),
                child: Text(
                  '${_hex(c.reference).substring(0, 8)}…${_hex(c.reference).substring(56)}',
                  overflow: TextOverflow.ellipsis,
                ),
              ),
            ),
          ],
          onChanged: _busy ? null : (v) => setState(() => _credential = v!),
        ),
        if (!credentialKnown ||
            (selected != null && !_canUseCredential(selected)))
          _note(l.pluginsEndpointCredentialUnavailable),
        _note(l.pluginsEndpointAdvanced),
        for (final e in _limits.entries)
          _field(labels[i++], e.key, e.value, numeric: true),
        if (_profile != 2 || _root.isNotEmpty)
          _note(l.pluginsEndpointCertificateDetails),
        if (_root.isNotEmpty)
          _note(l.pluginsEndpointCertificateSelected(_root.length)),
        Wrap(
          spacing: 8,
          runSpacing: 8,
          children: [
            if (_profile != 2)
              _button(
                l.pluginsEndpointCertificate,
                'endpoint-certificate',
                _pickRoot,
              ),
            if (_root.isNotEmpty)
              _button(
                l.pluginsEndpointRemoveCertificate,
                'endpoint-remove-certificate',
                () => setState(() => _root = Uint8List(0)),
              ),
          ],
        ),
        const SizedBox(height: 12),
        Wrap(
          spacing: 8,
          runSpacing: 8,
          children: [
            _button(
              l.pluginsEndpointSave,
              'endpoint-save',
              selected == null ? null : _save,
            ),
            TextButton(
              key: const ValueKey('endpoint-close'),
              onPressed: _close,
              child: Text(l.pluginsCredentialCancel),
            ),
          ],
        ),
      ],
    );
  }

  @override
  Widget build(BuildContext context) {
    final l = L10n.of(context);
    return Padding(
      padding: const EdgeInsets.all(19),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            l.pluginsEndpointTitle,
            style: TextStyle(
              color: widget.ink,
              fontSize: 13,
              fontWeight: FontWeight.w600,
            ),
          ),
          _note(l.pluginsEndpointDetails),
          Wrap(
            spacing: 8,
            runSpacing: 8,
            children: [
              _button(
                l.pluginsEndpointNew,
                'endpoint-new',
                _trusted && widget.plugins.any(_canUse) ? () => _open() : null,
              ),
              _button(l.pluginsCredentialRefresh, 'endpoint-refresh', _refresh),
            ],
          ),
          if (_busy) _note(l.pluginsEndpointWorking),
          if (_message != null) _note(_message!(l)),
          if (_trusted && _entries.isEmpty) _note(l.pluginsEndpointEmpty),
          if (_form) _editor(l),
          for (final e in _entries)
            Container(
              key: ValueKey('endpoint-row-${_hex(e.reference)}'),
              width: double.infinity,
              margin: const EdgeInsets.only(top: 10),
              padding: const EdgeInsets.all(12),
              decoration: BoxDecoration(
                border: Border.all(color: widget.line),
                borderRadius: widget.radius,
              ),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(e.policy.origin, style: TextStyle(color: widget.ink)),
                  _note(
                    '${e.policy.packageId}\n${l.pluginsEndpointDigest}: ${_hex(e.policy.packageDigest)}',
                  ),
                  _note(
                    '${e.policy.methods.join(', ')} · ${e.disabled
                        ? l.pluginsCredentialDisabled
                        : e.expiresMs <= BigInt.from(DateTime.now().millisecondsSinceEpoch)
                        ? l.pluginsCredentialExpired
                        : l.pluginsCredentialStored}',
                  ),
                  _note(
                    l.pluginsCredentialExpires(
                      MaterialLocalizations.of(context).formatMediumDate(
                        DateTime.fromMillisecondsSinceEpoch(
                          e.expiresMs.toInt(),
                        ),
                      ),
                    ),
                  ),
                  Wrap(
                    spacing: 8,
                    runSpacing: 8,
                    children: [
                      _button(
                        l.pluginsEndpointReplace,
                        'endpoint-replace-${_hex(e.reference)}',
                        _trusted &&
                                widget.plugins.any(
                                  (p) =>
                                      p.id == e.policy.packageId && _canUse(p),
                                )
                            ? () => _open(e)
                            : null,
                      ),
                      _button(
                        l.pluginsCredentialDisable,
                        'endpoint-disable-${_hex(e.reference)}',
                        _trusted && !e.disabled ? () => _disable(e) : null,
                      ),
                    ],
                  ),
                ],
              ),
            ),
        ],
      ),
    );
  }
}

/// Bounded structural DER validation. Cryptographic trust is checked by native code.
/// Requires one complete Certificate sequence, never a PEM string or a bundle.
bool _singleDerCertificate(Uint8List bytes) {
  if (bytes.isEmpty || bytes.length > 32768) return false;
  try {
    ({int tag, int start, int end}) read(int offset, int bound) {
      if (offset + 2 > bound) throw const FormatException();
      final tag = bytes[offset++];
      if ((tag & 0x1f) == 0x1f) throw const FormatException();
      var length = bytes[offset++];
      if ((length & 0x80) != 0) {
        final count = length & 0x7f;
        if (count == 0 ||
            count > 3 ||
            offset + count > bound ||
            bytes[offset] == 0) {
          throw const FormatException();
        }
        length = 0;
        for (var i = 0; i < count; i++) {
          length = (length << 8) | bytes[offset++];
        }
        if (length < 128) throw const FormatException();
      }
      if (offset + length > bound) throw const FormatException();
      return (tag: tag, start: offset, end: offset + length);
    }

    final outer = read(0, bytes.length);
    if (outer.tag != 0x30 || outer.end != bytes.length) return false;
    final tbs = read(outer.start, outer.end);
    final algorithm = read(tbs.end, outer.end);
    final signature = read(algorithm.end, outer.end);
    if (tbs.tag != 0x30 ||
        algorithm.tag != 0x30 ||
        algorithm.start == algorithm.end ||
        signature.tag != 0x03 ||
        signature.end != outer.end ||
        signature.end - signature.start < 2 ||
        bytes[signature.start] != 0) {
      return false;
    }
    var offset = tbs.start;
    if (bytes[offset] == 0xa0) {
      offset = read(offset, tbs.end).end;
    }
    for (final tag in [0x02, 0x30, 0x30, 0x30, 0x30, 0x30]) {
      final part = read(offset, tbs.end);
      if (part.tag != tag || part.start == part.end) return false;
      offset = part.end;
    }
    var previous = 0x80;
    while (offset < tbs.end) {
      final part = read(offset, tbs.end);
      if (![0x81, 0x82, 0xa3].contains(part.tag) || part.tag <= previous) {
        return false;
      }
      previous = part.tag;
      offset = part.end;
    }
    return offset == tbs.end;
  } catch (_) {
    return false;
  }
}
