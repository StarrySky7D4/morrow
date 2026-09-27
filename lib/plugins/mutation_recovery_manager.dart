import 'dart:async';
import 'dart:math';

import 'package:flutter/material.dart';
import 'package:flutter/foundation.dart' show listEquals;
import 'package:flutter/services.dart';
import 'package:morrow_i18n/morrow_i18n.dart';

import 'io_task_models.dart';
import 'mutation_recovery_session.dart';
import 'mutation_recovery_view_state.dart';
import 'mutation_task_models.dart';
import 'mutation_workflow.dart';
import 'plugin_library.dart';
import 'session_view_state.dart';

String _hex(List<int> bytes) =>
    bytes.map((v) => v.toRadixString(16).padLeft(2, '0')).join();
String _packageKey(PluginLibraryEntry p) => '${p.id}:${_hex(p.digest)}';
String _capability(MutationDisposition d) => 'file-${d.name}';
Uint8List _token() {
  final random = Random.secure();
  Uint8List value;
  do {
    value = Uint8List.fromList(List.generate(32, (_) => random.nextInt(256)));
  } while (value.every((v) => v == 0));
  return value;
}

/// Form-only handoff from an explicitly selected application operation.
/// It cannot start discovery or grant permission to execute a mutation.
class MutationRecoverySeed {
  MutationRecoverySeed({
    required this.packageId,
    required Uint8List packageDigest,
    required this.subject,
    required this.disposition,
  }) : packageDigest = Uint8List.fromList(packageDigest).asUnmodifiableView();
  final String packageId, subject;
  final Uint8List packageDigest;
  final MutationDisposition disposition;
}

class _Draft {
  String? package;
  String subject = '', timeout = '10000';
  MutationDisposition disposition = MutationDisposition.create;
}

final _drafts = Expando<_Draft>('mutation recovery drafts');

/// Read-only recovery commands. Hiding this view stops its status polling,
/// never the task. Only the native host decides current authority and outcomes.
class MutationRecoveryManager extends StatefulWidget {
  const MutationRecoveryManager({
    super.key,
    required this.backend,
    required this.ioBackend,
    required this.plugins,
    required this.registryRevision,
    required this.ink,
    required this.muted,
    required this.line,
    required this.radius,
    this.onChanged,
    this.seed,
  });
  final MutationTaskBackend backend;
  final WorkbenchIoTaskControl ioBackend;
  final List<PluginLibraryEntry> plugins;
  final BigInt? registryRevision;
  final Color ink, muted, line;
  final BorderRadius radius;
  final VoidCallback? onChanged;
  final MutationRecoverySeed? seed;

  @override
  State<MutationRecoveryManager> createState() =>
      _MutationRecoveryManagerState();
}

class _MutationRecoveryManagerState extends State<MutationRecoveryManager>
    with SessionViewState<MutationRecoveryManager> {
  late MutationRecoverySession _session;
  late MutationRecoveryViewState _view;
  late _Draft _draft;
  final _subject = TextEditingController();
  final _timeout = TextEditingController();
  Timer? _timer;
  MutationRecoverySeed? _appliedSeed;
  bool _formError = false, _availabilityQueued = false;
  String? _availability;

  List<PluginLibraryEntry> _choices(MutationDisposition disposition) => [
    for (final p in widget.plugins)
      if (p.enabled &&
          p.available &&
          p.declaredIo.contains(_capability(disposition)) &&
          p.approvedIo.contains(_capability(disposition)))
        p,
  ];
  bool get _canEdit => _session.canStart && widget.registryRevision != null;
  PluginLibraryEntry? _scope(MutationDiscoverRequest? request) {
    if (request == null || widget.registryRevision == null) return null;
    for (final p in _choices(request.disposition)) {
      if (p.id == request.packageId &&
          _hex(p.digest) == _hex(request.packageDigest)) {
        return p;
      }
    }
    return null;
  }

  @override
  void initState() {
    super.initState();
    _attach();
  }

  void _applySeedWhenIdle() {
    final seed = widget.seed;
    if (seed == null || identical(seed, _appliedSeed) || !_session.canStart) {
      return;
    }
    _draft.package = '${seed.packageId}:${_hex(seed.packageDigest)}';
    _draft.subject = seed.subject;
    _draft.disposition = seed.disposition;
    _subject.text = seed.subject;
    _appliedSeed = seed;
  }

  void _attach() {
    _appliedSeed = null;
    _session = MutationRecoverySession.forBackend(
      widget.backend,
      widget.ioBackend,
    );
    _view = MutationRecoveryViewState.forSession(_session);
    _draft = _drafts[_session] ??= _Draft();
    _subject.text = _draft.subject;
    _timeout.text = _draft.timeout;
    _view.addListener(_changed);
    if (!_session.busy) unawaited(_session.refresh());
  }

  @override
  void didUpdateWidget(covariant MutationRecoveryManager oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (!identical(oldWidget.backend, widget.backend) ||
        !identical(oldWidget.ioBackend, widget.ioBackend)) {
      _view.removeListener(_changed);
      _timer?.cancel();
      _availability = null;
      _availabilityQueued = false;
      _formError = false;
      _attach();
    }
    _applySeedWhenIdle();
  }

  @override
  void dispose() {
    _timer?.cancel();
    _view.removeListener(_changed);
    _subject.dispose();
    _timeout.dispose();
    super.dispose();
  }

  @override
  void sessionViewVisibilityChanged() {
    _schedule();
    if (sessionViewActive && !_session.busy) unawaited(_session.refresh());
  }

  void _changed() {
    if (!mounted) return;
    _applySeedWhenIdle();
    markSessionViewDirty();
    _schedule();
    final availability =
        '${_session.snapshot?.storage}:${_session.snapshot?.key == null ? '' : _hex(_session.snapshot!.key!)}';
    if (availability != _availability && !_availabilityQueued) {
      _availabilityQueued = true;
      final session = _session;
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (!mounted || !identical(session, _session)) return;
        _availabilityQueued = false;
        _availability = availability;
        widget.onChanged?.call();
      });
    }
  }

  bool get _ownsCurrentTask {
    final request =
        _session.discoveryRequest?.submission ??
        _session.reconcileRequest?.submission;
    return request != null &&
        _session.snapshot?.key != null &&
        listEquals(request, _session.snapshot?.submission);
  }

  void _schedule() {
    _timer?.cancel();
    _timer = null;
    if (!sessionViewActive ||
        _session.busy ||
        !_ownsCurrentTask ||
        _session.snapshot?.exit != null ||
        (_session.discoveryRequest == null &&
            _session.reconcileRequest == null)) {
      return;
    }
    final session = _session;
    _timer = Timer(const Duration(seconds: 1), () {
      if (mounted && sessionViewActive && identical(session, _session)) {
        unawaited(session.refresh());
      }
    });
  }

  Future<void> _run(Future<void> Function() action) async {
    final session = _session;
    setState(() => _formError = false);
    try {
      await action();
    } catch (_) {
      if (mounted && identical(session, _session)) {
        setState(() => _formError = true);
      }
    }
  }

  Future<void> _discover({bool resume = false}) => _run(() async {
    if (!_canEdit) return;
    final old = resume ? _view.pageRequest : null;
    final checkpoint = resume ? _view.checkpoint : null;
    if (resume && (old == null || checkpoint == null || _scope(old) == null)) {
      throw StateError('Discovery scope unavailable');
    }
    final selected = resume
        ? _scope(old)!
        : _choices(
            _draft.disposition,
          ).singleWhere((p) => _packageKey(p) == _draft.package);
    final request = MutationDiscoverRequest(
      submission: _token(),
      packageId: selected.id,
      packageDigest: selected.digest,
      registryRevision: widget.registryRevision!,
      subject: old?.subject ?? _subject.text,
      disposition: old?.disposition ?? _draft.disposition,
      scanLimit: 8,
      timeoutMs: int.parse(_timeout.text),
      checkpoint: checkpoint,
    );
    MutationTaskValidation.validateDiscovery(request);
    await _session.startDiscovery(request);
  });
  Future<void> _reconcile() => _run(() async {
    final request = _view.selectedRequest, plan = _view.selectedPlan;
    final selected = _scope(request);
    if (!_canEdit || selected == null || plan == null) return;
    final start = MutationReconcileRequest(
      submission: _token(),
      packageId: selected.id,
      packageDigest: selected.digest,
      registryRevision: widget.registryRevision!,
      plan: plan,
      timeoutMs: int.parse(_timeout.text),
    );
    MutationTaskValidation.validateReconciliation(start);
    await _session.startReconciliation(start);
  });
  Widget _note(String text) => Padding(
    padding: const EdgeInsets.symmetric(vertical: 5),
    child: Text(text, style: TextStyle(color: widget.muted, fontSize: 12)),
  );
  Widget _button(String label, String id, Future<void> Function()? action) =>
      OutlinedButton(
        key: ValueKey('mutation-recovery-$id'),
        onPressed: action,
        style: OutlinedButton.styleFrom(
          foregroundColor: widget.ink,
          side: BorderSide(color: widget.line),
          shape: RoundedRectangleBorder(borderRadius: widget.radius),
        ),
        child: Text(label),
      );
  String _disposition(AppLocalizations l, MutationDisposition d) => switch (d) {
    MutationDisposition.create => l.pluginsIoFileCreate,
    MutationDisposition.replace => l.pluginsIoFileReplace,
    MutationDisposition.delete => l.pluginsIoFileDelete,
  };
  String _storage(AppLocalizations l, IoStoragePhase p) => switch (p) {
    IoStoragePhase.local => l.pluginsHttpTaskLocal,
    IoStoragePhase.running => l.pluginsHttpTaskRunning,
    IoStoragePhase.stopping => l.pluginsHttpTaskStopping,
    IoStoragePhase.reclaimed => l.pluginsHttpTaskReclaimed,
    IoStoragePhase.recoveryRequired => l.pluginsHttpTaskRecoveryRequired,
    IoStoragePhase.unavailable => l.pluginsHttpTaskUnavailable,
  };
  String _notice(AppLocalizations l, MutationRecoveryNotice n) => switch (n) {
    MutationRecoveryNotice.status => l.pluginsHttpTaskStatusFailed,
    MutationRecoveryNotice.startUnknown => l.pluginsHttpTaskStartUnknown,
    MutationRecoveryNotice.submissionUnknown =>
      l.pluginsMutationSubmissionUnknown,
    MutationRecoveryNotice.readUnknown => l.pluginsHttpTaskReadUnknown,
    MutationRecoveryNotice.resultLost => l.pluginsMutationResultLost,
    MutationRecoveryNotice.controlUnknown => l.pluginsHttpTaskControlUnknown,
    MutationRecoveryNotice.identity => l.pluginsFileTaskIdentity,
    MutationRecoveryNotice.interrupted => l.pluginsFileTaskInterrupted,
  };
  String _phase(AppLocalizations l, MutationPhase p) => switch (p) {
    MutationPhase.none => l.pluginsMutationAbsent,
    MutationPhase.prepared => l.pluginsMutationPrepared,
    MutationPhase.outcomeUnknown => l.pluginsMutationUnknown,
    MutationPhase.observed => l.pluginsMutationObserved,
    MutationPhase.cancelledBeforeDispatch => l.pluginsMutationCancelled,
  };
  String _effect(AppLocalizations l, MutationEffect e) => switch (e) {
    MutationEffect.unspecified => l.pluginsMutationUnspecified,
    MutationEffect.osSucceeded => l.pluginsMutationSucceeded,
    MutationEffect.osRejected => l.pluginsMutationRejected,
  };
  @override
  Widget build(BuildContext context) {
    final l = L10n.of(context), choices = _choices(_draft.disposition);
    final selected = choices.any((p) => _packageKey(p) == _draft.package)
        ? _draft.package
        : null;
    final page = _view.page, plans = page?.plans ?? const <Uint8List>[];
    final result = _view.lastReconciliation;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          l.pluginsMutationTitle,
          style: TextStyle(color: widget.ink, fontWeight: FontWeight.w600),
        ),
        _note(l.pluginsMutationIntro),
        if (widget.registryRevision == null)
          _note(l.pluginsHttpTaskCatalogUnavailable),
        DropdownButton<MutationDisposition>(
          key: const ValueKey('mutation-recovery-disposition'),
          isExpanded: true,
          value: _draft.disposition,
          items: [
            for (final d in MutationDisposition.values)
              DropdownMenuItem(value: d, child: Text(_disposition(l, d))),
          ],
          onChanged: _canEdit
              ? (d) => setState(() {
                  if (d != null) _draft.disposition = d;
                  _draft.package = null;
                })
              : null,
        ),
        DropdownButton<String>(
          key: const ValueKey('mutation-recovery-package'),
          isExpanded: true,
          value: selected,
          hint: Text(l.pluginsEndpointPackage),
          items: [
            for (final p in choices)
              DropdownMenuItem(
                value: _packageKey(p),
                child: Text(p.name, overflow: TextOverflow.ellipsis),
              ),
          ],
          onChanged: _canEdit
              ? (v) => setState(() => _draft.package = v)
              : null,
        ),
        Wrap(
          spacing: 8,
          runSpacing: 8,
          children: [
            _button(
              l.pluginsFileMutationTitle,
              'native-scope',
              _canEdit
                  ? () async {
                      setState(() {
                        _draft.subject = mutationExecutionSubject;
                        _subject.text = _draft.subject;
                      });
                    }
                  : null,
            ),
            _button(
              l.pluginsGuestExecutionTitle,
              'guest-scope',
              _canEdit
                  ? () async {
                      setState(() {
                        _draft.subject = guestMutationExecutionSubject;
                        _subject.text = _draft.subject;
                      });
                    }
                  : null,
            ),
          ],
        ),
        TextField(
          key: const ValueKey('mutation-recovery-subject'),
          controller: _subject,
          enabled: _canEdit,
          maxLength: 256,
          onChanged: (v) => _draft.subject = v,
          decoration: InputDecoration(labelText: l.pluginsMutationSubject),
        ),
        const SizedBox(height: 8),
        TextField(
          key: const ValueKey('mutation-recovery-timeout'),
          controller: _timeout,
          enabled: _canEdit,
          keyboardType: TextInputType.number,
          inputFormatters: [
            FilteringTextInputFormatter.digitsOnly,
            LengthLimitingTextInputFormatter(5),
          ],
          onChanged: (v) => _draft.timeout = v,
          decoration: InputDecoration(labelText: l.pluginsFileTaskTimeout),
        ),
        const SizedBox(height: 12),
        Wrap(
          spacing: 8,
          runSpacing: 8,
          children: [
            _button(
              l.pluginsMutationDiscover,
              'discover',
              _canEdit && selected != null ? () => _discover() : null,
            ),
            _button(
              l.pluginsHttpTaskRefresh,
              'refresh',
              !_session.busy ? () => _run(_session.refresh) : null,
            ),
            _button(
              l.pluginsHttpTaskRead,
              'read',
              _session.canRead ? () => _run(_session.read) : null,
            ),
            _button(
              l.pluginsMutationNext,
              'next',
              _session.canNext
                  ? () => _run(() => _session.nextPage(_token(), 8))
                  : null,
            ),
            _button(
              l.pluginsMutationRetry,
              'retry',
              _session.canRetrySubmission
                  ? () => _run(_session.retrySubmission)
                  : null,
            ),
            _button(
              l.pluginsMutationRelease,
              'release',
              _session.canRelease
                  ? () => _run(() => _session.release(_token()))
                  : null,
            ),
            _button(
              l.pluginsHttpTaskCancel,
              'stop',
              _session.canStop ? () => _run(_session.stop) : null,
            ),
            _button(
              l.pluginsHttpTaskRepair,
              'repair',
              _session.canRepair ? () => _run(_session.repair) : null,
            ),
            _button(
              l.pluginsHttpTaskAcknowledge,
              'ack',
              _session.canAcknowledge ? () => _run(_session.acknowledge) : null,
            ),
            _button(
              l.pluginsHttpTaskAbandon,
              'abandon',
              _session.canAbandon ? () => _run(_session.abandon) : null,
            ),
          ],
        ),
        if (_session.busy) const LinearProgressIndicator(),
        if (_formError) _note(l.pluginsMutationFormError),
        if (_session.snapshot != null)
          _note(_storage(l, _session.snapshot!.storage)),
        if (_session.state != null)
          _note(switch (_session.state!.delivery) {
            MutationDelivery.pending => l.pluginsHttpTaskPending,
            MutationDelivery.ready => l.pluginsHttpTaskReady,
            MutationDelivery.consumed => l.pluginsHttpTaskConsumed,
          }),
        if (_session.snapshot?.key != null && !_ownsCurrentTask)
          _note(l.pluginsFileTaskOtherSession),
        if (_session.notice != null) _note(_notice(l, _session.notice!)),
        if (page != null) ...[
          const SizedBox(height: 12),
          Text(
            '${l.pluginsMutationPage} · ${plans.length}',
            style: TextStyle(color: widget.ink),
          ),
          if (_view.pageRequest != null)
            _note(
              '${_view.pageRequest!.packageId} · ${_view.pageRequest!.subject} · ${_disposition(l, _view.pageRequest!.disposition)}',
            ),
          if (plans.isEmpty) _note(l.pluginsMutationEmpty),
          if (page.done) _note(l.pluginsMutationDone),
          for (var i = 0; i < plans.length; i++)
            ListTile(
              key: ValueKey('mutation-recovery-plan-$i'),
              contentPadding: EdgeInsets.zero,
              title: Text('${l.pluginsMutationPlan} ${i + 1}'),
              subtitle: Text(l.pluginsMutationSelectPlan),
              trailing: Icon(
                identical(_view.selectedRequest, _view.pageRequest) &&
                        listEquals(_view.selectedPlan, plans[i])
                    ? Icons.check_circle_outline
                    : Icons.radio_button_unchecked,
              ),
              onTap: () => _view.selectPlan(i),
            ),
        ],
        if (_view.checkpoint != null) ...[
          _note(l.pluginsMutationCheckpoint),
          _button(
            l.pluginsMutationContinue,
            'continue',
            _canEdit && _scope(_view.pageRequest) != null
                ? () => _discover(resume: true)
                : null,
          ),
        ],
        if (_view.selectedPlan != null) ...[
          _note(l.pluginsMutationSelected),
          if (_view.selectedRequest != null)
            _note(
              '${_view.selectedRequest!.packageId} · ${_view.selectedRequest!.subject} · ${_disposition(l, _view.selectedRequest!.disposition)}',
            ),
          if (!_session.canStart) _note(l.pluginsMutationCleanup),
          if (_scope(_view.selectedRequest) == null)
            _note(l.pluginsMutationScopeUnavailable),
          _button(
            l.pluginsMutationReconcile,
            'reconcile',
            _canEdit && _scope(_view.selectedRequest) != null
                ? _reconcile
                : null,
          ),
        ],
        if (result != null) ...[
          const SizedBox(height: 12),
          Text(
            l.pluginsMutationLastResult,
            style: TextStyle(color: widget.ink),
          ),
          if (result.operationId != null)
            SelectableText(
              result.operationId!,
              style: TextStyle(color: widget.ink),
            ),
          _note(_phase(l, result.phase)),
          _note(_effect(l, result.effect)),
          if (result.osCode != null) _note('OS: ${result.osCode}'),
        ],
      ],
    );
  }
}
