import 'package:material_ui/material_ui.dart';

/// Bilingual UI copy for the refactored modern desktop workbench chrome.
/// Kept separate from `morrow_i18n` so generated Rust locale tables remain untouched.
class WorkbenchStrings {
  const WorkbenchStrings._(this.zh);

  final bool zh;

  static WorkbenchStrings of(BuildContext context) =>
      WorkbenchStrings._(Localizations.localeOf(context).languageCode == 'zh');

  String get tabStudio => zh ? '工作台' : 'Studio';
  String get tabMusic => zh ? '音乐' : 'Music';
  String get tabStyle => zh ? '外观' : 'Style';

  String get commandHint => zh ? '搜索卡片、跳转页面或执行操作…' : 'Search cards, jump to page, or run action…';
  String get commandEmpty => zh ? '没有匹配的命令或卡片' : 'No matching commands or cards';
  String get commandSectionCards => zh ? '卡片记录' : 'Cards';
  String get commandSectionNavigate => zh ? '前往页面' : 'Navigate';
  String get commandSectionActions => zh ? '快捷操作' : 'Quick actions';
  String commandFilterWorkspace(String q) =>
      zh ? '在工作台筛选 “$q”' : 'Filter workspace for "$q"';

  String get actionNewIdea => zh ? '新建想法' : 'New idea';
  String get actionFocusOn => zh ? '进入专注模式' : 'Enter focus mode';
  String get actionFocusOff => zh ? '退出专注模式' : 'Exit focus mode';
  String get actionExpandSidebar => zh ? '展开导航栏' : 'Expand sidebar';
  String get actionCollapseSidebar => zh ? '折叠为图标栏' : 'Collapse to icon rail';
  String get actionShowInspector => zh ? '打开右侧工作面板' : 'Open inspector panel';
  String get actionHideInspector => zh ? '收起右侧工作面板' : 'Hide inspector panel';
  String get actionSwitchGrid => zh ? '切换为瀑布流视图' : 'Switch to grid view';
  String get actionSwitchList => zh ? '切换为紧凑列表视图' : 'Switch to list view';
  String get actionLightTheme => zh ? '切换到浅色主题' : 'Switch to light theme';
  String get actionDarkTheme => zh ? '切换到深色主题' : 'Switch to dark theme';
  String get actionCycleGlass => zh ? '切换玻璃质感' : 'Cycle glass surface';
  String get actionComponentStyle => zh ? '组件材质设置' : 'Component materials';
  String get actionPluginSettings => zh ? '插件与宿主设置' : 'Plugin & host settings';

  String get densityGridTooltip => zh ? '瀑布流视图' : 'Masonry grid';
  String get densityListTooltip => zh ? '紧凑列表' : 'Compact list';
  String get focusModeTooltip => zh ? '专注模式 (Ctrl+Shift+F)' : 'Focus mode (Ctrl+Shift+F)';
  String get exitFocusTooltip => zh ? '退出专注模式 (Esc)' : 'Exit focus mode (Esc)';
  String get commandPaletteTooltip => zh ? '命令面板 (Ctrl+K)' : 'Command palette (Ctrl+K)';

  String get quickCaptureTitle => zh ? '随手记录' : 'Quick capture';
  String get quickCaptureSubtitle => zh ? '按 Enter 立即存入当前分区' : 'Press Enter to save into this space';
  String get workspacePulse => zh ? '空间脉搏' : 'Workspace pulse';
  String get pulseInbox => zh ? '灵感收件' : 'Inbox';
  String get pulseProjects => zh ? '推进项目' : 'Active';
  String get pulseExperiments => zh ? '实验记录' : 'Labs';
  String get pulseFavorites => zh ? '星标收藏' : 'Saved';
  String get musicBlockedBanner => zh
      ? '背景视频正在发声，音乐面板已自动让位。'
      : 'Background video audio is active; music playback is paused.';
  String get openCard => zh ? '打开' : 'Open';
  String get editCard => zh ? '编辑' : 'Edit';
  String filterChipLabel(String q) => zh ? '筛选: $q' : 'Filter: $q';
  String itemCountBadge(int count) => zh ? '$count 项' : '$count';

  String get fontPickerTitle => zh ? '系统字体' : 'System fonts';
  String get fontPickerSearchHint =>
      zh ? '搜索系统字体名称…' : 'Search system fonts…';
  String get fontPickerLoading =>
      zh ? '正在读取系统字体…' : 'Loading system fonts…';
  String get fontPickerEmpty =>
      zh ? '没有找到匹配的系统字体' : 'No matching system fonts';
  String get fontPickerRefreshTooltip =>
      zh ? '刷新系统字体列表' : 'Refresh system fonts';
  String get fontPickerDefaultSubtitle =>
      zh ? '跟随应用默认字体' : 'Use the app default typeface';
  String get fontPickerManualTitle =>
      zh ? '自定义字体名称或导入文件' : 'Custom family or font file';
  String get fontPickerActiveBadge => zh ? '当前字体' : 'Active';
  String fontPickerCount(int shown, int total) =>
      shown == total ? (zh ? '共 $total 款' : '$total fonts') : '$shown / $total';
}
