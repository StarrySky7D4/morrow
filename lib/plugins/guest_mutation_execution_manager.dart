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

import 'guest_mutation_execution_session.dart';
import 'guest_mutation_models.dart';
import 'io_task_models.dart';
import 'mutation_task_models.dart';
import 'mutation_workflow.dart';
import 'plugin_library.dart';
import 'session_view_state.dart';

export 'mutation_workflow.dart' show guestMutationExecutionSubject;

const _maxContent = MutationTaskValidation.maxContentBytes;

String _hex(List<int> bytes) =>
    bytes.map((value) => value.toRadixString(16).padLeft(2, '0')).join();
String _packageKey(PluginLibraryEntry value) =>
    '${value.id}:${_hex(value.digest)}';
String _capability(MutationDisposition value) => 'file-${value.name}';

Uint8List _token() {
  final random = Random.secure();
  Uint8List value;
  do {
    value = Uint8List.fromList(List.generate(32, (_) => random.nextInt(256)));
  } while (value.every((byte) => byte == 0));
  return value;
}

final class _Review {
  _Review({
    required this.start,
    required this.packageName,
    required this.target,
    required this.operationId,
    required this.length,
    required this.contentHash,
    required this.planHash,
  });
  final GuestMutationStartRequest start;
  final String packageName, target, operationId;
  final int length;
  final Uint8List? contentHash;
  final Uint8List planHash;
  bool prepareApproved = false;
}

class _Draft {
  MutationDisposition disposition = MutationDisposition.create;
  String? package, directory, file, contentName;
  XFile? content;
  String name = '', jobBudget = '', totalBudget = '';
  _Review? review;
}

final _drafts = Expando<_Draft>('guest mutation drafts');

/// Trusted UI for a separately negotiated mutation-v1 guest. This view never
/// obtains the guest lease or effect permit; the session and original host own
/// those receipts. Hiding the view only pauses display polling.
class GuestMutationExecutionManager extends StatefulWidget {
  const GuestMutationExecutionManager({
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

  final GuestMutationBackend backend;
  final WorkbenchIoTaskControl ioBackend;
  final List<PluginLibraryEntry> plugins;
  final BigInt? registryRevision;
  final Color ink, muted, line;
  final BorderRadius radius;
  final VoidCallback? onChanged;
  final VoidCallback? onOpenRecovery;
  final Future<String?> Function()? pickDirectory;
  final Future<XFile?> Function()? pickFile, pickContent;

  @override
  State<GuestMutationExecutionManager> createState() =>
      _GuestMutationExecutionManagerState();
}

class _GuestMutationExecutionManagerState
    extends State<GuestMutationExecutionManager>
    with SessionViewState<GuestMutationExecutionManager> {
  late GuestMutationExecutionSession _session;
  late _Draft _draft;
  final _name = TextEditingController();
  final _jobBudget = TextEditingController();
  final _totalBudget = TextEditingController();
  Timer? _timer, _dialogTimer;
  DialogRoute<bool>? _reviewRoute;
  int _epoch = 0;
  bool _picking = false, _reviewing = false, _availabilityQueued = false;
  bool _reviewDialogOpen = false;
  String? _formError, _availability;
  void Function()? _cancelContentRead;

  List<PluginLibraryEntry> _choices(MutationDisposition disposition) => [
    for (final plugin in widget.plugins)
      if (plugin.enabled &&
          plugin.available &&
          plugin.mutationSupported &&
          plugin.declaredIo.contains(_capability(disposition)) &&
          plugin.approvedIo.contains(_capability(disposition)))
        plugin,
  ];

  String _catalog(GuestMutationExecutionManager value) =>
      '${value.registryRevision}|${value.plugins.map((p) => '${p.id}:${_hex(p.digest)}:${p.enabled}:${p.available}:${p.mutationSupported}:${p.declaredIo.join(',')}:${p.approvedIo.join(',')}:${p.mutationBudget?.maxJobBytes}:${p.mutationBudget?.maxBytes}').join('|')}';

  PluginLibraryEntry? _selectedPackage() {
    for (final plugin in _choices(_draft.disposition)) {
      if (_packageKey(plugin) == _draft.package) return plugin;
    }
    return null;
  }

  bool _reviewCurrent(_Review review) {
    final package = _selectedPackage();
    return package != null &&
        _draft.package == _packageKey(package) &&
        package.id == review.start.selection.packageId &&
        listEquals(package.digest, review.start.selection.packageDigest) &&
        widget.registryRevision == review.start.selection.registryRevision;
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
    _session = GuestMutationExecutionSession.forBackend(
      widget.backend,
      widget.ioBackend,
    );
    _draft = _drafts[_session] ??= _Draft();
    _name.text = _draft.name;
    _jobBudget.text = _draft.jobBudget;
    _totalBudget.text = _draft.totalBudget;
    _session.addListener(_changed);
    if (!_session.busy) unawaited(_session.refresh());
  }

  @override
  void didUpdateWidget(covariant GuestMutationExecutionManager oldWidget) {
    super.didUpdateWidget(oldWidget);
    final backendChanged =
        !identical(oldWidget.backend, widget.backend) ||
        !identical(oldWidget.ioBackend, widget.ioBackend);
    final catalogChanged = _catalog(oldWidget) != _catalog(widget);
    if (backendChanged || catalogChanged) _dismissReviewDialog();
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
      // A plan not yet approved for Prepare must be reviewed against the new
      // catalog. An already prepared task keeps its original binding; fresh
      // authority checks still occur in its original owner.
      if (_draft.review?.prepareApproved != true) _draft.review = null;
      _draft.package = null;
      _formError = null;
    }
  }

  @override
  void dispose() {
    _cancelContentRead?.call();
    _dismissReviewDialog();
    _epoch++;
    _timer?.cancel();
    _session.removeListener(_changed);
    _dialogTimer?.cancel();
    _name.dispose();
    _jobBudget.dispose();
    _totalBudget.dispose();
    super.dispose();
  }

  @override
  void sessionViewVisibilityChanged() {
    _schedule();
    if (sessionViewActive && !_session.busy && !_reviewing) {
      unawaited(_session.refresh());
    }
  }

  bool get _ownsTask {
    final request = _session.request;
    return request != null &&
        _session.snapshot?.key != null &&
        listEquals(request.selection.submission, _session.snapshot?.submission);
  }

  void _schedule() {
    _timer?.cancel();
    _timer = null;
    if (!sessionViewActive ||
        _session.busy ||
        _reviewing ||
        !_ownsTask ||
        _session.snapshot?.exit != null) {
      return;
    }
    final session = _session;
    _timer = Timer(const Duration(seconds: 1), () {
      if (mounted &&
          sessionViewActive &&
          identical(session, _session) &&
          !_reviewing &&
          !session.busy) {
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

  void _invalidateDraft() {
    _epoch++;
    _cancelContentRead?.call();
    _draft.review = null;
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
            _draft.review = null;
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
            _draft.review = null;
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
          _draft.review = null;
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
        _maxContent) {
      throw const FormatException('Content exceeds 16 MiB');
    }
    if (!mounted || epoch != _epoch) {
      throw StateError('Content review cancelled');
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
          fail(StateError('Content review cancelled'));
        } else if (chunk.length > _maxContent - bytes.length) {
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
    void cancelRead() => fail(StateError('Content review cancelled'));
    _cancelContentRead = cancelRead;
    try {
      return await completed.future;
    } finally {
      timer.cancel();
      if (identical(_cancelContentRead, cancelRead)) _cancelContentRead = null;
      await subscription.cancel();
    }
  }

  void _cancelReview() {
    if (!_reviewing) return;
    _epoch++;
    _cancelContentRead?.call();
    if (_reviewDialogOpen) _dismissReviewDialog();
    setState(() {
      _reviewing = false;
      _formError = null;
    });
    _schedule();
  }

  void _dismissReviewDialog() {
    _dialogTimer?.cancel();
    final route = _reviewRoute;
    if (route == null) return;
    // Catalog changes can arrive during build. Remove exactly our stale route
    // after the frame, never pop another dialog that might be above it.
    WidgetsBinding.instance.addPostFrameCallback((_) {
      route.navigator?.removeRoute(route, false);
    });
  }

  String? _target() {
    if (_draft.disposition == MutationDisposition.create) {
      final directory = _draft.directory;
      if (directory == null || _name.text.isEmpty) return null;
      return '${directory.endsWith(Platform.pathSeparator) ? directory : '$directory${Platform.pathSeparator}'}${_name.text}';
    }
    return _draft.file;
  }

  ApprovedGuestBudget? _approvedBudget(PluginLibraryEntry package) {
    final declaration = package.mutationBudget;
    if (declaration == null) return null;
    final job = BigInt.tryParse(_jobBudget.text.trim());
    final total = BigInt.tryParse(_totalBudget.text.trim());
    if (job == null ||
        total == null ||
        job <= BigInt.zero ||
        total < job ||
        job > declaration.maxJobBytes ||
        total > declaration.maxBytes ||
        job > BigInt.from(GuestMutationValidation.maxApprovedJobBytes) ||
        total > BigInt.from(GuestMutationValidation.maxApprovedBytes)) {
      throw const FormatException('Invalid approved guest budget');
    }
    return ApprovedGuestBudget(maxJobBytes: job, maxBytes: total);
  }

  Future<bool?> _confirm(_Review review, {required String step}) async {
    final l = L10n.of(context);
    final plan = step != 'draft';
    final session = _session;
    Timer? expiryTimer;
    DialogRoute<bool>? route;
    _reviewDialogOpen = true;
    try {
      route = DialogRoute<bool>(
        context: context,
        builder: (dialogContext) => StatefulBuilder(
          builder: (dialogContext, update) {
            expiryTimer ??= plan && mounted
                ? Timer.periodic(const Duration(seconds: 1), (timer) {
                    if (!mounted || !dialogContext.mounted) {
                      timer.cancel();
                      return;
                    }
                    update(() {});
                    if (session.authorizationExpired) timer.cancel();
                  })
                : null;
            _dialogTimer = expiryTimer;
            final expired = plan && session.authorizationExpired;
            final available =
                mounted &&
                (!plan ||
                    (identical(session, _session) &&
                        _reviewCurrent(review) &&
                        (step == 'prepare'
                            ? session.canPrepare
                            : session.canExecute)));
            return AlertDialog(
              title: Text(
                step == 'execute'
                    ? l.pluginsFileMutationExecuteTitle
                    : step == 'prepare'
                    ? l.pluginsFileMutationPrepareTitle
                    : l.pluginsGuestExecutionDraft,
              ),
              content: SingleChildScrollView(
                child: Column(
                  mainAxisSize: MainAxisSize.min,
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    if (expired) Text(l.pluginsGuestExecutionExpired),
                    Text(
                      review.start.selection.disposition ==
                              MutationDisposition.create
                          ? l.pluginsIoFileCreate
                          : l.pluginsIoFileDelete,
                    ),
                    Text('${l.pluginsEndpointPackage}: ${review.packageName}'),
                    SelectableText(review.start.selection.packageId),
                    SelectableText(_hex(review.start.selection.packageDigest)),
                    Text('${l.pluginsFileMutationTarget}: ${review.target}'),
                    Text(
                      '${l.pluginsFileMutationContentLength}: ${review.length}',
                    ),
                    if (review.contentHash != null)
                      SelectableText(
                        '${l.pluginsFileMutationContentSha256}: ${_hex(review.contentHash!)}',
                      ),
                    if (review.start.approvedBudget != null) ...[
                      Text(
                        '${l.pluginsGuestExecutionJobBudget}: ${review.start.approvedBudget!.maxJobBytes}',
                      ),
                      Text(
                        '${l.pluginsGuestExecutionTotalBudget}: ${review.start.approvedBudget!.maxBytes}',
                      ),
                    ],
                    if (plan)
                      SelectableText(
                        '${l.pluginsGuestExecutionPlanSha256}: ${_hex(review.planHash)}',
                      ),
                    if (step == 'execute' &&
                        review.start.selection.disposition ==
                            MutationDisposition.delete)
                      Text(l.pluginsFileMutationExecuteDeleteConfirm),
                  ],
                ),
              ),
              actions: [
                TextButton(
                  key: const ValueKey('guest-mutation-confirm-cancel'),
                  onPressed: () => Navigator.pop(dialogContext, false),
                  child: Text(
                    MaterialLocalizations.of(dialogContext).cancelButtonLabel,
                  ),
                ),
                FilledButton(
                  key: ValueKey('guest-mutation-confirm-$step'),
                  onPressed: available && !expired
                      ? () => Navigator.pop(dialogContext, true)
                      : null,
                  child: Text(
                    step == 'execute'
                        ? review.start.selection.disposition ==
                                  MutationDisposition.delete
                              ? l.pluginsFileMutationExecuteDeleteConfirm
                              : l.pluginsFileMutationExecuteCreateConfirm
                        : step == 'prepare'
                        ? l.pluginsFileMutationPrepareConfirm
                        : l.pluginsGuestExecutionDraft,
                  ),
                ),
              ],
            );
          },
        ),
      );
      _reviewRoute = route;
      return await Navigator.of(context, rootNavigator: true).push(route);
    } finally {
      expiryTimer?.cancel();
      if (identical(_dialogTimer, expiryTimer)) _dialogTimer = null;
      if (identical(_reviewRoute, route)) _reviewRoute = null;
      _reviewDialogOpen = false;
    }
  }

  Future<void> _review() async {
    if (!_canEdit) return;
    final package = _selectedPackage(), target = _target();
    if (package == null ||
        target == null ||
        (_draft.disposition == MutationDisposition.create &&
            (_name.text.contains('/') || _name.text.contains('\\')))) {
      setState(() => _formError = 'selection');
      return;
    }
    final epoch = ++_epoch, backend = widget.backend;
    _timer?.cancel();
    setState(() {
      _reviewing = true;
      _formError = null;
    });
    try {
      final budget = _approvedBudget(package);
      final content = _draft.disposition == MutationDisposition.create
          ? await _readContent(_draft.content, epoch)
          : Uint8List(0);
      if (!mounted || epoch != _epoch || !identical(backend, widget.backend)) {
        return;
      }
      final contentHash = _draft.disposition == MutationDisposition.create
          ? Uint8List.fromList(sha256.convert(content).bytes)
          : null;
      final operationId = 'morrow-guest-${_hex(_token().sublist(0, 16))}';
      final selectedPath = _draft.disposition == MutationDisposition.create
          ? _draft.directory!
          : _draft.file!;
      final relativePath = _draft.disposition == MutationDisposition.create
          ? _name.text
          : '';
      final approval = sha256.convert(
        utf8.encode(
          jsonEncode([
            'morrow.guest.file-actions.approval.v1',
            package.id,
            _hex(package.digest),
            widget.registryRevision!.toString(),
            _draft.disposition.name,
            selectedPath,
            relativePath,
            operationId,
            content.length,
            contentHash == null ? '' : _hex(contentHash),
            budget?.maxJobBytes.toString() ?? '',
            budget?.maxBytes.toString() ?? '',
          ]),
        ),
      );
      final request = GuestMutationStartRequest(
        selection: MutationStartRequest(
          submission: _token(),
          packageId: package.id,
          packageDigest: package.digest,
          registryRevision: widget.registryRevision!,
          disposition: _draft.disposition,
          selectedPath: selectedPath,
          relativePath: relativePath,
          subject: guestMutationExecutionSubject,
          approvalSha256: Uint8List.fromList(approval.bytes),
          timeoutMs: 30000,
        ),
        approvedBudget: budget,
      );
      GuestMutationValidation.start(request);
      final draft = _Review(
        start: request,
        packageName: package.name,
        target: target,
        operationId: operationId,
        length: content.length,
        contentHash: contentHash,
        planHash: Uint8List(0),
      );
      final confirmed = await _confirm(draft, step: 'draft');
      if (confirmed != true ||
          !mounted ||
          epoch != _epoch ||
          !identical(backend, widget.backend) ||
          !_session.canStart ||
          _selectedPackage() != package ||
          widget.registryRevision != request.selection.registryRevision) {
        return;
      }
      setState(() => _reviewing = false);
      await _session.review(
        request,
        operationId: operationId,
        content: _draft.disposition == MutationDisposition.create
            ? content
            : null,
      );
      if (!mounted || epoch != _epoch || !identical(backend, widget.backend)) {
        return;
      }
      final planHash = _session.planSha256;
      if (planHash == null) return;
      setState(
        () => _draft.review = _Review(
          start: request,
          packageName: package.name,
          target: target,
          operationId: operationId,
          length: content.length,
          contentHash: contentHash,
          planHash: Uint8List.fromList(planHash),
        ),
      );
    } on FormatException catch (error) {
      if (mounted && epoch == _epoch) {
        setState(() => _formError = error.message);
      }
    } catch (_) {
      if (mounted && epoch == _epoch) setState(() => _formError = 'operation');
    } finally {
      if (mounted && epoch == _epoch) {
        setState(() => _reviewing = false);
        _schedule();
      }
    }
  }

  Future<bool> _refreshReview(
    _Review review,
    int epoch,
    GuestMutationExecutionSession session,
  ) async {
    bool current() =>
        mounted &&
        epoch == _epoch &&
        identical(session, _session) &&
        identical(_draft.review, review) &&
        _reviewCurrent(review) &&
        listEquals(session.planSha256, review.planHash);
    if (!current()) return false;
    // A dialog is not an authorization snapshot. Observe the original owner
    // again before using the exact reviewed plan; this never renews its lease.
    await session.refresh();
    return current();
  }

  Future<void> _prepare() async {
    final review = _draft.review;
    if (review == null ||
        !_reviewCurrent(review) ||
        !_session.canPrepare ||
        !listEquals(_session.planSha256, review.planHash)) {
      return;
    }
    final epoch = _epoch, backend = widget.backend, session = _session;
    _timer?.cancel();
    setState(() => _reviewing = true);
    try {
      final confirmed = await _confirm(review, step: 'prepare');
      if (confirmed != true ||
          !mounted ||
          epoch != _epoch ||
          !identical(backend, widget.backend) ||
          !await _refreshReview(review, epoch, session) ||
          !identical(_draft.review, review) ||
          !_reviewCurrent(review) ||
          !_session.canPrepare ||
          !listEquals(_session.planSha256, review.planHash)) {
        return;
      }
      review.prepareApproved = true;
      setState(() => _reviewing = false);
      await _run(() => _session.prepare(reviewedPlanSha256: review.planHash));
    } finally {
      if (mounted && epoch == _epoch) {
        setState(() => _reviewing = false);
        _schedule();
      }
    }
  }

  Future<void> _execute() async {
    final review = _draft.review;
    if (review == null ||
        !_reviewCurrent(review) ||
        !review.prepareApproved ||
        !_session.canExecute ||
        !listEquals(_session.planSha256, review.planHash)) {
      return;
    }
    final epoch = _epoch, backend = widget.backend, session = _session;
    _timer?.cancel();
    setState(() => _reviewing = true);
    try {
      final confirmed = await _confirm(review, step: 'execute');
      if (confirmed != true ||
          !mounted ||
          epoch != _epoch ||
          !identical(backend, widget.backend) ||
          !await _refreshReview(review, epoch, session) ||
          !identical(_draft.review, review) ||
          !_reviewCurrent(review) ||
          !_session.canExecute ||
          !listEquals(_session.planSha256, review.planHash)) {
        return;
      }
      setState(() => _reviewing = false);
      await _run(() => _session.execute(reviewedPlanSha256: review.planHash));
    } finally {
      if (mounted && epoch == _epoch) {
        setState(() => _reviewing = false);
        _schedule();
      }
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
      key: ValueKey('guest-mutation-$id'),
      title: Text(title, style: TextStyle(color: widget.muted, fontSize: 12)),
      children: [SelectableText(bounded)],
    );
  }

  String _phaseLabel(AppLocalizations l, GuestMutationFramePhase phase) =>
      switch (phase) {
        GuestMutationFramePhase.absent => l.pluginsMutationAbsent,
        GuestMutationFramePhase.prepared => l.pluginsMutationPrepared,
        GuestMutationFramePhase.outcomeUnknown => l.pluginsMutationUnknown,
        GuestMutationFramePhase.observed => l.pluginsMutationObserved,
        GuestMutationFramePhase.cancelledBeforeDispatch =>
          l.pluginsMutationCancelled,
        GuestMutationFramePhase.none => l.pluginsMutationUnspecified,
      };

  String _effectLabel(AppLocalizations l, GuestMutationFrameEffect effect) =>
      switch (effect) {
        GuestMutationFrameEffect.osSucceeded => l.pluginsMutationSucceeded,
        GuestMutationFrameEffect.osRejected => l.pluginsMutationRejected,
        GuestMutationFrameEffect.unspecified => l.pluginsMutationUnspecified,
      };

  String _frameLabel(AppLocalizations l, GuestMutationFrame frame) =>
      frame.status == GuestMutationFrameStatus.outcomeUnknown
      ? l.pluginsMutationUnknown
      : frame.status != GuestMutationFrameStatus.completed
      ? l.pluginsFileMutationFailure
      : '${_phaseLabel(l, frame.phase)} · ${_effectLabel(l, frame.effect)}';

  String _ownerLabel(AppLocalizations l, MutationResult owner) {
    if (owner.kind == MutationResultKind.failure) {
      return l.pluginsFileMutationFailure;
    }
    if (owner.phase == MutationPhase.observed &&
        owner.effect != MutationEffect.unspecified) {
      return '${l.pluginsMutationObserved} · ${owner.effect == MutationEffect.osSucceeded ? l.pluginsMutationSucceeded : l.pluginsMutationRejected}';
    }
    return switch (owner.phase) {
      MutationPhase.prepared => l.pluginsMutationPrepared,
      MutationPhase.outcomeUnknown => l.pluginsMutationUnknown,
      MutationPhase.observed => l.pluginsMutationObserved,
      MutationPhase.cancelledBeforeDispatch => l.pluginsMutationCancelled,
      MutationPhase.none
          when owner.kind == MutationResultKind.history &&
              owner.record == null =>
        l.pluginsMutationAbsent,
      MutationPhase.none =>
        owner.kind == MutationResultKind.selected
            ? l.pluginsMutationSelected
            : owner.kind == MutationResultKind.planned
            ? l.pluginsMutationPlan
            : l.pluginsMutationUnspecified,
    };
  }

  Widget _button(String label, String id, Future<void> Function()? action) =>
      OutlinedButton(
        key: ValueKey('guest-mutation-$id'),
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
    final package = _selectedPackage();
    final review = _draft.review;
    final frame = _session.outcome;
    final result = _session.lastResult;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          l.pluginsGuestExecutionTitle,
          style: TextStyle(color: widget.ink, fontWeight: FontWeight.w600),
        ),
        _note(l.pluginsGuestExecutionIntro),
        _note(l.pluginsGuestExecutionDeadline),
        if (_session.authorizationExpired)
          _note(l.pluginsGuestExecutionExpired),
        if (widget.registryRevision == null)
          _note(l.pluginsHttpTaskCatalogUnavailable),
        DropdownButton<MutationDisposition>(
          key: const ValueKey('guest-mutation-disposition'),
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
                  _invalidateDraft();
                  _draft.disposition = value;
                  _draft.package = null;
                  _draft.directory = null;
                  _draft.file = null;
                })
              : null,
        ),
        DropdownButton<String>(
          key: const ValueKey('guest-mutation-package'),
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
              ? (value) => setState(() {
                  _invalidateDraft();
                  _draft.package = value;
                  _draft.jobBudget = '';
                  _draft.totalBudget = '';
                  _jobBudget.clear();
                  _totalBudget.clear();
                })
              : null,
        ),
        if (package?.mutationBudget != null) ...[
          _note(
            l.pluginsGuestMutationBudget(
              package!.mutationBudget!.maxJobBytes.toString(),
              package.mutationBudget!.maxBytes.toString(),
            ),
          ),
          TextField(
            key: const ValueKey('guest-mutation-job-budget'),
            controller: _jobBudget,
            enabled: _canEdit,
            keyboardType: TextInputType.number,
            decoration: InputDecoration(
              labelText: l.pluginsGuestExecutionJobBudget,
            ),
            onChanged: (value) {
              _draft.jobBudget = value;
              _invalidateDraft();
            },
          ),
          TextField(
            key: const ValueKey('guest-mutation-total-budget'),
            controller: _totalBudget,
            enabled: _canEdit,
            keyboardType: TextInputType.number,
            decoration: InputDecoration(
              labelText: l.pluginsGuestExecutionTotalBudget,
            ),
            onChanged: (value) {
              _draft.totalBudget = value;
              _invalidateDraft();
            },
          ),
        ] else if (package != null)
          _note(l.pluginsGuestExecutionOrdinaryBudget),
        Wrap(
          spacing: 8,
          runSpacing: 8,
          children: [
            _button(
              _draft.disposition == MutationDisposition.create
                  ? l.pluginsFileMutationPickDirectory
                  : l.pluginsFileMutationPickFile,
              'pick-target',
              _canEdit && package != null ? _pickTarget : null,
            ),
            if (_draft.disposition == MutationDisposition.create)
              _button(
                l.pluginsFileMutationPickContent,
                'pick-content',
                _canEdit && package != null ? _pickSource : null,
              ),
            if (_draft.disposition == MutationDisposition.create &&
                _draft.content != null)
              _button(
                l.pluginsFileMutationEmptyContent,
                'empty-content',
                _canEdit
                    ? () async => setState(() {
                        _invalidateDraft();
                        _draft.content = null;
                        _draft.contentName = null;
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
            key: const ValueKey('guest-mutation-file-name'),
            controller: _name,
            enabled: _canEdit,
            maxLength: 255,
            decoration: InputDecoration(
              labelText: l.pluginsFileMutationFileName,
            ),
            onChanged: (value) {
              _draft.name = value;
              _invalidateDraft();
            },
          ),
        Wrap(
          spacing: 8,
          runSpacing: 8,
          children: [
            _button(
              l.pluginsGuestExecutionDraft,
              'draft',
              _canEdit && package != null ? _review : null,
            ),
            if (_reviewing)
              _button(l.pluginsCancel, 'cancel-review', () async {
                _cancelReview();
              }),
            _button(
              l.pluginsFileMutationPrepareConfirm,
              'prepare',
              !_reviewing &&
                      review != null &&
                      _reviewCurrent(review) &&
                      _session.canPrepare
                  ? _prepare
                  : null,
            ),
            _button(
              l.pluginsFileMutationExecute,
              'execute',
              !_reviewing &&
                      review != null &&
                      _reviewCurrent(review) &&
                      review.prepareApproved &&
                      _session.canExecute
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
              l.pluginsGuestExecutionHostQuery,
              'host-query',
              _session.canHostQuery ? () => _run(_session.hostQuery) : null,
            ),
            _button(
              l.pluginsFileMutationCancelPlan,
              'cancel-plan',
              _session.canCancelPlan ? () => _run(_session.cancelPlan) : null,
            ),
            _button(
              l.pluginsGuestExecutionRelease,
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
            'Invalid approved guest budget' =>
              l.pluginsGuestExecutionBudgetError,
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
        if (_session.uncertain) _note(l.pluginsGuestExecutionNoReplay),
        if (_session.snapshot?.key != null && !_ownsTask)
          _note(l.pluginsFileTaskOtherSession),
        if (review != null) ...[
          _note('${l.pluginsFileMutationTarget}: ${review.target}'),
          _note('${l.pluginsEndpointPackage}: ${review.packageName}'),
          SelectableText(_hex(review.start.selection.packageDigest)),
          _note('${l.pluginsFileMutationContentLength}: ${review.length}'),
          if (review.contentHash != null)
            SelectableText(
              '${l.pluginsFileMutationContentSha256}: ${_hex(review.contentHash!)}',
            ),
          SelectableText(
            '${l.pluginsGuestExecutionPlanSha256}: ${_hex(review.planHash)}',
          ),
        ],
        if (result?.owner != null && !identical(result, _session.history))
          _note(
            '${l.pluginsGuestExecutionOwner}: ${_ownerLabel(l, result!.owner!)}',
          ),
        if (result?.frame != null &&
            !identical(result!.frame, frame) &&
            !identical(result, _session.history))
          _note(
            '${l.pluginsGuestExecutionFrame}: ${_frameLabel(l, result.frame!)}',
          ),
        if (frame != null) ...[
          _note('${l.pluginsGuestExecutionFrame}: ${_frameLabel(l, frame)}'),
          if (frame.status != GuestMutationFrameStatus.completed)
            _diagnostic(
              'frame-code',
              l.pluginsFileMutationOperationError,
              '${frame.status.name} / ${frame.phase.name} / ${frame.effect.name}',
            ),
        ],
        if (_session.history?.frame != null)
          _note(
            '${l.pluginsMutationLastResult}: ${_frameLabel(l, _session.history!.frame!)}',
          ),
        if (_session.history?.owner != null)
          _note(
            '${l.pluginsMutationLastResult}: ${_ownerLabel(l, _session.history!.owner!)}',
          ),
        if (result?.failure != null)
          _diagnostic(
            'result-code',
            l.pluginsFileMutationOperationError,
            '${result!.failure!.kind.name} / ${result.failure!.job?.name ?? result.failure!.fault?.name ?? ''}',
          ),
        if (widget.onOpenRecovery != null &&
            _session.canStart &&
            _session.request != null &&
            (review != null ||
                _session.error != null ||
                frame?.phase == GuestMutationFramePhase.outcomeUnknown)) ...[
          _note(l.pluginsFileMutationRecoveryHint),
          _button(l.pluginsFileMutationRecovery, 'recovery', () async {
            widget.onOpenRecovery?.call();
          }),
        ],
      ],
    );
  }
}
