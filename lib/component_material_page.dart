import 'package:morrow_i18n/morrow_i18n.dart';
import 'package:flutter/material.dart';
import 'appearance.dart';
import 'color_compass.dart';
import 'settings_surface.dart';

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
  late SurfaceSettings surfaces = currentPalette.surfaces;
  Palette get currentPalette =>
      AppearanceScope.maybeOf(context) ?? widget.palette;
  SurfaceSettings get activeSurfaces =>
      AppearanceScope.maybeOf(context)?.surfaces ?? surfaces;
  Future<void> edit(String id, String title) async {
    // Legacy global values seed new entries; they never override other cards.
    final initial =
        activeSurfaces.components[id] ??
        ComponentMaterial(
          blur: activeSurfaces.componentCustom
              ? activeSurfaces.componentBlur
              : currentPalette.clear
              ? 1
              : currentPalette.liquid
              ? 6
              : 22,
          opacity: activeSurfaces.componentCustom
              ? activeSurfaces.componentOpacity
              : currentPalette.clear
              ? .10
              : currentPalette.liquid
              ? .18
              : currentPalette.frostedOpacity,
          color: activeSurfaces.componentColor,
        );
    final result = await Navigator.of(context).push<ComponentMaterial>(
      CanvasSettingsRoute<ComponentMaterial>(
        builder: (_) => ComponentMaterialPage(
          palette: currentPalette.withSurfaces(activeSurfaces),
          id: id,
          title: title,
          initial: initial,
        ),
      ),
    );
    if (!mounted || result == null) return;
    setState(
      () => surfaces = activeSurfaces.copyWith(
        components: {...activeSurfaces.components, id: result},
      ),
    );
    widget.onChanged(surfaces);
  }

  @override
  Widget build(BuildContext context) {
    final p = currentPalette.withSurfaces(activeSurfaces);
    final entries = widget.entries.entries.toList(growable: false);
    return AppearanceScope(
      palette: p,
      child: SettingsSurface(
        title: L10n.of(context).visualComponents,
        child: ListView.builder(
          key: const PageStorageKey('component-list-scroll'),
          padding: const EdgeInsets.only(bottom: 20),
          itemCount: entries.length + 1,
          itemBuilder: (context, index) {
            if (index == 0) {
              return Padding(
                padding: const EdgeInsets.only(bottom: 20),
                child: Text(
                  L10n.of(context).visualComponentsGuide,
                  style: TextStyle(color: p.muted, height: 1.6),
                ),
              );
            }
            final entry = entries[index - 1];
            return Padding(
              key: ValueKey(entry.key),
              padding: const EdgeInsets.only(bottom: 20),
              child: Glass(
                p: p,
                radius: 16,
                child: SettingsNavigationFeedback(
                  child: ListTile(
                    key: ValueKey('component-entry:${entry.key}'),
                    contentPadding: const EdgeInsets.symmetric(
                      horizontal: 20,
                      vertical: 8,
                    ),
                    title: Text(
                      entry.value,
                      maxLines: 2,
                      overflow: TextOverflow.ellipsis,
                      style: TextStyle(color: p.ink),
                    ),
                    subtitle: Text(
                      activeSurfaces.components[entry.key]?.enabled == true
                          ? L10n.of(context).visualIndependentMaterial
                          : entry.key == 'footer'
                          ? L10n.of(context).visualTransparentTips
                          : L10n.of(context).visualFollowTheme,
                      style: TextStyle(color: p.muted),
                    ),
                    trailing: const Icon(Icons.chevron_right),
                    onTap: () => edit(entry.key, entry.value),
                  ),
                ),
              ),
            );
          },
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
  Palette get currentPalette =>
      AppearanceScope.maybeOf(context) ?? widget.palette;
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
        onChanged: value.enabled ? (v) => setState(() => change(v)) : null,
      ),
    ],
  );
  Future<void> color() async {
    final chosen = await showStudioDialog<Color>(
      context: context,
      builder: (_) => ColorCompassDialog(
        title: L10n.of(context).visualComponentCompass(widget.title),
        initial: value.color ?? currentPalette.surface,
      ),
    );
    if (mounted && chosen != null) {
      setState(() => value = value.copyWith(color: chosen));
    }
  }

  Widget section(Palette p, List<Widget> children) => Glass(
    p: p,
    child: Padding(
      padding: const EdgeInsets.all(24),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: children,
      ),
    ),
  );

  @override
  Widget build(BuildContext context) {
    final l = L10n.of(context);
    final p = currentPalette.withSurfaces(
      currentPalette.surfaces.copyWith(
        components: {...currentPalette.surfaces.components, widget.id: value},
      ),
    );
    final previewContent = Center(
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          Icon(Icons.auto_awesome_outlined, color: p.accent),
          const SizedBox(height: 12),
          Text(l.visualMaterialPreview, style: TextStyle(color: p.ink)),
          const SizedBox(height: 8),
          Text(widget.title, style: TextStyle(color: p.muted)),
        ],
      ),
    );
    return AppearanceScope(
      palette: p,
      child: SettingsSurface(
        title: widget.title,
        child: SingleChildScrollView(
          padding: const EdgeInsets.only(bottom: 20),
          child: SettingsSections(
            children: [
              section(p, [
                SizedBox(
                  height: 210,
                  child: Stack(
                    fit: StackFit.expand,
                    children: [
                      DecoratedBox(
                        decoration: BoxDecoration(
                          borderRadius: p.borderRadius(20),
                          gradient: LinearGradient(
                            colors: [
                              p.accent.withValues(alpha: .35),
                              p.background,
                            ],
                          ),
                        ),
                      ),
                      Padding(
                        padding: const EdgeInsets.all(20),
                        child: widget.id == 'footer' && !value.enabled
                            ? previewContent
                            : Glass(
                                key: const ValueKey('component-preview'),
                                componentId: widget.id,
                                p: p,
                                child: previewContent,
                              ),
                      ),
                    ],
                  ),
                ),
                const SizedBox(height: 16),
                SwitchListTile.adaptive(
                  key: const ValueKey('component-custom-toggle'),
                  contentPadding: EdgeInsets.zero,
                  title: Text(l.visualUseCustomMaterial),
                  subtitle: Text(
                    widget.id == 'footer'
                        ? l.visualTipsMaterialGuide
                        : l.visualCustomMaterialGuide,
                  ),
                  value: value.enabled,
                  onChanged: (enabled) =>
                      setState(() => value = value.copyWith(enabled: enabled)),
                ),
                const SizedBox(height: 16),
                FilledButton(
                  key: const ValueKey('component-apply'),
                  onPressed: () => Navigator.of(context).pop(value),
                  child: Text(l.visualApplyComponent),
                ),
                const SizedBox(height: 8),
                OutlinedButton.icon(
                  key: const ValueKey('component-reset'),
                  onPressed: () =>
                      setState(() => value = const ComponentMaterial()),
                  icon: const Icon(Icons.restart_alt),
                  label: Text(l.visualResetMaterial),
                ),
                TextButton(
                  onPressed: () => Navigator.of(context).pop(),
                  child: Text(l.visualCancel),
                ),
              ]),
              section(p, [
                Text(
                  l.mainGlassTexture,
                  style: TextStyle(color: p.ink, fontWeight: FontWeight.w600),
                ),
                const SizedBox(height: 16),
                // Keep controls mounted during toggles/resizes, and visibly disable
                // them while the component follows the current theme.
                AnimatedOpacity(
                  duration: motionDuration(context, 240),
                  opacity: value.enabled ? 1 : .45,
                  child: IgnorePointer(
                    ignoring: !value.enabled,
                    child: ExcludeFocus(
                      excluding: !value.enabled,
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.stretch,
                        children: [
                          Wrap(
                            spacing: 8,
                            runSpacing: 8,
                            children: [
                              for (final mode in <GlassMode?>[
                                null,
                                ...GlassMode.values,
                              ])
                                ChoiceChip(
                                  key: ValueKey(
                                    'component-mode-${mode?.name ?? 'inherit'}',
                                  ),
                                  label: Text(switch (mode) {
                                    null => l.visualFollowTheme,
                                    GlassMode.frosted => l.mainFrosted,
                                    GlassMode.clear => l.mainCrystal,
                                    GlassMode.liquid => l.mainLiquidGlass,
                                  }),
                                  selected: value.mode == mode,
                                  onSelected: value.enabled
                                      ? (_) => setState(() {
                                          value = value.copyWith(
                                            mode: mode,
                                            inheritMode: mode == null,
                                            blur: switch (mode) {
                                              GlassMode.clear => 1,
                                              GlassMode.liquid => 6,
                                              GlassMode.frosted => 22,
                                              null => value.blur,
                                            },
                                            opacity: switch (mode) {
                                              GlassMode.clear => .10,
                                              GlassMode.liquid => .18,
                                              GlassMode.frosted =>
                                                currentPalette.frostedOpacity,
                                              null => value.opacity,
                                            },
                                          );
                                        })
                                      : null,
                                ),
                            ],
                          ),
                          const SizedBox(height: 20),
                          slider(
                            'component-blur',
                            l.visualFrosting,
                            value.blur,
                            40,
                            (v) => value = value.copyWith(blur: v),
                          ),
                          const SizedBox(height: 12),
                          slider(
                            'component-opacity',
                            l.visualOpacity,
                            value.opacity,
                            1,
                            (v) => value = value.copyWith(opacity: v),
                          ),
                          const SizedBox(height: 12),
                          slider(
                            'component-radius',
                            l.mainCornerRadius,
                            value.cornerRadius ?? p.cornerRadius,
                            32,
                            (v) => value = value.copyWith(cornerRadius: v),
                          ),
                          Align(
                            alignment: Alignment.centerRight,
                            child: TextButton(
                              key: const ValueKey('component-radius-inherit'),
                              onPressed:
                                  value.enabled && value.cornerRadius != null
                                  ? () => setState(
                                      () => value = value.copyWith(
                                        inheritRadius: true,
                                      ),
                                    )
                                  : null,
                              child: Text(l.visualFollowTheme),
                            ),
                          ),
                          const SizedBox(height: 12),
                          OutlinedButton.icon(
                            key: const ValueKey('component-color'),
                            onPressed: value.enabled ? color : null,
                            icon: Icon(
                              Icons.palette_outlined,
                              size: 16,
                              color: value.color ?? p.accent,
                            ),
                            label: Text(l.visualCustomCompass),
                          ),
                          TextButton(
                            key: const ValueKey('component-color-inherit'),
                            onPressed: value.enabled && value.color != null
                                ? () => setState(
                                    () => value = value.copyWith(
                                      inheritColor: true,
                                    ),
                                  )
                                : null,
                            child: Text(l.visualInheritColor),
                          ),
                        ],
                      ),
                    ),
                  ),
                ),
              ]),
            ],
          ),
        ),
      ),
    );
  }
}
