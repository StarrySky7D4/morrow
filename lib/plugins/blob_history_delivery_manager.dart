import 'dart:convert';
import 'dart:math';
import 'dart:typed_data';
import 'package:crypto/crypto.dart';
import 'package:flutter/material.dart';
import 'blob_history_delivery_models.dart';
import 'blob_history_delivery_session.dart';
import 'blob_upload_history_packet.dart';
import 'plugin_library.dart';

/// Trusted review of one public record and one explicitly selected recipient.
final class BlobHistoryDeliveryManager extends StatefulWidget {
  const BlobHistoryDeliveryManager({
    super.key,
    required this.backend,
    required this.plugins,
    required this.registryRevision,
    required this.onBack,
    this.submission,
  });
  final BlobHistoryDeliveryBackend backend;
  final List<PluginLibraryEntry> plugins;
  final BigInt? registryRevision;
  final Uint8List? submission;
  final VoidCallback onBack;
  @override
  State<BlobHistoryDeliveryManager> createState() =>
      _BlobHistoryDeliveryManagerState();
}

final class _Recipient {
  const _Recipient(this.plugin, this.handler);
  final PluginLibraryEntry plugin;
  final PluginTransformHandler handler;
  String get key => '${plugin.id}\n${handler.name}';
}

final class _BlobHistoryDeliveryManagerState
    extends State<BlobHistoryDeliveryManager> {
  late BlobHistoryDeliverySession session;
  late Object view;
  String? selected;
  String _s(String zh, String en) =>
      Localizations.localeOf(context).languageCode == 'zh' ? zh : en;
  static String _hex(List<int> value) =>
      value.map((n) => n.toRadixString(16).padLeft(2, '0')).join();
  List<_Recipient> get recipients => [
    for (final p in widget.plugins)
      if (p.enabled &&
          p.available &&
          !p.builtin &&
          p.dependencies.isEmpty &&
          p.declared.isEmpty &&
          p.approved.isEmpty &&
          p.declaredIo.isEmpty &&
          p.approvedIo.isEmpty &&
          p.ioHandlers.isEmpty &&
          !p.channelSupported &&
          !p.blobUploadSupported &&
          !p.mutationSupported &&
          !p.directorySupported &&
          p.handlers.map((h) => h.name).toSet().length == p.handlers.length)
        for (final h in p.handlers)
          if (h.inputType == PublicHistoryPacket.inputType &&
              h.maxInputBytes > 0 &&
              h.maxOutputBytes > 0 &&
              h.maxOutputBytes <= 8192)
            _Recipient(p, h),
  ];
  _Recipient? get recipient =>
      recipients.where((r) => r.key == selected).firstOrNull;
  bool get currentSelection {
    final request = session.request;
    return request != null &&
        widget.registryRevision == request.registryRevision &&
        recipients.any(
          (r) =>
              r.plugin.id == request.packageId &&
              _hex(r.plugin.digest) == _hex(request.packageDigest) &&
              r.handler.name == request.handler &&
              r.handler.outputType == request.outputType,
        );
  }

  void _bind() {
    session = BlobHistoryDeliverySession.forBackend(widget.backend);
    view = session.attachView(_changed);
  }

  void _changed() {
    if (mounted) setState(() {});
  }

  @override
  void initState() {
    super.initState();
    _bind();
  }

  @override
  void didUpdateWidget(covariant BlobHistoryDeliveryManager oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (!identical(oldWidget.backend, widget.backend)) {
      session.detachView(view);
      selected = null;
      _bind();
    } else if (oldWidget.registryRevision != widget.registryRevision ||
        !identical(oldWidget.plugins, widget.plugins)) {
      session.review(view, false);
    }
  }

  @override
  void dispose() {
    session.detachView(view);
    super.dispose();
  }

  Future<void> _prepare() async {
    final target = recipient,
        revision = widget.registryRevision,
        submission = widget.submission;
    if (target == null ||
        revision == null ||
        submission == null ||
        !session.canPrepare) {
      return;
    }
    final random = Random.secure();
    final id = Uint8List.fromList(
      List<int>.generate(32, (_) => random.nextInt(256)),
    );
    if (id.every((n) => n == 0)) return;
    await session.prepare(
      view,
      BlobHistoryDeliveryRequest(
        requestId: id,
        submission: submission,
        packageId: target.plugin.id,
        packageDigest: target.plugin.digest,
        registryRevision: revision,
        handler: target.handler.name,
        outputType: target.handler.outputType,
        timeoutMs: 120000,
      ),
    );
  }

  String _original(PublicHistoryPacket history) {
    if (history.originalUnknown) {
      return _s('原上传任务结果不确定', 'Original upload outcome is uncertain');
    }
    return _s('原上传任务结果：', 'Original upload result: ') +
        switch (history.originalResult?.outcome) {
          PublicHistoryOutcome.staged => _s('已保存', 'saved'),
          PublicHistoryOutcome.released => _s('已释放', 'released'),
          PublicHistoryOutcome.absent => _s('未找到内容', 'content absent'),
          PublicHistoryOutcome.error => _s('失败', 'failed'),
          null => _s('未记录', 'not recorded'),
        };
  }

  String _result(BlobHistoryDeliveryReceipt r) {
    if (r.close == BlobHistoryDeliveryClose.failed ||
        r.close == BlobHistoryDeliveryClose.unknown) {
      return _s(
        '本次交付的关闭结果未确认，保留原记录。',
        'This delivery has no confirmed closure. Keep its original record.',
      );
    }
    if (r.error == BlobHistoryDeliveryError.cancelled) {
      return _s(
        '已取消本次交付，插件未启动。',
        'This delivery was cancelled before starting the plugin.',
      );
    }
    if (r.error != null || r.deliveryError != null) {
      return _s(
        '本次交付未成功，原上传记录保持不变。',
        'This delivery did not succeed. The original upload record is unchanged.',
      );
    }
    if (!r.consumed) {
      return _s(
        '内容已准备，等待你的批准。',
        'The record is prepared and awaiting your approval.',
      );
    }
    return r.finished
        ? _s('本次交付已完成。', 'This delivery completed.')
        : _s('本次交付结果尚未确认。', 'The delivery result is not yet confirmed.');
  }

  String _packetDetails(BlobHistoryDeliveryPreview preview) {
    final h = preview.selectedHistory, r = h.originalResult, e = r?.execution;
    return [
      'Submission: ${_hex(h.submission)}',
      'Original plugin: ${h.packageId}',
      'Original plugin SHA-256: ${_hex(h.packageDigest)}',
      'Registry revision: ${h.registryRevision}',
      'Operation: ${h.operationId}',
      'Handler: ${h.handler}',
      'Content bytes: ${h.byteLength}',
      'Content SHA-256: ${_hex(h.wholeSha256)}',
      'Input SHA-256: ${_hex(h.inputSha256)}',
      'Request binding SHA-256: ${_hex(h.requestBindingSha256)}',
      'Reserved at (Unix ms): ${h.reservedAt}',
      'History revision: ${h.revision}',
      'Original result: ${r?.outcome.name ?? 'not recorded'}',
      'Original Unknown: ${h.originalUnknown}',
      'Original error: ${r?.errorCode ?? 'none'}',
      'Original calls: ${r?.calls ?? 'not recorded'}',
      'Original wire bytes: ${r?.wireBytes ?? 'not recorded'}',
      'Original native work bytes: ${r?.nativeWorkBytes ?? 'not recorded'}',
      'Execution exit: ${e?.exitCode ?? 'not recorded'}',
      'Execution error: ${e?.errorCode ?? 'none'}',
      'Execution host calls: ${e?.hostCalls ?? 'not recorded'}',
      'Observed at (Unix ms): ${h.observedAt ?? 'not recorded'}',
      'Original acknowledged at (Unix ms): ${h.acknowledgedAt ?? 'not recorded'}',
      'Current content record: ${h.currentStage.name}',
      'Query nonce: ${_hex(h.queryNonce)}',
      'Disclosure SHA-256: ${_hex(preview.packetSha256)}',
    ].join('\n');
  }

  Widget _output(BlobHistoryDeliveryReceipt receipt) {
    final output = receipt.output!;
    String? text;
    if (receipt.outputType == 'text.utf8') {
      try {
        text = utf8.decode(output);
      } on FormatException {
        /* Keep undecodable output as bytes. */
      }
    }
    return ExpansionTile(
      title: Text(_s('插件返回内容', 'Plugin output')),
      children: [
        SelectableText(
          '${receipt.outputType} · ${output.length} ${_s('字节', 'bytes')}\nSHA-256: ${sha256.convert(output)}',
        ),
        if (text != null) SelectableText(text),
      ],
    );
  }

  @override
  Widget build(BuildContext context) {
    final request = session.request,
        preview = session.preview,
        receipt = session.receipt;
    return AlertDialog(
      title: Text(_s('将历史记录交给插件', 'Share one history record with a plugin')),
      content: SizedBox(
        width: 720,
        height: 560,
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              _s(
                '只交付你核对的这一条历史记录。原内容不会发送，也不会重试上传或改写原任务结果。',
                'Share only the history record you review. Content is not sent, uploads are not retried, and the original result stays unchanged.',
              ),
            ),
            if (session.busy) const LinearProgressIndicator(),
            const SizedBox(height: 12),
            Expanded(
              child: SingleChildScrollView(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    if (request == null) ...[
                      if (widget.submission == null)
                        Text(
                          _s(
                            '请先返回历史列表，选择一条记录。',
                            'Return to history and choose one record.',
                          ),
                        ),
                      if (recipients.isEmpty)
                        Text(
                          _s(
                            '当前没有已启用且可接收历史记录的插件。',
                            'No enabled plugin can receive this history record.',
                          ),
                        ),
                      DropdownButtonFormField<String>(
                        key: const ValueKey('history-delivery-recipient'),
                        initialValue: recipients.any((r) => r.key == selected)
                            ? selected
                            : null,
                        isExpanded: true,
                        decoration: InputDecoration(
                          labelText: _s('接收插件', 'Recipient plugin'),
                        ),
                        items: [
                          for (final r in recipients)
                            DropdownMenuItem(
                              value: r.key,
                              child: Text(
                                '${r.plugin.name} · ${r.handler.name}',
                                overflow: TextOverflow.ellipsis,
                              ),
                            ),
                        ],
                        onChanged: session.canPrepare
                            ? (value) => setState(() => selected = value)
                            : null,
                      ),
                      const SizedBox(height: 12),
                      FilledButton(
                        key: const ValueKey('history-delivery-prepare'),
                        onPressed:
                            session.canPrepare &&
                                recipient != null &&
                                widget.submission != null &&
                                widget.registryRevision != null
                            ? _prepare
                            : null,
                        child: Text(
                          _s('预览要交付的记录', 'Preview the record to share'),
                        ),
                      ),
                    ] else ...[
                      Text('${_s('接收插件', 'Recipient')}: ${request.packageId}'),
                      Text('${_s('接收操作', 'Handler')}: ${request.handler}'),
                      SelectableText(
                        '${_s('接收插件校验值', 'Recipient SHA-256')}: ${_hex(request.packageDigest)}',
                      ),
                      if (preview != null) ...[
                        const SizedBox(height: 12),
                        Text(
                          _original(preview.selectedHistory),
                          key: const ValueKey('history-delivery-original'),
                        ),
                        Text(
                          '${_s('原上传插件', 'Original upload plugin')}: ${preview.selectedHistory.packageId}',
                        ),
                        Text(
                          '${_s('原内容大小', 'Original content size')}: ${preview.selectedHistory.byteLength} ${_s('字节', 'bytes')}',
                        ),
                        ExpansionTile(
                          title: Text(_s('完整交付字段', 'All shared fields')),
                          children: [SelectableText(_packetDetails(preview))],
                        ),
                        CheckboxListTile(
                          key: const ValueKey('history-delivery-reviewed'),
                          contentPadding: EdgeInsets.zero,
                          value: session.reviewed,
                          onChanged: session.canReview && currentSelection
                              ? (v) => session.review(view, v == true)
                              : null,
                          title: Text(
                            _s(
                              '我已核对接收插件及这条记录，允许交付一次。',
                              'I reviewed this recipient and record and allow one delivery.',
                            ),
                          ),
                        ),
                        FilledButton(
                          key: const ValueKey('history-delivery-approve'),
                          onPressed: session.canApprove && currentSelection
                              ? () => session.approve(view)
                              : null,
                          child: Text(_s('批准交付一次', 'Approve one delivery')),
                        ),
                      ],
                      if (!currentSelection)
                        Text(
                          _s(
                            '插件选择已变化；保留原请求，可读取或取消尚未执行的交付。',
                            'The plugin selection changed. Keep this request; read it or cancel an unstarted delivery.',
                          ),
                        ),
                      if (receipt != null) ...[
                        const SizedBox(height: 12),
                        Text(
                          _result(receipt),
                          key: const ValueKey('history-delivery-result'),
                        ),
                        Text(
                          receipt.acknowledged
                              ? _s(
                                  '本次交付确认已记录。',
                                  'Acknowledgement of this delivery was recorded.',
                                )
                              : _s(
                                  '本次交付尚未确认。',
                                  'This delivery has not been acknowledged.',
                                ),
                        ),
                        if (receipt.output != null) _output(receipt),
                      ],
                      if (session.observationRequired)
                        Text(
                          _s(
                            '回复未能确认。请读取原请求；不要重新交付。',
                            'The reply could not be confirmed. Read the original request; do not submit again.',
                          ),
                          key: const ValueKey('history-delivery-unresolved'),
                        ),
                      const SizedBox(height: 12),
                      Wrap(
                        spacing: 8,
                        runSpacing: 8,
                        children: [
                          OutlinedButton(
                            key: const ValueKey('history-delivery-read'),
                            onPressed: session.canRead
                                ? () => session.read(view)
                                : null,
                            child: Text(_s('读取原请求', 'Read original request')),
                          ),
                          OutlinedButton(
                            key: const ValueKey('history-delivery-cancel'),
                            onPressed: session.canCancel
                                ? () => session.cancel(view)
                                : null,
                            child: Text(_s('取消本次交付', 'Cancel this delivery')),
                          ),
                          OutlinedButton(
                            key: const ValueKey('history-delivery-ack'),
                            onPressed: session.canAcknowledge
                                ? () => session.acknowledge(view)
                                : null,
                            child: Text(
                              _s('确认本次结果', 'Acknowledge this result'),
                            ),
                          ),
                          TextButton(
                            key: const ValueKey('history-delivery-finish'),
                            onPressed: session.canFinish
                                ? () => session.finish(view)
                                : null,
                            child: Text(_s('完成此次查看', 'Finish this review')),
                          ),
                        ],
                      ),
                      const SizedBox(height: 12),
                      Text(
                        _s(
                          '关闭或返回后，这次记录仍保留在当前内容库会话中。',
                          'Closing or going back keeps this request in the current library session.',
                        ),
                      ),
                    ],
                  ],
                ),
              ),
            ),
          ],
        ),
      ),
      actions: [
        TextButton(
          key: const ValueKey('history-delivery-back'),
          onPressed: widget.onBack,
          child: Text(_s('返回历史记录', 'Back to history')),
        ),
      ],
    );
  }
}
