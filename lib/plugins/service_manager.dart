import 'dart:async';
import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:morrow_i18n/morrow_i18n.dart';

import 'plugin_library.dart';
import 'service_control.dart';
import 'service_session.dart';
import 'session_view_state.dart';

String _hex(List<int> bytes) =>
    bytes.map((v) => v.toRadixString(16).padLeft(2, '0')).join();
bool _same(List<int> a, List<int> b) =>
    ServiceValidation.compareBytes(a, b) == 0;
int _textOrder(String a, String b) =>
    ServiceValidation.compareBytes(utf8.encode(a), utf8.encode(b));

class _ScopeDraft {
  _ScopeDraft([ServiceContentScope? original])
    : kind = original?.kind ?? 2,
      card = TextEditingController(text: original?.cardId ?? ''),
      attachment = TextEditingController(text: original?.attachmentId ?? '');
  int kind;
  final TextEditingController card, attachment;
  ServiceContentScope get value => ServiceContentScope(
    kind: kind,
    cardId: card.text,
    attachmentId: attachment.text,
  );
  void dispose() {
    card.dispose();
    attachment.dispose();
  }
}

class _PrincipalDraft {
  _PrincipalDraft(
    this.id,
    Uint8List reference,
    List<ServiceContentScope> original,
  ) : reference = Uint8List.fromList(reference),
      scopes = original.map(_ScopeDraft.new).toList();
  final String id;
  final Uint8List reference;
  final List<_ScopeDraft> scopes;
  ServicePrincipal get value {
    final values = scopes.map((s) => s.value).toList()
      ..sort((a, b) {
        var order = a.kind.compareTo(b.kind);
        if (order == 0) order = _textOrder(a.cardId, b.cardId);
        if (order == 0) order = _textOrder(a.attachmentId, b.attachmentId);
        return order;
      });
    return ServicePrincipal(
      id: id,
      authenticationReference: reference,
      scopes: values,
    );
  }

  void dispose() {
    for (final scope in scopes) {
      scope.dispose();
    }
  }
}

/// Edits desired configuration and approvals. Saving never starts a listener.
class ServiceManager extends StatefulWidget {
  const ServiceManager({
    super.key,
    required this.backend,
    required this.plugins,
    required this.registryRevision,
    required this.ink,
    required this.muted,
    required this.line,
    required this.radius,
  });
  final WorkbenchServiceControl backend;
  final List<PluginLibraryEntry> plugins;
  final BigInt? registryRevision;
  final Color ink, muted, line;
  final BorderRadius radius;
  @override
  State<ServiceManager> createState() => _ServiceManagerState();
}

class _ServiceManagerState extends State<ServiceManager>
    with SessionViewState<ServiceManager> {
  late ServiceSession _session;
  int _attachment = 0, _formEpoch = 0;
  String? _form;
  bool _invalid = false;
  bool _copying = false;
  String _boundDirectory = '';
  StoredServiceConfig? _editing, _publicationConfig;
  StoredServiceAuthority? _rotating, _publicationOld;
  String? _packageId, _handler, _publicationReference;
  Uint8List? _packageDigest;
  final _service = TextEditingController();
  final _retention = TextEditingController(text: '86400000');
  final _authPrincipal = TextEditingController();
  final _authDays = TextEditingController(text: '7');
  final _address = TextEditingController(text: '127.0.0.1:8080');
  final _path = TextEditingController(text: '/invoke');
  final _query = TextEditingController(text: '/result');
  final _publicationDays = TextEditingController(text: '1');
  final Map<String, _PrincipalDraft> _principals = {};
  String _method = 'POST';
  bool _tls = false;
  static const _methods = [
    'GET',
    'HEAD',
    'POST',
    'PUT',
    'PATCH',
    'DELETE',
    'OPTIONS',
  ];

  String get _directory =>
      '${widget.registryRevision}|${widget.plugins.map((p) => '${p.id}:${_hex(p.digest)}:${p.available}:${p.declaredIo.join(',')}:${p.approvedIo.join(',')}:${p.ioHandlers.join(',')}').join('|')}';
  bool _usable(PluginLibraryEntry p) =>
      p.available &&
      p.digest.length == 32 &&
      const [
        'http-listen',
        'http-publish',
      ].every((c) => p.declaredIo.contains(c) && p.approvedIo.contains(c)) &&
      p.ioHandlers.isNotEmpty;
  bool _active(StoredServiceAuthority a) =>
      !a.disabled &&
      a.createdMs <= BigInt.from(DateTime.now().millisecondsSinceEpoch) &&
      a.expiresMs > BigInt.from(DateTime.now().millisecondsSinceEpoch);
  List<PluginLibraryEntry> get _packages => widget.plugins
      .where(
        (p) =>
            _usable(p) &&
            (_editing == null || _same(p.digest, _editing!.packageDigest)),
      )
      .toList();
  PluginLibraryEntry? _package(String? id, List<int>? digest) {
    for (final p in widget.plugins) {
      if (_usable(p) &&
          (id == null || id == p.id) &&
          digest != null &&
          _same(p.digest, digest)) {
        return p;
      }
    }
    return null;
  }

  PluginLibraryEntry? get _selectedPackage =>
      _package(_packageId, _packageDigest);
  StoredServiceConfig? _config(String id) {
    for (final c in _session.configs) {
      if (c.id == id) {
        return c;
      }
    }
    return null;
  }

  StoredServiceAuthority? _authority(String? reference) {
    for (final a in _session.authorities) {
      if (_hex(a.reference) == reference) {
        return a;
      }
    }
    return null;
  }

  bool get _bound =>
      widget.registryRevision != null && _boundDirectory == _directory;
  bool get _configCurrent =>
      _editing == null ||
      (_config(_editing!.id)?.revision == _editing!.revision &&
          _same(_config(_editing!.id)!.digest, _editing!.digest));
  bool get _publicationCurrent {
    final old = _publicationConfig;
    if (old == null) {
      return false;
    }
    final current = _config(old.id);
    final authority = _authority(_publicationReference);
    return current != null &&
        !current.disabled &&
        current.revision == old.revision &&
        _same(current.digest, old.digest) &&
        current.approvalReferences.any(
          (r) => _hex(r) == _publicationReference,
        ) &&
        (authority == null ||
            authority.kind == 2 && authority.publication?.configId == old.id) &&
        (authority?.revision ?? BigInt.zero) ==
            (_publicationOld?.revision ?? BigInt.zero);
  }

  bool get _authsCurrent => _principals.values.every((p) {
    final auth = _authority(_hex(p.reference));
    return auth != null &&
        auth.kind == 1 &&
        auth.principalId == p.id &&
        _active(auth);
  });

  void _attach() {
    _session = ServiceSession.forBackend(widget.backend);
    _session.addListener(_changed);
    _session.attach();
    if (!_session.busy) {
      unawaited(_session.refresh());
    }
  }

  void _changed() {
    markSessionViewDirty();
  }

  @override
  void initState() {
    super.initState();
    _attach();
  }

  @override
  void didUpdateWidget(covariant ServiceManager oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (!identical(oldWidget.backend, widget.backend)) {
      _attachment++;
      _copying = false;
      _session.removeListener(_changed);
      _session.detach();
      _clearDraft();
      _attach();
    }
    // Registry and locale changes intentionally preserve every draft field.
    // A registry mismatch only disables save until the user rebinds selection.
  }

  void _clearDraft() {
    _formEpoch++;
    _form = null;
    _invalid = false;
    _editing = null;
    _rotating = null;
    _publicationConfig = null;
    _publicationOld = null;
    _packageId = null;
    _packageDigest = null;
    _handler = null;
    _publicationReference = null;
    for (final principal in _principals.values) {
      principal.dispose();
    }
    _principals.clear();
  }

  @override
  void dispose() {
    _attachment++;
    _session.removeListener(_changed);
    _session.detach();
    _clearDraft();
    for (final c in [
      _service,
      _retention,
      _authPrincipal,
      _authDays,
      _address,
      _path,
      _query,
      _publicationDays,
    ]) {
      c.dispose();
    }
    super.dispose();
  }

  void _openConfig([StoredServiceConfig? config]) {
    setState(() {
      _clearDraft();
      _form = 'config';
      _editing = config;
      _boundDirectory = _directory;
      _service.text = config?.service ?? '';
      _retention.text = (config?.retentionMs ?? BigInt.from(86400000))
          .toString();
      _handler = config?.handler;
      if (config != null) {
        _packageDigest = Uint8List.fromList(config.packageDigest);
        _packageId = _package(null, config.packageDigest)?.id;
        for (final p in config.principals) {
          _principals[p.id] = _PrincipalDraft(
            p.id,
            p.authenticationReference,
            p.scopes,
          );
        }
      } else if (_packages.length == 1) {
        final package = _packages.single;
        _packageId = package.id;
        _packageDigest = Uint8List.fromList(package.digest);
        if (package.ioHandlers.length == 1) {
          _handler = package.ioHandlers.single;
        }
      }
    });
  }

  void _openAuth([StoredServiceAuthority? auth]) {
    setState(() {
      _clearDraft();
      _form = 'auth';
      _rotating = auth;
      _authPrincipal.text = auth?.principalId ?? '';
      _authDays.text = '7';
    });
  }

  void _choosePublication(
    StoredServiceConfig config, {
    String? reference,
    bool loadPolicy = true,
  }) {
    _publicationConfig = config;
    _publicationReference =
        reference ??
        (config.approvalReferences.isEmpty
            ? null
            : _hex(config.approvalReferences.first));
    _publicationOld = _authority(_publicationReference);
    _boundDirectory = _directory;
    if (loadPolicy) {
      final policy = _publicationOld?.publication;
      _address.text = policy?.listenAddress ?? '127.0.0.1:8080';
      _path.text = policy?.path ?? '/invoke';
      _query.text = policy?.queryPath ?? '/result';
      _method = policy?.method ?? 'POST';
      _tls = policy?.tlsRequired ?? false;
      _publicationDays.text = '1';
    }
  }

  void _openPublication([
    StoredServiceConfig? config,
    StoredServiceAuthority? authority,
  ]) {
    setState(() {
      _clearDraft();
      _form = 'publication';
      _boundDirectory = _directory;
      config ??= authority?.publication == null
          ? null
          : _config(authority!.publication!.configId);
      if (config != null) {
        _choosePublication(
          config!,
          reference: authority == null ? null : _hex(authority.reference),
        );
      }
    });
  }

  void _selectAuth(StoredServiceAuthority auth, bool selected) {
    setState(() {
      final old = _principals.remove(auth.principalId);
      if (selected) {
        _principals[auth.principalId] = _PrincipalDraft(
          auth.principalId,
          auth.reference,
          old?.scopes.map((s) => s.value).toList() ?? [],
        );
      }
      old?.dispose();
    });
  }

  void _rebind() {
    setState(() {
      if (_form == 'config') {
        final selected = _selectedPackage;
        if (selected != null) {
          _boundDirectory = _directory;
        }
      } else if (_publicationConfig != null) {
        final current = _config(_publicationConfig!.id);
        if (current != null &&
            current.approvalReferences.any(
              (r) => _hex(r) == _publicationReference,
            )) {
          _choosePublication(
            current,
            reference: _publicationReference,
            loadPolicy: false,
          );
        }
      }
    });
  }

  Future<void> _save() async {
    final session = _session,
        epoch = _attachment,
        formEpoch = _formEpoch,
        directory = _directory;
    if (!session.canWrite) {
      return;
    }
    Future<bool> operation;
    try {
      if (_form == 'config') {
        final package = _selectedPackage;
        if (!_bound ||
            !_configCurrent ||
            !_authsCurrent ||
            package == null ||
            !package.ioHandlers.contains(_handler)) {
          throw const FormatException('Stale selection');
        }
        final principals = _principals.values.map((p) => p.value).toList()
          ..sort((a, b) => _textOrder(a.id, b.id));
        final value = ServiceConfigUpdate(
          id: _editing?.id ?? '',
          expectedRevision: _editing?.revision ?? BigInt.zero,
          registryRevision: widget.registryRevision!,
          packageId: package.id,
          packageDigest: package.digest,
          service: _service.text,
          handler: _handler!,
          retentionMs: BigInt.parse(_retention.text),
          principals: principals,
        );
        ServiceValidation.configUpdate(value);
        operation = session.saveConfig(value);
      } else if (_form == 'auth') {
        final days = int.parse(_authDays.text);
        ServiceValidation.principalId(_authPrincipal.text);
        ServiceValidation.days(days);
        if (_rotating != null &&
            _authority(_hex(_rotating!.reference))?.revision !=
                _rotating!.revision) {
          throw const FormatException('Stale authentication');
        }
        operation = session.issueAuthentication(
          reference: _rotating?.reference ?? Uint8List(0),
          expectedRevision: _rotating?.revision ?? BigInt.zero,
          principalId: _authPrincipal.text,
          lifetimeDays: days,
        );
      } else if (_form == 'publication') {
        final config = _publicationConfig;
        final package = config == null
            ? null
            : _package(null, config.packageDigest);
        if (!_bound ||
            !_publicationCurrent ||
            config == null ||
            package == null ||
            !_methods.contains(_method)) {
          throw const FormatException('Stale publication');
        }
        final reference = config.approvalReferences.firstWhere(
          (r) => _hex(r) == _publicationReference,
        );
        final value = ServicePublicationUpdate(
          reference: reference,
          expectedRevision: _publicationOld?.revision ?? BigInt.zero,
          configRevision: config.revision,
          registryRevision: widget.registryRevision!,
          packageId: package.id,
          lifetimeDays: int.parse(_publicationDays.text),
          policy: ServicePublication(
            configId: config.id,
            configDigest: config.digest,
            listenAddress: _address.text,
            tlsRequired: _tls,
            method: _method,
            path: _path.text,
            queryPath: _query.text,
          ),
        );
        ServiceValidation.publicationUpdate(value);
        operation = session.savePublication(value);
      } else {
        return;
      }
    } catch (_) {
      setState(() => _invalid = true);
      return;
    }
    setState(() => _invalid = false);
    final saved = await operation;
    if (mounted &&
        epoch == _attachment &&
        identical(session, _session) &&
        formEpoch == _formEpoch &&
        directory == _directory &&
        saved) {
      setState(_clearDraft);
    }
  }

  Future<void> _copyToken() async {
    final session = _session, issued = session.issued, epoch = _attachment;
    if (_copying || issued == null || issued.token.isDisposed) {
      return;
    }
    setState(() => _copying = true);
    try {
      // Only this explicit action materializes text for the platform clipboard.
      // No token String is retained by state, controllers, history or notices.
      await Clipboard.setData(
        ClipboardData(text: String.fromCharCodes(issued.token.bytes)),
      );
    } catch (_) {
      if (mounted && epoch == _attachment) {
        setState(() => _invalid = true);
      }
    } finally {
      if (identical(session.issued, issued)) {
        session.clearToken();
      }
      if (mounted && epoch == _attachment && identical(session, _session)) {
        setState(() => _copying = false);
      }
    }
  }

  String _date(BigInt ms) {
    if (ms < BigInt.zero || ms > BigInt.from(8640000000000000)) {
      return ms.toString();
    }
    return DateTime.fromMillisecondsSinceEpoch(
      ms.toInt(),
      isUtc: true,
    ).toIso8601String();
  }

  String _scopeName(AppLocalizations l, int kind) => switch (kind) {
    1 => l.pluginsServiceScopeRename,
    2 => l.pluginsServiceScopeSummary,
    3 => l.pluginsServiceScopeQuery,
    4 => l.pluginsServiceScopeAttachment,
    5 => l.pluginsServiceScopeCreate,
    6 => l.pluginsServiceScopeEdit,
    7 => l.pluginsServiceScopeRead,
    _ => l.pluginsServiceUnsupported,
  };
  String _notice(AppLocalizations l, ServiceNotice value) => switch (value) {
    ServiceNotice.loadFailed => l.pluginsServiceLoadFailed,
    ServiceNotice.writeUnknown => l.pluginsServiceWriteUnknown,
    ServiceNotice.invalid => l.pluginsServiceInvalid,
    ServiceNotice.saved => l.pluginsServiceSaved,
    ServiceNotice.tokenDiscarded => l.pluginsServiceTokenDiscarded,
  };
  Widget _note(String text, {Key? key}) => Padding(
    padding: const EdgeInsets.symmetric(vertical: 5),
    child: Text(
      text,
      key: key,
      style: TextStyle(color: widget.muted, fontSize: 11, height: 1.5),
    ),
  );
  Widget _title(String text) => Text(
    text,
    style: TextStyle(
      color: widget.ink,
      fontSize: 13,
      fontWeight: FontWeight.w600,
    ),
  );
  Widget _panel(List<Widget> children, {Key? key}) => Container(
    key: key,
    width: double.infinity,
    margin: const EdgeInsets.symmetric(vertical: 7),
    padding: const EdgeInsets.all(12),
    decoration: BoxDecoration(
      border: Border.all(color: widget.line),
      borderRadius: widget.radius,
    ),
    child: Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: children,
    ),
  );
  Widget _button(String text, String key, VoidCallback? action) =>
      OutlinedButton(
        key: ValueKey('service-$key'),
        onPressed: action,
        style: OutlinedButton.styleFrom(
          foregroundColor: widget.ink,
          minimumSize: const Size(0, 38),
          side: BorderSide(color: widget.line),
          shape: RoundedRectangleBorder(borderRadius: widget.radius),
        ),
        child: Text(text, style: const TextStyle(fontSize: 11)),
      );
  Widget _actions(List<Widget> children) =>
      Wrap(spacing: 8, runSpacing: 6, children: children);
  Widget _field(
    String label,
    String key,
    TextEditingController controller, {
    bool enabled = true,
    bool numeric = false,
  }) => Padding(
    padding: const EdgeInsets.symmetric(vertical: 7),
    child: TextField(
      key: ValueKey('service-$key'),
      controller: controller,
      enabled: enabled && !_session.busy,
      keyboardType: numeric ? TextInputType.number : TextInputType.text,
      style: TextStyle(color: widget.ink, fontSize: 12),
      decoration: InputDecoration(labelText: label),
    ),
  );
  Widget _select<T>(
    String label,
    String key,
    T? value,
    List<DropdownMenuItem<T>> items,
    ValueChanged<T?>? changed,
  ) => Padding(
    padding: const EdgeInsets.symmetric(vertical: 7),
    child: DropdownButtonFormField<T>(
      key: ValueKey('service-$key'),
      initialValue: value,
      isExpanded: true,
      items: items,
      decoration: InputDecoration(labelText: label),
      onChanged: _session.busy ? null : changed,
      style: TextStyle(color: widget.ink, fontSize: 12),
    ),
  );
  DropdownMenuItem<T> _item<T>(T value, String label) => DropdownMenuItem(
    value: value,
    child: Text(label, overflow: TextOverflow.ellipsis),
  );

  Widget _configEditor(AppLocalizations l) {
    final package = _selectedPackage;
    final auths = _session.authorities
        .where((a) => a.kind == 1 && _active(a))
        .toList();
    final handlers = {...?package?.ioHandlers, ?_handler}.toList();
    return _panel([
      _title(
        _editing == null
            ? l.pluginsServiceNewConfig
            : l.pluginsServiceEditConfig,
      ),
      if (_editing != null)
        _note(
          '${l.pluginsServiceRevision}: ${_editing!.revision}\n${_editing!.id}',
        ),
      _select<String>(
        l.pluginsServicePackage,
        'package',
        _packages.any((p) => p.id == _packageId) ? _packageId : null,
        _packages.map((p) => _item(p.id, '${p.name} (${p.id})')).toList(),
        (id) => setState(() {
          final p = _packages.firstWhere((p) => p.id == id);
          _packageId = p.id;
          _packageDigest = Uint8List.fromList(p.digest);
          if (_handler == null && p.ioHandlers.length == 1) {
            _handler = p.ioHandlers.single;
          }
          _boundDirectory = _directory;
        }),
      ),
      if (_packageDigest != null)
        _note('${l.pluginsServicePackageDigest}: ${_hex(_packageDigest!)}'),
      if (package == null) _note(l.pluginsServicePackageUnavailable),
      if (_editing != null) _note(l.pluginsServiceDigestFixed),
      _select<String>(
        l.pluginsServiceHandler,
        'handler',
        _handler,
        handlers.map((v) => _item(v, v)).toList(),
        (v) => setState(() => _handler = v),
      ),
      _field(
        l.pluginsServiceIdentity,
        'service',
        _service,
        enabled: _editing == null,
      ),
      _field(
        l.pluginsServiceRetention,
        'retention-ms',
        _retention,
        numeric: true,
      ),
      _title(l.pluginsServicePrincipals),
      _note(l.pluginsServiceScopesHelp),
      if (auths.isEmpty) _note(l.pluginsServiceNoAuthentication),
      for (final auth in auths)
        CheckboxListTile(
          key: ValueKey('service-principal-${_hex(auth.reference)}'),
          contentPadding: EdgeInsets.zero,
          title: Text(
            auth.principalId,
            style: TextStyle(color: widget.ink, fontSize: 12),
          ),
          subtitle: Text(
            '${l.pluginsServiceExpires}: ${_date(auth.expiresMs)}',
            style: TextStyle(color: widget.muted, fontSize: 11),
          ),
          value:
              _principals[auth.principalId] != null &&
              _same(_principals[auth.principalId]!.reference, auth.reference),
          onChanged: _session.busy ? null : (v) => _selectAuth(auth, v!),
        ),
      for (final p in _principals.values) _principalEditor(l, p),
      if (!_configCurrent || !_authsCurrent)
        _note(l.pluginsServicePolicyChanged),
      if (!_bound) _note(l.pluginsServiceCatalogChanged),
      _actions([
        _button(
          l.pluginsServiceRefreshSelection,
          'config-rebind',
          _session.busy || package == null ? null : _rebind,
        ),
        _button(
          l.pluginsServiceSaveConfig,
          'config-save',
          _session.canWrite &&
                  _bound &&
                  _configCurrent &&
                  _authsCurrent &&
                  package != null
              ? () => unawaited(_save())
              : null,
        ),
        _button(
          l.pluginsServiceCloseEditor,
          'config-close',
          () => setState(_clearDraft),
        ),
      ]),
    ], key: const ValueKey('service-config-editor'));
  }

  Widget _principalEditor(AppLocalizations l, _PrincipalDraft p) {
    final auth = _authority(_hex(p.reference));
    final valid =
        auth != null &&
        _active(auth) &&
        auth.kind == 1 &&
        auth.principalId == p.id;
    return _panel([
      _title(p.id),
      _note('${l.pluginsServiceReference}: ${_hex(p.reference)}'),
      if (!valid) _note(l.pluginsServiceAuthenticationUnavailable),
      for (var i = 0; i < p.scopes.length; i++) _scopeEditor(l, p, i),
      _actions([
        _button(
          l.pluginsServiceAddScope,
          'scope-add-${p.id}',
          _session.busy
              ? null
              : () => setState(() => p.scopes.add(_ScopeDraft())),
        ),
        _button(
          l.pluginsServiceRemovePrincipal,
          'principal-remove-${p.id}',
          _session.busy
              ? null
              : () => setState(() {
                  _principals.remove(p.id);
                  p.dispose();
                }),
        ),
      ]),
    ]);
  }

  Widget _scopeEditor(AppLocalizations l, _PrincipalDraft p, int index) {
    final scope = p.scopes[index];
    return _panel([
      _select<int>(
        l.pluginsServiceScopeKind,
        'scope-kind-${p.id}-$index',
        scope.kind,
        [
          for (var n = 1; n <= 7; n++) _item(n, _scopeName(l, n)),
          if (scope.kind < 1 || scope.kind > 7)
            _item(scope.kind, l.pluginsServiceUnsupported),
        ],
        (kind) => setState(() => scope.kind = kind!),
      ),
      _field(l.pluginsServiceCardId, 'scope-card-${p.id}-$index', scope.card),
      if (scope.kind == 4 || scope.attachment.text.isNotEmpty)
        _field(
          l.pluginsServiceAttachmentId,
          'scope-attachment-${p.id}-$index',
          scope.attachment,
        ),
      _button(
        l.pluginsServiceRemoveScope,
        'scope-remove-${p.id}-$index',
        _session.busy
            ? null
            : () => setState(() {
                p.scopes.removeAt(index);
                scope.dispose();
              }),
      ),
    ]);
  }

  Widget _authEditor(AppLocalizations l) => _panel([
    _title(
      _rotating == null
          ? l.pluginsServiceNewAuthentication
          : l.pluginsServiceRotateAuthentication,
    ),
    _note(l.pluginsServiceTokenHelp),
    _field(
      l.pluginsServicePrincipalId,
      'auth-principal',
      _authPrincipal,
      enabled: _rotating == null,
    ),
    _field(l.pluginsServiceDays, 'auth-days', _authDays, numeric: true),
    _actions([
      _button(
        _rotating == null ? l.pluginsServiceIssue : l.pluginsServiceRotate,
        'auth-save',
        _session.canWrite ? () => unawaited(_save()) : null,
      ),
      _button(
        l.pluginsServiceCloseEditor,
        'auth-close',
        () => setState(_clearDraft),
      ),
    ]),
  ], key: const ValueKey('service-auth-editor'));
  Widget _publicationEditor(AppLocalizations l) {
    final selected = _publicationConfig;
    final choices = _session.configs.where((c) => !c.disabled).toList();
    final refs = selected?.approvalReferences ?? <Uint8List>[];
    final package = selected == null
        ? null
        : _package(null, selected.packageDigest);
    return _panel([
      _title(l.pluginsServicePublicationEditor),
      _note(l.pluginsServicePublicationHelp),
      _select<String>(
        l.pluginsServiceConfiguration,
        'publication-config',
        choices.any((c) => c.id == selected?.id) ? selected?.id : null,
        choices.map((c) => _item(c.id, '${c.service} (${c.id})')).toList(),
        (v) => setState(
          () => _choosePublication(choices.firstWhere((c) => c.id == v)),
        ),
      ),
      _select<String>(
        l.pluginsServiceReference,
        'publication-reference',
        refs.any((r) => _hex(r) == _publicationReference)
            ? _publicationReference
            : null,
        refs.map((r) => _item(_hex(r), _hex(r))).toList(),
        selected == null
            ? null
            : (v) => setState(() => _choosePublication(selected, reference: v)),
      ),
      if (selected != null)
        _note(
          '${l.pluginsServiceRevision}: ${selected.revision}\n${l.pluginsServiceConfigDigest}: ${_hex(selected.digest)}',
        ),
      if (_publicationOld != null)
        _note(
          '${l.pluginsServiceExpires}: ${_date(_publicationOld!.expiresMs)}',
        ),
      if (package == null) _note(l.pluginsServicePackageUnavailable),
      _field(l.pluginsServiceListenAddress, 'publication-address', _address),
      CheckboxListTile(
        key: const ValueKey('service-publication-tls'),
        contentPadding: EdgeInsets.zero,
        title: Text(
          l.pluginsServiceTls,
          style: TextStyle(color: widget.ink, fontSize: 12),
        ),
        value: _tls,
        onChanged: _session.busy ? null : (v) => setState(() => _tls = v!),
      ),
      _note(l.pluginsServiceTlsHelp),
      _select<String>(
        l.pluginsServiceMethod,
        'publication-method',
        _method,
        {..._methods, _method}.map((v) => _item(v, v)).toList(),
        (v) => setState(() => _method = v!),
      ),
      _field(l.pluginsServicePath, 'publication-path', _path),
      _field(l.pluginsServiceQueryPath, 'publication-query', _query),
      _field(
        l.pluginsServiceDays,
        'publication-days',
        _publicationDays,
        numeric: true,
      ),
      if (!_publicationCurrent) _note(l.pluginsServicePolicyChanged),
      if (!_bound) _note(l.pluginsServiceCatalogChanged),
      _actions([
        _button(
          l.pluginsServiceRefreshSelection,
          'publication-rebind',
          _session.busy ? null : _rebind,
        ),
        _button(
          l.pluginsServiceSavePublication,
          'publication-save',
          _session.canWrite && _bound && _publicationCurrent && package != null
              ? () => unawaited(_save())
              : null,
        ),
        _button(
          l.pluginsServiceCloseEditor,
          'publication-close',
          () => setState(_clearDraft),
        ),
      ]),
    ], key: const ValueKey('service-publication-editor'));
  }

  Widget _configRow(AppLocalizations l, StoredServiceConfig config) => _panel([
    _title(config.service),
    _note('${config.id}\n${l.pluginsServiceRevision}: ${config.revision}'),
    _note(
      '${l.pluginsServiceHandler}: ${config.handler}\n${l.pluginsServicePackageDigest}: ${_hex(config.packageDigest)}',
    ),
    if (config.disabled) _note(l.pluginsServiceDisabled),
    for (final principal in config.principals)
      _note(
        '${principal.id}: ${principal.scopes.map((s) => '${_scopeName(l, s.kind)} · ${s.cardId}${s.attachmentId.isEmpty ? '' : ' · ${s.attachmentId}'}').join('; ')}',
      ),
    _actions([
      _button(
        l.pluginsServiceEditConfig,
        'config-edit-${config.id}',
        _session.busy ? null : () => _openConfig(config),
      ),
      _button(
        l.pluginsServicePublicationEditor,
        'config-publish-${config.id}',
        _session.busy || config.disabled
            ? null
            : () => _openPublication(config),
      ),
      _button(
        l.pluginsServiceDisable,
        'config-disable-${config.id}',
        _session.canWrite && !config.disabled
            ? () => unawaited(_session.disableConfig(config))
            : null,
      ),
    ]),
  ], key: ValueKey('service-config-${config.id}'));
  Widget _authorityRow(AppLocalizations l, StoredServiceAuthority authority) {
    final ref = _hex(authority.reference), policy = authority.publication;
    final config = policy == null ? null : _config(policy.configId);
    final mismatch =
        policy != null &&
        (config == null ||
            config.disabled ||
            !_same(config.digest, policy.configDigest) ||
            !config.approvalReferences.any(
              (r) => _same(r, authority.reference),
            ));
    return _panel([
      _title(
        authority.kind == 1
            ? authority.principalId
            : policy?.configId ?? l.pluginsServiceUnsupported,
      ),
      _note(
        '${l.pluginsServiceReference}: $ref\n${l.pluginsServiceRevision}: ${authority.revision}',
      ),
      _note(
        '${l.pluginsServiceCreated}: ${_date(authority.createdMs)}\n${l.pluginsServiceExpires}: ${_date(authority.expiresMs)}',
      ),
      if (authority.disabled)
        _note(l.pluginsServiceDisabled)
      else if (!_active(authority))
        _note(l.pluginsServiceExpired),
      if (policy != null)
        _note(
          '${policy.listenAddress} · ${policy.method} ${policy.path}\n${l.pluginsServiceQueryPath}: ${policy.queryPath}\n${l.pluginsServiceTls}: ${policy.tlsRequired ? l.pluginsServiceYes : l.pluginsServiceNo}\n${l.pluginsServiceConfigDigest}: ${_hex(policy.configDigest)}',
        ),
      if (mismatch) _note(l.pluginsServicePublicationMismatch),
      _actions([
        if (authority.kind == 1)
          _button(
            l.pluginsServiceRotate,
            'auth-rotate-$ref',
            _session.busy ? null : () => _openAuth(authority),
          ),
        if (policy != null)
          _button(
            l.pluginsServiceEditPublication,
            'publication-edit-$ref',
            _session.busy ? null : () => _openPublication(config, authority),
          ),
        _button(
          l.pluginsServiceDisable,
          'authority-disable-$ref',
          _session.canWrite && !authority.disabled
              ? () => unawaited(_session.disableAuthority(authority))
              : null,
        ),
      ]),
    ], key: ValueKey('service-authority-$ref'));
  }

  @override
  Widget build(BuildContext context) {
    final l = AppLocalizations.of(context)!;
    final issued = _session.issued;
    return _panel([
      _title(l.pluginsServiceTitle),
      _note(l.pluginsServiceManagementOnly),
      _actions([
        _button(
          l.pluginsServiceRefresh,
          'refresh',
          _session.busy ? null : () => unawaited(_session.refresh()),
        ),
        _button(
          l.pluginsServiceNewConfig,
          'new-config',
          _session.busy ? null : () => _openConfig(),
        ),
        _button(
          l.pluginsServiceNewAuthentication,
          'new-auth',
          _session.busy ? null : () => _openAuth(),
        ),
        _button(
          l.pluginsServicePublicationEditor,
          'new-publication',
          _session.busy ? null : () => _openPublication(),
        ),
      ]),
      if (_session.busy) _note(l.pluginsServiceWorking),
      if (_session.notice != null)
        _note(
          _notice(l, _session.notice!),
          key: const ValueKey('service-notice'),
        ),
      if (_invalid)
        _note(l.pluginsServiceInvalid, key: const ValueKey('service-invalid')),
      if (_session.uncertain) ...[
        _note(l.pluginsServiceUncertainHelp),
        _button(
          l.pluginsServiceAcknowledgeUncertain,
          'acknowledge-uncertain',
          !_session.busy && _session.trusted
              ? () {
                  _session.acknowledgeUncertain();
                }
              : null,
        ),
      ],
      if (issued != null && !issued.token.isDisposed)
        _panel([
          _title(l.pluginsServiceIssuedToken), _note(l.pluginsServiceTokenHelp),
          _note(
            '${issued.authority.principalId}\n${l.pluginsServiceExpires}: ${_date(issued.authority.expiresMs)}',
          ),
          // Display-only text is rebuilt from the owned bytes, never cached by state.
          Text(
            String.fromCharCodes(issued.token.bytes),
            key: const ValueKey('service-token'),
            style: TextStyle(color: widget.ink, fontSize: 12),
          ),
          _actions([
            _button(
              l.pluginsServiceCopyClear,
              'token-copy',
              _copying ? null : () => unawaited(_copyToken()),
            ),
            _button(
              l.pluginsServiceClearToken,
              'token-clear',
              _session.clearToken,
            ),
          ]),
        ]),
      if (_form == 'config') _configEditor(l),
      if (_form == 'auth') _authEditor(l),
      if (_form == 'publication') _publicationEditor(l),
      _title(l.pluginsServiceConfigurations),
      if (_session.configs.isEmpty) _note(l.pluginsServiceNoConfigurations),
      ..._session.configs.map((c) => _configRow(l, c)),
      _title(l.pluginsServiceAuthorities),
      if (_session.authorities.isEmpty) _note(l.pluginsServiceNoAuthorities),
      ..._session.authorities.map((a) => _authorityRow(l, a)),
    ], key: const ValueKey('service-manager'));
  }
}
