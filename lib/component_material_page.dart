import 'package:flutter/material.dart';
import 'appearance.dart';
import 'color_compass.dart';
import 'desktop_frame.dart';

class ComponentMaterialListPage extends StatefulWidget {
  const ComponentMaterialListPage({
    super.key,
    required this.palette,
    required this.entries,
    required this.onChanged,
  });
  final Palette palette;
  final Map<String, String> entries;
  final ValueChanged<SurfaceSettings> onChanged;
  @override
  State<ComponentMaterialListPage> createState() =>
      _ComponentMaterialListPageState();
}

class _ComponentMaterialListPageState extends State<ComponentMaterialListPage> {
  late SurfaceSettings surfaces = widget.palette.surfaces;
  Future<void> edit(String id, String title) async {
    // Legacy global values seed new entries; they never override other cards.
    final initial =
        surfaces.components[id] ??
        ComponentMaterial(
          blur: surfaces.componentBlur,
          opacity: surfaces.componentOpacity,
          color: surfaces.componentColor ?? widget.palette.surface,
        );
    final result = await Navigator.of(context).push<ComponentMaterial>(
      MaterialPageRoute(
        builder: (_) => ComponentMaterialPage(
          palette: widget.palette.withSurfaces(surfaces),
          id: id,
          title: title,
          initial: initial,
        ),
      ),
    );
    if (!mounted || result == null) return;
    setState(
      () => surfaces = surfaces.copyWith(
        components: {...surfaces.components, id: result},
      ),
    );
    widget.onChanged(surfaces);
  }

  @override
  Widget build(BuildContext context) {
    final p = widget.palette;
    return _captionSafe(
      context,
      Scaffold(
        backgroundColor: p.background,
        appBar: AppBar(
          title: const Text('组件与卡片'),
          backgroundColor: p.background,
        ),
        body: Center(
          child: ConstrainedBox(
            constraints: const BoxConstraints(maxWidth: 680),
            child: ListView(
              padding: const EdgeInsets.all(20),
              children: [
                Text(
                  '每项单独设置，默认跟随主题。修改一张卡片不会影响其他卡片。',
                  style: TextStyle(color: p.muted, height: 1.6),
                ),
                const SizedBox(height: 16),
                for (final entry in widget.entries.entries)
                  ListTile(
                    key: ValueKey('component-entry:${entry.key}'),
                    title: Text(
                      entry.value,
                      maxLines: 2,
                      overflow: TextOverflow.ellipsis,
                    ),
                    subtitle: Text(
                      surfaces.components[entry.key]?.enabled == true
                          ? '独立材质'
                          : '跟随主题',
                    ),
                    trailing: const Icon(Icons.chevron_right),
                    onTap: () => edit(entry.key, entry.value),
                  ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

class ComponentMaterialPage extends StatefulWidget {
  const ComponentMaterialPage({
    super.key,
    required this.palette,
    required this.id,
    required this.title,
    required this.initial,
  });
  final Palette palette;
  final String id, title;
  final ComponentMaterial initial;
  @override
  State<ComponentMaterialPage> createState() => _ComponentMaterialPageState();
}

class _ComponentMaterialPageState extends State<ComponentMaterialPage> {
  late ComponentMaterial value = widget.initial;
  Widget slider(
    String key,
    String label,
    double current,
    double max,
    ValueChanged<double> change,
  ) => Column(
    children: [
      Row(
        children: [
          Text(label),
          const Spacer(),
          Text('${(current / max * 100).round()}%'),
        ],
      ),
      Slider(
        key: ValueKey(key),
        value: current,
        max: max,
        divisions: 100,
        onChanged: (v) => setState(() => change(v)),
      ),
    ],
  );
  Future<void> color() async {
    final chosen = await showStudioDialog<Color>(
      context: context,
      builder: (_) => ColorCompassDialog(
        title: '${widget.title} · 调色罗盘',
        initial: value.color ?? widget.palette.surface,
      ),
    );
    if (mounted && chosen != null) {
      setState(() => value = value.copyWith(color: chosen));
    }
  }

  @override
  Widget build(BuildContext context) {
    final p = widget.palette.withSurfaces(
      widget.palette.surfaces.copyWith(
        components: {...widget.palette.surfaces.components, widget.id: value},
      ),
    );
    return AppearanceScope(
      palette: p,
      child: _captionSafe(
        context,
        Scaffold(
          backgroundColor: p.background,
          appBar: AppBar(
            title: Text(widget.title),
            backgroundColor: p.background,
          ),
          body: Center(
            child: ConstrainedBox(
              constraints: const BoxConstraints(maxWidth: 560),
              child: ListView(
                padding: const EdgeInsets.all(24),
                children: [
                  SizedBox(
                    height: 130,
                    child: Stack(
                      fit: StackFit.expand,
                      children: [
                        DecoratedBox(
                          decoration: BoxDecoration(
                            borderRadius: p.borderRadius(20),
                            gradient: LinearGradient(
                              colors: [
                                p.accent.withValues(alpha: .25),
                                p.background,
                              ],
                            ),
                          ),
                        ),
                        Padding(
                          padding: const EdgeInsets.all(20),
                          child: Glass(
                            key: const ValueKey('component-preview'),
                            componentId: widget.id,
                            p: p,
                            child: Center(
                              child: Text(
                                '材质预览',
                                style: TextStyle(color: p.ink),
                              ),
                            ),
                          ),
                        ),
                      ],
                    ),
                  ),
                  const SizedBox(height: 16),
                  SwitchListTile.adaptive(
                    key: const ValueKey('component-custom-toggle'),
                    contentPadding: EdgeInsets.zero,
                    title: const Text('使用自定义材质'),
                    subtitle: const Text('关闭后跟随主题，保留本项自定义参数'),
                    value: value.enabled,
                    onChanged: (enabled) => setState(
                      () => value = value.copyWith(enabled: enabled),
                    ),
                  ),
                  AnimatedSize(
                    duration: motionDuration(context, 240),
                    child: value.enabled
                        ? Column(
                            crossAxisAlignment: CrossAxisAlignment.stretch,
                            children: [
                              slider(
                                'component-blur',
                                '磨砂效果',
                                value.blur,
                                40,
                                (v) => value = value.copyWith(blur: v),
                              ),
                              slider(
                                'component-opacity',
                                '不透明度',
                                value.opacity,
                                1,
                                (v) => value = value.copyWith(opacity: v),
                              ),
                              OutlinedButton.icon(
                                key: const ValueKey('component-color'),
                                onPressed: color,
                                icon: const Icon(
                                  Icons.palette_outlined,
                                  size: 16,
                                ),
                                label: const Text('调色罗盘 · 自定义'),
                              ),
                            ],
                          )
                        : const SizedBox.shrink(),
                  ),
                  const SizedBox(height: 24),
                  FilledButton(
                    key: const ValueKey('component-apply'),
                    onPressed: () => Navigator.of(context).pop(value),
                    child: const Text('应用到此组件'),
                  ),
                  TextButton(
                    onPressed: () => Navigator.of(context).pop(),
                    child: const Text('取消'),
                  ),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }
}

Widget _captionSafe(BuildContext context, Widget child) => Padding(
  padding: EdgeInsets.only(
    top: context.findAncestorWidgetOfExactType<DesktopFrame>() == null ? 0 : 32,
  ),
  child: child,
);
