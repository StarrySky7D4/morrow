import 'dart:async';
import 'dart:math';

import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:morrow_i18n/morrow_i18n.dart';

import 'io_task_models.dart';
import 'endpoint_control.dart';
import 'service_endpoint_catalog.dart';
import 'plugin_library.dart';
import 'service_control.dart';
import 'service_run_control.dart';
import 'service_tls_picker.dart';
import 'service_run_session.dart';
import 'service_session.dart';
import 'session_view_state.dart';

String _hex(List<int> bytes) =>
    bytes.map((v) => v.toRadixString(16).padLeft(2, '0')).join();

class _RunChoice {
  const _RunChoice(this.config, this.publication, this.plugin, this.registry);
  final StoredServiceConfig config;
  final StoredServiceAuthority publication;
  final PluginLibraryEntry plugin;
  final BigInt registry;
  String get key =>
      '${config.id}:${config.revision}:${_hex(config.digest)}:'
      '${_hex(publication.reference)}:${publication.revision}:'
      '${plugin.id}:${_hex(plugin.digest)}:$registry';
}

/// Explicit finite service runs using the existing stored approvals and owner.
/// The widget owns observation timing only; the backend session owns attempts.
class ServiceRunManager extends StatefulWidget {
  const ServiceRunManager({
    super.key,
    required this.backend,
    required this.ioBackend,
    required this.metadataBackend,
    required this.plugins,
    required this.registryRevision,
    required this.ink,
    required this.muted,
    required this.line,
    required this.radius,
    this.onChanged,
    this.endpointBackend,
  });
  final WorkbenchServiceRunControl backend;
  final WorkbenchIoTaskControl ioBackend;
  final WorkbenchServiceControl metadataBackend;
  final WorkbenchEndpointControl? endpointBackend;
  final List<PluginLibraryEntry> plugins;
  final BigInt? registryRevision;
  final Color ink, muted, line;
  final BorderRadius radius;
  final VoidCallback? onChanged;
  @override
  State<ServiceRunManager> createState() => _ServiceRunManagerState();
}

class _ServiceRunManagerState extends State<ServiceRunManager>
    with SessionViewState<ServiceRunManager> {
  late ServiceRunSession _run;
  late ServiceSession _metadata;
  Timer? _timer;
  String? _selection;
  String _boundDirectory = '', _lastState = '';
  int _attachment = 0;
  bool _invalid = false, _refreshing = false;
  bool _refreshQueued = false, _waitingForMetadata = false;
  List<StoredEndpoint> _endpoints = const [];
  final Map<String, ServiceEndpointSelection> _outbound = {};
  bool _endpointsTrusted = false, _endpointsFailed = false;
  bool _tlsRequired = false;
  ServiceTlsSelection? _tlsSelection;
  final _fields = {
    'lifetime': TextEditingController(text: '60000'),
    'jobs': TextEditingController(text: '64'),
    'bytes': TextEditingController(text: '4194304'),
    'calls': TextEditingController(text: '4'),
    'job-bytes': TextEditingController(text: '1048576'),
    'total-bytes': TextEditingController(text: '4194304'),
    'request-bytes': TextEditingController(text: '65536'),
    'response-bytes': TextEditingController(text: '65536'),
    'header-bytes': TextEditingController(text: '16384'),
    'concurrent': TextEditingController(text: '1'),
    'timeout': TextEditingController(text: '10000'),
  };

  String _signature(ServiceRunManager w) =>
      '${w.registryRevision}|${w.plugins.map((p) => '${p.id}:${_hex(p.digest)}:${p.enabled}:${p.available}:${p.declaredIo.join(',')}:${p.approvedIo.join(',')}:${p.ioHandlers.join(',')}').join('|')}';
  String get _directory => _signature(widget);
  bool get _metadataCurrent =>
      !_refreshing &&
      !_metadata.busy &&
      _metadata.trusted &&
      !_metadata.uncertain &&
      widget.registryRevision != null &&
      _boundDirectory == _directory;

  bool _active(StoredServiceAuthority authority, BigInt now) =>
      !authority.disabled &&
      authority.createdMs <= now &&
      authority.expiresMs > now;

  List<_RunChoice> get _choices {
    if (!_metadataCurrent) return const [];
    final now = BigInt.from(DateTime.now().millisecondsSinceEpoch);
    final result = <_RunChoice>[];
    for (final config in _metadata.configs) {
      if (config.disabled) continue;
      final authsCurrent = config.principals.every(
        (principal) => _metadata.authorities.any(
          (authority) =>
              authority.kind == 1 &&
              authority.principalId == principal.id &&
              listEquals(
                authority.reference,
                principal.authenticationReference,
              ) &&
              _active(authority, now),
        ),
      );
      if (!authsCurrent) continue;
      for (final plugin in widget.plugins) {
        if (!plugin.available ||
            !plugin.enabled ||
            !listEquals(plugin.digest, config.packageDigest) ||
            !plugin.ioHandlers.contains(config.handler) ||
            !const ['http-listen', 'http-publish'].every(
              (capability) =>
                  plugin.declaredIo.contains(capability) &&
                  plugin.approvedIo.contains(capability),
            )) {
          continue;
        }
        for (final authority in _metadata.authorities) {
          final policy = authority.publication;
          if (authority.kind != 2 ||
              !_active(authority, now) ||
              policy == null ||
              policy.configId != config.id ||
              !listEquals(policy.configDigest, config.digest) ||
              !config.approvalReferences.any(
                (ref) => listEquals(ref, authority.reference),
              )) {
            continue;
          }
          // Shared validation requires numeric loopback for non-TLS policy.
          try {
            ServiceValidation.publication(policy);
          } on FormatException {
            continue;
          }
          result.add(
            _RunChoice(config, authority, plugin, widget.registryRevision!),
          );
        }
      }
    }
    return result;
  }

  List<StoredEndpoint> get _endpointChoices {
    if (!_endpointsTrusted) return const [];
    final choices = _choices.where((c) => c.key == _selection).toList();
    if (choices.length != 1) return const [];
    final plugin = choices.single.plugin;
    final now = BigInt.from(DateTime.now().millisecondsSinceEpoch);
    bool allowed(String capability) =>
        plugin.declaredIo.contains(capability) &&
        plugin.approvedIo.contains(capability);
    if (!allowed('http-request')) return const [];
    return _endpoints
        .where(
          (e) =>
              !e.disabled &&
              e.createdMs <= now &&
              e.expiresMs > now &&
              e.policy.packageId == plugin.id &&
              listEquals(e.policy.packageDigest, plugin.digest) &&
              (e.policy.credentialReference.isEmpty ||
                  allowed('credential-use')),
        )
        .toList();
  }

  bool get _outboundCurrent =>
      _outbound.isEmpty ||
      (!_refreshing &&
          _endpointsTrusted &&
          _outbound.values.every(
            (selection) => _endpointChoices.any(
              (e) =>
                  _hex(e.reference) == selection.key &&
                  e.revision == selection.revision,
            ),
          ));

  void _attach() {
    _attachment++;
    _run = ServiceRunSession.forBackend(widget.backend, widget.ioBackend);
    _metadata = ServiceSession.forBackend(widget.metadataBackend);
    _run.addListener(_changed);
    _metadata.addListener(_metadataChanged);
    _timer = Timer.periodic(const Duration(seconds: 1), (_) {
      if (_run.shouldPoll) unawaited(_run.refresh());
      if (_outbound.isNotEmpty && !_outboundCurrent) markSessionViewDirty();
    });
    unawaited(_refresh());
  }

  void _detach() {
    _attachment++;
    _refreshQueued = false;
    _waitingForMetadata = false;
    _timer?.cancel();
    _run.removeListener(_changed);
    _metadata.removeListener(_metadataChanged);
  }

  @override
  void initState() {
    super.initState();
    _attach();
  }

  @override
  void didUpdateWidget(covariant ServiceRunManager oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (!identical(oldWidget.backend, widget.backend) ||
        !identical(oldWidget.ioBackend, widget.ioBackend) ||
        !identical(oldWidget.metadataBackend, widget.metadataBackend) ||
        !identical(oldWidget.endpointBackend, widget.endpointBackend)) {
      _detach();
      _selection = null;
      _tlsSelection = null;
      _tlsRequired = false;
      _outbound.clear();
      _endpoints = const [];
      _endpointsTrusted = false;
      _endpointsFailed = false;
      _boundDirectory = '';
      _lastState = '';
      _refreshing = false;
      _attach();
    } else if (_signature(oldWidget) != _directory) {
      _boundDirectory = '';
      _scheduleRefresh();
    }
  }

  @override
  void dispose() {
    _detach();
    for (final field in _fields.values) {
      field.dispose();
    }
    super.dispose();
  }

  void _metadataChanged() {
    if (!mounted) return;
    if (_waitingForMetadata && !_metadata.busy) {
      _waitingForMetadata = false;
      // A successful concurrent read can unblock our own fresh observation.
      // A failed read stays blocked until the user explicitly refreshes.
      if (_metadata.trusted && !_metadata.uncertain) _scheduleRefresh();
    }
    markSessionViewDirty();
  }

  void _scheduleRefresh() {
    if (_refreshQueued || !mounted) return;
    _refreshQueued = true;
    final attachment = _attachment;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted || attachment != _attachment) return;
      _refreshQueued = false;
      // An in-flight refresh checks for a newer directory on completion.
      if (!_refreshing) unawaited(_refresh());
    });
  }

  void _changed() {
    if (!mounted) return;
    final state = '${_run.task?.storage}:${_hex(_run.task?.key ?? [])}';
    if (state != _lastState) {
      _lastState = state;
      final attachment = _attachment;
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (mounted && attachment == _attachment) widget.onChanged?.call();
      });
    }
    markSessionViewDirty();
  }

  Future<void> _refresh() async {
    if (_refreshing) return;
    final attachment = _attachment, directory = _directory;
    final run = _run, metadata = _metadata;
    final endpoints = widget.endpointBackend;
    _waitingForMetadata = false;
    setState(() {
      _refreshing = true;
      _endpointsTrusted = false;
      _endpointsFailed = false;
    });
    // Observe the owner first. Metadata uses the ordinary business route and
    // can wait behind a guest; observation must not depend on that completion.
    await run.refresh();
    if (!mounted || attachment != _attachment) return;
    if (metadata.busy) {
      _waitingForMetadata = true;
    } else {
      await metadata.refresh();
    }
    if (!mounted || attachment != _attachment) return;
    if (endpoints != null) {
      try {
        final loaded = await loadServiceEndpoints(endpoints);
        if (!mounted || attachment != _attachment) return;
        _endpoints = loaded;
        _endpointsTrusted = true;
      } catch (_) {
        if (!mounted || attachment != _attachment) return;
        _endpointsFailed = true;
      }
    }
    setState(() {
      _refreshing = false;
      if (!_waitingForMetadata && metadata.trusted && directory == _directory) {
        _boundDirectory = directory;
      }
    });
    if (directory != _directory) _scheduleRefresh();
  }

  Future<void> _start() async {
    if (!_run.canStart || !_metadataCurrent || !_outboundCurrent) return;
    final selected = _choices.where((c) => c.key == _selection).toList();
    if (selected.length != 1) return;
    final choice = selected.single;
    if (choice.publication.publication!.tlsRequired &&
        _tlsSelection?.validity?.validAt(DateTime.now()) != true) {
      return;
    }
    try {
      int number(String key) => int.parse(_fields[key]!.text.trim());
      BigInt big(String key) => BigInt.parse(_fields[key]!.text.trim());
      final random = Random.secure();
      final submission = Uint8List.fromList(
        List.generate(32, (_) => random.nextInt(256)),
      );
      if (submission.every((v) => v == 0)) submission[0] = 1;
      // Freeze the request in this synchronous event-loop turn, before any
      // await or backend call can deliver a new directory observation.
      final request = ServiceRunRequest(
        submission: submission,
        configId: choice.config.id,
        configDigest: choice.config.digest,
        configRevision: choice.config.revision,
        publication: choice.publication.reference,
        publicationRevision: choice.publication.revision,
        packageId: choice.plugin.id,
        packageDigest: choice.plugin.digest,
        registryRevision: choice.registry,
        lifetimeMs: number('lifetime'),
        maxJobs: big('jobs'),
        maxBytes: big('bytes'),
        maxCalls: number('calls'),
        maxJobBytes: big('job-bytes'),
        maxTotalBytes: big('total-bytes'),
        maxRequestBytes: number('request-bytes'),
        maxResponseBytes: number('response-bytes'),
        maxHeaderBytes: number('header-bytes'),
        maxConcurrent: number('concurrent'),
        timeoutMs: number('timeout'),
        outbound: _outbound.values.toList(),
        tls: choice.publication.publication!.tlsRequired ? _tlsSelection : null,
      );
      ServiceRunValidation.request(request);
      setState(() => _invalid = false);
      await _run.start(request);
    } on FormatException {
      if (mounted) setState(() => _invalid = true);
    }
  }

  Widget _note(String text, {String? key}) => Padding(
    padding: const EdgeInsets.symmetric(vertical: 5),
    child: Text(
      text,
      key: key == null ? null : ValueKey('service-run-$key'),
      style: TextStyle(color: widget.muted, fontSize: 11, height: 1.5),
    ),
  );
  Widget _button(String text, String key, VoidCallback? action) =>
      OutlinedButton(
        key: ValueKey('service-run-$key'),
        onPressed: action,
        style: OutlinedButton.styleFrom(
          foregroundColor: widget.ink,
          minimumSize: const Size(0, 38),
          side: BorderSide(color: widget.line),
          shape: RoundedRectangleBorder(borderRadius: widget.radius),
        ),
        child: Text(text, style: const TextStyle(fontSize: 11)),
      );
  Widget _field(String label, String key) => Padding(
    padding: const EdgeInsets.symmetric(vertical: 7),
    child: TextField(
      key: ValueKey('service-run-$key'),
      controller: _fields[key],
      enabled: !_run.busy && _run.task?.key == null && !_run.startUnknown,
      keyboardType: TextInputType.number,
      style: TextStyle(color: widget.ink, fontSize: 12),
      decoration: InputDecoration(labelText: label),
    ),
  );

  String _phase(AppLocalizations l, ServiceRunPhase phase) => switch (phase) {
    ServiceRunPhase.starting => l.pluginsServiceRunStarting,
    ServiceRunPhase.running => l.pluginsServiceRunRunning,
    ServiceRunPhase.stopping => l.pluginsServiceRunStopping,
    ServiceRunPhase.exited => l.pluginsServiceRunExited,
  };
  String _outcome(AppLocalizations l, ServiceNetworkOutcome outcome) =>
      switch (outcome) {
        ServiceNetworkOutcome.pending => l.pluginsServiceRunPending,
        ServiceNetworkOutcome.succeeded => l.pluginsServiceRunSucceeded,
        ServiceNetworkOutcome.invalid => l.pluginsServiceRunInvalidOutcome,
        ServiceNetworkOutcome.denied => l.pluginsServiceRunDenied,
        ServiceNetworkOutcome.limit => l.pluginsServiceRunLimit,
        ServiceNetworkOutcome.cancelled => l.pluginsServiceRunCancelled,
        ServiceNetworkOutcome.timeout => l.pluginsServiceRunTimeoutOutcome,
        ServiceNetworkOutcome.transport => l.pluginsServiceRunTransport,
        ServiceNetworkOutcome.closed => l.pluginsServiceRunClosed,
      };
  String _storage(AppLocalizations l, IoStoragePhase storage) =>
      switch (storage) {
        IoStoragePhase.local => l.pluginsServiceRunLocal,
        IoStoragePhase.running => l.pluginsServiceRunOwned,
        IoStoragePhase.stopping => l.pluginsServiceRunReclaiming,
        IoStoragePhase.reclaimed => l.pluginsServiceRunReclaimed,
        IoStoragePhase.recoveryRequired => l.pluginsServiceRunRecovery,
        IoStoragePhase.unavailable => l.pluginsServiceRunUnavailable,
      };
  String _notice(AppLocalizations l, ServiceRunNotice notice) =>
      switch (notice) {
        ServiceRunNotice.statusFailed => l.pluginsServiceRunStatusFailed,
        ServiceRunNotice.startUnknown => l.pluginsServiceRunStartUnknown,
        ServiceRunNotice.startRejected => l.pluginsServiceRunStartRejected,
        ServiceRunNotice.controlUnknown => l.pluginsServiceRunControlUnknown,
        ServiceRunNotice.identityChanged => l.pluginsServiceRunIdentityChanged,
        ServiceRunNotice.invalid => l.pluginsServiceRunInvalid,
      };

  String _job(AppLocalizations l, IoJobError error) => switch (error) {
    IoJobError.none => l.pluginsHttpTaskOk,
    IoJobError.invalidOptions => l.pluginsHttpTaskInvalidOptions,
    IoJobError.busy => l.pluginsHttpTaskBusy,
    IoJobError.closed => l.pluginsHttpTaskClosed,
    IoJobError.unavailable => l.pluginsHttpTaskUnavailable,
    IoJobError.consumed => l.pluginsHttpTaskConsumed,
    IoJobError.readBound => l.pluginsHttpTaskReadBound,
    IoJobError.limit => l.pluginsHttpTaskLimit,
    IoJobError.spawn => l.pluginsHttpTaskSpawn,
    IoJobError.disconnect => l.pluginsHttpTaskDisconnect,
  };

  @override
  Widget build(BuildContext context) {
    final l = L10n.of(context), choices = _choices;
    final selectionCurrent = choices.any((c) => c.key == _selection);
    final service = _run.service, task = _run.task;
    return Container(
      key: const ValueKey('service-run-manager'),
      width: double.infinity,
      padding: const EdgeInsets.all(12),
      decoration: BoxDecoration(
        border: Border.all(color: widget.line),
        borderRadius: widget.radius,
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            l.pluginsServiceRunTitle,
            style: TextStyle(
              color: widget.ink,
              fontSize: 13,
              fontWeight: FontWeight.w600,
            ),
          ),
          _note(l.pluginsServiceRunHint),
          if (_run.attempt?.tls case final ServiceTlsSelection selected) ...[
            _note(l.pluginsServiceTlsAttempt),
            _note(
              _hex(selected.certificateSha256),
              key: 'attempt-tls-fingerprint',
            ),
          ],
          if (_run.attempt?.outbound.isNotEmpty == true) ...[
            _note(l.pluginsServiceRunOutboundAttempt),
            for (final endpoint in _run.attempt!.outbound)
              _note(
                '${endpoint.key} · r${endpoint.revision}',
                key: 'attempt-endpoint-${endpoint.key}',
              ),
          ],
          Wrap(
            spacing: 8,
            runSpacing: 6,
            children: [
              _button(
                l.pluginsHttpTaskRefresh,
                'refresh',
                _run.busy ? null : _run.refresh,
              ),
              _button(
                l.pluginsServiceRefresh,
                'refresh-records',
                _refreshing || _metadata.busy ? null : _refresh,
              ),
            ],
          ),
          if (_run.notice != null)
            _note(_notice(l, _run.notice!), key: 'notice'),
          if (_run.startFailureDetail case final String detail)
            _note(
              l.pluginsServiceRunHostFailure(detail),
              key: 'failure-detail',
            ),
          if (_invalid) _note(l.pluginsServiceRunInvalid, key: 'invalid'),
          if (task != null) _note(_storage(l, task.storage), key: 'storage'),
          if (!_run.trusted && !_run.busy && task != null)
            _note(l.pluginsServiceRunLastObservation, key: 'unverified'),
          if (service != null && listEquals(service.task.key, task?.key)) ...[
            _note(_phase(l, service.phase), key: 'phase'),
            if (service.address != null)
              _note(service.address!, key: 'address'),
            _note(
              l.pluginsServiceRunNetwork(
                _outcome(l, service.bind),
                _outcome(l, service.listener),
                _outcome(l, service.supervision),
              ),
              key: 'network',
            ),
          ],
          if (task?.key != null) ...[
            _note(l.pluginsServiceRunTask, key: 'identity-label'),
            SelectableText(
              _hex(task!.key!),
              key: const ValueKey('service-run-identity'),
              style: TextStyle(color: widget.muted, fontSize: 11),
            ),
          ],
          if (task?.exit case final IoTaskExit exit)
            _note(
              l.pluginsHttpTaskExit(
                _job(l, exit.disconnect),
                _job(l, exit.execution),
                _job(l, exit.maintenance),
              ),
              key: 'exit',
            ),
          if (_run.attempt != null && _run.startUnknown) ...[
            _note(l.pluginsServiceRunAttempt),
            SelectableText(
              _hex(_run.attempt!.submission),
              key: const ValueKey('service-run-attempt'),
              style: TextStyle(color: widget.muted, fontSize: 11),
            ),
          ],
          Wrap(
            spacing: 8,
            runSpacing: 6,
            children: [
              _button(
                l.pluginsServiceRunStop,
                'stop',
                _run.canStop ? _run.stop : null,
              ),
              _button(
                l.pluginsHttpTaskRepair,
                'repair',
                _run.canRepair ? _run.repair : null,
              ),
              _button(
                l.pluginsHttpTaskAcknowledge,
                'acknowledge',
                _run.canAcknowledge ? _run.acknowledge : null,
              ),
              if (_run.canAbandon)
                _button(
                  l.pluginsServiceRunAbandon,
                  'abandon',
                  _run.abandonAttempt,
                ),
            ],
          ),
          const SizedBox(height: 10),
          _note(l.pluginsServiceRunNextSettings),
          DropdownButtonFormField<String>(
            key: ValueKey(
              'service-run-selection-${selectionCurrent ? _selection : ''}',
            ),
            initialValue: selectionCurrent ? _selection : null,
            isExpanded: true,
            decoration: InputDecoration(
              labelText: l.pluginsServiceRunSelection,
            ),
            style: TextStyle(color: widget.ink, fontSize: 12),
            items: choices
                .map(
                  (c) => DropdownMenuItem(
                    value: c.key,
                    child: Text(
                      '${c.plugin.name} · ${c.config.service} · ${c.publication.publication!.listenAddress}',
                      overflow: TextOverflow.ellipsis,
                    ),
                  ),
                )
                .toList(),
            onChanged: !_run.canStart
                ? null
                : (value) => setState(() {
                    if (value != _selection) {
                      _outbound.clear();
                      _tlsSelection = null;
                    }
                    _selection = value;
                    _tlsRequired = choices.any(
                      (c) =>
                          c.key == value &&
                          c.publication.publication!.tlsRequired,
                    );
                  }),
          ),
          if (choices.isEmpty) _note(l.pluginsServiceRunNoSelection),
          if (_tlsRequired)
            if (widget.backend case final WorkbenchServiceTlsControl tlsBackend)
              ServiceTlsPicker(
                key: ValueKey('service-tls-$_selection-$_attachment'),
                backend: tlsBackend,
                enabled: _run.canStart && selectionCurrent,
                onChanged: (value) => setState(() => _tlsSelection = value),
                ink: widget.ink,
                muted: widget.muted,
                line: widget.line,
                radius: widget.radius,
              )
            else
              _note(l.pluginsServiceTlsUnavailable),
          if (_selection != null && !selectionCurrent)
            _note(l.pluginsServiceRunStale, key: 'stale'),
          if (widget.endpointBackend != null) ...[
            _note(l.pluginsServiceRunOutboundHint),
            if (_endpointsFailed)
              _note(l.pluginsServiceRunOutboundFailed, key: 'outbound-failed'),
            if (!_outboundCurrent && _outbound.isNotEmpty)
              _note(l.pluginsServiceRunOutboundStale, key: 'outbound-stale'),
            for (final endpoint in _endpointChoices)
              CheckboxListTile(
                key: ValueKey(
                  'service-run-endpoint-${_hex(endpoint.reference)}',
                ),
                contentPadding: EdgeInsets.zero,
                dense: true,
                controlAffinity: ListTileControlAffinity.leading,
                activeColor: widget.ink,
                title: Text(
                  endpoint.policy.origin,
                  style: TextStyle(color: widget.ink, fontSize: 12),
                ),
                subtitle: Text(
                  '${endpoint.policy.methods.join(', ')} · ${_hex(endpoint.reference).substring(0, 12)} · r${endpoint.revision}',
                  style: TextStyle(color: widget.muted, fontSize: 11),
                ),
                value:
                    _outbound[_hex(endpoint.reference)]?.revision ==
                    endpoint.revision,
                onChanged:
                    !_run.canStart ||
                        _refreshing ||
                        (_outbound.length >= 8 &&
                            !_outbound.containsKey(_hex(endpoint.reference)))
                    ? null
                    : (checked) {
                        setState(() {
                          if (checked == true) {
                            _outbound[_hex(
                              endpoint.reference,
                            )] = ServiceEndpointSelection(
                              reference: endpoint.reference,
                              revision: endpoint.revision,
                            );
                          } else {
                            _outbound.remove(_hex(endpoint.reference));
                          }
                        });
                      },
              ),
            if (_outbound.isNotEmpty)
              _button(
                l.pluginsServiceRunOutboundClear,
                'outbound-clear',
                _run.canStart ? () => setState(_outbound.clear) : null,
              ),
          ],
          _field(l.pluginsServiceRunLifetime, 'lifetime'),
          _field(l.pluginsServiceRunJobs, 'jobs'),
          _field(l.pluginsServiceRunBytes, 'bytes'),
          PageStorage(
            bucket: PageStorageBucket(),
            child: ExpansionTile(
              key: const ValueKey('service-run-advanced'),
              tilePadding: EdgeInsets.zero,
              title: Text(
                l.pluginsServiceRunAdvanced,
                style: TextStyle(color: widget.ink, fontSize: 12),
              ),
              children: [
                _field(l.pluginsServiceRunCalls, 'calls'),
                _field(l.pluginsServiceRunJobBytes, 'job-bytes'),
                _field(l.pluginsServiceRunTotalBytes, 'total-bytes'),
                _field(l.pluginsServiceRunRequestBytes, 'request-bytes'),
                _field(l.pluginsServiceRunResponseBytes, 'response-bytes'),
                _field(l.pluginsServiceRunHeaderBytes, 'header-bytes'),
                _field(l.pluginsServiceRunConcurrent, 'concurrent'),
                _field(l.pluginsServiceRunTimeout, 'timeout'),
              ],
            ),
          ),
          _note(l.pluginsServiceRunBoundsHint),
          _button(
            l.pluginsServiceRunStart,
            'start',
            _run.canStart &&
                    selectionCurrent &&
                    _metadataCurrent &&
                    (!_tlsRequired || _tlsSelection != null) &&
                    _outboundCurrent
                ? _start
                : null,
          ),
        ],
      ),
    );
  }
}
