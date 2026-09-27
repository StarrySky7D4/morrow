import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:math';
import 'dart:typed_data';

import 'package:crypto/crypto.dart';
import 'package:file_selector/file_selector.dart';
import 'package:flutter/foundation.dart' show listEquals;
import 'package:flutter/material.dart';
import 'package:morrow_i18n/morrow_i18n.dart';

import 'io_task_models.dart';
import 'mutation_execution_session.dart';
import 'mutation_task_models.dart';
import 'mutation_workflow.dart';
import 'plugin_library.dart';
import 'session_view_state.dart';

export 'mutation_workflow.dart' show mutationExecutionSubject;

const _maxContentBytes = MutationTaskValidation.maxContentBytes;

String _hex(List<int> bytes) =>
    bytes.map((byte) => byte.toRadixString(16).padLeft(2, '0')).join();

Uint8List _token() {
  final random = Random.secure();
  Uint8List token;
  do {
    token = Uint8List.fromList(List.generate(32, (_) => random.nextInt(256)));
  } while (token.every((byte) => byte == 0));
  return token;
}

String _packageKey(PluginLibraryEntry plugin) =>
    '${plugin.id}:${_hex(plugin.digest)}';
String _capability(MutationDisposition disposition) =>
    'file-${disposition.name}';

final class _Approval {
  const _Approval({
    required this.request,
    required this.packageName,
    required this.target,
    required this.contentLength,
    required this.contentSha256,
    required this.operationId,
  });
  final MutationStartRequest request;
  final String packageName, target, operationId;
  final int contentLength;
  final Uint8List? contentSha256;
}

class _Draft {
  MutationDisposition disposition = MutationDisposition.create;
  String? package, directory, file, contentName;
  XFile? content;
  String name = '';
  _Approval? approval;
}

final _drafts = Expando<_Draft>('mutation execution drafts');

/// An explicit, native-only file operation. The session owns uncertain work;
/// this view only retains one local draft and the last reviewed operation.
class MutationExecutionManager extends StatefulWidget {
  const MutationExecutionManager({
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
    this.onOpenRecovery,
    this.pickDirectory,
    this.pickFile,
    this.pickContent,
  });

  final MutationTaskBackend backend;
  final WorkbenchIoTaskControl ioBackend;
  final List<PluginLibraryEntry> plugins;
  final BigInt? registryRevision;
  final Color ink, muted, line;
  final BorderRadius radius;
  final VoidCallback? onChanged, onOpenRecovery;
  final Future<String?> Function()? pickDirectory;
  final Future<XFile?> Function()? pickFile, pickContent;

  @override
  State<MutationExecutionManager> createState() =>
      _MutationExecutionManagerState();
}

class _MutationExecutionManagerState extends State<MutationExecutionManager>
    with SessionViewState<MutationExecutionManager> {
  late MutationExecutionSession _session;
  late _Draft _draft;
  final _name = TextEditingController();
  Timer? _timer;
  int _epoch = 0;
  bool _picking = false, _reviewing = false, _availabilityQueued = false;
  String? _formError, _availability;
  void Function()? _cancelContentRead;

  List<PluginLibraryEntry> _choices(MutationDisposition disposition) => [
    for (final plugin in widget.plugins)
      if (plugin.enabled &&
          plugin.available &&
          plugin.mutationBudget == null &&
          plugin.declaredIo.contains(_capability(disposition)) &&
          plugin.approvedIo.contains(_capability(disposition)))
        plugin,
  ];

  String _catalog(MutationExecutionManager value) =>
      '${value.registryRevision}|${value.plugins.map((p) => '${p.id}:${_hex(p.digest)}:${p.enabled}:${p.available}:${p.declaredIo.join(',')}:${p.approvedIo.join(',')}').join('|')}';

  PluginLibraryEntry? _selectedPackage() {
    for (final plugin in _choices(_draft.disposition)) {
      if (_packageKey(plugin) == _draft.package) return plugin;
    }
    return null;
  }

  bool get _canEdit =>
      !_picking &&
      !_reviewing &&
      _session.canStart &&
      widget.registryRevision != null;

  @override
  void initState() {
    super.initState();
    _attach();
  }

  void _attach() {
    _session = MutationExecutionSession.forBackend(
      widget.backend,
      widget.ioBackend,
    );
    _draft = _drafts[_session] ??= _Draft();
    _name.text = _draft.name;
    _session.addListener(_changed);
    if (!_session.busy) unawaited(_session.refresh());
  }

  @override
  void didUpdateWidget(covariant MutationExecutionManager oldWidget) {
    super.didUpdateWidget(oldWidget);
    final backendChanged =
        !identical(oldWidget.backend, widget.backend) ||
        !identical(oldWidget.ioBackend, widget.ioBackend);
    final catalogChanged = _catalog(oldWidget) != _catalog(widget);
    if (backendChanged) {
      _cancelContentRead?.call();
      _session.removeListener(_changed);
      _timer?.cancel();
      _epoch++;
      _picking = false;
      _reviewing = false;
      _availability = null;
      _availabilityQueued = false;
      _formError = null;
      _attach();
    }
    if (catalogChanged) {
      _cancelContentRead?.call();
      _epoch++;
      _picking = false;
      _reviewing = false;
      _draft.approval = null;
      _draft.package = null;
      _formError = null;
    }
  }

  @override
  void dispose() {
    _cancelContentRead?.call();
    _epoch++;
    _timer?.cancel();
    _session.removeListener(_changed);
    _name.dispose();
    super.dispose();
  }

  @override
  void sessionViewVisibilityChanged() {
    _schedule();
    if (sessionViewActive && !_session.busy && !_reviewing) {
      unawaited(_session.refresh());
    }
  }

  bool get _ownsCurrentTask {
    final request = _session.request;
    return request != null &&
        _session.snapshot?.key != null &&
        listEquals(request.submission, _session.snapshot?.submission);
  }

  void _schedule() {
    _timer?.cancel();
    _timer = null;
    if (!sessionViewActive ||
        _session.busy ||
        _reviewing ||
        !_ownsCurrentTask ||
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

  Future<void> _run(Future<void> Function() action) async {
    setState(() => _formError = null);
    try {
      await action();
    } catch (_) {
      if (mounted) setState(() => _formError = 'operation');
    }
  }

  Future<void> _pickTarget() async {
    if (!_canEdit || _selectedPackage() == null) return;
    final epoch = ++_epoch, backend = widget.backend;
    setState(() => _picking = true);
    try {
      if (_draft.disposition == MutationDisposition.create) {
        final directory =
            await (widget.pickDirectory?.call() ?? getDirectoryPath());
        if (!mounted ||
            epoch != _epoch ||
            !identical(backend, widget.backend)) {
          return;
        }
        if (directory != null) {
          setState(() {
            _draft.directory = directory;
            _draft.file = null;
            _draft.approval = null;
          });
        }
      } else {
        final file = await (widget.pickFile?.call() ?? openFile());
        if (!mounted ||
            epoch != _epoch ||
            !identical(backend, widget.backend)) {
          return;
        }
        if (file != null) {
          setState(() {
            _draft.file = file.path;
            _draft.directory = null;
            _draft.approval = null;
          });
        }
      }
    } catch (_) {
      if (mounted && epoch == _epoch) setState(() => _formError = 'selection');
    } finally {
      if (mounted && epoch == _epoch) setState(() => _picking = false);
    }
  }

  Future<void> _pickSource() async {
    if (!_canEdit || _draft.disposition != MutationDisposition.create) return;
    final epoch = ++_epoch, backend = widget.backend;
    setState(() => _picking = true);
    try {
      final content = await (widget.pickContent?.call() ?? openFile());
      if (!mounted || epoch != _epoch || !identical(backend, widget.backend)) {
        return;
      }
      if (content != null) {
        setState(() {
          _draft.content = content;
          _draft.contentName = content.name;
          _draft.approval = null;
        });
      }
    } catch (_) {
      if (mounted && epoch == _epoch) setState(() => _formError = 'selection');
    } finally {
      if (mounted && epoch == _epoch) setState(() => _picking = false);
    }
  }

  Future<Uint8List> _readContent(XFile? file, int epoch) async {
    if (file == null) return Uint8List(0);
    if (await file.length().timeout(const Duration(seconds: 15)) >
        _maxContentBytes) {
      throw const FormatException('Content exceeds 16 MiB');
    }
    if (!mounted || epoch != _epoch) {
      throw StateError('Content review was cancelled');
    }
    final bytes = BytesBuilder();
    final completed = Completer<Uint8List>();
    StreamSubscription<List<int>>? subscription;
    void fail(Object error, [StackTrace? stack]) {
      if (!completed.isCompleted) completed.completeError(error, stack);
      if (subscription != null) unawaited(subscription.cancel());
    }

    subscription = file.openRead().listen(
      (chunk) {
        if (!mounted || epoch != _epoch) {
          fail(StateError('Content review was cancelled'));
        } else if (chunk.length > _maxContentBytes - bytes.length) {
          fail(const FormatException('Content exceeds 16 MiB'));
        } else {
          bytes.add(chunk);
        }
      },
      onError: fail,
      onDone: () {
        if (!completed.isCompleted) completed.complete(bytes.takeBytes());
      },
      cancelOnError: true,
    );
    final timer = Timer(
      const Duration(seconds: 30),
      () => fail(TimeoutException('Content review deadline')),
    );
    void cancelRead() => fail(StateError('Content review was cancelled'));
    _cancelContentRead = cancelRead;
    try {
      return await completed.future;
    } finally {
      timer.cancel();
      if (identical(_cancelContentRead, cancelRead)) {
        _cancelContentRead = null;
      }
      await subscription.cancel();
    }
  }

  void _cancelReview() {
    if (!_reviewing) return;
    _epoch++;
    _cancelContentRead?.call();
    setState(() {
      _reviewing = false;
      _formError = null;
    });
  }

  String? _target() {
    if (_draft.disposition == MutationDisposition.create) {
      final directory = _draft.directory, name = _name.text;
      if (directory == null || name.isEmpty) return null;
      return '${directory.endsWith(Platform.pathSeparator) ? directory : '$directory${Platform.pathSeparator}'}$name';
    }
    return _draft.file;
  }

  Future<void> _reviewPrepare() async {
    if (!_canEdit) return;
    final package = _selectedPackage(), target = _target();
    if (package == null || target == null) {
      setState(() => _formError = 'selection');
      return;
    }
    if (_draft.disposition == MutationDisposition.create &&
        (_name.text.contains('/') || _name.text.contains('\\'))) {
      setState(() => _formError = 'selection');
      return;
    }
    final epoch = ++_epoch, backend = widget.backend;
    _timer?.cancel();
    _timer = null;
    setState(() {
      _reviewing = true;
      _formError = null;
    });
    try {
      final content = _draft.disposition == MutationDisposition.create
          ? await _readContent(_draft.content, epoch)
          : Uint8List(0);
      if (!mounted || epoch != _epoch || !identical(widget.backend, backend)) {
        return;
      }
      final contentHash = _draft.disposition == MutationDisposition.create
          ? Uint8List.fromList(sha256.convert(content).bytes)
          : null;
      final operationId = 'morrow-${_hex(_token().sublist(0, 16))}';
      final selectedPath = _draft.disposition == MutationDisposition.create
          ? _draft.directory!
          : _draft.file!;
      final relativePath = _draft.disposition == MutationDisposition.create
          ? _name.text
          : '';
      final approval = sha256.convert(
        utf8.encode(
          jsonEncode([
            'morrow.file-actions.approval.v1',
            package.id,
            _hex(package.digest),
            widget.registryRevision!.toString(),
            _draft.disposition.name,
            selectedPath,
            relativePath,
            operationId,
            content.length,
            contentHash == null ? '' : _hex(contentHash),
          ]),
        ),
      );
      final request = MutationStartRequest(
        submission: _token(),
        packageId: package.id,
        packageDigest: package.digest,
        registryRevision: widget.registryRevision!,
        disposition: _draft.disposition,
        selectedPath: selectedPath,
        relativePath: relativePath,
        subject: mutationExecutionSubject,
        approvalSha256: Uint8List.fromList(approval.bytes),
        timeoutMs: 30000,
      );
      MutationTaskValidation.validateStart(request);
      final reviewed = _Approval(
        request: request,
        packageName: package.name,
        target: target,
        contentLength: content.length,
        contentSha256: contentHash,
        operationId: operationId,
      );
      final confirmed = await _confirm(reviewed, execute: false);
      if (!mounted ||
          epoch != _epoch ||
          !identical(widget.backend, backend) ||
          _selectedPackage() != package ||
          widget.registryRevision != request.registryRevision) {
        return;
      }
      if (confirmed != true) return;
      if (!_session.canStart) return;
      _draft.approval = reviewed;
      setState(() => _reviewing = false);
      await _session.prepare(
        request,
        operationId: operationId,
        content: request.disposition == MutationDisposition.create
            ? content
            : null,
      );
    } on FormatException catch (error) {
      if (mounted && epoch == _epoch) {
        setState(() => _formError = error.message);
      }
    } catch (_) {
      if (mounted && epoch == _epoch) setState(() => _formError = 'operation');
    } finally {
      if (mounted && epoch == _epoch) setState(() => _reviewing = false);
    }
  }

  Future<bool?> _confirm(_Approval review, {required bool execute}) {
    final l = L10n.of(context);
    return showDialog<bool>(
      context: context,
      builder: (dialogContext) => AlertDialog(
        title: Text(
          execute
              ? l.pluginsFileMutationExecuteTitle
              : l.pluginsFileMutationPrepareTitle,
        ),
        content: SingleChildScrollView(
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text('${l.pluginsEndpointPackage}: ${review.packageName}'),
              SelectableText(review.request.packageId),
              SelectableText(_hex(review.request.packageDigest)),
              Text(
                review.request.disposition == MutationDisposition.create
                    ? l.pluginsIoFileCreate
                    : l.pluginsIoFileDelete,
              ),
              Text('${l.pluginsFileMutationTarget}: ${review.target}'),
              Text(
                '${l.pluginsFileMutationContentLength}: ${review.contentLength}',
              ),
              if (review.contentSha256 != null)
                SelectableText(
                  '${l.pluginsFileMutationContentSha256}: ${_hex(review.contentSha256!)}',
                ),
              if (execute &&
                  review.request.disposition == MutationDisposition.delete)
                Text(l.pluginsFileMutationExecuteDeleteConfirm),
            ],
          ),
        ),
        actions: [
          TextButton(
            key: const ValueKey('mutation-execution-confirm-cancel'),
            onPressed: () => Navigator.pop(dialogContext, false),
            child: Text(
              MaterialLocalizations.of(dialogContext).cancelButtonLabel,
            ),
          ),
          FilledButton(
            key: ValueKey(
              execute
                  ? 'mutation-execution-confirm-execute'
                  : 'mutation-execution-confirm-prepare',
            ),
            onPressed: () => Navigator.pop(dialogContext, true),
            child: Text(
              execute
                  ? review.request.disposition == MutationDisposition.delete
                        ? l.pluginsFileMutationExecuteDeleteConfirm
                        : l.pluginsFileMutationExecuteCreateConfirm
                  : l.pluginsFileMutationPrepareConfirm,
            ),
          ),
        ],
      ),
    );
  }

  Future<void> _execute() async {
    final approval = _draft.approval;
    if (!_session.canExecute || approval == null) return;
    final epoch = _epoch, backend = widget.backend;
    final current = _selectedPackage();
    if (current == null ||
        current.id != approval.request.packageId ||
        !listEquals(current.digest, approval.request.packageDigest) ||
        widget.registryRevision != approval.request.registryRevision) {
      setState(() => _formError = 'scope');
      return;
    }
    _timer?.cancel();
    _timer = null;
    setState(() => _reviewing = true);
    try {
      final confirmed = await _confirm(approval, execute: true);
      if (confirmed != true ||
          !mounted ||
          epoch != _epoch ||
          !identical(backend, widget.backend) ||
          !_session.canExecute ||
          !identical(_draft.approval, approval) ||
          _selectedPackage() != current) {
        return;
      }
      setState(() => _reviewing = false);
      await _run(_session.execute);
    } finally {
      if (mounted && epoch == _epoch) setState(() => _reviewing = false);
    }
  }

  Widget _note(String value) => Padding(
    padding: const EdgeInsets.symmetric(vertical: 5),
    child: Text(value, style: TextStyle(color: widget.muted, fontSize: 12)),
  );

  Widget _diagnostic(String id, String title, Object error) {
    final raw = error.toString();
    final bounded = raw.length <= 2048 ? raw : raw.substring(0, 2048);
    return ExpansionTile(
      key: ValueKey('mutation-execution-$id'),
      title: Text(title, style: TextStyle(color: widget.muted, fontSize: 12)),
      children: [SelectableText(bounded)],
    );
  }

  Widget _button(String label, String id, Future<void> Function()? action) =>
      OutlinedButton(
        key: ValueKey('mutation-execution-$id'),
        onPressed: action,
        style: OutlinedButton.styleFrom(
          foregroundColor: widget.ink,
          side: BorderSide(color: widget.line),
          shape: RoundedRectangleBorder(borderRadius: widget.radius),
        ),
        child: Text(label),
      );

  @override
  Widget build(BuildContext context) {
    final l = L10n.of(context);
    final choices = _choices(_draft.disposition);
    final selected = choices.any((p) => _packageKey(p) == _draft.package)
        ? _draft.package
        : null;
    final approval = _draft.approval;
    final result = _session.lastResult;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          l.pluginsFileMutationTitle,
          style: TextStyle(color: widget.ink, fontWeight: FontWeight.w600),
        ),
        _note(l.pluginsFileMutationIntro),
        if (widget.registryRevision == null)
          _note(l.pluginsHttpTaskCatalogUnavailable),
        DropdownButton<MutationDisposition>(
          key: const ValueKey('mutation-execution-disposition'),
          isExpanded: true,
          value: _draft.disposition,
          items: [
            DropdownMenuItem(
              value: MutationDisposition.create,
              child: Text(l.pluginsIoFileCreate),
            ),
            DropdownMenuItem(
              value: MutationDisposition.delete,
              child: Text(l.pluginsIoFileDelete),
            ),
          ],
          onChanged: _canEdit
              ? (value) => setState(() {
                  if (value == null) return;
                  _draft.disposition = value;
                  _draft.package = null;
                  _draft.directory = null;
                  _draft.file = null;
                  _draft.approval = null;
                })
              : null,
        ),
        DropdownButton<String>(
          key: const ValueKey('mutation-execution-package'),
          isExpanded: true,
          value: selected,
          hint: Text(l.pluginsEndpointPackage),
          items: [
            for (final plugin in choices)
              DropdownMenuItem(
                value: _packageKey(plugin),
                child: Text(plugin.name, overflow: TextOverflow.ellipsis),
              ),
          ],
          onChanged: _canEdit
              ? (value) => setState(() {
                  _draft.package = value;
                  _draft.approval = null;
                })
              : null,
        ),
        Wrap(
          spacing: 8,
          runSpacing: 8,
          children: [
            _button(
              _draft.disposition == MutationDisposition.create
                  ? l.pluginsFileMutationPickDirectory
                  : l.pluginsFileMutationPickFile,
              'pick-target',
              _canEdit && selected != null ? _pickTarget : null,
            ),
            if (_draft.disposition == MutationDisposition.create)
              _button(
                l.pluginsFileMutationPickContent,
                'pick-content',
                _canEdit && selected != null ? _pickSource : null,
              ),
            if (_draft.disposition == MutationDisposition.create &&
                _draft.content != null)
              _button(
                l.pluginsFileMutationEmptyContent,
                'empty-content',
                _canEdit
                    ? () async => setState(() {
                        _draft.content = null;
                        _draft.contentName = null;
                        _draft.approval = null;
                      })
                    : null,
              ),
          ],
        ),
        if (_draft.directory != null) _note(_draft.directory!),
        if (_draft.file != null) _note(_draft.file!),
        if (_draft.contentName != null) _note(_draft.contentName!),
        if (_draft.disposition == MutationDisposition.create)
          TextField(
            key: const ValueKey('mutation-execution-file-name'),
            controller: _name,
            enabled: _canEdit,
            maxLength: 255,
            onChanged: (value) {
              _draft.name = value;
              _draft.approval = null;
            },
            decoration: InputDecoration(
              labelText: l.pluginsFileMutationFileName,
            ),
          ),
        const SizedBox(height: 8),
        Wrap(
          spacing: 8,
          runSpacing: 8,
          children: [
            _button(
              l.pluginsFileMutationReview,
              'review',
              _canEdit && selected != null ? _reviewPrepare : null,
            ),
            if (_reviewing)
              _button(l.pluginsCancel, 'cancel-review', () async {
                _cancelReview();
              }),
            _button(
              l.pluginsFileMutationExecute,
              'execute',
              !_reviewing &&
                      _session.canExecute &&
                      approval != null &&
                      widget.registryRevision ==
                          approval.request.registryRevision &&
                      _selectedPackage() != null
                  ? _execute
                  : null,
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
              l.pluginsMutationRetry,
              'retry',
              _session.canRetrySubmission
                  ? () => _run(_session.retrySubmission)
                  : null,
            ),
            _button(
              l.pluginsFileMutationQuery,
              'query',
              _session.canQuery ? () => _run(_session.query) : null,
            ),
            _button(
              l.pluginsFileMutationCancelPlan,
              'cancel-plan',
              _session.canCancelPlan ? () => _run(_session.cancelPlan) : null,
            ),
            _button(
              l.pluginsMutationRelease,
              'release',
              _session.canRelease ? () => _run(_session.release) : null,
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
        if (_formError != null)
          _note(switch (_formError) {
            'selection' => l.pluginsFileMutationSelectionRequired,
            'scope' => l.pluginsFileMutationScopeChanged,
            'Content exceeds 16 MiB' => l.pluginsFileMutationFileTooLarge,
            _ => l.pluginsFileMutationFailure,
          }),
        if (_session.error != null)
          _diagnostic(
            'operation-error',
            l.pluginsFileMutationOperationError,
            _session.error!,
          ),
        if (_session.cleanupError != null)
          _diagnostic(
            'cleanup-error',
            l.pluginsFileMutationCleanupError,
            _session.cleanupError!,
          ),
        if (_session.uncertain) _note(l.pluginsMutationUnknown),
        if (_session.snapshot?.key != null && !_ownsCurrentTask)
          _note(l.pluginsFileTaskOtherSession),
        if (approval != null) ...[
          _note('${l.pluginsFileMutationTarget}: ${approval.target}'),
          _note(
            '${l.pluginsFileMutationContentLength}: ${approval.contentLength}',
          ),
          if (approval.contentSha256 != null)
            _note(
              '${l.pluginsFileMutationContentSha256}: ${_hex(approval.contentSha256!)}',
            ),
        ],
        if (result != null) ...[
          _note(
            result.phase == MutationPhase.outcomeUnknown
                ? l.pluginsMutationUnknown
                : result.phase == MutationPhase.observed
                ? l.pluginsMutationObserved
                : result.phase == MutationPhase.prepared
                ? l.pluginsMutationPrepared
                : result.phase == MutationPhase.cancelledBeforeDispatch
                ? l.pluginsMutationCancelled
                : l.pluginsMutationAbsent,
          ),
          _note(
            result.effect == MutationEffect.osSucceeded
                ? l.pluginsMutationSucceeded
                : result.effect == MutationEffect.osRejected
                ? l.pluginsMutationRejected
                : l.pluginsMutationUnspecified,
          ),
        ],
        if (widget.onOpenRecovery != null &&
            _session.canStart &&
            (approval != null ||
                _session.error != null ||
                result?.phase == MutationPhase.outcomeUnknown)) ...[
          _note(l.pluginsFileMutationRecoveryHint),
          _button(l.pluginsFileMutationRecovery, 'recovery', () async {
            widget.onOpenRecovery?.call();
          }),
        ],
      ],
    );
  }
}
