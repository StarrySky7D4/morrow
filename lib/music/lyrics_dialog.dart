import 'package:flutter/material.dart';
import '../appearance.dart';
import 'lyrics_service.dart';

class LyricsSearchDialog extends StatefulWidget {
  const LyricsSearchDialog({
    super.key,
    required this.title,
    required this.artist,
    required this.service,
  });
  final String title, artist;
  final LyricsService service;
  @override
  State<LyricsSearchDialog> createState() => _LyricsSearchDialogState();
}

class _LyricsSearchDialogState extends State<LyricsSearchDialog> {
  late final title = TextEditingController(text: widget.title);
  late final artist = TextEditingController(text: widget.artist);
  List<LyricMatch>? matches;
  bool busy = false;
  String? error;
  Future<void> search() async {
    if (busy || title.text.trim().isEmpty) return;
    setState(() {
      busy = true;
      error = null;
    });
    try {
      final result = await widget.service.search(title.text, artist.text);
      if (mounted) setState(() => matches = result);
    } catch (_) {
      if (mounted) setState(() => error = '无法连接歌词服务，请稍后重试或导入本地歌词。');
    } finally {
      if (mounted) setState(() => busy = false);
    }
  }

  @override
  void dispose() {
    title.dispose();
    artist.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => StudioDialog(
    title: '查找歌词',
    subtitle: '按歌名和歌手查询 LRCLIB，选择对应版本。',
    content: SizedBox(
      width: 460,
      child: SingleChildScrollView(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            TextField(
              controller: title,
              decoration: const InputDecoration(labelText: '歌名'),
              onSubmitted: (_) => search(),
            ),
            const SizedBox(height: 12),
            TextField(
              controller: artist,
              decoration: const InputDecoration(labelText: '歌手（可选）'),
              onSubmitted: (_) => search(),
            ),
            if (busy)
              const Padding(
                padding: EdgeInsets.all(16),
                child: LinearProgressIndicator(),
              ),
            if (error != null)
              Padding(padding: const EdgeInsets.all(12), child: Text(error!)),
            if (matches?.isEmpty ?? false)
              const Padding(
                padding: EdgeInsets.all(12),
                child: Text('没有找到歌词，试试调整歌名或歌手。'),
              ),
            if (matches != null)
              ...matches!.map(
                (match) => ListTile(
                  title: Text('${match.title} · ${match.artist}'),
                  subtitle: Text(
                    '${match.album}\n${match.synced ? '同步歌词' : '纯文本歌词'} · ${match.duration.round()} 秒',
                  ),
                  isThreeLine: true,
                  trailing: const Icon(Icons.chevron_right),
                  onTap: () => Navigator.pop(context, match),
                ),
              ),
          ],
        ),
      ),
    ),
    actions: [
      TextButton(
        onPressed: () => Navigator.pop(context),
        child: const Text('取消'),
      ),
      FilledButton(onPressed: busy ? null : search, child: const Text('搜索')),
    ],
  );
}
