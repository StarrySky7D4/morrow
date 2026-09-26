import 'dart:async';
import 'dart:math';
import 'dart:typed_data';
import 'package:file_selector/file_selector.dart';
import 'package:flutter/material.dart';
import 'package:morrow_i18n/morrow_i18n.dart';
import 'file_task_models.dart';
import 'file_task_session.dart';
import 'io_task_models.dart';
import 'plugin_library.dart';
import 'session_view_state.dart';

String _hex(List<int> bytes) =>
    bytes.map((b) => b.toRadixString(16).padLeft(2, '0')).join();

class _Choice {
  _Choice(this.plugin, this.handler);
  final PluginLibraryEntry plugin;
  final String handler;
  String get key => '${plugin.id}:${_hex(plugin.digest)}:$handler';
}

/// Native picker and bounded verification view. The backend session, not the
/// widget, owns delivered metadata and uncertain attempts. No export or writes.
class FileTaskManager extends StatefulWidget {
  const FileTaskManager({
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
    this.pickFile,
  });
  final FileTaskBackend backend;
  final WorkbenchIoTaskControl ioBackend;
  final List<PluginLibraryEntry> plugins;
  final BigInt? registryRevision;
  final Color ink, muted, line;
  final BorderRadius radius;
  final VoidCallback? onChanged;
  final Future<XFile?> Function()? pickFile;
  @override
  State<FileTaskManager> createState() => _FileTaskManagerState();
}

class _FileTaskManagerState extends State<FileTaskManager>
    with SessionViewState<FileTaskManager> {
  late FileTaskSession _session;
  final _ceiling = TextEditingController(text: '262144');
  final _timeout = TextEditingController(text: '10000');
  String? _choice, _path, _name, _availability;
  String? _formError;
  bool _picking = false, _availabilityQueued = false;
  int _epoch = 0;
  Timer? _timer;
  List<_Choice> get _choices => [
    for (final p in widget.plugins)
      if (p.enabled &&
          p.available &&
          p.declaredIo.contains('file-read') &&
          p.approvedIo.contains('file-read'))
        for (final h in p.ioHandlers) _Choice(p, h),
  ];
  String _catalog(FileTaskManager w) =>
      '${w.registryRevision}|${w.plugins.map((p) => '${p.id}:${_hex(p.digest)}:${p.enabled}:${p.available}:${p.declaredIo.join(',')}:${p.approvedIo.join(',')}:${p.ioHandlers.join(',')}').join('|')}';
  bool get _canEdit =>
      !_picking && _session.canStart && widget.registryRevision != null;
  @override
  void initState() {
    super.initState();
    _attach();
  }

  void _attach() {
    _session = FileTaskSession.forBackend(widget.backend, widget.ioBackend);
    _session.addListener(_changed);
    if (!_session.busy) {
      unawaited(_session.refresh());
    }
  }

  @override
  void didUpdateWidget(covariant FileTaskManager old) {
    super.didUpdateWidget(old);
    final changed =
        !identical(old.backend, widget.backend) ||
        !identical(old.ioBackend, widget.ioBackend);
    if (changed) {
      _session.removeListener(_changed);
      _timer?.cancel();
      _epoch++;
      _availabilityQueued = false;
      _availability = null;
      _path = null;
      _name = null;
      _picking = false;
      _attach();
    }
    if (changed || _catalog(old) != _catalog(widget)) {
      _epoch++;
      _picking = false;
      _choice = null;
      _formError = null;
    }
  }

  @override
  void dispose() {
    _epoch++;
    _timer?.cancel();
    _session.removeListener(_changed);
    _ceiling.dispose();
    _timeout.dispose();
    super.dispose();
  }

  @override
  void sessionViewVisibilityChanged() {
    _schedule();
    if (sessionViewActive && !_session.busy) {
      unawaited(_session.refresh());
    }
  }

  void _changed() {
    if (!mounted) {
      return;
    }
    markSessionViewDirty();
    _schedule();
    final availability =
        '${_session.snapshot?.storage}:${_session.snapshot?.key == null ? '' : _hex(_session.snapshot!.key!)}';
    if (availability != _availability && !_availabilityQueued) {
      _availabilityQueued = true;
      final session = _session;
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (!mounted || !identical(_session, session)) {
          return;
        }
        _availabilityQueued = false;
        _availability = availability;
        widget.onChanged?.call();
      });
    }
  }

  void _schedule() {
    _timer?.cancel();
    _timer = null;
    if (!sessionViewActive ||
        _session.busy ||
        !_session.ownsTask ||
        _session.snapshot?.exit != null) {
      return;
    }
    final session = _session;
    _timer = Timer(const Duration(seconds: 1), () {
      if (mounted && sessionViewActive && identical(session, _session)) {
        unawaited(session.refresh());
      }
    });
  }

  Future<void> _pick() async {
    if (!_canEdit) {
      return;
    }
    final epoch = ++_epoch, backend = widget.backend;
    setState(() {
      _picking = true;
      _formError = null;
    });
    try {
      final selected = await (widget.pickFile?.call() ?? openFile());
      if (!mounted || epoch != _epoch || !identical(widget.backend, backend)) {
        return;
      }
      if (selected != null) {
        setState(() {
          _path = selected.path;
          _name = selected.name;
        });
      }
    } catch (_) {
      if (mounted && epoch == _epoch) setState(() => _formError = 'selection');
    } finally {
      if (mounted && epoch == _epoch) setState(() => _picking = false);
    }
  }

  Future<void> _start() async {
    if (!_canEdit || _path == null) {
      return;
    }
    final session = _session, epoch = _epoch;
    try {
      final selected = _choices.singleWhere((v) => v.key == _choice);
      final rng = Random.secure();
      final request = FileTaskRequest(
        submission: Uint8List.fromList(
          List.generate(32, (_) => rng.nextInt(256)),
        ),
        packageId: selected.plugin.id,
        packageDigest: selected.plugin.digest,
        registryRevision: widget.registryRevision!,
        handler: selected.handler,
        selectedPath: _path!,
        maxBytes: BigInt.parse(_ceiling.text),
        timeoutMs: int.parse(_timeout.text),
      );
      FileTaskValidation.validateRequest(request);
      setState(() => _formError = null);
      await session.start(request);
      if (session.canVerify) await session.verify();
    } catch (_) {
      if (mounted && epoch == _epoch) setState(() => _formError = 'options');
    }
  }

  Widget _note(String text) => Padding(
    padding: const EdgeInsets.symmetric(vertical: 5),
    child: Text(text, style: TextStyle(color: widget.muted, fontSize: 12)),
  );
  Widget _button(String text, String id, VoidCallback? action) =>
      OutlinedButton(
        key: ValueKey(id),
        onPressed: action,
        style: OutlinedButton.styleFrom(
          foregroundColor: widget.ink,
          side: BorderSide(color: widget.line),
          shape: RoundedRectangleBorder(borderRadius: widget.radius),
        ),
        child: Text(text),
      );
  String _phase(AppLocalizations l, [FileTaskPhase? value]) =>
      switch (value ?? _session.phase) {
        FileTaskPhase.idle => l.pluginsFileTaskIdle,
        FileTaskPhase.capturing => l.pluginsFileTaskCapturing,
        FileTaskPhase.reading => l.pluginsFileTaskReading,
        FileTaskPhase.finishing => l.pluginsFileTaskFinishing,
        FileTaskPhase.verified => l.pluginsFileTaskVerified,
        FileTaskPhase.cancelled => l.pluginsFileTaskCancelled,
        FileTaskPhase.failed => l.pluginsFileTaskFailed,
        FileTaskPhase.unknown => l.pluginsFileTaskUnknown,
      };
  String _storage(AppLocalizations l, IoStoragePhase phase) => switch (phase) {
    IoStoragePhase.local => l.pluginsHttpTaskLocal,
    IoStoragePhase.running => l.pluginsHttpTaskRunning,
    IoStoragePhase.stopping => l.pluginsHttpTaskStopping,
    IoStoragePhase.reclaimed => l.pluginsHttpTaskReclaimed,
    IoStoragePhase.recoveryRequired => l.pluginsHttpTaskRecoveryRequired,
    IoStoragePhase.unavailable => l.pluginsHttpTaskUnavailable,
  };
  String _notice(AppLocalizations l, FileTaskNotice notice) => switch (notice) {
    FileTaskNotice.status => l.pluginsHttpTaskStatusFailed,
    FileTaskNotice.startUnknown => l.pluginsHttpTaskStartUnknown,
    FileTaskNotice.readUnknown => l.pluginsHttpTaskReadUnknown,
    FileTaskNotice.controlUnknown => l.pluginsHttpTaskControlUnknown,
    FileTaskNotice.commandUnknown => l.pluginsFileTaskCommandUnknown,
    FileTaskNotice.integrity => l.pluginsFileTaskIntegrity,
    FileTaskNotice.identity => l.pluginsFileTaskIdentity,
    FileTaskNotice.interrupted => l.pluginsFileTaskInterrupted,
  };
  @override
  Widget build(BuildContext context) {
    final l = L10n.of(context), choices = _choices, state = _session.snapshot;
    final selected = choices.any((c) => c.key == _choice) ? _choice : null;
    final preview = _session.preview;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          l.pluginsFileTaskTitle,
          style: TextStyle(color: widget.ink, fontWeight: FontWeight.w600),
        ),
        _note(l.pluginsFileTaskIntro),
        if (widget.registryRevision == null)
          _note(l.pluginsHttpTaskCatalogUnavailable),
        DropdownButton<String>(
          key: const ValueKey('file-task-handler'),
          isExpanded: true,
          value: selected,
          hint: Text(l.pluginsFileTaskSelectHandler),
          items: [
            for (final c in choices)
              DropdownMenuItem(
                value: c.key,
                child: Text(
                  '${c.plugin.name} · ${c.handler}',
                  overflow: TextOverflow.ellipsis,
                ),
              ),
          ],
          onChanged: _canEdit ? (v) => setState(() => _choice = v) : null,
        ),
        _button(
          l.pluginsFileTaskSelectFile,
          'file-task-pick',
          _canEdit ? _pick : null,
        ),
        if (_name != null || _session.attempt != null)
          _note(
            l.pluginsSelectedFile(
              _name ??
                  _session.attempt!.selectedPath.split(RegExp(r'[/\\]')).last,
            ),
          ),
        TextField(
          key: const ValueKey('file-task-ceiling'),
          controller: _ceiling,
          enabled: _canEdit,
          keyboardType: TextInputType.number,
          decoration: InputDecoration(labelText: l.pluginsFileTaskCeiling),
        ),
        TextField(
          key: const ValueKey('file-task-timeout'),
          controller: _timeout,
          enabled: _canEdit,
          keyboardType: TextInputType.number,
          decoration: InputDecoration(labelText: l.pluginsFileTaskTimeout),
        ),
        _note(l.pluginsFileTaskBudgets),
        if (_formError != null) _note(l.pluginsFileTaskFormError),
        Wrap(
          spacing: 8,
          runSpacing: 6,
          children: [
            _button(
              l.pluginsFileTaskStart,
              'file-task-start',
              _canEdit && selected != null && _path != null ? _start : null,
            ),
            _button(
              l.pluginsHttpTaskRefresh,
              'file-task-refresh',
              !_session.busy ? _session.refresh : null,
            ),
            _button(
              l.pluginsFileTaskVerify,
              'file-task-verify',
              _session.canVerify ? _session.verify : null,
            ),
            _button(
              l.pluginsHttpTaskCancel,
              'file-task-cancel',
              _session.canCancel ? () => _session.cancel() : null,
            ),
            _button(
              l.pluginsHttpTaskRepair,
              'file-task-repair',
              _session.canRepair ? _session.repair : null,
            ),
            _button(
              l.pluginsHttpTaskAcknowledge,
              'file-task-ack',
              _session.canAcknowledge ? _session.acknowledge : null,
            ),
            _button(
              l.pluginsHttpTaskAbandon,
              'file-task-abandon',
              _session.canAbandon ? _session.abandon : null,
            ),
          ],
        ),
        if (_session.busy || _picking) const LinearProgressIndicator(),
        _note(_phase(l)),
        if (state != null) _note(_storage(l, state.storage)),
        if (state?.key != null && !_session.ownsTask)
          _note(l.pluginsFileTaskOtherSession),
        if (_session.notice != null) _note(_notice(l, _session.notice!)),
        if (_session.length != null)
          _note(
            l.pluginsFileTaskProgress(
              _session.received.toString(),
              _session.length.toString(),
            ),
          ),
        if (_session.digest != null)
          SelectableText(
            'SHA-256: ${_session.digest}',
            style: TextStyle(color: widget.ink, fontSize: 11),
          ),
        if (preview.isNotEmpty)
          _note(
            '${l.pluginsFileTaskHexPreview}: ${_hex(preview.take(64).toList())}',
          ),
        for (final h in _session.history)
          _note(
            '${l.pluginsFileTaskPrevious}: ${_phase(l, h.phase)} · ${l.pluginsFileTaskProgress(h.received.toString(), h.length?.toString() ?? '?')} · ${h.digest ?? '—'}',
          ),
      ],
    );
  }
}
