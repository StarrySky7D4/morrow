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
    required this.scratchpadBuilder,
    required this.musicPanelBuilder,
    required this.appearanceBuilder,
    required this.onSelectPage,
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
  final WidgetBuilder scratchpadBuilder;
  final WidgetBuilder musicPanelBuilder;
  final WidgetBuilder appearanceBuilder;
  final ValueChanged<WorkbenchPage> onSelectPage;
  final bool compact;

  @override
  State<WorkbenchInspector> createState() => _WorkbenchInspectorState();
}

class _WorkbenchInspectorState extends State<WorkbenchInspector> {
  int _tabDirection = 1;

  @override
  void didUpdateWidget(WorkbenchInspector oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.tab != widget.tab) {
      _tabDirection = widget.tab.index >= oldWidget.tab.index ? 1 : -1;
    }
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
                        fontSize: 9,
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

    final body = SharedAxisSwitcher(
      axis: Axis.horizontal,
      fit: StackFit.expand,
      durationMs: 280,
      slideOffset: 0.10,
      direction: _tabDirection,
      child: SingleChildScrollView(
        key: PageStorageKey(
          'inspector-scroll-${widget.tab.name}-${widget.compact}',
        ),
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
  ) => Column(
    crossAxisAlignment: CrossAxisAlignment.stretch,
    children: [
      widget.quickCaptureBuilder(context),
      const SizedBox(height: 16),
      widget.scratchpadBuilder(context),
      const SizedBox(height: 16),
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
  );

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
                        fontSize: 11,
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
                      fontSize: 12,
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
                            style: TextStyle(fontSize: 10, color: p.muted),
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
              style: TextStyle(fontSize: 10, height: 1.6, color: p.muted),
            ),
          ],
        ),
      ),
    );
  }
}
