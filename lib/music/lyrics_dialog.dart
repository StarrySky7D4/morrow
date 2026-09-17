import 'package:morrow_i18n/morrow_i18n.dart';
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
  bool failed = false;
  Future<void> search() async {
    if (busy || title.text.trim().isEmpty) return;
    setState(() {
      busy = true;
      failed = false;
    });
    try {
      final result = await widget.service.search(title.text, artist.text);
      if (mounted) setState(() => matches = result);
    } catch (_) {
      if (mounted) setState(() => failed = true);
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
    title: L10n.of(context).visualFindLyrics,
    subtitle: L10n.of(context).visualFindLyricsGuide,
    content: SizedBox(
      width: 460,
      child: SingleChildScrollView(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            TextField(
              controller: title,
              decoration: InputDecoration(
                labelText: L10n.of(context).visualSongTitle,
              ),
              onSubmitted: (_) => search(),
            ),
            const SizedBox(height: 12),
            TextField(
              controller: artist,
              decoration: InputDecoration(
                labelText: L10n.of(context).visualOptionalArtist,
              ),
              onSubmitted: (_) => search(),
            ),
            if (busy)
              const Padding(
                padding: EdgeInsets.all(16),
                child: LinearProgressIndicator(),
              ),
            if (failed)
              Padding(
                padding: const EdgeInsets.all(12),
                child: Text(L10n.of(context).visualLyricsServiceFailure),
              ),
            if (matches?.isEmpty ?? false)
              Padding(
                padding: EdgeInsets.all(12),
                child: Text(L10n.of(context).visualLyricsNotFound),
              ),
            if (matches != null)
              ...matches!.map(
                (match) => ListTile(
                  title: Text('${match.title} · ${match.artist}'),
                  subtitle: Text(
                    L10n.of(context).visualLyricsMatch(
                      match.album,
                      match.synced
                          ? L10n.of(context).visualSyncedLyrics
                          : L10n.of(context).visualPlainLyrics,
                      match.duration.round(),
                    ),
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
        child: Text(L10n.of(context).visualCancel),
      ),
      FilledButton(
        onPressed: busy ? null : search,
        child: Text(L10n.of(context).visualSearch),
      ),
    ],
  );
}
