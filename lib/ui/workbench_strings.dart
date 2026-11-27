import 'package:material_ui/material_ui.dart';

import '../schedule/schedule_store.dart';
import '../widgets_suite/automation_engine.dart';
import '../widgets_suite/countdown_store.dart';
import '../widgets_suite/timer_controller.dart';

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
  String get moduleQuickCapture => zh ? '随手记' : 'Capture';
  String get moduleTimer => zh ? '定时器' : 'Timer';
  String get moduleSchedule => zh ? '日程' : 'Schedule';
  String get moduleCountdown => zh ? '倒数日' : 'Countdowns';
  String get moduleAutomation => zh ? '自动化' : 'Automation';
  String get moduleScratchpad => zh ? '便签' : 'Daily';
  String moduleToggleTooltip(String label, bool collapsed) => collapsed
      ? (zh ? '展开 $label' : 'Show $label')
      : (zh ? '收起 $label' : 'Hide $label');
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

  String get scheduleTitle => zh ? '日程与日历' : 'Calendar & Schedule';
  String get scheduleToday => zh ? '今天' : 'Today';
  String get scheduleTomorrow => zh ? '明天' : 'Tomorrow';
  String get scheduleAllDay => zh ? '全天' : 'All day';
  String get scheduleAdd => zh ? '新建日程' : 'New event';
  String get scheduleEdit => zh ? '编辑日程' : 'Edit event';
  String get scheduleDelete => zh ? '删除日程' : 'Delete event';
  String get scheduleEmptyDay =>
      zh ? '这一天还没有安排' : 'No events for this day';
  String get scheduleTitleHint => zh ? '输入日程标题…' : 'Event title…';
  String get scheduleTitleError =>
      zh ? '请输入有效的日程标题' : 'Please enter an event title';
  String get scheduleDateLabel => zh ? '日期' : 'Date';
  String get scheduleTimeLabel =>
      zh ? '时间（可选，如 14:30）' : 'Time (optional, e.g. 14:30)';
  String get scheduleTimeError =>
      zh ? '时间格式应为 HH:MM（如 09:30），留空为全天' : 'Use HH:MM (e.g. 09:30) or leave blank';
  String get scheduleNoteLabel => zh ? '备注（可选）' : 'Note (optional)';
  String get scheduleLinkCardLabel => zh ? '关联卡片' : 'Linked card';
  String get scheduleNoLinkedCard => zh ? '不关联卡片' : 'No linked card';
  String get scheduleMissingCard => zh ? '关联卡片已移除' : 'Linked card removed';
  String get scheduleActionLinkCard => zh ? '关联到日程' : 'Link to schedule';
  String get scheduleCardSectionTitle => zh ? '关联日程' : 'Linked schedule';
  String get scheduleCardEmpty => zh ? '暂未关联日程' : 'No linked events yet';
  String get scheduleCardAdd => zh ? '添加关联日程' : 'Add linked event';
  String get scheduleUnlinkTooltip => zh ? '取消与卡片关联' : 'Unlink from card';
  String get scheduleCancel => zh ? '取消' : 'Cancel';
  String get scheduleSave => zh ? '保存' : 'Save';
  String scheduleTodayPending(int count) =>
      zh ? '今日 $count' : '$count today';
  String scheduleMonthLabel(int year, int month) =>
      zh ? '$year年$month月' : '$year-${month.toString().padLeft(2, '0')}';
  String scheduleDaySummary(int month, int day, bool isToday) => zh
      ? '$month月$day日${isToday ? ' · 今天' : ''}'
      : '$month/$day${isToday ? ' · Today' : ''}';
  List<String> get scheduleWeekdays => zh
      ? const ['一', '二', '三', '四', '五', '六', '日']
      : const ['M', 'T', 'W', 'T', 'F', 'S', 'S'];
  String scheduleQuickDays(int offset) => switch (offset) {
    0 => scheduleToday,
    1 => scheduleTomorrow,
    _ => zh ? '+$offset 天' : '+${offset}d',
  };

  String formatScheduleStamp(ScheduleEntry entry, DateTime now) {
    final today = ScheduleEntry.dateOnly(now);
    final tomorrow = today.add(const Duration(days: 1));
    final target = entry.date;
    final datePart = target == today
        ? scheduleToday
        : target == tomorrow
        ? scheduleTomorrow
        : target.year == today.year
        ? '${target.month}/${target.day}'
        : '${target.year}/${target.month}/${target.day}';
    return entry.isAllDay ? datePart : '$datePart ${entry.timeText}';
  }

  String formatCardScheduleBadge(
    ScheduleEntry entry,
    DateTime now,
    int totalCount,
  ) {
    final stamp = formatScheduleStamp(entry, now);
    return totalCount > 1 ? '$stamp +${totalCount - 1}' : stamp;
  }

  String get timerTitle => zh ? '专注定时器' : 'Focus Timer';
  String get timerStart => zh ? '开始' : 'Start';
  String get timerPause => zh ? '暂停' : 'Pause';
  String get timerReset => zh ? '重置' : 'Reset';
  String get timerCustomMinutesLabel => zh ? '分' : 'Min';
  String get timerCustomSecondsLabel => zh ? '秒' : 'Sec';
  String get timerCustomApply => zh ? '应用' : 'Apply';
  String get timerCustomTooltip => zh ? '自定义倒计时时长' : 'Set custom duration';
  String get timerLinkCard => zh ? '关联卡片（累计时长）' : 'Link card (track focus)';
  String get timerNoCard => zh ? '自由专注（不绑定卡片）' : 'Free focus (no card)';
  String get timerFocusAction => zh ? '为此卡片专注' : 'Focus on this card';
  String timerTodayStats(int sessions, int minutes) => zh
      ? '今日 $sessions 次 · ${minutes}m'
      : '$sessions done · ${minutes}m';
  String timerModeLabel(WorkbenchTimerMode mode) => switch (mode) {
    WorkbenchTimerMode.focus => zh ? '专注 25m' : 'Focus 25m',
    WorkbenchTimerMode.shortBreak => zh ? '短休 5m' : 'Break 5m',
    WorkbenchTimerMode.longBreak => zh ? '长休 15m' : 'Long 15m',
    WorkbenchTimerMode.custom => zh ? '自定义' : 'Custom',
    WorkbenchTimerMode.stopwatch => zh ? '正计时' : 'Stopwatch',
  };

  String get countdownTitle => zh ? '倒数日' : 'Countdowns';
  String get countdownAdd => zh ? '新建倒数日' : 'New countdown';
  String get countdownEdit => zh ? '编辑倒数日' : 'Edit countdown';
  String get countdownEmpty =>
      zh ? '添加重要节点，置顶卡片或到底部' : 'Add milestones to pin or track in footer';
  String get countdownTitleHint => zh ? '例如：版本发布、截止日期…' : 'Milestone title…';
  String get countdownPinFooter => zh ? '优先展示到底部提醒栏' : 'Show in footer bar';
  String countdownDaysBadge(int days) {
    if (days == 0) return zh ? '就是今天' : 'Today';
    if (days > 0) return zh ? '还有 $days 天' : '${days}d left';
    return zh ? '已过 ${-days} 天' : '${-days}d ago';
  }

  String countdownFooterLine(CountdownItem item, DateTime now) {
    final d = item.daysFrom(now);
    if (d == 0) return zh ? '倒数日 · 今天是「${item.title}」' : 'Milestone · "${item.title}" is today';
    if (d > 0) return zh ? '倒数日 · 距「${item.title}」还有 $d 天' : 'Milestone · $d days until "${item.title}"';
    return zh ? '倒数日 ·「${item.title}」已过 ${-d} 天' : 'Milestone · "${item.title}" was ${-d}d ago';
  }

  String get automationTitle => zh ? '自动化联动' : 'Automation';
  String get automationAdd => zh ? '新建规则' : 'New rule';
  String get automationRuleNameHint => zh ? '规则名称…' : 'Rule name…';
  String get automationCustomMsgHint =>
      zh ? '提醒文案（可选）' : 'Reminder text (optional)';
  String get automationTriggerLabel => zh ? '当满足条件' : 'When';
  String get automationActionLabel => zh ? '自动执行动作' : 'Then';
  String get automationTestRun => zh ? '试运行' : 'Test';
  String get automationRecentLogs => zh ? '最近触发记录' : 'Recent runs';
  String automationEnabledBadge(int count) =>
      zh ? '$count 条启用' : '$count active';
  String triggerLabel(AutomationTrigger t) => switch (t) {
    AutomationTrigger.timerStarted => zh ? '定时器开始' : 'Timer starts',
    AutomationTrigger.timerCompleted => zh ? '定时器完成' : 'Timer finishes',
    AutomationTrigger.scheduleDue => zh ? '日程到点' : 'Schedule due',
    AutomationTrigger.countdownToday => zh ? '倒数日到达当天' : 'Countdown arrives',
  };
  String actionLabel(AutomationAction a) => switch (a) {
    AutomationAction.playMusic => zh ? '播放音乐' : 'Play music',
    AutomationAction.pauseMusic => zh ? '暂停音乐' : 'Pause music',
    AutomationAction.footerReminder => zh ? '底部横幅提醒' : 'Footer reminder',
    AutomationAction.enterFocusMode => zh ? '进入专注模式' : 'Enter focus mode',
    AutomationAction.exitFocusMode => zh ? '退出专注模式' : 'Exit focus mode',
    AutomationAction.startTimer => zh ? '启动定时器' : 'Start timer',
  };

  String get widgetFloatTooltip => zh ? '弹出可拖拽/缩放小窗' : 'Pop out floating widget';
  String get widgetDetachTooltip => zh ? '脱离主窗体置顶显示' : 'Detach native mini window';
  String get widgetDockTooltip => zh ? '收回主工作台' : 'Dock back to workbench';
  String get widgetDragOutHint => zh ? '拖出窗外可独立置顶' : 'Drag outside to detach';
  String get editorDetachTooltip => zh ? '脱离主窗体独立编辑' : 'Detach into standalone window';
  String get editorDragTooltip => zh ? '拖拽移动，拖出边缘可脱离主窗体' : 'Drag to move; drag outside to detach';
  String widgetDetachedBanner(String name) => zh
      ? '「$name」已脱离主窗体独立置顶运行'
      : '"$name" is running in a detached window';
  String footerTodayScheduleLine(String timeAndTitle, int count) => count > 1
      ? (zh ? '今日日程：$timeAndTitle（共 $count 项待办）' : 'Today: $timeAndTitle ($count pending)')
      : (zh ? '今日日程：$timeAndTitle' : 'Today: $timeAndTitle');
}
