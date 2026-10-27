import 'dart:async';
import 'dart:math';
import 'dart:typed_data';
import 'package:file_selector/file_selector.dart';
import 'package:flutter/material.dart';
import 'package:morrow_i18n/morrow_i18n.dart';
import 'directory_task_models.dart';
import 'directory_task_session.dart';
import 'io_task_models.dart';
import 'plugin_library.dart';
import 'session_view_state.dart';

String _hex(List<int> bytes) =>
    bytes.map((v) => v.toRadixString(16).padLeft(2, '0')).join();

class _Choice {
  _Choice(this.plugin, this.handler);
  final PluginLibraryEntry plugin;
  final String handler;
  String get key => '${plugin.id}:${_hex(plugin.digest)}:$handler';
}

/// A view lease over the retained original session. Native picker admission is
/// revoked when its view changes; the original owner and uncertain submissions
/// remain retained until their own checked retirement completes.
class DirectoryTaskManager extends StatefulWidget {
  const DirectoryTaskManager({
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
    this.pickDirectory,
  });
  final DirectoryTaskBackend backend;
  final WorkbenchIoTaskControl ioBackend;
  final List<PluginLibraryEntry> plugins;
  final BigInt? registryRevision;
  final Color ink, muted, line;
  final BorderRadius radius;
  final VoidCallback? onChanged;
  final Future<String?> Function()? pickDirectory;
  @override
  State<DirectoryTaskManager> createState() => _DirectoryTaskManagerState();
}

class _DirectoryTaskManagerState extends State<DirectoryTaskManager>
    with SessionViewState<DirectoryTaskManager> {
  late DirectoryTaskSession _session;
  final _entries = TextEditingController(text: '1024');
  final _metadata = TextEditingController(text: '1048576');
  final _timeout = TextEditingController(text: '10000');
  String? _choice, _path, _availability;
  bool _picking = false, _formError = false, _availabilityQueued = false;
  int _epoch = 0;
  Timer? _timer;
  bool get _nativePicker =>
      widget.backend is NativeDirectoryPickerCapabilities &&
      (widget.backend as NativeDirectoryPickerCapabilities)
          .supportsNativeDirectoryPicker;
  List<_Choice> get _choices => [
    for (final p in widget.plugins)
      if (p.enabled &&
          p.available &&
          p.directorySupported &&
          p.declaredIo.contains('file-list') &&
          p.approvedIo.contains('file-list'))
        for (final h in p.ioHandlers) _Choice(p, h),
  ];
  bool get _canEdit =>
      !_picking && _session.canStart && widget.registryRevision != null;
  String _catalog(DirectoryTaskManager w) =>
      '${w.registryRevision}|${w.plugins.map((p) => '${p.id}:${_hex(p.digest)}:${p.enabled}:${p.available}:${p.directorySupported}:${p.declaredIo.join(',')}:${p.approvedIo.join(',')}:${p.ioHandlers.join(',')}').join('|')}';
  @override
  void initState() {
    super.initState();
    _attach();
  }

  void _attach() {
    _session = DirectoryTaskSession.forBackend(
      widget.backend,
      widget.ioBackend,
    );
    _session.addListener(_changed);
    if (!_session.busy) unawaited(_session.refresh());
  }

  @override
  void didUpdateWidget(covariant DirectoryTaskManager old) {
    super.didUpdateWidget(old);
    final changed =
        !identical(old.backend, widget.backend) ||
        !identical(old.ioBackend, widget.ioBackend);
    if (changed || _catalog(old) != _catalog(widget)) {
      // An unconfirmed catalog also withdraws permission to continue a live
      // picker; an unchanged confirmed catalog or locale does not.
      unawaited(_session.revokeNativePickerView());
    }
    if (changed) {
      _session.removeListener(_changed);
      _timer?.cancel();
      _availability = null;
      _availabilityQueued = false;
      _attach();
    }
    if (changed || _catalog(old) != _catalog(widget)) {
      ++_epoch;
      _picking = false;
      _choice = null;
      _path = null;
      _formError = false;
    }
  }

  @override
  void dispose() {
    ++_epoch;
    _timer?.cancel();
    _session.removeListener(_changed);
    unawaited(_session.revokeNativePickerView());
    _entries.dispose();
    _metadata.dispose();
    _timeout.dispose();
    super.dispose();
  }

  @override
  void sessionViewVisibilityChanged() {
    if (!sessionViewActive) unawaited(_session.revokeNativePickerView());
    _schedule();
    if (sessionViewActive && !_session.busy) unawaited(_session.refresh());
  }

  void _changed() {
    if (!mounted) return;
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
    if (!_canEdit || _nativePicker) return;
    final epoch = ++_epoch;
    final backend = widget.backend;
    setState(() {
      _picking = true;
      _formError = false;
    });
    try {
      final path = await (widget.pickDirectory?.call() ?? getDirectoryPath());
      if (!mounted || epoch != _epoch || !identical(backend, widget.backend)) {
        return;
      }
      if (path != null) setState(() => _path = path);
    } catch (_) {
      if (mounted && epoch == _epoch) setState(() => _formError = true);
    } finally {
      if (mounted && epoch == _epoch) setState(() => _picking = false);
    }
  }

  Future<void> _start() async {
    if (!_canEdit || (!_nativePicker && _path == null)) return;
    final session = _session, epoch = _epoch;
    try {
      final choice = _choices.singleWhere((v) => v.key == _choice);
      final random = Random.secure();
      final request = DirectoryTaskRequest(
        submission: Uint8List.fromList(
          List.generate(32, (_) => random.nextInt(256)),
        ),
        packageId: choice.plugin.id,
        packageDigest: choice.plugin.digest,
        registryRevision: widget.registryRevision!,
        handler: choice.handler,
        selectedPath: _nativePicker ? '' : _path!,
        nativePicker: _nativePicker,
        maxEntries: int.parse(_entries.text),
        maxMetadataBytes: BigInt.parse(_metadata.text),
        timeoutMs: int.parse(_timeout.text),
      );
      DirectoryTaskValidation.request(request);
      setState(() => _formError = false);
      await session.start(request);
      if (session.canExecute) await session.execute();
    } catch (_) {
      if (mounted && epoch == _epoch) setState(() => _formError = true);
    }
  }

  Widget _note(String value) => Padding(
    padding: const EdgeInsets.symmetric(vertical: 5),
    child: Text(value, style: TextStyle(color: widget.muted, fontSize: 12)),
  );
  Widget _button(String label, String key, VoidCallback? action) =>
      OutlinedButton(
        key: ValueKey(key),
        onPressed: action,
        style: OutlinedButton.styleFrom(
          foregroundColor: widget.ink,
          side: BorderSide(color: widget.line),
          shape: RoundedRectangleBorder(borderRadius: widget.radius),
        ),
        child: Text(label),
      );
  String _phase(AppLocalizations l, DirectoryTaskPhase phase) =>
      switch (phase) {
        DirectoryTaskPhase.idle => l.pluginsHttpTaskLocal,
        DirectoryTaskPhase.capturing ||
        DirectoryTaskPhase.ready ||
        DirectoryTaskPhase.running => l.pluginsHttpTaskRunning,
        DirectoryTaskPhase.completed => l.pluginsHttpTaskCompleted,
        DirectoryTaskPhase.cancelled => l.pluginsFileTaskCancelled,
        DirectoryTaskPhase.failed => l.pluginsHttpTaskFailed,
        DirectoryTaskPhase.unknown => l.pluginsMutationUnknown,
      };
  String _storage(AppLocalizations l, IoStoragePhase phase) => switch (phase) {
    IoStoragePhase.local => l.pluginsHttpTaskLocal,
    IoStoragePhase.running => l.pluginsHttpTaskRunning,
    IoStoragePhase.stopping => l.pluginsHttpTaskStopping,
    IoStoragePhase.reclaimed => l.pluginsHttpTaskReclaimed,
    IoStoragePhase.recoveryRequired => l.pluginsHttpTaskRecoveryRequired,
    IoStoragePhase.unavailable => l.pluginsHttpTaskUnavailable,
  };
  String _notice(AppLocalizations l, DirectoryTaskNotice notice) =>
      switch (notice) {
        DirectoryTaskNotice.status => l.pluginsHttpTaskStatusFailed,
        DirectoryTaskNotice.startUnknown => l.pluginsHttpTaskStartUnknown,
        DirectoryTaskNotice.commandUnknown ||
        DirectoryTaskNotice.controlUnknown => l.pluginsHttpTaskControlUnknown,
        DirectoryTaskNotice.readUnknown => l.pluginsHttpTaskReadUnknown,
        DirectoryTaskNotice.interrupted => l.pluginsFileTaskInterrupted,
      };
  @override
  Widget build(BuildContext context) {
    final l = L10n.of(context), choices = _choices, state = _session.snapshot;
    final selected = choices.any((v) => v.key == _choice) ? _choice : null;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          l.pluginsDirectoryTitle,
          style: TextStyle(color: widget.ink, fontWeight: FontWeight.w600),
        ),
        _note(l.pluginsDirectoryIntro),
        if (widget.registryRevision == null)
          _note(l.pluginsHttpTaskCatalogUnavailable),
        DropdownButton<String>(
          key: const ValueKey('directory-task-handler'),
          isExpanded: true,
          value: selected,
          hint: Text(l.pluginsDirectorySelectHandler),
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
          onChanged: _canEdit
              ? (value) => setState(() => _choice = value)
              : null,
        ),
        if (_nativePicker)
          _note(l.pluginsDirectoryNativePickerIntro)
        else
          _button(
            l.pluginsFileMutationPickDirectory,
            'directory-task-pick',
            _canEdit ? _pick : null,
          ),
        if (_path != null ||
            (_session.attempt != null && !_session.attempt!.nativePicker))
          _note(_path ?? _session.attempt!.selectedPath),
        TextField(
          key: const ValueKey('directory-task-entries'),
          controller: _entries,
          enabled: _canEdit,
          keyboardType: TextInputType.number,
          decoration: InputDecoration(labelText: l.pluginsDirectoryLimit),
        ),
        TextField(
          key: const ValueKey('directory-task-metadata'),
          controller: _metadata,
          enabled: _canEdit,
          keyboardType: TextInputType.number,
          decoration: InputDecoration(
            labelText: l.pluginsDirectoryMetadataLimit,
          ),
        ),
        TextField(
          key: const ValueKey('directory-task-timeout'),
          controller: _timeout,
          enabled: _canEdit,
          keyboardType: TextInputType.number,
          decoration: InputDecoration(labelText: l.pluginsFileTaskTimeout),
        ),
        if (_formError) _note(l.pluginsDirectoryFormError),
        Wrap(
          spacing: 8,
          runSpacing: 6,
          children: [
            _button(
              _nativePicker
                  ? l.pluginsDirectoryNativePickerStart
                  : l.pluginsDirectoryStart,
              'directory-task-start',
              _canEdit && selected != null && (_nativePicker || _path != null)
                  ? _start
                  : null,
            ),
            _button(
              l.pluginsHttpTaskRefresh,
              'directory-task-refresh',
              !_session.busy ? _session.refresh : null,
            ),
            if (_session.attempt != null)
              _button(
                l.pluginsDirectoryStart,
                'directory-task-continue',
                _session.canExecute ? _session.execute : null,
              ),
            _button(
              l.pluginsHttpTaskCancel,
              'directory-task-cancel',
              _session.canCancel ? _session.cancel : null,
            ),
            _button(
              l.pluginsHttpTaskRepair,
              'directory-task-repair',
              _session.canRepair ? _session.repair : null,
            ),
            _button(
              l.pluginsHttpTaskAcknowledge,
              'directory-task-ack',
              _session.canAcknowledge ? _session.acknowledge : null,
            ),
            _button(
              l.pluginsHttpTaskAbandon,
              'directory-task-abandon',
              _session.canAbandon ? _session.abandon : null,
            ),
          ],
        ),
        if (_session.busy || _picking) const LinearProgressIndicator(),
        if (_session.attempt != null) _note(_phase(l, _session.phase)),
        if (state != null) _note(_storage(l, state.storage)),
        if (state?.key != null && !_session.ownsTask)
          _note(l.pluginsFileTaskOtherSession),
        if (_session.notice != null) _note(_notice(l, _session.notice!)),
        if (_session.captured != null)
          _note(
            l.pluginsDirectoryEntries(_session.captured!.entries.toString()),
          ),
        if (_session.history.isNotEmpty) _note(l.pluginsHttpTaskHistory),
        for (final h in _session.history.reversed.take(5))
          _note('${_phase(l, h.phase)} · ${h.entries ?? '—'}'),
      ],
    );
  }
}
