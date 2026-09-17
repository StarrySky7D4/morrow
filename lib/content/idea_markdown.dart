import 'package:morrow_i18n/morrow_i18n.dart';
import 'package:flutter/material.dart';
import 'package:flutter_markdown_plus/flutter_markdown_plus.dart';
import 'package:url_launcher/url_launcher.dart';
import '../appearance.dart';
import '../attachments/attachment.dart';
import '../media/texture_repository.dart';
import '../media/texture_source.dart';
import 'rich_content.dart';

class IdeaMarkdown extends StatelessWidget {
  const IdeaMarkdown({
    super.key,
    required this.data,
    this.attachments = const [],
  });
  final String data;
  final List<IdeaAttachment> attachments;
  @override
  Widget build(BuildContext context) {
    final p = AppearanceScope.of(context);
    return MarkdownBody(
      data: data,
      selectable: true,
      softLineBreak: true,
      styleSheet: MarkdownStyleSheet.fromTheme(Theme.of(context)).copyWith(
        p: TextStyle(color: p.ink, height: 1.8, fontSize: 13),
        h1: TextStyle(color: p.ink, fontSize: 25, fontWeight: FontWeight.w700),
        h2: TextStyle(color: p.ink, fontSize: 21, fontWeight: FontWeight.w600),
        code: TextStyle(color: p.accent, fontFamily: 'Consolas', fontSize: 12),
        codeblockDecoration: BoxDecoration(
          color: p.ink.withValues(alpha: .045),
          borderRadius: p.borderRadius(12),
        ),
        blockquoteDecoration: BoxDecoration(
          color: p.accent.withValues(alpha: .065),
          border: Border(left: BorderSide(color: p.accent, width: 3)),
        ),
        tableBorder: TableBorder.all(color: p.line),
        tableCellsPadding: const EdgeInsets.all(9),
      ),
      onTapLink: (_, href, _) async {
        if (href == null) return;
        final safe = safeContentLink(href);
        if (safe == null || safe.startsWith('attachment:')) return;
        try {
          if (!await launchUrl(
            Uri.parse(safe),
            mode: LaunchMode.externalApplication,
          )) {
            throw const FormatException('Link launch failed');
          }
        } catch (_) {
          if (context.mounted) {
            ScaffoldMessenger.of(context).showSnackBar(
              SnackBar(content: Text(L10n.of(context).visualLinkFailure)),
            );
          }
        }
      },
      imageBuilder: (uri, _, alt) {
        if (uri.scheme == 'attachment') {
          final name = uri.pathSegments.join('/');
          final matches = attachments.where(
            (a) =>
                (a.source.name == name ||
                    a.source.location == name ||
                    a.pluginId == name) &&
                [TextureKind.image, TextureKind.gif].contains(a.source.kind),
          );
          if (matches.isNotEmpty) {
            return _InlineAttachment(
              key: ValueKey(matches.first.source.location),
              attachment: matches.first,
            );
          }
        }
        return _LinkedImage(
          key: ValueKey(uri.toString()),
          uri: uri,
          alt: alt ?? L10n.of(context).visualImage,
        );
      },
    );
  }
}

class _LinkedImage extends StatefulWidget {
  const _LinkedImage({super.key, required this.uri, required this.alt});
  final Uri uri;
  final String alt;
  @override
  State<_LinkedImage> createState() => _LinkedImageState();
}

class _LinkedImageState extends State<_LinkedImage> {
  bool load = false;
  @override
  Widget build(BuildContext context) {
    final safe = safeContentLink(widget.uri.toString());
    final remote =
        safe != null && ['http', 'https'].contains(widget.uri.scheme);
    if (load && remote) {
      return Image.network(
        safe,
        fit: BoxFit.contain,
        height: 220,
        errorBuilder: (_, _, _) =>
            Text(L10n.of(context).visualImageLoadFailure(widget.alt)),
      );
    }
    // Reading arbitrary local paths and silently loading tracking images are avoided.
    return OutlinedButton.icon(
      onPressed: remote ? () => setState(() => load = true) : null,
      icon: const Icon(Icons.image_outlined, size: 17),
      label: Text(
        remote
            ? L10n.of(context).visualLoadImage(widget.alt)
            : L10n.of(context).visualImageNotImported(widget.alt),
      ),
    );
  }
}

class _InlineAttachment extends StatefulWidget {
  const _InlineAttachment({super.key, required this.attachment});
  final IdeaAttachment attachment;
  @override
  State<_InlineAttachment> createState() => _InlineAttachmentState();
}

class _InlineAttachmentState extends State<_InlineAttachment> {
  ResolvedTexture? resolved;
  bool failed = false;
  @override
  void initState() {
    super.initState();
    _load();
  }

  Future<void> _load() async {
    try {
      final result = await TextureRepository.resolve(widget.attachment.source);
      if (!mounted) {
        result.release?.call();
        return;
      }
      setState(() => resolved = result);
    } catch (_) {
      if (mounted) setState(() => failed = true);
    }
  }

  @override
  void dispose() {
    resolved?.release?.call();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    if (failed) {
      return Text(
        L10n.of(context).visualImageUnavailable(widget.attachment.source.name),
      );
    }
    if (resolved == null) {
      return const SizedBox(
        height: 40,
        child: Center(child: CircularProgressIndicator()),
      );
    }
    final result = resolved!;
    Widget error(BuildContext context, Object error, StackTrace? stack) => Text(
      L10n.of(context).visualImageUnavailable(widget.attachment.source.name),
    );
    return ConstrainedBox(
      constraints: const BoxConstraints(maxHeight: 300),
      child: result.bytes != null
          ? Image.memory(
              result.bytes!,
              fit: BoxFit.contain,
              errorBuilder: error,
            )
          : Image.network(result.uri, fit: BoxFit.contain, errorBuilder: error),
    );
  }
}
