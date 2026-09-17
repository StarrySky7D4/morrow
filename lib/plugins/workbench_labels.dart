import 'package:morrow_i18n/morrow_i18n.dart';
import 'package:flutter/widgets.dart';
import 'workbench_ids.dart';

/// Presentation-only labels. This scope permits label changes without changing
/// navigation, queries or storage; it is not a language-pack/localization engine.
class WorkbenchLabels {
  const WorkbenchLabels({
    this.localization,
    this.pages = const {},
    this.filters = const {},
    this.sorts = const {},
  });
  final AppLocalizations? localization;
  AppLocalizations get l => localization ?? L10n.forLocale(const Locale('zh'));
  final Map<WorkbenchPage, String> pages;
  final Map<WorkbenchFilter, String> filters;
  final Map<WorkbenchSort, String> sorts;

  String page(WorkbenchPage value) =>
      pages[value] ??
      switch (value) {
        WorkbenchPage.overview => l.mainPageOverview,
        WorkbenchPage.inbox => l.mainPageInbox,
        WorkbenchPage.projects => l.mainPageProjects,
        WorkbenchPage.laboratory => l.mainPageLaboratory,
        WorkbenchPage.favorites => l.mainPageFavorites,
      };
  String sort(WorkbenchSort value) =>
      sorts[value] ??
      switch (value) {
        WorkbenchSort.recent => l.mainSortRecent,
        WorkbenchSort.favoritesFirst => l.mainSortFavorites,
        WorkbenchSort.title => l.mainSortTitle,
      };
  String filter(WorkbenchFilter value) =>
      filters[value] ??
      switch (value) {
        GeneralFilter.all => l.mainFilterAll,
        GeneralFilter.pendingTodos => l.mainFilterPending,
        GeneralFilter.attachments => l.mainFilterAttachments,
        GeneralFilter.favorites => l.mainFilterFavorites,
        GeneralFilter.image => l.mainFilterImage,
        GeneralFilter.media => l.mainFilterMedia,
        GeneralFilter.file => l.mainFilterFile,
        GeneralFilter.text => l.mainFilterText,
        StageFilter(:final stage) => switch (stage) {
          WorkbenchStage.unsorted => l.mainStageUnsorted,
          WorkbenchStage.organized => l.mainStageOrganized,
          WorkbenchStage.planned => l.mainStagePlanned,
          WorkbenchStage.active => l.mainStageActive,
          WorkbenchStage.completed => l.mainStageCompleted,
          WorkbenchStage.unverified => l.mainStageUnverified,
          WorkbenchStage.verifying => l.mainStageVerifying,
          WorkbenchStage.recorded => l.mainStageRecorded,
        },
      };
}

class WorkbenchLabelsScope extends InheritedWidget {
  const WorkbenchLabelsScope({
    super.key,
    required this.labels,
    required super.child,
  });
  final WorkbenchLabels labels;
  static WorkbenchLabels? maybeOf(BuildContext context) => context
      .dependOnInheritedWidgetOfExactType<WorkbenchLabelsScope>()
      ?.labels;
  static WorkbenchLabels of(BuildContext context) =>
      context
          .dependOnInheritedWidgetOfExactType<WorkbenchLabelsScope>()
          ?.labels ??
      WorkbenchLabels(localization: L10n.of(context));
  @override
  bool updateShouldNotify(WorkbenchLabelsScope oldWidget) =>
      oldWidget.labels != labels;
}
