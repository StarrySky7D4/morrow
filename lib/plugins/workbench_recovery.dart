import 'package:flutter/material.dart';
import 'host_notices.dart';
import 'workbench_supervision.dart';
import 'workbench_owner_management.dart';
import 'session_coordinator.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:morrow_i18n/morrow_i18n.dart';

String workbenchFailureMessage(
  Object error, {
  String? fallback,
  AppLocalizations? localization,
}) {
  final l = localization ?? L10n.forLocale(const Locale('zh'));
  if (error is RecoverySwitchUnconfirmed) {
    return l.recoverySwitchUnconfirmed(error.path);
  }
  if (error is WorkbenchSupervisionUnconfirmed) {
    return l.localeName.startsWith('zh')
        ? '内容服务已停止，操作结果未知。原库所有权尚未确认释放，请保留原库与诊断记录；不会自动重试或清库解锁。'
        : 'Content service stopped. Operation outcomes are unknown. The original library owner is retained; keep the library and diagnostic records. No automatic retry or unlock.';
  }
  const prefix = 'Morrow workbench host: ';
  final detail = error is StateError ? error.message.toString() : '';
  final plain = detail.startsWith(prefix)
      ? detail.substring(prefix.length)
      : detail;
  return hostStorageNotice(l, plain) ??
      (detail.startsWith(prefix) ? plain : fallback ?? l.recoveryOpenFailed);
}

final class RecoverySwitchUnconfirmed implements Exception {
  const RecoverySwitchUnconfirmed(this.path);
  final String path;
}

/// Recovery actions are host supplied; this view never reads protection files.
class WorkbenchRecovery extends StatefulWidget {
  const WorkbenchRecovery({
    super.key,
    this.message,
    this.failure,
    this.locale,
    required this.onRetry,
    this.onRestore,
    this.onRestoreSnapshot,
    this.session,
    this.onPreviewOwner,
    this.onRecoverOwner,
  });
  final Future<WorkbenchOwnerPreview> Function()? onPreviewOwner;
  final Future<WorkbenchOwnerRecovery> Function(WorkbenchOwnerPreview)?
  onRecoverOwner;
  final String? message;
  final Object? failure;
  final Locale? locale;
  final SessionCoordinator? session;
  final Future<void> Function() onRetry;
  final Future<void> Function()? onRestore;
  final Future<void> Function()? onRestoreSnapshot;
  @override
  State<WorkbenchRecovery> createState() => _WorkbenchRecoveryState();
}

class _WorkbenchRecoveryState extends State<WorkbenchRecovery> {
  @override
  void initState() {
    super.initState();
    widget.session?.addListener(_sessionChanged);
  }

  @override
  void didUpdateWidget(WorkbenchRecovery oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.session != widget.session) {
      oldWidget.session?.removeListener(_sessionChanged);
      widget.session?.addListener(_sessionChanged);
      _ownerPreview = null;
      _previewGeneration = null;
      _resourceRecovered = false;
      _error = null;
    }
  }

  void _sessionChanged() {
    if (mounted) {
      setState(() {
        if (_previewGeneration != null &&
            _previewGeneration != widget.session?.generation) {
          _ownerPreview = null;
          _resourceRecovered = false;
          _error = const WorkbenchOwnerManagementUnconfirmed(
            'stale_owner_preview',
          );
        }
      });
    }
  }

  @override
  void dispose() {
    widget.session?.removeListener(_sessionChanged);
    super.dispose();
  }

  bool get _blocked => _busy || !(widget.session?.mayRecover ?? true);
  bool _busy = false;
  bool _confirming = false;
  WorkbenchOwnerPreview? _ownerPreview;
  bool _resourceRecovered = false;
  int? _previewGeneration;
  bool get _transportEnded =>
      widget.session?.owner == null ||
      widget.session?.transportExitCode != null;
  Object? _error;
  String _words(BuildContext context, String zh, String en) =>
      L10n.of(context).localeName.startsWith('zh') ? zh : en;
  Future<void> _previewOwner() async {
    if (_busy) return;
    setState(() {
      _busy = true;
      _error = null;
      _ownerPreview = null;
    });
    final session = widget.session;
    final generation = session?.generation;
    try {
      final preview = await widget.onPreviewOwner!();
      if (mounted) {
        if (!identical(session, widget.session) ||
            generation != widget.session?.generation) {
          throw const WorkbenchOwnerManagementUnconfirmed(
            'stale_owner_preview',
          );
        }
        setState(() {
          _ownerPreview = preview;
          _previewGeneration = generation;
          _resourceRecovered = preview.confirmedRecovery != null;
        });
      }
    } catch (e) {
      if (mounted) setState(() => _error = e);
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _recoverOwner(BuildContext context) async {
    final preview = _ownerPreview;
    if (_busy ||
        preview?.eligible != true ||
        widget.onRecoverOwner == null ||
        !_transportEnded) {
      return;
    }
    final session = widget.session;
    final generation = _previewGeneration;
    setState(() {
      _busy = true;
      _confirming = true;
    });
    var acknowledged = false;
    final approved = await showDialog<bool>(
      context: context,
      builder: (dialog) => StatefulBuilder(
        builder: (dialog, setDialogState) => AlertDialog(
          title: Text(_words(context, '确认资源恢复', 'Confirm resource recovery')),
          content: CheckboxListTile(
            value: acknowledged,
            onChanged: (v) => setDialogState(() => acknowledged = v == true),
            title: Text(
              _words(
                context,
                '资源回收已有完整证明。原操作结果仍为未知；此操作保留历史，不重放业务。',
                'Complete resource cleanup has been proved. Original operation outcomes remain Unknown. History is retained and no operation is replayed.',
              ),
            ),
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(dialog, false),
              child: Text(MaterialLocalizations.of(context).cancelButtonLabel),
            ),
            FilledButton(
              key: const ValueKey('confirm-owner-recovery'),
              onPressed: acknowledged
                  ? () => Navigator.pop(dialog, true)
                  : null,
              child: Text(
                _words(context, '保留未知结果并恢复', 'Recover; retain Unknown'),
              ),
            ),
          ],
        ),
      ),
    );
    if (!mounted) return;
    setState(() => _confirming = false);
    if (approved != true) {
      setState(() => _busy = false);
      return;
    }
    if (!identical(session, widget.session) ||
        generation != widget.session?.generation ||
        !identical(preview, _ownerPreview) ||
        !_transportEnded) {
      setState(() {
        _busy = false;
        _ownerPreview = null;
        _error = const WorkbenchOwnerManagementUnconfirmed(
          'stale_owner_preview',
        );
      });
      return;
    }
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      final result = await widget.onRecoverOwner!(preview!);
      if (mounted) {
        setState(() {
          _resourceRecovered = result.recovered;
          _ownerPreview = null;
        });
      }
    } catch (e) {
      if (mounted) {
        setState(() {
          _error = e;
          _ownerPreview = null;
        });
      }
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  String _ownerReason(BuildContext context, String reason) => switch (reason) {
    'eligible_resource_only' => _words(
      context,
      '原进程已结束，完整资源回收证明已保存。可以显式恢复；操作结果仍未知。',
      'The original process ended and complete resource proof is durable. Explicit recovery is available; outcomes remain Unknown.',
    ),
    'original_resource_proof_missing' => _words(
      context,
      '缺少完整原始资源证明，无法安全恢复。进程消失或超时不能补证，请保留原库和诊断记录。',
      'Complete original resource proof is missing. Process absence or timeout cannot replace it. Preserve this library and diagnostics.',
    ),
    'legacy_identity_unprovable' => _words(
      context,
      '旧版记录缺少可信进程代次，无法补证恢复。请保留原库及证据。',
      'This legacy record lacks trusted process creation identity. Recovery cannot be proved; preserve the library and evidence.',
    ),
    'original_supervisor_still_live' => _words(
      context,
      '原 supervisor 仍存活，拒绝第二个 owner。请等待原会话完成清理。',
      'The original supervisor is still live. Another owner is refused; wait for its cleanup.',
    ),
    'artifact_binding_mismatch' => _words(
      context,
      '当前程序或插件与原始证明不一致，拒绝恢复。',
      'Current executable or plugin artifacts differ from the original proof. Recovery is refused.',
    ),
    'already_resource_recovered' => _words(
      context,
      '资源恢复已确认；原操作结果仍未知。关闭此窗口后可以手动重新打开。',
      'Resource recovery is already confirmed; outcomes remain Unknown. Select Retry to open a new session.',
    ),
    'ui_transport_still_live' => _words(
      context,
      '原 UI 传输进程尚未确认退出。请等待退出后再次检查资源恢复。',
      'The original UI transport has not exited. Wait for its exit, then check resource recovery again.',
    ),
    'stale_owner_preview' => _words(
      context,
      '预览已过期或属于另一会话。请重新检查资源恢复，再确认。',
      'This preview is stale or belongs to another session. Check resource recovery again before confirming.',
    ),
    'unconfirmed_or_invalid' || 'recovery_result_unconfirmed' => _words(
      context,
      '恢复提交或输出结果尚未确认。请再次检查资源恢复；原操作结果仍为 Unknown，不会自动重试。',
      'The recovery commit or output is unconfirmed. Check resource recovery again. Original outcomes remain Unknown; no automatic retry occurs.',
    ),
    'already_normal_released' => _words(
      context,
      '原会话已正常释放。可以手动重试打开。',
      'The original session was normally released. You may manually retry opening.',
    ),
    _ => _words(
      context,
      '恢复条件未确认。请保留原库和诊断记录。',
      'Recovery conditions are unconfirmed. Preserve the library and diagnostics.',
    ),
  };
  Future<void> _run(Future<void> Function() action) async {
    if (_blocked) return;
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      await action();
    } catch (error) {
      if (mounted) {
        setState(() {
          _error = error;
        });
      }
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final colors = ColorScheme.fromSeed(seedColor: const Color(0xff7468bc));
    return MaterialApp(
      locale: widget.locale,
      supportedLocales: L10n.supportedLocales,
      localizationsDelegates: const [
        L10n.delegate,
        GlobalMaterialLocalizations.delegate,
        GlobalWidgetsLocalizations.delegate,
        GlobalCupertinoLocalizations.delegate,
      ],
      theme: ThemeData(
        useMaterial3: true,
        colorScheme: colors,
        scaffoldBackgroundColor: const Color(0xfff5f4fa),
      ),
      home: Builder(
        builder: (context) {
          final l = L10n.of(context);
          return Scaffold(
            body: Center(
              child: SingleChildScrollView(
                padding: const EdgeInsets.all(24),
                child: ConstrainedBox(
                  constraints: const BoxConstraints(maxWidth: 440),
                  child: DecoratedBox(
                    decoration: BoxDecoration(
                      color: Colors.white.withValues(alpha: .88),
                      borderRadius: BorderRadius.circular(28),
                      border: Border.all(color: Colors.white),
                      boxShadow: [
                        BoxShadow(
                          color: colors.primary.withValues(alpha: .08),
                          blurRadius: 30,
                          offset: const Offset(0, 10),
                        ),
                      ],
                    ),
                    child: Padding(
                      padding: const EdgeInsets.all(28),
                      child: Column(
                        mainAxisSize: MainAxisSize.min,
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Icon(
                            Icons.lock_reset_rounded,
                            size: 36,
                            color: colors.primary,
                          ),
                          const SizedBox(height: 16),
                          Text(
                            l.recoveryTitle,
                            style: TextStyle(
                              fontSize: 24,
                              fontWeight: FontWeight.w600,
                            ),
                          ),
                          const SizedBox(height: 12),
                          if (widget.session?.waitingForExit ?? false) ...[
                            Text(
                              widget.session!.phase ==
                                      SessionPhase.closingUnconfirmed
                                  ? l.recoveryClosingUnconfirmed
                                  : l.recoveryClosing,
                            ),
                            const SizedBox(height: 12),
                          ],
                          Text(
                            widget.message ??
                                workbenchFailureMessage(
                                  widget.failure ?? StateError('startup'),
                                  localization: l,
                                ),
                            style: const TextStyle(height: 1.6),
                          ),
                          if (widget.onRestoreSnapshot != null) ...[
                            const SizedBox(height: 12),
                            Text(
                              l.recoverySnapshotGuide,
                              style: TextStyle(fontSize: 12, height: 1.6),
                            ),
                            const SizedBox(height: 10),
                            OutlinedButton.icon(
                              key: const ValueKey('restore-library-snapshot'),
                              onPressed: _blocked
                                  ? null
                                  : () => _run(widget.onRestoreSnapshot!),
                              icon: const Icon(
                                Icons.inventory_2_outlined,
                                size: 18,
                              ),
                              label: Text(l.recoverySnapshot),
                            ),
                          ],
                          if (widget.onRestore != null) ...[
                            const SizedBox(height: 12),
                            Text(
                              l.recoveryKeyGuide,
                              style: TextStyle(
                                height: 1.6,
                                color: Color(0xff686578),
                              ),
                            ),
                          ],
                          if (widget.onPreviewOwner != null) ...[
                            const SizedBox(height: 16),
                            OutlinedButton(
                              key: const ValueKey('preview-owner-recovery'),
                              onPressed: _busy ? null : _previewOwner,
                              child: Text(
                                _words(
                                  context,
                                  '检查资源恢复条件',
                                  'Check resource recovery',
                                ),
                              ),
                            ),
                            if (_ownerPreview case final preview?) ...[
                              Text(_ownerReason(context, preview.reason)),
                              if ((preview.eligible || _resourceRecovered) &&
                                  !_transportEnded)
                                Text(
                                  _ownerReason(
                                    context,
                                    'ui_transport_still_live',
                                  ),
                                ),
                              if (preview.eligible &&
                                  widget.onRecoverOwner != null)
                                OutlinedButton(
                                  key: const ValueKey('recover-owner'),
                                  onPressed: _busy || !_transportEnded
                                      ? null
                                      : () => _recoverOwner(context),
                                  child: Text(
                                    _words(
                                      context,
                                      '显式恢复资源',
                                      'Recover resources explicitly',
                                    ),
                                  ),
                                ),
                            ],
                            if (_resourceRecovered)
                              Text(
                                _words(
                                  context,
                                  '资源恢复已确认，原操作结果仍未知。点击重试以打开新会话。',
                                  'Resource recovery is confirmed; original outcomes remain Unknown. Select Retry to open a new session.',
                                ),
                              ),
                          ],
                          if (_error != null) ...[
                            const SizedBox(height: 16),
                            Text(
                              _error is WorkbenchOwnerManagementUnconfirmed
                                  ? _ownerReason(
                                      context,
                                      (_error
                                              as WorkbenchOwnerManagementUnconfirmed)
                                          .reason,
                                    )
                                  : workbenchFailureMessage(
                                      _error!,
                                      fallback: widget.onPreviewOwner != null
                                          ? _error.toString()
                                          : l.recoveryFailed,
                                      localization: l,
                                    ),
                              style: TextStyle(
                                color: colors.error,
                                height: 1.5,
                              ),
                            ),
                          ],
                          const SizedBox(height: 24),
                          if (_busy && !_confirming)
                            const Padding(
                              padding: EdgeInsets.only(bottom: 16),
                              child: LinearProgressIndicator(),
                            ),
                          Wrap(
                            spacing: 12,
                            runSpacing: 12,
                            children: [
                              FilledButton.tonal(
                                key: const ValueKey('retry-workbench'),
                                onPressed: _blocked
                                    ? null
                                    : () => _run(widget.onRetry),
                                child: Text(l.recoveryRetry),
                              ),
                              if (widget.onRestore != null)
                                OutlinedButton.icon(
                                  onPressed: _blocked
                                      ? null
                                      : () => _run(widget.onRestore!),
                                  icon: const Icon(Icons.key_rounded, size: 18),
                                  label: Text(l.recoveryChooseKey),
                                ),
                            ],
                          ),
                        ],
                      ),
                    ),
                  ),
                ),
              ),
            ),
          );
        },
      ),
    );
  }
}
