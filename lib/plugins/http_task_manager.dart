import 'dart:async';
import 'dart:convert';
import 'dart:math';
import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:morrow_i18n/morrow_i18n.dart';

import 'endpoint_control.dart';
import 'io_task_control.dart';
import 'plugin_library.dart';

String _hex(List<int> value) =>
    value.map((v) => v.toRadixString(16).padLeft(2, '0')).join();
bool _equal(List<int>? a, List<int>? b) => a == null || b == null
    ? a == null && b == null
    : a.length == b.length &&
          List.generate(a.length, (i) => a[i] == b[i]).every((v) => v);
int _compare(List<int> a, List<int> b) {
  for (var i = 0; i < a.length && i < b.length; i++) {
    if (a[i] != b[i]) return a[i].compareTo(b[i]);
  }
  return a.length.compareTo(b.length);
}

enum _Notice {
  statusFailed,
  endpointsFailed,
  invalid,
  startUnknown,
  readUnknown,
  controlUnknown,
  readPending,
}

class _History {
  const _History(
    this.snapshot,
    this.result,
    this.readUnknown, {
    this.attempt,
    this.startUnknown = false,
  });
  final IoTaskSnapshot snapshot;
  final IoTaskResult? result;
  final bool readUnknown;
  final Uint8List? attempt;
  final bool startUnknown;
}

/// A consumed result and an uncertain attempt belong to the backend session,
/// not to a collapsible settings widget. This controller never owns a timer.
class _HttpSession extends ChangeNotifier {
  _HttpSession(this.backend);
  final WorkbenchIoTaskControl backend;
  IoTaskSnapshot? snapshot;
  IoTaskResult? result;
  Uint8List? attempt;
  bool busy = false, trusted = false, startUnknown = false, readUnknown = false;
  bool silentObservation = false;
  _Notice? notice;
  final List<_History> history = [];
  int _operation = 0, _availability = 0, _notifiedAvailability = 0;
  bool takeAvailabilityChange() {
    if (_availability == _notifiedAvailability) return false;
    _notifiedAvailability = _availability;
    return true;
  }

  bool get canStart =>
      trusted &&
      !busy &&
      !startUnknown &&
      snapshot?.key == null &&
      snapshot?.storage == IoStoragePhase.local;
  bool get shouldPoll =>
      !busy &&
      trusted &&
      !startUnknown &&
      snapshot?.key != null &&
      (snapshot!.storage == IoStoragePhase.running ||
          snapshot!.storage == IoStoragePhase.stopping ||
          snapshot!.delivery == IoDeliveryPhase.pending);
  bool get canAbandon =>
      trusted &&
      !busy &&
      startUnknown &&
      snapshot?.key == null &&
      snapshot?.storage == IoStoragePhase.local;
  void abandonAttempt() {
    if (!canAbandon) return;
    history.insert(
      0,
      _History(
        snapshot!,
        null,
        false,
        attempt: attempt == null ? null : Uint8List.fromList(attempt!),
        startUnknown: true,
      ),
    );
    if (history.length > 5) history.removeLast();
    attempt = null;
    startUnknown = false;
    notice = null;
    notifyListeners();
  }

  static int _access(IoStoragePhase phase) => switch (phase) {
    IoStoragePhase.local || IoStoragePhase.reclaimed => 0,
    IoStoragePhase.running || IoStoragePhase.stopping => 1,
    IoStoragePhase.recoveryRequired => 2,
    IoStoragePhase.unavailable => 3,
  };
  void _accept(IoTaskSnapshot value, {bool started = false}) {
    if (value.key != null) HttpTaskValidation.identity(value.key!);
    if (value.submission != null) {
      HttpTaskValidation.identity(value.submission!);
    }
    final old = snapshot;
    if (old?.key != null && !_equal(old!.key, value.key)) {
      history.insert(0, _History(old, result, readUnknown));
      if (history.length > 5) history.removeLast();
      result = null;
      readUnknown = false;
      if (!startUnknown) attempt = null;
    }
    if (started ||
        (old == null
            ? _access(value.storage) != 0
            : _access(old.storage) != _access(value.storage))) {
      _availability++;
    }
    snapshot = value;
    trusted = true;
    if (startUnknown &&
        value.key != null &&
        _equal(value.submission, attempt)) {
      startUnknown = false;
    }
    if (!startUnknown && value.submission != null) {
      attempt = Uint8List.fromList(value.submission!);
    }
  }

  Future<void> refresh({bool poll = false, bool silent = false}) async {
    if (busy) return;
    final key = poll ? snapshot?.key : null;
    final operation = ++_operation;
    busy = true;
    silentObservation = silent;
    notifyListeners();
    try {
      final value = key == null
          ? await backend.ioStatus()
          : await backend.pollIo(Uint8List.fromList(key));
      if (operation != _operation) return;
      if (key != null && !_equal(value.key, key)) {
        throw const FormatException('Task changed');
      }
      _accept(value);
      if (notice == _Notice.statusFailed || notice == _Notice.controlUnknown) {
        notice = null;
      }
    } catch (_) {
      if (operation == _operation) {
        trusted = false;
        notice = _Notice.statusFailed;
      }
    } finally {
      if (operation == _operation) {
        busy = false;
        silentObservation = false;
        notifyListeners();
      }
    }
  }

  Future<void> start(HttpTaskRequest request) async {
    if (!canStart) return;
    final operation = ++_operation;
    busy = true;
    attempt = Uint8List.fromList(request.submission);
    result = null;
    readUnknown = false;
    notice = null;
    notifyListeners();
    try {
      final value = await backend.startHttp(request);
      if (operation != _operation) return;
      if (value.key == null || !_equal(value.submission, request.submission)) {
        throw const FormatException('Submission changed');
      }
      _accept(value, started: true);
    } catch (_) {
      if (operation == _operation) {
        startUnknown = true;
        trusted = false;
        notice = _Notice.startUnknown;
      }
    } finally {
      if (operation == _operation) {
        busy = false;
        notifyListeners();
      }
    }
  }

  Future<void> read() async {
    final key = snapshot?.key;
    if (busy ||
        key == null ||
        readUnknown ||
        result != null ||
        snapshot?.delivery != IoDeliveryPhase.ready) {
      return;
    }
    final operation = ++_operation;
    busy = true;
    notice = null;
    notifyListeners();
    try {
      final value = await backend.readIo(Uint8List.fromList(key));
      if (operation != _operation) return;
      if (!_equal(value.snapshot.key, key)) {
        throw const FormatException('Task changed');
      }
      _accept(value.snapshot);
      result = value.result;
      if (result == null) notice = _Notice.readPending;
    } catch (_) {
      if (operation == _operation) {
        readUnknown = true;
        notice = _Notice.readUnknown;
      }
    } finally {
      if (operation == _operation) {
        busy = false;
        notifyListeners();
      }
    }
  }

  Future<void> action(String action) async {
    final key = snapshot?.key;
    if (busy || key == null) return;
    final operation = ++_operation;
    busy = true;
    notice = null;
    notifyListeners();
    try {
      final value = await switch (action) {
        'cancel' => backend.cancelIo(Uint8List.fromList(key)),
        'repair' => backend.repairIo(Uint8List.fromList(key)),
        'ack' => backend.acknowledgeIo(Uint8List.fromList(key)),
        _ => throw const FormatException('Unknown action'),
      };
      if (operation != _operation) return;
      if (action == 'ack') {
        if (value.key != null || value.storage != IoStoragePhase.local) {
          throw const FormatException('Incomplete acknowledgement');
        }
      } else if (!_equal(value.key, key)) {
        throw const FormatException('Task changed');
      }
      _accept(value);
      if (action == 'ack') {
        result = null;
        attempt = null;
        readUnknown = false;
        startUnknown = false;
      }
    } catch (_) {
      if (operation == _operation) {
        trusted = false;
        notice = _Notice.controlUnknown;
      }
    } finally {
      if (operation == _operation) {
        busy = false;
        notifyListeners();
      }
    }
  }
}

final _sessions = Expando<_HttpSession>('HTTP task backend sessions');

class HttpTaskManager extends StatefulWidget {
  const HttpTaskManager({
    super.key,
    required this.backend,
    required this.endpointBackend,
    required this.plugins,
    required this.registryRevision,
    required this.ink,
    required this.muted,
    required this.line,
    required this.radius,
    this.onChanged,
  });
  final WorkbenchIoTaskControl backend;
  final WorkbenchEndpointControl endpointBackend;
  final List<PluginLibraryEntry> plugins;
  final BigInt? registryRevision;
  final Color ink, muted, line;
  final BorderRadius radius;
  final VoidCallback? onChanged;
  @override
  State<HttpTaskManager> createState() => _HttpTaskManagerState();
}

class _HttpTaskManagerState extends State<HttpTaskManager> {
  late _HttpSession _session;
  late String _directory;
  final _target = TextEditingController(text: '/');
  final _headers = TextEditingController();
  final _body = TextEditingController();
  final _timeout = TextEditingController(text: '10000');
  List<StoredEndpoint> _endpoints = [];
  String? _endpoint;
  String _method = 'GET', _bodyFormat = 'text';
  _Notice? _formNotice;
  bool _loadingEndpoints = false, _endpointsTrusted = false;
  bool _refreshOnIdle = false;
  bool _availabilityQueued = false;
  int _endpointEpoch = 0, _attachmentEpoch = 0;
  Timer? _timer;
  String _fingerprint() =>
      '${widget.registryRevision}|${widget.plugins.map((p) => '${p.id}:${_hex(p.digest)}:${p.enabled}:${p.available}:${p.declaredIo.join(',')}:${p.approvedIo.join(',')}:${p.ioHandlers.join(',')}').join('|')}';
  void _attach() {
    _session = _sessions[widget.backend] ??= _HttpSession(widget.backend);
    _session.addListener(_changed);
    _refreshOnIdle = _session.busy;
    if (!_session.busy) {
      unawaited(_session.refresh());
    }
    _schedule();
    _notifyAvailabilityLater();
  }

  void _notifyAvailabilityLater() {
    if (_availabilityQueued || widget.onChanged == null) return;
    _availabilityQueued = true;
    final epoch = _attachmentEpoch, session = _session;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted ||
          epoch != _attachmentEpoch ||
          !identical(session, _session)) {
        return;
      }
      _availabilityQueued = false;
      // Only consume this backend's pending notification after the attachment
      // is still current and the parent's build has finished.
      if (widget.onChanged != null && session.takeAvailabilityChange()) {
        widget.onChanged!.call();
      }
    });
  }

  void _changed() {
    if (!mounted) return;
    setState(() {});
    _schedule();
    if (_refreshOnIdle && !_session.busy) {
      _refreshOnIdle = false;
      unawaited(_session.refresh());
    }
    _notifyAvailabilityLater();
  }

  void _schedule() {
    _timer?.cancel();
    _timer = null;
    if (!_session.shouldPoll) return;
    final session = _session, epoch = _attachmentEpoch;
    _timer = Timer(const Duration(seconds: 1), () {
      if (mounted &&
          epoch == _attachmentEpoch &&
          identical(session, _session)) {
        unawaited(session.refresh(poll: true, silent: true));
      }
    });
  }

  @override
  void initState() {
    super.initState();
    _directory = _fingerprint();
    _attach();
    unawaited(_loadEndpoints());
  }

  @override
  void didUpdateWidget(covariant HttpTaskManager oldWidget) {
    super.didUpdateWidget(oldWidget);
    final changedBackend = !identical(oldWidget.backend, widget.backend);
    final directory = _fingerprint();
    if (changedBackend) {
      _attachmentEpoch++;
      _availabilityQueued = false;
      _session.removeListener(_changed);
      _timer?.cancel();
      _attach();
      _target.text = '/';
      _headers.clear();
      _body.clear();
      _timeout.text = '10000';
      _bodyFormat = 'text';
    }
    if (changedBackend ||
        !identical(oldWidget.endpointBackend, widget.endpointBackend) ||
        directory != _directory) {
      _directory = directory;
      _endpointEpoch++;
      _loadingEndpoints = false;
      _endpointsTrusted = false;
      _endpoints = [];
      _endpoint = null;
      _formNotice = null;
      unawaited(_loadEndpoints());
    }
  }

  @override
  void dispose() {
    _attachmentEpoch++;
    _endpointEpoch++;
    _timer?.cancel();
    _session.removeListener(_changed);
    _target.dispose();
    _headers.dispose();
    _body.dispose();
    _timeout.dispose();
    super.dispose();
  }

  bool _packageAllowed(StoredEndpoint e) => widget.plugins.any(
    (p) =>
        p.available &&
        p.enabled &&
        p.id == e.policy.packageId &&
        _equal(p.digest, e.policy.packageDigest) &&
        p.ioHandlers.contains('morrow.http.forward.v1') &&
        p.declaredIo.contains('http-request') &&
        p.approvedIo.contains('http-request') &&
        (e.policy.credentialReference.isEmpty ||
            (p.declaredIo.contains('credential-use') &&
                p.approvedIo.contains('credential-use'))),
  );
  List<StoredEndpoint> get _choices => _endpoints
      .where(
        (e) =>
            !e.disabled &&
            e.expiresMs > BigInt.from(DateTime.now().millisecondsSinceEpoch) &&
            const [1, 2, 3].contains(e.policy.profile) &&
            e.policy.timeoutMs >= 1 &&
            e.policy.timeoutMs <= 30000 &&
            e.policy.maxRequestBytes >= 1 &&
            e.policy.maxRequestBytes <= 65536 &&
            e.policy.maxResponseBytes >= 1 &&
            e.policy.maxResponseBytes <= 65536 &&
            e.policy.maxHeaderBytes >= 1 &&
            e.policy.maxHeaderBytes <= 16384 &&
            e.policy.maxConcurrent >= 1 &&
            e.policy.maxConcurrent <= 128 &&
            e.policy.maxFrameBytes >= 1 &&
            e.policy.maxFrameBytes <= 131072 &&
            e.policy.methods.every(
              (m) => const [
                'GET',
                'HEAD',
                'POST',
                'PUT',
                'PATCH',
                'DELETE',
                'OPTIONS',
              ].contains(m),
            ) &&
            _packageAllowed(e),
      )
      .toList();
  StoredEndpoint? get _selected {
    for (final e in _choices) {
      if (_hex(e.reference) == _endpoint) return e;
    }
    return null;
  }

  bool _currentEndpoints(WorkbenchEndpointControl backend, int epoch) =>
      mounted &&
      epoch == _endpointEpoch &&
      identical(widget.endpointBackend, backend);
  static StoredEndpoint _checkedEndpoint(StoredEndpoint e) {
    HttpTaskValidation.identity(e.reference);
    HttpTaskValidation.identity(e.policy.packageDigest);
    if (e.revision <= BigInt.zero ||
        e.revision >= (BigInt.one << 64) ||
        e.createdMs <= BigInt.zero ||
        e.expiresMs <= e.createdMs ||
        e.expiresMs > BigInt.from(253402300799999) ||
        e.policy.packageId.isEmpty ||
        e.policy.methods.isEmpty ||
        e.policy.methods.length > 16 ||
        e.policy.methods.toSet().length != e.policy.methods.length ||
        e.policy.methods.any((m) => !RegExp(r'^[A-Z]{1,16}$').hasMatch(m))) {
      throw const FormatException('Invalid endpoint metadata');
    }
    if (e.policy.credentialReference.isNotEmpty) {
      HttpTaskValidation.identity(e.policy.credentialReference);
    }
    final p = e.policy;
    return StoredEndpoint(
      reference: Uint8List.fromList(e.reference),
      revision: e.revision,
      createdMs: e.createdMs,
      expiresMs: e.expiresMs,
      disabled: e.disabled,
      policy: EndpointPolicy(
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
      ),
    );
  }

  Future<void> _loadEndpoints() async {
    if (_loadingEndpoints) return;
    final backend = widget.endpointBackend, epoch = ++_endpointEpoch;
    setState(() {
      _loadingEndpoints = true;
      _endpointsTrusted = false;
      _formNotice = null;
    });
    try {
      final entries = <StoredEndpoint>[];
      Uint8List? after, snapshot;
      for (var pageIndex = 0; pageIndex < 512; pageIndex++) {
        final page = await backend.endpointPage(
          after: after == null ? null : Uint8List.fromList(after),
          snapshot: snapshot == null ? null : Uint8List.fromList(snapshot),
        );
        if (!_currentEndpoints(backend, epoch)) return;
        if (page.snapshot.length != 32 ||
            (snapshot != null && !_equal(snapshot, page.snapshot)) ||
            page.entries.length > 2 ||
            entries.length + page.entries.length > 512) {
          throw const FormatException('Inconsistent endpoint page');
        }
        snapshot ??= Uint8List.fromList(page.snapshot);
        for (final raw in page.entries) {
          final e = _checkedEndpoint(raw);
          if ((after != null && _compare(e.reference, after) <= 0) ||
              (entries.isNotEmpty &&
                  _compare(e.reference, entries.last.reference) <= 0)) {
            throw const FormatException('Invalid endpoint ordering');
          }
          entries.add(e);
        }
        final next = page.next;
        if (next == null) {
          setState(() {
            _endpoints = entries;
            _endpointsTrusted = true;
            final selected = _selected;
            if (selected == null) {
              _endpoint = _choices.isEmpty
                  ? null
                  : _hex(_choices.first.reference);
            }
            final current = _selected;
            if (current != null && !current.policy.methods.contains(_method)) {
              _method = current.policy.methods.first;
            }
          });
          return;
        }
        HttpTaskValidation.identity(next);
        if ((after != null && _compare(next, after) <= 0) ||
            (entries.isNotEmpty &&
                _compare(next, entries.last.reference) < 0)) {
          throw const FormatException('Invalid endpoint cursor');
        }
        after = Uint8List.fromList(next);
      }
      throw const FormatException('Endpoint page bound');
    } catch (_) {
      if (_currentEndpoints(backend, epoch)) {
        setState(() {
          _endpoints = [];
          _endpoint = null;
          _formNotice = _Notice.endpointsFailed;
        });
      }
    } finally {
      if (_currentEndpoints(backend, epoch)) {
        setState(() => _loadingEndpoints = false);
      }
    }
  }

  void _select(String? reference) {
    setState(() {
      _endpoint = reference;
      final e = _selected;
      if (e != null) {
        if (!e.policy.methods.contains(_method)) {
          _method = e.policy.methods.first;
        }
        _timeout.text = '${min(10000, e.policy.timeoutMs)}';
      }
    });
  }

  void _start() {
    if (!_session.canStart ||
        !_endpointsTrusted ||
        widget.registryRevision == null) {
      return;
    }
    try {
      final endpoint = _selected;
      if (endpoint == null || !endpoint.policy.methods.contains(_method)) {
        throw const FormatException();
      }
      final headers = <HttpTaskHeader>[];
      for (final line in _headers.text.split(RegExp(r'\r?\n'))) {
        if (line.trim().isEmpty) continue;
        final colon = line.indexOf(':');
        if (colon < 1) throw const FormatException();
        var value = line.substring(colon + 1);
        if (value.startsWith(' ')) value = value.substring(1);
        headers.add(
          HttpTaskHeader(
            name: line.substring(0, colon).trim(),
            value: Uint8List.fromList(utf8.encode(value)),
          ),
        );
      }
      final bytes = _bodyFormat == 'base64'
          ? base64Decode(_body.text.trim())
          : Uint8List.fromList(utf8.encode(_body.text));
      final timeout = int.tryParse(_timeout.text);
      if (timeout == null ||
          timeout > endpoint.policy.timeoutMs ||
          bytes.length > endpoint.policy.maxRequestBytes ||
          headers.fold<int>(
                0,
                (sum, h) => sum + h.name.length + h.value.length,
              ) >
              endpoint.policy.maxHeaderBytes) {
        throw const FormatException();
      }
      final random = Random.secure();
      final token = Uint8List.fromList(
        List.generate(32, (_) => random.nextInt(256)),
      );
      final request = HttpTaskRequest(
        submission: token,
        endpoint: endpoint.reference,
        endpointRevision: endpoint.revision,
        packageDigest: endpoint.policy.packageDigest,
        registryRevision: widget.registryRevision!,
        method: _method,
        target: _target.text,
        headers: headers,
        body: bytes,
        timeoutMs: timeout,
      );
      HttpTaskValidation.validateRequest(request);
      setState(() => _formNotice = null);
      unawaited(_session.start(request));
    } catch (_) {
      setState(() => _formNotice = _Notice.invalid);
    }
  }

  String _notice(AppLocalizations l, _Notice notice) => switch (notice) {
    _Notice.statusFailed => l.pluginsHttpTaskStatusFailed,
    _Notice.endpointsFailed => l.pluginsHttpTaskEndpointsFailed,
    _Notice.invalid => l.pluginsHttpTaskInvalid,
    _Notice.startUnknown => l.pluginsHttpTaskStartUnknown,
    _Notice.readUnknown => l.pluginsHttpTaskReadUnknown,
    _Notice.controlUnknown => l.pluginsHttpTaskControlUnknown,
    _Notice.readPending => l.pluginsHttpTaskReadPending,
  };
  String _storage(AppLocalizations l, IoStoragePhase storage) =>
      switch (storage) {
        IoStoragePhase.local => l.pluginsHttpTaskLocal,
        IoStoragePhase.running => l.pluginsHttpTaskRunning,
        IoStoragePhase.stopping => l.pluginsHttpTaskStopping,
        IoStoragePhase.reclaimed => l.pluginsHttpTaskReclaimed,
        IoStoragePhase.recoveryRequired => l.pluginsHttpTaskRecoveryRequired,
        IoStoragePhase.unavailable => l.pluginsHttpTaskUnavailable,
      };
  String _delivery(AppLocalizations l, IoDeliveryPhase delivery) =>
      switch (delivery) {
        IoDeliveryPhase.absent => l.pluginsHttpTaskAbsent,
        IoDeliveryPhase.pending => l.pluginsHttpTaskPending,
        IoDeliveryPhase.ready => l.pluginsHttpTaskReady,
        IoDeliveryPhase.consumed => l.pluginsHttpTaskConsumed,
        IoDeliveryPhase.unavailable => l.pluginsHttpTaskResultUnavailable,
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
  String _fault(AppLocalizations l, IoExecutionFault fault) => switch (fault) {
    IoExecutionFault.none => l.pluginsHttpTaskOk,
    IoExecutionFault.taskProtocol => l.pluginsHttpTaskProtocol,
    IoExecutionFault.deadline => l.pluginsHttpTaskDeadline,
    IoExecutionFault.packageBinding => l.pluginsHttpTaskPackageChanged,
    IoExecutionFault.inactiveConnection => l.pluginsHttpTaskInactive,
    IoExecutionFault.invalidModule => l.pluginsHttpTaskModule,
    IoExecutionFault.unsupportedAbi => l.pluginsHttpTaskUnsupported,
    IoExecutionFault.limits => l.pluginsHttpTaskLimit,
    IoExecutionFault.cancelled => l.pluginsHttpTaskCancelled,
    IoExecutionFault.trap => l.pluginsHttpTaskTrap,
  };
  String _outcome(AppLocalizations l, IoHttpStatus status) => switch (status) {
    IoHttpStatus.invalid => l.pluginsHttpTaskProtocol,
    IoHttpStatus.accepted => l.pluginsHttpTaskAccepted,
    IoHttpStatus.pending => l.pluginsHttpTaskPending,
    IoHttpStatus.completed => l.pluginsHttpTaskCompleted,
    IoHttpStatus.denied => l.pluginsHttpTaskDenied,
    IoHttpStatus.revoked => l.pluginsHttpTaskRevoked,
    IoHttpStatus.expired => l.pluginsCredentialExpired,
    IoHttpStatus.unsupported => l.pluginsHttpTaskUnsupported,
    IoHttpStatus.quota => l.pluginsHttpTaskLimit,
    IoHttpStatus.notFound => l.pluginsHttpTaskNotFound,
    IoHttpStatus.conflict => l.pluginsHttpTaskConflict,
    IoHttpStatus.cancelled => l.pluginsHttpTaskCancelled,
    IoHttpStatus.outcomeUnknown => l.pluginsHttpTaskOutcomeUnknown,
    IoHttpStatus.evidenceUnavailable => l.pluginsHttpTaskEvidenceUnavailable,
    IoHttpStatus.failed => l.pluginsHttpTaskFailed,
  };
  Widget _note(String value, {Key? key}) => Padding(
    padding: const EdgeInsets.symmetric(vertical: 5),
    child: Text(
      value,
      key: key,
      style: TextStyle(color: widget.muted, fontSize: 11, height: 1.6),
    ),
  );
  Widget _button(String label, String key, VoidCallback? action) =>
      OutlinedButton(
        key: ValueKey(key),
        onPressed: action,
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
    int lines = 1,
    bool numeric = false,
  }) => Padding(
    padding: const EdgeInsets.symmetric(vertical: 6),
    child: TextField(
      key: ValueKey(key),
      controller: controller,
      enabled: _session.canStart,
      minLines: lines,
      maxLines: lines == 1 ? 1 : 6,
      keyboardType: numeric
          ? TextInputType.number
          : lines == 1
          ? TextInputType.text
          : TextInputType.multiline,
      decoration: InputDecoration(labelText: label),
      style: TextStyle(color: widget.ink, fontSize: 12),
    ),
  );
  String? _safeText(List<int> value) {
    try {
      final text = utf8.decode(value);
      if (text.codeUnits.any(
        (v) => (v < 32 && v != 9 && v != 10 && v != 13) || v == 127,
      )) {
        return null;
      }
      return text;
    } catch (_) {
      return null;
    }
  }

  Widget _result(
    AppLocalizations l,
    IoTaskResult result, {
    String prefix = 'http-task-result',
  }) {
    final http = result.http;
    return Column(
      key: ValueKey(prefix),
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        _note(
          l.pluginsHttpTaskExecution(
            result.exitCode,
            _fault(l, result.executionFault),
          ),
        ),
        _note(
          l.pluginsHttpTaskCounters(
            result.chargedBytes.toString(),
            result.calls.toString(),
          ),
        ),
        if (result.cancelled) _note(l.pluginsHttpTaskCancelled),
        if (result.unknown) _note(l.pluginsHttpTaskOutcomeUnknown),
        if (http != null) ...[
          _note(
            l.pluginsHttpTaskHttpResult(
              http.httpStatus,
              _outcome(l, http.status),
            ),
          ),
          if (http.status == IoHttpStatus.completed && http.httpStatus >= 400)
            _note(l.pluginsHttpTaskRemoteError),
          _note(l.pluginsHttpTaskResponseHeaders),
          _note(
            http.headers
                .map(
                  (h) =>
                      '${h.name}: ${_safeText(h.value) ?? '${l.pluginsHttpTaskBase64}: ${base64Encode(h.value)}'}',
                )
                .join('\n'),
            key: ValueKey('$prefix-headers'),
          ),
          _note(l.pluginsHttpTaskResponseBase64),
          _note(base64Encode(http.body), key: ValueKey('$prefix-body')),
          if (_safeText(http.body) case final String text) ...[
            _note(l.pluginsHttpTaskResponseText),
            _note(text, key: ValueKey('$prefix-text')),
          ],
        ],
      ],
    );
  }

  Widget _exit(AppLocalizations l, IoTaskExit exit) => _note(
    l.pluginsHttpTaskExit(
      _job(l, exit.disconnect),
      _job(l, exit.execution),
      _job(l, exit.maintenance),
    ),
  );
  @override
  Widget build(BuildContext context) {
    final l = L10n.of(context), state = _session.snapshot, selected = _selected;
    final key = state?.key, active = key != null && !_session.busy;
    final methods = selected?.policy.methods ?? const <String>[];
    final canEdit = _session.canStart;
    return Padding(
      padding: const EdgeInsets.all(19),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            l.pluginsHttpTaskTitle,
            style: TextStyle(
              color: widget.ink,
              fontSize: 13,
              fontWeight: FontWeight.w600,
            ),
          ),
          _note(l.pluginsHttpTaskDetails),
          if (state != null)
            _note(
              '${_storage(l, state.storage)}\n${_delivery(l, state.delivery)}',
              key: const ValueKey('http-task-status'),
            ),
          if (_session.busy && !_session.silentObservation)
            _note(l.pluginsHttpTaskWorking),
          if (_session.attempt != null)
            _note(
              l.pluginsHttpTaskSubmission(_hex(_session.attempt!)),
              key: const ValueKey('http-task-submission'),
            ),
          if (key != null) _note(l.pluginsHttpTaskKey(_hex(key))),
          if (_session.startUnknown)
            _note(l.pluginsHttpTaskStartUnknown)
          else if (_session.readUnknown)
            _note(l.pluginsHttpTaskReadUnknown),
          if (_session.notice != null &&
              _session.notice != _Notice.startUnknown &&
              _session.notice != _Notice.readUnknown)
            _note(_notice(l, _session.notice!)),
          Wrap(
            spacing: 8,
            runSpacing: 8,
            children: [
              _button(
                l.pluginsHttpTaskRefresh,
                'http-task-status-refresh',
                _session.busy ? null : () => unawaited(_session.refresh()),
              ),
              _button(
                l.pluginsHttpTaskPoll,
                'http-task-poll',
                active ? () => unawaited(_session.refresh(poll: true)) : null,
              ),
              _button(
                l.pluginsHttpTaskRead,
                'http-task-read',
                active &&
                        state?.delivery == IoDeliveryPhase.ready &&
                        !_session.readUnknown &&
                        _session.result == null
                    ? () => unawaited(_session.read())
                    : null,
              ),
              _button(
                l.pluginsHttpTaskCancel,
                'http-task-cancel',
                active &&
                        (state?.storage == IoStoragePhase.running ||
                            state?.storage == IoStoragePhase.stopping)
                    ? () => unawaited(_session.action('cancel'))
                    : null,
              ),
              _button(
                l.pluginsHttpTaskRepair,
                'http-task-repair',
                active && state?.storage == IoStoragePhase.recoveryRequired
                    ? () => unawaited(_session.action('repair'))
                    : null,
              ),
              _button(
                l.pluginsHttpTaskAcknowledge,
                'http-task-ack',
                active &&
                        state?.storage == IoStoragePhase.reclaimed &&
                        state?.exit != null
                    ? () => unawaited(_session.action('ack'))
                    : null,
              ),
            ],
          ),
          if (_session.startUnknown) ...[
            _note(l.pluginsHttpTaskAbandonDetails),
            _button(
              l.pluginsHttpTaskAbandon,
              'http-task-abandon-attempt',
              _session.canAbandon ? _session.abandonAttempt : null,
            ),
          ],
          if (state?.exit != null) _exit(l, state!.exit!),
          if (_session.result != null) _result(l, _session.result!),
          if (_session.history.isNotEmpty) _note(l.pluginsHttpTaskHistory),
          for (var i = 0; i < _session.history.length; i++) ...[
            if ((_session.history[i].attempt ??
                    _session.history[i].snapshot.submission)
                case final Uint8List identity)
              _note(
                l.pluginsHttpTaskSubmission(_hex(identity)),
                key: ValueKey('http-task-history-$i-submission'),
              ),
            _note(_storage(l, _session.history[i].snapshot.storage)),
            if (_session.history[i].startUnknown)
              _note(l.pluginsHttpTaskArchivedUnknown),
            if (_session.history[i].snapshot.exit != null)
              _exit(l, _session.history[i].snapshot.exit!),
            if (_session.history[i].readUnknown)
              _note(l.pluginsHttpTaskReadUnknown),
            if (_session.history[i].result != null)
              _result(
                l,
                _session.history[i].result!,
                prefix: 'http-task-history-$i',
              ),
          ],
          const SizedBox(height: 14),
          _note(l.pluginsHttpTaskNew),
          _button(
            l.pluginsHttpTaskRefreshEndpoints,
            'http-task-endpoints-refresh',
            _loadingEndpoints ? null : () => unawaited(_loadEndpoints()),
          ),
          if (_loadingEndpoints) _note(l.pluginsHttpTaskLoadingEndpoints),
          if (_formNotice != null) _note(_notice(l, _formNotice!)),
          if (widget.registryRevision == null)
            _note(l.pluginsHttpTaskCatalogUnavailable),
          if (_endpointsTrusted && _choices.isEmpty)
            _note(l.pluginsHttpTaskNoEndpoints),
          DropdownButtonFormField<String>(
            key: const ValueKey('http-task-endpoint'),
            initialValue: selected == null ? null : _hex(selected.reference),
            isExpanded: true,
            decoration: InputDecoration(labelText: l.pluginsHttpTaskEndpoint),
            items: _choices
                .map(
                  (e) => DropdownMenuItem(
                    value: _hex(e.reference),
                    child: Text(
                      '${e.policy.origin} (${e.policy.packageId})',
                      overflow: TextOverflow.ellipsis,
                    ),
                  ),
                )
                .toList(),
            onChanged: canEdit && _endpointsTrusted ? _select : null,
          ),
          DropdownButtonFormField<String>(
            key: const ValueKey('http-task-method'),
            initialValue: methods.contains(_method) ? _method : null,
            isExpanded: true,
            decoration: InputDecoration(labelText: l.pluginsEndpointMethods),
            items: methods
                .map((m) => DropdownMenuItem(value: m, child: Text(m)))
                .toList(),
            onChanged: canEdit ? (v) => setState(() => _method = v!) : null,
          ),
          _field(l.pluginsHttpTaskTarget, 'http-task-target', _target),
          _field(
            l.pluginsHttpTaskHeaders,
            'http-task-headers',
            _headers,
            lines: 2,
          ),
          _note(l.pluginsHttpTaskHeadersHint),
          DropdownButtonFormField<String>(
            key: const ValueKey('http-task-body-format'),
            initialValue: _bodyFormat,
            isExpanded: true,
            decoration: InputDecoration(labelText: l.pluginsHttpTaskBodyFormat),
            items: [
              DropdownMenuItem(
                value: 'text',
                child: Text(l.pluginsHttpTaskText),
              ),
              DropdownMenuItem(
                value: 'base64',
                child: Text(l.pluginsHttpTaskBase64),
              ),
            ],
            onChanged: canEdit ? (v) => setState(() => _bodyFormat = v!) : null,
          ),
          _field(l.pluginsHttpTaskBody, 'http-task-body', _body, lines: 2),
          _field(
            l.pluginsHttpTaskTimeout,
            'http-task-timeout',
            _timeout,
            numeric: true,
          ),
          _note(l.pluginsHttpTaskExplicit),
          _button(
            l.pluginsHttpTaskStart,
            'http-task-start',
            canEdit &&
                    _endpointsTrusted &&
                    selected != null &&
                    widget.registryRevision != null
                ? _start
                : null,
          ),
        ],
      ),
    );
  }
}
