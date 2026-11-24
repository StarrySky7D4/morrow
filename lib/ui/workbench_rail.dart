import 'dart:ui' as ui;
import 'package:flutter/rendering.dart' show OverflowBoxFit;
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
    final duration = workbenchDuration(context, WorkbenchDurations.medium);

    return TweenAnimationBuilder<double>(
      tween: Tween(end: expanded ? 1.0 : 0.0),
      duration: duration,
      curve: WorkbenchCurves.emphasized,
      builder: (context, t, _) {
        final currentWidth = ui.lerpDouble(68.0, expandedWidth, t)!;
        final hPad = ui.lerpDouble(10.0, 14.0, t)!;
        final expandFade = ((t - 0.30) / 0.70).clamp(0.0, 1.0);
        final collapseFade = (1.0 - t / 0.55).clamp(0.0, 1.0);
        final showExpandedDetails = t >= 0.30;
        final showCollapsedDetails = t <= 0.55;
        final innerExpandedWidth = expandedWidth - 28;

        return SizedBox(
          width: currentWidth,
          child: componentInteractions(
            'navigation',
            Glass(
              componentId: 'navigation',
              p: p,
              radius: 24,
              child: Padding(
                padding: EdgeInsets.fromLTRB(hPad, 18, hPad, 14),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
                    _buildMorphHeader(
                      context,
                      p,
                      l,
                      t,
                      expandFade,
                      showExpandedDetails,
                    ),
                    SizedBox(height: ui.lerpDouble(10, 16, t)!),
                    _buildQuickCommand(
                      p,
                      s,
                      t,
                      expandFade,
                      showExpandedDetails,
                      innerExpandedWidth,
                    ),
                    SizedBox(height: ui.lerpDouble(12, 18, t)!),
                    Expanded(
                      child: SingleChildScrollView(
                        child: Column(
                          crossAxisAlignment: CrossAxisAlignment.stretch,
                          children: [
                            if (t > 0.02)
                              ClipRect(
                                child: Align(
                                  alignment: Alignment.topLeft,
                                  heightFactor: t,
                                  child: Offstage(
                                    offstage: !showExpandedDetails,
                                    child: Opacity(
                                      opacity: expandFade,
                                      alwaysIncludeSemantics: true,
                                      child: Padding(
                                        padding: const EdgeInsets.only(
                                          left: 10,
                                          bottom: 10,
                                        ),
                                        child: Text(
                                          l.mainMySpace,
                                          maxLines: 1,
                                          softWrap: false,
                                          overflow: TextOverflow.ellipsis,
                                          style: TextStyle(
                                            fontSize: 10,
                                            color: p.muted,
                                            letterSpacing: 1.4,
                                            fontWeight: FontWeight.w600,
                                          ),
                                        ),
                                      ),
                                    ),
                                  ),
                                ),
                              ),
                            _morphNavItem(
                              context,
                              p,
                              labels,
                              WorkbenchPage.overview,
                              t,
                              expandFade,
                              collapseFade,
                              showExpandedDetails,
                              showCollapsedDetails,
                              innerExpandedWidth,
                            ),
                            _morphNavItem(
                              context,
                              p,
                              labels,
                              WorkbenchPage.inbox,
                              t,
                              expandFade,
                              collapseFade,
                              showExpandedDetails,
                              showCollapsedDetails,
                              innerExpandedWidth,
                              count: counts[WorkbenchPage.inbox],
                            ),
                            _morphNavItem(
                              context,
                              p,
                              labels,
                              WorkbenchPage.projects,
                              t,
                              expandFade,
                              collapseFade,
                              showExpandedDetails,
                              showCollapsedDetails,
                              innerExpandedWidth,
                              count: counts[WorkbenchPage.projects],
                            ),
                            _morphNavItem(
                              context,
                              p,
                              labels,
                              WorkbenchPage.laboratory,
                              t,
                              expandFade,
                              collapseFade,
                              showExpandedDetails,
                              showCollapsedDetails,
                              innerExpandedWidth,
                              count: counts[WorkbenchPage.laboratory],
                            ),
                            Padding(
                              padding: EdgeInsets.symmetric(
                                vertical: ui.lerpDouble(8, 10, t)!,
                                horizontal: ui.lerpDouble(8, 10, t)!,
                              ),
                              child: Divider(height: 1, color: p.line),
                            ),
                            _morphNavItem(
                              context,
                              p,
                              labels,
                              WorkbenchPage.favorites,
                              t,
                              expandFade,
                              collapseFade,
                              showExpandedDetails,
                              showCollapsedDetails,
                              innerExpandedWidth,
                              count: counts[WorkbenchPage.favorites],
                            ),
                          ],
                        ),
                      ),
                    ),
                    if (t > 0.02)
                      ClipRect(
                        child: Align(
                          alignment: Alignment.topCenter,
                          heightFactor: t,
                          child: Offstage(
                            offstage: t < 0.05,
                            child: Opacity(
                              opacity: ((t - 0.2) / 0.8).clamp(0.0, 1.0),
                              alwaysIncludeSemantics: true,
                              child: Padding(
                                padding: const EdgeInsets.only(
                                  top: 14,
                                  bottom: 14,
                                ),
                                child: SizedBox(
                                  width: (currentWidth - 2 * hPad).clamp(
                                    0.0,
                                    innerExpandedWidth,
                                  ),
                                  child: ClipRect(
                                    child: OverflowBox(
                                      alignment: Alignment.topLeft,
                                      minWidth: innerExpandedWidth,
                                      maxWidth: innerExpandedWidth,
                                      fit: OverflowBoxFit.deferToChild,
                                      child: componentInteractions(
                                        'corner-tips',
                                        cornerTipSurface(
                                          child: Column(
                                            crossAxisAlignment:
                                                CrossAxisAlignment.start,
                                            children: [
                                              Row(
                                                children: [
                                                  Icon(
                                                    Icons.wb_twilight_rounded,
                                                    color: p.accent,
                                                    size: 18,
                                                  ),
                                                  const Spacer(),
                                                  Container(
                                                    width: 6,
                                                    height: 6,
                                                    decoration: BoxDecoration(
                                                      color: p.accent
                                                          .withValues(
                                                            alpha: 0.45,
                                                          ),
                                                      shape: BoxShape.circle,
                                                    ),
                                                  ),
                                                ],
                                              ),
                                              const SizedBox(height: 8),
                                              RotatingTip(
                                                key: const ValueKey(
                                                  'corner-tips',
                                                ),
                                                lines:
                                                    TipPreferencesScope.maybeOf(
                                                      context,
                                                    )?.lines(
                                                      'corner-tips',
                                                      localizedCornerTips(
                                                        context,
                                                      ),
                                                    ) ??
                                                    localizedCornerTips(
                                                      context,
                                                    ),
                                                style: TextStyle(
                                                  fontSize: 10,
                                                  color: p.muted,
                                                  height: 1.7,
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
                            ),
                          ),
                        ),
                      )
                    else
                      const SizedBox(height: 14),
                    _buildMorphFooter(
                      context,
                      p,
                      l,
                      s,
                      t,
                      expandFade,
                      collapseFade,
                      showExpandedDetails,
                      showCollapsedDetails,
                    ),
                  ],
                ),
              ),
            ),
          ),
        );
      },
    );
  }

  Widget _buildMorphHeader(
    BuildContext context,
    Palette p,
    AppLocalizations l,
    double t,
    double expandFade,
    bool showExpandedDetails,
  ) {
    final logoSize = ui.lerpDouble(34.0, 30.0, t)!;
    return SizedBox(
      height: ui.lerpDouble(84.0, 42.0, t)!,
      child: Stack(
        children: [
          Align(
            alignment: Alignment.lerp(
              Alignment.topCenter,
              Alignment.centerLeft,
              t,
            )!,
            child: Padding(
              padding: EdgeInsets.only(left: 6 * t),
              child: _brandLogo(p, logoSize),
            ),
          ),
          if (showExpandedDetails)
            Positioned(
              left: 46,
              right: 38,
              top: 0,
              bottom: 0,
              child: Opacity(
                opacity: expandFade,
                alwaysIncludeSemantics: true,
                child: Align(
                  alignment: Alignment.centerLeft,
                  child: Column(
                    mainAxisSize: MainAxisSize.min,
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        'Morrow',
                        maxLines: 1,
                        softWrap: false,
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
                        softWrap: false,
                        overflow: TextOverflow.ellipsis,
                        style: TextStyle(fontSize: 9, color: p.muted),
                      ),
                    ],
                  ),
                ),
              ),
            ),
          Align(
            alignment: Alignment.lerp(
              Alignment.bottomCenter,
              Alignment.centerRight,
              t,
            )!,
            child: Tooltip(
              message: expanded ? l.mainCollapseSidebar : l.mainExpandSidebar,
              child: Semantics(
                button: true,
                child: Material(
                  type: MaterialType.transparency,
                  child: InkWell(
                    key: const ValueKey('sidebar-toggle'),
                    borderRadius: p.borderRadius(11),
                    onTap: onToggleExpanded,
                    child: AnimatedContainer(
                      duration: workbenchDuration(
                        context,
                        WorkbenchDurations.micro,
                      ),
                      curve: WorkbenchCurves.snappy,
                      width: 34,
                      height: 34,
                      decoration: BoxDecoration(
                        color: Colors.transparent,
                        borderRadius: p.borderRadius(11),
                        border: Border.all(color: Colors.transparent),
                      ),
                      child: Center(
                        child: AnimatedRotation(
                          turns: expanded ? 0 : 0.5,
                          duration: workbenchDuration(
                            context,
                            WorkbenchDurations.medium,
                          ),
                          curve: WorkbenchCurves.emphasized,
                          child: Icon(
                            Icons.chevron_left_rounded,
                            size: 18,
                            color: p.muted,
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
    );
  }

  Widget _buildQuickCommand(
    Palette p,
    WorkbenchStrings s,
    double t,
    double expandFade,
    bool showExpandedDetails,
    double innerExpandedWidth,
  ) => Tooltip(
    message: s.commandPaletteTooltip,
    child: InkWell(
      borderRadius: p.borderRadius(12),
      onTap: onOpenCommandPalette,
      child: Container(
        height: ui.lerpDouble(38, 36, t)!,
        decoration: BoxDecoration(
          color: p.surface.withValues(
            alpha: p.dark
                ? ui.lerpDouble(0.28, 0.24, t)!
                : ui.lerpDouble(0.48, 0.45, t)!,
          ),
          borderRadius: p.borderRadius(12),
          border: Border.all(color: p.line),
        ),
        child: Stack(
          alignment: Alignment.centerLeft,
          children: [
            Align(
              alignment: Alignment.lerp(
                Alignment.center,
                Alignment.centerLeft,
                t,
              )!,
              child: Padding(
                padding: EdgeInsets.only(left: 10 * t),
                child: Icon(
                  t < 0.5 ? Icons.search_rounded : Icons.bolt_rounded,
                  size: t < 0.5 ? 18 : 15,
                  color: t < 0.5 ? p.muted : p.accent,
                ),
              ),
            ),
            if (showExpandedDetails)
              Positioned.fill(
                left: 32,
                right: 10,
                child: Opacity(
                  opacity: expandFade,
                  alwaysIncludeSemantics: true,
                  child: ClipRect(
                    child: OverflowBox(
                      alignment: Alignment.centerLeft,
                      minWidth: innerExpandedWidth - 46,
                      maxWidth: innerExpandedWidth - 46,
                      child: Row(
                        crossAxisAlignment: CrossAxisAlignment.center,
                        children: [
                          Expanded(
                            child: Text(
                              s.commandPaletteTooltip.split(' ').first,
                              maxLines: 1,
                              softWrap: false,
                              overflow: TextOverflow.ellipsis,
                              style: TextStyle(
                                fontSize: 11,
                                height: 1.2,
                                color: p.muted,
                              ),
                            ),
                          ),
                          Container(
                            height: 20,
                            alignment: Alignment.center,
                            padding: const EdgeInsets.symmetric(horizontal: 6),
                            decoration: BoxDecoration(
                              color: p.ink.withValues(alpha: 0.06),
                              borderRadius: p.borderRadius(6),
                            ),
                            child: Text(
                              'Ctrl K',
                              style: TextStyle(
                                fontSize: 9.5,
                                height: 1.0,
                                color: p.muted,
                                fontWeight: FontWeight.w600,
                              ),
                            ),
                          ),
                        ],
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

  Widget _buildMorphFooter(
    BuildContext context,
    Palette p,
    AppLocalizations l,
    WorkbenchStrings s,
    double t,
    double expandFade,
    double collapseFade,
    bool showExpandedDetails,
    bool showCollapsedDetails,
  ) {
    final trailingActions = <Widget>[
      if (hasPluginSettings)
        IconButton(
          tooltip: s.actionPluginSettings,
          visualDensity: VisualDensity.compact,
          constraints: const BoxConstraints(minWidth: 30, minHeight: 30),
          padding: EdgeInsets.zero,
          onPressed: onOpenPluginSettings,
          icon: Icon(Icons.extension_outlined, size: 17, color: p.muted),
        ),
      IconButton(
        tooltip: s.focusModeTooltip,
        visualDensity: VisualDensity.compact,
        constraints: const BoxConstraints(minWidth: 30, minHeight: 30),
        padding: EdgeInsets.zero,
        onPressed: onToggleFocusMode,
        icon: Icon(
          focusMode ? Icons.fullscreen_exit_rounded : Icons.fullscreen_rounded,
          size: 17,
          color: focusMode ? p.accent : p.muted,
        ),
      ),
    ];

    return Column(
      mainAxisSize: MainAxisSize.min,
      children: [
        if (showCollapsedDetails)
          ClipRect(
            child: Align(
              alignment: Alignment.bottomCenter,
              heightFactor: (1 - t).clamp(0.0, 1.0),
              child: Opacity(
                opacity: collapseFade,
                alwaysIncludeSemantics: true,
                child: Padding(
                  padding: const EdgeInsets.only(bottom: 8),
                  child: Tooltip(
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
                          boxShadow: [
                            BoxShadow(
                              color: p.accent.withValues(
                                alpha: p.dark ? 0.30 : 0.24,
                              ),
                              blurRadius: 10,
                              offset: const Offset(0, 3),
                            ),
                          ],
                        ),
                        child: Icon(
                          Icons.add_rounded,
                          size: 20,
                          color: p.onAccent,
                        ),
                      ),
                    ),
                  ),
                ),
              ),
            ),
          ),
        Container(
          padding: EdgeInsets.symmetric(
            horizontal: ui.lerpDouble(4, 8, t)!,
            vertical: ui.lerpDouble(4, 8, t)!,
          ),
          decoration: BoxDecoration(
            color: t > 0.1
                ? p.surface.withValues(alpha: (p.dark ? 0.2 : 0.35) * t)
                : Colors.transparent,
            borderRadius: p.borderRadius(14),
            border: Border.all(
              color: t > 0.1 ? p.line.withValues(alpha: t) : Colors.transparent,
            ),
          ),
          child: t < 0.5
              ? Column(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    for (var i = 0; i < trailingActions.length; i++) ...[
                      if (i > 0) const SizedBox(height: 4),
                      trailingActions[i],
                    ],
                  ],
                )
              : Row(
                  children: [
                    if (showExpandedDetails) ...[
                      Opacity(
                        opacity: expandFade,
                        alwaysIncludeSemantics: true,
                        child: CircleAvatar(
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
                      ),
                      const SizedBox(width: 8),
                      Expanded(
                        child: Opacity(
                          opacity: expandFade,
                          alwaysIncludeSemantics: true,
                          child: Text(
                            l.mainPersonalWorkspace,
                            maxLines: 1,
                            softWrap: false,
                            overflow: TextOverflow.ellipsis,
                            style: TextStyle(
                              fontSize: 11,
                              color: p.ink,
                              fontWeight: FontWeight.w500,
                            ),
                          ),
                        ),
                      ),
                    ] else
                      const Spacer(),
                    ...trailingActions,
                  ],
                ),
        ),
      ],
    );
  }

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

  Widget _morphNavItem(
    BuildContext context,
    Palette p,
    WorkbenchLabels labels,
    WorkbenchPage page,
    double t,
    double expandFade,
    double collapseFade,
    bool showExpandedDetails,
    bool showCollapsedDetails,
    double innerExpandedWidth, {
    int? count,
  }) {
    final title = labels.page(page);
    final selected = section == page;
    final duration = workbenchDuration(context, WorkbenchDurations.short);

    return Padding(
      padding: EdgeInsets.only(bottom: ui.lerpDouble(6, 4, t)!),
      child: Tooltip(
        message: t < 0.5
            ? (count != null && count > 0 ? '$title ($count)' : title)
            : '',
        child: NeumorphicSurface(
          palette: p,
          depth: selected ? -0.9 : 0,
          borderRadius: p.borderRadius(12),
          child: AnimatedContainer(
            duration: duration,
            curve: WorkbenchCurves.springOut,
            height: 40,
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
                child: Stack(
                  alignment: Alignment.centerLeft,
                  children: [
                    if (t > 0.15)
                      Positioned(
                        left: 10,
                        child: AnimatedContainer(
                          duration: duration,
                          curve: WorkbenchCurves.springOut,
                          width: 3 * expandFade,
                          height: selected ? 16 : 0,
                          decoration: BoxDecoration(
                            color: p.accent,
                            borderRadius: BorderRadius.circular(2),
                          ),
                        ),
                      ),
                    Align(
                      alignment: Alignment.lerp(
                        Alignment.center,
                        Alignment.centerLeft,
                        t,
                      )!,
                      child: Padding(
                        padding: EdgeInsets.only(left: (selected ? 21 : 17) * t),
                        child: Icon(
                          iconForPage(page, active: selected),
                          size: 18,
                          color: selected ? p.accent : p.muted,
                        ),
                      ),
                    ),
                    if (showCollapsedDetails && count != null && count > 0)
                      Positioned(
                        top: 6,
                        right: 7,
                        child: Opacity(
                          opacity: collapseFade,
                          alwaysIncludeSemantics: true,
                          child: Container(
                            width: 6,
                            height: 6,
                            decoration: BoxDecoration(
                              color: p.accent,
                              shape: BoxShape.circle,
                            ),
                          ),
                        ),
                      ),
                    if (showExpandedDetails)
                      Positioned.fill(
                        left: 48,
                        right: 10,
                        child: Opacity(
                          opacity: expandFade,
                          alwaysIncludeSemantics: true,
                          child: ClipRect(
                            child: OverflowBox(
                              alignment: Alignment.centerLeft,
                              minWidth: innerExpandedWidth - 60,
                              maxWidth: innerExpandedWidth - 60,
                              child: Row(
                                children: [
                                  Expanded(
                                    child: Text(
                                      title,
                                      maxLines: 1,
                                      softWrap: false,
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
      useBackdropGroup: false,
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
