part of '../main.dart';

extension _CardOrder on _StudioState {
  CardOrderPreferences? get cardOrder => CardOrderScope.maybeOf(context);
  bool get manualCards => cardOrder?.manual(section.id) ?? false;
  String get customOrderLabel =>
      Localizations.localeOf(context).languageCode == 'zh'
      ? '自定义排序'
      : 'Custom order';
  bool get canSelectCardSort =>
      mounted && cardOrder != null && !cardOrder!.busy && !cardOrder!.loading;
  bool get canOrderCards =>
      canSelectCardSort &&
      menuWritable &&
      (!(widget.workbench?.writable ?? false) ||
          _queries.phase == QueryPhase.ready);
  Object get cardOrderRevision => (
    section,
    filter,
    query,
    sort,
    _contentGeneration,
    cardOrder?.revision,
    _queries.phase,
    _queries.ids,
  );
  bool onCurrentPage(Idea idea) => switch (section) {
    WorkbenchPage.inbox => idea.category == '灵感',
    WorkbenchPage.projects => idea.category == '进行中',
    WorkbenchPage.laboratory => idea.category == '实验',
    WorkbenchPage.favorites => idea.favorite,
    _ => true,
  };
  List<Idea> applyCardOrder(List<Idea> values) {
    if (!manualCards) return values;
    final byId = {for (final item in values) item.id: item};
    return [
      for (final id in cardOrder!.order(section.id, byId.keys)) byId[id]!,
    ];
  }

  void orderFailed() {
    if (!mounted) return;
    ScaffoldMessenger.of(context).showSnackBar(
      SnackBar(
        content: Text(
          Localizations.localeOf(context).languageCode == 'zh'
              ? '排序未能保存，原顺序已保留。请重试。'
              : 'Order could not be saved. The previous order is preserved.',
        ),
      ),
    );
  }

  Future<void> selectCardSort(String choice) async {
    if (!canSelectCardSort) return;
    final page = section;
    final previous = sort;
    refreshPage(
      () => sort = choice == 'custom'
          ? WorkbenchSort.recent
          : WorkbenchSort.values.byName(choice),
    );
    try {
      await cardOrder!.select(section.id, choice == 'custom');
    } catch (_) {
      if (mounted && section == page) refreshPage(() => sort = previous);
      orderFailed();
    }
  }

  Future<void> moveCard(Object source, Object target, bool after) async {
    if (!canOrderCards) return;
    try {
      await cardOrder!.move(
        section.id,
        source as String,
        target as String,
        after,
        all: [for (final idea in ideas.where(onCurrentPage)) idea.id],
        visible: [for (final idea in visibleIdeas) idea.id],
      );
    } catch (_) {
      orderFailed();
    }
  }

  bool canNudgeCard(Idea idea, int delta) {
    final index = visibleIdeas.indexWhere((item) => item.id == idea.id);
    return canOrderCards &&
        index >= 0 &&
        index + delta >= 0 &&
        index + delta < visibleIdeas.length;
  }

  void nudgeCard(Idea idea, int delta) {
    if (!canNudgeCard(idea, delta)) return;
    final index = visibleIdeas.indexWhere((item) => item.id == idea.id);
    moveCard(idea.id, visibleIdeas[index + delta].id, delta > 0);
  }

  Widget sortableCard(Idea idea, Widget child) => HoldReorder(
    key: ValueKey('card-drag:${idea.id}'),
    scope: this,
    id: idea.id,
    revision: cardOrderRevision,
    label: displayTitle(idea),
    enabled: canOrderCards,
    onMove: moveCard,
    child: child,
  );
  Widget cardSortMenu() => PopupMenuButton<String>(
    key: const ValueKey('workbench-sort'),
    tooltip: Localizations.localeOf(context).languageCode == 'zh'
        ? '排序 · 长按卡片可拖拽调整'
        : 'Sort · hold a card to reorder',
    initialValue: manualCards ? 'custom' : sort.name,
    enabled: canSelectCardSort,
    onSelected: selectCardSort,
    itemBuilder: (_) => [
      PopupMenuItem(value: 'custom', child: Text(customOrderLabel)),
      for (final value in WorkbenchSort.values)
        PopupMenuItem(
          value: value.name,
          child: Text(workbenchLabels.sort(value)),
        ),
    ],
    child: Padding(
      padding: const EdgeInsets.all(6),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          Icon(
            manualCards ? Icons.drag_indicator_rounded : Icons.sort_rounded,
            size: 13,
            color: p.muted,
          ),
          const SizedBox(width: 5),
          Text(
            manualCards ? customOrderLabel : workbenchLabels.sort(sort),
            style: TextStyle(fontSize: 10, color: p.muted),
          ),
        ],
      ),
    ),
  );
}
