import 'package:flutter/material.dart';
import '../appearance.dart';
import 'texture_source.dart';

class TextureLinkDialog extends StatefulWidget {
  const TextureLinkDialog({super.key});
  @override
  State<TextureLinkDialog> createState() => _TextureLinkDialogState();
}

class _TextureLinkDialogState extends State<TextureLinkDialog> {
  final address = TextEditingController();
  TextureKind kind = TextureKind.image;
  bool invalid = false;
  @override
  void dispose() {
    address.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => StudioDialog(
    title: '从外面带一点灵感',
    subtitle: '粘贴图片、GIF 或视频的 HTTP / HTTPS 直链。网页分享链接需要先找到原始媒体地址。',
    icon: Icons.add_link_rounded,
    content: Column(
      mainAxisSize: MainAxisSize.min,
      children: [
        TextField(
          key: const ValueKey('texture-url'),
          controller: address,
          autofocus: true,
          maxLength: 2048,
          onChanged: (value) {
            setState(() {
              invalid = false;
              kind = TextureSource.kindFor(Uri.tryParse(value)?.path ?? value);
              if (kind == TextureKind.audio) kind = TextureKind.video;
            });
          },
          decoration: InputDecoration(
            labelText: '媒体地址',
            hintText: 'https://…/background.mp4',
            errorText: invalid ? '请输入有效且不含登录信息的 HTTP / HTTPS 地址' : null,
          ),
        ),
        const SizedBox(height: 16),
        DropdownButtonFormField<TextureKind>(
          key: ValueKey(kind),
          initialValue: kind,
          decoration: const InputDecoration(labelText: '素材类型'),
          items: TextureKind.values
              .where((value) => value != TextureKind.audio)
              .map(
                (value) => DropdownMenuItem(
                  value: value,
                  child: Text(switch (value) {
                    TextureKind.image => '图片',
                    TextureKind.gif => 'GIF 动图',
                    TextureKind.video => '视频',
                    TextureKind.audio => '音频',
                  }),
                ),
              )
              .toList(),
          onChanged: (value) => setState(() => kind = value!),
        ),
        const SizedBox(height: 12),
        const Text(
          '视频默认静音循环播放，可在设置中开启声音。网络素材需允许访问，网页端还需支持跨域加载。',
          style: TextStyle(fontSize: 11, height: 1.7),
        ),
      ],
    ),
    actions: [
      TextButton(
        onPressed: () => Navigator.pop(context),
        child: const Text('取消'),
      ),
      FilledButton(
        onPressed: () {
          final value = address.text.trim();
          if (!TextureSource.validUrl(value)) {
            setState(() => invalid = true);
            return;
          }
          final uri = Uri.parse(value);
          Navigator.pop(
            context,
            TextureSource(
              location: value,
              name: uri.pathSegments.isEmpty ? uri.host : uri.pathSegments.last,
              kind: kind,
            ),
          );
        },
        child: const Text('应用素材'),
      ),
    ],
  );
}
