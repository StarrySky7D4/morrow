import 'package:morrow_i18n/morrow_i18n.dart';
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
    title: L10n.of(context).visualTextureLinkTitle,
    subtitle: L10n.of(context).visualTextureLinkGuide,
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
            labelText: L10n.of(context).visualMediaAddress,
            hintText: 'https://…/background.mp4',
            errorText: invalid
                ? L10n.of(context).visualMediaAddressInvalid
                : null,
          ),
        ),
        const SizedBox(height: 16),
        DropdownButtonFormField<TextureKind>(
          key: ValueKey(kind),
          initialValue: kind,
          decoration: InputDecoration(
            labelText: L10n.of(context).visualMediaType,
          ),
          items: TextureKind.values
              .where(
                (value) =>
                    value != TextureKind.audio && value != TextureKind.file,
              )
              .map(
                (value) => DropdownMenuItem(
                  value: value,
                  child: Text(switch (value) {
                    TextureKind.image => L10n.of(context).visualImage,
                    TextureKind.gif => L10n.of(context).visualGif,
                    TextureKind.video => L10n.of(context).visualVideo,
                    TextureKind.audio => L10n.of(context).visualAudio,
                    TextureKind.file => L10n.of(context).visualFile,
                  }),
                ),
              )
              .toList(),
          onChanged: (value) => setState(() => kind = value!),
        ),
        const SizedBox(height: 12),
        Text(
          L10n.of(context).visualTexturePlaybackGuide,
          style: TextStyle(fontSize: 11, height: 1.7),
        ),
      ],
    ),
    actions: [
      TextButton(
        onPressed: () => Navigator.pop(context),
        child: Text(L10n.of(context).visualCancel),
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
        child: Text(L10n.of(context).visualApplyTexture),
      ),
    ],
  );
}
