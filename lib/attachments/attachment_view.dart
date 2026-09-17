import 'package:morrow_i18n/morrow_i18n.dart';
import 'dart:async';
import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:media_kit/media_kit.dart';
import 'package:media_kit_video/media_kit_video.dart';
import '../appearance.dart';
import '../media/texture_repository.dart';
import '../media/texture_source.dart';
import 'attachment.dart';
import 'file_access_native.dart'
    if (dart.library.js_interop) 'file_access_web.dart';

class AttachmentTile extends StatelessWidget {
  const AttachmentTile({super.key, required this.attachment, this.onRemove});
  final IdeaAttachment attachment;
  final VoidCallback? onRemove;
  Future<void> action(BuildContext context, Future<void> Function() run) async {
    try {
      await run();
    } catch (error) {
      if (context.mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(
            content: Text(switch (error) {
              FormatException(message: '文件操作失败，请重试。') => L10n.of(
                context,
              ).visualFileRetry,
              FormatException(message: '无法打开文件，请先安装对应应用，或将附件另存后打开。') => L10n.of(
                context,
              ).visualFileOpenFailure,
              _ => L10n.of(context).visualAttachmentFailure,
            }),
          ),
        );
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    final p = AppearanceScope.of(context);
    final kind = attachment.source.kind;
    final icon = switch (kind) {
      TextureKind.image || TextureKind.gif => Icons.image_outlined,
      TextureKind.video => Icons.movie_outlined,
      TextureKind.audio => Icons.audiotrack_outlined,
      TextureKind.file =>
        [
              'blend',
              'obj',
              'fbx',
              'stl',
              'dwg',
              'dxf',
              'step',
              'stp',
            ].contains(attachment.extension)
            ? Icons.view_in_ar_outlined
            : Icons.insert_drive_file_outlined,
    };
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 4),
      child: Material(
        color: p.surface.withValues(alpha: .16),
        shape: RoundedRectangleBorder(
          borderRadius: p.borderRadius(12),
          side: BorderSide(color: p.line),
        ),
        child: ListTile(
          contentPadding: const EdgeInsets.symmetric(horizontal: 10),
          leading: Icon(icon, color: p.accent),
          title: Text(
            attachment.source.name,
            maxLines: 2,
            overflow: TextOverflow.ellipsis,
            style: TextStyle(fontSize: 12, color: p.ink),
          ),
          subtitle: Text(
            L10n.of(context).visualAttachmentDetails(
              kind == TextureKind.file
                  ? (kIsWeb
                        ? L10n.of(context).visualDownloadOpen
                        : L10n.of(context).visualDefaultOpen)
                  : L10n.of(context).visualClickPreview,
              attachment.extension.toUpperCase(),
              attachment.sizeLabel,
            ),
            style: TextStyle(fontSize: 10, color: p.muted),
          ),
          onTap: () => attachment.previewable
              ? showStudioDialog<void>(
                  context: context,
                  builder: (_) => AttachmentPreview(attachment: attachment),
                )
              : action(context, () => openAttachment(attachment)),
          trailing: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              IconButton(
                tooltip: L10n.of(context).visualSaveAttachment,
                icon: const Icon(Icons.download_outlined, size: 18),
                onPressed: () =>
                    action(context, () => exportAttachment(attachment)),
              ),
              if (onRemove != null)
                IconButton(
                  tooltip: L10n.of(context).visualRemoveAttachment,
                  icon: const Icon(Icons.close, size: 18),
                  onPressed: onRemove,
                ),
            ],
          ),
        ),
      ),
    );
  }
}

class AttachmentPreview extends StatefulWidget {
  const AttachmentPreview({super.key, required this.attachment});
  final IdeaAttachment attachment;
  @override
  State<AttachmentPreview> createState() => _AttachmentPreviewState();
}

enum _PreviewFailure { media, read }

class _AttachmentPreviewState extends State<AttachmentPreview> {
  ResolvedTexture? resolved;
  Player? player;
  VideoController? video;
  StreamSubscription<String>? errors;
  _PreviewFailure? error;
  @override
  void initState() {
    super.initState();
    load();
  }

  Future<void> load() async {
    try {
      final data = await TextureRepository.resolve(widget.attachment.source);
      if (!mounted) {
        data.release?.call();
        return;
      }
      resolved = data;
      if ([
        TextureKind.audio,
        TextureKind.video,
      ].contains(widget.attachment.source.kind)) {
        MediaKit.ensureInitialized();
        final engine = player = Player();
        video = VideoController(engine);
        errors = engine.stream.error.listen((_) {
          if (mounted) setState(() => error = _PreviewFailure.media);
        });
        await engine.open(Media(data.uri), play: false);
      }
      if (mounted) setState(() {});
    } catch (_) {
      if (mounted) setState(() => error = _PreviewFailure.read);
    }
  }

  @override
  void dispose() {
    errors?.cancel();
    final data = resolved;
    final engine = player;
    if (engine != null) {
      engine.dispose().whenComplete(() => data?.release?.call());
    } else {
      data?.release?.call();
    }
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => StudioDialog(
    title: widget.attachment.source.name,
    subtitle: L10n.of(context).visualAttachmentPreview,
    content: SizedBox(
      width: 620,
      height: 320,
      child: error != null
          ? Center(
              child: Text(
                error == _PreviewFailure.media
                    ? L10n.of(context).visualMediaPreviewFailure
                    : L10n.of(context).visualAttachmentReadFailure,
              ),
            )
          : resolved == null
          ? const Center(child: CircularProgressIndicator())
          : video != null
          ? Video(controller: video!, controls: MaterialVideoControls)
          : InteractiveViewer(
              child: Image.memory(
                resolved!.bytes!,
                fit: BoxFit.contain,
                errorBuilder: (_, _, _) => Center(
                  child: Text(L10n.of(context).visualImageDecodeFailure),
                ),
              ),
            ),
    ),
    actions: [
      TextButton(
        onPressed: () => Navigator.pop(context),
        child: Text(L10n.of(context).visualClose),
      ),
    ],
  );
}
