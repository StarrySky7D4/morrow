import 'dart:async';
import 'dart:math';
import 'dart:typed_data';
import 'package:flutter/material.dart';
import 'package:morrow_i18n/morrow_i18n.dart';
import 'fs_watch_models.dart';
import 'fs_watch_session.dart';
import 'io_task_models.dart';
import 'plugin_library.dart';
import 'session_view_state.dart';

String _hex(List<int> value) =>
    value.map((v) => v.toRadixString(16).padLeft(2, '0')).join();

class _Choice {
  const _Choice(this.plugin, this.handler);
  final PluginLibraryEntry plugin;
  final String handler;
  String get key => '${plugin.id}:${_hex(plugin.digest)}:$handler';
}

/// A view over the original retained session. Leaving the view withdraws the
/// pending native confirmation; it never discards a receipt or opens a new run.
class FsWatchManager extends StatefulWidget {
  const FsWatchManager({
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
  });
  final FsWatchBackend backend;
  final WorkbenchIoTaskControl ioBackend;
  final List<PluginLibraryEntry> plugins;
  final BigInt? registryRevision;
  final Color ink, muted, line;
  final BorderRadius radius;
  final VoidCallback? onChanged;
  @override
  State<FsWatchManager> createState() => _FsWatchManagerState();
}

class _FsWatchManagerState extends State<FsWatchManager>
    with SessionViewState<FsWatchManager> {
  late FsWatchSession _session;
  final _path = TextEditingController();
  final _frames = TextEditingController();
  final _bytes = TextEditingController();
  final _window = TextEditingController();
  String? _choice, _availability;
  bool _invalid = false, _availabilityQueued = false;
  int _epoch = 0;
  List<_Choice> get _choices => [
    for (final p in widget.plugins)
      if (p.enabled &&
          p.available &&
          p.fsWatchSupported &&
          p.fsWatchApproved &&
          p.fsWatchLimits != null &&
          p.approvedIo.contains('file-list'))
        for (final h in p.fsWatchHandlers) _Choice(p, h),
  ];
  bool get _canEdit =>
      sessionViewActive && _session.canStart && widget.registryRevision != null;
  String _catalog(FsWatchManager w) =>
      '${w.registryRevision}|${w.plugins.map((p) => '${p.id}:${_hex(p.digest)}:${p.enabled}:${p.available}:${p.fsWatchSupported}:${p.fsWatchApproved}:${p.approvedIo.join(',')}:${p.fsWatchHandlers.join(',')}:${p.fsWatchLimits?.frames}:${p.fsWatchLimits?.wireBytes}:${p.fsWatchLimits?.windowMs}').join('|')}';

  @override
  void initState() {
    super.initState();
    _attach();
  }

  void _attach() {
    _session = FsWatchSession.forBackend(widget.backend, widget.ioBackend);
    _session.addListener(_changed);
    final attempt = _session.attempt;
    if (attempt != null) {
      _path.text = attempt.path;
      _frames.text = attempt.limits.frames.toString();
      _bytes.text = attempt.limits.wireBytes.toString();
      _window.text = attempt.limits.windowMs.toString();
    }
    if (sessionViewActive) unawaited(_session.initialize());
  }

  @override
  void didUpdateWidget(covariant FsWatchManager old) {
    super.didUpdateWidget(old);
    final changed =
        !identical(old.backend, widget.backend) ||
        !identical(old.ioBackend, widget.ioBackend);
    if (changed || _catalog(old) != _catalog(widget)) {
      unawaited(_session.withdrawView());
      ++_epoch;
      _choice = null;
      _invalid = false;
    }
    if (changed) {
      _session.removeListener(_changed);
      _availability = null;
      _availabilityQueued = false;
      _path.clear();
      _frames.clear();
      _bytes.clear();
      _window.clear();
      _attach();
    } else if (_catalog(old) != _catalog(widget) && sessionViewActive) {
      unawaited(_session.initialize());
    }
  }

  @override
  void sessionViewVisibilityChanged() {
    if (sessionViewActive) {
      if (!_session.canStart) unawaited(_session.initialize());
    } else if (_session.attempt != null) {
      unawaited(_session.withdrawView());
    }
  }

  @override
  void dispose() {
    ++_epoch;
    _session.removeListener(_changed);
    unawaited(_session.withdrawView());
    _path.dispose();
    _frames.dispose();
    _bytes.dispose();
    _window.dispose();
    super.dispose();
  }

  void _changed() {
    if (!mounted) return;
    markSessionViewDirty();
    final value =
        '${_session.phase}:${_session.task?.storage}:${_session.task?.key == null ? '' : _hex(_session.task!.key!)}';
    if (value != _availability && !_availabilityQueued) {
      _availabilityQueued = true;
      final session = _session, epoch = _epoch;
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (!mounted || !identical(session, _session)) return;
        _availabilityQueued = false;
        if (epoch != _epoch) return;
        _availability = value;
        widget.onChanged?.call();
      });
    }
  }

  void _select(String? value) {
    // A dropdown returns its draft choice before its popup route is dismissed.
    // This changes only the form, never starts work or renews an active run.
    if (!_session.canStart || widget.registryRevision == null) return;
    final selected = _choices.where((c) => c.key == value).firstOrNull;
    if (value != null && selected == null) return;
    setState(() {
      _choice = value;
      _invalid = false;
      if (value == null) return;
      final cap = selected!.plugin.fsWatchLimits!;
      _frames.text = min(cap.frames, 4).toString();
      _bytes.text = cap.wireBytes.toString();
      _window.text = cap.windowMs.toString();
    });
  }

  Future<void> _start() async {
    if (!_canEdit) return;
    final session = _session, epoch = _epoch;
    try {
      final choice = _choices.singleWhere((c) => c.key == _choice);
      final cap = choice.plugin.fsWatchLimits!;
      final limits = FsWatchLimits(
        frames: int.parse(_frames.text),
        wireBytes: BigInt.parse(_bytes.text),
        windowMs: int.parse(_window.text),
      );
      if (limits.frames > cap.frames ||
          limits.wireBytes > cap.wireBytes ||
          limits.windowMs > cap.windowMs)
        throw const FormatException('Declared ceiling');
      final random = Random.secure();
      final request = FsWatchRequest(
        submission: Uint8List.fromList(
          List.generate(32, (_) => random.nextInt(256)),
        ),
        packageId: choice.plugin.id,
        digest: choice.plugin.digest,
        revision: widget.registryRevision!,
        handler: choice.handler,
        // Preserve exactly what the user nominated, including raw UTF-16.
        path: _path.text,
        limits: limits,
      );
      setState(() => _invalid = false);
      await session.start(request);
    } catch (_) {
      if (mounted && epoch == _epoch && identical(session, _session)) {
        setState(() => _invalid = true);
      }
    }
  }

  Widget _note(String text, {String? key}) => Padding(
    padding: const EdgeInsets.symmetric(vertical: 5),
    child: Text(
      text,
      key: key == null ? null : ValueKey(key),
      style: TextStyle(color: widget.muted, fontSize: 12),
    ),
  );
  Widget _button(String label, String key, VoidCallback? action) =>
      OutlinedButton(
        key: ValueKey(key),
        onPressed: sessionViewActive ? action : null,
        style: OutlinedButton.styleFrom(
          foregroundColor: widget.ink,
          side: BorderSide(color: widget.line),
          shape: RoundedRectangleBorder(borderRadius: widget.radius),
        ),
        child: Text(label),
      );
  Widget _field(
    String label,
    String key,
    TextEditingController controller, {
    bool numeric = false,
  }) => TextField(
    key: ValueKey(key),
    controller: controller,
    enabled: _canEdit,
    keyboardType: numeric ? TextInputType.number : TextInputType.text,
    decoration: InputDecoration(labelText: label),
  );
  String _outcome(AppLocalizations l, FsWatchResult r) => r.unknown
      ? l.pluginsWatchUnknown
      : r.success
      ? l.pluginsHttpTaskCompleted
      : switch (r.error) {
          FsWatchError.cancelled => l.pluginsWatchCancelled,
          FsWatchError.expired => l.pluginsWatchExpired,
          _ => l.pluginsHttpTaskFailed,
        };
  String _phase(AppLocalizations l) => switch (_session.phase) {
    FsWatchSessionPhase.idle => l.pluginsWatchIdle,
    FsWatchSessionPhase.running => l.pluginsWatchRunning,
    FsWatchSessionPhase.reviewReady => l.pluginsWatchReady,
    FsWatchSessionPhase.reviewed => l.pluginsWatchReviewed,
    FsWatchSessionPhase.acknowledged => l.pluginsWatchAcknowledged,
    FsWatchSessionPhase.unknown => l.pluginsWatchUnknown,
  };

  @override
  Widget build(BuildContext context) {
    final l = L10n.of(context), choices = _choices, attempt = _session.attempt;
    final selected = choices.where((c) => c.key == _choice).firstOrNull;
    final result = _session.result;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          l.pluginsWatchTitle,
          style: TextStyle(color: widget.ink, fontWeight: FontWeight.w600),
        ),
        _note(l.pluginsWatchIntro),
        if (widget.registryRevision == null)
          _note(l.pluginsHttpTaskCatalogUnavailable),
        if (choices.isEmpty) _note(l.pluginsWatchNoPlugins),
        DropdownButton<String>(
          key: const ValueKey('fs-watch-handler'),
          isExpanded: true,
          value: selected?.key,
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
        // Keep this callback attached while its own popup covers the page.
        // Admission still requires the active original page in _start.
        onChanged: _session.canStart && widget.registryRevision != null
            ? _select
            : null,
        ),
        _field(l.pluginsWatchPath, 'fs-watch-path', _path),
        _field(l.pluginsWatchFrames, 'fs-watch-frames', _frames, numeric: true),
        _field(l.pluginsWatchBytes, 'fs-watch-bytes', _bytes, numeric: true),
        _field(
          l.pluginsFileTaskTimeout,
          'fs-watch-window',
          _window,
          numeric: true,
        ),
        if (selected != null)
          _note(
            l.pluginsWatchBounds(
              selected.plugin.fsWatchLimits!.frames.toString(),
              selected.plugin.fsWatchLimits!.wireBytes.toString(),
              selected.plugin.fsWatchLimits!.windowMs.toString(),
            ),
          ),
        if (_invalid) _note(l.pluginsWatchFormError),
        if (attempt != null)
          _note(
            '${attempt.packageId} · ${attempt.handler}\n${attempt.path}',
            key: 'fs-watch-original',
          ),
        _note(_phase(l), key: 'fs-watch-phase'),
        if (_session.task?.key != null && !_session.ownsTask)
          _note(l.pluginsFileTaskOtherSession),
        Wrap(
          spacing: 8,
          runSpacing: 6,
          children: [
            _button(
              l.pluginsWatchStart,
              'fs-watch-start',
              _canEdit && selected != null ? _start : null,
            ),
            _button(
              l.pluginsHttpTaskRefresh,
              'fs-watch-refresh',
              _session.canPoll
                  ? _session.poll
                  : !_session.busy &&
                        _session.phase == FsWatchSessionPhase.idle &&
                        attempt == null
                  ? _session.initialize
                  : null,
            ),
            _button(
              l.pluginsHttpTaskRead,
              'fs-watch-read',
              _session.canRead ? _session.read : null,
            ),
            _button(
              l.pluginsHttpTaskCancel,
              'fs-watch-cancel',
              _session.canCancel ? _session.cancel : null,
            ),
            _button(
              l.pluginsHttpTaskRepair,
              'fs-watch-repair',
              _session.canRepair ? _session.repair : null,
            ),
            _button(
              l.pluginsHttpTaskAcknowledge,
              'fs-watch-ack',
              _session.canAcknowledge ? _session.acknowledge : null,
            ),
            _button(
              l.pluginsWatchNew,
              'fs-watch-new',
              _session.canBeginNew
                  ? () {
                      _session.beginNew();
                      setState(() {
                        _choice = null;
                        _invalid = false;
                      });
                    }
                  : null,
            ),
          ],
        ),
        if (_session.busy) const LinearProgressIndicator(),
        if (result != null) ...[
          _note(_outcome(l, result), key: 'fs-watch-result'),
          _note(
            l.pluginsWatchRecords(
              result.deliveries.length.toString(),
              result.chargedBytes.toString(),
            ),
          ),
          if (result.knownClosed &&
              (result.workerJoined || result.error == FsWatchError.spawn))
            _note(l.pluginsWatchClosed),
        ],
        for (final record in _session.history.reversed)
          _note(
            '${record.request.packageId} · ${record.request.path}\n${_outcome(l, record.result)}',
          ),
      ],
    );
  }
}
