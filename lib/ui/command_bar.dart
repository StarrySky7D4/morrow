import 'package:material_ui/material_ui.dart';
import 'package:morrow_i18n/morrow_i18n.dart';
import '../appearance.dart';
import '../plugins/workbench_ids.dart';
import '../plugins/workbench_labels.dart';
import 'workbench_motion.dart';
import 'workbench_rail.dart';
import 'workbench_strings.dart';

/// Top workbench command bar with live section context, inline search + Ctrl+K
/// command palette trigger, view density switch, focus mode, and inspector toggle.
class WorkbenchCommandBar extends StatelessWidget {
  const WorkbenchCommandBar({
    super.key,
    required this.palette,
    required this.section,
    required this.visibleCount,
    required this.sidebar,
    required this.desktop,
    required this.showAppearance,
    required this.focusMode,
    required this.compactCards,
    required this.search,
    required this.searchFocus,
    required this.onToggleAppearance,
    required this.onToggleFocusMode,
    required this.onToggleCompactCards,
    required this.onOpenCommandPalette,
    required this.onCreateIdea,
    required this.onClearSearch,
    required this.wrapSearchInteractions,
    this.recoveryControl,
  });

  final Palette palette;
  final WorkbenchPage section;
  final int visibleCount;
  final bool sidebar;
  final bool desktop;
  final bool showAppearance;
  final bool focusMode;
  final bool compactCards;
  final TextEditingController search;
  final FocusNode searchFocus;
  final VoidCallback onToggleAppearance;
  final VoidCallback onToggleFocusMode;
  final VoidCallback onToggleCompactCards;
  final VoidCallback onOpenCommandPalette;
  final VoidCallback onCreateIdea;
  final VoidCallback onClearSearch;
  final Widget Function(Widget child) wrapSearchInteractions;
  final Widget? recoveryControl;

  @override
  Widget build(BuildContext context) {
    final p = palette;
    final l = L10n.of(context);
    final s = WorkbenchStrings.of(context);
    final labels = WorkbenchLabelsScope.of(context);

    return Row(
      children: [
        if (sidebar)
          Flexible(
            flex: 2,
            child: SharedAxisSwitcher(
              axis: Axis.horizontal,
              alignment: Alignment.centerLeft,
              child: Row(
                key: ValueKey('command-crumb-${section.id}'),
                mainAxisSize: MainAxisSize.min,
                children: [
                  Container(
                    width: 28,
                    height: 28,
                    decoration: BoxDecoration(
                      color: p.accent.withValues(alpha: 0.12),
                      borderRadius: p.borderRadius(9),
                    ),
                    child: Icon(
                      WorkbenchRail.iconForPage(section, active: true),
                      size: 15,
                      color: p.accent,
                    ),
                  ),
                  const SizedBox(width: 9),
                  Flexible(
                    child: Text(
                      labels.page(section),
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: TextStyle(
                        color: p.ink,
                        fontSize: 13,
                        fontWeight: FontWeight.w600,
                      ),
                    ),
                  ),
                  const SizedBox(width: 8),
                  Container(
                    padding: const EdgeInsets.symmetric(
                      horizontal: 7,
                      vertical: 2,
                    ),
                    decoration: BoxDecoration(
                      color: p.ink.withValues(alpha: 0.06),
                      borderRadius: p.borderRadius(8),
                    ),
                    child: Text(
                      s.itemCountBadge(visibleCount),
                      style: TextStyle(fontSize: 10, color: p.muted),
                    ),
                  ),
                ],
              ),
            ),
          ),
        if (sidebar) const SizedBox(width: 12),
        if (sidebar)
          Flexible(
            flex: 3,
            child: ConstrainedBox(
              constraints: const BoxConstraints(maxWidth: 360),
              child: _searchCapsule(context, p, l, s),
            ),
          )
        else
          Expanded(child: _searchCapsule(context, p, l, s)),
        const SizedBox(width: 8),
        if (sidebar) ...[
          _actionIcon(
            key: const ValueKey('view-density-toggle'),
            p: p,
            tooltip: compactCards
                ? s.densityGridTooltip
                : s.densityListTooltip,
            active: compactCards,
            onPressed: onToggleCompactCards,
            icon: SoftSwap(
              child: Icon(
                compactCards
                    ? Icons.view_agenda_outlined
                    : Icons.grid_view_rounded,
                key: ValueKey(compactCards),
                size: 18,
                color: compactCards ? p.accent : p.muted,
              ),
            ),
          ),
          const SizedBox(width: 4),
          _actionIcon(
            key: const ValueKey('focus-mode-toggle'),
            p: p,
            tooltip: focusMode ? s.exitFocusTooltip : s.focusModeTooltip,
            active: focusMode,
            onPressed: onToggleFocusMode,
            icon: Icon(
              focusMode
                  ? Icons.fullscreen_exit_rounded
                  : Icons.fullscreen_rounded,
              size: 19,
              color: focusMode ? p.accent : p.muted,
            ),
          ),
        ],
        if (recoveryControl != null) ...[
          const SizedBox(width: 2),
          recoveryControl!,
        ],
        const SizedBox(width: 2),
        _actionIcon(
          key: const ValueKey('appearance-toggle'),
          p: p,
          tooltip: desktop && showAppearance
              ? l.mainHideAppearance
              : l.mainShowAppearance,
          active: desktop && showAppearance && !focusMode,
          onPressed: onToggleAppearance,
          icon: Icon(
            Icons.tune_rounded,
            size: 18,
            color: desktop && showAppearance && !focusMode ? p.accent : p.muted,
          ),
        ),
        const SizedBox(width: 6),
        if (desktop)
          FilledButton.icon(
            key: const ValueKey('header-new-idea'),
            onPressed: onCreateIdea,
            style: FilledButton.styleFrom(
              backgroundColor: p.accent,
              foregroundColor: p.onAccent,
              padding: const EdgeInsets.symmetric(horizontal: 13, vertical: 10),
              minimumSize: const Size(0, 38),
              shape: RoundedRectangleBorder(borderRadius: p.borderRadius(12)),
            ),
            icon: const Icon(Icons.add_rounded, size: 17),
            label: Text(l.mainNewIdea, style: const TextStyle(fontSize: 11)),
          )
        else
          Tooltip(
            message: l.mainNewIdea,
            child: InkWell(
              key: const ValueKey('header-new-idea'),
              borderRadius: p.borderRadius(12),
              onTap: onCreateIdea,
              child: Container(
                width: 38,
                height: 38,
                decoration: BoxDecoration(
                  color: p.accent,
                  borderRadius: p.borderRadius(12),
                ),
                child: Icon(Icons.add_rounded, size: 19, color: p.onAccent),
              ),
            ),
          ),
      ],
    );
  }

  Widget _searchCapsule(
    BuildContext context,
    Palette p,
    AppLocalizations l,
    WorkbenchStrings s,
  ) => wrapSearchInteractions(
    SizedBox(
      key: const ValueKey('header-search'),
      height: 38,
      child: Glass(
        componentId: 'search',
        recessed: true,
        p: p,
        radius: 13,
        child: Row(
          children: [
            const SizedBox(width: 11),
            Icon(Icons.search_rounded, size: 16, color: p.muted),
            const SizedBox(width: 6),
            Expanded(
              child: TextField(
                controller: search,
                focusNode: searchFocus,
                style: TextStyle(fontSize: 11, color: p.ink),
                decoration: InputDecoration(
                  isDense: true,
                  filled: false,
                  hintText: l.mainSearchHint,
                  hintStyle: TextStyle(fontSize: 11, color: p.muted),
                  border: InputBorder.none,
                  enabledBorder: InputBorder.none,
                  focusedBorder: InputBorder.none,
                  contentPadding: const EdgeInsets.symmetric(vertical: 8),
                ),
              ),
            ),
            if (search.text.isNotEmpty)
              IconButton(
                tooltip: l.mainClearSearch,
                visualDensity: VisualDensity.compact,
                constraints: const BoxConstraints(minWidth: 28, minHeight: 28),
                padding: EdgeInsets.zero,
                onPressed: onClearSearch,
                icon: Icon(Icons.close_rounded, size: 14, color: p.muted),
              ),
            Padding(
              padding: const EdgeInsets.only(right: 6),
              child: Tooltip(
                message: s.commandPaletteTooltip,
                child: InkWell(
                  key: const ValueKey('command-palette-trigger'),
                  borderRadius: p.borderRadius(8),
                  onTap: onOpenCommandPalette,
                  child: Container(
                    padding: const EdgeInsets.symmetric(
                      horizontal: 7,
                      vertical: 4,
                    ),
                    decoration: BoxDecoration(
                      color: p.ink.withValues(alpha: 0.06),
                      borderRadius: p.borderRadius(8),
                      border: Border.all(color: p.line),
                    ),
                    child: Text(
                      'Ctrl K',
                      style: TextStyle(
                        fontSize: 9,
                        fontWeight: FontWeight.w600,
                        color: p.muted,
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
  );

  Widget _actionIcon({
    Key? key,
    required Palette p,
    required String tooltip,
    required bool active,
    required VoidCallback onPressed,
    required Widget icon,
  }) => Tooltip(
    message: tooltip,
    child: InkWell(
      key: key,
      borderRadius: p.borderRadius(11),
      onTap: onPressed,
      child: AnimatedContainer(
        duration: const Duration(milliseconds: 180),
        width: 36,
        height: 36,
        decoration: BoxDecoration(
          color: active
              ? p.accent.withValues(alpha: p.dark ? 0.22 : 0.14)
              : Colors.transparent,
          borderRadius: p.borderRadius(11),
          border: Border.all(
            color: active
                ? p.accent.withValues(alpha: 0.35)
                : Colors.transparent,
          ),
        ),
        child: Center(child: icon),
      ),
    ),
  );
}
