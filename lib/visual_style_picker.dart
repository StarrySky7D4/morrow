import 'package:flutter/material.dart';
import 'package:morrow_i18n/morrow_i18n.dart';
import 'appearance.dart';
import 'collapsible_panel.dart';
import 'neumorphic_controls.dart';

/// Keep the settings overview compact. The selected style stays visible while
/// the two previews are only interactive after explicit expansion.
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
    String title(VisualStyle style) =>
        style == VisualStyle.flat ? l.mainStyleFlat : l.mainStyleNeumorphism;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Semantics(
          expanded: expanded,
          button: true,
          child: NeumorphicSurface(
            palette: p,
            depth: expanded ? -0.55 : 0.55,
            borderRadius: p.borderRadius(13),
            child: Material(
              color: Colors.transparent,
              child: InkWell(
                key: const ValueKey('visual-style-toggle'),
                borderRadius: p.borderRadius(13),
                onTap: () => setState(() => expanded = !expanded),
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
                              title(p.surfaces.visualStyle),
                              style: TextStyle(color: p.muted, fontSize: 10),
                              maxLines: 2,
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
        CollapsiblePanel(
          expanded: expanded,
          child: Padding(
            padding: const EdgeInsets.only(top: 12),
            child: Column(
              children: [
                for (final style in VisualStyle.values)
                  Padding(
                    padding: const EdgeInsets.only(bottom: 10),
                    child: Semantics(
                      selected: p.surfaces.visualStyle == style,
                      button: true,
                      child: NeumorphicSurface(
                        palette: p,
                        depth: p.surfaces.visualStyle == style ? -1 : .7,
                        borderRadius: p.borderRadius(13),
                        child: Material(
                          color: Colors.transparent,
                          child: InkWell(
                            key: ValueKey('visual-style-${style.name}'),
                            borderRadius: p.borderRadius(13),
                            onTap: () {
                              widget.onChanged(style);
                              setState(() => expanded = false);
                            },
                            child: AnimatedContainer(
                              duration: motionDuration(context, 220),
                              padding: const EdgeInsets.all(12),
                              decoration: BoxDecoration(
                                color: p.accent.withValues(
                                  alpha: p.surfaces.visualStyle == style
                                      ? .055
                                      : .015,
                                ),
                                borderRadius: p.borderRadius(13),
                                border: Border.all(
                                  color: p.surfaces.visualStyle == style
                                      ? p.accent.withValues(alpha: .4)
                                      : p.line,
                                ),
                              ),
                              child: Row(
                                children: [
                                  _StyleSwatch(p: p, style: style),
                                  const SizedBox(width: 12),
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
                                        const SizedBox(height: 4),
                                        Text(
                                          style == VisualStyle.flat
                                              ? l.mainStyleFlatDescription
                                              : l.mainStyleNeumorphismDescription,
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
                                    p.surfaces.visualStyle == style
                                        ? Icons.check_circle_rounded
                                        : Icons.circle_outlined,
                                    color: p.surfaces.visualStyle == style
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
  Widget build(BuildContext context) => ExcludeSemantics(
    child: Container(
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
        decoration: BoxDecoration(
          color: style == VisualStyle.flat ? p.surface : p.background,
          borderRadius: p.borderRadius(8),
          border: style == VisualStyle.flat ? Border.all(color: p.line) : null,
          boxShadow: style == VisualStyle.neumorphism
              ? [
                  BoxShadow(
                    color: Colors.black.withValues(alpha: p.dark ? .5 : .14),
                    offset: const Offset(3, 3),
                    blurRadius: 5,
                  ),
                  BoxShadow(
                    color: Colors.white.withValues(alpha: p.dark ? .09 : .95),
                    offset: const Offset(-3, -3),
                    blurRadius: 5,
                  ),
                ]
              : null,
        ),
        child: Icon(Icons.auto_awesome, size: 12, color: p.accent),
      ),
    ),
  );
}
