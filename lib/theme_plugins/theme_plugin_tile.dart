import 'package:flutter/material.dart';
import '../appearance.dart';
import 'theme_plugin_controller.dart';

/// Theme selection is a shortcut to the same registry used by Plugin Library.
class ThemePluginTile extends StatelessWidget {
  const ThemePluginTile({
    super.key,
    required this.controller,
    required this.palette,
    required this.onSelected,
  });
  final ThemePluginController controller;
  final Palette palette;
  final VoidCallback onSelected;
  @override
  Widget build(BuildContext context) {
    final c = controller, p = palette;
    final zh = Localizations.localeOf(context).languageCode == 'zh';
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Text(
          zh ? '主题插件' : 'Theme plugins',
          style: TextStyle(color: p.ink, fontWeight: FontWeight.w600),
        ),
        const SizedBox(height: 8),
        if (c.entries.isEmpty)
          Text(
            zh
                ? '在插件管理中导入主题文件。主题可与下方风格叠加。'
                : 'Import a theme in Plugin Manager. Themes combine with the styles below.',
            style: TextStyle(color: p.muted, fontSize: 11, height: 1.5),
          ),
        for (final entry in c.entries)
          Padding(
            padding: const EdgeInsets.only(bottom: 8),
            child: OutlinedButton(
              key: ValueKey('theme-plugin-${entry.id}'),
              onPressed: c.busy || !entry.available || entry.enabled
                  ? null
                  : () async {
                      if (await c.activate(entry.id) && context.mounted) {
                        onSelected();
                      }
                    },
              child: Padding(
                padding: const EdgeInsets.symmetric(vertical: 10),
                child: Row(
                  children: [
                    Icon(
                      entry.enabled
                          ? Icons.check_circle_outline
                          : Icons.palette_outlined,
                      size: 18,
                    ),
                    const SizedBox(width: 8),
                    Expanded(
                      child: Text(
                        entry.name,
                        style: const TextStyle(fontSize: 11),
                      ),
                    ),
                  ],
                ),
              ),
            ),
          ),
        if (c.active) ...[
          Material(
            type: MaterialType.transparency,
            child: SwitchListTile.adaptive(
              key: const ValueKey('theme-plugin-full-override'),
              contentPadding: EdgeInsets.zero,
              title: Text(
                zh ? '主题材质与背景' : 'Theme materials & backdrop',
                style: TextStyle(color: p.ink, fontSize: 11),
              ),
              subtitle: Text(
                zh
                    ? '关闭以保留自选材质和背景；风格始终叠加。'
                    : 'Off keeps your materials and backdrop. Styles always combine.',
                style: TextStyle(color: p.muted, fontSize: 10),
              ),
              value: c.fullOverride,
              onChanged: c.busy ? null : c.setFullOverride,
            ),
          ),
          TextButton(
            key: const ValueKey('theme-plugin-deactivate'),
            onPressed: c.busy ? null : c.deactivate,
            child: Text(zh ? '停用主题' : 'Disable theme'),
          ),
        ],
        if (c.busy) const LinearProgressIndicator(minHeight: 2),
        if (c.error != null)
          Text(
            zh
                ? '主题未能加载或保存，请到插件管理检查后重试。'
                : 'Theme loading or saving failed. Check Plugin Manager and retry.',
            key: const ValueKey('theme-plugin-error'),
            style: TextStyle(
              color: Theme.of(context).colorScheme.error,
              fontSize: 11,
            ),
          ),
        const SizedBox(height: 16),
      ],
    );
  }
}
