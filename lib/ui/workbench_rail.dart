import 'package:material_ui/material_ui.dart';
import 'package:morrow_i18n/morrow_i18n.dart';
import '../appearance.dart';
import '../little_tips.dart';
import '../neumorphic_controls.dart';
import '../plugins/workbench_ids.dart';
import '../plugins/workbench_labels.dart';
import '../tip_preferences.dart';
import 'workbench_motion.dart';
import 'workbench_strings.dart';

/// Collapsible left navigation rail that morphs smoothly between an expanded
/// workspace sidebar and a compact 68px icon rail.
class WorkbenchRail extends StatelessWidget {
  const WorkbenchRail({
    super.key,
    required this.palette,
    required this.section,
    required this.expanded,
    required this.desktop,
    required this.counts,
    required this.hasPluginSettings,
    required this.focusMode,
    required this.onSelectPage,
    required this.onToggleExpanded,
    required this.onCreateIdea,
    required this.onOpenCommandPalette,
    required this.onToggleFocusMode,
    required this.onOpenPluginSettings,
    required this.componentInteractions,
    required this.cornerTipSurface,
  });

  final Palette palette;
  final WorkbenchPage section;
  final bool expanded;
  final bool desktop;
  final Map<WorkbenchPage, int> counts;
  final bool hasPluginSettings;
  final bool focusMode;
  final ValueChanged<WorkbenchPage> onSelectPage;
  final VoidCallback onToggleExpanded;
  final VoidCallback onCreateIdea;
  final VoidCallback onOpenCommandPalette;
  final VoidCallback onToggleFocusMode;
  final VoidCallback onOpenPluginSettings;
  final Widget Function(String id, Widget child) componentInteractions;
  final Widget Function({required Widget child}) cornerTipSurface;

  static IconData iconForPage(WorkbenchPage page, {bool active = false}) =>
      switch (page) {
        WorkbenchPage.overview =>
          active ? Icons.grid_view_rounded : Icons.grid_view_outlined,
        WorkbenchPage.inbox =>
          active ? Icons.inbox_rounded : Icons.inbox_outlined,
        WorkbenchPage.projects =>
          active ? Icons.folder_rounded : Icons.folder_open_rounded,
        WorkbenchPage.laboratory =>
          active ? Icons.science_rounded : Icons.science_outlined,
        WorkbenchPage.favorites =>
          active ? Icons.bookmark_rounded : Icons.bookmark_border_rounded,
      };

  @override
  Widget build(BuildContext context) {
    final p = palette;
    final l = L10n.of(context);
    final s = WorkbenchStrings.of(context);
    final labels = WorkbenchLabelsScope.of(context);
    final expandedWidth = desktop ? 236.0 : 204.0;
    final targetWidth = expanded ? expandedWidth : 68.0;

    return AnimatedContainer(
      duration: workbenchDuration(context, WorkbenchDurations.medium),
      curve: WorkbenchCurves.springOut,
      width: targetWidth,
      child: componentInteractions(
        'navigation',
        Glass(
          componentId: 'navigation',
          p: p,
          radius: 24,
          child: LayoutBuilder(
            builder: (context, constraints) {
              Widget content = SingleChildScrollView(
                child: ConstrainedBox(
                  constraints: BoxConstraints(minHeight: constraints.maxHeight),
                  child: IntrinsicHeight(
                    child: Padding(
                      padding: EdgeInsets.fromLTRB(
                        expanded ? 14 : 10,
                        18,
                        expanded ? 14 : 10,
                        14,
                      ),
                      child: expanded
                          ? _buildExpanded(context, p, l, s, labels)
                          : _buildCollapsed(context, p, l, s, labels),
                    ),
                  ),
                ),
              );
              if (expanded) {
                content = ClipRect(
                  child: OverflowBox(
                    alignment: Alignment.topLeft,
                    minWidth: expandedWidth,
                    maxWidth: expandedWidth,
                    minHeight: 0,
                    maxHeight: constraints.maxHeight,
                    child: content,
                  ),
                );
              }
              return content;
            },
          ),
        ),
      ),
    );
  }

  Widget _buildExpanded(
    BuildContext context,
    Palette p,
    AppLocalizations l,
    WorkbenchStrings s,
    WorkbenchLabels labels,
  ) => Column(
    crossAxisAlignment: CrossAxisAlignment.start,
    children: [
      Padding(
        padding: const EdgeInsets.only(left: 6),
        child: Row(
          children: [
            _brandLogo(p, 30),
            const SizedBox(width: 10),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    'Morrow',
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: TextStyle(
                      fontSize: 19,
                      color: p.ink,
                      fontWeight: FontWeight.w600,
                      letterSpacing: -0.7,
                    ),
                  ),
                  Text(
                    l.mainSidebarMotto,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: TextStyle(fontSize: 9, color: p.muted),
                  ),
                ],
              ),
            ),
            IconButton(
              key: const ValueKey('sidebar-toggle'),
              tooltip: l.mainCollapseSidebar,
              visualDensity: VisualDensity.compact,
              onPressed: onToggleExpanded,
              icon: AnimatedRotation(
                turns: expanded ? 0 : 0.5,
                duration: workbenchDuration(context, WorkbenchDurations.medium),
                curve: WorkbenchCurves.springOut,
                child: Icon(
                  Icons.chevron_left_rounded,
                  size: 18,
                  color: p.muted,
                ),
              ),
            ),
          ],
        ),
      ),
      const SizedBox(height: 16),
      _quickCommandPill(p, s),
      const SizedBox(height: 18),
      Padding(
        padding: const EdgeInsets.only(left: 10, bottom: 10),
        child: Text(
          l.mainMySpace,
          maxLines: 1,
          overflow: TextOverflow.ellipsis,
          style: TextStyle(
            fontSize: 10,
            color: p.muted,
            letterSpacing: 1.4,
            fontWeight: FontWeight.w600,
          ),
        ),
      ),
      _expandedNavItem(context, p, labels, WorkbenchPage.overview),
      _expandedNavItem(
        context,
        p,
        labels,
        WorkbenchPage.inbox,
        count: counts[WorkbenchPage.inbox],
      ),
      _expandedNavItem(
        context,
        p,
        labels,
        WorkbenchPage.projects,
        count: counts[WorkbenchPage.projects],
      ),
      _expandedNavItem(
        context,
        p,
        labels,
        WorkbenchPage.laboratory,
        count: counts[WorkbenchPage.laboratory],
      ),
      Padding(
        padding: const EdgeInsets.symmetric(vertical: 10, horizontal: 10),
        child: Divider(height: 1, color: p.line),
      ),
      _expandedNavItem(
        context,
        p,
        labels,
        WorkbenchPage.favorites,
        count: counts[WorkbenchPage.favorites],
      ),
      const Spacer(),
      const SizedBox(height: 14),
      componentInteractions(
        'corner-tips',
        cornerTipSurface(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Row(
                children: [
                  Icon(Icons.wb_twilight_rounded, color: p.accent, size: 18),
                  const Spacer(),
                  Container(
                    width: 6,
                    height: 6,
                    decoration: BoxDecoration(
                      color: p.accent.withValues(alpha: 0.45),
                      shape: BoxShape.circle,
                    ),
                  ),
                ],
              ),
              const SizedBox(height: 8),
              RotatingTip(
                key: const ValueKey('corner-tips'),
                lines:
                    TipPreferencesScope.maybeOf(
                      context,
                    )?.lines('corner-tips', localizedCornerTips(context)) ??
                    localizedCornerTips(context),
                style: TextStyle(fontSize: 10, color: p.muted, height: 1.7),
              ),
            ],
          ),
        ),
      ),
      const SizedBox(height: 14),
      Container(
        padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 8),
        decoration: BoxDecoration(
          color: p.surface.withValues(alpha: p.dark ? 0.2 : 0.35),
          borderRadius: p.borderRadius(14),
          border: Border.all(color: p.line),
        ),
        child: Row(
          children: [
            CircleAvatar(
              radius: 15,
              backgroundColor: p.accent.withValues(alpha: 0.14),
              child: Text(
                'M',
                style: TextStyle(
                  color: p.accent,
                  fontSize: 12,
                  fontWeight: FontWeight.w700,
                ),
              ),
            ),
            const SizedBox(width: 8),
            Expanded(
              child: Text(
                l.mainPersonalWorkspace,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: TextStyle(
                  fontSize: 11,
                  color: p.ink,
                  fontWeight: FontWeight.w500,
                ),
              ),
            ),
            if (hasPluginSettings)
              IconButton(
                tooltip: s.actionPluginSettings,
                visualDensity: VisualDensity.compact,
                constraints: const BoxConstraints(minWidth: 30, minHeight: 30),
                padding: EdgeInsets.zero,
                onPressed: onOpenPluginSettings,
                icon: Icon(Icons.extension_outlined, size: 15, color: p.muted),
              ),
            IconButton(
              tooltip: s.focusModeTooltip,
              visualDensity: VisualDensity.compact,
              constraints: const BoxConstraints(minWidth: 30, minHeight: 30),
              padding: EdgeInsets.zero,
              onPressed: onToggleFocusMode,
              icon: Icon(
                focusMode
                    ? Icons.fullscreen_exit_rounded
                    : Icons.fullscreen_rounded,
                size: 16,
                color: focusMode ? p.accent : p.muted,
              ),
            ),
          ],
        ),
      ),
    ],
  );

  Widget _buildCollapsed(
    BuildContext context,
    Palette p,
    AppLocalizations l,
    WorkbenchStrings s,
    WorkbenchLabels labels,
  ) => Column(
    crossAxisAlignment: CrossAxisAlignment.center,
    children: [
      _brandLogo(p, 34),
      const SizedBox(height: 8),
      IconButton(
        key: const ValueKey('sidebar-toggle'),
        tooltip: l.mainExpandSidebar,
        onPressed: onToggleExpanded,
        icon: AnimatedRotation(
          turns: expanded ? 0 : 0.5,
          duration: workbenchDuration(context, WorkbenchDurations.medium),
          curve: WorkbenchCurves.springOut,
          child: Icon(Icons.chevron_left_rounded, size: 18, color: p.muted),
        ),
      ),
      const SizedBox(height: 6),
      Tooltip(
        message: s.commandPaletteTooltip,
        child: InkWell(
          borderRadius: p.borderRadius(12),
          onTap: onOpenCommandPalette,
          child: Container(
            width: 42,
            height: 38,
            decoration: BoxDecoration(
              color: p.surface.withValues(alpha: p.dark ? 0.28 : 0.48),
              borderRadius: p.borderRadius(12),
              border: Border.all(color: p.line),
            ),
            child: Icon(Icons.search_rounded, size: 18, color: p.muted),
          ),
        ),
      ),
      const SizedBox(height: 14),
      _collapsedNavItem(context, p, labels, WorkbenchPage.overview),
      _collapsedNavItem(
        context,
        p,
        labels,
        WorkbenchPage.inbox,
        count: counts[WorkbenchPage.inbox],
      ),
      _collapsedNavItem(
        context,
        p,
        labels,
        WorkbenchPage.projects,
        count: counts[WorkbenchPage.projects],
      ),
      _collapsedNavItem(
        context,
        p,
        labels,
        WorkbenchPage.laboratory,
        count: counts[WorkbenchPage.laboratory],
      ),
      Padding(
        padding: const EdgeInsets.symmetric(vertical: 8),
        child: SizedBox(width: 24, child: Divider(height: 1, color: p.line)),
      ),
      _collapsedNavItem(
        context,
        p,
        labels,
        WorkbenchPage.favorites,
        count: counts[WorkbenchPage.favorites],
      ),
      const Spacer(),
      Tooltip(
        message: l.mainNewIdea,
        child: InkWell(
          borderRadius: p.borderRadius(12),
          onTap: onCreateIdea,
          child: Container(
            width: 42,
            height: 40,
            decoration: BoxDecoration(
              color: p.accent,
              borderRadius: p.borderRadius(12),
            ),
            child: Icon(Icons.add_rounded, size: 20, color: p.onAccent),
          ),
        ),
      ),
      const SizedBox(height: 8),
      if (hasPluginSettings)
        IconButton(
          tooltip: s.actionPluginSettings,
          onPressed: onOpenPluginSettings,
          icon: Icon(Icons.extension_outlined, size: 18, color: p.muted),
        ),
      IconButton(
        tooltip: s.focusModeTooltip,
        onPressed: onToggleFocusMode,
        icon: Icon(
          focusMode ? Icons.fullscreen_exit_rounded : Icons.fullscreen_rounded,
          size: 18,
          color: focusMode ? p.accent : p.muted,
        ),
      ),
    ],
  );

  Widget _brandLogo(Palette p, double size) => Container(
    width: size,
    height: size,
    decoration: BoxDecoration(
      gradient: LinearGradient(
        begin: Alignment.topLeft,
        end: Alignment.bottomRight,
        colors: [p.accent, Color.lerp(p.accent, Colors.white, 0.16)!],
      ),
      borderRadius: p.borderRadius(size * 0.34),
      boxShadow: [
        BoxShadow(
          color: p.accent.withValues(alpha: 0.25),
          blurRadius: 10,
          offset: const Offset(0, 3),
        ),
      ],
    ),
    child: Icon(Icons.all_inclusive_rounded, size: size * 0.62, color: p.onAccent),
  );

  Widget _quickCommandPill(Palette p, WorkbenchStrings s) => InkWell(
    borderRadius: p.borderRadius(12),
    onTap: onOpenCommandPalette,
    child: Container(
      height: 36,
      padding: const EdgeInsets.symmetric(horizontal: 10),
      decoration: BoxDecoration(
        color: p.surface.withValues(alpha: p.dark ? 0.24 : 0.45),
        borderRadius: p.borderRadius(12),
        border: Border.all(color: p.line),
      ),
      child: Row(
        children: [
          Icon(Icons.bolt_rounded, size: 15, color: p.accent),
          const SizedBox(width: 7),
          Expanded(
            child: Text(
              s.commandPaletteTooltip.split(' ').first,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: TextStyle(fontSize: 11, color: p.muted),
            ),
          ),
          Container(
            padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 2),
            decoration: BoxDecoration(
              color: p.ink.withValues(alpha: 0.06),
              borderRadius: p.borderRadius(6),
            ),
            child: Text(
              'Ctrl K',
              style: TextStyle(
                fontSize: 9,
                color: p.muted,
                fontWeight: FontWeight.w600,
              ),
            ),
          ),
        ],
      ),
    ),
  );

  Widget _expandedNavItem(
    BuildContext context,
    Palette p,
    WorkbenchLabels labels,
    WorkbenchPage page, {
    int? count,
  }) {
    final title = labels.page(page);
    final selected = section == page;
    final duration = workbenchDuration(context, WorkbenchDurations.short);

    return Padding(
      padding: const EdgeInsets.only(bottom: 4),
      child: NeumorphicSurface(
        palette: p,
        depth: selected ? -0.9 : 0,
        borderRadius: p.borderRadius(12),
        child: AnimatedContainer(
          duration: duration,
          curve: WorkbenchCurves.springOut,
          decoration: BoxDecoration(
            color: selected
                ? p.accent.withValues(alpha: p.dark ? 0.18 : 0.12)
                : Colors.transparent,
            borderRadius: p.borderRadius(12),
            border: Border.all(
              color: selected
                  ? p.accent.withValues(alpha: 0.28)
                  : Colors.transparent,
            ),
          ),
          child: Material(
            color: Colors.transparent,
            child: InkWell(
              key: ValueKey('nav-${page.id}'),
              borderRadius: p.borderRadius(12),
              onTap: () => onSelectPage(page),
              child: Padding(
                padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 10),
                child: Row(
                  children: [
                    AnimatedContainer(
                      duration: duration,
                      curve: WorkbenchCurves.springOut,
                      width: 3,
                      height: selected ? 16 : 0,
                      decoration: BoxDecoration(
                        color: p.accent,
                        borderRadius: BorderRadius.circular(2),
                      ),
                    ),
                    SizedBox(width: selected ? 8 : 4),
                    Icon(
                      iconForPage(page, active: selected),
                      size: 18,
                      color: selected ? p.accent : p.muted,
                    ),
                    const SizedBox(width: 10),
                    Expanded(
                      child: Text(
                        title,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: TextStyle(
                          fontSize: 12,
                          fontWeight: selected
                              ? FontWeight.w600
                              : FontWeight.w400,
                          color: selected ? p.ink : p.muted,
                        ),
                      ),
                    ),
                    if (count != null && count > 0)
                      Container(
                        padding: const EdgeInsets.symmetric(
                          horizontal: 6,
                          vertical: 2,
                        ),
                        decoration: BoxDecoration(
                          color: selected
                              ? p.accent.withValues(alpha: 0.16)
                              : p.ink.withValues(alpha: 0.06),
                          borderRadius: p.borderRadius(8),
                        ),
                        child: Text(
                          '$count',
                          style: TextStyle(
                            fontSize: 10,
                            fontWeight: FontWeight.w600,
                            color: selected ? p.accent : p.muted,
                          ),
                        ),
                      ),
                  ],
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }

  Widget _collapsedNavItem(
    BuildContext context,
    Palette p,
    WorkbenchLabels labels,
    WorkbenchPage page, {
    int? count,
  }) {
    final title = labels.page(page);
    final selected = section == page;
    final duration = workbenchDuration(context, WorkbenchDurations.short);

    return Padding(
      padding: const EdgeInsets.only(bottom: 6),
      child: Tooltip(
        message: count != null && count > 0 ? '$title ($count)' : title,
        child: NeumorphicSurface(
          palette: p,
          depth: selected ? -0.9 : 0,
          borderRadius: p.borderRadius(12),
          child: AnimatedContainer(
            duration: duration,
            curve: WorkbenchCurves.springOut,
            width: 44,
            height: 40,
            decoration: BoxDecoration(
              color: selected
                  ? p.accent.withValues(alpha: p.dark ? 0.20 : 0.13)
                  : Colors.transparent,
              borderRadius: p.borderRadius(12),
              border: Border.all(
                color: selected
                    ? p.accent.withValues(alpha: 0.32)
                    : Colors.transparent,
              ),
            ),
            child: Material(
              color: Colors.transparent,
              child: InkWell(
                key: ValueKey('nav-${page.id}'),
                borderRadius: p.borderRadius(12),
                onTap: () => onSelectPage(page),
                child: Stack(
                  alignment: Alignment.center,
                  children: [
                    Icon(
                      iconForPage(page, active: selected),
                      size: 19,
                      color: selected ? p.accent : p.muted,
                    ),
                    if (count != null && count > 0)
                      Positioned(
                        top: 6,
                        right: 7,
                        child: Container(
                          width: 6,
                          height: 6,
                          decoration: BoxDecoration(
                            color: p.accent,
                            shape: BoxShape.circle,
                          ),
                        ),
                      ),
                  ],
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}

/// Floating glass bottom dock used on narrow mobile/compact viewports (< 760px).
class WorkbenchMobileNav extends StatelessWidget {
  const WorkbenchMobileNav({
    super.key,
    required this.palette,
    required this.section,
    required this.onSelectPage,
  });

  final Palette palette;
  final WorkbenchPage section;
  final ValueChanged<WorkbenchPage> onSelectPage;

  @override
  Widget build(BuildContext context) {
    final p = palette;
    final labels = WorkbenchLabelsScope.of(context);
    final duration = workbenchDuration(context, WorkbenchDurations.short);

    return Glass(
      componentId: 'navigation',
      p: p,
      radius: 22,
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 6),
        child: Row(
          children: [
            for (final page in WorkbenchPage.values)
              Expanded(
                child: InkWell(
                  key: ValueKey('nav-${page.id}'),
                  borderRadius: p.borderRadius(14),
                  onTap: () => onSelectPage(page),
                  child: AnimatedContainer(
                    duration: duration,
                    curve: WorkbenchCurves.springOut,
                    padding: const EdgeInsets.symmetric(
                      horizontal: 4,
                      vertical: 6,
                    ),
                    decoration: BoxDecoration(
                      color: section == page
                          ? p.accent.withValues(alpha: p.dark ? 0.22 : 0.14)
                          : Colors.transparent,
                      borderRadius: p.borderRadius(14),
                    ),
                    child: Column(
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        Icon(
                          WorkbenchRail.iconForPage(
                            page,
                            active: section == page,
                          ),
                          size: 18,
                          color: section == page ? p.accent : p.muted,
                        ),
                        const SizedBox(height: 2),
                        Text(
                          labels.page(page),
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: TextStyle(
                            fontSize: 9,
                            fontWeight: section == page
                                ? FontWeight.w600
                                : FontWeight.w400,
                            color: section == page ? p.accent : p.muted,
                          ),
                        ),
                      ],
                    ),
                  ),
                ),
              ),
          ],
        ),
      ),
    );
  }
}
