// ignore: unused_import
import 'package:intl/intl.dart' as intl;
import 'app_localizations.dart';

// ignore_for_file: type=lint

/// The translations for English (`en`).
class AppLocalizationsEn extends AppLocalizations {
  AppLocalizationsEn([String locale = 'en']) : super(locale);

  @override
  String get commonAppName => 'Morrow';

  @override
  String get commonCancel => 'Cancel';

  @override
  String commonCount(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count items',
      one: '1 item',
      zero: 'No items',
    );
    return '$_temp0';
  }

  @override
  String commonGreeting(String name) {
    return 'Hello, $name';
  }

  @override
  String get importsAttachmentLimit =>
      'Up to 20 attachments can be imported at once. Paste the rest separately.';

  @override
  String get importsClipboardChanged =>
      'The clipboard changed while being read. Please paste again.';

  @override
  String get importsEmbeddedImageUnreadable =>
      'An embedded image could not be read.';

  @override
  String get importsEmbeddedImagesSeparate =>
      'Some embedded images need to be imported as separate files.';

  @override
  String get importsExcelValues =>
      'Table values and formulas were converted to Markdown. Formatting and merged cells are preserved in the XML attachment.';

  @override
  String get importsExcelXmlKept =>
      'The original Excel table has been preserved as an XML attachment.';

  @override
  String get importsFileTooLarge => 'The clipboard file exceeds 200 MB.';

  @override
  String get importsItemLimit =>
      'Only the first 20 items were read. Paste additional items separately.';

  @override
  String get importsItemUnreadable =>
      'One clipboard item could not be read. Other readable content has been kept.';

  @override
  String get importsLocalImageNotRead =>
      'Local linked images were not read automatically. Paste the image or import the original file.';

  @override
  String get importsMergedTable =>
      'Merged cells were converted to a readable table. Original formatting is preserved in the HTML attachment.';

  @override
  String get importsOfficeBusy =>
      'Another application is using the clipboard. Office objects were not read.';

  @override
  String get importsOfficeEmbeddedKept =>
      'The embedded Office object is preserved as an original attachment. Edit its charts, formulas and layout in the original application.';

  @override
  String get importsOfficeExportFailed =>
      'An Office object exceeds the limit or could not be exported. Save it in the original application, then import it.';

  @override
  String get importsOfficeReadFailed =>
      'Office content could not be read. Other clipboard content is still available.';

  @override
  String get importsOfficeUnavailable =>
      'The Office clipboard is temporarily unavailable.';

  @override
  String get importsOfficeUnreadable =>
      'The original Office object could not be read. Other available content has been kept.';

  @override
  String get importsRichFallback =>
      'Some rich text formatting could not be converted. Readable text has been kept.';

  @override
  String get importsRichTooLarge =>
      'Rich text exceeds 2 MB. Please import the document as an attachment.';

  @override
  String get importsRtfTooLarge =>
      'The RTF content is too large. Please import the original document.';

  @override
  String get importsSpreadsheetTooLarge =>
      'The table is too large. Please import the Excel file.';

  @override
  String get importsTableConverted =>
      'The table was converted to Markdown. Complete data is preserved in the TSV attachment.';

  @override
  String get importsTextTooLarge =>
      'The text exceeds 2 MB. Please import it as a file.';

  @override
  String get importsTotalTooLarge =>
      'The files in this paste exceed 200 MB in total. Import them in smaller batches.';

  @override
  String get importsUnsupported =>
      'Clipboard access is not supported here. Please import a file.';

  @override
  String get mainActiveProjects => 'In progress';

  @override
  String get mainAdjustCustomTone => 'Adjust custom tone';

  @override
  String get mainAmbientDetail => 'Flowing light adds a touch of color.';

  @override
  String get mainAppTitle => 'Morrow — Room for ideas';

  @override
  String get mainAppearance => 'Appearance';

  @override
  String get mainArrangeIdeas => 'Sort ideas';

  @override
  String mainAttachmentCount(int count) {
    return 'Attachments · $count';
  }

  @override
  String mainAttachmentHint(int count, String name) {
    return '$count attachments · $name';
  }

  @override
  String get mainAttachmentLimit =>
      'Each record supports up to 20 attachments.';

  @override
  String get mainAutosaveNotice => 'Appearance and ideas are saved locally';

  @override
  String get mainAwaitDiscovery => 'Waiting for a new discovery';

  @override
  String get mainBackToWorkbench => 'Back to workspace';

  @override
  String get mainBackgroundCanvas => 'Background canvas';

  @override
  String get mainBackgroundSound => 'Play background audio';

  @override
  String get mainBodyHint =>
      'Write your thoughts or paste some content…\n\nSupports # headings, lists, tables and code blocks';

  @override
  String get mainBrightWhite => 'White';

  @override
  String get mainBuiltinTexture => 'Use built-in texture';

  @override
  String get mainCanvasCompass => 'Canvas tint wheel';

  @override
  String get mainCaptureIdea => 'Capture idea';

  @override
  String get mainCaptureNow => 'Capture a thought';

  @override
  String mainCardAttachments(int count, String name) {
    return 'Files $count · $name';
  }

  @override
  String get mainCategoryExperiment => 'Experiment';

  @override
  String get mainCategoryIdea => 'Idea';

  @override
  String get mainCategoryProject => 'Project';

  @override
  String get mainCategoryPrompt => 'Where to keep it';

  @override
  String get mainChangeFailed =>
      'This change could not be saved. Your draft is preserved; you can retry.';

  @override
  String get mainCheckAgain => 'Check again';

  @override
  String get mainClearSearch => 'Clear search';

  @override
  String get mainClipboardEmpty =>
      'No readable text or files in the clipboard. Copy a file from your file manager, or use Import file.';

  @override
  String get mainClipboardReadFailed =>
      'Could not read the content. Import a file, or check file and clipboard permissions.';

  @override
  String get mainClipboardSupport =>
      'Supports Markdown, Office rich text and tables, screenshots and files. Complex objects keep their original attachments. Up to 20 attachments, 200 MB each.';

  @override
  String get mainCollapseSidebar => 'Collapse sidebar';

  @override
  String get mainCompletedProjects => 'Completed';

  @override
  String get mainComponentCompass => 'Component tint wheel';

  @override
  String get mainComponentEmpty => 'Empty state';

  @override
  String get mainComponentFooter => 'Footer tips and lyrics';

  @override
  String get mainComponentHero => 'Overview card';

  @override
  String get mainComponentNavigation => 'Sidebar navigation';

  @override
  String get mainComponentQuickCapture => 'Quick capture';

  @override
  String get mainComponentSearch => 'Search bar';

  @override
  String get mainComponentSettings => 'Components and cards · Settings';

  @override
  String get mainContentProtection => 'Content protection';

  @override
  String get mainContentRead => 'Content read';

  @override
  String mainContentReadFiles(int count) {
    return 'Content read; $count attachments preserved';
  }

  @override
  String get mainCornerRadius => 'Corner radius';

  @override
  String get mainCredentialSettings => 'Credentials';

  @override
  String get mainCrystal => 'Crystal clear';

  @override
  String get mainCrystalDetail =>
      'Light and clear, letting color shine through.';

  @override
  String get mainCuriosity =>
      'Interesting things begin\nwith a little curiosity.';

  @override
  String get mainCustomCompass => 'Color wheel · Custom';

  @override
  String get mainCustomLightness => 'Custom lightness';

  @override
  String get mainCustomTheme => 'Custom';

  @override
  String get mainDaily => 'Little things';

  @override
  String get mainDailyExplore => 'Take ten minutes to explore';

  @override
  String get mainDailyIdea => 'Write down an idea';

  @override
  String get mainDailyWater => 'Pour yourself a glass of water';

  @override
  String get mainDarkTheme => 'Dark';

  @override
  String get mainDeepBlack => 'Black';

  @override
  String get mainDefaultCanvas => 'Default';

  @override
  String get mainDefaultGlobalColor => 'Default theme color · All controls';

  @override
  String get mainDelete => 'Delete';

  @override
  String mainDeleted(String title) {
    return 'Deleted “$title”';
  }

  @override
  String get mainDiagnosticDetails => 'Diagnostic details';

  @override
  String get mainDone => 'Done';

  @override
  String get mainEdit => 'Edit';

  @override
  String get mainEditIdeaTitle => 'Make your idea clearer';

  @override
  String get mainEditorClosedUnknown =>
      'The editor closed, but the save is not yet confirmed. Reopen the workspace to check before creating another copy.';

  @override
  String get mainEditorSubtitle =>
      'Text, tables, images—keep them here while your idea takes shape.';

  @override
  String get mainEditorUnavailable =>
      'The editor is unavailable. Check the content service and retry.';

  @override
  String get mainEndpointSettings => 'Outbound endpoints';

  @override
  String get mainExpandSettings => 'Expand settings';

  @override
  String get mainExpandSidebar => 'Expand sidebar';

  @override
  String get mainExtensionPlugins => 'Extensions';

  @override
  String get mainFavoriteAttachments => 'Saved attachments';

  @override
  String get mainFavoriteRecords => 'Saved records';

  @override
  String mainFavoriteTooltip(String title) {
    return 'Favorite $title';
  }

  @override
  String get mainFavoritesIntro =>
      'Your favorite text, images and files, all together.';

  @override
  String mainFieldLimit(int limit) {
    return 'This field supports up to $limit characters. Shorten the content or import it as a file.';
  }

  @override
  String get mainFilterAll => 'All';

  @override
  String get mainFilterAttachments => 'With files';

  @override
  String get mainFilterFavorites => 'Favorites only';

  @override
  String get mainFilterFile => 'Files';

  @override
  String get mainFilterImage => 'Images';

  @override
  String get mainFilterMedia => 'Audio / video';

  @override
  String get mainFilterPending => 'To do';

  @override
  String get mainFilterText => 'Text';

  @override
  String get mainFollowTheme => 'Follow theme';

  @override
  String get mainFrostDetail =>
      'Soften the background and give your thoughts room.';

  @override
  String get mainFrostEffect => 'Blur';

  @override
  String get mainFrostOpacity => 'Frost opacity';

  @override
  String get mainFrostUnavailable =>
      'Desktop blur is unavailable on this system. Tint and opacity can still be adjusted.';

  @override
  String get mainFrosted => 'Frosted';

  @override
  String get mainGlassTexture => 'Glass style';

  @override
  String mainGlobalColor(String color) {
    return '$color · All controls';
  }

  @override
  String get mainGreeting => 'Let your ideas grow.';

  @override
  String get mainGreetingDetail =>
      'Keep the everyday details and the sparks of inspiration.';

  @override
  String get mainHeroBody =>
      'A thought, a small task, a what-if.\nThis is where they begin.';

  @override
  String get mainHeroCaption => 'THE POSSIBILITY CORNER';

  @override
  String get mainHeroTitle => 'It is okay to start small.';

  @override
  String get mainHideAppearance => 'Hide appearance settings';

  @override
  String get mainHideCustomTone => 'Hide custom tone';

  @override
  String get mainHidePreview => 'Hide preview';

  @override
  String get mainHttpSettings => 'HTTP tasks';

  @override
  String get mainHypothesis => 'Hypothesis';

  @override
  String get mainHypothesisPrompt => 'Hypothesis to test';

  @override
  String get mainHypothesisSection => 'Hypothesis / What to try';

  @override
  String get mainIdeaDetails => 'Keep the details. Make the next step clearer.';

  @override
  String get mainIdeaNameHint => 'Give it a name';

  @override
  String get mainIdeaNameRequired => 'Write down your idea first';

  @override
  String get mainIdeaSaved => 'Idea saved.';

  @override
  String get mainImportFailed =>
      'Could not import the media. Check the file and available storage.';

  @override
  String get mainImportFile => 'Import file';

  @override
  String get mainInboxIntro =>
      'Capture first, organize later. Turn promising ideas into small projects.';

  @override
  String get mainIoNoDeclarations =>
      'No installed plugin currently declares file or network access.';

  @override
  String get mainIoSettings => 'Network & file access';

  @override
  String get mainIoSettingsGuide =>
      'Manage plugin file and network permissions separately from appearance. Approving a declared capability does not grant access to every file or endpoint; available operations depend on the current backend.';

  @override
  String get mainIoSettingsSummary =>
      'Permissions, credentials, endpoints and API services';

  @override
  String get mainJustNow => 'Just now';

  @override
  String get mainLabIntro =>
      'Start with a hypothesis. Keep your attempts, observations and surprises.';

  @override
  String get mainLanguage => 'Language';

  @override
  String get mainLanguageChinese => '简体中文';

  @override
  String get mainLanguageEnglish => 'English';

  @override
  String get mainLanguageSystem => 'System default';

  @override
  String get mainLavender => 'Lavender';

  @override
  String get mainLightOpacity => '20% · Light';

  @override
  String get mainLiquidAllCanvases =>
      'Available independently on all four canvas types';

  @override
  String get mainLiquidDetail =>
      'Flowing highlights and gentle refraction, like a suspended drop of water.';

  @override
  String get mainLiquidEffect => 'Liquid glass effect';

  @override
  String get mainLiquidGlass => 'Liquid glass';

  @override
  String get mainLivePreview => 'Live preview';

  @override
  String get mainLocalMedia => 'Local media';

  @override
  String get mainMakeYours => 'MAKE IT YOURS';

  @override
  String get mainMarkOrganized => 'Mark as organized';

  @override
  String get mainMarkdownBody => 'Body · Markdown';

  @override
  String get mainMediaLimits => 'Images / GIFs ≤ 25 MB; videos ≤ 150 MB';

  @override
  String get mainMonochrome => 'Monochrome';

  @override
  String mainMoreSteps(int count) {
    return '$count more steps; open to view';
  }

  @override
  String mainMovedProject(String title) {
    return '“$title” moved to Projects';
  }

  @override
  String get mainMusic => 'Music player';

  @override
  String get mainMySpace => 'My space';

  @override
  String get mainNavigation => 'Navigation';

  @override
  String get mainNewIdea => 'New idea';

  @override
  String get mainNewIdeaTitle => 'Catch a new idea';

  @override
  String get mainNoHypothesis => 'No hypothesis yet';

  @override
  String get mainNoMatches => 'No matching ideas here';

  @override
  String get mainNoResultYet =>
      'The result can wait. The process is worth recording too.';

  @override
  String get mainNotNow => 'Not now';

  @override
  String get mainObservationSection => 'Observations / What you found';

  @override
  String get mainObservations => 'Observations and conclusions';

  @override
  String get mainObservationsPrompt => 'Observations, process and conclusions';

  @override
  String get mainOneHourAgo => '1 hour ago';

  @override
  String get mainOnlineMedia => 'Online media';

  @override
  String get mainOpaqueFallback => 'Clear panels over the current theme color.';

  @override
  String get mainOpenNextStep => 'Open project to edit next steps';

  @override
  String get mainOrganizedCount => 'Organized';

  @override
  String get mainOriginalColors => 'Original colors';

  @override
  String get mainPageFavorites => 'Favorites';

  @override
  String get mainPageInbox => 'Inbox';

  @override
  String get mainPageLaboratory => 'Lab';

  @override
  String get mainPageOverview => 'Overview';

  @override
  String get mainPageProjects => 'Projects';

  @override
  String mainPageSummary(String page) {
    return '$page · Overview';
  }

  @override
  String get mainPasteChanged =>
      'The input changed while pasting. Please reopen the editor.';

  @override
  String get mainPasteContent => 'Paste content';

  @override
  String get mainPause => 'Pause';

  @override
  String get mainPersonalWorkspace => 'Personal workspace';

  @override
  String get mainPlay => 'Play';

  @override
  String get mainPluginSettings => 'Plugins and services';

  @override
  String get mainPluginSettingsSummary =>
      'Built-in tools, extensions, network and file access, and content protection';

  @override
  String get mainPreviewEmpty => 'Your preview will appear here';

  @override
  String mainProgress(int done, int total) {
    return 'Small steps · $done/$total';
  }

  @override
  String get mainProjectIntro =>
      'Use checklists to make progress. Every small step brings you closer.';

  @override
  String get mainQueryAgain => 'Run a new query';

  @override
  String get mainQueryCapacity => 'Query history is full';

  @override
  String get mainQueryCapacityDetail =>
      'Your content is preserved. This version cannot clear query history yet.';

  @override
  String get mainQueryLoading => 'Finding ideas…';

  @override
  String get mainQueryRetry => 'Retry query';

  @override
  String get mainQueryTerminated => 'This query has ended';

  @override
  String get mainQueryUnknown => 'Results are not yet confirmed';

  @override
  String get mainQuickHint => 'What just came to mind?';

  @override
  String get mainReadOnlySettings =>
      'Content is read-only. Check the workbench plugin to restore editing.';

  @override
  String get mainRecentThoughts => 'Recent thoughts';

  @override
  String get mainRecordedCount => 'With observations';

  @override
  String get mainRestoreDefault => 'Reset to default';

  @override
  String get mainRetry => 'Retry';

  @override
  String get mainRetrySave => 'Retry save';

  @override
  String get mainSage => 'Sage';

  @override
  String get mainSampleBody0 =>
      'Keep sudden thoughts here.\nNo rush to finish—just let them begin.';

  @override
  String get mainSampleBody1 =>
      'A small page for favorite words,\nmusic and everyday details.';

  @override
  String get mainSampleBody2 =>
      'Try generative art. Let code grow\ninto unexpected shapes.';

  @override
  String get mainSampleBody3 =>
      'A quiet companion to help remember\nthe little things that slip away.';

  @override
  String get mainSampleTitle0 => 'A home for ideas';

  @override
  String get mainSampleTitle1 => 'A quiet digital garden';

  @override
  String get mainSampleTitle2 => 'Make something just for fun';

  @override
  String get mainSampleTitle3 => 'My little desktop helper';

  @override
  String get mainSampleTodo0 => 'Organize the first collection';

  @override
  String get mainSampleTodo1 => 'Design the garden entrance';

  @override
  String get mainSampleTodo2 => 'Plant a new idea';

  @override
  String get mainSampleTodo3 => 'Sketch a small prototype';

  @override
  String get mainSampleTodo4 => 'Design the reminder interaction';

  @override
  String get mainSaveConnectionUnknown =>
      'The connection was interrupted; the save result is unknown. Reopen the library to confirm before retrying.';

  @override
  String get mainSaveFailed =>
      'Could not save. Your changes remain in this session.';

  @override
  String get mainSaveIdea => 'Save idea';

  @override
  String get mainSaveNotSubmitted =>
      'Not submitted. Your draft and attachments are preserved; you can edit and save again.';

  @override
  String get mainSaveReadOnly =>
      'Changes are not saved. Enable the workbench plugin in Plugins and services, then retry.';

  @override
  String get mainSaveUnknown =>
      'This save is not yet confirmed. Your draft and attachments are preserved. Retry this submission; closing will refresh the workspace to check.';

  @override
  String get mainSaving => 'Saving…';

  @override
  String get mainSearchHint => 'Search your ideas…';

  @override
  String get mainServiceRunSettings => 'Service runtime';

  @override
  String get mainServiceSettings => 'API services';

  @override
  String get mainSettings => 'Settings';

  @override
  String get mainShowAppearance => 'Show appearance settings';

  @override
  String get mainSidebarMotto => 'A little order. Room to wonder.';

  @override
  String get mainSlowProgress => 'Small steps still move you forward.';

  @override
  String get mainSolidCanvas => 'Solid';

  @override
  String get mainSolidDetail => 'A calm, solid-color canvas.';

  @override
  String get mainSolidOpacity => '100% · Solid';

  @override
  String get mainSortFavorites => 'Favorites first';

  @override
  String get mainSortRecent => 'Recently added';

  @override
  String get mainSortTitle => 'By title';

  @override
  String get mainSquareCorners => 'Set to 0 for square corners';

  @override
  String get mainStageActive => 'In progress';

  @override
  String get mainStageCompleted => 'Completed';

  @override
  String get mainStageOrganized => 'Organized';

  @override
  String get mainStagePlanned => 'Planned';

  @override
  String get mainStageRecorded => 'Recorded';

  @override
  String mainStageTooltip(String title) {
    return 'Change stage for $title';
  }

  @override
  String get mainStageUnsorted => 'To organize';

  @override
  String get mainStageUnverified => 'To test';

  @override
  String get mainStageVerifying => 'Testing';

  @override
  String get mainStayCurious => 'STAY CURIOUS. STAY YOU.';

  @override
  String mainSteps(int done, int total) {
    return '$done/$total steps';
  }

  @override
  String get mainStorageUnavailable =>
      'Local storage is unavailable. Changes will last for this session only.';

  @override
  String get mainStorageUnreadable =>
      'Saved content could not be read. The original data is preserved and will not be overwritten.';

  @override
  String get mainTenMinutesAgo => '10 minutes ago';

  @override
  String get mainTextureCanvas => 'Texture';

  @override
  String get mainTextureDetail =>
      'Fine paper-like texture adds a tactile feel.';

  @override
  String get mainThemeCompass => 'Theme color wheel';

  @override
  String get mainThemeGrayscale => 'Theme grayscale';

  @override
  String get mainThemeTone => 'Theme colors';

  @override
  String get mainThreeHoursAgo => '3 hours ago';

  @override
  String get mainTintOpacity => 'Tint opacity';

  @override
  String get mainToProject => 'Move to projects';

  @override
  String get mainTodosPrompt => 'Next steps (one per line, optional)';

  @override
  String get mainTransparencyUnavailable =>
      'System transparency could not be enabled. You can use the default background instead.';

  @override
  String get mainTransparentCanvas => 'Transparent';

  @override
  String get mainTransparentDetail =>
      'See the space behind the window; on the web, see the host background.';

  @override
  String get mainUndo => 'Undo';

  @override
  String mainUnfavoriteTooltip(String title) {
    return 'Unfavorite $title';
  }

  @override
  String get mainUnsortedCount => 'To organize';

  @override
  String get mainUnverifiedCount => 'To test';

  @override
  String get mainView => 'View';

  @override
  String get mainViewAll => 'Show all';

  @override
  String get mainWarmSand => 'Warm sand';

  @override
  String get mainWhiteTheme => 'White';

  @override
  String get mainWindowRadius => 'Window corners';

  @override
  String get mainWindowRadiusDetail =>
      'Adjust the window edge separately; it becomes square when maximized';

  @override
  String get mainWindowsFrostOnly =>
      'Desktop blur is available on Windows only';

  @override
  String get mainWorkbench => 'Workspace';

  @override
  String get mainWorkbenchPlugin => 'Workspace plugin';

  @override
  String get mainWriteHypothesis =>
      'Open the record and write down what you want to test.';

  @override
  String get mainYesterday => 'Yesterday';

  @override
  String get pluginsApprovalUnknown =>
      'Activation or permission changes could not be confirmed';

  @override
  String get pluginsApproveEnable => 'Approve and enable';

  @override
  String get pluginsApproveWorkbench => 'Allow reading and editing, and enable';

  @override
  String get pluginsAttachment => 'Read attachments';

  @override
  String get pluginsBackingUp => 'Backing up…';

  @override
  String get pluginsBackupLibrary => 'Back up library';

  @override
  String get pluginsBackupLibraryType => 'Library backup';

  @override
  String get pluginsBackupProtection => 'Back up protection file';

  @override
  String get pluginsBackupUnknown =>
      'Backup is not yet confirmed. Keep any file that was created and check the destination.';

  @override
  String pluginsBinaryPreview(String hex) {
    return 'Binary content: $hex';
  }

  @override
  String get pluginsBuiltin => 'Built-in workbench';

  @override
  String pluginsBuiltinCount(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count characters',
      one: '1 character',
    );
    return '$_temp0 · This session only; not saved as a card';
  }

  @override
  String get pluginsBuiltinEmpty => 'Enter text to preview uppercase';

  @override
  String get pluginsBuiltinHeading => 'Text tools';

  @override
  String get pluginsBuiltinInput => 'Enter text';

  @override
  String get pluginsCancel => 'Cancel';

  @override
  String get pluginsChoosePackage => 'Choose plugin file';

  @override
  String get pluginsChooseSmallFile => 'Choose a small file';

  @override
  String get pluginsCloseTextTool => 'Hide text tool';

  @override
  String get pluginsCloseUnknown =>
      'Closing the plugin view could not be confirmed';

  @override
  String get pluginsCloseView => 'Close view';

  @override
  String get pluginsConnectionLost =>
      'Connection interrupted. Reopen this plugin view.';

  @override
  String get pluginsContentPermissions => 'Content permissions';

  @override
  String get pluginsCreate => 'Create content';

  @override
  String get pluginsCredentialCancel => 'Close form';

  @override
  String get pluginsCredentialCreateTitle => 'New credential';

  @override
  String pluginsCredentialDays(int days) {
    String _temp0 = intl.Intl.pluralLogic(
      days,
      locale: localeName,
      other: '$days days',
      one: '1 day',
    );
    return '$_temp0';
  }

  @override
  String get pluginsCredentialDetails =>
      'Store credentials securely for approved API connections. Saving a credential does not approve a server or enable a plugin. Saved secrets cannot be viewed.';

  @override
  String get pluginsCredentialDisable => 'Disable';

  @override
  String get pluginsCredentialDisabled => 'Disabled';

  @override
  String get pluginsCredentialDisabledDone => 'Credential disabled.';

  @override
  String get pluginsCredentialEmpty => 'No saved credentials';

  @override
  String get pluginsCredentialExpired => 'Expired';

  @override
  String pluginsCredentialExpires(String date) {
    return 'Expires: $date';
  }

  @override
  String get pluginsCredentialHeader => 'Header name';

  @override
  String get pluginsCredentialInvalid =>
      'Check the header name and enter a new secret value. The secret field has been cleared.';

  @override
  String get pluginsCredentialLifetime => 'Valid for';

  @override
  String get pluginsCredentialLoadFailed =>
      'Credentials could not be read consistently. Refresh status to try again.';

  @override
  String get pluginsCredentialNew => 'Add credential';

  @override
  String get pluginsCredentialReading => 'Reading credentials…';

  @override
  String pluginsCredentialReference(String reference) {
    return 'Credential $reference';
  }

  @override
  String get pluginsCredentialRefresh => 'Refresh status';

  @override
  String get pluginsCredentialReplace => 'Replace secret';

  @override
  String pluginsCredentialReplaceTitle(String reference) {
    return 'Replace credential $reference';
  }

  @override
  String get pluginsCredentialSave => 'Save credential';

  @override
  String get pluginsCredentialSaved =>
      'Credential saved. API connections still need separate approval.';

  @override
  String get pluginsCredentialSecret => 'New secret value';

  @override
  String get pluginsCredentialStored => 'Saved';

  @override
  String get pluginsCredentialTitle => 'API credentials';

  @override
  String get pluginsCredentialUnknown =>
      'The result could not be confirmed. The secret field has been cleared. Refresh status before making another change.';

  @override
  String pluginsDeclared(String permissions) {
    return 'Declared permissions: $permissions';
  }

  @override
  String get pluginsDependenciesNotice =>
      'Dependencies must be configured in the host. This page does not approve them.';

  @override
  String get pluginsDisable => 'Disable';

  @override
  String get pluginsDisableWorkbench => 'Disable workbench plugin';

  @override
  String get pluginsDisabled => 'Disabled';

  @override
  String get pluginsDisabledDetails =>
      'Not enabled. Allow reading and editing workbench content to use the editor and tools.';

  @override
  String get pluginsEdit => 'Edit content';

  @override
  String get pluginsEmptyLibrary => 'No third-party plugins imported yet.';

  @override
  String get pluginsEmptyResult => '(empty result)';

  @override
  String get pluginsEnabled => 'Enabled';

  @override
  String get pluginsEnabledDetails =>
      'Enabled. This plugin can read and edit workbench content. Disabling it preserves your data.';

  @override
  String get pluginsEndpointAdvanced =>
      'Policy limits (bytes unless stated otherwise)';

  @override
  String get pluginsEndpointCertificate => 'Choose DER trust root';

  @override
  String get pluginsEndpointCertificateDetails =>
      'Optional trust root for HTTPS: one binary DER certificate (.der or .cer), up to 32 KiB. PEM and certificate bundles are not accepted. Remove the trust root before switching to HTTP.';

  @override
  String get pluginsEndpointCertificateInvalid =>
      'Choose one valid binary DER certificate (.der or .cer) no larger than 32 KiB.';

  @override
  String pluginsEndpointCertificateSelected(int bytes) {
    return 'DER trust root selected ($bytes bytes)';
  }

  @override
  String get pluginsEndpointConcurrency => 'Concurrent requests (1–128)';

  @override
  String get pluginsEndpointCreateTitle => 'New endpoint approval';

  @override
  String get pluginsEndpointCredential => 'Credential reference';

  @override
  String get pluginsEndpointCredentialLifetime =>
      'The selected credential must remain valid for the full endpoint lifetime. Its expiration will not be extended.';

  @override
  String get pluginsEndpointCredentialUnavailable =>
      'Credentials require the package’s declared and approved credential-use permission and a valid saved reference.';

  @override
  String get pluginsEndpointCredentialsFailed =>
      'Credential references could not be read. Refresh status before choosing a credential.';

  @override
  String get pluginsEndpointDetails =>
      'Save a server policy for a specific package and digest. Saving does not connect to the network, enable a plugin, or make network tasks immediately available.';

  @override
  String get pluginsEndpointDigest => 'Package digest';

  @override
  String get pluginsEndpointDisabledDone => 'Endpoint approval disabled.';

  @override
  String get pluginsEndpointEmpty => 'No saved endpoint approvals';

  @override
  String get pluginsEndpointFrameBytes => 'Frame budget (1–131072 bytes)';

  @override
  String get pluginsEndpointHeaderBytes => 'Maximum header bytes (1–16384)';

  @override
  String get pluginsEndpointInvalid =>
      'Check the package, origin, methods, 1–30 day lifetime, credential permission, certificate, and policy limits.';

  @override
  String get pluginsEndpointLifetime => 'Valid for (1–30 days)';

  @override
  String get pluginsEndpointLoadFailed =>
      'Endpoint approvals could not be read consistently. Refresh status to try again.';

  @override
  String get pluginsEndpointLocalHttp => 'Local HTTP';

  @override
  String get pluginsEndpointLocalHttps => 'Local HTTPS';

  @override
  String get pluginsEndpointMethods => 'Allowed request methods';

  @override
  String get pluginsEndpointNew => 'Add endpoint';

  @override
  String get pluginsEndpointNoCredential => 'No credential';

  @override
  String get pluginsEndpointOrigin =>
      'Origin only, for example https://api.example.com';

  @override
  String get pluginsEndpointPackage => 'Package';

  @override
  String get pluginsEndpointPackageUnavailable =>
      'This package is unavailable or has no approved HTTP permission. Existing approvals can still be disabled.';

  @override
  String get pluginsEndpointProfile => 'Connection profile';

  @override
  String get pluginsEndpointPublicHttps => 'Public HTTPS';

  @override
  String get pluginsEndpointRemoveCertificate => 'Remove trust root';

  @override
  String get pluginsEndpointReplace => 'Replace approval';

  @override
  String get pluginsEndpointReplaceTitle =>
      'Replace endpoint approval using the current package digest';

  @override
  String get pluginsEndpointRequestBytes => 'Maximum request bytes (1–65536)';

  @override
  String get pluginsEndpointResponseBytes => 'Maximum response bytes (1–65536)';

  @override
  String get pluginsEndpointSave => 'Save endpoint approval';

  @override
  String get pluginsEndpointSaved =>
      'Endpoint approval saved. No network connection was made.';

  @override
  String get pluginsEndpointTimeout => 'Timeout (1–30000 milliseconds)';

  @override
  String get pluginsEndpointTitle => 'API endpoint approvals';

  @override
  String get pluginsEndpointUnknown =>
      'The result could not be confirmed. Refresh status before making another change. The request will not be sent again automatically.';

  @override
  String get pluginsEndpointWorking => 'Updating endpoint status…';

  @override
  String get pluginsExistingVersion =>
      'This version is already installed. Its enabled state is unchanged.';

  @override
  String pluginsFileLimit(int limit) {
    return 'The file is too large. Choose a file no larger than $limit bytes.';
  }

  @override
  String get pluginsHttpTaskAbandon => 'End observation of this attempt';

  @override
  String get pluginsHttpTaskAbandonDetails =>
      'Only after fresh status confirms no active task and the original library is available, you may end this observation. This does not prove that no remote effects occurred. The identity and uncertainty remain in history; a new request requires another explicit submission.';

  @override
  String get pluginsHttpTaskAbsent => 'No result delivery';

  @override
  String get pluginsHttpTaskAccepted => 'Accepted';

  @override
  String get pluginsHttpTaskAcknowledge => 'Acknowledge finished task';

  @override
  String get pluginsHttpTaskArchivedUnknown =>
      'Observation ended by the user. The previous remote effects remain unconfirmed; this attempt was not replayed.';

  @override
  String get pluginsHttpTaskBase64 => 'Base64';

  @override
  String get pluginsHttpTaskBody => 'Request body';

  @override
  String get pluginsHttpTaskBodyFormat => 'Request body encoding';

  @override
  String get pluginsHttpTaskBusy => 'Busy';

  @override
  String get pluginsHttpTaskCancel => 'Request cancellation';

  @override
  String get pluginsHttpTaskCancelled =>
      'Cancellation observed; remote effects may still have occurred';

  @override
  String get pluginsHttpTaskCatalogUnavailable =>
      'Plugin catalog status is unavailable. Refresh the plugin library before a new submission; existing task controls remain available.';

  @override
  String get pluginsHttpTaskClosed => 'Closed';

  @override
  String get pluginsHttpTaskCompleted => 'Completed';

  @override
  String get pluginsHttpTaskConflict => 'Conflict';

  @override
  String get pluginsHttpTaskConsumed => 'Result consumed';

  @override
  String get pluginsHttpTaskControlUnknown =>
      'The control result could not be confirmed. Refresh task status before deciding the next action.';

  @override
  String pluginsHttpTaskCounters(String bytes, String calls) {
    return 'IO calls: $calls; charged bytes: $bytes';
  }

  @override
  String get pluginsHttpTaskDeadline => 'Deadline exceeded';

  @override
  String get pluginsHttpTaskDenied => 'Denied';

  @override
  String get pluginsHttpTaskDetails =>
      'Run one explicit request using an approved endpoint and an enabled plugin with the experimental HTTP forward handler. Task status remains available while the content library is busy.';

  @override
  String get pluginsHttpTaskDisconnect => 'Connection cleanup failed';

  @override
  String get pluginsHttpTaskEndpoint => 'Approved endpoint';

  @override
  String get pluginsHttpTaskEndpointsFailed =>
      'Endpoints could not be read consistently, or the content library is busy. Task controls remain available. Refresh endpoints when the library returns.';

  @override
  String get pluginsHttpTaskEvidenceUnavailable =>
      'Outcome evidence unavailable';

  @override
  String pluginsHttpTaskExecution(int code, String fault) {
    return 'Guest execution: $fault; exit code: $code';
  }

  @override
  String pluginsHttpTaskExit(
    String disconnect,
    String execution,
    String maintenance,
  ) {
    return 'Worker exit — execution: $execution; disconnect: $disconnect; maintenance: $maintenance';
  }

  @override
  String get pluginsHttpTaskExplicit =>
      'Submit sends one real request. Each click creates a new identity. Unknown submissions and result reads are never replayed automatically. Cancellation does not prove the remote operation was undone.';

  @override
  String get pluginsHttpTaskFailed => 'Failed';

  @override
  String get pluginsHttpTaskHeaders =>
      'Ordinary request headers, one Name: value per line';

  @override
  String get pluginsHttpTaskHeadersHint =>
      'Repeated headers stay separate. Credential and connection headers are supplied only by the runtime.';

  @override
  String get pluginsHttpTaskHistory => 'Previous task observations (up to 5)';

  @override
  String pluginsHttpTaskHttpResult(int code, String status) {
    return 'HTTP outcome: $status; remote status: $code';
  }

  @override
  String get pluginsHttpTaskInactive => 'Connection is inactive';

  @override
  String get pluginsHttpTaskInvalid =>
      'Check the selected endpoint, method, relative target, ordinary headers, body encoding and timeout against the approval limits.';

  @override
  String get pluginsHttpTaskInvalidOptions => 'Invalid options';

  @override
  String pluginsHttpTaskKey(String identity) {
    return 'Task identity: $identity';
  }

  @override
  String get pluginsHttpTaskLimit => 'Quota or limit reached';

  @override
  String get pluginsHttpTaskLoadingEndpoints => 'Reading approved endpoints…';

  @override
  String get pluginsHttpTaskLocal =>
      'Content library available; no active task';

  @override
  String get pluginsHttpTaskModule => 'Invalid guest module';

  @override
  String get pluginsHttpTaskNew => 'Prepare a new request';

  @override
  String get pluginsHttpTaskNoEndpoints =>
      'No current endpoint matches an enabled, approved HTTP forward plugin.';

  @override
  String get pluginsHttpTaskNotFound => 'Not found';

  @override
  String get pluginsHttpTaskOk => 'OK';

  @override
  String get pluginsHttpTaskOutcomeUnknown =>
      'Remote outcome unknown; do not assume rollback or resend';

  @override
  String get pluginsHttpTaskPackageChanged => 'Package binding changed';

  @override
  String get pluginsHttpTaskPending => 'Result pending';

  @override
  String get pluginsHttpTaskPoll => 'Check task';

  @override
  String get pluginsHttpTaskProtocol => 'Task protocol error';

  @override
  String get pluginsHttpTaskRead => 'Read result once';

  @override
  String get pluginsHttpTaskReadBound => 'Read limit exceeded';

  @override
  String get pluginsHttpTaskReadPending =>
      'No result was returned. Check status before explicitly reading again.';

  @override
  String get pluginsHttpTaskReadUnknown =>
      'The result read could not be confirmed and may already have consumed the result. It will not be read again. Status and exit can still be checked.';

  @override
  String get pluginsHttpTaskReady =>
      'Result ready: read explicitly. Ready does not mean the worker has exited.';

  @override
  String get pluginsHttpTaskReclaimed =>
      'Worker exited; original content library returned';

  @override
  String get pluginsHttpTaskRecoveryRequired =>
      'Worker exited; cleanup or maintenance requires repair';

  @override
  String get pluginsHttpTaskRefresh => 'Refresh task status';

  @override
  String get pluginsHttpTaskRefreshEndpoints => 'Refresh approved endpoints';

  @override
  String get pluginsHttpTaskRemoteError =>
      'The remote server returned a 4xx/5xx response. This is a completed HTTP exchange, separate from guest execution errors.';

  @override
  String get pluginsHttpTaskRepair => 'Repair cleanup';

  @override
  String get pluginsHttpTaskResponseBase64 => 'Response body: exact Base64';

  @override
  String get pluginsHttpTaskResponseHeaders =>
      'Response headers (duplicates preserved; binary values use Base64)';

  @override
  String get pluginsHttpTaskResponseText => 'Response body: plain text preview';

  @override
  String get pluginsHttpTaskResultUnavailable => 'Result delivery unavailable';

  @override
  String get pluginsHttpTaskRevoked => 'Approval revoked';

  @override
  String get pluginsHttpTaskRunning =>
      'Running; content library is owned by the worker';

  @override
  String get pluginsHttpTaskSpawn => 'Worker could not start';

  @override
  String get pluginsHttpTaskStart => 'Submit new request';

  @override
  String get pluginsHttpTaskStartUnknown =>
      'The submission result is unknown. Its identity is retained. Query status to find the same task; the request will not be sent again.';

  @override
  String get pluginsHttpTaskStatusFailed =>
      'Task status could not be confirmed. Refresh status; no request has been replayed.';

  @override
  String get pluginsHttpTaskStopping =>
      'Stopping; waiting for actual worker exit';

  @override
  String pluginsHttpTaskSubmission(String identity) {
    return 'Submission identity: $identity';
  }

  @override
  String get pluginsHttpTaskTarget =>
      'Relative target, for example /v1/items?limit=10';

  @override
  String get pluginsHttpTaskText => 'UTF-8 text';

  @override
  String get pluginsHttpTaskTimeout =>
      'Timeout in milliseconds (1–30000, within endpoint approval)';

  @override
  String get pluginsHttpTaskTitle => 'HTTP tasks';

  @override
  String get pluginsHttpTaskTrap => 'Guest execution trapped';

  @override
  String get pluginsHttpTaskUnavailable =>
      'Original content library unavailable; recovery needs attention';

  @override
  String get pluginsHttpTaskUnsupported => 'Unsupported operation';

  @override
  String get pluginsHttpTaskWorking => 'Waiting for the task control response…';

  @override
  String get pluginsImport => 'Import';

  @override
  String get pluginsImportDetails =>
      'You choose whether to enable a plugin after importing it. Disabling or uninstalling preserves your content.';

  @override
  String pluginsImportPreview(String name) {
    return 'Import preview: $name';
  }

  @override
  String get pluginsImportUnknown => 'Import could not be confirmed';

  @override
  String get pluginsImportedDisabled =>
      'Imported and disabled. Choose which permissions to allow.';

  @override
  String get pluginsInputFailed => 'The input file could not be read';

  @override
  String get pluginsInputTooLong =>
      'This view has reached its input limit. Shorten the text and try again.';

  @override
  String get pluginsInspectFailed => 'The plugin preview could not be loaded';

  @override
  String get pluginsInspectedOnly =>
      'The file has only been inspected. Enable the plugin separately after importing it.';

  @override
  String get pluginsInsufficientApproval =>
      'The plugin is enabled but needs content permission. The workbench stays read-only. Disable it to review permissions again.';

  @override
  String pluginsIoApproved(String permissions) {
    return 'Approved: $permissions';
  }

  @override
  String get pluginsIoCredentialUse => 'Use approved credentials';

  @override
  String pluginsIoDeclared(String permissions) {
    return 'Requested network and file permissions: $permissions';
  }

  @override
  String get pluginsIoFileCreate => 'Create files';

  @override
  String get pluginsIoFileDelete => 'Delete files';

  @override
  String get pluginsIoFileList => 'Browse approved folders';

  @override
  String get pluginsIoFileRead => 'Read approved files';

  @override
  String get pluginsIoFileReplace => 'Replace files';

  @override
  String get pluginsIoHttpListen => 'Listen for network connections';

  @override
  String get pluginsIoHttpPublish => 'Provide an API service';

  @override
  String get pluginsIoHttpRequest => 'Call network APIs';

  @override
  String get pluginsIoNoneApproved => 'No network or file permissions approved';

  @override
  String get pluginsIoRevoke => 'Revoke all network and file permissions';

  @override
  String get pluginsIoSave => 'Save network and file permissions';

  @override
  String get pluginsIoScopeNotice =>
      'These choices save permission categories only. Server addresses, file access and credentials need separate approval; unavailable features remain unavailable. Reopen the plugin form after changes.';

  @override
  String get pluginsIoTitle => 'Network and file permissions';

  @override
  String get pluginsIoWebSocketConnect => 'Connect to WebSocket services';

  @override
  String get pluginsListUnknown => 'The plugin list could not be confirmed';

  @override
  String get pluginsManageAbove =>
      'Manage this plugin using the workbench plugin controls above.';

  @override
  String get pluginsManagementUnavailable =>
      'Plugin management is unavailable. Existing content remains readable.';

  @override
  String get pluginsNoPermissions => 'No content permissions declared.';

  @override
  String get pluginsOpenTextTool => 'Open text tool';

  @override
  String get pluginsOpenView => 'Open view';

  @override
  String get pluginsOpeningView => 'Opening plugin view…';

  @override
  String get pluginsOperation => 'Query operation results';

  @override
  String pluginsOtherCapability(String name) {
    return 'Other declared permission: $name';
  }

  @override
  String get pluginsPackageFile => 'Morrow plugin';

  @override
  String get pluginsPreviewOnly =>
      'Preview only. Results are not automatically saved to existing content.';

  @override
  String get pluginsPreviewTruncated =>
      '…showing the first 4,096 characters only';

  @override
  String get pluginsProtection => 'Content protection';

  @override
  String get pluginsProtectionDetails =>
      'Back up the original protection file for recovery with this system account. It contains no cards or attachments.';

  @override
  String get pluginsProtectionFileType => 'Library protection file';

  @override
  String get pluginsProtectionSaved =>
      'Protection file backed up. Choose it for recovery if startup fails.';

  @override
  String get pluginsRead => 'Read content';

  @override
  String get pluginsReadingState => 'Loading plugin status…';

  @override
  String pluginsRefreshFailed(String reason) {
    return '$reason. The list could not be refreshed. Select “Refresh list” to retry reading it.';
  }

  @override
  String get pluginsRefreshList => 'Refresh list';

  @override
  String get pluginsRefreshState => 'Refresh status';

  @override
  String get pluginsRename => 'Rename';

  @override
  String pluginsResultBytes(int count, String preview) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count bytes',
      one: '1 byte',
    );
    return '$_temp0\n$preview';
  }

  @override
  String get pluginsSavePermissions => 'Save permissions';

  @override
  String pluginsSelectedFile(String name) {
    return 'Selected file: $name';
  }

  @override
  String get pluginsServiceAcknowledgeUncertain =>
      'I reviewed the refreshed records';

  @override
  String get pluginsServiceAddScope => 'Add content scope';

  @override
  String get pluginsServiceAttachmentId => 'Exact attachment identity';

  @override
  String get pluginsServiceAuthenticationUnavailable =>
      'This selected authentication is missing, disabled, expired, or belongs to another principal. Its draft scopes are retained; select a valid replacement or explicitly remove it.';

  @override
  String get pluginsServiceAuthorities =>
      'Authentication and publication records';

  @override
  String get pluginsServiceCardId => 'Exact card identity';

  @override
  String get pluginsServiceCatalogChanged =>
      'The package catalog changed or is unavailable. Your draft is preserved. Explicitly refresh the selection before saving.';

  @override
  String get pluginsServiceClearToken => 'Clear token';

  @override
  String get pluginsServiceCloseEditor => 'Close editor';

  @override
  String get pluginsServiceConfigDigest => 'Configuration digest';

  @override
  String get pluginsServiceConfiguration => 'Saved configuration';

  @override
  String get pluginsServiceConfigurations => 'Saved configurations';

  @override
  String get pluginsServiceCopyClear => 'Copy and clear token';

  @override
  String get pluginsServiceCreated => 'Created (UTC)';

  @override
  String get pluginsServiceDays => 'Requested lifetime (1–30 days)';

  @override
  String get pluginsServiceDigestFixed =>
      'Editing keeps the original package digest. A matching package must be selected; this does not enable it.';

  @override
  String get pluginsServiceDisable => 'Disable';

  @override
  String get pluginsServiceDisabled => 'Disabled';

  @override
  String get pluginsServiceEditConfig => 'Edit configuration';

  @override
  String get pluginsServiceEditPublication => 'Edit publication';

  @override
  String get pluginsServiceExpired => 'Expired or not yet valid';

  @override
  String get pluginsServiceExpires => 'Actual expiry (UTC)';

  @override
  String get pluginsServiceHandler => 'Declared service handler';

  @override
  String get pluginsServiceIdentity => 'Service identity';

  @override
  String get pluginsServiceInvalid =>
      'Check the fields, selected approvals, and current package before saving.';

  @override
  String get pluginsServiceIssue => 'Issue token';

  @override
  String get pluginsServiceIssuedToken => 'One-time bearer token';

  @override
  String get pluginsServiceListenAddress => 'Numeric listen address and port';

  @override
  String get pluginsServiceLoadFailed =>
      'Records could not be refreshed. Refresh again before making changes.';

  @override
  String get pluginsServiceManagementOnly =>
      'Manage saved configurations and approvals here. Saving does not start a listener, run a package, or activate a service.';

  @override
  String get pluginsServiceMethod => 'HTTP method';

  @override
  String get pluginsServiceNewAuthentication => 'New authentication';

  @override
  String get pluginsServiceNewConfig => 'New configuration';

  @override
  String get pluginsServiceNo => 'No';

  @override
  String get pluginsServiceNoAuthentication =>
      'Create a currently valid authentication record first.';

  @override
  String get pluginsServiceNoAuthorities =>
      'No authentication or publication records.';

  @override
  String get pluginsServiceNoConfigurations => 'No service configurations.';

  @override
  String get pluginsServicePackage => 'Declared and approved package';

  @override
  String get pluginsServicePackageDigest => 'Package digest';

  @override
  String get pluginsServicePackageUnavailable =>
      'The matching package or its listen/publish approvals are unavailable. Historical records remain readable and can be disabled.';

  @override
  String get pluginsServicePath => 'Exact request path';

  @override
  String get pluginsServicePolicyChanged =>
      'The selected original record changed or is no longer usable. Refresh the selection, or reopen the editor from the current record. Your draft remains here.';

  @override
  String get pluginsServicePrincipalId => 'Principal identity';

  @override
  String get pluginsServicePrincipals =>
      'Authorized principals and content scopes';

  @override
  String get pluginsServicePublicationEditor => 'Publication approval';

  @override
  String get pluginsServicePublicationHelp =>
      'Approval is bound to this exact configuration, revision and reference. Its actual expiry is limited by every selected authentication record and may be shorter than requested. Saving does not start listening.';

  @override
  String get pluginsServicePublicationMismatch =>
      'This publication no longer matches the current configuration. Review and explicitly save a replacement approval.';

  @override
  String get pluginsServiceQueryPath => 'Separate result query path (optional)';

  @override
  String get pluginsServiceReference => 'Approval reference';

  @override
  String get pluginsServiceRefresh => 'Refresh records';

  @override
  String get pluginsServiceRefreshSelection => 'Refresh this selection';

  @override
  String get pluginsServiceRemovePrincipal => 'Remove principal';

  @override
  String get pluginsServiceRemoveScope => 'Remove scope';

  @override
  String get pluginsServiceRetention =>
      'Request history retention (milliseconds, up to 30 days)';

  @override
  String get pluginsServiceRevision => 'Revision';

  @override
  String get pluginsServiceRotate => 'Rotate token';

  @override
  String get pluginsServiceRotateAuthentication => 'Rotate authentication';

  @override
  String get pluginsServiceRunAbandon => 'Keep record and end this attempt';

  @override
  String get pluginsServiceRunAdvanced => 'Request and worker limits';

  @override
  String get pluginsServiceRunAttempt => 'Unresolved start attempt';

  @override
  String get pluginsServiceRunBoundsHint =>
      'These limits must also fit the plugin declaration and saved approvals. Reserved work consumes the cumulative budget even when cancelled. Expiry stops this run; renewal is not automatic.';

  @override
  String get pluginsServiceRunBytes =>
      'Run byte budget (bytes, up to 67,108,864)';

  @override
  String get pluginsServiceRunCalls => 'Calls per job (up to 1,024)';

  @override
  String get pluginsServiceRunCancelled => 'Cancelled';

  @override
  String get pluginsServiceRunClosed => 'Closed';

  @override
  String get pluginsServiceRunConcurrent => 'Concurrent jobs (up to 128)';

  @override
  String get pluginsServiceRunControlUnknown =>
      'The control result is unknown. Refresh the original service state before another operation.';

  @override
  String get pluginsServiceRunDenied => 'Denied';

  @override
  String get pluginsServiceRunExited => 'Service exited';

  @override
  String get pluginsServiceRunHeaderBytes =>
      'Maximum header size (bytes, up to 65,536)';

  @override
  String get pluginsServiceRunHint =>
      'Choose an approved publication and finite run limits, then explicitly start the service. After stopping, wait for the original workbench owner to return before acknowledging the result.';

  @override
  String pluginsServiceRunHostFailure(String detail) {
    return 'Host diagnostic: $detail';
  }

  @override
  String get pluginsServiceRunHttpPanel =>
      'An API service currently owns this task. Use the service run panel above to stop it or acknowledge its exit. Your HTTP request draft is retained.';

  @override
  String get pluginsServiceRunIdentityChanged =>
      'A different task owns the content. This panel will not control it using the previous service identity.';

  @override
  String get pluginsServiceRunInvalid =>
      'Check the selected service and numeric limits. No new run was submitted.';

  @override
  String get pluginsServiceRunInvalidOutcome => 'Invalid configuration';

  @override
  String get pluginsServiceRunJobBytes => 'Bytes per job (up to 16,777,216)';

  @override
  String get pluginsServiceRunJobs =>
      'Total task reservations (up to 1,000,000)';

  @override
  String get pluginsServiceRunLastObservation =>
      'Showing the last observation; current state is unverified.';

  @override
  String get pluginsServiceRunLifetime => 'Run duration (ms, up to 3,600,000)';

  @override
  String get pluginsServiceRunLimit => 'Limit reached';

  @override
  String get pluginsServiceRunLocal => 'Content is available locally';

  @override
  String pluginsServiceRunNetwork(
    String bind,
    String listener,
    String supervision,
  ) {
    return 'Bind: $bind; listener: $listener; supervision: $supervision';
  }

  @override
  String get pluginsServiceRunNextSettings =>
      'Settings for the next explicit run';

  @override
  String get pluginsServiceRunNoSelection =>
      'No approved publication is available. Check the plugin, configuration and authentication state.';

  @override
  String get pluginsServiceRunOutboundAttempt =>
      'Endpoints bound to this start attempt';

  @override
  String get pluginsServiceRunOutboundClear => 'Clear endpoint selection';

  @override
  String get pluginsServiceRunOutboundFailed =>
      'Could not verify the endpoint list. Refresh before using selected endpoints.';

  @override
  String get pluginsServiceRunOutboundHint =>
      'Outgoing APIs (optional, up to 8). Only endpoints approved for this package are listed. Leaving all unchecked blocks outgoing calls.';

  @override
  String get pluginsServiceRunOutboundStale =>
      'A selected endpoint changed or is no longer available. Select its current version explicitly, or clear the selection.';

  @override
  String get pluginsServiceRunOwned =>
      'Content is managed by the running service';

  @override
  String get pluginsServiceRunPending => 'Pending';

  @override
  String get pluginsServiceRunReclaimed =>
      'Content ownership reclaimed; acknowledgement required';

  @override
  String get pluginsServiceRunReclaiming =>
      'Waiting to reclaim content ownership';

  @override
  String get pluginsServiceRunRecovery => 'Cleanup needs repair';

  @override
  String get pluginsServiceRunRequestBytes => 'Maximum request size (bytes)';

  @override
  String get pluginsServiceRunResponseBytes => 'Maximum response size (bytes)';

  @override
  String get pluginsServiceRunRunning => 'Service running';

  @override
  String get pluginsServiceRunSelection => 'Approved service publication';

  @override
  String get pluginsServiceRunStale =>
      'The selected package, configuration or approval changed. Refresh records and select it again before starting.';

  @override
  String get pluginsServiceRunStart => 'Start finite service';

  @override
  String get pluginsServiceRunStartRejected =>
      'The start response reported an error. The current task has been checked; review the reason and any cleanup state before proceeding.';

  @override
  String get pluginsServiceRunStartUnknown =>
      'The start result is unknown. This attempt identity is retained; refresh to locate it. It will not be started again automatically.';

  @override
  String get pluginsServiceRunStarting => 'Starting service';

  @override
  String get pluginsServiceRunStatusFailed =>
      'Could not verify the current service state. Refresh before taking further action.';

  @override
  String get pluginsServiceRunStop => 'Stop service';

  @override
  String get pluginsServiceRunStopping =>
      'Stopping; waiting for listener and worker exit';

  @override
  String get pluginsServiceRunSucceeded => 'Succeeded';

  @override
  String get pluginsServiceRunTask => 'Current task identity';

  @override
  String get pluginsServiceRunTimeout => 'Job timeout (ms, up to 30,000)';

  @override
  String get pluginsServiceRunTimeoutOutcome => 'Timed out';

  @override
  String get pluginsServiceRunTitle => 'Run API service';

  @override
  String get pluginsServiceRunTotalBytes =>
      'Worker byte budget (up to 67,108,864)';

  @override
  String get pluginsServiceRunTransport => 'Transport failure';

  @override
  String get pluginsServiceRunUnavailable => 'Content storage unavailable';

  @override
  String get pluginsServiceSaveConfig => 'Save configuration';

  @override
  String get pluginsServiceSavePublication => 'Save publication approval';

  @override
  String get pluginsServiceSaved =>
      'Saved. Review the returned revision and actual expiry below.';

  @override
  String get pluginsServiceScopeAttachment => 'Read attachment';

  @override
  String get pluginsServiceScopeCreate => 'Create content';

  @override
  String get pluginsServiceScopeEdit => 'Edit content';

  @override
  String get pluginsServiceScopeKind => 'Allowed content operation';

  @override
  String get pluginsServiceScopeQuery => 'Query operation';

  @override
  String get pluginsServiceScopeRead => 'Read content';

  @override
  String get pluginsServiceScopeRename => 'Rename card';

  @override
  String get pluginsServiceScopeSummary => 'Read summary';

  @override
  String get pluginsServiceScopesHelp =>
      'Select authentication explicitly. Add each allowed operation and exact object identity below. Removing a scope or principal requires its own button; existing scopes are preserved while editing.';

  @override
  String get pluginsServiceTitle => 'Service configuration';

  @override
  String get pluginsServiceTls => 'Require TLS';

  @override
  String get pluginsServiceTlsAttempt =>
      'Certificate PEM digest bound to this start attempt';

  @override
  String get pluginsServiceTlsCertificate => 'Choose certificate chain';

  @override
  String get pluginsServiceTlsChecked =>
      'Certificate and key pairing checked. Certificate PEM SHA-256 is shown below. Clients must still verify the hostname, validity and trust chain.';

  @override
  String get pluginsServiceTlsChecking => 'Processing certificate selection…';

  @override
  String get pluginsServiceTlsFailed =>
      'Certificate check failed. Check the PEM files, key pairing and local paths before trying again.';

  @override
  String get pluginsServiceTlsHelp =>
      'Non-loopback addresses require TLS. This saves the requirement only; no listener or TLS identity is created here.';

  @override
  String get pluginsServiceTlsHint =>
      'Select a PEM certificate chain and private key, then check them. Files are checked again at start; an active certificate does not rotate automatically.';

  @override
  String get pluginsServiceTlsInspect => 'Check certificate';

  @override
  String get pluginsServiceTlsOutsideValidity =>
      'The certificate chain is not yet valid or has expired. Check or replace the certificate and inspect it again before starting.';

  @override
  String get pluginsServiceTlsPrivateKey => 'Choose private key';

  @override
  String get pluginsServiceTlsRecheck =>
      'Check the certificate again before starting. A change in the clock does not restore the previous selection.';

  @override
  String get pluginsServiceTlsUnavailable =>
      'This backend does not support local TLS certificate selection.';

  @override
  String pluginsServiceTlsValidity(String end, String start) {
    return 'Shared certificate-chain validity (UTC): $start through $end. The service stops after expiry.';
  }

  @override
  String get pluginsServiceTokenDiscarded =>
      'The one-time token was cleared while this panel was closed. Issue a new token explicitly if needed.';

  @override
  String get pluginsServiceTokenHelp =>
      'This token is shown only now. Copy it explicitly if needed. Clearing or closing this panel removes it from the session; it cannot be retrieved from the list. Rotation replaces the previous token.';

  @override
  String get pluginsServiceUncertainHelp =>
      'Refresh and inspect the original records first. Acknowledging this notice only allows another explicit action; it does not prove that the previous change failed or replay it.';

  @override
  String get pluginsServiceUnsupported => 'Unsupported historical value';

  @override
  String get pluginsServiceWorking => 'Working…';

  @override
  String get pluginsServiceWriteUnknown =>
      'The result of the last change is unknown. It has not been sent again.';

  @override
  String get pluginsServiceYes => 'Yes';

  @override
  String get pluginsSettingsUnknown =>
      'The setting has not been confirmed. Refresh the status before choosing again.';

  @override
  String get pluginsSnapshotDetails =>
      'A library backup includes its cards, attachments and audit records. External assets remain references. Recovery requires the original system account.';

  @override
  String get pluginsSnapshotSaved =>
      'Library backed up, including its attachments and original protection file.';

  @override
  String get pluginsStateUnavailable =>
      'Plugin status could not be loaded. Try again.';

  @override
  String get pluginsSummary => 'Read summaries';

  @override
  String get pluginsTextInput => 'Input text';

  @override
  String get pluginsThirdParty => 'Third-party plugins';

  @override
  String get pluginsTlsIdentitiesDisable => 'Disable identity';

  @override
  String get pluginsTlsIdentitiesEmpty =>
      'No saved identities in this library.';

  @override
  String get pluginsTlsIdentitiesFileMode => 'Next start: checked local files.';

  @override
  String get pluginsTlsIdentitiesHint =>
      'Select an identity explicitly. Replacing or disabling an identity stops services that use it; a new start is always explicit.';

  @override
  String get pluginsTlsIdentitiesImport =>
      'Prepare certificate files for import or replacement';

  @override
  String get pluginsTlsIdentitiesReplace => 'Replace with checked files';

  @override
  String get pluginsTlsIdentitiesSave => 'Save as a new identity';

  @override
  String get pluginsTlsIdentitiesSaved =>
      'Saved. Review the identity and revision below, then select it for a new start.';

  @override
  String get pluginsTlsIdentitiesSavedMode =>
      'Next start: saved identity. The host checks its certificate validity at startup.';

  @override
  String get pluginsTlsIdentitiesSelect => 'Use for next start';

  @override
  String get pluginsTlsIdentitiesStale =>
      'The selected identity changed, was disabled, or has not been refreshed. Select a current identity again.';

  @override
  String get pluginsTlsIdentitiesTitle => 'Saved TLS identities';

  @override
  String get pluginsTlsIdentitiesUnknownHint =>
      'Refresh and inspect the records before acknowledging. A missing receipt does not mean the change failed; do not create it again without checking.';

  @override
  String get pluginsTlsIdentitiesUseFile => 'Use checked files for next start';

  @override
  String get pluginsTransform => 'Transform';

  @override
  String get pluginsTransformUnknown =>
      'The transformation could not be confirmed';

  @override
  String get pluginsUiExecution =>
      'Plugin execution did not finish. Reopen the view and try again.';

  @override
  String get pluginsUiRejected =>
      'The plugin action was not accepted. Check the input and current permissions.';

  @override
  String get pluginsUiUnavailable =>
      'The plugin is unavailable. Check its status and reopen the view.';

  @override
  String get pluginsUnavailableView => 'Plugin view unavailable';

  @override
  String pluginsUnconfirmed(String reason) {
    return '$reason. The operation is unconfirmed. Check the refreshed status before choosing again.';
  }

  @override
  String get pluginsUninstallKeepContent => 'Uninstall (keep content)';

  @override
  String get pluginsUninstallUnknown => 'Uninstallation could not be confirmed';

  @override
  String get pluginsUninstalled =>
      'Uninstalled. Your existing content has been preserved.';

  @override
  String get pluginsUpdatingView => 'Updating preview…';

  @override
  String get pluginsUseText => 'Use text instead';

  @override
  String get pluginsUseTransform => 'Use transform';

  @override
  String get pluginsViewFailed => 'The plugin view could not be opened';

  @override
  String get pluginsWorkbench => 'Workbench plugin';

  @override
  String get pluginsWorkbenchReadOnly =>
      'The plugin is allowed, but this workbench is read-only. Resolve the library or plugin issue, then refresh its status.';

  @override
  String get recoveryAllFiles => 'All files';

  @override
  String get recoveryBackupExists =>
      'A file already exists at the backup location. Choose a new filename.';

  @override
  String get recoveryBackupFile => 'Library backup';

  @override
  String get recoveryBackupUnknown =>
      'The backup result needs checking. Keep the current file and inspect the save location.';

  @override
  String get recoveryBindingMissing =>
      'This library has no bound protection file, so the selected file cannot be matched to it.';

  @override
  String get recoveryBusy =>
      'This library is in use by another process. Close the other window and try again.';

  @override
  String get recoveryChooseKey => 'Choose recovery file';

  @override
  String get recoveryCloseFirst =>
      'The workspace is still running. Close it before switching libraries.';

  @override
  String get recoveryFailed =>
      'Recovery did not finish. Keep the original files and try again.';

  @override
  String get recoveryIdentityBusy =>
      'Another copy of this library is in use. Close that workspace before opening this copy.';

  @override
  String get recoveryIdentityMismatch =>
      'The registered library identity does not match. Keep the original data and restore the correct backup.';

  @override
  String get recoveryKeyFile => 'Library protection file';

  @override
  String get recoveryKeyGuide =>
      'If the protection file is missing or damaged, select a backup of it. The file must belong to this library and requires the original system account.';

  @override
  String get recoveryKeyMismatch =>
      'The library key does not match or cannot be decrypted. Use the original file and system account.';

  @override
  String get recoveryKeyUnknown =>
      'The recovery result needs checking. Try opening again; a copy of the previous protection file was retained if one existed.';

  @override
  String get recoveryLibraryInvalid =>
      'The library could not be verified or opened. Keep the original library and protection key, then try again.';

  @override
  String get recoveryMaintenance =>
      'The library needs attention. Keep the original files and review the diagnostic details.';

  @override
  String get recoveryMissingKey =>
      'The library protection key is missing. Restore the original .audit-key file and try again.';

  @override
  String get recoveryMissingLibrary =>
      'The protection key exists, but the library is missing or empty. Restore the original library.';

  @override
  String get recoveryOpenFailed =>
      'The workspace could not open. Check the plugin files and data folder, then try again.';

  @override
  String get recoveryPluginUnavailable =>
      'The workspace plugin is unavailable. Existing content can still be viewed and exported.';

  @override
  String get recoveryRegistryInvalid =>
      'The active library registration is damaged or unsupported. Opening stopped to preserve the data.';

  @override
  String get recoveryRegistryUnreadable =>
      'The active library or registration file cannot be read. Check the original location; no replacement library will be created automatically.';

  @override
  String get recoveryRetry => 'Try again';

  @override
  String get recoverySnapshot => 'Restore library backup';

  @override
  String get recoverySnapshotGuide =>
      'You can restore a library backup to a new folder and switch to it. The original folder is kept. This restores content as it was when backed up and requires the original system account.';

  @override
  String get recoverySnapshotInvalid =>
      'The library backup format or integrity check is invalid. Keep the original backup file.';

  @override
  String get recoverySnapshotUnknown =>
      'The restore result needs checking. Inspect the destination folder; the original library was not replaced.';

  @override
  String recoverySwitchUnconfirmed(String path) {
    return 'The backup was restored to $path, but switching could not be confirmed. Keep this folder and reopen the workspace to check.';
  }

  @override
  String get recoverySwitchUnknown =>
      'The library switch is unconfirmed. Reopen the workspace to check.';

  @override
  String get recoveryTargetExists =>
      'The restore destination already exists. Choose a new folder that does not exist yet.';

  @override
  String get recoveryTitle => 'Reopen workspace';

  @override
  String get visualApplyColor => 'Apply color';

  @override
  String get visualApplyComponent => 'Apply to this component';

  @override
  String get visualApplyTexture => 'Apply media';

  @override
  String visualAttachmentDetails(String action, String extension, String size) {
    return '$extension · $size · $action';
  }

  @override
  String get visualAttachmentFailure =>
      'File operation failed. Check the file and available storage.';

  @override
  String get visualAttachmentPreview => 'Local attachment preview';

  @override
  String get visualAttachmentReadFailure =>
      'The attachment could not be read. Import it again.';

  @override
  String get visualAudio => 'Audio';

  @override
  String get visualAudioStateFailure =>
      'The audio state could not be confirmed. Try again.';

  @override
  String get visualAutoLyrics => 'Find missing lyrics online automatically';

  @override
  String get visualCancel => 'Cancel';

  @override
  String get visualChangeCover => 'Change album artwork';

  @override
  String get visualChooseAudio =>
      'Choose an audio file or an LRC lyric file with the same name.';

  @override
  String get visualChooseLyrics => 'Choose an LRC or TXT lyric file.';

  @override
  String get visualClickPreview => 'Select to preview';

  @override
  String get visualClose => 'Close';

  @override
  String get visualCloseDialog => 'Close dialog';

  @override
  String get visualCloseWindow => 'Close window';

  @override
  String get visualCollapsePlaylist => 'Collapse playlist';

  @override
  String get visualColorGuide =>
      'Drag the compass to choose hue and saturation, then adjust brightness. You can also enter a color value.';

  @override
  String get visualColorTitle => 'Add color to your space';

  @override
  String visualComponentCompass(String title) {
    return '$title · Color compass';
  }

  @override
  String get visualComponents => 'Components and cards';

  @override
  String get visualComponentsGuide =>
      'Each item follows the theme by default. Customize one card without changing the others.';

  @override
  String get visualCornerTips1 =>
      'Not every idea has to be useful.\nSome simply make today more interesting.';

  @override
  String get visualCornerTips2 =>
      'Write it down, then let it grow.\nAn idea does not have to arrive complete.';

  @override
  String get visualCornerTips3 =>
      'Leave a little space for yourself.\nCuriosity needs room to breathe.';

  @override
  String get visualCornerTips4 =>
      'Try something new today.\nA small detour can bring a surprise.';

  @override
  String get visualCornerTips5 =>
      'Daydreaming can lead somewhere.\nGive your thoughts a path to wander.';

  @override
  String get visualCornerTips6 =>
      'Make time for what you love.\nThere is no need to prove its worth.';

  @override
  String get visualCornerTips7 =>
      'Progress can be small.\nBeing willing to start already matters.';

  @override
  String get visualCornerTips8 =>
      'Look out the window sometimes.\nLife is a source of inspiration too.';

  @override
  String get visualCover => 'Artwork';

  @override
  String get visualCustomCompass => 'Color compass · Custom';

  @override
  String get visualCustomMaterialGuide =>
      'Turn off to follow the theme while keeping this item\'s custom settings.';

  @override
  String get visualDefaultOpen => 'Open with default app';

  @override
  String get visualDownloadOpen => 'Download to open';

  @override
  String get visualEmbeddedLyrics => 'Embedded in audio';

  @override
  String get visualExpandPlaylist => 'Expand playlist';

  @override
  String get visualFile => 'File';

  @override
  String get visualFileOpenFailure =>
      'The file could not be opened. Install a compatible app, or save the attachment and open it there.';

  @override
  String get visualFileRetry => 'File operation failed. Try again.';

  @override
  String get visualFindLyrics => 'Find lyrics';

  @override
  String get visualFindLyricsGuide =>
      'Search LRCLIB by song and artist, then choose the matching version.';

  @override
  String get visualFollowTheme => 'Follow theme';

  @override
  String get visualFooterLyrics => 'Show lyrics in footer';

  @override
  String get visualFooterTips => 'Show tips in footer';

  @override
  String get visualFooterTips1 =>
      'Nothing urgent. Give curiosity a little time.';

  @override
  String get visualFooterTips10 =>
      'You do not have to fill every minute. Leave some room.';

  @override
  String get visualFooterTips2 =>
      'Jot down a thought. You can organize it later.';

  @override
  String get visualFooterTips3 =>
      'Break a big idea into one small step for today.';

  @override
  String get visualFooterTips4 => 'Stretch a little and rest your eyes.';

  @override
  String get visualFooterTips5 =>
      'An idea is allowed to remain unanswered for now.';

  @override
  String get visualFooterTips6 => 'Some discoveries arrive when you slow down.';

  @override
  String get visualFooterTips7 =>
      'Saving a detail is a way to nurture an idea.';

  @override
  String get visualFooterTips8 =>
      'A quick note today might become tomorrow\'s beginning.';

  @override
  String get visualFooterTips9 =>
      'Let your mind wander, then return to what you enjoy.';

  @override
  String get visualFrosting => 'Frosting';

  @override
  String get visualGif => 'Animated GIF';

  @override
  String get visualHexColor => 'HEX color';

  @override
  String get visualHexInvalid => 'Enter a six-digit hexadecimal color.';

  @override
  String get visualImage => 'Image';

  @override
  String get visualImageDecodeFailure =>
      'This image cannot be decoded. Save it and open it with another app.';

  @override
  String visualImageLoadFailure(String name) {
    return 'Image could not be loaded: $name';
  }

  @override
  String visualImageNotImported(String name) {
    return '$name (image not imported)';
  }

  @override
  String visualImageUnavailable(String name) {
    return 'Image unavailable: $name';
  }

  @override
  String get visualImportFailure =>
      'Import failed. Check the file, encoding, and available storage.';

  @override
  String get visualImportLyrics => 'Import lyrics';

  @override
  String get visualImportMusic => 'Import music';

  @override
  String get visualImportMusicHint => 'Select + to import local songs';

  @override
  String get visualIndependentMaterial => 'Custom material';

  @override
  String get visualInheritColor => 'Use theme tint';

  @override
  String get visualLinkFailure =>
      'The link could not be opened. Copy the address and try again.';

  @override
  String visualLoadImage(String name) {
    return 'Load image · $name';
  }

  @override
  String get visualLoading => 'Loading…';

  @override
  String get visualLyricsEmpty => 'The lyric file is empty.';

  @override
  String get visualLyricsFile => 'Lyric file';

  @override
  String get visualLyricsImportHint => 'Import a lyric file or search online.';

  @override
  String get visualLyricsLoading => 'Loading lyrics…';

  @override
  String visualLyricsMatch(String album, String kind, int seconds) {
    String _temp0 = intl.Intl.pluralLogic(
      seconds,
      locale: localeName,
      other: '$seconds seconds',
      one: '1 second',
    );
    return '$album\n$kind · $_temp0';
  }

  @override
  String get visualLyricsMissing =>
      'No lyrics found. Import a file or search again.';

  @override
  String get visualLyricsNotFound =>
      'No lyrics found. Try changing the song title or artist.';

  @override
  String get visualLyricsOnPlay => 'Load lyrics automatically during playback';

  @override
  String get visualLyricsParseFailure =>
      'Lyrics could not be parsed. Try importing them again.';

  @override
  String get visualLyricsReadFailure =>
      'Lyrics could not be loaded. Import them manually or try again.';

  @override
  String get visualLyricsServiceFailure =>
      'Could not connect to the lyric service. Try again later or import local lyrics.';

  @override
  String get visualLyricsSize => 'Keep lyric files within 1 MB.';

  @override
  String get visualLyricsSources => 'Local file → Embedded → LRCLIB';

  @override
  String get visualLyricsVersions =>
      'Multiple versions found. Choose one in search.';

  @override
  String get visualMaterialPreview => 'Material preview';

  @override
  String get visualMaximize => 'Maximize';

  @override
  String get visualMediaAddress => 'Media address';

  @override
  String get visualMediaAddressInvalid =>
      'Enter a valid HTTP or HTTPS address without login information.';

  @override
  String get visualMediaPreviewFailure =>
      'This media cannot be previewed. Save it and open it with another app.';

  @override
  String get visualMediaType => 'Media type';

  @override
  String get visualMinimize => 'Minimize';

  @override
  String get visualMusic => 'Music';

  @override
  String get visualMusicEmptyTitle => 'Make room for music';

  @override
  String get visualMusicPlayer => 'Music player';

  @override
  String get visualNextTrack => 'Next track';

  @override
  String get visualNoLyricsRead => 'No lyrics loaded';

  @override
  String visualNoLyricsTitle(String title) {
    return '♪ $title · No lyrics';
  }

  @override
  String get visualNoTimeline => 'No timing data';

  @override
  String get visualOpacity => 'Opacity';

  @override
  String get visualOptionalArtist => 'Artist (optional)';

  @override
  String get visualPauseMusic => 'Pause music';

  @override
  String get visualPaused => 'Paused';

  @override
  String get visualPlainLyrics => 'Plain-text lyrics';

  @override
  String get visualPlayMusic => 'Play music';

  @override
  String get visualPlaybackFailure =>
      'This song cannot be played. Check the file or try another audio format.';

  @override
  String visualPlaybackPosition(int count, int index, String state) {
    return '$index / $count · $state';
  }

  @override
  String get visualPlaybackRequestFailure =>
      'Playback could not be started. Try again.';

  @override
  String get visualPlaying => 'Playing';

  @override
  String get visualPlaylistEmpty => 'Your playlist is empty';

  @override
  String get visualPlaylistLyricsHint =>
      'Import LRC lyrics from the playlist menu';

  @override
  String get visualPlaylistSaved =>
      'Playlist and lyrics are saved automatically';

  @override
  String get visualPlaylistUpdateFailure =>
      'The playlist could not be updated. Try again.';

  @override
  String get visualPreviewColor => 'Preview color';

  @override
  String get visualPreviousTrack => 'Previous track';

  @override
  String get visualRemoveAttachment => 'Remove attachment';

  @override
  String get visualRemoveTrack => 'Remove from playlist';

  @override
  String get visualResetMaterial => 'Reset to theme';

  @override
  String get visualRestoreWindow => 'Restore';

  @override
  String get visualSaveAttachment => 'Save attachment as';

  @override
  String get visualSearch => 'Search';

  @override
  String get visualSearchLyrics => 'Search lyrics';

  @override
  String get visualSongCover => 'Album artwork';

  @override
  String get visualSongTitle => 'Song title';

  @override
  String get visualSyncedLyrics => 'Synced lyrics';

  @override
  String get visualTextureFailure =>
      'Media could not be loaded. Check the file, address, or format. Web media must also allow cross-origin access.';

  @override
  String get visualTextureLinkGuide =>
      'Paste a direct HTTP or HTTPS link to an image, GIF, or video. For shared web pages, first find the original media address.';

  @override
  String get visualTextureLinkTitle => 'Bring in some inspiration';

  @override
  String get visualTexturePlaybackGuide =>
      'Videos loop silently by default; enable sound in settings. Online media must permit access, including cross-origin loading on the web.';

  @override
  String get visualTipsMaterialGuide =>
      'Off keeps tips transparent; on uses the material below. Your custom values are retained.';

  @override
  String get visualTransparentTips => 'Transparent overlay (default)';

  @override
  String get visualUseCustomMaterial => 'Use custom material';

  @override
  String get visualVideo => 'Video';

  @override
  String get visualViewLyrics => 'View lyrics';
}
