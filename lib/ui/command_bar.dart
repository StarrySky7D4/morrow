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
class WorkbenchCommandBar extends StatefulWidget {
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

  static Widget actionIconBox({
    required BuildContext context,
    required Palette p,
    required Widget icon,
    bool active = false,
    double size = 36,
  }) => AnimatedContainer(
    duration: workbenchDuration(context, WorkbenchDurations.micro),
    curve: WorkbenchCurves.snappy,
    width: size,
    height: size,
    decoration: BoxDecoration(
      color: active
          ? p.accent.withValues(alpha: p.dark ? 0.22 : 0.14)
          : Colors.transparent,
      borderRadius: p.borderRadius(11),
      border: Border.all(
        color: active ? p.accent.withValues(alpha: 0.35) : Colors.transparent,
      ),
    ),
    child: Center(child: icon),
  );

  static Widget actionIconButton({
    Key? key,
    required BuildContext context,
    required Palette p,
    required String tooltip,
    required VoidCallback onPressed,
    required Widget icon,
    bool active = false,
    double size = 36,
  }) => Tooltip(
    message: tooltip,
    child: Semantics(
      button: true,
      child: Material(
        type: MaterialType.transparency,
        child: InkWell(
          key: key,
          borderRadius: p.borderRadius(11),
          onTap: onPressed,
          child: actionIconBox(
            context: context,
            p: p,
            icon: icon,
            active: active,
            size: size,
          ),
        ),
      ),
    ),
  );

  @override
  State<WorkbenchCommandBar> createState() => _WorkbenchCommandBarState();
}

class _WorkbenchCommandBarState extends State<WorkbenchCommandBar> {
  int _crumbDirection = 1;

  @override
  void didUpdateWidget(WorkbenchCommandBar oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.section != widget.section) {
      _crumbDirection = widget.section.index >= oldWidget.section.index
          ? 1
          : -1;
    }
  }

  @override
  Widget build(BuildContext context) {
    final p = widget.palette;
    final l = L10n.of(context);
    final s = WorkbenchStrings.of(context);
    final labels = WorkbenchLabelsScope.of(context);

    return Row(
      children: [
        if (widget.sidebar)
          Flexible(
            flex: 2,
            child: SharedAxisSwitcher(
              axis: Axis.horizontal,
              durationMs: 240,
              slideOffset: 0.08,
              direction: _crumbDirection,
              alignment: Alignment.centerLeft,
              child: Row(
                key: ValueKey('command-crumb-${widget.section.id}'),
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
                      WorkbenchRail.iconForPage(widget.section, active: true),
                      size: 15,
                      color: p.accent,
                    ),
                  ),
                  const SizedBox(width: 9),
                  Flexible(
                    child: Text(
                      labels.page(widget.section),
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
                      s.itemCountBadge(widget.visibleCount),
                      style: TextStyle(fontSize: 10, color: p.muted),
                    ),
                  ),
                ],
              ),
            ),
          ),
        if (widget.sidebar) const SizedBox(width: 12),
        if (widget.sidebar)
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
        if (widget.sidebar) ...[
          _actionIcon(
            key: const ValueKey('view-density-toggle'),
            context: context,
            p: p,
            tooltip: widget.compactCards
                ? s.densityGridTooltip
                : s.densityListTooltip,
            active: widget.compactCards,
            onPressed: widget.onToggleCompactCards,
            icon: SoftSwap(
              child: Icon(
                widget.compactCards
                    ? Icons.view_agenda_outlined
                    : Icons.grid_view_rounded,
                key: ValueKey(widget.compactCards),
                size: 18,
                color: widget.compactCards ? p.accent : p.muted,
              ),
            ),
          ),
          const SizedBox(width: 4),
          _actionIcon(
            key: const ValueKey('focus-mode-toggle'),
            context: context,
            p: p,
            tooltip: widget.focusMode ? s.exitFocusTooltip : s.focusModeTooltip,
            active: widget.focusMode,
            onPressed: widget.onToggleFocusMode,
            icon: SoftSwap(
              child: Icon(
                widget.focusMode
                    ? Icons.fullscreen_exit_rounded
                    : Icons.fullscreen_rounded,
                key: ValueKey(widget.focusMode),
                size: 19,
                color: widget.focusMode ? p.accent : p.muted,
              ),
            ),
          ),
        ],
        if (widget.recoveryControl != null) ...[
          const SizedBox(width: 4),
          widget.recoveryControl!,
        ],
        const SizedBox(width: 4),
        _actionIcon(
          key: const ValueKey('appearance-toggle'),
          context: context,
          p: p,
          tooltip: widget.desktop && widget.showAppearance
              ? l.mainHideAppearance
              : l.mainShowAppearance,
          active: widget.desktop && widget.showAppearance && !widget.focusMode,
          onPressed: widget.onToggleAppearance,
          icon: Icon(
            Icons.tune_rounded,
            size: 18,
            color: widget.desktop && widget.showAppearance && !widget.focusMode
                ? p.accent
                : p.muted,
          ),
        ),
        const SizedBox(width: 6),
        if (widget.desktop)
          FilledButton.icon(
            key: const ValueKey('header-new-idea'),
            onPressed: widget.onCreateIdea,
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
              onTap: widget.onCreateIdea,
              child: Container(
                width: 38,
                height: 38,
                decoration: BoxDecoration(
                  color: p.accent,
                  borderRadius: p.borderRadius(12),
                  boxShadow: [
                    BoxShadow(
                      color: p.accent.withValues(alpha: p.dark ? 0.30 : 0.24),
                      blurRadius: 10,
                      offset: const Offset(0, 3),
                    ),
                  ],
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
  ) => widget.wrapSearchInteractions(
    SizedBox(
      key: const ValueKey('header-search'),
      height: 38,
      child: Glass(
        componentId: 'search',
        recessed: true,
        p: p,
        radius: 13,
        child: Row(
          crossAxisAlignment: CrossAxisAlignment.center,
          children: [
            const SizedBox(width: 11),
            Icon(Icons.search_rounded, size: 16, color: p.muted),
            const SizedBox(width: 7),
            Expanded(
              child: Center(
                child: TextField(
                  controller: widget.search,
                  focusNode: widget.searchFocus,
                  textAlignVertical: TextAlignVertical.center,
                  style: TextStyle(fontSize: 12, height: 1.25, color: p.ink),
                  decoration: InputDecoration(
                    isCollapsed: true,
                    filled: false,
                    hintText: l.mainSearchHint,
                    hintStyle: TextStyle(
                      fontSize: 12,
                      height: 1.25,
                      color: p.muted,
                    ),
                    border: InputBorder.none,
                    enabledBorder: InputBorder.none,
                    focusedBorder: InputBorder.none,
                    contentPadding: EdgeInsets.zero,
                  ),
                ),
              ),
            ),
            if (widget.search.text.isNotEmpty)
              IconButton(
                tooltip: l.mainClearSearch,
                visualDensity: VisualDensity.compact,
                constraints: const BoxConstraints(minWidth: 28, minHeight: 28),
                padding: EdgeInsets.zero,
                onPressed: widget.onClearSearch,
                icon: Icon(Icons.close_rounded, size: 14, color: p.muted),
              ),
            Padding(
              padding: const EdgeInsets.only(right: 7),
              child: Tooltip(
                message: s.commandPaletteTooltip,
                child: Material(
                  color: Colors.transparent,
                  child: InkWell(
                    key: const ValueKey('command-palette-trigger'),
                    borderRadius: p.borderRadius(7),
                    onTap: widget.onOpenCommandPalette,
                    child: Container(
                      height: 22,
                      alignment: Alignment.center,
                      padding: const EdgeInsets.symmetric(horizontal: 7),
                      decoration: BoxDecoration(
                        color: p.ink.withValues(alpha: 0.06),
                        borderRadius: p.borderRadius(7),
                        border: Border.all(color: p.line),
                      ),
                      child: Text(
                        'Ctrl K',
                        style: TextStyle(
                          fontSize: 9.5,
                          height: 1.0,
                          fontWeight: FontWeight.w600,
                          color: p.muted,
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
  );

  Widget _actionIcon({
    Key? key,
    required BuildContext context,
    required Palette p,
    required String tooltip,
    required bool active,
    required VoidCallback onPressed,
    required Widget icon,
  }) => WorkbenchCommandBar.actionIconButton(
    key: key,
    context: context,
    p: p,
    tooltip: tooltip,
    active: active,
    onPressed: onPressed,
    icon: icon,
  );
}
