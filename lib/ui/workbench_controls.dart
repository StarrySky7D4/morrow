import 'package:material_ui/material_ui.dart';

import '../appearance.dart';

/// Unified compact icon button used across workbench widget cards and mini windows.
class WorkbenchMiniIconButton extends StatelessWidget {
  const WorkbenchMiniIconButton({
    super.key,
    required this.palette,
    required this.icon,
    required this.tooltip,
    required this.color,
    required this.onTap,
    this.size = 28,
    this.iconSize = 15,
  });

  final Palette palette;
  final IconData icon;
  final String tooltip;
  final Color color;
  final VoidCallback onTap;
  final double size;
  final double iconSize;

  @override
  Widget build(BuildContext context) => Tooltip(
    message: tooltip,
    child: Material(
      color: Colors.transparent,
      child: InkWell(
        borderRadius: palette.borderRadius(9),
        onTap: onTap,
        child: SizedBox(
          width: size,
          height: size,
          child: Icon(icon, size: iconSize, color: color),
        ),
      ),
    ),
  );
}

/// Crisp custom pill toggle replacing scaled native switches on compact cards.
class WorkbenchCompactSwitch extends StatelessWidget {
  const WorkbenchCompactSwitch({
    super.key,
    required this.palette,
    required this.value,
    required this.onChanged,
    this.semanticLabel,
  });

  final Palette palette;
  final bool value;
  final ValueChanged<bool> onChanged;
  final String? semanticLabel;

  @override
  Widget build(BuildContext context) {
    final p = palette;
    final disableAnim = MediaQuery.disableAnimationsOf(context);
    final duration = disableAnim
        ? Duration.zero
        : const Duration(milliseconds: 160);

    return Semantics(
      button: true,
      toggled: value,
      label: semanticLabel,
      child: Material(
        color: Colors.transparent,
        child: InkWell(
          borderRadius: BorderRadius.circular(999),
          onTap: () => onChanged(!value),
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: 4, vertical: 3),
            child: AnimatedContainer(
              duration: duration,
              curve: Curves.easeOutCubic,
              width: 34,
              height: 20,
              padding: const EdgeInsets.symmetric(horizontal: 3),
              decoration: BoxDecoration(
                color: value
                    ? p.accent
                    : p.ink.withValues(alpha: p.dark ? 0.18 : 0.14),
                borderRadius: BorderRadius.circular(999),
                border: Border.all(
                  color: value
                      ? p.accent.withValues(alpha: 0.45)
                      : p.line,
                ),
              ),
              child: AnimatedAlign(
                duration: duration,
                curve: Curves.easeOutCubic,
                alignment:
                    value ? Alignment.centerRight : Alignment.centerLeft,
                child: Container(
                  width: 14,
                  height: 14,
                  decoration: BoxDecoration(
                    color: value
                        ? p.onAccent
                        : (p.dark
                            ? Colors.white.withValues(alpha: 0.85)
                            : Colors.white),
                    shape: BoxShape.circle,
                    boxShadow: [
                      BoxShadow(
                        color: Colors.black.withValues(alpha: 0.16),
                        blurRadius: 3,
                        offset: const Offset(0, 1),
                      ),
                    ],
                  ),
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}
