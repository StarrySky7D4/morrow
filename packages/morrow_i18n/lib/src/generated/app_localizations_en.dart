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
  String get mainSaveFailed =>
      'Could not save. Your changes remain in this session.';

  @override
  String get mainSaveIdea => 'Save idea';

  @override
  String get mainSaveNotSubmitted =>
      'Not submitted. Your draft and attachments are preserved; you can edit and save again.';

  @override
  String get mainSaveUnknown =>
      'This save is not yet confirmed. Your draft and attachments are preserved. Retry this submission; closing will refresh the workspace to check.';

  @override
  String get mainSaving => 'Saving…';

  @override
  String get mainSearchHint => 'Search your ideas…';

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
  String get pluginsExistingVersion =>
      'This version is already installed. Its enabled state is unchanged.';

  @override
  String pluginsFileLimit(int limit) {
    return 'The file is too large. Choose a file no larger than $limit bytes.';
  }

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
  String get visualUseCustomMaterial => 'Use custom material';

  @override
  String get visualVideo => 'Video';

  @override
  String get visualViewLyrics => 'View lyrics';
}
