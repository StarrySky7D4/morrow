/// UI identities are not display text and are not a new persisted/wire format.
/// Keep the v1 adapter below fixed when changing labels or adding localization.
enum WorkbenchPage {
  overview('workbench.page.overview'),
  inbox('workbench.page.inbox'),
  projects('workbench.page.projects'),
  laboratory('workbench.page.laboratory'),
  favorites('workbench.page.favorites');

  const WorkbenchPage(this.id);
  final String id;

  List<WorkbenchFilter> get filters => switch (this) {
    inbox => const [
      GeneralFilter.all,
      StageFilter(WorkbenchStage.unsorted),
      StageFilter(WorkbenchStage.organized),
    ],
    projects => const [
      GeneralFilter.all,
      StageFilter(WorkbenchStage.planned),
      StageFilter(WorkbenchStage.active),
      StageFilter(WorkbenchStage.completed),
    ],
    laboratory => const [
      GeneralFilter.all,
      StageFilter(WorkbenchStage.unverified),
      StageFilter(WorkbenchStage.verifying),
      StageFilter(WorkbenchStage.recorded),
    ],
    favorites => const [
      GeneralFilter.all,
      GeneralFilter.image,
      GeneralFilter.media,
      GeneralFilter.file,
      GeneralFilter.text,
    ],
    overview => const [
      GeneralFilter.all,
      GeneralFilter.pendingTodos,
      GeneralFilter.attachments,
      GeneralFilter.favorites,
    ],
  };
}

enum WorkbenchSort {
  recent('workbench.sort.recent'),
  favoritesFirst('workbench.sort.favorites-first'),
  title('workbench.sort.title');

  const WorkbenchSort(this.id);
  final String id;
}

/// These identify stage filters only. Existing Idea values and command payloads
/// remain v1 strings until a separately versioned content migration is available.
enum WorkbenchStage {
  unsorted('workbench.stage.inbox.unsorted'),
  organized('workbench.stage.inbox.organized'),
  planned('workbench.stage.project.planned'),
  active('workbench.stage.project.active'),
  completed('workbench.stage.project.completed'),
  unverified('workbench.stage.experiment.unverified'),
  verifying('workbench.stage.experiment.verifying'),
  recorded('workbench.stage.experiment.recorded');

  const WorkbenchStage(this.id);
  final String id;
}

sealed class WorkbenchFilter {
  const WorkbenchFilter();
  String get id;
}

enum GeneralFilter implements WorkbenchFilter {
  all('workbench.filter.all'),
  pendingTodos('workbench.filter.pending-todos'),
  attachments('workbench.filter.attachments'),
  favorites('workbench.filter.favorites'),
  image('workbench.filter.image'),
  media('workbench.filter.media'),
  file('workbench.filter.file'),
  text('workbench.filter.text');

  const GeneralFilter(this.id);
  @override
  final String id;
}

final class StageFilter extends WorkbenchFilter {
  const StageFilter(this.stage);
  final WorkbenchStage stage;
  @override
  String get id => 'workbench.filter.${stage.id}';
  @override
  bool operator ==(Object other) =>
      other is StageFilter && other.stage == stage;
  @override
  int get hashCode => stage.hashCode;
}

/// Explicit compatibility boundary. Values here are historical protocol tokens,
/// not a source of translated labels. Do not derive them from UI text or enum names.
abstract final class WorkbenchV1 {
  static String section(WorkbenchPage page) => switch (page) {
    WorkbenchPage.overview => '概览',
    WorkbenchPage.inbox => '灵感收件箱',
    WorkbenchPage.projects => '小项目',
    WorkbenchPage.laboratory => '实验室',
    WorkbenchPage.favorites => '已收藏',
  };
  static String sort(WorkbenchSort sort) => switch (sort) {
    WorkbenchSort.recent => '最近添加',
    WorkbenchSort.favoritesFirst => '收藏优先',
    WorkbenchSort.title => '标题排序',
  };
  static String stage(WorkbenchStage stage) => switch (stage) {
    WorkbenchStage.unsorted => '待整理',
    WorkbenchStage.organized => '已整理',
    WorkbenchStage.planned => '计划中',
    WorkbenchStage.active => '推进中',
    WorkbenchStage.completed => '已完成',
    WorkbenchStage.unverified => '待验证',
    WorkbenchStage.verifying => '验证中',
    WorkbenchStage.recorded => '已记录',
  };
  static String filter(WorkbenchFilter filter) => switch (filter) {
    GeneralFilter.all => '全部',
    GeneralFilter.pendingTodos => '有待办',
    GeneralFilter.attachments => '含附件',
    GeneralFilter.favorites => '仅收藏',
    GeneralFilter.image => '图像',
    GeneralFilter.media => '音视频',
    GeneralFilter.file => '文件',
    GeneralFilter.text => '文字',
    StageFilter(:final stage) => WorkbenchV1.stage(stage),
  };

  /// Component appearance settings already use these keys in saved preferences.
  /// Preserve them independently of the navigation and display identities.
  static String summaryComponentId(WorkbenchPage page) =>
      'summary:${section(page)}';
}
