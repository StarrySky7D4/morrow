// ignore: unused_import
import 'package:intl/intl.dart' as intl;
import 'app_localizations.dart';

// ignore_for_file: type=lint

/// The translations for Chinese (`zh`).
class AppLocalizationsZh extends AppLocalizations {
  AppLocalizationsZh([String locale = 'zh']) : super(locale);

  @override
  String get commonAppName => 'Morrow';

  @override
  String get commonCancel => '取消';

  @override
  String commonCount(int count) {
    return '$count 项';
  }

  @override
  String commonGreeting(String name) {
    return '你好，$name';
  }

  @override
  String get importsAttachmentLimit => '最多导入 20 个附件，其余内容请分次粘贴。';

  @override
  String get importsClipboardChanged => '读取期间剪贴板发生了变化，请重新粘贴。';

  @override
  String get importsEmbeddedImageUnreadable => '有一张内嵌图片无法读取。';

  @override
  String get importsEmbeddedImagesSeparate => '部分内嵌图片需要作为文件单独导入。';

  @override
  String get importsExcelValues => '表格显示值与公式已转为 Markdown，格式和合并信息保留在 XML 附件。';

  @override
  String get importsExcelXmlKept => 'Excel 原始表格已保留为 XML 附件。';

  @override
  String get importsFileTooLarge => '剪贴板文件超过 200 MB。';

  @override
  String get importsItemLimit => '本次只读取前 20 项，请分次粘贴更多内容。';

  @override
  String get importsItemUnreadable => '有一项剪贴板内容无法读取，其余可读内容已保留。';

  @override
  String get importsLocalImageNotRead => '本地链接图片没有自动读取，请粘贴图片或导入原文件。';

  @override
  String get importsMergedTable => '合并单元格已转为阅读表格；原始排版保留在 HTML 附件。';

  @override
  String get importsOfficeBusy => '剪贴板正被其他应用占用，Office 对象未读取。';

  @override
  String get importsOfficeEmbeddedKept =>
      'Office 嵌入对象已保留为原始附件；图表、公式和版式可用原软件继续编辑。';

  @override
  String get importsOfficeExportFailed => '有一个 Office 对象超过限制或无法导出，请在原软件保存后导入。';

  @override
  String get importsOfficeReadFailed => 'Office 内容读取失败，其他剪贴板内容仍可使用。';

  @override
  String get importsOfficeUnavailable => 'Office 剪贴板暂不可用。';

  @override
  String get importsOfficeUnreadable => 'Office 原始对象未能读取，已保留其他可用内容。';

  @override
  String get importsRichFallback => '富文本排版无法完整转换，已保留可读文字。';

  @override
  String get importsRichTooLarge => '富文本超过 2 MB，请将文档作为附件导入。';

  @override
  String get importsRtfTooLarge => 'RTF 过大，请导入原始文档。';

  @override
  String get importsSpreadsheetTooLarge => '表格过大，请导入 Excel 文件。';

  @override
  String get importsTableConverted => '表格已转换为 Markdown，完整数据保留在 TSV 附件中。';

  @override
  String get importsTextTooLarge => '文本超过 2 MB，请作为文件导入。';

  @override
  String get importsTotalTooLarge => '本次粘贴的文件总量超过 200 MB，请分次导入。';

  @override
  String get importsUnsupported => '此环境不支持读取剪贴板，请使用导入文件。';

  @override
  String get mainActiveProjects => '正在推进';

  @override
  String get mainAdjustCustomTone => '调整自定义色调';

  @override
  String get mainAmbientDetail => '流动的光晕，为灵感留一点色彩。';

  @override
  String get mainAppTitle => 'Morrow — 留一点空间给灵感';

  @override
  String get mainAppearance => '空间外观';

  @override
  String get mainArrangeIdeas => '排列想法';

  @override
  String mainAttachmentCount(int count) {
    return '附件 · $count';
  }

  @override
  String mainAttachmentHint(int count, String name) {
    return '$count 个附件 · $name';
  }

  @override
  String get mainAttachmentLimit => '每条记录最多保存 20 个附件。';

  @override
  String get mainAutosaveNotice => '外观与灵感自动保存在本机';

  @override
  String get mainAwaitDiscovery => '等待一次新的发现';

  @override
  String get mainBackToWorkbench => '返回工作台';

  @override
  String get mainBackgroundCanvas => '背景画布';

  @override
  String get mainBackgroundSound => '播放背景声音';

  @override
  String get mainBodyHint => '写下思路，或粘贴一段内容…\n\n支持 # 标题、列表、表格和代码块';

  @override
  String get mainBrightWhite => '亮白';

  @override
  String get mainBuiltinTexture => '使用内置纹理';

  @override
  String get mainCanvasCompass => '画布染色罗盘';

  @override
  String get mainCaptureIdea => '记录灵感';

  @override
  String get mainCaptureNow => '记录此刻的想法';

  @override
  String mainCardAttachments(int count, String name) {
    return '附件 $count · $name';
  }

  @override
  String get mainCategoryExperiment => '实验';

  @override
  String get mainCategoryIdea => '灵感';

  @override
  String get mainCategoryProject => '进行中';

  @override
  String get mainCategoryPrompt => '放在哪里';

  @override
  String get mainChangeFailed => '这次修改未能保存，草稿保留，可重试。';

  @override
  String get mainCheckAgain => '重新检查';

  @override
  String get mainClearSearch => '清空搜索';

  @override
  String get mainClipboardEmpty => '剪贴板中没有可读取的文本或文件。请从资源管理器复制文件，或使用导入文件。';

  @override
  String get mainClipboardReadFailed => '无法读取内容，请使用导入文件，或检查文件与剪贴板权限。';

  @override
  String get mainClipboardSupport =>
      '支持 Markdown、Office 富文本与表格、截图及文件。复杂对象保留原始附件；最多 20 个附件，单个不超过 200 MB。';

  @override
  String get mainCollapseSidebar => '收起侧边栏';

  @override
  String get mainCompletedProjects => '已经完成';

  @override
  String get mainComponentCompass => '组件染色罗盘';

  @override
  String get mainComponentEmpty => '空白提示';

  @override
  String get mainComponentFooter => '底部提示与歌词';

  @override
  String get mainComponentHero => '概览卡片';

  @override
  String get mainComponentNavigation => '侧边导航';

  @override
  String get mainComponentQuickCapture => '快速记录';

  @override
  String get mainComponentSearch => '搜索栏';

  @override
  String get mainComponentSettings => '组件与卡片 · 独立设置';

  @override
  String get mainContentProtection => '内容保护';

  @override
  String get mainContentRead => '已读取内容';

  @override
  String mainContentReadFiles(int count) {
    return '已读取内容，保留 $count 个附件';
  }

  @override
  String get mainCornerRadius => '圆角幅度';

  @override
  String get mainCredentialSettings => '凭据';

  @override
  String get mainCrystal => '超透';

  @override
  String get mainCrystalDetail => '通透轻盈，让光与色彩穿过界面。';

  @override
  String get mainCuriosity => '所有有趣的东西，\n都始于一点点好奇。';

  @override
  String get mainCustomCompass => '调色罗盘 · 自定义';

  @override
  String get mainCustomLightness => '自定义明暗';

  @override
  String get mainCustomTheme => '自定义';

  @override
  String get mainDaily => '此刻的小事';

  @override
  String get mainDailyExplore => '留十分钟，随便探索';

  @override
  String get mainDailyIdea => '把一个想法写下来';

  @override
  String get mainDailyWater => '给自己倒一杯水';

  @override
  String get mainDarkTheme => '深色';

  @override
  String get mainDeepBlack => '深黑';

  @override
  String get mainDefaultCanvas => '默认';

  @override
  String get mainDefaultGlobalColor => '默认主题色 · 全局控件';

  @override
  String get mainDelete => '删除';

  @override
  String mainDeleted(String title) {
    return '已删除「$title」';
  }

  @override
  String get mainDiagnosticDetails => '诊断详情';

  @override
  String get mainDone => '收好';

  @override
  String get mainEdit => '编辑';

  @override
  String get mainEditIdeaTitle => '让想法更清晰';

  @override
  String get mainEditorClosedUnknown => '编辑器已关闭，但保存状态暂时无法确认。请重新打开工作台检查，避免重复创建。';

  @override
  String get mainEditorSubtitle => '文字、表格、图片，先放在这里。让一个念头慢慢成形。';

  @override
  String get mainEditorUnavailable => '编辑器暂时无法打开，请检查内容服务后重试。';

  @override
  String get mainEndpointSettings => '出站端点';

  @override
  String get mainExpandSettings => '展开完整设置';

  @override
  String get mainExpandSidebar => '展开侧边栏';

  @override
  String get mainExtensionPlugins => '扩展插件';

  @override
  String get mainFavoriteAttachments => '收藏附件';

  @override
  String get mainFavoriteRecords => '收藏记录';

  @override
  String mainFavoriteTooltip(String title) {
    return '收藏 $title';
  }

  @override
  String get mainFavoritesIntro => '喜欢的文字、图像与文件，集中收在这里。';

  @override
  String mainFieldLimit(int limit) {
    return '此输入框最多 $limit 个字符，请缩短内容或将其作为文件导入。';
  }

  @override
  String get mainFilterAll => '全部';

  @override
  String get mainFilterAttachments => '含附件';

  @override
  String get mainFilterFavorites => '仅收藏';

  @override
  String get mainFilterFile => '文件';

  @override
  String get mainFilterImage => '图像';

  @override
  String get mainFilterMedia => '音视频';

  @override
  String get mainFilterPending => '有待办';

  @override
  String get mainFilterText => '文字';

  @override
  String get mainFollowTheme => '跟随主题';

  @override
  String get mainFrostDetail => '柔化背景，让思绪安静地浮现。';

  @override
  String get mainFrostEffect => '磨砂效果';

  @override
  String get mainFrostOpacity => '磨砂不透明度';

  @override
  String get mainFrostUnavailable => '当前系统无法启用桌面磨砂，染色和透明度仍可调整。';

  @override
  String get mainFrosted => '磨砂';

  @override
  String get mainGlassTexture => '玻璃质感';

  @override
  String mainGlobalColor(String color) {
    return '$color · 全局控件';
  }

  @override
  String get mainGreeting => '让想法，自由生长。';

  @override
  String get mainGreetingDetail => '收纳日常的零碎，也留住灵光一闪。';

  @override
  String get mainHeroBody => '一个念头、一件小事、一个「万一呢」。\n这里是它们开始的地方。';

  @override
  String get mainHeroCaption => '让可能性发生';

  @override
  String get mainHeroTitle => '还没成形，也没关系。';

  @override
  String get mainHideAppearance => '收起外观设置';

  @override
  String get mainHideCustomTone => '收起自定义色调';

  @override
  String get mainHidePreview => '收起预览';

  @override
  String get mainHttpSettings => 'HTTP 任务';

  @override
  String get mainHypothesis => '假设';

  @override
  String get mainHypothesisPrompt => '想验证的假设';

  @override
  String get mainHypothesisSection => '假设 / 想试什么';

  @override
  String get mainIdeaDetails => '留住细节，让下一步更清楚。';

  @override
  String get mainIdeaNameHint => '给它起个名字';

  @override
  String get mainIdeaNameRequired => '先写下你的想法吧';

  @override
  String get mainIdeaSaved => '灵感已收好。';

  @override
  String get mainImportFailed => '素材导入失败，请检查文件与可用存储空间。';

  @override
  String get mainImportFile => '导入文件';

  @override
  String get mainInboxIntro => '先接住，再整理。把值得继续的念头转成小项目。';

  @override
  String get mainIoNoDeclarations => '当前已安装的插件尚未声明文件或网络访问能力。';

  @override
  String get mainIoSettings => '网络与文件访问';

  @override
  String get mainIoSettingsGuide =>
      '在此管理插件的文件与网络权限。批准能力声明不代表授权访问所有文件或端点；具体操作以当前后端支持的能力为准。';

  @override
  String get mainIoSettingsSummary => '权限、凭据、端点与 API 服务';

  @override
  String get mainJustNow => '刚刚';

  @override
  String get mainLabIntro => '从一个假设开始，保留尝试、观察和意外发现。';

  @override
  String get mainLanguage => '界面语言';

  @override
  String get mainLanguageChinese => '简体中文';

  @override
  String get mainLanguageEnglish => 'English';

  @override
  String get mainLanguageSystem => '跟随系统';

  @override
  String get mainLavender => '淡紫';

  @override
  String get mainLightOpacity => '20% · 轻盈';

  @override
  String get mainLiquidAllCanvases => '独立于背景类型，四种画布均可开启';

  @override
  String get mainLiquidDetail => '流动的高光、柔和折射，让面板像一滴凝住的水。';

  @override
  String get mainLiquidEffect => '液体玻璃效果';

  @override
  String get mainLiquidGlass => '液体玻璃';

  @override
  String get mainLivePreview => '实时预览';

  @override
  String get mainLocalMedia => '本地素材';

  @override
  String get mainMakeYours => '打造你的风格';

  @override
  String get mainMarkOrganized => '标记已整理';

  @override
  String get mainMarkdownBody => '正文 · Markdown';

  @override
  String get mainMediaLimits => '图片 / GIF ≤ 25 MB，视频 ≤ 150 MB';

  @override
  String get mainMonochrome => '黑白灰';

  @override
  String mainMoreSteps(int count) {
    return '另有 $count 步，打开查看';
  }

  @override
  String mainMovedProject(String title) {
    return '「$title」已放入小项目';
  }

  @override
  String get mainMusic => '随身听';

  @override
  String get mainMySpace => '我的空间';

  @override
  String get mainNavigation => '导航';

  @override
  String get mainNewIdea => '新建灵感';

  @override
  String get mainNewIdeaTitle => '接住一个新想法';

  @override
  String get mainNoHypothesis => '还没有写下假设';

  @override
  String get mainNoMatches => '这里还没有匹配的想法';

  @override
  String get mainNoResultYet => '结果还未发生，过程也值得记录。';

  @override
  String get mainNotNow => '再想想';

  @override
  String get mainObservationSection => '观察 / 留下发现';

  @override
  String get mainObservations => '观察与结论';

  @override
  String get mainObservationsPrompt => '观察、过程与结论';

  @override
  String get mainOneHourAgo => '1 小时前';

  @override
  String get mainOnlineMedia => '网络采集';

  @override
  String get mainOpaqueFallback => '保留通透面板，以当前主题作为底色。';

  @override
  String get mainOpenNextStep => '打开项目，编辑下一步';

  @override
  String get mainOrganizedCount => '已经整理';

  @override
  String get mainOriginalColors => '保留原色';

  @override
  String get mainPageFavorites => '已收藏';

  @override
  String get mainPageInbox => '灵感收件箱';

  @override
  String get mainPageLaboratory => '实验室';

  @override
  String get mainPageOverview => '概览';

  @override
  String get mainPageProjects => '小项目';

  @override
  String mainPageSummary(String page) {
    return '$page · 概览';
  }

  @override
  String get mainPasteChanged => '粘贴期间输入发生变化，请重新打开编辑器。';

  @override
  String get mainPasteContent => '粘贴内容';

  @override
  String get mainPause => '暂停';

  @override
  String get mainPersonalWorkspace => '个人工作台';

  @override
  String get mainPlay => '播放';

  @override
  String get mainPluginSettings => '插件与服务';

  @override
  String get mainPluginSettingsSummary => '内置工具、扩展插件、网络与文件访问、内容保护';

  @override
  String get mainPreviewEmpty => '预览会显示在这里';

  @override
  String mainProgress(int done, int total) {
    return '小小的进展 · $done/$total';
  }

  @override
  String get mainProjectIntro => '用清单推动进展。每一个完成的小步，都在靠近结果。';

  @override
  String get mainQueryAgain => '重新筛选';

  @override
  String get mainQueryCapacity => '查询历史容量已满';

  @override
  String get mainQueryCapacityDetail => '已有内容已保留。此版本尚不支持清理查询历史。';

  @override
  String get mainQueryLoading => '正在筛选…';

  @override
  String get mainQueryRetry => '重试筛选';

  @override
  String get mainQueryTerminated => '此次筛选已终止';

  @override
  String get mainQueryUnknown => '尚未确认筛选结果';

  @override
  String get mainQuickHint => '脑海中闪过了什么？';

  @override
  String get mainReadOnlySettings => '内容库当前只读，请检查工作台插件以恢复编辑。';

  @override
  String get mainRecentThoughts => '最近的念头';

  @override
  String get mainRecordedCount => '已有记录';

  @override
  String get mainRestoreDefault => '恢复默认';

  @override
  String get mainRetry => '重试';

  @override
  String get mainRetrySave => '重试保存';

  @override
  String get mainSage => '鼠尾草';

  @override
  String get mainSampleBody0 => '把突然冒出的念头放在这里。\n不急着完成，先让它发生。';

  @override
  String get mainSampleBody1 => '用小小的网页，收藏喜欢的文字、\n音乐和生活里的细枝末节。';

  @override
  String get mainSampleBody2 => '试试生成艺术，让代码长出\n意料之外的形状。';

  @override
  String get mainSampleBody3 => '一个低调常驻的伙伴，帮我记住\n那些容易忘记的小事。';

  @override
  String get mainSampleTitle0 => '给灵感一个容器';

  @override
  String get mainSampleTitle1 => '一个安静的数字花园';

  @override
  String get mainSampleTitle2 => '周末，做点无用的东西';

  @override
  String get mainSampleTitle3 => '我的桌面小助手';

  @override
  String get mainSampleTodo0 => '整理第一批收藏';

  @override
  String get mainSampleTodo1 => '设计花园入口';

  @override
  String get mainSampleTodo2 => '种下一条新想法';

  @override
  String get mainSampleTodo3 => '画一个小小的原型';

  @override
  String get mainSampleTodo4 => '定义提醒交互';

  @override
  String get mainSaveConnectionUnknown => '连接中断，保存结果尚未确认。请重新打开内容库核对后再重试。';

  @override
  String get mainSaveFailed => '保存失败，改动仍在当前会话中。';

  @override
  String get mainSaveIdea => '保存灵感';

  @override
  String get mainSaveNotSubmitted => '尚未提交。草稿与附件已保留，可修改后再次保存。';

  @override
  String get mainSaveReadOnly => '改动尚未保存。请在“插件与服务”中启用工作台插件，然后重试。';

  @override
  String get mainSaveUnknown => '这次保存尚未确认。草稿与附件已保留，请重试同一次提交；关闭后会重新读取工作台确认。';

  @override
  String get mainSaving => '正在保存…';

  @override
  String get mainSearchHint => '搜索你的奇思妙想…';

  @override
  String get mainServiceRunSettings => '服务运行';

  @override
  String get mainServiceSettings => 'API 服务';

  @override
  String get mainSettings => '设置';

  @override
  String get mainShowAppearance => '显示外观设置';

  @override
  String get mainSidebarMotto => '杂事有序，奇想自由。';

  @override
  String get mainSlowProgress => '慢一点，也是在向前。';

  @override
  String get mainSolidCanvas => '纯色';

  @override
  String get mainSolidDetail => '一张安静的纯色画布。';

  @override
  String get mainSolidOpacity => '100% · 纯粹';

  @override
  String get mainSortFavorites => '收藏优先';

  @override
  String get mainSortRecent => '最近添加';

  @override
  String get mainSortTitle => '标题排序';

  @override
  String get mainSquareCorners => '拖至 0 即为方角';

  @override
  String get mainStageActive => '推进中';

  @override
  String get mainStageCompleted => '已完成';

  @override
  String get mainStageOrganized => '已整理';

  @override
  String get mainStagePlanned => '计划中';

  @override
  String get mainStageRecorded => '已记录';

  @override
  String mainStageTooltip(String title) {
    return '修改状态 $title';
  }

  @override
  String get mainStageUnsorted => '待整理';

  @override
  String get mainStageUnverified => '待验证';

  @override
  String get mainStageVerifying => '验证中';

  @override
  String get mainStayCurious => '保持好奇，做你自己。';

  @override
  String mainSteps(int done, int total) {
    return '$done/$total 步';
  }

  @override
  String get mainStorageUnavailable => '本地存储暂不可用，当前改动仅保留在本次会话。';

  @override
  String get mainStorageUnreadable => '存储内容无法读取，原数据仍保留。当前会话不会覆盖它。';

  @override
  String get mainTenMinutesAgo => '10 分钟前';

  @override
  String get mainTextureCanvas => '纹理';

  @override
  String get mainTextureDetail => '细密的纸感网点，让空间多一点触感。';

  @override
  String get mainThemeCompass => '主题色彩色罗盘';

  @override
  String get mainThemeGrayscale => '主题灰度';

  @override
  String get mainThemeTone => '主题色调';

  @override
  String get mainThreeHoursAgo => '3 小时前';

  @override
  String get mainTintOpacity => '染色不透明度';

  @override
  String get mainToProject => '转为项目';

  @override
  String get mainTodosPrompt => '下一小步（每行一项，可选）';

  @override
  String get mainTransparencyUnavailable => '系统透明效果未能启用，可切换到默认背景继续使用。';

  @override
  String get mainTransparentCanvas => '透明';

  @override
  String get mainTransparentDetail => '透出窗口背后的空间；网页透出宿主背景。';

  @override
  String get mainUndo => '撤销';

  @override
  String mainUnfavoriteTooltip(String title) {
    return '取消收藏 $title';
  }

  @override
  String get mainUnsortedCount => '等待整理';

  @override
  String get mainUnverifiedCount => '等待验证';

  @override
  String get mainView => '查看';

  @override
  String get mainViewAll => '查看全部';

  @override
  String get mainWarmSand => '暖沙';

  @override
  String get mainWhiteTheme => '白色';

  @override
  String get mainWindowRadius => '窗体圆角';

  @override
  String get mainWindowRadiusDetail => '独立调整窗口边框，最大化时自动展平';

  @override
  String get mainWindowsFrostOnly => '桌面磨砂仅在 Windows 版可用';

  @override
  String get mainWorkbench => '工作台';

  @override
  String get mainWorkbenchPlugin => '工作台插件';

  @override
  String get mainWriteHypothesis => '打开记录，写下这次想验证的问题。';

  @override
  String get mainYesterday => '昨天';

  @override
  String get pluginsApprovalUnknown => '启用或权限变更未能确认';

  @override
  String get pluginsApproveEnable => '批准并启用';

  @override
  String get pluginsApproveWorkbench => '允许读取与编辑，并启用';

  @override
  String get pluginsAttachment => '读取附件';

  @override
  String get pluginsBackingUp => '正在备份…';

  @override
  String get pluginsBackupLibrary => '备份内容库';

  @override
  String get pluginsBackupLibraryType => '内容库备份';

  @override
  String get pluginsBackupProtection => '备份保护文件';

  @override
  String get pluginsBackupUnknown => '备份结果尚未确认，请保留可能生成的文件并检查保存位置。';

  @override
  String pluginsBinaryPreview(String hex) {
    return '二进制内容：$hex';
  }

  @override
  String get pluginsBuiltin => '内置工作台';

  @override
  String pluginsBuiltinCount(int count) {
    return '$count 个字符 · 仅本次使用，不保存为卡片';
  }

  @override
  String get pluginsBuiltinEmpty => '输入后查看大写转换';

  @override
  String get pluginsBuiltinHeading => '文字小工具';

  @override
  String get pluginsBuiltinInput => '输入文字';

  @override
  String get pluginsCancel => '取消';

  @override
  String get pluginsChoosePackage => '选择插件文件';

  @override
  String get pluginsChooseSmallFile => '选择小文件';

  @override
  String get pluginsCloseTextTool => '收起文字工具';

  @override
  String get pluginsCloseUnknown => '插件界面关闭未能确认';

  @override
  String get pluginsCloseView => '关闭界面';

  @override
  String get pluginsConnectionLost => '连接已中断，请重新打开此插件界面。';

  @override
  String get pluginsContentPermissions => '内容权限';

  @override
  String get pluginsCreate => '新建内容';

  @override
  String get pluginsCredentialCancel => '关闭表单';

  @override
  String get pluginsCredentialCreateTitle => '新建凭据';

  @override
  String pluginsCredentialDays(int days) {
    return '$days 天';
  }

  @override
  String get pluginsCredentialDetails =>
      '安全保存已批准 API 连接使用的凭据。保存凭据不会批准服务器或启用插件，已保存的秘密内容无法查看。';

  @override
  String get pluginsCredentialDisable => '停用';

  @override
  String get pluginsCredentialDisabled => '已停用';

  @override
  String get pluginsCredentialDisabledDone => '凭据已停用。';

  @override
  String get pluginsCredentialEmpty => '尚未保存凭据';

  @override
  String get pluginsCredentialExpired => '已过期';

  @override
  String pluginsCredentialExpires(String date) {
    return '到期时间：$date';
  }

  @override
  String get pluginsCredentialHeader => '请求头名称';

  @override
  String get pluginsCredentialInvalid => '请检查请求头名称并输入新的秘密内容。秘密输入框已清空。';

  @override
  String get pluginsCredentialLifetime => '有效期';

  @override
  String get pluginsCredentialLoadFailed => '无法完整读取一致的凭据状态，请刷新后重试。';

  @override
  String get pluginsCredentialNew => '添加凭据';

  @override
  String get pluginsCredentialReading => '正在读取凭据…';

  @override
  String pluginsCredentialReference(String reference) {
    return '凭据 $reference';
  }

  @override
  String get pluginsCredentialRefresh => '刷新状态';

  @override
  String get pluginsCredentialReplace => '替换秘密内容';

  @override
  String pluginsCredentialReplaceTitle(String reference) {
    return '替换凭据 $reference';
  }

  @override
  String get pluginsCredentialSave => '保存凭据';

  @override
  String get pluginsCredentialSaved => '凭据已保存，API 连接仍需单独批准。';

  @override
  String get pluginsCredentialSecret => '新的秘密内容';

  @override
  String get pluginsCredentialStored => '已保存';

  @override
  String get pluginsCredentialTitle => 'API 凭据';

  @override
  String get pluginsCredentialUnknown => '暂时无法确认结果，秘密输入框已清空。请先刷新状态，再进行修改。';

  @override
  String pluginsDeclared(String permissions) {
    return '声明权限：$permissions';
  }

  @override
  String get pluginsDependenciesNotice => '声明了依赖，需要在宿主配置。本页不会批准依赖。';

  @override
  String get pluginsDisable => '停用';

  @override
  String get pluginsDisableWorkbench => '停用工作台插件';

  @override
  String get pluginsDisabled => '未启用';

  @override
  String get pluginsDisabledDetails => '尚未启用。允许读取和编辑工作台内容后，可继续使用编辑与工具功能。';

  @override
  String get pluginsEdit => '编辑内容';

  @override
  String get pluginsEmptyLibrary => '尚未导入第三方插件。';

  @override
  String get pluginsEmptyResult => '（空结果）';

  @override
  String get pluginsEnabled => '已启用';

  @override
  String get pluginsEnabledDetails => '已启用。插件可读取和编辑工作台内容，停用后保留已有资料。';

  @override
  String get pluginsEndpointAdvanced => '政策额度（未注明时以字节计）';

  @override
  String get pluginsEndpointCertificate => '选择 DER 信任根';

  @override
  String get pluginsEndpointCertificateDetails =>
      'HTTPS 可选信任根：单个二进制 DER 证书（.der 或 .cer），最大 32 KiB。不接受 PEM 或证书包。切换到 HTTP 前请移除信任根。';

  @override
  String get pluginsEndpointCertificateInvalid =>
      '请选择不超过 32 KiB 的单个有效二进制 DER 证书（.der 或 .cer）。';

  @override
  String pluginsEndpointCertificateSelected(int bytes) {
    return '已选择 DER 信任根（$bytes 字节）';
  }

  @override
  String get pluginsEndpointConcurrency => '并发请求（1–128）';

  @override
  String get pluginsEndpointCreateTitle => '新建端点批准';

  @override
  String get pluginsEndpointCredential => '凭据引用';

  @override
  String get pluginsEndpointCredentialLifetime =>
      '所选凭据须在端点的整个有效期内有效，系统不会延长凭据期限。';

  @override
  String get pluginsEndpointCredentialUnavailable =>
      '使用凭据需要插件包声明并获批 credential-use 权限，且已保存的引用仍有效。';

  @override
  String get pluginsEndpointCredentialsFailed => '无法读取凭据引用，请刷新状态后再选择凭据。';

  @override
  String get pluginsEndpointDetails =>
      '为指定插件包及摘要保存服务器政策。保存不会连接网络、启用插件，也不代表网络任务立即可用。';

  @override
  String get pluginsEndpointDigest => '插件包摘要';

  @override
  String get pluginsEndpointDisabledDone => '端点批准已停用。';

  @override
  String get pluginsEndpointEmpty => '尚无已保存的端点批准';

  @override
  String get pluginsEndpointFrameBytes => '帧预算（1–131072 字节）';

  @override
  String get pluginsEndpointHeaderBytes => '请求头上限（1–16384 字节）';

  @override
  String get pluginsEndpointInvalid => '请检查插件包、源地址、方法、1–30 天期限、凭据权限、证书及政策额度。';

  @override
  String get pluginsEndpointLifetime => '有效期（1–30 天）';

  @override
  String get pluginsEndpointLoadFailed => '无法一致地读取端点批准，请刷新状态后重试。';

  @override
  String get pluginsEndpointLocalHttp => '本机 HTTP';

  @override
  String get pluginsEndpointLocalHttps => '本机 HTTPS';

  @override
  String get pluginsEndpointMethods => '允许的请求方法';

  @override
  String get pluginsEndpointNew => '添加端点';

  @override
  String get pluginsEndpointNoCredential => '不使用凭据';

  @override
  String get pluginsEndpointOrigin => '仅源地址，例如 https://api.example.com';

  @override
  String get pluginsEndpointPackage => '插件包';

  @override
  String get pluginsEndpointPackageUnavailable =>
      '该插件包不可用或尚未批准 HTTP 权限，已有批准仍可停用。';

  @override
  String get pluginsEndpointProfile => '连接类型';

  @override
  String get pluginsEndpointPublicHttps => '公网 HTTPS';

  @override
  String get pluginsEndpointRemoveCertificate => '移除信任根';

  @override
  String get pluginsEndpointReplace => '替换批准';

  @override
  String get pluginsEndpointReplaceTitle => '使用当前插件包摘要替换端点批准';

  @override
  String get pluginsEndpointRequestBytes => '请求上限（1–65536 字节）';

  @override
  String get pluginsEndpointResponseBytes => '响应上限（1–65536 字节）';

  @override
  String get pluginsEndpointSave => '保存端点批准';

  @override
  String get pluginsEndpointSaved => '端点批准已保存，未建立网络连接。';

  @override
  String get pluginsEndpointTimeout => '超时（1–30000 毫秒）';

  @override
  String get pluginsEndpointTitle => 'API 端点批准';

  @override
  String get pluginsEndpointUnknown => '无法确认操作结果。请刷新状态后再修改，请求不会自动重发。';

  @override
  String get pluginsEndpointWorking => '正在更新端点状态…';

  @override
  String get pluginsExistingVersion => '此版本已在插件列表中，现有启用状态保持不变。';

  @override
  String pluginsFileLimit(int limit) {
    return '文件太大，请选择不超过 $limit 字节的文件。';
  }

  @override
  String get pluginsHttpTaskAbandon => '结束本次尝试的观察';

  @override
  String get pluginsHttpTaskAbandonDetails =>
      '仅在刷新确认无活动任务且原内容库可用后，才能结束本次观察。这不证明远端未发生效果。身份与未知状态仍保留在历史中，新请求需要另一次明确提交。';

  @override
  String get pluginsHttpTaskAbsent => '暂无结果交付';

  @override
  String get pluginsHttpTaskAccepted => '已接受';

  @override
  String get pluginsHttpTaskAcknowledge => '确认任务结束';

  @override
  String get pluginsHttpTaskArchivedUnknown => '用户已结束观察。此前远端效果仍未确认，本次尝试未被重发。';

  @override
  String get pluginsHttpTaskBase64 => 'Base64';

  @override
  String get pluginsHttpTaskBody => '请求正文';

  @override
  String get pluginsHttpTaskBodyFormat => '请求正文编码';

  @override
  String get pluginsHttpTaskBusy => '忙碌';

  @override
  String get pluginsHttpTaskCancel => '请求取消';

  @override
  String get pluginsHttpTaskCancelled => '已观察到取消，远端仍可能已经执行';

  @override
  String get pluginsHttpTaskCatalogUnavailable =>
      '插件目录状态不可用。新提交前请刷新插件库，已有任务控制仍可使用。';

  @override
  String get pluginsHttpTaskClosed => '已关闭';

  @override
  String get pluginsHttpTaskCompleted => '已完成';

  @override
  String get pluginsHttpTaskConflict => '冲突';

  @override
  String get pluginsHttpTaskConsumed => '结果已消费';

  @override
  String get pluginsHttpTaskControlUnknown => '无法确认控制操作结果，请刷新任务状态后再决定下一步。';

  @override
  String pluginsHttpTaskCounters(String bytes, String calls) {
    return 'IO 调用次数：$calls；计费字节：$bytes';
  }

  @override
  String get pluginsHttpTaskDeadline => '超过期限';

  @override
  String get pluginsHttpTaskDenied => '已拒绝';

  @override
  String get pluginsHttpTaskDetails =>
      '通过已批准端点和已启用、具有实验性 HTTP 转发处理器的插件执行一次明确请求。内容库忙碌时仍可查看及控制任务。';

  @override
  String get pluginsHttpTaskDisconnect => '连接清理失败';

  @override
  String get pluginsHttpTaskEndpoint => '已批准端点';

  @override
  String get pluginsHttpTaskEndpointsFailed =>
      '无法一致读取端点，或内容库正在忙碌。任务控制仍可使用，内容库归还后请刷新端点。';

  @override
  String get pluginsHttpTaskEvidenceUnavailable => '结果证据不可用';

  @override
  String pluginsHttpTaskExecution(int code, String fault) {
    return '插件执行：$fault；退出码：$code';
  }

  @override
  String pluginsHttpTaskExit(
    String disconnect,
    String execution,
    String maintenance,
  ) {
    return '后台线程退出记录：执行 $execution；断连 $disconnect；维护 $maintenance';
  }

  @override
  String get pluginsHttpTaskExplicit =>
      '提交会发送一次真实请求，每次点击生成新身份。未知提交及结果读取不会自动重放，取消不代表远端操作已撤销。';

  @override
  String get pluginsHttpTaskFailed => '失败';

  @override
  String get pluginsHttpTaskHeaders => '普通请求头，每行 Name: value';

  @override
  String get pluginsHttpTaskHeadersHint => '重复请求头保持独立。凭据与连接请求头仅由运行时提供。';

  @override
  String get pluginsHttpTaskHistory => '此前任务记录（最多 5 项）';

  @override
  String pluginsHttpTaskHttpResult(int code, String status) {
    return 'HTTP 结果：$status；远端状态码：$code';
  }

  @override
  String get pluginsHttpTaskInactive => '连接未激活';

  @override
  String get pluginsHttpTaskInvalid => '请根据批准额度检查所选端点、方法、相对目标、普通请求头、正文编码与超时。';

  @override
  String get pluginsHttpTaskInvalidOptions => '参数无效';

  @override
  String pluginsHttpTaskKey(String identity) {
    return '任务身份：$identity';
  }

  @override
  String get pluginsHttpTaskLimit => '达到额度或限制';

  @override
  String get pluginsHttpTaskLoadingEndpoints => '正在读取批准端点…';

  @override
  String get pluginsHttpTaskLocal => '内容库可用，当前无任务';

  @override
  String get pluginsHttpTaskModule => '插件模块无效';

  @override
  String get pluginsHttpTaskNew => '准备新请求';

  @override
  String get pluginsHttpTaskNoEndpoints => '没有端点匹配当前已启用、已批准的 HTTP 转发插件。';

  @override
  String get pluginsHttpTaskNotFound => '未找到';

  @override
  String get pluginsHttpTaskOk => '正常';

  @override
  String get pluginsHttpTaskOutcomeUnknown => '远端结果未知，不应假定已回滚或再次发送';

  @override
  String get pluginsHttpTaskPackageChanged => '插件包绑定已变化';

  @override
  String get pluginsHttpTaskPending => '结果待定';

  @override
  String get pluginsHttpTaskPoll => '核对任务';

  @override
  String get pluginsHttpTaskProtocol => '任务协议错误';

  @override
  String get pluginsHttpTaskRead => '读取一次结果';

  @override
  String get pluginsHttpTaskReadBound => '超过读取额度';

  @override
  String get pluginsHttpTaskReadPending => '尚未返回结果，请核对状态后再明确读取。';

  @override
  String get pluginsHttpTaskReadUnknown =>
      '无法确认读取结果，结果可能已被消费。不会再次读取，仍可核对状态及退出情况。';

  @override
  String get pluginsHttpTaskReady => '结果就绪，请明确读取。就绪不代表后台线程已退出。';

  @override
  String get pluginsHttpTaskReclaimed => '后台线程已退出，原内容库已归还';

  @override
  String get pluginsHttpTaskRecoveryRequired => '后台线程已退出，清理或维护需要修复';

  @override
  String get pluginsHttpTaskRefresh => '刷新任务状态';

  @override
  String get pluginsHttpTaskRefreshEndpoints => '刷新批准端点';

  @override
  String get pluginsHttpTaskRemoteError =>
      '远端返回了 4xx/5xx 响应。这是已完成的 HTTP 交互，与插件执行错误分别记录。';

  @override
  String get pluginsHttpTaskRepair => '修复清理';

  @override
  String get pluginsHttpTaskResponseBase64 => '响应正文：完整 Base64';

  @override
  String get pluginsHttpTaskResponseHeaders => '响应头（保留重复项，二进制值使用 Base64）';

  @override
  String get pluginsHttpTaskResponseText => '响应正文：纯文本预览';

  @override
  String get pluginsHttpTaskResultUnavailable => '结果交付不可用';

  @override
  String get pluginsHttpTaskRevoked => '批准已撤销';

  @override
  String get pluginsHttpTaskRunning => '运行中，内容库由后台线程持有';

  @override
  String get pluginsHttpTaskSpawn => '后台线程未能启动';

  @override
  String get pluginsHttpTaskStart => '提交新请求';

  @override
  String get pluginsHttpTaskStartUnknown =>
      '提交结果未知，已保留本次身份。请查询状态核对同一任务，请求不会再次发送。';

  @override
  String get pluginsHttpTaskStatusFailed => '无法确认任务状态。请刷新状态，请求未被重发。';

  @override
  String get pluginsHttpTaskStopping => '正在停止，等待后台线程实际退出';

  @override
  String pluginsHttpTaskSubmission(String identity) {
    return '提交身份：$identity';
  }

  @override
  String get pluginsHttpTaskTarget => '相对目标，例如 /v1/items?limit=10';

  @override
  String get pluginsHttpTaskText => 'UTF-8 文本';

  @override
  String get pluginsHttpTaskTimeout => '超时毫秒数（1–30000，且不超过端点批准）';

  @override
  String get pluginsHttpTaskTitle => 'HTTP 任务';

  @override
  String get pluginsHttpTaskTrap => '插件执行异常终止';

  @override
  String get pluginsHttpTaskUnavailable => '原内容库不可用，需要处理恢复问题';

  @override
  String get pluginsHttpTaskUnsupported => '不支持的操作';

  @override
  String get pluginsHttpTaskWorking => '正在等待任务控制回执…';

  @override
  String get pluginsImport => '导入';

  @override
  String get pluginsImportDetails => '导入后由你选择是否启用。停用或卸载不会删除已有内容。';

  @override
  String pluginsImportPreview(String name) {
    return '导入预览：$name';
  }

  @override
  String get pluginsImportUnknown => '导入结果未能确认';

  @override
  String get pluginsImportedDisabled => '已导入，尚未启用。请选择需要允许的权限。';

  @override
  String get pluginsInputFailed => '输入文件未能读取';

  @override
  String get pluginsInputTooLong => '输入内容已超过此界面的容量，请缩短后重试。';

  @override
  String get pluginsInspectFailed => '插件预览未能读取';

  @override
  String get pluginsInspectedOnly => '目前仅检查了文件。导入后仍需单独启用。';

  @override
  String get pluginsInsufficientApproval => '插件已启用，但内容权限不足；工作台保持只读。可停用后重新确认权限。';

  @override
  String pluginsIoApproved(String permissions) {
    return '已批准：$permissions';
  }

  @override
  String get pluginsIoCredentialUse => '使用已批准的凭据';

  @override
  String pluginsIoDeclared(String permissions) {
    return '请求的网络与文件权限：$permissions';
  }

  @override
  String get pluginsIoFileCreate => '创建文件';

  @override
  String get pluginsIoFileDelete => '删除文件';

  @override
  String get pluginsIoFileList => '浏览已批准的文件夹';

  @override
  String get pluginsIoFileRead => '读取已批准的文件';

  @override
  String get pluginsIoFileReplace => '替换文件';

  @override
  String get pluginsIoHttpListen => '监听网络连接';

  @override
  String get pluginsIoHttpPublish => '对外提供 API 服务';

  @override
  String get pluginsIoHttpRequest => '调用网络 API';

  @override
  String get pluginsIoNoneApproved => '尚未批准网络与文件权限';

  @override
  String get pluginsIoRevoke => '撤销全部网络与文件权限';

  @override
  String get pluginsIoSave => '保存网络与文件权限';

  @override
  String get pluginsIoScopeNotice =>
      '这些选择仅保存权限类别。连接地址、文件范围和凭据仍需另行批准；尚未提供的能力不会因此启用。修改后需重新打开插件表单。';

  @override
  String get pluginsIoTitle => '网络与文件权限';

  @override
  String get pluginsIoWebSocketConnect => '连接 WebSocket 服务';

  @override
  String get pluginsListUnknown => '插件列表未能确认';

  @override
  String get pluginsManageAbove => '请使用上方工作台插件按钮管理此插件。';

  @override
  String get pluginsManagementUnavailable => '插件管理暂不可用，已有内容仍可读取。';

  @override
  String get pluginsNoPermissions => '未声明内容权限。';

  @override
  String get pluginsOpenTextTool => '打开文字工具';

  @override
  String get pluginsOpenView => '打开界面';

  @override
  String get pluginsOpeningView => '正在打开插件界面…';

  @override
  String get pluginsOperation => '查询操作结果';

  @override
  String pluginsOtherCapability(String name) {
    return '其他声明权限：$name';
  }

  @override
  String get pluginsPackageFile => 'Morrow 插件';

  @override
  String get pluginsPreviewOnly => '结果仅供预览，不会自动写入已有内容。';

  @override
  String get pluginsPreviewTruncated => '…仅显示前 4096 个字符';

  @override
  String get pluginsProtection => '内容保护';

  @override
  String get pluginsProtectionDetails => '保存原保护文件的备份，供当前系统账户恢复使用。此文件不包含卡片和附件。';

  @override
  String get pluginsProtectionFileType => '内容库保护文件';

  @override
  String get pluginsProtectionSaved => '保护文件已备份，可在启动失败时选择此文件恢复。';

  @override
  String get pluginsRead => '读取内容';

  @override
  String get pluginsReadingState => '正在读取插件状态…';

  @override
  String pluginsRefreshFailed(String reason) {
    return '$reason。列表暂未刷新，请点“刷新列表”重试读取。';
  }

  @override
  String get pluginsRefreshList => '刷新列表';

  @override
  String get pluginsRefreshState => '刷新状态';

  @override
  String get pluginsRename => '重命名';

  @override
  String pluginsResultBytes(int count, String preview) {
    return '$count 字节\n$preview';
  }

  @override
  String get pluginsSavePermissions => '保存权限';

  @override
  String pluginsSelectedFile(String name) {
    return '已选文件：$name';
  }

  @override
  String get pluginsServiceAcknowledgeUncertain => '我已核对刷新后的记录';

  @override
  String get pluginsServiceAddScope => '添加内容范围';

  @override
  String get pluginsServiceAttachmentId => '精确附件标识';

  @override
  String get pluginsServiceAuthenticationUnavailable =>
      '所选认证已缺失、停用、过期或属于其他主体。范围草稿已保留；请选择有效替代项，或明确移除。';

  @override
  String get pluginsServiceAuthorities => '认证与发布记录';

  @override
  String get pluginsServiceCardId => '精确卡片标识';

  @override
  String get pluginsServiceCatalogChanged => '包目录已变化或不可用。草稿已保留；保存前请明确刷新选择。';

  @override
  String get pluginsServiceClearToken => '清除令牌';

  @override
  String get pluginsServiceCloseEditor => '关闭编辑器';

  @override
  String get pluginsServiceConfigDigest => '配置摘要';

  @override
  String get pluginsServiceConfiguration => '已保存的配置';

  @override
  String get pluginsServiceConfigurations => '已保存的配置';

  @override
  String get pluginsServiceCopyClear => '复制并清除令牌';

  @override
  String get pluginsServiceCreated => '创建时间（UTC）';

  @override
  String get pluginsServiceDays => '请求有效期（1–30 天）';

  @override
  String get pluginsServiceDigestFixed => '编辑绑定原包摘要，必须选择匹配的包；此操作不会启用包。';

  @override
  String get pluginsServiceDisable => '停用';

  @override
  String get pluginsServiceDisabled => '已停用';

  @override
  String get pluginsServiceEditConfig => '编辑配置';

  @override
  String get pluginsServiceEditPublication => '编辑发布批准';

  @override
  String get pluginsServiceExpired => '已过期或尚未生效';

  @override
  String get pluginsServiceExpires => '实际到期时间（UTC）';

  @override
  String get pluginsServiceHandler => '已声明的服务处理器';

  @override
  String get pluginsServiceIdentity => '服务标识';

  @override
  String get pluginsServiceInvalid => '请检查各字段、所选批准及当前包后再保存。';

  @override
  String get pluginsServiceIssue => '签发令牌';

  @override
  String get pluginsServiceIssuedToken => '一次性访问令牌';

  @override
  String get pluginsServiceListenAddress => '数字监听地址与端口';

  @override
  String get pluginsServiceLoadFailed => '未能刷新记录。请再次刷新后再修改。';

  @override
  String get pluginsServiceManagementOnly =>
      '这里仅管理已保存的配置和批准。保存不会启动监听、运行包或启用服务。';

  @override
  String get pluginsServiceMethod => 'HTTP 方法';

  @override
  String get pluginsServiceNewAuthentication => '新建认证';

  @override
  String get pluginsServiceNewConfig => '新建配置';

  @override
  String get pluginsServiceNo => '否';

  @override
  String get pluginsServiceNoAuthentication => '请先创建当前有效的认证记录。';

  @override
  String get pluginsServiceNoAuthorities => '暂无认证或发布记录。';

  @override
  String get pluginsServiceNoConfigurations => '暂无服务配置。';

  @override
  String get pluginsServicePackage => '已声明并批准的包';

  @override
  String get pluginsServicePackageDigest => '包摘要';

  @override
  String get pluginsServicePackageUnavailable =>
      '匹配的包或其监听／发布批准不可用。历史记录仍可查看和停用。';

  @override
  String get pluginsServicePath => '精确请求路径';

  @override
  String get pluginsServicePolicyChanged =>
      '所选原记录已变化或不可用。请刷新选择，或从当前记录重新打开编辑器；草稿仍保留在这里。';

  @override
  String get pluginsServicePrincipalId => '主体标识';

  @override
  String get pluginsServicePrincipals => '授权主体与内容范围';

  @override
  String get pluginsServicePublicationEditor => '发布批准';

  @override
  String get pluginsServicePublicationHelp =>
      '批准绑定此精确配置、修订和引用。实际到期时间受每个所选认证限制，可能短于请求时长。保存不会启动监听。';

  @override
  String get pluginsServicePublicationMismatch => '此发布批准不再匹配当前配置。请核对后明确保存替代批准。';

  @override
  String get pluginsServiceQueryPath => '独立结果查询路径（可选）';

  @override
  String get pluginsServiceReference => '批准引用';

  @override
  String get pluginsServiceRefresh => '刷新记录';

  @override
  String get pluginsServiceRefreshSelection => '刷新此选择';

  @override
  String get pluginsServiceRemovePrincipal => '移除主体';

  @override
  String get pluginsServiceRemoveScope => '移除范围';

  @override
  String get pluginsServiceRetention => '请求历史保留时长（毫秒，最多 30 天）';

  @override
  String get pluginsServiceRevision => '修订';

  @override
  String get pluginsServiceRotate => '轮换令牌';

  @override
  String get pluginsServiceRotateAuthentication => '轮换认证';

  @override
  String get pluginsServiceRunAbandon => '保留记录并结束本次尝试';

  @override
  String get pluginsServiceRunAdvanced => '请求与工作线程限制';

  @override
  String get pluginsServiceRunAttempt => '结果未明的启动尝试';

  @override
  String get pluginsServiceRunBoundsHint =>
      '这些限制还必须符合插件声明与已保存的授权。任务预留后即计入累计预算，取消不会退还。到期后停止本次运行，不会自动续期。';

  @override
  String get pluginsServiceRunBytes => '运行字节预算（字节，最多 67,108,864）';

  @override
  String get pluginsServiceRunCalls => '每项作业调用次数（最多 1,024）';

  @override
  String get pluginsServiceRunCancelled => '已取消';

  @override
  String get pluginsServiceRunClosed => '已关闭';

  @override
  String get pluginsServiceRunConcurrent => '并发作业数（最多 128）';

  @override
  String get pluginsServiceRunControlUnknown => '控制操作结果不明。再次操作前请刷新原服务状态。';

  @override
  String get pluginsServiceRunDenied => '已拒绝';

  @override
  String get pluginsServiceRunExited => '服务已退出';

  @override
  String get pluginsServiceRunHeaderBytes => '最大请求头大小（字节，最多 65,536）';

  @override
  String get pluginsServiceRunHint =>
      '选择已批准的发布与有限运行额度，显式启动服务。停止后等待原工作台回收，再确认结束状态。';

  @override
  String pluginsServiceRunHostFailure(String detail) {
    return '宿主诊断：$detail';
  }

  @override
  String get pluginsServiceRunHttpPanel =>
      '当前任务属于 API 服务。请使用上方服务运行面板停止服务或确认退出，HTTP 请求草稿仍会保留。';

  @override
  String get pluginsServiceRunIdentityChanged =>
      '内容正由另一任务管理。此面板不会使用原服务身份控制该任务。';

  @override
  String get pluginsServiceRunInvalid => '请检查所选服务与数值限制，尚未提交新的运行。';

  @override
  String get pluginsServiceRunInvalidOutcome => '配置无效';

  @override
  String get pluginsServiceRunJobBytes => '每项作业字节数（最多 16,777,216）';

  @override
  String get pluginsServiceRunJobs => '累计任务预留次数（最多 1,000,000）';

  @override
  String get pluginsServiceRunLastObservation => '以下为最近一次观察，当前状态尚未核实。';

  @override
  String get pluginsServiceRunLifetime => '运行时长（毫秒，最多 3,600,000）';

  @override
  String get pluginsServiceRunLimit => '已达到限额';

  @override
  String get pluginsServiceRunLocal => '内容可在本地访问';

  @override
  String pluginsServiceRunNetwork(
    String bind,
    String listener,
    String supervision,
  ) {
    return '绑定：$bind；监听：$listener；监督：$supervision';
  }

  @override
  String get pluginsServiceRunNextSettings => '下一次显式运行的设置';

  @override
  String get pluginsServiceRunNoSelection => '没有可用的已批准发布，请检查插件、配置与认证状态。';

  @override
  String get pluginsServiceRunOutboundAttempt => '本次启动绑定的端点';

  @override
  String get pluginsServiceRunOutboundClear => '清空端点选择';

  @override
  String get pluginsServiceRunOutboundFailed => '无法核实端点列表。使用已选端点前，请刷新重试。';

  @override
  String get pluginsServiceRunOutboundHint =>
      '出站 API（可选，最多 8 个）。仅列出已为当前插件批准的端点；全部不选时禁止出站调用。';

  @override
  String get pluginsServiceRunOutboundStale => '已选端点发生变化或已不可用。请明确选择当前版本，或清空选择。';

  @override
  String get pluginsServiceRunOwned => '内容由运行中的服务管理';

  @override
  String get pluginsServiceRunPending => '等待中';

  @override
  String get pluginsServiceRunReclaimed => '内容控制权已收回，等待确认';

  @override
  String get pluginsServiceRunReclaiming => '正在等待收回内容控制权';

  @override
  String get pluginsServiceRunRecovery => '清理需要修复';

  @override
  String get pluginsServiceRunRequestBytes => '最大请求大小（字节）';

  @override
  String get pluginsServiceRunResponseBytes => '最大响应大小（字节）';

  @override
  String get pluginsServiceRunRunning => '服务运行中';

  @override
  String get pluginsServiceRunSelection => '已授权的服务发布';

  @override
  String get pluginsServiceRunStale => '所选插件、配置或授权已变化。启动前请刷新记录并重新选择。';

  @override
  String get pluginsServiceRunStart => '启动有限服务';

  @override
  String get pluginsServiceRunStartRejected =>
      '启动响应报告了错误，已核对当前任务。请查看原因及可能需要的清理状态后继续操作。';

  @override
  String get pluginsServiceRunStartUnknown =>
      '启动结果不明，已保留本次尝试身份。请刷新以查找对应服务，不会自动重新启动。';

  @override
  String get pluginsServiceRunStarting => '服务正在启动';

  @override
  String get pluginsServiceRunStatusFailed => '无法核实当前服务状态。继续操作前请刷新状态。';

  @override
  String get pluginsServiceRunStop => '停止服务';

  @override
  String get pluginsServiceRunStopping => '正在停止，等待监听器和工作线程退出';

  @override
  String get pluginsServiceRunSucceeded => '成功';

  @override
  String get pluginsServiceRunTask => '当前任务身份';

  @override
  String get pluginsServiceRunTimeout => '作业超时（毫秒，最多 30,000）';

  @override
  String get pluginsServiceRunTimeoutOutcome => '已超时';

  @override
  String get pluginsServiceRunTitle => '运行 API 服务';

  @override
  String get pluginsServiceRunTotalBytes => '工作线程字节预算（最多 67,108,864）';

  @override
  String get pluginsServiceRunTransport => '传输失败';

  @override
  String get pluginsServiceRunUnavailable => '内容存储不可用';

  @override
  String get pluginsServiceSaveConfig => '保存配置';

  @override
  String get pluginsServiceSavePublication => '保存发布批准';

  @override
  String get pluginsServiceSaved => '已保存。请在下方核对返回的修订和实际到期时间。';

  @override
  String get pluginsServiceScopeAttachment => '读取附件';

  @override
  String get pluginsServiceScopeCreate => '创建内容';

  @override
  String get pluginsServiceScopeEdit => '编辑内容';

  @override
  String get pluginsServiceScopeKind => '允许的内容操作';

  @override
  String get pluginsServiceScopeQuery => '查询操作';

  @override
  String get pluginsServiceScopeRead => '读取内容';

  @override
  String get pluginsServiceScopeRename => '重命名卡片';

  @override
  String get pluginsServiceScopeSummary => '读取摘要';

  @override
  String get pluginsServiceScopesHelp =>
      '请明确选择认证，并逐项添加允许的操作及对象标识。移除范围或主体需点击对应按钮；编辑时保留原有范围。';

  @override
  String get pluginsServiceTitle => '服务配置';

  @override
  String get pluginsServiceTls => '要求 TLS';

  @override
  String get pluginsServiceTlsAttempt => '本次启动绑定的证书 PEM 摘要';

  @override
  String get pluginsServiceTlsCertificate => '选择证书链';

  @override
  String get pluginsServiceTlsChecked =>
      '证书与密钥配对已检查，证书 PEM 的 SHA-256 如下。客户端仍须验证域名、有效期与信任链。';

  @override
  String get pluginsServiceTlsChecking => '正在处理证书选择…';

  @override
  String get pluginsServiceTlsFailed => '证书检查未完成。请检查 PEM 文件、密钥配对与本地路径后重试。';

  @override
  String get pluginsServiceTlsHelp => '非本机回环地址必须要求 TLS。这里仅保存要求，不创建监听器或 TLS 身份。';

  @override
  String get pluginsServiceTlsHint =>
      '选择 PEM 证书链与私钥并检查；文件在启动时会再次校验，运行中不会自动更换证书。';

  @override
  String get pluginsServiceTlsInspect => '检查证书';

  @override
  String get pluginsServiceTlsOutsideValidity =>
      '证书链尚未生效或已过期。请检查或更换证书后再次检查，当前选择不能启动服务。';

  @override
  String get pluginsServiceTlsPrivateKey => '选择私钥';

  @override
  String get pluginsServiceTlsRecheck => '启动前请重新检查证书。时钟变化不会恢复之前的选择。';

  @override
  String get pluginsServiceTlsUnavailable => '当前后端不支持本地 TLS 证书选择。';

  @override
  String pluginsServiceTlsValidity(String end, String start) {
    return '证书链共同有效区间（UTC）：$start 至 $end。到期后服务会停止。';
  }

  @override
  String get pluginsServiceTokenDiscarded =>
      '面板关闭期间，一次性令牌已清除。如有需要，请明确执行新的签发操作。';

  @override
  String get pluginsServiceTokenHelp =>
      '令牌仅在本次显示。如有需要，请主动复制。清除或关闭此面板会从会话中移除令牌，列表无法再次读出。轮换会替换此前的令牌。';

  @override
  String get pluginsServiceUncertainHelp =>
      '请先刷新并核对原记录。确认此提示仅允许执行下一次明确操作，并不证明上一次修改失败，也不会重发。';

  @override
  String get pluginsServiceUnsupported => '不支持的历史值';

  @override
  String get pluginsServiceWorking => '正在处理…';

  @override
  String get pluginsServiceWriteUnknown => '上一次修改的结果尚不确定，系统没有再次发送。';

  @override
  String get pluginsServiceYes => '是';

  @override
  String get pluginsSettingsUnknown => '插件设置未确认，请刷新状态后重新选择。';

  @override
  String get pluginsSnapshotDetails =>
      '内容库备份包含库内卡片、附件和审计记录；外部素材保留引用，恢复仍需原系统账户。';

  @override
  String get pluginsSnapshotSaved => '内容库已备份，包含库内附件与原保护文件。';

  @override
  String get pluginsStateUnavailable => '插件状态暂时无法读取，请重试。';

  @override
  String get pluginsSummary => '读取摘要';

  @override
  String get pluginsTextInput => '输入文字';

  @override
  String get pluginsThirdParty => '第三方插件';

  @override
  String get pluginsTlsIdentitiesDisable => '禁用身份';

  @override
  String get pluginsTlsIdentitiesEmpty => '当前资料库尚无已保存身份。';

  @override
  String get pluginsTlsIdentitiesFileMode => '下次启动：已检查的本地文件。';

  @override
  String get pluginsTlsIdentitiesHint =>
      '请明确选择身份。替换或禁用身份会停止使用它的服务，采用新身份需要再次启动。';

  @override
  String get pluginsTlsIdentitiesImport => '准备导入或替换的证书文件';

  @override
  String get pluginsTlsIdentitiesReplace => '用已检查文件替换';

  @override
  String get pluginsTlsIdentitiesSave => '保存为新身份';

  @override
  String get pluginsTlsIdentitiesSaved => '已保存。请核对下方身份与修订，需要启动时再明确选择。';

  @override
  String get pluginsTlsIdentitiesSavedMode => '下次启动：已保存身份。宿主会在启动时重新检查证书有效期。';

  @override
  String get pluginsTlsIdentitiesSelect => '用于下次启动';

  @override
  String get pluginsTlsIdentitiesStale => '已选身份已变化、被禁用或尚未刷新。请重新选择当前身份。';

  @override
  String get pluginsTlsIdentitiesTitle => '已保存的 TLS 身份';

  @override
  String get pluginsTlsIdentitiesUnknownHint =>
      '请先刷新并核对记录，再确认继续。回执丢失不代表修改失败，未核对前请勿重复创建。';

  @override
  String get pluginsTlsIdentitiesUseFile => '下次启动使用已检查文件';

  @override
  String get pluginsTransform => '转换';

  @override
  String get pluginsTransformUnknown => '转换结果未能确认';

  @override
  String get pluginsUiExecution => '插件执行未完成，请重新打开界面后重试。';

  @override
  String get pluginsUiRejected => '插件操作未被接受，请检查输入与当前权限。';

  @override
  String get pluginsUiUnavailable => '插件已不可用，请检查状态并重新打开界面。';

  @override
  String get pluginsUnavailableView => '插件界面暂不可用';

  @override
  String pluginsUnconfirmed(String reason) {
    return '$reason。操作未确认，请核对刷新后的状态再选择。';
  }

  @override
  String get pluginsUninstallKeepContent => '卸载（保留内容）';

  @override
  String get pluginsUninstallUnknown => '卸载结果未能确认';

  @override
  String get pluginsUninstalled => '已卸载，已有内容仍保留。';

  @override
  String get pluginsUpdatingView => '正在更新预览…';

  @override
  String get pluginsUseText => '改用文字';

  @override
  String get pluginsUseTransform => '使用转换';

  @override
  String get pluginsViewFailed => '插件界面未能打开';

  @override
  String get pluginsWorkbench => '工作台插件';

  @override
  String get pluginsWorkbenchReadOnly => '插件已允许，但当前工作台只读；处理内容库或插件提示后可刷新状态。';

  @override
  String get recoveryAllFiles => '所有文件';

  @override
  String get recoveryBackupExists => '备份位置已有文件，请选择新的文件名。';

  @override
  String get recoveryBackupFile => '内容库备份';

  @override
  String get recoveryBackupUnknown => '备份结果需要核对，请保留当前文件并检查保存位置。';

  @override
  String get recoveryBindingMissing => '此内容库尚未绑定保护文件，无法核对所选文件的归属。';

  @override
  String get recoveryBusy => '此内容库正在由另一个进程使用，请关闭另一个窗口后重试。';

  @override
  String get recoveryChooseKey => '选择恢复文件';

  @override
  String get recoveryCloseFirst => '工作台仍在运行，请先关闭后再切换内容库。';

  @override
  String get recoveryFailed => '恢复未完成，请保留原文件并重试。';

  @override
  String get recoveryIdentityBusy => '同一内容库身份的另一份副本正在使用中，请先关闭原工作台再打开此副本。';

  @override
  String get recoveryIdentityMismatch => '已登记的内容库身份不匹配，请保留原资料并选择正确备份恢复。';

  @override
  String get recoveryKeyFile => '内容库保护文件';

  @override
  String get recoveryKeyGuide => '保护文件丢失或损坏时，可选择原文件的备份进行恢复。文件需属于此内容库，并使用原系统账户。';

  @override
  String get recoveryKeyMismatch => '内容库保护密钥不匹配或无法解密，请使用原文件及原系统账户。';

  @override
  String get recoveryKeyUnknown => '恢复结果需要核对，请重试打开；原保护文件副本已保留（若此前存在）。';

  @override
  String get recoveryLibraryInvalid => '内容库无法验证或打开，请保留原内容库与保护密钥后重试。';

  @override
  String get recoveryMaintenance => '内容库需要检查，请保留原文件并核对诊断信息。';

  @override
  String get recoveryMissingKey => '内容库保护密钥缺失，请恢复原 .audit-key 文件后重试。';

  @override
  String get recoveryMissingLibrary => '保护密钥仍在，但内容库缺失或为空，请恢复原内容库。';

  @override
  String get recoveryOpenFailed => '工作台暂时无法打开，请检查插件文件与数据目录后重试。';

  @override
  String get recoveryPluginUnavailable => '工作台插件不可用，已有内容仍可查看和导出。';

  @override
  String get recoveryRegistryInvalid => '活动内容库登记已损坏或不受支持，已停止打开以保护资料。';

  @override
  String get recoveryRegistryUnreadable => '活动内容库或登记文件无法读取，请检查原位置；不会自动创建替代内容库。';

  @override
  String get recoveryRetry => '重试打开';

  @override
  String get recoverySnapshot => '从内容库备份恢复';

  @override
  String get recoverySnapshotGuide =>
      '也可从内容库备份恢复到新目录并切换工作台。原目录会保留；恢复的是备份时的内容，仍需原系统账户。';

  @override
  String get recoverySnapshotInvalid => '内容库备份格式或校验不正确，请保留原备份文件。';

  @override
  String get recoverySnapshotUnknown => '内容库恢复结果需要核对，请检查目标目录；原内容库未被替换。';

  @override
  String recoverySwitchUnconfirmed(String path) {
    return '备份已恢复至 $path，切换结果未确认，请保留此目录并重新打开工作台核对。';
  }

  @override
  String get recoverySwitchUnknown => '内容库切换结果尚未确认，请重新打开工作台核对。';

  @override
  String get recoveryTargetExists => '恢复目标已存在，请选择尚不存在的新目录。';

  @override
  String get recoveryTitle => '重新打开工作台';

  @override
  String get visualApplyColor => '应用颜色';

  @override
  String get visualApplyComponent => '应用到此组件';

  @override
  String get visualApplyTexture => '应用素材';

  @override
  String visualAttachmentDetails(String action, String extension, String size) {
    return '$extension · $size · $action';
  }

  @override
  String get visualAttachmentFailure => '文件操作失败，请检查文件与存储空间。';

  @override
  String get visualAttachmentPreview => '本地附件预览';

  @override
  String get visualAttachmentReadFailure => '附件无法读取，请重新导入。';

  @override
  String get visualAudio => '音频';

  @override
  String get visualAudioStateFailure => '声音状态未能确认，请重试。';

  @override
  String get visualAutoLyrics => '自动联网补全歌词';

  @override
  String get visualCancel => '取消';

  @override
  String get visualChangeCover => '更换歌曲封面';

  @override
  String get visualChooseAudio => '请选择音频或同名 LRC 歌词文件。';

  @override
  String get visualChooseLyrics => '请选择 LRC 或 TXT 歌词文件。';

  @override
  String get visualClickPreview => '点击预览';

  @override
  String get visualClose => '关闭';

  @override
  String get visualCloseDialog => '关闭弹窗';

  @override
  String get visualCloseWindow => '关闭窗口';

  @override
  String get visualCollapsePlaylist => '收起播放列表';

  @override
  String get visualColorGuide => '拖动罗盘选取色相与饱和度，再调整明暗。也可以直接输入色值。';

  @override
  String get visualColorTitle => '给空间一点颜色';

  @override
  String visualComponentCompass(String title) {
    return '$title · 调色罗盘';
  }

  @override
  String get visualComponents => '组件与卡片';

  @override
  String get visualComponentsGuide => '每项单独设置，默认跟随主题。修改一张卡片不会影响其他卡片。';

  @override
  String get visualCornerTips1 => '不必每个想法都有用\n有些只是让今天更有趣。';

  @override
  String get visualCornerTips2 => '先写下来，再慢慢想\n灵感不必一次就完整。';

  @override
  String get visualCornerTips3 => '留一点空白给自己\n好奇心也需要呼吸。';

  @override
  String get visualCornerTips4 => '今天试一点新东西\n小小的偏离，也有惊喜。';

  @override
  String get visualCornerTips5 => '走神也可能有收获\n给思绪一条散步的小路。';

  @override
  String get visualCornerTips6 => '给喜欢的事一点时间\n不用急着证明它的意义。';

  @override
  String get visualCornerTips7 => '进度可以很小\n愿意开始就已经很好。';

  @override
  String get visualCornerTips8 => '偶尔抬头看看窗外\n生活也是灵感的来源。';

  @override
  String get visualCover => '封面';

  @override
  String get visualCustomCompass => '调色罗盘 · 自定义';

  @override
  String get visualCustomMaterialGuide => '关闭后跟随主题，保留本项自定义参数';

  @override
  String get visualDefaultOpen => '使用默认应用打开';

  @override
  String get visualDownloadOpen => '下载后打开';

  @override
  String get visualEmbeddedLyrics => '音频内嵌';

  @override
  String get visualExpandPlaylist => '展开播放列表';

  @override
  String get visualFile => '文件';

  @override
  String get visualFileOpenFailure => '无法打开文件，请先安装对应应用，或将附件另存后打开。';

  @override
  String get visualFileRetry => '文件操作失败，请重试。';

  @override
  String get visualFindLyrics => '查找歌词';

  @override
  String get visualFindLyricsGuide => '按歌名和歌手查询 LRCLIB，选择对应版本。';

  @override
  String get visualFollowTheme => '跟随主题';

  @override
  String get visualFooterLyrics => '底部显示歌词';

  @override
  String get visualFooterTips => '底部显示提示语';

  @override
  String get visualFooterTips1 => '没有紧迫的事。给好奇心一点时间。';

  @override
  String get visualFooterTips10 => '不用填满每一分钟。留一点余地。';

  @override
  String get visualFooterTips2 => '想到什么就记一点，不用马上整理。';

  @override
  String get visualFooterTips3 => '把大的想法，拆成今天的一小步。';

  @override
  String get visualFooterTips4 => '伸个懒腰，让眼睛休息一会儿。';

  @override
  String get visualFooterTips5 => '允许一个想法暂时没有答案。';

  @override
  String get visualFooterTips6 => '有些收获，会在慢下来以后出现。';

  @override
  String get visualFooterTips7 => '收藏一个细节，也是在照顾灵感。';

  @override
  String get visualFooterTips8 => '今天的随手一记，可能是明天的开始。';

  @override
  String get visualFooterTips9 => '走一会儿神，再回到喜欢的事情。';

  @override
  String get visualFrosting => '磨砂效果';

  @override
  String get visualGif => 'GIF 动图';

  @override
  String get visualHexColor => 'HEX 色值';

  @override
  String get visualHexInvalid => '请输入 6 位十六进制色值';

  @override
  String get visualImage => '图片';

  @override
  String get visualImageDecodeFailure => '图片无法解码，可另存后打开。';

  @override
  String visualImageLoadFailure(String name) {
    return '图片无法加载：$name';
  }

  @override
  String visualImageNotImported(String name) {
    return '$name（图片未导入）';
  }

  @override
  String visualImageUnavailable(String name) {
    return '图片暂不可用：$name';
  }

  @override
  String get visualImportFailure => '导入失败，请检查文件、编码与存储空间。';

  @override
  String get visualImportLyrics => '导入歌词';

  @override
  String get visualImportMusic => '导入音乐';

  @override
  String get visualImportMusicHint => '点击 + 导入本地歌曲';

  @override
  String get visualIndependentMaterial => '独立材质';

  @override
  String get visualInheritColor => '使用主题染色';

  @override
  String get visualLinkFailure => '无法打开链接，请复制地址后重试。';

  @override
  String visualLoadImage(String name) {
    return '加载图片 · $name';
  }

  @override
  String get visualLoading => '正在载入…';

  @override
  String get visualLyricsEmpty => '歌词文件为空。';

  @override
  String get visualLyricsFile => '歌词文件';

  @override
  String get visualLyricsImportHint => '可导入歌词文件，或联网搜索。';

  @override
  String get visualLyricsLoading => '正在读取歌词…';

  @override
  String visualLyricsMatch(String album, String kind, int seconds) {
    return '$album\n$kind · $seconds 秒';
  }

  @override
  String get visualLyricsMissing => '未找到歌词，可导入或重新搜索';

  @override
  String get visualLyricsNotFound => '没有找到歌词，试试调整歌名或歌手。';

  @override
  String get visualLyricsOnPlay => '播放时自动读取歌词';

  @override
  String get visualLyricsParseFailure => '歌词解析未完成，可重新导入';

  @override
  String get visualLyricsReadFailure => '歌词读取失败，可手动导入或重试';

  @override
  String get visualLyricsServiceFailure => '无法连接歌词服务，请稍后重试或导入本地歌词。';

  @override
  String get visualLyricsSize => '歌词文件请控制在 1 MB 以内。';

  @override
  String get visualLyricsSources => '本地文件 → 内嵌 → LRCLIB';

  @override
  String get visualLyricsVersions => '存在多个版本，请在搜索中选择';

  @override
  String get visualMaterialPreview => '材质预览';

  @override
  String get visualMaximize => '最大化';

  @override
  String get visualMediaAddress => '媒体地址';

  @override
  String get visualMediaAddressInvalid => '请输入有效且不含登录信息的 HTTP / HTTPS 地址';

  @override
  String get visualMediaPreviewFailure => '此媒体无法预览，可另存后使用其他应用打开。';

  @override
  String get visualMediaType => '素材类型';

  @override
  String get visualMinimize => '最小化';

  @override
  String get visualMusic => '音乐';

  @override
  String get visualMusicEmptyTitle => '留一点空间给音乐';

  @override
  String get visualMusicPlayer => '随身听';

  @override
  String get visualNextTrack => '下一首';

  @override
  String get visualNoLyricsRead => '尚未读取到歌词';

  @override
  String visualNoLyricsTitle(String title) {
    return '♪ $title · 暂无歌词';
  }

  @override
  String get visualNoTimeline => '无时间轴';

  @override
  String get visualOpacity => '不透明度';

  @override
  String get visualOptionalArtist => '歌手（可选）';

  @override
  String get visualPauseMusic => '暂停音乐';

  @override
  String get visualPaused => '已暂停';

  @override
  String get visualPlainLyrics => '纯文本歌词';

  @override
  String get visualPlayMusic => '播放音乐';

  @override
  String get visualPlaybackFailure => '歌曲无法播放，请检查文件或更换音频格式。';

  @override
  String visualPlaybackPosition(int count, int index, String state) {
    return '$index / $count · $state';
  }

  @override
  String get visualPlaybackRequestFailure => '播放请求未能完成，请重试。';

  @override
  String get visualPlaying => '播放中';

  @override
  String get visualPlaylistEmpty => '播放列表还是空的';

  @override
  String get visualPlaylistLyricsHint => '从播放列表菜单导入 LRC 歌词';

  @override
  String get visualPlaylistSaved => '播放列表与歌词自动保存';

  @override
  String get visualPlaylistUpdateFailure => '播放列表未能更新，请重试。';

  @override
  String get visualPreviewColor => '预览色值';

  @override
  String get visualPreviousTrack => '上一首';

  @override
  String get visualRemoveAttachment => '移除附件';

  @override
  String get visualRemoveTrack => '移出播放列表';

  @override
  String get visualResetMaterial => '重置为跟随主题';

  @override
  String get visualRestoreWindow => '还原';

  @override
  String get visualSaveAttachment => '另存附件';

  @override
  String get visualSearch => '搜索';

  @override
  String get visualSearchLyrics => '搜索歌词';

  @override
  String get visualSongCover => '歌曲封面';

  @override
  String get visualSongTitle => '歌名';

  @override
  String get visualSyncedLyrics => '同步歌词';

  @override
  String get visualTextureFailure => '素材加载失败，请检查文件、网络地址或格式。网页直链还需允许跨域访问。';

  @override
  String get visualTextureLinkGuide =>
      '粘贴图片、GIF 或视频的 HTTP / HTTPS 直链。网页分享链接需要先找到原始媒体地址。';

  @override
  String get visualTextureLinkTitle => '从外面带一点灵感';

  @override
  String get visualTexturePlaybackGuide =>
      '视频默认静音循环播放，可在设置中开启声音。网络素材需允许访问，网页端还需支持跨域加载。';

  @override
  String get visualTipsMaterialGuide => '关闭时保持透明浮层；开启后使用下方材质。自定义参数会保留。';

  @override
  String get visualTransparentTips => '透明浮层（默认）';

  @override
  String get visualUseCustomMaterial => '使用自定义材质';

  @override
  String get visualVideo => '视频';

  @override
  String get visualViewLyrics => '查看歌词';
}
