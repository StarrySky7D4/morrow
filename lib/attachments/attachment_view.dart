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
            content: Text(
              error is FormatException ? error.message : '文件操作失败，请检查文件与存储空间。',
            ),
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
            '${attachment.extension.toUpperCase()} · ${attachment.sizeLabel}${kind == TextureKind.file ? (kIsWeb ? ' · 下载后打开' : ' · 使用默认应用打开') : ' · 点击预览'}',
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
                tooltip: '另存附件',
                icon: const Icon(Icons.download_outlined, size: 18),
                onPressed: () =>
                    action(context, () => exportAttachment(attachment)),
              ),
              if (onRemove != null)
                IconButton(
                  tooltip: '移除附件',
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

class _AttachmentPreviewState extends State<AttachmentPreview> {
  ResolvedTexture? resolved;
  Player? player;
  VideoController? video;
  StreamSubscription<String>? errors;
  String? error;
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
          if (mounted) setState(() => error = '此媒体无法预览，可另存后使用其他应用打开。');
        });
        await engine.open(Media(data.uri), play: false);
      }
      if (mounted) setState(() {});
    } catch (_) {
      if (mounted) setState(() => error = '附件无法读取，请重新导入。');
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
    subtitle: '本地附件预览',
    content: SizedBox(
      width: 620,
      height: 320,
      child: error != null
          ? Center(child: Text(error!))
          : resolved == null
          ? const Center(child: CircularProgressIndicator())
          : video != null
          ? Video(controller: video!, controls: MaterialVideoControls)
          : InteractiveViewer(
              child: Image.memory(
                resolved!.bytes!,
                fit: BoxFit.contain,
                errorBuilder: (_, _, _) =>
                    const Center(child: Text('图片无法解码，可另存后打开。')),
              ),
            ),
    ),
    actions: [
      TextButton(
        onPressed: () => Navigator.pop(context),
        child: const Text('关闭'),
      ),
    ],
  );
}
