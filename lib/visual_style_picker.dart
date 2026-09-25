import 'theme_plugins/theme_plugin_controller.dart';
import 'theme_plugins/theme_plugin_tile.dart';
import 'package:flutter/material.dart';
import 'package:morrow_i18n/morrow_i18n.dart';
import 'appearance.dart';
import 'collapsible_panel.dart';
import 'neumorphic_controls.dart';

/// Keeps the selected style visible while the choices stay collapsed.
class VisualStylePicker extends StatefulWidget {
  const VisualStylePicker({
    super.key,
    required this.palette,
    required this.onChanged,
  });

  final Palette palette;
  final ValueChanged<VisualStyle> onChanged;

  @override
  State<VisualStylePicker> createState() => _VisualStylePickerState();
}

class _VisualStylePickerState extends State<VisualStylePicker> {
  bool expanded = false;

  @override
  Widget build(BuildContext context) {
    final l = L10n.of(context);
    final p = widget.palette;
    final selected = p.surfaces.visualStyle;
    final plugins = ThemePluginScope.of(context);
    final pluginActive = plugins?.active == true;
    final pluginTitle = plugins?.plugin?.name(Localizations.localeOf(context));
    String title(VisualStyle style) => switch (style) {
      VisualStyle.flat => l.mainStyleFlat,
      VisualStyle.neumorphism => l.mainStyleNeumorphism,
      VisualStyle.paper => l.mainStylePaper,
      VisualStyle.clay => l.mainStyleClay,
      VisualStyle.fluent => l.mainStyleFluent,
      VisualStyle.brutalist => l.mainStyleBrutalist,
      VisualStyle.industrial => l.mainStyleIndustrial,
    };
    String description(VisualStyle style) => switch (style) {
      VisualStyle.flat => l.mainStyleFlatDescription,
      VisualStyle.neumorphism => l.mainStyleNeumorphismDescription,
      VisualStyle.paper => l.mainStylePaperDescription,
      VisualStyle.clay => l.mainStyleClayDescription,
      VisualStyle.fluent => l.mainStyleFluentDescription,
      VisualStyle.brutalist => l.mainStyleBrutalistDescription,
      VisualStyle.industrial => l.mainStyleIndustrialDescription,
    };
    bool experimental(VisualStyle style) =>
        style != VisualStyle.flat && style != VisualStyle.neumorphism;
    String styleTitle(VisualStyle style) => experimental(style)
        ? '${title(style)} · ${l.mainStyleExperimental}'
        : title(style);
    String semanticTitle(VisualStyle style) => experimental(style)
        ? '${title(style)}, ${l.mainStyleExperimental}'
        : title(style);
    void toggle() => setState(() => expanded = !expanded);
    void choose(VisualStyle style) {
      widget.onChanged(style);
      setState(() => expanded = false);
    }

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Semantics(
          label:
              '${l.mainVisualStyle}: ${pluginActive ? '$pluginTitle + ${semanticTitle(selected)}' : semanticTitle(selected)}',
          expanded: expanded,
          button: true,
          onTap: toggle,
          child: ExcludeSemantics(
            child: NeumorphicSurface(
              palette: p,
              depth: expanded ? -0.55 : 0.55,
              borderRadius: p.borderRadius(13),
              child: Material(
                color: Colors.transparent,
                child: InkWell(
                  key: const ValueKey('visual-style-toggle'),
                  borderRadius: p.borderRadius(13),
                  onTap: toggle,
                  child: Padding(
                    padding: const EdgeInsets.symmetric(
                      horizontal: 12,
                      vertical: 13,
                    ),
                    child: Row(
                      children: [
                        Icon(Icons.style_outlined, size: 19, color: p.accent),
                        const SizedBox(width: 12),
                        Expanded(
                          child: Column(
                            crossAxisAlignment: CrossAxisAlignment.start,
                            children: [
                              Text(
                                l.mainVisualStyle,
                                style: TextStyle(
                                  color: p.ink,
                                  fontSize: 12,
                                  fontWeight: FontWeight.w600,
                                ),
                              ),
                              const SizedBox(height: 4),
                              Text(
                                pluginActive
                                    ? '$pluginTitle + ${styleTitle(selected)}'
                                    : styleTitle(selected),
                                style: TextStyle(color: p.muted, fontSize: 10),
                              ),
                            ],
                          ),
                        ),
                        const SizedBox(width: 8),
                        AnimatedRotation(
                          turns: expanded ? .5 : 0,
                          duration: motionDuration(context, 220),
                          child: Icon(
                            Icons.expand_more,
                            size: 20,
                            color: p.muted,
                          ),
                        ),
                      ],
                    ),
                  ),
                ),
              ),
            ),
          ),
        ),
        CollapsiblePanel(
          expanded: expanded,
          child: Padding(
            padding: const EdgeInsets.only(top: 12),
            child: Column(
              children: [
                if (plugins != null)
                  ThemePluginTile(
                    controller: plugins,
                    palette: p,
                    onSelected: () {
                      if (mounted) setState(() => expanded = false);
                    },
                  ),
                for (final style in VisualStyle.values)
                  Padding(
                    padding: const EdgeInsets.only(bottom: 14),
                    child: Semantics(
                      label: '${semanticTitle(style)}. ${description(style)}',
                      selected: selected == style,
                      button: true,
                      onTap: () => choose(style),
                      child: ExcludeSemantics(
                        child: NeumorphicSurface(
                          palette: p,
                          depth: selected == style ? -1 : .7,
                          borderRadius: p.borderRadius(13),
                          child: Material(
                            color: Colors.transparent,
                            child: InkWell(
                              key: ValueKey('visual-style-${style.name}'),
                              borderRadius: p.borderRadius(13),
                              onTap: () => choose(style),
                              child: AnimatedContainer(
                                duration: motionDuration(context, 220),
                                padding: const EdgeInsets.all(12),
                                decoration: BoxDecoration(
                                  color: p.accent.withValues(
                                    alpha: selected == style ? .055 : .015,
                                  ),
                                  borderRadius: p.borderRadius(13),
                                  border: Border.all(
                                    color: selected == style
                                        ? p.accent.withValues(alpha: .4)
                                        : p.line,
                                  ),
                                ),
                                child: LayoutBuilder(
                                  builder: (context, constraints) => Row(
                                    children: [
                                      if (constraints.maxWidth >= 290) ...[
                                        _StyleSwatch(p: p, style: style),
                                        const SizedBox(width: 12),
                                      ],
                                      Expanded(
                                        child: Column(
                                          crossAxisAlignment:
                                              CrossAxisAlignment.start,
                                          children: [
                                            Text(
                                              title(style),
                                              style: TextStyle(
                                                color: p.ink,
                                                fontSize: 12,
                                                fontWeight: FontWeight.w600,
                                              ),
                                            ),
                                            if (experimental(style)) ...[
                                              const SizedBox(height: 3),
                                              Text(
                                                l.mainStyleExperimental,
                                                style: TextStyle(
                                                  color: p.accent,
                                                  fontSize: 10,
                                                  fontWeight: FontWeight.w600,
                                                ),
                                              ),
                                            ],
                                            const SizedBox(height: 4),
                                            Text(
                                              description(style),
                                              style: TextStyle(
                                                color: p.muted,
                                                fontSize: 10,
                                                height: 1.5,
                                              ),
                                            ),
                                          ],
                                        ),
                                      ),
                                      const SizedBox(width: 8),
                                      Icon(
                                        selected == style
                                            ? Icons.check_circle_rounded
                                            : Icons.circle_outlined,
                                        color: selected == style
                                            ? p.accent
                                            : p.muted,
                                        size: 18,
                                      ),
                                    ],
                                  ),
                                ),
                              ),
                            ),
                          ),
                        ),
                      ),
                    ),
                  ),
              ],
            ),
          ),
        ),
      ],
    );
  }
}

class _StyleSwatch extends StatelessWidget {
  const _StyleSwatch({required this.p, required this.style});

  final Palette p;
  final VisualStyle style;

  @override
  Widget build(BuildContext context) {
    final tint = Color.lerp(p.surface, p.accent, .18)!;
    final shadow = BoxShadow(
      color: Colors.black.withValues(alpha: p.dark ? .38 : .14),
      offset: const Offset(3, 3),
      blurRadius: 5,
    );
    final lightShadow = BoxShadow(
      color: Colors.white.withValues(alpha: p.dark ? .08 : .9),
      offset: const Offset(-3, -3),
      blurRadius: 5,
    );
    final decoration = switch (style) {
      VisualStyle.flat => BoxDecoration(
        color: p.surface,
        borderRadius: p.borderRadius(8),
        border: Border.all(color: p.line),
      ),
      VisualStyle.neumorphism => BoxDecoration(
        color: p.background,
        borderRadius: p.borderRadius(8),
        boxShadow: [shadow, lightShadow],
      ),
      VisualStyle.paper => BoxDecoration(
        color: p.surface,
        border: Border.all(color: p.line),
        boxShadow: [
          BoxShadow(
            color: p.ink.withValues(alpha: .12),
            offset: const Offset(1, 2),
            blurRadius: 1,
          ),
        ],
      ),
      VisualStyle.clay => BoxDecoration(
        color: tint,
        borderRadius: BorderRadius.circular(13),
        boxShadow: [
          BoxShadow(
            color: p.accent.withValues(alpha: .25),
            offset: const Offset(2, 4),
            blurRadius: 5,
          ),
        ],
      ),
      VisualStyle.fluent => BoxDecoration(
        gradient: LinearGradient(colors: [p.surface, tint]),
        borderRadius: BorderRadius.circular(7),
        border: Border.all(color: p.accent.withValues(alpha: .45)),
      ),
      VisualStyle.brutalist => BoxDecoration(
        color: p.surface,
        border: Border.all(color: p.ink, width: 2),
        boxShadow: [BoxShadow(color: p.accent, offset: const Offset(3, 3))],
      ),
      VisualStyle.industrial => BoxDecoration(
        color: Color.lerp(p.background, p.surface, .5),
        borderRadius: BorderRadius.circular(2),
        border: Border.all(color: p.muted, width: 1.5),
      ),
    };
    final icon = switch (style) {
      VisualStyle.flat => Icons.auto_awesome,
      VisualStyle.neumorphism => Icons.layers_outlined,
      VisualStyle.paper => Icons.article_outlined,
      VisualStyle.clay => Icons.circle,
      VisualStyle.fluent => Icons.window_rounded,
      VisualStyle.brutalist => Icons.crop_square,
      VisualStyle.industrial => Icons.settings_outlined,
    };
    return Container(
      key: ValueKey('visual-style-preview-${style.name}'),
      width: 42,
      height: 42,
      decoration: BoxDecoration(
        color: p.background,
        borderRadius: p.borderRadius(12),
      ),
      alignment: Alignment.center,
      child: Container(
        width: 24,
        height: 24,
        decoration: decoration,
        alignment: Alignment.center,
        child: Icon(icon, size: 12, color: p.accent),
      ),
    );
  }
}
