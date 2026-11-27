import 'package:material_ui/material_ui.dart';
import 'package:morrow_i18n/morrow_i18n.dart';
import '../appearance.dart';
import '../music/music_controller.dart';
import '../plugins/workbench_ids.dart';
import 'workbench_motion.dart';
import 'workbench_strings.dart';

enum InspectorTab { studio, music, appearance }

/// Dockable right-side workbench inspector (and compact settings body) organizing
/// Quick Capture, Daily Tasks, Workspace Pulse, Music, and Appearance into 3 tabs.
class WorkbenchInspector extends StatefulWidget {
  const WorkbenchInspector({
    super.key,
    required this.palette,
    required this.tab,
    required this.onTabChanged,
    required this.music,
    required this.backgroundAudible,
    required this.remainingDailyTasks,
    required this.inboxCount,
    required this.activeProjectsCount,
    required this.experimentsCount,
    required this.favoritesCount,
    required this.quickCaptureBuilder,
    required this.timerBuilder,
    required this.scheduleBuilder,
    required this.countdownBuilder,
    required this.automationBuilder,
    required this.scratchpadBuilder,
    required this.musicPanelBuilder,
    required this.appearanceBuilder,
    required this.onSelectPage,
    this.currentPage,
    this.compact = false,
  });

  final Palette palette;
  final InspectorTab tab;
  final ValueChanged<InspectorTab> onTabChanged;
  final MusicController music;
  final bool backgroundAudible;
  final int remainingDailyTasks;
  final int inboxCount;
  final int activeProjectsCount;
  final int experimentsCount;
  final int favoritesCount;
  final WidgetBuilder quickCaptureBuilder;
  final WidgetBuilder timerBuilder;
  final WidgetBuilder scheduleBuilder;
  final WidgetBuilder countdownBuilder;
  final WidgetBuilder automationBuilder;
  final WidgetBuilder scratchpadBuilder;
  final WidgetBuilder musicPanelBuilder;
  final WidgetBuilder appearanceBuilder;
  final ValueChanged<WorkbenchPage> onSelectPage;
  final WorkbenchPage? currentPage;
  final bool compact;

  @override
  State<WorkbenchInspector> createState() => _WorkbenchInspectorState();
}

class _WorkbenchInspectorState extends State<WorkbenchInspector> {
  static const _collapsedStorageKey = ValueKey<String>(
    'inspector-collapsed-modules',
  );

  int _tabDirection = 1;
  Set<String> _collapsedModules = <String>{};

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    final stored = PageStorage.maybeOf(context)?.readState(
      context,
      identifier: _collapsedStorageKey,
    );
    if (stored is Set<String>) {
      _collapsedModules = Set<String>.from(stored);
    }
  }

  @override
  void didUpdateWidget(WorkbenchInspector oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.tab != widget.tab) {
      _tabDirection = widget.tab.index >= oldWidget.tab.index ? 1 : -1;
    }
  }

  void _toggleModule(String id) {
    setState(() {
      if (!_collapsedModules.remove(id)) {
        _collapsedModules.add(id);
      }
      PageStorage.maybeOf(context)?.writeState(
        context,
        Set<String>.from(_collapsedModules),
        identifier: _collapsedStorageKey,
      );
    });
  }

  @override
  Widget build(BuildContext context) {
    final p = widget.palette;
    final s = WorkbenchStrings.of(context);

    final segmentBar = ListenableBuilder(
      listenable: widget.music,
      builder: (context, _) => SlidingSegmentBar<InspectorTab>(
        value: widget.tab,
        onChanged: widget.onTabChanged,
        options: [
          SegmentOption(
            value: InspectorTab.studio,
            icon: Icons.space_dashboard_outlined,
            label: s.tabStudio,
            itemKey: const ValueKey('inspector-tab-studio'),
            badge: widget.remainingDailyTasks > 0
                ? Container(
                    padding: const EdgeInsets.symmetric(
                      horizontal: 5,
                      vertical: 1,
                    ),
                    decoration: BoxDecoration(
                      color: p.accent.withValues(alpha: 0.16),
                      borderRadius: p.borderRadius(6),
                    ),
                    child: Text(
                      '${widget.remainingDailyTasks}',
                      style: TextStyle(
                        fontSize: 10.5,
                        fontWeight: FontWeight.w700,
                        color: p.accent,
                      ),
                    ),
                  )
                : null,
          ),
          SegmentOption(
            value: InspectorTab.music,
            icon: Icons.music_note_rounded,
            label: s.tabMusic,
            itemKey: const ValueKey('inspector-tab-music'),
            badge: widget.music.playing
                ? Container(
                    width: 6,
                    height: 6,
                    decoration: BoxDecoration(
                      color: p.accent,
                      shape: BoxShape.circle,
                    ),
                  )
                : null,
          ),
          SegmentOption(
            value: InspectorTab.appearance,
            icon: Icons.tune_rounded,
            label: s.tabStyle,
            itemKey: const ValueKey('inspector-tab-appearance'),
          ),
        ],
      ),
    );

    final body = ScrollConfiguration(
      behavior: ScrollConfiguration.of(context).copyWith(scrollbars: false),
      child: SharedAxisSwitcher(
        axis: Axis.horizontal,
        fit: StackFit.expand,
        durationMs: 280,
        slideOffset: 0.10,
        direction: _tabDirection,
        child: SingleChildScrollView(
          key: PageStorageKey(
            'inspector-scroll-${widget.tab.name}-${widget.compact}',
          ),
          primary: false,
          padding: EdgeInsets.only(top: 14, bottom: widget.compact ? 28 : 18),
          child: KeyedSubtree(
            key: ValueKey('inspector-body-${widget.tab.name}'),
            child: switch (widget.tab) {
              InspectorTab.studio => _buildStudioTab(context, p, s),
              InspectorTab.music => _buildMusicTab(context, p, s),
              InspectorTab.appearance => widget.appearanceBuilder(context),
            },
          ),
        ),
      ),
    );

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        segmentBar,
        Expanded(child: body),
      ],
    );
  }

  Widget _buildStudioTab(
    BuildContext context,
    Palette p,
    WorkbenchStrings s,
  ) {
    final modules = [
      (
        'quick-capture',
        s.moduleQuickCapture,
        Icons.edit_note_rounded,
        widget.quickCaptureBuilder,
      ),
      ('timer', s.moduleTimer, Icons.timer_outlined, widget.timerBuilder),
      (
        'schedule',
        s.moduleSchedule,
        Icons.calendar_month_rounded,
        widget.scheduleBuilder,
      ),
      (
        'countdown',
        s.moduleCountdown,
        Icons.hourglass_bottom_rounded,
        widget.countdownBuilder,
      ),
      (
        'automation',
        s.moduleAutomation,
        Icons.bolt_rounded,
        widget.automationBuilder,
      ),
      (
        'scratchpad',
        s.moduleScratchpad,
        Icons.checklist_rounded,
        widget.scratchpadBuilder,
      ),
    ];
    final duration = workbenchDuration(context, WorkbenchDurations.short);

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Wrap(
          spacing: 6,
          runSpacing: 6,
          children: [
            for (final mod in modules) ...[
              Builder(
                builder: (context) {
                  final expanded = !_collapsedModules.contains(mod.$1);
                  return Tooltip(
                    message: s.moduleToggleTooltip(mod.$2, !expanded),
                    child: Material(
                      color: Colors.transparent,
                      child: InkWell(
                        key: ValueKey('inspector-module-toggle-${mod.$1}'),
                        borderRadius: p.borderRadius(9),
                        onTap: () => _toggleModule(mod.$1),
                        child: AnimatedContainer(
                          duration: duration,
                          curve: WorkbenchCurves.snappy,
                          padding: const EdgeInsets.symmetric(
                            horizontal: 8,
                            vertical: 5,
                          ),
                          decoration: BoxDecoration(
                            color: expanded
                                ? p.accent.withValues(alpha: p.dark ? 0.20 : 0.12)
                                : p.surface.withValues(
                                    alpha: p.dark ? 0.25 : 0.42,
                                  ),
                            borderRadius: p.borderRadius(9),
                            border: Border.all(
                              color: expanded
                                  ? p.accent.withValues(alpha: 0.36)
                                  : p.line,
                            ),
                          ),
                          child: Row(
                            mainAxisSize: MainAxisSize.min,
                            children: [
                              Icon(
                                mod.$3,
                                size: 12,
                                color: expanded ? p.accent : p.muted,
                              ),
                              const SizedBox(width: 4),
                              Text(
                                mod.$2,
                                style: TextStyle(
                                  fontSize: 11,
                                  fontWeight: expanded
                                      ? FontWeight.w600
                                      : FontWeight.w400,
                                  color: expanded ? p.accent : p.muted,
                                ),
                              ),
                            ],
                          ),
                        ),
                      ),
                    ),
                  );
                },
              ),
            ],
          ],
        ),
        for (final mod in modules)
          if (!_collapsedModules.contains(mod.$1))
            Padding(
              padding: const EdgeInsets.only(top: 12),
              child: mod.$4(context),
            ),
        if (widget.currentPage != WorkbenchPage.overview) ...[
          const SizedBox(height: 14),
          _WorkspacePulseCard(
            palette: p,
            strings: s,
            inboxCount: widget.inboxCount,
            activeProjectsCount: widget.activeProjectsCount,
            experimentsCount: widget.experimentsCount,
            favoritesCount: widget.favoritesCount,
            onSelectPage: widget.onSelectPage,
          ),
        ],
      ],
    );
  }

  Widget _buildMusicTab(
    BuildContext context,
    Palette p,
    WorkbenchStrings s,
  ) => Column(
    crossAxisAlignment: CrossAxisAlignment.stretch,
    children: [
      if (widget.backgroundAudible)
        Padding(
          padding: const EdgeInsets.only(bottom: 14),
          child: Glass(
            p: p,
            radius: 16,
            child: Padding(
              padding: const EdgeInsets.all(14),
              child: Row(
                children: [
                  Icon(Icons.volume_up_rounded, size: 18, color: p.accent),
                  const SizedBox(width: 10),
                  Expanded(
                    child: Text(
                      s.musicBlockedBanner,
                      style: TextStyle(
                        fontSize: 12,
                        height: 1.5,
                        color: p.muted,
                      ),
                    ),
                  ),
                ],
              ),
            ),
          ),
        ),
      widget.musicPanelBuilder(context),
    ],
  );
}

class _WorkspacePulseCard extends StatelessWidget {
  const _WorkspacePulseCard({
    required this.palette,
    required this.strings,
    required this.inboxCount,
    required this.activeProjectsCount,
    required this.experimentsCount,
    required this.favoritesCount,
    required this.onSelectPage,
  });

  final Palette palette;
  final WorkbenchStrings strings;
  final int inboxCount;
  final int activeProjectsCount;
  final int experimentsCount;
  final int favoritesCount;
  final ValueChanged<WorkbenchPage> onSelectPage;

  @override
  Widget build(BuildContext context) {
    final p = palette;
    final l = L10n.of(context);
    final tiles = [
      (
        WorkbenchPage.inbox,
        strings.pulseInbox,
        inboxCount,
        Icons.inbox_outlined,
      ),
      (
        WorkbenchPage.projects,
        strings.pulseProjects,
        activeProjectsCount,
        Icons.folder_open_rounded,
      ),
      (
        WorkbenchPage.laboratory,
        strings.pulseExperiments,
        experimentsCount,
        Icons.science_outlined,
      ),
      (
        WorkbenchPage.favorites,
        strings.pulseFavorites,
        favoritesCount,
        Icons.bookmark_border_rounded,
      ),
    ];

    return Glass(
      p: p,
      radius: 20,
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                Icon(Icons.insights_rounded, size: 16, color: p.accent),
                const SizedBox(width: 7),
                Expanded(
                  child: Text(
                    strings.workspacePulse,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: TextStyle(
                      fontSize: 13,
                      fontWeight: FontWeight.w600,
                      color: p.ink,
                    ),
                  ),
                ),
              ],
            ),
            const SizedBox(height: 12),
            GridView.count(
              crossAxisCount: 2,
              shrinkWrap: true,
              physics: const NeverScrollableScrollPhysics(),
              mainAxisSpacing: 8,
              crossAxisSpacing: 8,
              childAspectRatio: 1.75,
              children: [
                for (final tile in tiles)
                  InkWell(
                    borderRadius: p.borderRadius(12),
                    onTap: () => onSelectPage(tile.$1),
                    child: Container(
                      padding: const EdgeInsets.symmetric(
                        horizontal: 10,
                        vertical: 8,
                      ),
                      decoration: BoxDecoration(
                        color: p.surface.withValues(alpha: p.dark ? 0.25 : 0.45),
                        borderRadius: p.borderRadius(12),
                        border: Border.all(color: p.line),
                      ),
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        mainAxisAlignment: MainAxisAlignment.spaceBetween,
                        children: [
                          Row(
                            children: [
                              Icon(tile.$4, size: 13, color: p.accent),
                              const Spacer(),
                              Text(
                                '${tile.$3}',
                                style: TextStyle(
                                  fontSize: 15,
                                  fontWeight: FontWeight.w700,
                                  color: p.ink,
                                ),
                              ),
                            ],
                          ),
                          Text(
                            tile.$2,
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                            style: TextStyle(fontSize: 11, color: p.muted),
                          ),
                        ],
                      ),
                    ),
                  ),
              ],
            ),
            const SizedBox(height: 12),
            Text(
              l.mainCuriosity,
              style: TextStyle(fontSize: 11, height: 1.6, color: p.muted),
            ),
          ],
        ),
      ),
    );
  }
}
