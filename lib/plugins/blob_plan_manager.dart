import 'dart:async';
import 'dart:convert';
import 'dart:math';
import 'dart:typed_data';
import 'package:file_selector/file_selector.dart';
import 'package:flutter/material.dart';
import 'blob_plan_models.dart';
import 'blob_plan_session.dart';
import 'blob_upload_models.dart';
import 'blob_upload_session.dart';
import 'io_task_models.dart';
import 'plugin_library.dart';

/// The trusted review surface. Plugin strings are plain text; only the original
/// retained request/proposal can reach the native single-use approval control.
final class BlobPlanManager extends StatefulWidget {
  const BlobPlanManager({
    super.key,
    required this.plans,
    required this.uploads,
    required this.io,
    this.entry,
    this.registryRevision,
    this.pickInput,
  });
  final BlobPlanBackend plans;
  final BlobUploadBackend uploads;
  final WorkbenchIoTaskControl io;
  final PluginLibraryEntry? entry;
  final BigInt? registryRevision;
  final Future<XFile?> Function()? pickInput;
  @override
  State<BlobPlanManager> createState() => _BlobPlanManagerState();
}

final class _BlobPlanManagerState extends State<BlobPlanManager> {
  late BlobPlanSession plans;
  late BlobUploadSession uploads;
  late Object planView;
  Object? uploadView;
  final text = TextEditingController();
  Uint8List? fileBytes;
  String? fileName, selected, notice;
  bool working = false, reviewed = false;
  int generation = 0;
  Timer? timer;

  String _s(String zh, String en) =>
      Localizations.localeOf(context).languageCode == 'zh' ? zh : en;
  List<PluginTransformHandler> get handlers {
    final entry = widget.entry;
    if (entry == null) return const [];
    return [
      for (final mapping in entry.blobPlanners)
        for (final handler in entry.handlers)
          if (handler.name == mapping.handler) handler,
    ];
  }

  PluginTransformHandler? get handler =>
      handlers.where((h) => h.name == selected).firstOrNull;
  bool get eligible =>
      widget.entry?.enabled == true &&
      widget.entry?.available == true &&
      widget.entry?.blobUploadApproved == true &&
      widget.registryRevision != null &&
      handler != null;
  bool get busy => working || plans.busy || uploads.busy;
  bool get uploadClosed {
    final key = plans.snapshot?.uploadKey;
    return key != null &&
        uploads.history.any((r) => _same(r.snapshot.task.key, key));
  }

  static bool _same(Uint8List? a, Uint8List? b) =>
      a != null &&
      b != null &&
      a.length == b.length &&
      Iterable<int>.generate(a.length).every((i) => a[i] == b[i]);

  @override
  void initState() {
    super.initState();
    _bind();
  }

  void _bind() {
    plans = BlobPlanSession.forBackend(widget.plans, widget.io);
    uploads = BlobUploadSession.forBackend(widget.uploads, widget.io);
    planView = plans.attachView();
    selected = handlers.firstOrNull?.name;
    unawaited(_action(_refresh));
  }

  void _detach() {
    timer?.cancel();
    timer = null;
    // The planning session owns cancellation of the one original upload. The
    // upload view is detached first, with no second cancellation from this UI.
    final attached = uploadView;
    uploadView = null;
    if (attached != null) {
      unawaited(uploads.detachView(attached, cancelOriginal: false));
    }
    final finishSent =
        plans.snapshot?.consumed == true &&
        (uploadClosed ||
            (uploads.acknowledgementUncertain &&
                _same(uploads.originalKey, plans.snapshot?.uploadKey)));
    unawaited(plans.detachView(planView, cancelOriginal: !finishSent));
  }

  @override
  void didUpdateWidget(covariant BlobPlanManager oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (!identical(oldWidget.plans, widget.plans) ||
        !identical(oldWidget.uploads, widget.uploads) ||
        !identical(oldWidget.io, widget.io)) {
      generation++;
      _detach();
      working = false;
      reviewed = false;
      notice = null;
      fileBytes = null;
      fileName = null;
      text.clear();
      _bind();
    } else if (!identical(oldWidget.entry, widget.entry) ||
        oldWidget.registryRevision != widget.registryRevision) {
      reviewed = false;
      if (plans.request == null) {
        selected = handlers.firstOrNull?.name;
        fileBytes = null;
        fileName = null;
      }
    }
  }

  @override
  void dispose() {
    generation++;
    _detach();
    text.dispose();
    super.dispose();
  }

  Future<void> _action(Future<void> Function() operation) async {
    if (busy) return;
    final epoch = generation;
    setState(() {
      working = true;
      notice = null;
    });
    try {
      await operation();
    } catch (_) {
      if (mounted && generation == epoch) notice = 'observation';
    } finally {
      if (mounted && generation == epoch) {
        setState(() => working = false);
        _schedule();
      }
    }
  }

  void _schedule() {
    timer?.cancel();
    timer = null;
    // Poll only a positively identified original worker that is still running.
    // Missing replies, reads, approvals and acknowledgements are never retried.
    if (!mounted || busy || plans.failure != null || uploads.failure != null) {
      return;
    }
    if (plans.snapshot?.running == true ||
        (uploadView != null &&
            !uploadClosed &&
            uploads.snapshot?.task.exit == null &&
            uploads.snapshot?.task.key != null)) {
      timer = Timer(const Duration(milliseconds: 500), () {
        if (mounted) unawaited(_action(_refresh));
      });
    }
  }

  Future<void> _refresh() async {
    final originalPlans = plans, originalUploads = uploads;
    final view = planView, epoch = generation;
    await originalPlans.refresh(view);
    if (!mounted || generation != epoch) return;
    final state = originalPlans.snapshot;
    if (state?.uploadKey == null ||
        state?.uploadSubmission == null ||
        uploadClosed) {
      return;
    }
    if (originalUploads.submission != null &&
        (!_same(originalUploads.submission, state!.uploadSubmission) ||
            !_same(originalUploads.originalKey, state.uploadKey))) {
      notice = 'different-upload';
      return;
    }
    final attached = uploadView ??= originalUploads.attachView();
    await originalUploads.adoptOriginal(
      attached,
      state!.uploadKey!,
      state.uploadSubmission!,
    );
  }

  Future<void> _chooseFile() => _action(() async {
    final epoch = generation, originalHandler = handler;
    final file = await (widget.pickInput ?? openFile)();
    if (!mounted ||
        epoch != generation ||
        file == null ||
        originalHandler != handler ||
        plans.request != null) {
      return;
    }
    final limit = min(handler!.maxInputBytes, 32768);
    final size = await file.length();
    if (!mounted || epoch != generation) return;
    if (size > limit) {
      notice = 'too-large';
      return;
    }
    // A bounded stream also guards a file which grows after its size check.
    final bytes = BytesBuilder(copy: true);
    await for (final chunk in file.openRead()) {
      if (!mounted || epoch != generation) return;
      if (bytes.length + chunk.length > limit) {
        notice = 'too-large';
        return;
      }
      bytes.add(chunk);
    }
    if (!mounted ||
        epoch != generation ||
        originalHandler != handler ||
        plans.request != null) {
      return;
    }
    fileBytes = bytes.takeBytes().asUnmodifiableView();
    fileName = file.name;
  });

  Future<void> _start() => _action(() async {
    if (!eligible || !plans.canStart) return;
    final selectedHandler = handler!;
    final input = selectedHandler.inputType == 'text.utf8'
        ? Uint8List.fromList(utf8.encode(text.text))
        : fileBytes;
    if (input == null) {
      notice = 'input-required';
      return;
    }
    if (input.length > min(selectedHandler.maxInputBytes, 32768)) {
      notice = 'too-large';
      return;
    }
    final random = Random.secure();
    final submission = Uint8List.fromList(
      List.generate(32, (_) => random.nextInt(256)),
    );
    if (submission.every((b) => b == 0)) {
      throw StateError('No attempt identity');
    }
    reviewed = false;
    await plans.start(
      planView,
      BlobPlanRequest(
        submission: submission,
        packageId: widget.entry!.id,
        packageDigest: widget.entry!.digest,
        registryRevision: widget.registryRevision!,
        handler: selectedHandler.name,
        inputType: selectedHandler.inputType,
        input: input,
      ),
    );
  });

  Future<void> _approve() => _action(() async {
    if (!reviewed || !plans.canApprove) return;
    final original = plans, view = planView, epoch = generation;
    reviewed = false;
    await original.approve(view);
    if (mounted && epoch == generation) await _refresh();
  });

  Future<void> _finish() => _action(() async {
    final originalPlans = plans, originalUploads = uploads;
    final view = planView, attached = uploadView, epoch = generation;
    if (originalPlans.snapshot?.uploadKey != null && !uploadClosed) {
      if (attached == null ||
          !originalUploads.canAcknowledge ||
          !await originalUploads.acknowledge(attached)) {
        return;
      }
    }
    if (!mounted || generation != epoch) return;
    if (await originalPlans.acknowledge(view)) {
      reviewed = false;
      if (attached != null) await originalUploads.detachView(attached);
      uploadView = null;
    }
  });

  Widget _button(
    String key,
    String label,
    bool enabled,
    Future<void> Function() action,
  ) => OutlinedButton(
    key: ValueKey(key),
    onPressed: busy || !enabled ? null : action,
    child: Text(label),
  );
  String _observation(BlobUploadObservation value) => switch (value) {
    BlobUploadObservation.absent => _s(
      '原记录中没有已保存内容',
      'Original history contains no saved content',
    ),
    BlobUploadObservation.staged => _s(
      '原记录中已有保存内容',
      'Original history contains saved content',
    ),
    BlobUploadObservation.released => _s(
      '原记录中的内容已释放',
      'Original content has been released',
    ),
  };

  @override
  Widget build(BuildContext context) {
    final request = plans.request,
        state = plans.snapshot,
        proposal = plans.result?.view;
    final upload = uploadView == null ? null : uploads.snapshot;
    final finishable =
        plans.canAcknowledge &&
        (state?.uploadKey == null || uploadClosed || uploads.canAcknowledge);
    return AlertDialog(
      title: Text(_s('保存插件内容', 'Save plugin content')),
      content: SizedBox(
        width: 660,
        child: SingleChildScrollView(
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                _s(
                  '内容会保存到当前内容库。先查看插件生成的提案，再决定是否批准本次保存。',
                  'Content is saved in the current library. Review the plugin proposal before approving this save.',
                ),
              ),
              const SizedBox(height: 12),
              if (request == null && widget.entry != null) ...[
                Text(widget.entry!.name),
                if (!eligible)
                  Text(
                    _s(
                      '请先在插件列表中启用插件并允许保存内容。',
                      'Enable this plugin and allow content saving in the plugin list first.',
                    ),
                  ),
                DropdownButtonFormField<String>(
                  key: const ValueKey('blob-plan-handler'),
                  initialValue: selected,
                  items: [
                    for (final h in handlers)
                      DropdownMenuItem(value: h.name, child: Text(h.name)),
                  ],
                  onChanged: busy || !plans.canStart
                      ? null
                      : (value) => setState(() {
                          selected = value;
                          fileBytes = null;
                          fileName = null;
                          reviewed = false;
                        }),
                  decoration: InputDecoration(
                    labelText: _s('插件功能', 'Plugin action'),
                  ),
                ),
                if (handler?.inputType == 'text.utf8')
                  TextField(
                    key: const ValueKey('blob-plan-input'),
                    controller: text,
                    enabled: !busy && plans.canStart,
                    minLines: 3,
                    maxLines: 6,
                    maxLength: min(handler!.maxInputBytes, 32768),
                    decoration: InputDecoration(
                      labelText: _s('交给插件的内容', 'Content for the plugin'),
                    ),
                  )
                else if (handler != null) ...[
                  Text(
                    _s(
                      '选择此插件支持的输入文件。文件内容将在本次提案中固定。',
                      'Choose an input file supported by this plugin. Its contents are fixed for this proposal.',
                    ),
                  ),
                  _button(
                    'blob-plan-file',
                    fileName ?? _s('选择输入文件', 'Choose input file'),
                    plans.canStart,
                    _chooseFile,
                  ),
                ],
                Text(
                  _s(
                    '输入最多 ${handler == null ? 0 : min(handler!.maxInputBytes, 32768)} 字节。',
                    'Input limit: ${handler == null ? 0 : min(handler!.maxInputBytes, 32768)} bytes.',
                  ),
                ),
                _button(
                  'blob-plan-start',
                  _s('生成上传提案', 'Prepare save proposal'),
                  eligible && plans.canStart,
                  _start,
                ),
              ] else if (request == null)
                Text(
                  _s(
                    '当前没有待处理的上传提案。请从插件列表选择“保存内容”。',
                    'No pending save proposal. Choose Save content in the plugin list to begin.',
                  ),
                ),
              if (request != null) ...[
                Text(
                  _s(
                    '本次插件：${request.packageId}',
                    'Plugin for this attempt: ${request.packageId}',
                  ),
                ),
                if (state?.running == true)
                  Text(_s('正在准备提案…', 'Preparing proposal…')),
                if (state?.error != null)
                  Text(
                    _s(
                      '此提案已停止或不可用。可核对状态并结束本次记录。',
                      'This proposal has stopped or is unavailable. Check its status and finish this record.',
                    ),
                  ),
                if (plans.deliveryLost)
                  Text(
                    _s(
                      '未收到原提案，无法批准本次保存。',
                      'The original proposal was not received. This save cannot be approved.',
                    ),
                  ),
                if (plans.approvalAttempted && state?.consumed != true)
                  Text(
                    _s(
                      '批准已发出，结果尚未确认。更新状态以查找原上传。',
                      'Approval was sent but its result is unconfirmed. Refresh to find the original upload.',
                    ),
                  ),
                if (state?.submissionFailed == true)
                  Text(
                    _s(
                      '本次上传未能启动；此批准已使用。',
                      'This upload could not start; its approval has been used.',
                    ),
                  ),
                if (proposal != null)
                  Card(
                    child: Padding(
                      padding: const EdgeInsets.all(12),
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Text(
                            _s(
                              '插件提供的内容说明',
                              'Description supplied by the plugin',
                            ),
                          ),
                          Text(
                            proposal.label,
                            key: const ValueKey('blob-plan-label'),
                          ),
                          Text(
                            _s(
                              '保存大小：${proposal.byteLength} 字节',
                              'Save size: ${proposal.byteLength} bytes',
                            ),
                          ),
                          Text(_s('位置：当前内容库', 'Destination: current library')),
                          ExpansionTile(
                            title: Text(_s('核对内容指纹', 'Content fingerprint')),
                            children: [
                              SelectableText(
                                proposal.wholeSha256
                                    .map(
                                      (b) =>
                                          b.toRadixString(16).padLeft(2, '0'),
                                    )
                                    .join(),
                              ),
                            ],
                          ),
                        ],
                      ),
                    ),
                  ),
                if (proposal != null && !plans.approvalAttempted) ...[
                  CheckboxListTile(
                    key: const ValueKey('blob-plan-reviewed'),
                    value: reviewed,
                    onChanged: busy || !plans.canApprove
                        ? null
                        : (v) => setState(() => reviewed = v == true),
                    title: Text(
                      _s(
                        '我已核对以上内容，允许本次保存。',
                        'I reviewed this content and allow this save.',
                      ),
                    ),
                  ),
                  _button(
                    'blob-plan-approve',
                    _s('批准本次保存', 'Approve this save'),
                    reviewed && plans.canApprove,
                    _approve,
                  ),
                ],
                Wrap(
                  spacing: 8,
                  runSpacing: 8,
                  children: [
                    _button(
                      'blob-plan-refresh',
                      _s('更新状态', 'Refresh status'),
                      true,
                      () => _action(_refresh),
                    ),
                    _button(
                      'blob-plan-read',
                      _s('查看提案', 'Read proposal'),
                      plans.canRead,
                      () => _action(() async {
                        reviewed = false;
                        await plans.read(planView);
                      }),
                    ),
                    _button(
                      'blob-plan-cancel',
                      _s('停止本次任务', 'Stop this task'),
                      !uploadClosed &&
                          !plans.acknowledgementUncertain &&
                          !uploads.acknowledgementUncertain,
                      () => _action(() async {
                        reviewed = false;
                        await plans.cancel(planView);
                      }),
                    ),
                  ],
                ),
              ],
              if (upload != null && !uploadClosed) ...[
                const Divider(),
                Text(_s('原上传任务', 'Original upload task')),
                if (upload.task.exit == null) Text(_s('正在保存…', 'Saving…')),
                if (uploads.result != null)
                  Text(
                    uploads.result!.unknown
                        ? _s(
                            '上传结果尚不能确认。',
                            'The upload outcome is still unconfirmed.',
                          )
                        : uploads.result!.error != null
                        ? _s(
                            '本次上传未成功完成；请核对原记录。',
                            'This upload did not complete successfully; check its original history.',
                          )
                        : uploads.result!.phase == BlobUploadObservation.staged
                        ? _s('内容已保存。', 'Content saved.')
                        : _s(
                            '原任务已返回结果。',
                            'The original task returned a result.',
                          ),
                  ),
                if (uploads.deliveryLost)
                  Text(
                    _s(
                      '未收到结果回复。请核对原记录；不会重新上传。',
                      'The result reply was not received. Check the original history; the upload will not be repeated.',
                    ),
                  ),
                if (upload.observed != null)
                  Text(
                    _observation(upload.observed!),
                    key: const ValueKey('blob-plan-history'),
                  ),
                Wrap(
                  spacing: 8,
                  runSpacing: 8,
                  children: [
                    _button(
                      'blob-upload-read',
                      _s('查看上传结果', 'Read upload result'),
                      uploads.canRead,
                      () => _action(() async {
                        await uploads.read(uploadView!);
                      }),
                    ),
                    _button(
                      'blob-upload-inspect',
                      _s('核对原记录', 'Check original history'),
                      upload.task.exit != null,
                      () => _action(() async {
                        await uploads.inspect(uploadView!);
                      }),
                    ),
                    _button(
                      'blob-upload-confirm',
                      _s('已核对，确认此记录', 'Confirm reviewed history'),
                      upload.delivered &&
                          upload.observed != null &&
                          (uploads.deliveryLost || upload.recoveryRequired) &&
                          !uploads.acknowledgementUncertain,
                      () => _action(() async {
                        await uploads.confirm(uploadView!, upload.observed!);
                      }),
                    ),
                    _button(
                      'blob-upload-repair',
                      _s('完成原任务清理', 'Finish original task cleanup'),
                      upload.task.exit != null &&
                          upload.task.storage ==
                              IoStoragePhase.recoveryRequired &&
                          !uploads.acknowledgementUncertain,
                      () => _action(() async {
                        await uploads.repair(uploadView!);
                      }),
                    ),
                  ],
                ),
              ],
              if (plans.acknowledgementUncertain ||
                  uploads.acknowledgementUncertain)
                Text(
                  _s(
                    '结束操作的回复缺失。保留本次记录，不能重复结束或新建上传。',
                    'The finish reply is missing. This record is retained; finishing again or starting another upload is unavailable.',
                  ),
                ),
              if (notice != null ||
                  plans.failure != null ||
                  uploads.failure != null)
                Text(
                  notice == 'too-large'
                      ? _s('输入超过此插件的大小限制。', 'Input exceeds the plugin limit.')
                      : notice == 'input-required'
                      ? _s('请先选择输入文件。', 'Choose an input file first.')
                      : notice == 'different-upload'
                      ? _s(
                          '另一个原上传仍待处理，未更换该任务。',
                          'Another original upload is still pending; that task was not replaced.',
                        )
                      : _s(
                          '暂时无法确认操作结果。请更新原任务状态。',
                          'The operation outcome cannot be confirmed yet. Refresh the original task status.',
                        ),
                ),
              if (request != null)
                _button(
                  'blob-plan-finish',
                  _s('结束并保留记录', 'Finish and keep record'),
                  finishable,
                  _finish,
                ),
              if (plans.history.isNotEmpty)
                Text(
                  _s(
                    '本会话已完成 ${plans.history.length} 条记录。',
                    '${plans.history.length} record(s) completed in this session.',
                  ),
                ),
              if (busy) const LinearProgressIndicator(),
              const SizedBox(height: 12),
              Text(
                _s(
                  '关闭窗口会请求停止尚未结束的原任务。已保存的内容不会因此删除；重新打开可继续核对。',
                  'Closing requests that the original unfinished task stop. Saved content is kept; reopen to continue checking.',
                ),
              ),
            ],
          ),
        ),
      ),
      actions: [
        TextButton(
          key: const ValueKey('blob-plan-close'),
          onPressed: () => Navigator.of(context).pop(),
          child: Text(_s('关闭', 'Close')),
        ),
      ],
    );
  }
}
