import 'dart:typed_data';
import 'package:flutter/material.dart';
import 'blob_upload_history_models.dart';
import 'blob_upload_models.dart';

/// Read-only history, deliberately separate from live upload/repair controls.
final class BlobUploadHistoryManager extends StatefulWidget {
  const BlobUploadHistoryManager({super.key, required this.backend});
  final BlobUploadHistoryBackend backend;
  @override
  State<BlobUploadHistoryManager> createState() =>
      _BlobUploadHistoryManagerState();
}

final class _BlobUploadHistoryManagerState
    extends State<BlobUploadHistoryManager> {
  List<BlobUploadHistoryRecord> rows = [];
  final Map<String, BlobUploadHistoryInspection> observations = {};
  Uint8List? cursor;
  bool busy = false, more = false;
  String? notice;
  int epoch = 0;

  String _s(String zh, String en) =>
      Localizations.localeOf(context).languageCode == 'zh' ? zh : en;
  static String _hex(Uint8List value) =>
      value.map((b) => b.toRadixString(16).padLeft(2, '0')).join();
  bool _current(BlobUploadHistoryBackend backend, int generation) =>
      mounted && identical(widget.backend, backend) && epoch == generation;
  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) _load(reset: true);
    });
  }

  @override
  void didUpdateWidget(covariant BlobUploadHistoryManager oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (!identical(oldWidget.backend, widget.backend)) {
      epoch++;
      rows = [];
      observations.clear();
      cursor = null;
      busy = false;
      more = false;
      notice = null;
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (mounted) _load(reset: true);
      });
    }
  }

  @override
  void dispose() {
    epoch++;
    super.dispose();
  }

  Future<void> _load({required bool reset}) async {
    if (busy || (!reset && (!more || rows.length >= 1024))) return;
    final backend = widget.backend, generation = ++epoch;
    setState(() {
      busy = true;
      notice = null;
    });
    try {
      final page = await backend.listBlobUploadHistory(
        after: reset ? null : cursor,
        limit: 16,
      );
      if (!_current(backend, generation)) return;
      final next = reset ? page.records : [...rows, ...page.records];
      if (page.records.length > 16 ||
          next.length > 1024 ||
          next.map((r) => _hex(r.submission)).toSet().length != next.length ||
          (page.records.isNotEmpty &&
              (page.cursor == null ||
                  _hex(page.cursor!) != _hex(page.records.last.submission)))) {
        throw const FormatException('History page inconsistent');
      }
      setState(() {
        rows = List.unmodifiable(next);
        cursor = page.cursor;
        more = page.records.length == 16 && rows.length < 1024;
        if (reset) observations.clear();
      });
    } catch (_) {
      if (_current(backend, generation)) {
        setState(() {
          notice = _s(
            '读取未完成。保留当前内容库；可以稍后重新查看。',
            'History could not be read. Keep this library open and check again later.',
          );
        });
      }
    } finally {
      if (_current(backend, generation)) setState(() => busy = false);
    }
  }

  Future<void> _inspect(BlobUploadHistoryRecord record) async {
    if (busy) return;
    final backend = widget.backend, generation = ++epoch;
    setState(() {
      busy = true;
      notice = null;
    });
    try {
      final value = await backend.inspectBlobUploadHistory(record.submission);
      if (!_current(backend, generation)) return;
      if (_hex(value.record.submission) != _hex(record.submission) ||
          _hex(value.record.bindingSha256) != _hex(record.bindingSha256) ||
          value.record.revision < record.revision) {
        throw const FormatException('Historical request changed');
      }
      setState(() {
        observations[_hex(record.submission)] = value;
        rows = [
          for (final row in rows)
            _hex(row.submission) == _hex(record.submission)
                ? value.record
                : row,
        ];
      });
    } catch (_) {
      if (_current(backend, generation)) {
        setState(() {
          notice = _s(
            '当前内容状态未能读取，原任务记录保持不变。',
            'Current content state could not be read. The original task record is unchanged.',
          );
        });
      }
    } finally {
      if (_current(backend, generation)) setState(() => busy = false);
    }
  }

  String _result(BlobUploadHistoryRecord row) {
    final result = row.originalResult;
    if (result == null) {
      return _s(
        '原任务结果未记录，执行结果不确定',
        'Original task result was not recorded; outcome is uncertain',
      );
    }
    if (result.unknown) {
      return _s('原任务结果不确定', 'Original task result is uncertain');
    }
    if (result.error != null) {
      return _s('原任务报告失败', 'Original task reported failure');
    }
    return switch (result.phase) {
      BlobUploadObservation.staged => _s(
        '原任务报告：内容已保存',
        'Original task reported: content saved',
      ),
      BlobUploadObservation.released => _s(
        '原任务报告：内容已释放',
        'Original task reported: content released',
      ),
      _ => _s('原任务报告：未找到内容', 'Original task reported: content absent'),
    };
  }

  String _currentStage(BlobUploadObservation value) => switch (value) {
    BlobUploadObservation.staged => _s(
      '当前内容记录：已保存',
      'Current content record: saved',
    ),
    BlobUploadObservation.released => _s(
      '当前内容记录：已释放',
      'Current content record: released',
    ),
    BlobUploadObservation.absent => _s(
      '当前内容记录：未找到',
      'Current content record: absent',
    ),
  };
  String _time(BigInt value) {
    if (value.abs() > BigInt.from(8640000000000000)) {
      return _s('时间不可显示', 'Time unavailable');
    }
    return DateTime.fromMillisecondsSinceEpoch(
      value.toInt(),
    ).toLocal().toString().split('.').first;
  }

  Widget _row(BlobUploadHistoryRecord row) {
    final key = _hex(row.submission),
        observed = observations[_hex(row.submission)];
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(row.packageId, style: Theme.of(context).textTheme.titleMedium),
            const SizedBox(height: 4),
            Text(
              '${_time(row.reservedAt)} · ${row.byteLength} ${_s('字节', 'bytes')}',
            ),
            const SizedBox(height: 8),
            Text(_result(row), key: ValueKey('upload-history-result-$key')),
            Text(
              row.acknowledgedAt == null
                  ? _s(
                      '未记录原任务确认',
                      'Original task acknowledgement was not recorded',
                    )
                  : _s('原任务已确认', 'Original task was acknowledged'),
            ),
            if (observed != null) ...[
              const SizedBox(height: 8),
              Text(
                _currentStage(observed.currentStage),
                key: ValueKey('upload-history-current-$key'),
              ),
              Text(
                _s(
                  '这是当前内容状态，不会改写原任务结果。',
                  'This is the current content state; it does not change the original result.',
                ),
              ),
            ],
            const SizedBox(height: 8),
            OutlinedButton.icon(
              key: ValueKey('upload-history-inspect-$key'),
              onPressed: busy ? null : () => _inspect(row),
              icon: const Icon(Icons.search),
              label: Text(_s('查看当前内容记录', 'Check current content record')),
            ),
            ExpansionTile(
              tilePadding: EdgeInsets.zero,
              title: Text(_s('核对信息', 'Record details')),
              children: [
                Align(
                  alignment: Alignment.centerLeft,
                  child: SelectableText(
                    '${_s('任务标识', 'Submission')}: $key\n${_s('内容校验值', 'Content SHA-256')}: ${_hex(row.wholeSha256)}\n${_s('插件校验值', 'Plugin SHA-256')}: ${_hex(row.packageDigest)}',
                  ),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }

  @override
  Widget build(BuildContext context) => AlertDialog(
    title: Text(_s('历史上传记录', 'Upload history')),
    content: SizedBox(
      width: 720,
      height: 560,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            _s(
              '记录来自当前内容库。确认记录不代表当时的界面已收到结果。这里分别显示原任务结果与当前内容状态，不会启动上传或替旧任务确认。',
              'Records belong to this library. Acknowledgement does not prove that the former UI received the result. Original results and current content state are separate; this view starts no upload and acknowledges no old task.',
            ),
          ),
          if (busy)
            const Padding(
              padding: EdgeInsets.symmetric(vertical: 8),
              child: LinearProgressIndicator(),
            ),
          if (notice != null)
            Padding(
              padding: const EdgeInsets.symmetric(vertical: 8),
              child: Text(
                notice!,
                key: const ValueKey('upload-history-notice'),
              ),
            ),
          const SizedBox(height: 12),
          Expanded(
            child: rows.isEmpty
                ? Center(
                    child: Text(
                      busy
                          ? _s('正在读取…', 'Reading…')
                          : notice != null
                          ? _s('尚未取得可显示的记录。', 'No records could be loaded.')
                          : _s(
                              '当前内容库没有可显示的上传记录。空列表不代表旧任务未执行。',
                              'This library has no upload records to display. An empty list does not prove an old task did not run.',
                            ),
                    ),
                  )
                : ListView.builder(
                    itemCount: rows.length,
                    itemBuilder: (_, i) => _row(rows[i]),
                  ),
          ),
          if (more)
            TextButton(
              key: const ValueKey('upload-history-more'),
              onPressed: busy ? null : () => _load(reset: false),
              child: Text(_s('读取更多记录', 'Read more records')),
            ),
        ],
      ),
    ),
    actions: [
      TextButton(
        key: const ValueKey('upload-history-refresh'),
        onPressed: busy ? null : () => _load(reset: true),
        child: Text(_s('重新查看', 'Refresh')),
      ),
      TextButton(
        onPressed: () => Navigator.of(context).pop(),
        child: Text(_s('关闭', 'Close')),
      ),
    ],
  );
}
