import 'dart:math' as math;
import 'package:material_ui/material_ui.dart';
import 'package:morrow_i18n/morrow_i18n.dart';
import '../appearance.dart';
import '../neumorphic_controls.dart';
import '../plugins/workbench_ids.dart';
import '../plugins/workbench_labels.dart';
import '../theme_plugins/theme_plugin_controller.dart';
import 'workbench_motion.dart';
import 'workbench_strings.dart';

/// Compact Bento-style hero card for the Overview workspace.
class WorkbenchBentoHero extends StatelessWidget {
  const WorkbenchBentoHero({
    super.key,
    required this.palette,
    required this.backdropArtwork,
    required this.inboxCount,
    required this.activeProjectsCount,
    required this.experimentsCount,
    required this.favoritesCount,
    required this.onSelectPage,
    required this.onCreateIdea,
    required this.wrapInteractions,
  });

  final Palette palette;
  final Widget backdropArtwork;
  final int inboxCount;
  final int activeProjectsCount;
  final int experimentsCount;
  final int favoritesCount;
  final ValueChanged<WorkbenchPage> onSelectPage;
  final VoidCallback onCreateIdea;
  final Widget Function(String id, Widget child) wrapInteractions;

  @override
  Widget build(BuildContext context) {
    final p = palette;
    final l = L10n.of(context);
    final s = WorkbenchStrings.of(context);
    final caption =
        ThemePluginScope.of(
          context,
        )?.plugin?.caption(Localizations.localeOf(context)) ??
        l.mainHeroCaption;

    final stats = [
      (WorkbenchPage.inbox, s.pulseInbox, inboxCount, Icons.inbox_outlined),
      (
        WorkbenchPage.projects,
        s.pulseProjects,
        activeProjectsCount,
        Icons.folder_open_rounded,
      ),
      (
        WorkbenchPage.laboratory,
        s.pulseExperiments,
        experimentsCount,
        Icons.science_outlined,
      ),
      (
        WorkbenchPage.favorites,
        s.pulseFavorites,
        favoritesCount,
        Icons.bookmark_border_rounded,
      ),
    ];

    return wrapInteractions(
      'hero',
      LayoutBuilder(
        builder: (context, constraints) => Glass(
          componentId: 'hero',
          p: p,
          radius: 22,
          child: ConstrainedBox(
            constraints: BoxConstraints(
              minHeight: p.uiTheme == null ? 152 : 196,
            ),
            child: Stack(
              children: [
                if (p.uiTheme != null &&
                    ThemePluginScope.of(context)?.plugin != null)
                  Positioned.fill(
                    child: ClipRRect(
                      borderRadius: p.componentBorderRadius(
                        22,
                        componentId: 'hero',
                      ),
                      child: Align(
                        alignment: Alignment.centerRight,
                        child: SizedBox(
                          width: constraints.maxWidth < 520
                              ? constraints.maxWidth
                              : math.min(constraints.maxWidth * 0.54, 360),
                          height: double.infinity,
                          child: Opacity(
                            opacity: constraints.maxWidth < 520 ? 0.16 : 0.92,
                            child: backdropArtwork,
                          ),
                        ),
                      ),
                    ),
                  )
                else
                  Positioned(
                    right: constraints.maxWidth < 520 ? -150 : -30,
                    top: -65,
                    bottom: -65,
                    width: 320,
                    child: IgnorePointer(
                      child: Opacity(
                        opacity: constraints.maxWidth < 520 ? 0.22 : 0.95,
                        child: backdropArtwork,
                      ),
                    ),
                  ),
                Padding(
                  padding: const EdgeInsets.fromLTRB(22, 18, 22, 18),
                  child: Column(
                    mainAxisSize: MainAxisSize.min,
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Row(
                        children: [
                          Container(
                            width: 6,
                            height: 6,
                            decoration: BoxDecoration(
                              color: p.accent,
                              shape: BoxShape.circle,
                            ),
                          ),
                          const SizedBox(width: 7),
                          Flexible(
                            child: Text(
                              caption,
                              maxLines: 1,
                              overflow: TextOverflow.ellipsis,
                              style: TextStyle(
                                fontSize: 9,
                                letterSpacing: 1.5,
                                fontWeight: FontWeight.w600,
                                color: p.accent,
                              ),
                            ),
                          ),
                        ],
                      ),
                      const SizedBox(height: 10),
                      Text(
                        l.mainGreeting,
                        style: TextStyle(
                          fontSize: 22,
                          height: 1.25,
                          fontWeight: FontWeight.w600,
                          letterSpacing: -0.5,
                          color: p.ink,
                        ),
                      ),
                      const SizedBox(height: 5),
                      Text(
                        l.mainHeroBody,
                        maxLines: 2,
                        overflow: TextOverflow.ellipsis,
                        style: TextStyle(
                          fontSize: 11,
                          height: 1.65,
                          color: p.muted,
                        ),
                      ),
                      const SizedBox(height: 14),
                      Wrap(
                        spacing: 8,
                        runSpacing: 8,
                        crossAxisAlignment: WrapCrossAlignment.center,
                        children: [
                          for (final stat in stats)
                            InkWell(
                              borderRadius: p.borderRadius(10),
                              onTap: () => onSelectPage(stat.$1),
                              child: Container(
                                padding: const EdgeInsets.symmetric(
                                  horizontal: 10,
                                  vertical: 6,
                                ),
                                decoration: BoxDecoration(
                                  color: p.surface.withValues(
                                    alpha: p.dark ? 0.34 : 0.56,
                                  ),
                                  borderRadius: p.borderRadius(10),
                                  border: Border.all(color: p.line),
                                ),
                                child: Row(
                                  mainAxisSize: MainAxisSize.min,
                                  children: [
                                    Icon(stat.$4, size: 13, color: p.accent),
                                    const SizedBox(width: 5),
                                    Text(
                                      stat.$2,
                                      style: TextStyle(
                                        fontSize: 10,
                                        color: p.muted,
                                      ),
                                    ),
                                    const SizedBox(width: 5),
                                    Text(
                                      '${stat.$3}',
                                      style: TextStyle(
                                        fontSize: 11,
                                        fontWeight: FontWeight.w700,
                                        color: p.ink,
                                      ),
                                    ),
                                  ],
                                ),
                              ),
                            ),
                          InkWell(
                            borderRadius: p.borderRadius(10),
                            onTap: onCreateIdea,
                            child: Container(
                              padding: const EdgeInsets.symmetric(
                                horizontal: 11,
                                vertical: 6,
                              ),
                              decoration: BoxDecoration(
                                color: p.accent.withValues(alpha: 0.14),
                                borderRadius: p.borderRadius(10),
                                border: Border.all(
                                  color: p.accent.withValues(alpha: 0.32),
                                ),
                              ),
                              child: Row(
                                mainAxisSize: MainAxisSize.min,
                                children: [
                                  Text(
                                    l.mainCaptureNow,
                                    style: TextStyle(
                                      fontSize: 11,
                                      color: p.accent,
                                      fontWeight: FontWeight.w600,
                                    ),
                                  ),
                                  const SizedBox(width: 5),
                                  Icon(
                                    Icons.arrow_forward_rounded,
                                    color: p.accent,
                                    size: 14,
                                  ),
                                ],
                              ),
                            ),
                          ),
                        ],
                      ),
                    ],
                  ),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

/// Filter pills, active search chip, and sort menu ribbon above the card grid.
class WorkspaceFilterBar extends StatelessWidget {
  const WorkspaceFilterBar({
    super.key,
    required this.palette,
    required this.section,
    required this.filter,
    required this.pageFilters,
    required this.query,
    required this.visibleCount,
    required this.queryPending,
    required this.sortMenu,
    required this.onSelectFilter,
    required this.onClearQuery,
  });

  final Palette palette;
  final WorkbenchPage section;
  final WorkbenchFilter filter;
  final List<WorkbenchFilter> pageFilters;
  final String query;
  final int visibleCount;
  final bool queryPending;
  final Widget sortMenu;
  final ValueChanged<WorkbenchFilter> onSelectFilter;
  final VoidCallback onClearQuery;

  @override
  Widget build(BuildContext context) {
    final p = palette;
    final l = L10n.of(context);
    final s = WorkbenchStrings.of(context);
    final labels = WorkbenchLabelsScope.of(context);
    final duration = workbenchDuration(context, WorkbenchDurations.short);

    return Wrap(
      alignment: WrapAlignment.spaceBetween,
      crossAxisAlignment: WrapCrossAlignment.center,
      spacing: 10,
      runSpacing: 10,
      children: [
        Wrap(
          crossAxisAlignment: WrapCrossAlignment.center,
          spacing: 6,
          runSpacing: 6,
          children: [
            Text(
              section == WorkbenchPage.overview
                  ? l.mainRecentThoughts
                  : labels.page(section),
              style: TextStyle(
                fontSize: 14,
                color: p.ink,
                fontWeight: FontWeight.w600,
              ),
            ),
            Container(
              padding: const EdgeInsets.symmetric(horizontal: 7, vertical: 2),
              decoration: BoxDecoration(
                color: p.ink.withValues(alpha: 0.06),
                borderRadius: p.borderRadius(7),
              ),
              child: Text(
                queryPending ? '…' : visibleCount.toString().padLeft(2, '0'),
                style: TextStyle(fontSize: 10, color: p.muted),
              ),
            ),
            const SizedBox(width: 4),
            for (final label in pageFilters)
              InkWell(
                key: ValueKey('filter-${label.id}'),
                borderRadius: p.borderRadius(9),
                onTap: () => onSelectFilter(label),
                child: NeumorphicSurface(
                  palette: p,
                  depth: filter == label ? -0.8 : 0,
                  borderRadius: p.borderRadius(9),
                  child: AnimatedContainer(
                    duration: duration,
                    curve: WorkbenchCurves.springOut,
                    padding: const EdgeInsets.symmetric(
                      horizontal: 11,
                      vertical: 6,
                    ),
                    decoration: BoxDecoration(
                      color: filter == label
                          ? p.accent.withValues(alpha: p.dark ? 0.22 : 0.13)
                          : p.surface.withValues(alpha: p.dark ? 0.16 : 0.30),
                      borderRadius: p.borderRadius(9),
                      border: Border.all(
                        color: filter == label
                            ? p.accent.withValues(alpha: 0.35)
                            : p.line,
                      ),
                    ),
                    child: Text(
                      labels.filter(label),
                      style: TextStyle(
                        fontSize: 11,
                        fontWeight: filter == label
                            ? FontWeight.w600
                            : FontWeight.w400,
                        color: filter == label ? p.accent : p.muted,
                      ),
                    ),
                  ),
                ),
              ),
            if (query.isNotEmpty)
              InkWell(
                borderRadius: p.borderRadius(9),
                onTap: onClearQuery,
                child: Container(
                  padding: const EdgeInsets.symmetric(
                    horizontal: 9,
                    vertical: 6,
                  ),
                  decoration: BoxDecoration(
                    color: p.accent.withValues(alpha: 0.12),
                    borderRadius: p.borderRadius(9),
                    border: Border.all(color: p.accent.withValues(alpha: 0.3)),
                  ),
                  child: Row(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      Text(
                        s.filterChipLabel(query),
                        style: TextStyle(
                          fontSize: 10,
                          color: p.accent,
                          fontWeight: FontWeight.w600,
                        ),
                      ),
                      const SizedBox(width: 4),
                      Icon(Icons.close_rounded, size: 12, color: p.accent),
                    ],
                  ),
                ),
              ),
          ],
        ),
        sortMenu,
      ],
    );
  }
}
