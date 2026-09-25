part of '../main.dart';

extension _ComponentMenus on _StudioState {
  bool get menuWritable =>
      (widget.workbench == null || widget.workbench!.writable) &&
      _pluginBusyOwner == null;

  bool canChangeCard(Idea idea) =>
      mounted &&
      menuWritable &&
      ideas.any((current) => identical(current, idea));

  List<ComponentMenuAction> componentActions(String id) => [
    if (id == 'hero' || id == 'empty' || id.startsWith('summary:'))
      ComponentMenuAction(
        label: l.mainNewIdea,
        icon: Icons.add,
        isEnabled: () => mounted && menuWritable,
        onSelected: createIdea,
      ),
    if (id == 'navigation')
      for (final page in WorkbenchPage.values)
        ComponentMenuAction(
          label: workbenchLabels.page(page),
          icon: Icons.folder_open_outlined,
          onSelected: () => selectSection(page),
        ),
    if (id == 'search' || id.startsWith('summary:'))
      ComponentMenuAction(
        label: l.mainClearSearch,
        icon: Icons.search_off,
        isEnabled: () => search.text.isNotEmpty || query.isNotEmpty,
        onSelected: () => refreshPage(() {
          search.clear();
          query = '';
        }),
      ),
    if (id == 'quick-capture')
      ComponentMenuAction(
        label: l.mainNewIdea,
        icon: Icons.note_add_outlined,
        isEnabled: () =>
            mounted && menuWritable && quickNote.text.trim().isNotEmpty,
        onSelected: saveQuickNote,
      ),
    if (id == 'daily') ...[
      ComponentMenuAction(
        label: l.mainTaskMarkComplete,
        icon: Icons.done_all,
        isEnabled: () => mounted && menuWritable,
        onSelected: () => setDailyCompletion(true),
      ),
      ComponentMenuAction(
        label: l.mainTaskMarkIncomplete,
        icon: Icons.restart_alt,
        isEnabled: () => mounted && menuWritable,
        onSelected: () => setDailyCompletion(false),
      ),
    ],
    if (id == 'footer')
      ComponentMenuAction(
        label: l.visualFooterLyrics,
        icon: music.showLyrics ? Icons.check : Icons.lyrics_outlined,
        isEnabled: () => mounted && menuWritable,
        onSelected: () => music.setShowLyrics(!music.showLyrics),
      ),
    if ((!p.overridesMaterials || TipPreferences.componentIds.contains(id)) &&
        componentEntries.containsKey(id))
      ComponentMenuAction(
        label: l.mainComponentSettings,
        icon: Icons.tune,
        isEnabled: () => mounted && menuWritable,
        onSelected: () => openComponentSettings(id, componentEntries[id]!),
      ),
  ];

  Widget componentInteractions(String id, Widget child) => ComponentContextMenu(
    key: ValueKey('component-menu-$id'),
    actions: () => componentActions(id),
    child: child,
  );

  void setDailyCompletion(bool done) {
    final tasks = dailyTipItems.map((item) => item.id).toSet();
    refreshPage(() {
      pruneDailyCompletion(tasks);
      done ? completed.addAll(tasks) : completed.removeAll(tasks);
    });
    persist();
  }

  List<TipItem> get dailyTipItems =>
      TipPreferencesScope.maybeOf(
        context,
      )?.items('daily', localizedDailyTips(context)) ??
      [
        for (final (i, text) in localizedDailyTips(context).indexed)
          TipItem(TipPreferences.defaultDailyIds[i], text),
      ];

  void pruneDailyCompletion(Set<String> active) => completed.removeWhere(
    (id) =>
        (id.startsWith('daily:') ||
            TipPreferences.defaultDailyIds.contains(id)) &&
        !active.contains(id),
  );

  void setDailyTipCompletion(String id, bool done) {
    final active = dailyTipItems.map((item) => item.id).toSet();
    if (!mounted || !menuWritable || !active.contains(id)) return;
    refreshPage(() {
      pruneDailyCompletion(active);
      done ? completed.add(id) : completed.remove(id);
    });
    persist();
  }

  List<ComponentMenuAction> extraCardActions(Idea idea) => [
    for (final delta in [-1, 1])
      ComponentMenuAction(
        label: Localizations.localeOf(context).languageCode == 'zh'
            ? (delta < 0 ? '前移卡片' : '后移卡片')
            : (delta < 0 ? 'Move card earlier' : 'Move card later'),
        icon: delta < 0 ? Icons.arrow_upward : Icons.arrow_downward,
        isEnabled: () => canNudgeCard(idea, delta),
        onSelected: () => nudgeCard(idea, delta),
      ),
    ComponentMenuAction(
      label: l.mainEdit,
      icon: Icons.edit_outlined,
      isEnabled: () => canChangeCard(idea),
      onSelected: () => openIdea(idea, initialAction: 'edit'),
    ),
    ComponentMenuAction(
      label: MaterialLocalizations.of(context).copyButtonLabel,
      icon: Icons.copy_outlined,
      onSelected: () => Clipboard.setData(
        ClipboardData(
          text: '${displayTitle(idea)}\n\n${displayDescription(idea)}',
        ),
      ),
    ),
    if (idea.category == '灵感')
      ComponentMenuAction(
        label: l.mainToProject,
        icon: Icons.drive_file_move_outline,
        isEnabled: () => canChangeCard(idea),
        onSelected: () => moveToProject(idea),
      ),
    ComponentMenuAction(
      label: l.mainDelete,
      icon: Icons.delete_outline,
      isEnabled: () => canChangeCard(idea),
      onSelected: () => openIdea(idea, initialAction: 'delete'),
    ),
  ];
}
