import 'package:flutter/material.dart';
import 'theme_plugin_controller.dart';
import 'ui_theme_tokens.dart';

/// Generic host rendering only. All themed illustration pixels come from the package.
class ThemeArtwork extends StatelessWidget {
  const ThemeArtwork({super.key, required this.plugin, this.fade = true});
  final UiThemePlugin plugin;
  final bool fade;
  @override
  Widget build(BuildContext context) {
    if (plugin.artwork == null) return const SizedBox.shrink();
    Widget image = Image.memory(
      plugin.artwork!,
      fit: BoxFit.cover,
      alignment: Alignment.centerRight,
      gaplessPlayback: true,
      filterQuality: FilterQuality.medium,
      excludeFromSemantics: true,
      errorBuilder: (_, _, _) => const SizedBox.shrink(),
    );
    if (fade) {
      image = ShaderMask(
        blendMode: BlendMode.dstIn,
        shaderCallback: (bounds) => const LinearGradient(
          colors: [Colors.transparent, Colors.white, Colors.white],
          stops: [0, .28, 1],
        ).createShader(bounds),
        child: image,
      );
    }
    return IgnorePointer(child: ExcludeSemantics(child: image));
  }
}

class ThemeCanvas extends StatelessWidget {
  const ThemeCanvas({super.key, required this.tokens});
  final UiThemeTokens tokens;
  @override
  Widget build(BuildContext context) => IgnorePointer(
    child: DecoratedBox(
      decoration: BoxDecoration(
        gradient: LinearGradient(
          begin: Alignment.topLeft,
          end: Alignment.bottomRight,
          colors: [
            tokens.background,
            Color.lerp(tokens.background, tokens.secondary, .05)!,
          ],
        ),
      ),
    ),
  );
}
