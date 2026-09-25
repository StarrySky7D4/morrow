import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:intl/intl.dart' as intl;

import 'app_localizations_de.dart';
import 'app_localizations_en.dart';
import 'app_localizations_es.dart';
import 'app_localizations_fr.dart';
import 'app_localizations_ja.dart';
import 'app_localizations_ko.dart';
import 'app_localizations_pt.dart';
import 'app_localizations_ru.dart';
import 'app_localizations_zh.dart';

// ignore_for_file: type=lint

/// Callers can lookup localized strings with an instance of AppLocalizations
/// returned by `AppLocalizations.of(context)`.
///
/// Applications need to include `AppLocalizations.delegate()` in their app's
/// `localizationDelegates` list, and the locales they support in the app's
/// `supportedLocales` list. For example:
///
/// ```dart
/// import 'generated/app_localizations.dart';
///
/// return MaterialApp(
///   localizationsDelegates: AppLocalizations.localizationsDelegates,
///   supportedLocales: AppLocalizations.supportedLocales,
///   home: MyApplicationHome(),
/// );
/// ```
///
/// ## Update pubspec.yaml
///
/// Please make sure to update your pubspec.yaml to include the following
/// packages:
///
/// ```yaml
/// dependencies:
///   # Internationalization support.
///   flutter_localizations:
///     sdk: flutter
///   intl: any # Use the pinned version from flutter_localizations
///
///   # Rest of dependencies
/// ```
///
/// ## iOS Applications
///
/// iOS applications define key application metadata, including supported
/// locales, in an Info.plist file that is built into the application bundle.
/// To configure the locales supported by your app, you’ll need to edit this
/// file.
///
/// First, open your project’s ios/Runner.xcworkspace Xcode workspace file.
/// Then, in the Project Navigator, open the Info.plist file under the Runner
/// project’s Runner folder.
///
/// Next, select the Information Property List item, select Add Item from the
/// Editor menu, then select Localizations from the pop-up menu.
///
/// Select and expand the newly-created Localizations item then, for each
/// locale your application supports, add a new item and select the locale
/// you wish to add from the pop-up menu in the Value field. This list should
/// be consistent with the languages listed in the AppLocalizations.supportedLocales
/// property.
abstract class AppLocalizations {
  AppLocalizations(String locale)
    : localeName = intl.Intl.canonicalizedLocale(locale.toString());

  final String localeName;

  static AppLocalizations? of(BuildContext context) {
    return Localizations.of<AppLocalizations>(context, AppLocalizations);
  }

  static const LocalizationsDelegate<AppLocalizations> delegate =
      _AppLocalizationsDelegate();

  /// A list of this localizations delegate along with the default localizations
  /// delegates.
  ///
  /// Returns a list of localizations delegates containing this delegate along with
  /// GlobalMaterialLocalizations.delegate, GlobalCupertinoLocalizations.delegate,
  /// and GlobalWidgetsLocalizations.delegate.
  ///
  /// Additional delegates can be added by appending to this list in
  /// MaterialApp. This list does not have to be used at all if a custom list
  /// of delegates is preferred or required.
  static const List<LocalizationsDelegate<dynamic>> localizationsDelegates =
      <LocalizationsDelegate<dynamic>>[
        delegate,
        GlobalMaterialLocalizations.delegate,
        GlobalCupertinoLocalizations.delegate,
        GlobalWidgetsLocalizations.delegate,
      ];

  /// A list of this localizations delegate's supported locales.
  static const List<Locale> supportedLocales = <Locale>[
    Locale('en'),
    Locale('de'),
    Locale('es'),
    Locale('fr'),
    Locale('ja'),
    Locale('ko'),
    Locale('pt'),
    Locale('ru'),
    Locale('zh'),
  ];

  /// No description provided for @commonAppName.
  ///
  /// In en, this message translates to:
  /// **'Morrow'**
  String get commonAppName;

  /// No description provided for @commonCancel.
  ///
  /// In en, this message translates to:
  /// **'Cancel'**
  String get commonCancel;

  /// Typed integer and English plural qualification
  ///
  /// In en, this message translates to:
  /// **'{count, plural, =0{No items} one{1 item} other{{count} items}}'**
  String commonCount(int count);

  /// No description provided for @commonGreeting.
  ///
  /// In en, this message translates to:
  /// **'Hello, {name}'**
  String commonGreeting(String name);

  /// No description provided for @importsAttachmentLimit.
  ///
  /// In en, this message translates to:
  /// **'Up to 20 attachments can be imported at once. Paste the rest separately.'**
  String get importsAttachmentLimit;

  /// No description provided for @importsClipboardChanged.
  ///
  /// In en, this message translates to:
  /// **'The clipboard changed while being read. Please paste again.'**
  String get importsClipboardChanged;

  /// No description provided for @importsEmbeddedImageUnreadable.
  ///
  /// In en, this message translates to:
  /// **'An embedded image could not be read.'**
  String get importsEmbeddedImageUnreadable;

  /// No description provided for @importsEmbeddedImagesSeparate.
  ///
  /// In en, this message translates to:
  /// **'Some embedded images need to be imported as separate files.'**
  String get importsEmbeddedImagesSeparate;

  /// No description provided for @importsExcelValues.
  ///
  /// In en, this message translates to:
  /// **'Table values and formulas were converted to Markdown. Formatting and merged cells are preserved in the XML attachment.'**
  String get importsExcelValues;

  /// No description provided for @importsExcelXmlKept.
  ///
  /// In en, this message translates to:
  /// **'The original Excel table has been preserved as an XML attachment.'**
  String get importsExcelXmlKept;

  /// No description provided for @importsFileTooLarge.
  ///
  /// In en, this message translates to:
  /// **'The clipboard file exceeds 200 MB.'**
  String get importsFileTooLarge;

  /// No description provided for @importsItemLimit.
  ///
  /// In en, this message translates to:
  /// **'Only the first 20 items were read. Paste additional items separately.'**
  String get importsItemLimit;

  /// No description provided for @importsItemUnreadable.
  ///
  /// In en, this message translates to:
  /// **'One clipboard item could not be read. Other readable content has been kept.'**
  String get importsItemUnreadable;

  /// No description provided for @importsLocalImageNotRead.
  ///
  /// In en, this message translates to:
  /// **'Local linked images were not read automatically. Paste the image or import the original file.'**
  String get importsLocalImageNotRead;

  /// No description provided for @importsMergedTable.
  ///
  /// In en, this message translates to:
  /// **'Merged cells were converted to a readable table. Original formatting is preserved in the HTML attachment.'**
  String get importsMergedTable;

  /// No description provided for @importsOfficeBusy.
  ///
  /// In en, this message translates to:
  /// **'Another application is using the clipboard. Office objects were not read.'**
  String get importsOfficeBusy;

  /// No description provided for @importsOfficeEmbeddedKept.
  ///
  /// In en, this message translates to:
  /// **'The embedded Office object is preserved as an original attachment. Edit its charts, formulas and layout in the original application.'**
  String get importsOfficeEmbeddedKept;

  /// No description provided for @importsOfficeExportFailed.
  ///
  /// In en, this message translates to:
  /// **'An Office object exceeds the limit or could not be exported. Save it in the original application, then import it.'**
  String get importsOfficeExportFailed;

  /// No description provided for @importsOfficeReadFailed.
  ///
  /// In en, this message translates to:
  /// **'Office content could not be read. Other clipboard content is still available.'**
  String get importsOfficeReadFailed;

  /// No description provided for @importsOfficeUnavailable.
  ///
  /// In en, this message translates to:
  /// **'The Office clipboard is temporarily unavailable.'**
  String get importsOfficeUnavailable;

  /// No description provided for @importsOfficeUnreadable.
  ///
  /// In en, this message translates to:
  /// **'The original Office object could not be read. Other available content has been kept.'**
  String get importsOfficeUnreadable;

  /// No description provided for @importsRichFallback.
  ///
  /// In en, this message translates to:
  /// **'Some rich text formatting could not be converted. Readable text has been kept.'**
  String get importsRichFallback;

  /// No description provided for @importsRichTooLarge.
  ///
  /// In en, this message translates to:
  /// **'Rich text exceeds 2 MB. Please import the document as an attachment.'**
  String get importsRichTooLarge;

  /// No description provided for @importsRtfTooLarge.
  ///
  /// In en, this message translates to:
  /// **'The RTF content is too large. Please import the original document.'**
  String get importsRtfTooLarge;

  /// No description provided for @importsSpreadsheetTooLarge.
  ///
  /// In en, this message translates to:
  /// **'The table is too large. Please import the Excel file.'**
  String get importsSpreadsheetTooLarge;

  /// No description provided for @importsTableConverted.
  ///
  /// In en, this message translates to:
  /// **'The table was converted to Markdown. Complete data is preserved in the TSV attachment.'**
  String get importsTableConverted;

  /// No description provided for @importsTextTooLarge.
  ///
  /// In en, this message translates to:
  /// **'The text exceeds 2 MB. Please import it as a file.'**
  String get importsTextTooLarge;

  /// No description provided for @importsTotalTooLarge.
  ///
  /// In en, this message translates to:
  /// **'The files in this paste exceed 200 MB in total. Import them in smaller batches.'**
  String get importsTotalTooLarge;

  /// No description provided for @importsUnsupported.
  ///
  /// In en, this message translates to:
  /// **'Clipboard access is not supported here. Please import a file.'**
  String get importsUnsupported;

  /// No description provided for @mainActiveProjects.
  ///
  /// In en, this message translates to:
  /// **'In progress'**
  String get mainActiveProjects;

  /// No description provided for @mainAdjustCustomTone.
  ///
  /// In en, this message translates to:
  /// **'Adjust custom tone'**
  String get mainAdjustCustomTone;

  /// No description provided for @mainAmbientDetail.
  ///
  /// In en, this message translates to:
  /// **'Flowing light adds a touch of color.'**
  String get mainAmbientDetail;

  /// No description provided for @mainAppTitle.
  ///
  /// In en, this message translates to:
  /// **'Morrow — Room for ideas'**
  String get mainAppTitle;

  /// No description provided for @mainAppearance.
  ///
  /// In en, this message translates to:
  /// **'Appearance'**
  String get mainAppearance;

  /// No description provided for @mainArrangeIdeas.
  ///
  /// In en, this message translates to:
  /// **'Sort ideas'**
  String get mainArrangeIdeas;

  /// No description provided for @mainAttachmentCount.
  ///
  /// In en, this message translates to:
  /// **'Attachments · {count}'**
  String mainAttachmentCount(int count);

  /// No description provided for @mainAttachmentHint.
  ///
  /// In en, this message translates to:
  /// **'{count} attachments · {name}'**
  String mainAttachmentHint(int count, String name);

  /// No description provided for @mainAttachmentLimit.
  ///
  /// In en, this message translates to:
  /// **'Each record supports up to 20 attachments.'**
  String get mainAttachmentLimit;

  /// No description provided for @mainAutosaveNotice.
  ///
  /// In en, this message translates to:
  /// **'Appearance and ideas are saved locally'**
  String get mainAutosaveNotice;

  /// No description provided for @mainAwaitDiscovery.
  ///
  /// In en, this message translates to:
  /// **'Waiting for a new discovery'**
  String get mainAwaitDiscovery;

  /// No description provided for @mainBackToWorkbench.
  ///
  /// In en, this message translates to:
  /// **'Back to workspace'**
  String get mainBackToWorkbench;

  /// No description provided for @mainBackgroundCanvas.
  ///
  /// In en, this message translates to:
  /// **'Background canvas'**
  String get mainBackgroundCanvas;

  /// No description provided for @mainBackgroundSound.
  ///
  /// In en, this message translates to:
  /// **'Play background audio'**
  String get mainBackgroundSound;

  /// No description provided for @mainBodyHint.
  ///
  /// In en, this message translates to:
  /// **'Write your thoughts or paste some content…\n\nSupports # headings, lists, tables and code blocks'**
  String get mainBodyHint;

  /// No description provided for @mainBrightWhite.
  ///
  /// In en, this message translates to:
  /// **'White'**
  String get mainBrightWhite;

  /// No description provided for @mainBuiltinTexture.
  ///
  /// In en, this message translates to:
  /// **'Use built-in texture'**
  String get mainBuiltinTexture;

  /// No description provided for @mainCanvasCompass.
  ///
  /// In en, this message translates to:
  /// **'Canvas tint wheel'**
  String get mainCanvasCompass;

  /// No description provided for @mainCaptureIdea.
  ///
  /// In en, this message translates to:
  /// **'Capture idea'**
  String get mainCaptureIdea;

  /// No description provided for @mainCaptureNow.
  ///
  /// In en, this message translates to:
  /// **'Capture a thought'**
  String get mainCaptureNow;

  /// No description provided for @mainCardAttachments.
  ///
  /// In en, this message translates to:
  /// **'Files {count} · {name}'**
  String mainCardAttachments(int count, String name);

  /// No description provided for @mainCategoryExperiment.
  ///
  /// In en, this message translates to:
  /// **'Experiment'**
  String get mainCategoryExperiment;

  /// No description provided for @mainCategoryIdea.
  ///
  /// In en, this message translates to:
  /// **'Idea'**
  String get mainCategoryIdea;

  /// No description provided for @mainCategoryProject.
  ///
  /// In en, this message translates to:
  /// **'Project'**
  String get mainCategoryProject;

  /// No description provided for @mainCategoryPrompt.
  ///
  /// In en, this message translates to:
  /// **'Where to keep it'**
  String get mainCategoryPrompt;

  /// No description provided for @mainChangeFailed.
  ///
  /// In en, this message translates to:
  /// **'This change could not be saved. Your draft is preserved; you can retry.'**
  String get mainChangeFailed;

  /// No description provided for @mainCheckAgain.
  ///
  /// In en, this message translates to:
  /// **'Check again'**
  String get mainCheckAgain;

  /// No description provided for @mainClearSearch.
  ///
  /// In en, this message translates to:
  /// **'Clear search'**
  String get mainClearSearch;

  /// No description provided for @mainClipboardEmpty.
  ///
  /// In en, this message translates to:
  /// **'No readable text or files in the clipboard. Copy a file from your file manager, or use Import file.'**
  String get mainClipboardEmpty;

  /// No description provided for @mainClipboardReadFailed.
  ///
  /// In en, this message translates to:
  /// **'Could not read the content. Import a file, or check file and clipboard permissions.'**
  String get mainClipboardReadFailed;

  /// No description provided for @mainClipboardSupport.
  ///
  /// In en, this message translates to:
  /// **'Supports Markdown, Office rich text and tables, screenshots and files. Complex objects keep their original attachments. Up to 20 attachments, 200 MB each.'**
  String get mainClipboardSupport;

  /// No description provided for @mainCollapseSidebar.
  ///
  /// In en, this message translates to:
  /// **'Collapse sidebar'**
  String get mainCollapseSidebar;

  /// No description provided for @mainCompletedProjects.
  ///
  /// In en, this message translates to:
  /// **'Completed'**
  String get mainCompletedProjects;

  /// No description provided for @mainComponentCompass.
  ///
  /// In en, this message translates to:
  /// **'Component tint wheel'**
  String get mainComponentCompass;

  /// No description provided for @mainComponentEmpty.
  ///
  /// In en, this message translates to:
  /// **'Empty state'**
  String get mainComponentEmpty;

  /// No description provided for @mainComponentFooter.
  ///
  /// In en, this message translates to:
  /// **'Footer tips and lyrics'**
  String get mainComponentFooter;

  /// No description provided for @mainComponentHero.
  ///
  /// In en, this message translates to:
  /// **'Overview card'**
  String get mainComponentHero;

  /// No description provided for @mainComponentNavigation.
  ///
  /// In en, this message translates to:
  /// **'Sidebar navigation'**
  String get mainComponentNavigation;

  /// No description provided for @mainComponentQuickCapture.
  ///
  /// In en, this message translates to:
  /// **'Quick capture'**
  String get mainComponentQuickCapture;

  /// No description provided for @mainComponentSearch.
  ///
  /// In en, this message translates to:
  /// **'Search bar'**
  String get mainComponentSearch;

  /// No description provided for @mainComponentSettings.
  ///
  /// In en, this message translates to:
  /// **'Components and cards · Settings'**
  String get mainComponentSettings;

  /// No description provided for @mainContentCommittedRefreshFailed.
  ///
  /// In en, this message translates to:
  /// **'The operation was committed, but the latest content could not be loaded. Refresh to view it.'**
  String get mainContentCommittedRefreshFailed;

  /// No description provided for @mainContentProtection.
  ///
  /// In en, this message translates to:
  /// **'Content protection'**
  String get mainContentProtection;

  /// No description provided for @mainContentRead.
  ///
  /// In en, this message translates to:
  /// **'Content read'**
  String get mainContentRead;

  /// No description provided for @mainContentReadFiles.
  ///
  /// In en, this message translates to:
  /// **'Content read; {count} attachments preserved'**
  String mainContentReadFiles(int count);

  /// No description provided for @mainCornerRadius.
  ///
  /// In en, this message translates to:
  /// **'Corner radius'**
  String get mainCornerRadius;

  /// No description provided for @mainCredentialSettings.
  ///
  /// In en, this message translates to:
  /// **'Credentials'**
  String get mainCredentialSettings;

  /// No description provided for @mainCrystal.
  ///
  /// In en, this message translates to:
  /// **'Crystal clear'**
  String get mainCrystal;

  /// No description provided for @mainCrystalDetail.
  ///
  /// In en, this message translates to:
  /// **'Light and clear, letting color shine through.'**
  String get mainCrystalDetail;

  /// No description provided for @mainCuriosity.
  ///
  /// In en, this message translates to:
  /// **'Interesting things begin\nwith a little curiosity.'**
  String get mainCuriosity;

  /// No description provided for @mainCustomCompass.
  ///
  /// In en, this message translates to:
  /// **'Color wheel · Custom'**
  String get mainCustomCompass;

  /// No description provided for @mainCustomLightness.
  ///
  /// In en, this message translates to:
  /// **'Custom lightness'**
  String get mainCustomLightness;

  /// No description provided for @mainCustomTheme.
  ///
  /// In en, this message translates to:
  /// **'Custom'**
  String get mainCustomTheme;

  /// No description provided for @mainDaily.
  ///
  /// In en, this message translates to:
  /// **'Little things'**
  String get mainDaily;

  /// No description provided for @mainDailyExplore.
  ///
  /// In en, this message translates to:
  /// **'Take ten minutes to explore'**
  String get mainDailyExplore;

  /// No description provided for @mainDailyIdea.
  ///
  /// In en, this message translates to:
  /// **'Write down an idea'**
  String get mainDailyIdea;

  /// No description provided for @mainDailyWater.
  ///
  /// In en, this message translates to:
  /// **'Pour yourself a glass of water'**
  String get mainDailyWater;

  /// No description provided for @mainDarkTheme.
  ///
  /// In en, this message translates to:
  /// **'Dark'**
  String get mainDarkTheme;

  /// No description provided for @mainDeepBlack.
  ///
  /// In en, this message translates to:
  /// **'Black'**
  String get mainDeepBlack;

  /// No description provided for @mainDefaultCanvas.
  ///
  /// In en, this message translates to:
  /// **'Default'**
  String get mainDefaultCanvas;

  /// No description provided for @mainDefaultGlobalColor.
  ///
  /// In en, this message translates to:
  /// **'Default theme color · All controls'**
  String get mainDefaultGlobalColor;

  /// No description provided for @mainDelete.
  ///
  /// In en, this message translates to:
  /// **'Delete'**
  String get mainDelete;

  /// No description provided for @mainDeleted.
  ///
  /// In en, this message translates to:
  /// **'Deleted “{title}”'**
  String mainDeleted(String title);

  /// No description provided for @mainDiagnosticDetails.
  ///
  /// In en, this message translates to:
  /// **'Diagnostic details'**
  String get mainDiagnosticDetails;

  /// No description provided for @mainDone.
  ///
  /// In en, this message translates to:
  /// **'Done'**
  String get mainDone;

  /// No description provided for @mainDraftHandoffRecoveryAssets.
  ///
  /// In en, this message translates to:
  /// **'Pinned attachments'**
  String get mainDraftHandoffRecoveryAssets;

  /// No description provided for @mainDraftHandoffRecoveryCancel.
  ///
  /// In en, this message translates to:
  /// **'Cancel handoff'**
  String get mainDraftHandoffRecoveryCancel;

  /// No description provided for @mainDraftHandoffRecoveryCancelBody.
  ///
  /// In en, this message translates to:
  /// **'Cancel this unfinished handoff. The original operation can no longer create a successor draft. The parent draft remains unchanged.'**
  String get mainDraftHandoffRecoveryCancelBody;

  /// No description provided for @mainDraftHandoffRecoveryCancelled.
  ///
  /// In en, this message translates to:
  /// **'Handoff cancelled'**
  String get mainDraftHandoffRecoveryCancelled;

  /// No description provided for @mainDraftHandoffRecoveryChildCommitted.
  ///
  /// In en, this message translates to:
  /// **'Successor draft saved; parent draft not yet retired'**
  String get mainDraftHandoffRecoveryChildCommitted;

  /// No description provided for @mainDraftHandoffRecoveryComplete.
  ///
  /// In en, this message translates to:
  /// **'Create successor draft'**
  String get mainDraftHandoffRecoveryComplete;

  /// No description provided for @mainDraftHandoffRecoveryCompleteBody.
  ///
  /// In en, this message translates to:
  /// **'Create the successor draft from the saved original proposal. This will not submit another final card.'**
  String get mainDraftHandoffRecoveryCompleteBody;

  /// No description provided for @mainDraftHandoffRecoveryConfirmTitle.
  ///
  /// In en, this message translates to:
  /// **'Confirm draft handoff action'**
  String get mainDraftHandoffRecoveryConfirmTitle;

  /// No description provided for @mainDraftHandoffRecoveryConfirmed.
  ///
  /// In en, this message translates to:
  /// **'The operation was confirmed and saved.'**
  String get mainDraftHandoffRecoveryConfirmed;

  /// No description provided for @mainDraftHandoffRecoveryConflict.
  ///
  /// In en, this message translates to:
  /// **'The current state has changed; review it'**
  String get mainDraftHandoffRecoveryConflict;

  /// No description provided for @mainDraftHandoffRecoveryEmpty.
  ///
  /// In en, this message translates to:
  /// **'No handoff records to review'**
  String get mainDraftHandoffRecoveryEmpty;

  /// No description provided for @mainDraftHandoffRecoveryFailed.
  ///
  /// In en, this message translates to:
  /// **'Could not complete the operation. Refresh and review the record.'**
  String get mainDraftHandoffRecoveryFailed;

  /// No description provided for @mainDraftHandoffRecoveryFields.
  ///
  /// In en, this message translates to:
  /// **'Original draft content'**
  String get mainDraftHandoffRecoveryFields;

  /// No description provided for @mainDraftHandoffRecoveryInspect.
  ///
  /// In en, this message translates to:
  /// **'View and review'**
  String get mainDraftHandoffRecoveryInspect;

  /// No description provided for @mainDraftHandoffRecoveryIntro.
  ///
  /// In en, this message translates to:
  /// **'View saved handoff records only. Opening this page does not submit content.'**
  String get mainDraftHandoffRecoveryIntro;

  /// No description provided for @mainDraftHandoffRecoveryParentRetired.
  ///
  /// In en, this message translates to:
  /// **'Handoff complete'**
  String get mainDraftHandoffRecoveryParentRetired;

  /// No description provided for @mainDraftHandoffRecoveryPending.
  ///
  /// In en, this message translates to:
  /// **'Successor draft has not been created'**
  String get mainDraftHandoffRecoveryPending;

  /// No description provided for @mainDraftHandoffRecoveryReadOnly.
  ///
  /// In en, this message translates to:
  /// **'This workspace is view-only'**
  String get mainDraftHandoffRecoveryReadOnly;

  /// No description provided for @mainDraftHandoffRecoveryRetire.
  ///
  /// In en, this message translates to:
  /// **'Retire parent draft'**
  String get mainDraftHandoffRecoveryRetire;

  /// No description provided for @mainDraftHandoffRecoveryRetireBody.
  ///
  /// In en, this message translates to:
  /// **'After confirming the successor draft was saved, mark the parent draft as retired.'**
  String get mainDraftHandoffRecoveryRetireBody;

  /// No description provided for @mainDraftHandoffRecoverySelection.
  ///
  /// In en, this message translates to:
  /// **'Select a record to view the complete draft.'**
  String get mainDraftHandoffRecoverySelection;

  /// No description provided for @mainDraftHandoffRecoveryTitle.
  ///
  /// In en, this message translates to:
  /// **'Draft handoff recovery'**
  String get mainDraftHandoffRecoveryTitle;

  /// No description provided for @mainDraftHandoffRecoveryUnknown.
  ///
  /// In en, this message translates to:
  /// **'The operation result is not confirmed. Review the original record; do not repeat the operation.'**
  String get mainDraftHandoffRecoveryUnknown;

  /// No description provided for @mainDraftImportRecoveryCancelBody.
  ///
  /// In en, this message translates to:
  /// **'After cancellation, the original abandonment request will no longer run. This does not delete the attachment or undo other changes.'**
  String get mainDraftImportRecoveryCancelBody;

  /// No description provided for @mainDraftImportRecoveryCancelDecision.
  ///
  /// In en, this message translates to:
  /// **'Cancel abandonment request'**
  String get mainDraftImportRecoveryCancelDecision;

  /// No description provided for @mainDraftImportRecoveryCancelTitle.
  ///
  /// In en, this message translates to:
  /// **'Cancel this abandonment request?'**
  String get mainDraftImportRecoveryCancelTitle;

  /// No description provided for @mainDraftImportRecoveryCancelled.
  ///
  /// In en, this message translates to:
  /// **'Cancelled: the original abandonment request will no longer run.'**
  String get mainDraftImportRecoveryCancelled;

  /// No description provided for @mainDraftImportRecoveryClose.
  ///
  /// In en, this message translates to:
  /// **'Close'**
  String get mainDraftImportRecoveryClose;

  /// No description provided for @mainDraftImportRecoveryCommitted.
  ///
  /// In en, this message translates to:
  /// **'Completed: this attachment import was abandoned.'**
  String get mainDraftImportRecoveryCommitted;

  /// No description provided for @mainDraftImportRecoveryConfirmedRefreshFailed.
  ///
  /// In en, this message translates to:
  /// **'Action confirmed, but the list has not refreshed. Refresh to see its latest status.'**
  String get mainDraftImportRecoveryConfirmedRefreshFailed;

  /// No description provided for @mainDraftImportRecoveryConflict.
  ///
  /// In en, this message translates to:
  /// **'Conflict: the draft changed. This request can only be cancelled.'**
  String get mainDraftImportRecoveryConflict;

  /// No description provided for @mainDraftImportRecoveryEmpty.
  ///
  /// In en, this message translates to:
  /// **'No attachment decisions need review.'**
  String get mainDraftImportRecoveryEmpty;

  /// No description provided for @mainDraftImportRecoveryFailed.
  ///
  /// In en, this message translates to:
  /// **'Could not review this decision. Refresh and try again.'**
  String get mainDraftImportRecoveryFailed;

  /// No description provided for @mainDraftImportRecoveryPending.
  ///
  /// In en, this message translates to:
  /// **'Pending: the original abandonment request is not confirmed.'**
  String get mainDraftImportRecoveryPending;

  /// No description provided for @mainDraftImportRecoveryRetry.
  ///
  /// In en, this message translates to:
  /// **'Retry original abandonment'**
  String get mainDraftImportRecoveryRetry;

  /// No description provided for @mainDraftImportRecoveryTitle.
  ///
  /// In en, this message translates to:
  /// **'Review attachment decisions'**
  String get mainDraftImportRecoveryTitle;

  /// No description provided for @mainEdit.
  ///
  /// In en, this message translates to:
  /// **'Edit'**
  String get mainEdit;

  /// No description provided for @mainEditIdeaTitle.
  ///
  /// In en, this message translates to:
  /// **'Make your idea clearer'**
  String get mainEditIdeaTitle;

  /// No description provided for @mainEditorClosedUnknown.
  ///
  /// In en, this message translates to:
  /// **'The editor closed, but the save is not yet confirmed. Reopen the workspace to check before creating another copy.'**
  String get mainEditorClosedUnknown;

  /// No description provided for @mainEditorContinueDraft.
  ///
  /// In en, this message translates to:
  /// **'Continue editing'**
  String get mainEditorContinueDraft;

  /// No description provided for @mainEditorContinueFailed.
  ///
  /// In en, this message translates to:
  /// **'The previous edit is confirmed and your newer draft remains in this window. Could not open the next editor; please retry.'**
  String get mainEditorContinueFailed;

  /// No description provided for @mainEditorNewerDraft.
  ///
  /// In en, this message translates to:
  /// **'The previous edit was saved. Your newer draft is still unsaved.'**
  String get mainEditorNewerDraft;

  /// No description provided for @mainEditorPendingDraft.
  ///
  /// In en, this message translates to:
  /// **'You can keep typing. Resolve the previous save first; newer edits will not be sent automatically.'**
  String get mainEditorPendingDraft;

  /// No description provided for @mainEditorRecoveryAbandonBody.
  ///
  /// In en, this message translates to:
  /// **'Abandon this uncommitted edit? Its original operation will be sealed. Saved content and newer drafts stay unchanged. This does not undo a committed edit.'**
  String get mainEditorRecoveryAbandonBody;

  /// No description provided for @mainEditorRecoveryAbandonTitle.
  ///
  /// In en, this message translates to:
  /// **'Abandon old edit'**
  String get mainEditorRecoveryAbandonTitle;

  /// No description provided for @mainEditorRecoveryCommitted.
  ///
  /// In en, this message translates to:
  /// **'The original edit committed. Review the current content and acknowledge it without saving again.'**
  String get mainEditorRecoveryCommitted;

  /// No description provided for @mainEditorRecoveryConfirm.
  ///
  /// In en, this message translates to:
  /// **'Review and acknowledge'**
  String get mainEditorRecoveryConfirm;

  /// No description provided for @mainEditorRecoveryConflict.
  ///
  /// In en, this message translates to:
  /// **'The content baseline or operation history changed. The original proposal is retained and cannot overwrite current content.'**
  String get mainEditorRecoveryConflict;

  /// No description provided for @mainEditorRecoveryEmpty.
  ///
  /// In en, this message translates to:
  /// **'No persistent edit proposals need review.'**
  String get mainEditorRecoveryEmpty;

  /// No description provided for @mainEditorRecoveryFailed.
  ///
  /// In en, this message translates to:
  /// **'Review could not finish. The original proposal is retained. Refresh and try again.'**
  String get mainEditorRecoveryFailed;

  /// No description provided for @mainEditorRecoveryPending.
  ///
  /// In en, this message translates to:
  /// **'The original edit is unconfirmed. Continue retries only that proposal, without submitting newer changes.'**
  String get mainEditorRecoveryPending;

  /// No description provided for @mainEditorRecoveryResume.
  ///
  /// In en, this message translates to:
  /// **'Continue original edit'**
  String get mainEditorRecoveryResume;

  /// No description provided for @mainEditorRecoveryTitle.
  ///
  /// In en, this message translates to:
  /// **'Review pending edits'**
  String get mainEditorRecoveryTitle;

  /// No description provided for @mainEditorSubtitle.
  ///
  /// In en, this message translates to:
  /// **'Text, tables, images—keep them here while your idea takes shape.'**
  String get mainEditorSubtitle;

  /// No description provided for @mainEditorUnavailable.
  ///
  /// In en, this message translates to:
  /// **'The editor is unavailable. Check the content service and retry.'**
  String get mainEditorUnavailable;

  /// No description provided for @mainEndpointSettings.
  ///
  /// In en, this message translates to:
  /// **'Outbound endpoints'**
  String get mainEndpointSettings;

  /// No description provided for @mainExpandSettings.
  ///
  /// In en, this message translates to:
  /// **'Expand settings'**
  String get mainExpandSettings;

  /// No description provided for @mainExpandSidebar.
  ///
  /// In en, this message translates to:
  /// **'Expand sidebar'**
  String get mainExpandSidebar;

  /// No description provided for @mainExtensionPlugins.
  ///
  /// In en, this message translates to:
  /// **'Extensions'**
  String get mainExtensionPlugins;

  /// No description provided for @mainFavoriteAttachments.
  ///
  /// In en, this message translates to:
  /// **'Saved attachments'**
  String get mainFavoriteAttachments;

  /// No description provided for @mainFavoriteRecords.
  ///
  /// In en, this message translates to:
  /// **'Saved records'**
  String get mainFavoriteRecords;

  /// No description provided for @mainFavoriteTooltip.
  ///
  /// In en, this message translates to:
  /// **'Favorite {title}'**
  String mainFavoriteTooltip(String title);

  /// No description provided for @mainFavoritesIntro.
  ///
  /// In en, this message translates to:
  /// **'Your favorite text, images and files, all together.'**
  String get mainFavoritesIntro;

  /// No description provided for @mainFieldLimit.
  ///
  /// In en, this message translates to:
  /// **'This field supports up to {limit} characters. Shorten the content or import it as a file.'**
  String mainFieldLimit(int limit);

  /// No description provided for @mainFilterAll.
  ///
  /// In en, this message translates to:
  /// **'All'**
  String get mainFilterAll;

  /// No description provided for @mainFilterAttachments.
  ///
  /// In en, this message translates to:
  /// **'With files'**
  String get mainFilterAttachments;

  /// No description provided for @mainFilterFavorites.
  ///
  /// In en, this message translates to:
  /// **'Favorites only'**
  String get mainFilterFavorites;

  /// No description provided for @mainFilterFile.
  ///
  /// In en, this message translates to:
  /// **'Files'**
  String get mainFilterFile;

  /// No description provided for @mainFilterImage.
  ///
  /// In en, this message translates to:
  /// **'Images'**
  String get mainFilterImage;

  /// No description provided for @mainFilterMedia.
  ///
  /// In en, this message translates to:
  /// **'Audio / video'**
  String get mainFilterMedia;

  /// No description provided for @mainFilterPending.
  ///
  /// In en, this message translates to:
  /// **'To do'**
  String get mainFilterPending;

  /// No description provided for @mainFilterText.
  ///
  /// In en, this message translates to:
  /// **'Text'**
  String get mainFilterText;

  /// No description provided for @mainFollowTheme.
  ///
  /// In en, this message translates to:
  /// **'Follow theme'**
  String get mainFollowTheme;

  /// No description provided for @mainFontApply.
  ///
  /// In en, this message translates to:
  /// **'Apply font'**
  String get mainFontApply;

  /// No description provided for @mainFontDefault.
  ///
  /// In en, this message translates to:
  /// **'Default font'**
  String get mainFontDefault;

  /// No description provided for @mainFontFailed.
  ///
  /// In en, this message translates to:
  /// **'Unable to load this font. Check the file or restart the app and try again.'**
  String get mainFontFailed;

  /// No description provided for @mainFontFamily.
  ///
  /// In en, this message translates to:
  /// **'System font family'**
  String get mainFontFamily;

  /// No description provided for @mainFontHelp.
  ///
  /// In en, this message translates to:
  /// **'TTF / OTF, up to 20 MiB. Unavailable system fonts fall back automatically.'**
  String get mainFontHelp;

  /// No description provided for @mainFontHint.
  ///
  /// In en, this message translates to:
  /// **'For example: Arial or Microsoft YaHei'**
  String get mainFontHint;

  /// No description provided for @mainFontImport.
  ///
  /// In en, this message translates to:
  /// **'Import font file'**
  String get mainFontImport;

  /// No description provided for @mainFontReset.
  ///
  /// In en, this message translates to:
  /// **'Restore default'**
  String get mainFontReset;

  /// No description provided for @mainFontSettings.
  ///
  /// In en, this message translates to:
  /// **'Fonts'**
  String get mainFontSettings;

  /// No description provided for @mainFontUnavailable.
  ///
  /// In en, this message translates to:
  /// **'The saved font is unavailable. Using the default font for now.'**
  String get mainFontUnavailable;

  /// No description provided for @mainFrostDetail.
  ///
  /// In en, this message translates to:
  /// **'Soften the background and give your thoughts room.'**
  String get mainFrostDetail;

  /// No description provided for @mainFrostEffect.
  ///
  /// In en, this message translates to:
  /// **'Blur'**
  String get mainFrostEffect;

  /// No description provided for @mainFrostOpacity.
  ///
  /// In en, this message translates to:
  /// **'Frost opacity'**
  String get mainFrostOpacity;

  /// No description provided for @mainFrostUnavailable.
  ///
  /// In en, this message translates to:
  /// **'Desktop blur is unavailable on this system. Tint and opacity can still be adjusted.'**
  String get mainFrostUnavailable;

  /// No description provided for @mainFrosted.
  ///
  /// In en, this message translates to:
  /// **'Frosted'**
  String get mainFrosted;

  /// No description provided for @mainGlassTexture.
  ///
  /// In en, this message translates to:
  /// **'Glass style'**
  String get mainGlassTexture;

  /// No description provided for @mainGlobalColor.
  ///
  /// In en, this message translates to:
  /// **'{color} · All controls'**
  String mainGlobalColor(String color);

  /// No description provided for @mainGreeting.
  ///
  /// In en, this message translates to:
  /// **'Let your ideas grow.'**
  String get mainGreeting;

  /// No description provided for @mainGreetingDetail.
  ///
  /// In en, this message translates to:
  /// **'Keep the everyday details and the sparks of inspiration.'**
  String get mainGreetingDetail;

  /// No description provided for @mainHeroBody.
  ///
  /// In en, this message translates to:
  /// **'A thought, a small task, a what-if.\nThis is where they begin.'**
  String get mainHeroBody;

  /// No description provided for @mainHeroCaption.
  ///
  /// In en, this message translates to:
  /// **'THE POSSIBILITY CORNER'**
  String get mainHeroCaption;

  /// No description provided for @mainHeroTitle.
  ///
  /// In en, this message translates to:
  /// **'It is okay to start small.'**
  String get mainHeroTitle;

  /// No description provided for @mainHideAppearance.
  ///
  /// In en, this message translates to:
  /// **'Hide appearance settings'**
  String get mainHideAppearance;

  /// No description provided for @mainHideCustomTone.
  ///
  /// In en, this message translates to:
  /// **'Hide custom tone'**
  String get mainHideCustomTone;

  /// No description provided for @mainHidePreview.
  ///
  /// In en, this message translates to:
  /// **'Hide preview'**
  String get mainHidePreview;

  /// No description provided for @mainHttpSettings.
  ///
  /// In en, this message translates to:
  /// **'HTTP tasks'**
  String get mainHttpSettings;

  /// No description provided for @mainHypothesis.
  ///
  /// In en, this message translates to:
  /// **'Hypothesis'**
  String get mainHypothesis;

  /// No description provided for @mainHypothesisPrompt.
  ///
  /// In en, this message translates to:
  /// **'Hypothesis to test'**
  String get mainHypothesisPrompt;

  /// No description provided for @mainHypothesisSection.
  ///
  /// In en, this message translates to:
  /// **'Hypothesis / What to try'**
  String get mainHypothesisSection;

  /// No description provided for @mainIdeaDetails.
  ///
  /// In en, this message translates to:
  /// **'Keep the details. Make the next step clearer.'**
  String get mainIdeaDetails;

  /// No description provided for @mainIdeaNameHint.
  ///
  /// In en, this message translates to:
  /// **'Give it a name'**
  String get mainIdeaNameHint;

  /// No description provided for @mainIdeaNameRequired.
  ///
  /// In en, this message translates to:
  /// **'Write down your idea first'**
  String get mainIdeaNameRequired;

  /// No description provided for @mainIdeaSaved.
  ///
  /// In en, this message translates to:
  /// **'Idea saved.'**
  String get mainIdeaSaved;

  /// No description provided for @mainImportFailed.
  ///
  /// In en, this message translates to:
  /// **'Could not import the media. Check the file and available storage.'**
  String get mainImportFailed;

  /// No description provided for @mainImportFile.
  ///
  /// In en, this message translates to:
  /// **'Import file'**
  String get mainImportFile;

  /// No description provided for @mainInboxIntro.
  ///
  /// In en, this message translates to:
  /// **'Capture first, organize later. Turn promising ideas into small projects.'**
  String get mainInboxIntro;

  /// No description provided for @mainIoNoDeclarations.
  ///
  /// In en, this message translates to:
  /// **'No installed plugin currently declares file or network access.'**
  String get mainIoNoDeclarations;

  /// No description provided for @mainIoSettings.
  ///
  /// In en, this message translates to:
  /// **'Network & file access'**
  String get mainIoSettings;

  /// No description provided for @mainIoSettingsGuide.
  ///
  /// In en, this message translates to:
  /// **'Manage plugin file and network permissions separately from appearance. Approving a declared capability does not grant access to every file or endpoint; available operations depend on the current backend.'**
  String get mainIoSettingsGuide;

  /// No description provided for @mainIoSettingsSummary.
  ///
  /// In en, this message translates to:
  /// **'Permissions, credentials, endpoints and API services'**
  String get mainIoSettingsSummary;

  /// No description provided for @mainJustNow.
  ///
  /// In en, this message translates to:
  /// **'Just now'**
  String get mainJustNow;

  /// No description provided for @mainLabIntro.
  ///
  /// In en, this message translates to:
  /// **'Start with a hypothesis. Keep your attempts, observations and surprises.'**
  String get mainLabIntro;

  /// No description provided for @mainLanguage.
  ///
  /// In en, this message translates to:
  /// **'Language'**
  String get mainLanguage;

  /// No description provided for @mainLanguageChinese.
  ///
  /// In en, this message translates to:
  /// **'简体中文'**
  String get mainLanguageChinese;

  /// No description provided for @mainLanguageEnglish.
  ///
  /// In en, this message translates to:
  /// **'English'**
  String get mainLanguageEnglish;

  /// No description provided for @mainLanguageSystem.
  ///
  /// In en, this message translates to:
  /// **'System default'**
  String get mainLanguageSystem;

  /// No description provided for @mainLavender.
  ///
  /// In en, this message translates to:
  /// **'Lavender'**
  String get mainLavender;

  /// No description provided for @mainLegacyStageComplete.
  ///
  /// In en, this message translates to:
  /// **'The legacy format will check every task and complete the project. Continue?'**
  String get mainLegacyStageComplete;

  /// No description provided for @mainLegacyStageReopen.
  ///
  /// In en, this message translates to:
  /// **'Returning to another stage in the legacy format will uncheck the last task “{task}” and every task with that name. Continue?'**
  String mainLegacyStageReopen(String task);

  /// No description provided for @mainLegacyTodoContinue.
  ///
  /// In en, this message translates to:
  /// **'Continue'**
  String get mainLegacyTodoContinue;

  /// No description provided for @mainLegacyTodoGroup.
  ///
  /// In en, this message translates to:
  /// **'The legacy format identifies tasks by text. This changes every task named “{task}”. Continue?'**
  String mainLegacyTodoGroup(String task);

  /// No description provided for @mainLegacyTodoTitle.
  ///
  /// In en, this message translates to:
  /// **'Legacy checklist changes'**
  String get mainLegacyTodoTitle;

  /// No description provided for @mainLightOpacity.
  ///
  /// In en, this message translates to:
  /// **'20% · Light'**
  String get mainLightOpacity;

  /// No description provided for @mainLiquidAllCanvases.
  ///
  /// In en, this message translates to:
  /// **'Available independently on all four canvas types'**
  String get mainLiquidAllCanvases;

  /// No description provided for @mainLiquidDetail.
  ///
  /// In en, this message translates to:
  /// **'Flowing highlights and gentle refraction, like a suspended drop of water.'**
  String get mainLiquidDetail;

  /// No description provided for @mainLiquidEffect.
  ///
  /// In en, this message translates to:
  /// **'Liquid glass effect'**
  String get mainLiquidEffect;

  /// No description provided for @mainLiquidGlass.
  ///
  /// In en, this message translates to:
  /// **'Liquid glass'**
  String get mainLiquidGlass;

  /// No description provided for @mainLivePreview.
  ///
  /// In en, this message translates to:
  /// **'Live preview'**
  String get mainLivePreview;

  /// No description provided for @mainLocalMedia.
  ///
  /// In en, this message translates to:
  /// **'Local media'**
  String get mainLocalMedia;

  /// No description provided for @mainMakeYours.
  ///
  /// In en, this message translates to:
  /// **'MAKE IT YOURS'**
  String get mainMakeYours;

  /// No description provided for @mainMarkOrganized.
  ///
  /// In en, this message translates to:
  /// **'Mark as organized'**
  String get mainMarkOrganized;

  /// No description provided for @mainMarkdownBody.
  ///
  /// In en, this message translates to:
  /// **'Body · Markdown'**
  String get mainMarkdownBody;

  /// No description provided for @mainMediaLimits.
  ///
  /// In en, this message translates to:
  /// **'Images / GIFs ≤ 25 MB; videos ≤ 150 MB'**
  String get mainMediaLimits;

  /// No description provided for @mainMonochrome.
  ///
  /// In en, this message translates to:
  /// **'Monochrome'**
  String get mainMonochrome;

  /// No description provided for @mainMoreSteps.
  ///
  /// In en, this message translates to:
  /// **'{count} more steps; open to view'**
  String mainMoreSteps(int count);

  /// No description provided for @mainMovedProject.
  ///
  /// In en, this message translates to:
  /// **'“{title}” moved to Projects'**
  String mainMovedProject(String title);

  /// No description provided for @mainMusic.
  ///
  /// In en, this message translates to:
  /// **'Music player'**
  String get mainMusic;

  /// No description provided for @mainMySpace.
  ///
  /// In en, this message translates to:
  /// **'My space'**
  String get mainMySpace;

  /// No description provided for @mainNavigation.
  ///
  /// In en, this message translates to:
  /// **'Navigation'**
  String get mainNavigation;

  /// No description provided for @mainNewIdea.
  ///
  /// In en, this message translates to:
  /// **'New idea'**
  String get mainNewIdea;

  /// No description provided for @mainNewIdeaTitle.
  ///
  /// In en, this message translates to:
  /// **'Catch a new idea'**
  String get mainNewIdeaTitle;

  /// No description provided for @mainNoHypothesis.
  ///
  /// In en, this message translates to:
  /// **'No hypothesis yet'**
  String get mainNoHypothesis;

  /// No description provided for @mainNoMatches.
  ///
  /// In en, this message translates to:
  /// **'No matching ideas here'**
  String get mainNoMatches;

  /// No description provided for @mainNoResultYet.
  ///
  /// In en, this message translates to:
  /// **'The result can wait. The process is worth recording too.'**
  String get mainNoResultYet;

  /// No description provided for @mainNotNow.
  ///
  /// In en, this message translates to:
  /// **'Not now'**
  String get mainNotNow;

  /// No description provided for @mainObservationSection.
  ///
  /// In en, this message translates to:
  /// **'Observations / What you found'**
  String get mainObservationSection;

  /// No description provided for @mainObservations.
  ///
  /// In en, this message translates to:
  /// **'Observations and conclusions'**
  String get mainObservations;

  /// No description provided for @mainObservationsPrompt.
  ///
  /// In en, this message translates to:
  /// **'Observations, process and conclusions'**
  String get mainObservationsPrompt;

  /// No description provided for @mainOneHourAgo.
  ///
  /// In en, this message translates to:
  /// **'1 hour ago'**
  String get mainOneHourAgo;

  /// No description provided for @mainOnlineMedia.
  ///
  /// In en, this message translates to:
  /// **'Online media'**
  String get mainOnlineMedia;

  /// No description provided for @mainOpaqueFallback.
  ///
  /// In en, this message translates to:
  /// **'Clear panels over the current theme color.'**
  String get mainOpaqueFallback;

  /// No description provided for @mainOpenNextStep.
  ///
  /// In en, this message translates to:
  /// **'Open project to edit next steps'**
  String get mainOpenNextStep;

  /// No description provided for @mainOrganizedCount.
  ///
  /// In en, this message translates to:
  /// **'Organized'**
  String get mainOrganizedCount;

  /// No description provided for @mainOriginalColors.
  ///
  /// In en, this message translates to:
  /// **'Original colors'**
  String get mainOriginalColors;

  /// No description provided for @mainPageFavorites.
  ///
  /// In en, this message translates to:
  /// **'Favorites'**
  String get mainPageFavorites;

  /// No description provided for @mainPageInbox.
  ///
  /// In en, this message translates to:
  /// **'Inbox'**
  String get mainPageInbox;

  /// No description provided for @mainPageLaboratory.
  ///
  /// In en, this message translates to:
  /// **'Lab'**
  String get mainPageLaboratory;

  /// No description provided for @mainPageOverview.
  ///
  /// In en, this message translates to:
  /// **'Overview'**
  String get mainPageOverview;

  /// No description provided for @mainPageProjects.
  ///
  /// In en, this message translates to:
  /// **'Projects'**
  String get mainPageProjects;

  /// No description provided for @mainPageSummary.
  ///
  /// In en, this message translates to:
  /// **'{page} · Overview'**
  String mainPageSummary(String page);

  /// No description provided for @mainPasteChanged.
  ///
  /// In en, this message translates to:
  /// **'The input changed while pasting. Please reopen the editor.'**
  String get mainPasteChanged;

  /// No description provided for @mainPasteContent.
  ///
  /// In en, this message translates to:
  /// **'Paste content'**
  String get mainPasteContent;

  /// No description provided for @mainPause.
  ///
  /// In en, this message translates to:
  /// **'Pause'**
  String get mainPause;

  /// No description provided for @mainPersonalWorkspace.
  ///
  /// In en, this message translates to:
  /// **'Personal workspace'**
  String get mainPersonalWorkspace;

  /// No description provided for @mainPlay.
  ///
  /// In en, this message translates to:
  /// **'Play'**
  String get mainPlay;

  /// No description provided for @mainPluginSettings.
  ///
  /// In en, this message translates to:
  /// **'Plugins and services'**
  String get mainPluginSettings;

  /// No description provided for @mainPluginSettingsSummary.
  ///
  /// In en, this message translates to:
  /// **'Built-in tools, extensions, network and file access, and content protection'**
  String get mainPluginSettingsSummary;

  /// No description provided for @mainPreviewEmpty.
  ///
  /// In en, this message translates to:
  /// **'Your preview will appear here'**
  String get mainPreviewEmpty;

  /// No description provided for @mainProgress.
  ///
  /// In en, this message translates to:
  /// **'Small steps · {done}/{total}'**
  String mainProgress(int done, int total);

  /// No description provided for @mainProjectIntro.
  ///
  /// In en, this message translates to:
  /// **'Use checklists to make progress. Every small step brings you closer.'**
  String get mainProjectIntro;

  /// No description provided for @mainQueryAgain.
  ///
  /// In en, this message translates to:
  /// **'Run a new query'**
  String get mainQueryAgain;

  /// No description provided for @mainQueryCapacity.
  ///
  /// In en, this message translates to:
  /// **'Query history is full'**
  String get mainQueryCapacity;

  /// No description provided for @mainQueryCapacityDetail.
  ///
  /// In en, this message translates to:
  /// **'Your content is preserved. This version cannot clear query history yet.'**
  String get mainQueryCapacityDetail;

  /// No description provided for @mainQueryLoading.
  ///
  /// In en, this message translates to:
  /// **'Finding ideas…'**
  String get mainQueryLoading;

  /// No description provided for @mainQueryRetry.
  ///
  /// In en, this message translates to:
  /// **'Retry query'**
  String get mainQueryRetry;

  /// No description provided for @mainQueryTerminated.
  ///
  /// In en, this message translates to:
  /// **'This query has ended'**
  String get mainQueryTerminated;

  /// No description provided for @mainQueryUnknown.
  ///
  /// In en, this message translates to:
  /// **'Results are not yet confirmed'**
  String get mainQueryUnknown;

  /// No description provided for @mainQuickHint.
  ///
  /// In en, this message translates to:
  /// **'What just came to mind?'**
  String get mainQuickHint;

  /// No description provided for @mainReadOnlySettings.
  ///
  /// In en, this message translates to:
  /// **'Content is read-only. Check the workbench plugin to restore editing.'**
  String get mainReadOnlySettings;

  /// No description provided for @mainRecentThoughts.
  ///
  /// In en, this message translates to:
  /// **'Recent thoughts'**
  String get mainRecentThoughts;

  /// No description provided for @mainRecordedCount.
  ///
  /// In en, this message translates to:
  /// **'With observations'**
  String get mainRecordedCount;

  /// No description provided for @mainRestoreDefault.
  ///
  /// In en, this message translates to:
  /// **'Reset to default'**
  String get mainRestoreDefault;

  /// No description provided for @mainRetry.
  ///
  /// In en, this message translates to:
  /// **'Retry'**
  String get mainRetry;

  /// No description provided for @mainRetrySave.
  ///
  /// In en, this message translates to:
  /// **'Retry save'**
  String get mainRetrySave;

  /// No description provided for @mainSage.
  ///
  /// In en, this message translates to:
  /// **'Sage'**
  String get mainSage;

  /// No description provided for @mainSampleBody0.
  ///
  /// In en, this message translates to:
  /// **'Keep sudden thoughts here.\nNo rush to finish—just let them begin.'**
  String get mainSampleBody0;

  /// No description provided for @mainSampleBody1.
  ///
  /// In en, this message translates to:
  /// **'A small page for favorite words,\nmusic and everyday details.'**
  String get mainSampleBody1;

  /// No description provided for @mainSampleBody2.
  ///
  /// In en, this message translates to:
  /// **'Try generative art. Let code grow\ninto unexpected shapes.'**
  String get mainSampleBody2;

  /// No description provided for @mainSampleBody3.
  ///
  /// In en, this message translates to:
  /// **'A quiet companion to help remember\nthe little things that slip away.'**
  String get mainSampleBody3;

  /// No description provided for @mainSampleTitle0.
  ///
  /// In en, this message translates to:
  /// **'A home for ideas'**
  String get mainSampleTitle0;

  /// No description provided for @mainSampleTitle1.
  ///
  /// In en, this message translates to:
  /// **'A quiet digital garden'**
  String get mainSampleTitle1;

  /// No description provided for @mainSampleTitle2.
  ///
  /// In en, this message translates to:
  /// **'Make something just for fun'**
  String get mainSampleTitle2;

  /// No description provided for @mainSampleTitle3.
  ///
  /// In en, this message translates to:
  /// **'My little desktop helper'**
  String get mainSampleTitle3;

  /// No description provided for @mainSampleTodo0.
  ///
  /// In en, this message translates to:
  /// **'Organize the first collection'**
  String get mainSampleTodo0;

  /// No description provided for @mainSampleTodo1.
  ///
  /// In en, this message translates to:
  /// **'Design the garden entrance'**
  String get mainSampleTodo1;

  /// No description provided for @mainSampleTodo2.
  ///
  /// In en, this message translates to:
  /// **'Plant a new idea'**
  String get mainSampleTodo2;

  /// No description provided for @mainSampleTodo3.
  ///
  /// In en, this message translates to:
  /// **'Sketch a small prototype'**
  String get mainSampleTodo3;

  /// No description provided for @mainSampleTodo4.
  ///
  /// In en, this message translates to:
  /// **'Design the reminder interaction'**
  String get mainSampleTodo4;

  /// No description provided for @mainSaveConnectionUnknown.
  ///
  /// In en, this message translates to:
  /// **'The connection was interrupted; the save result is unknown. Reopen the library to confirm before retrying.'**
  String get mainSaveConnectionUnknown;

  /// No description provided for @mainSaveFailed.
  ///
  /// In en, this message translates to:
  /// **'Could not save. Your changes remain in this session.'**
  String get mainSaveFailed;

  /// No description provided for @mainSaveIdea.
  ///
  /// In en, this message translates to:
  /// **'Save idea'**
  String get mainSaveIdea;

  /// No description provided for @mainSaveNotSubmitted.
  ///
  /// In en, this message translates to:
  /// **'Not submitted. Your draft and attachments are preserved; you can edit and save again.'**
  String get mainSaveNotSubmitted;

  /// No description provided for @mainSaveReadOnly.
  ///
  /// In en, this message translates to:
  /// **'Changes are not saved. Enable the workbench plugin in Plugins and services, then retry.'**
  String get mainSaveReadOnly;

  /// No description provided for @mainSaveReadbackPending.
  ///
  /// In en, this message translates to:
  /// **'Settings were committed, but readback is unconfirmed. Your draft is preserved; retry will reconcile the original submission first.'**
  String get mainSaveReadbackPending;

  /// No description provided for @mainSaveRecoveryAbandon.
  ///
  /// In en, this message translates to:
  /// **'Abandon old proposal'**
  String get mainSaveRecoveryAbandon;

  /// No description provided for @mainSaveRecoveryAbandonConfirm.
  ///
  /// In en, this message translates to:
  /// **'Abandon only the uncommitted proposal? Your current draft and saved content stay intact. This does not undo committed settings.'**
  String get mainSaveRecoveryAbandonConfirm;

  /// No description provided for @mainSaveRecoveryCommitted.
  ///
  /// In en, this message translates to:
  /// **'The old settings were committed. Reviewing confirms the result without replacing your current draft.'**
  String get mainSaveRecoveryCommitted;

  /// No description provided for @mainSaveRecoveryConflict.
  ///
  /// In en, this message translates to:
  /// **'The library has changed. The old proposal cannot overwrite newer settings. Keep it pending or abandon the uncommitted proposal.'**
  String get mainSaveRecoveryConflict;

  /// No description provided for @mainSaveRecoveryDone.
  ///
  /// In en, this message translates to:
  /// **'The old proposal is resolved. Your draft is unchanged; retry saving when ready.'**
  String get mainSaveRecoveryDone;

  /// No description provided for @mainSaveRecoveryEmpty.
  ///
  /// In en, this message translates to:
  /// **'No persistent save proposal needs review.'**
  String get mainSaveRecoveryEmpty;

  /// No description provided for @mainSaveRecoveryPending.
  ///
  /// In en, this message translates to:
  /// **'The old proposal is not confirmed. Continue to reconcile and attempt the original save; your newer draft stays intact.'**
  String get mainSaveRecoveryPending;

  /// No description provided for @mainSaveRecoveryResolve.
  ///
  /// In en, this message translates to:
  /// **'Resolve original save'**
  String get mainSaveRecoveryResolve;

  /// No description provided for @mainSaveRecoveryReview.
  ///
  /// In en, this message translates to:
  /// **'Review save'**
  String get mainSaveRecoveryReview;

  /// No description provided for @mainSaveRecoveryTitle.
  ///
  /// In en, this message translates to:
  /// **'A save needs review'**
  String get mainSaveRecoveryTitle;

  /// No description provided for @mainSaveUnknown.
  ///
  /// In en, this message translates to:
  /// **'This save is not yet confirmed. Your draft and attachments are preserved. Retry this submission; closing will refresh the workspace to check.'**
  String get mainSaveUnknown;

  /// No description provided for @mainSaving.
  ///
  /// In en, this message translates to:
  /// **'Saving…'**
  String get mainSaving;

  /// No description provided for @mainSearchHint.
  ///
  /// In en, this message translates to:
  /// **'Search your ideas…'**
  String get mainSearchHint;

  /// No description provided for @mainServiceRunSettings.
  ///
  /// In en, this message translates to:
  /// **'Service runtime'**
  String get mainServiceRunSettings;

  /// No description provided for @mainServiceSettings.
  ///
  /// In en, this message translates to:
  /// **'API services'**
  String get mainServiceSettings;

  /// No description provided for @mainSettings.
  ///
  /// In en, this message translates to:
  /// **'Settings'**
  String get mainSettings;

  /// No description provided for @mainShowAppearance.
  ///
  /// In en, this message translates to:
  /// **'Show appearance settings'**
  String get mainShowAppearance;

  /// No description provided for @mainSidebarMotto.
  ///
  /// In en, this message translates to:
  /// **'A little order. Room to wonder.'**
  String get mainSidebarMotto;

  /// No description provided for @mainSlowProgress.
  ///
  /// In en, this message translates to:
  /// **'Small steps still move you forward.'**
  String get mainSlowProgress;

  /// No description provided for @mainSolidCanvas.
  ///
  /// In en, this message translates to:
  /// **'Solid'**
  String get mainSolidCanvas;

  /// No description provided for @mainSolidDetail.
  ///
  /// In en, this message translates to:
  /// **'A calm, solid-color canvas.'**
  String get mainSolidDetail;

  /// No description provided for @mainSolidOpacity.
  ///
  /// In en, this message translates to:
  /// **'100% · Solid'**
  String get mainSolidOpacity;

  /// No description provided for @mainSortFavorites.
  ///
  /// In en, this message translates to:
  /// **'Favorites first'**
  String get mainSortFavorites;

  /// No description provided for @mainSortRecent.
  ///
  /// In en, this message translates to:
  /// **'Recently added'**
  String get mainSortRecent;

  /// No description provided for @mainSortTitle.
  ///
  /// In en, this message translates to:
  /// **'By title'**
  String get mainSortTitle;

  /// No description provided for @mainSquareCorners.
  ///
  /// In en, this message translates to:
  /// **'Set to 0 for square corners'**
  String get mainSquareCorners;

  /// No description provided for @mainStageActive.
  ///
  /// In en, this message translates to:
  /// **'In progress'**
  String get mainStageActive;

  /// No description provided for @mainStageCompleted.
  ///
  /// In en, this message translates to:
  /// **'Completed'**
  String get mainStageCompleted;

  /// No description provided for @mainStageOrganized.
  ///
  /// In en, this message translates to:
  /// **'Organized'**
  String get mainStageOrganized;

  /// No description provided for @mainStagePlanned.
  ///
  /// In en, this message translates to:
  /// **'Planned'**
  String get mainStagePlanned;

  /// No description provided for @mainStageRecorded.
  ///
  /// In en, this message translates to:
  /// **'Recorded'**
  String get mainStageRecorded;

  /// No description provided for @mainStageTooltip.
  ///
  /// In en, this message translates to:
  /// **'Change stage for {title}'**
  String mainStageTooltip(String title);

  /// No description provided for @mainStageUnsorted.
  ///
  /// In en, this message translates to:
  /// **'To organize'**
  String get mainStageUnsorted;

  /// No description provided for @mainStageUnverified.
  ///
  /// In en, this message translates to:
  /// **'To test'**
  String get mainStageUnverified;

  /// No description provided for @mainStageVerifying.
  ///
  /// In en, this message translates to:
  /// **'Testing'**
  String get mainStageVerifying;

  /// No description provided for @mainStayCurious.
  ///
  /// In en, this message translates to:
  /// **'STAY CURIOUS. STAY YOU.'**
  String get mainStayCurious;

  /// No description provided for @mainSteps.
  ///
  /// In en, this message translates to:
  /// **'{done}/{total} steps'**
  String mainSteps(int done, int total);

  /// No description provided for @mainStorageUnavailable.
  ///
  /// In en, this message translates to:
  /// **'Local storage is unavailable. Changes will last for this session only.'**
  String get mainStorageUnavailable;

  /// No description provided for @mainStorageUnreadable.
  ///
  /// In en, this message translates to:
  /// **'Saved content could not be read. The original data is preserved and will not be overwritten.'**
  String get mainStorageUnreadable;

  /// Experimental interface style
  ///
  /// In en, this message translates to:
  /// **'Brutalist'**
  String get mainStyleBrutalist;

  /// Experimental interface style
  ///
  /// In en, this message translates to:
  /// **'Hard edges, bold borders and offset shadows'**
  String get mainStyleBrutalistDescription;

  /// Experimental interface style
  ///
  /// In en, this message translates to:
  /// **'Clay'**
  String get mainStyleClay;

  /// Experimental interface style
  ///
  /// In en, this message translates to:
  /// **'Rounded forms with softly raised color'**
  String get mainStyleClayDescription;

  /// Visual style relief depth control
  ///
  /// In en, this message translates to:
  /// **'Relief depth'**
  String get mainStyleDepth;

  /// Visual style relief depth control
  ///
  /// In en, this message translates to:
  /// **'Adjust raised and inset depth without changing glass opacity.'**
  String get mainStyleDepthGuide;

  /// Visual style relief depth control
  ///
  /// In en, this message translates to:
  /// **'Reset to 100%'**
  String get mainStyleDepthReset;

  /// Experimental interface style
  ///
  /// In en, this message translates to:
  /// **'Experimental'**
  String get mainStyleExperimental;

  /// Interface style and component material inheritance
  ///
  /// In en, this message translates to:
  /// **'Flat · Default'**
  String get mainStyleFlat;

  /// Interface style and component material inheritance
  ///
  /// In en, this message translates to:
  /// **'Light edges and clear layers'**
  String get mainStyleFlatDescription;

  /// Experimental interface style
  ///
  /// In en, this message translates to:
  /// **'Fluent'**
  String get mainStyleFluent;

  /// Experimental interface style
  ///
  /// In en, this message translates to:
  /// **'Layered translucent surfaces and accent lines'**
  String get mainStyleFluentDescription;

  /// Experimental interface style
  ///
  /// In en, this message translates to:
  /// **'Industrial'**
  String get mainStyleIndustrial;

  /// Experimental interface style
  ///
  /// In en, this message translates to:
  /// **'Metal-like panels, compact controls and precise lines'**
  String get mainStyleIndustrialDescription;

  /// Interface style and component material inheritance
  ///
  /// In en, this message translates to:
  /// **'Neumorphism'**
  String get mainStyleNeumorphism;

  /// Interface style and component material inheritance
  ///
  /// In en, this message translates to:
  /// **'Soft paired shadows and gentle relief'**
  String get mainStyleNeumorphismDescription;

  /// Experimental interface style
  ///
  /// In en, this message translates to:
  /// **'Paper'**
  String get mainStylePaper;

  /// Experimental interface style
  ///
  /// In en, this message translates to:
  /// **'Matte sheets, fine borders and restrained depth'**
  String get mainStylePaperDescription;

  /// No description provided for @mainTaskAdd.
  ///
  /// In en, this message translates to:
  /// **'Add task'**
  String get mainTaskAdd;

  /// No description provided for @mainTaskAmbiguousDecision.
  ///
  /// In en, this message translates to:
  /// **'The completion of these legacy tasks with matching names is uncertain. Confirm each task separately.'**
  String get mainTaskAmbiguousDecision;

  /// No description provided for @mainTaskCompleteAllAndSetStage.
  ///
  /// In en, this message translates to:
  /// **'Complete all and set stage'**
  String get mainTaskCompleteAllAndSetStage;

  /// No description provided for @mainTaskCompleteAllConfirm.
  ///
  /// In en, this message translates to:
  /// **'Mark every task complete and set the stage to “{stage}”?'**
  String mainTaskCompleteAllConfirm(String stage);

  /// No description provided for @mainTaskLegacyReadOnly.
  ///
  /// In en, this message translates to:
  /// **'Legacy tasks are matched by text. After upgrading, tasks with matching names can be managed separately.'**
  String get mainTaskLegacyReadOnly;

  /// No description provided for @mainTaskMarkComplete.
  ///
  /// In en, this message translates to:
  /// **'Confirm complete'**
  String get mainTaskMarkComplete;

  /// No description provided for @mainTaskMarkIncomplete.
  ///
  /// In en, this message translates to:
  /// **'Confirm incomplete'**
  String get mainTaskMarkIncomplete;

  /// No description provided for @mainTaskMoveDown.
  ///
  /// In en, this message translates to:
  /// **'Move down'**
  String get mainTaskMoveDown;

  /// No description provided for @mainTaskMoveUp.
  ///
  /// In en, this message translates to:
  /// **'Move up'**
  String get mainTaskMoveUp;

  /// No description provided for @mainTaskProgressThreeWay.
  ///
  /// In en, this message translates to:
  /// **'{complete} complete · {incomplete} incomplete · {ambiguous} to confirm'**
  String mainTaskProgressThreeWay(int ambiguous, int complete, int incomplete);

  /// No description provided for @mainTaskRemoveConfirm.
  ///
  /// In en, this message translates to:
  /// **'Remove task “{task}”?'**
  String mainTaskRemoveConfirm(String task);

  /// No description provided for @mainTaskRename.
  ///
  /// In en, this message translates to:
  /// **'Rename'**
  String get mainTaskRename;

  /// No description provided for @mainTaskSetStage.
  ///
  /// In en, this message translates to:
  /// **'Change stage only'**
  String get mainTaskSetStage;

  /// No description provided for @mainTaskStagePrompt.
  ///
  /// In en, this message translates to:
  /// **'Project stage'**
  String get mainTaskStagePrompt;

  /// No description provided for @mainTaskTextPrompt.
  ///
  /// In en, this message translates to:
  /// **'Task text'**
  String get mainTaskTextPrompt;

  /// No description provided for @mainTenMinutesAgo.
  ///
  /// In en, this message translates to:
  /// **'10 minutes ago'**
  String get mainTenMinutesAgo;

  /// No description provided for @mainTextureCanvas.
  ///
  /// In en, this message translates to:
  /// **'Texture'**
  String get mainTextureCanvas;

  /// No description provided for @mainTextureDetail.
  ///
  /// In en, this message translates to:
  /// **'Fine paper-like texture adds a tactile feel.'**
  String get mainTextureDetail;

  /// No description provided for @mainThemeCompass.
  ///
  /// In en, this message translates to:
  /// **'Theme color wheel'**
  String get mainThemeCompass;

  /// No description provided for @mainThemeGrayscale.
  ///
  /// In en, this message translates to:
  /// **'Theme grayscale'**
  String get mainThemeGrayscale;

  /// No description provided for @mainThemeTone.
  ///
  /// In en, this message translates to:
  /// **'Theme colors'**
  String get mainThemeTone;

  /// No description provided for @mainThreeHoursAgo.
  ///
  /// In en, this message translates to:
  /// **'3 hours ago'**
  String get mainThreeHoursAgo;

  /// No description provided for @mainTintOpacity.
  ///
  /// In en, this message translates to:
  /// **'Tint opacity'**
  String get mainTintOpacity;

  /// No description provided for @mainToProject.
  ///
  /// In en, this message translates to:
  /// **'Move to projects'**
  String get mainToProject;

  /// No description provided for @mainTodosPrompt.
  ///
  /// In en, this message translates to:
  /// **'Next steps (one per line, optional)'**
  String get mainTodosPrompt;

  /// No description provided for @mainTransparencyUnavailable.
  ///
  /// In en, this message translates to:
  /// **'System transparency could not be enabled. You can use the default background instead.'**
  String get mainTransparencyUnavailable;

  /// No description provided for @mainTransparentCanvas.
  ///
  /// In en, this message translates to:
  /// **'Transparent'**
  String get mainTransparentCanvas;

  /// No description provided for @mainTransparentDetail.
  ///
  /// In en, this message translates to:
  /// **'See the space behind the window; on the web, see the host background.'**
  String get mainTransparentDetail;

  /// No description provided for @mainUndo.
  ///
  /// In en, this message translates to:
  /// **'Undo'**
  String get mainUndo;

  /// No description provided for @mainUnfavoriteTooltip.
  ///
  /// In en, this message translates to:
  /// **'Unfavorite {title}'**
  String mainUnfavoriteTooltip(String title);

  /// No description provided for @mainUnsortedCount.
  ///
  /// In en, this message translates to:
  /// **'To organize'**
  String get mainUnsortedCount;

  /// No description provided for @mainUnverifiedCount.
  ///
  /// In en, this message translates to:
  /// **'To test'**
  String get mainUnverifiedCount;

  /// No description provided for @mainView.
  ///
  /// In en, this message translates to:
  /// **'View'**
  String get mainView;

  /// No description provided for @mainViewAll.
  ///
  /// In en, this message translates to:
  /// **'Show all'**
  String get mainViewAll;

  /// Interface style and component material inheritance
  ///
  /// In en, this message translates to:
  /// **'Interface style'**
  String get mainVisualStyle;

  /// No description provided for @mainWarmSand.
  ///
  /// In en, this message translates to:
  /// **'Warm sand'**
  String get mainWarmSand;

  /// No description provided for @mainWhiteTheme.
  ///
  /// In en, this message translates to:
  /// **'White'**
  String get mainWhiteTheme;

  /// No description provided for @mainWindowRadius.
  ///
  /// In en, this message translates to:
  /// **'Window corners'**
  String get mainWindowRadius;

  /// No description provided for @mainWindowRadiusDetail.
  ///
  /// In en, this message translates to:
  /// **'Adjust the window edge separately; it becomes square when maximized'**
  String get mainWindowRadiusDetail;

  /// No description provided for @mainWindowsFrostOnly.
  ///
  /// In en, this message translates to:
  /// **'Desktop blur is available on Windows only'**
  String get mainWindowsFrostOnly;

  /// No description provided for @mainWorkbench.
  ///
  /// In en, this message translates to:
  /// **'Workspace'**
  String get mainWorkbench;

  /// No description provided for @mainWorkbenchPlugin.
  ///
  /// In en, this message translates to:
  /// **'Workspace plugin'**
  String get mainWorkbenchPlugin;

  /// No description provided for @mainWriteHypothesis.
  ///
  /// In en, this message translates to:
  /// **'Open the record and write down what you want to test.'**
  String get mainWriteHypothesis;

  /// No description provided for @mainYesterday.
  ///
  /// In en, this message translates to:
  /// **'Yesterday'**
  String get mainYesterday;

  /// No description provided for @pluginsApprovalUnknown.
  ///
  /// In en, this message translates to:
  /// **'Activation or permission changes could not be confirmed'**
  String get pluginsApprovalUnknown;

  /// No description provided for @pluginsApproveEnable.
  ///
  /// In en, this message translates to:
  /// **'Approve and enable'**
  String get pluginsApproveEnable;

  /// No description provided for @pluginsApproveWorkbench.
  ///
  /// In en, this message translates to:
  /// **'Allow reading and editing, and enable'**
  String get pluginsApproveWorkbench;

  /// No description provided for @pluginsAttachment.
  ///
  /// In en, this message translates to:
  /// **'Read attachments'**
  String get pluginsAttachment;

  /// No description provided for @pluginsBackingUp.
  ///
  /// In en, this message translates to:
  /// **'Backing up…'**
  String get pluginsBackingUp;

  /// No description provided for @pluginsBackupLibrary.
  ///
  /// In en, this message translates to:
  /// **'Back up library'**
  String get pluginsBackupLibrary;

  /// No description provided for @pluginsBackupLibraryType.
  ///
  /// In en, this message translates to:
  /// **'Library backup'**
  String get pluginsBackupLibraryType;

  /// No description provided for @pluginsBackupProtection.
  ///
  /// In en, this message translates to:
  /// **'Back up protection file'**
  String get pluginsBackupProtection;

  /// No description provided for @pluginsBackupUnknown.
  ///
  /// In en, this message translates to:
  /// **'Backup is not yet confirmed. Keep any file that was created and check the destination.'**
  String get pluginsBackupUnknown;

  /// No description provided for @pluginsBinaryPreview.
  ///
  /// In en, this message translates to:
  /// **'Binary content: {hex}'**
  String pluginsBinaryPreview(String hex);

  /// No description provided for @pluginsBuiltin.
  ///
  /// In en, this message translates to:
  /// **'Built-in workbench'**
  String get pluginsBuiltin;

  /// First-party workbench text tool display only
  ///
  /// In en, this message translates to:
  /// **'{count, plural, =1{1 character} other{{count} characters}} · This session only; not saved as a card'**
  String pluginsBuiltinCount(int count);

  /// First-party workbench text tool display only
  ///
  /// In en, this message translates to:
  /// **'Enter text to preview uppercase'**
  String get pluginsBuiltinEmpty;

  /// First-party workbench text tool display only
  ///
  /// In en, this message translates to:
  /// **'Text tools'**
  String get pluginsBuiltinHeading;

  /// First-party workbench text tool display only
  ///
  /// In en, this message translates to:
  /// **'Enter text'**
  String get pluginsBuiltinInput;

  /// No description provided for @pluginsCancel.
  ///
  /// In en, this message translates to:
  /// **'Cancel'**
  String get pluginsCancel;

  /// No description provided for @pluginsChoosePackage.
  ///
  /// In en, this message translates to:
  /// **'Choose plugin file'**
  String get pluginsChoosePackage;

  /// No description provided for @pluginsChooseSmallFile.
  ///
  /// In en, this message translates to:
  /// **'Choose a small file'**
  String get pluginsChooseSmallFile;

  /// No description provided for @pluginsCloseTextTool.
  ///
  /// In en, this message translates to:
  /// **'Hide text tool'**
  String get pluginsCloseTextTool;

  /// No description provided for @pluginsCloseUnknown.
  ///
  /// In en, this message translates to:
  /// **'Closing the plugin view could not be confirmed'**
  String get pluginsCloseUnknown;

  /// No description provided for @pluginsCloseView.
  ///
  /// In en, this message translates to:
  /// **'Close view'**
  String get pluginsCloseView;

  /// No description provided for @pluginsConnectionLost.
  ///
  /// In en, this message translates to:
  /// **'Connection interrupted. Reopen this plugin view.'**
  String get pluginsConnectionLost;

  /// No description provided for @pluginsContentPermissions.
  ///
  /// In en, this message translates to:
  /// **'Content permissions'**
  String get pluginsContentPermissions;

  /// No description provided for @pluginsCreate.
  ///
  /// In en, this message translates to:
  /// **'Create content'**
  String get pluginsCreate;

  /// No description provided for @pluginsCredentialCancel.
  ///
  /// In en, this message translates to:
  /// **'Close form'**
  String get pluginsCredentialCancel;

  /// No description provided for @pluginsCredentialCreateTitle.
  ///
  /// In en, this message translates to:
  /// **'New credential'**
  String get pluginsCredentialCreateTitle;

  /// No description provided for @pluginsCredentialDays.
  ///
  /// In en, this message translates to:
  /// **'{days, plural, =1{1 day} other{{days} days}}'**
  String pluginsCredentialDays(int days);

  /// No description provided for @pluginsCredentialDetails.
  ///
  /// In en, this message translates to:
  /// **'Store credentials securely for approved API connections. Saving a credential does not approve a server or enable a plugin. Saved secrets cannot be viewed.'**
  String get pluginsCredentialDetails;

  /// No description provided for @pluginsCredentialDisable.
  ///
  /// In en, this message translates to:
  /// **'Disable'**
  String get pluginsCredentialDisable;

  /// No description provided for @pluginsCredentialDisabled.
  ///
  /// In en, this message translates to:
  /// **'Disabled'**
  String get pluginsCredentialDisabled;

  /// No description provided for @pluginsCredentialDisabledDone.
  ///
  /// In en, this message translates to:
  /// **'Credential disabled.'**
  String get pluginsCredentialDisabledDone;

  /// No description provided for @pluginsCredentialEmpty.
  ///
  /// In en, this message translates to:
  /// **'No saved credentials'**
  String get pluginsCredentialEmpty;

  /// No description provided for @pluginsCredentialExpired.
  ///
  /// In en, this message translates to:
  /// **'Expired'**
  String get pluginsCredentialExpired;

  /// No description provided for @pluginsCredentialExpires.
  ///
  /// In en, this message translates to:
  /// **'Expires: {date}'**
  String pluginsCredentialExpires(String date);

  /// No description provided for @pluginsCredentialHeader.
  ///
  /// In en, this message translates to:
  /// **'Header name'**
  String get pluginsCredentialHeader;

  /// No description provided for @pluginsCredentialInvalid.
  ///
  /// In en, this message translates to:
  /// **'Check the header name and enter a new secret value. The secret field has been cleared.'**
  String get pluginsCredentialInvalid;

  /// No description provided for @pluginsCredentialLifetime.
  ///
  /// In en, this message translates to:
  /// **'Valid for'**
  String get pluginsCredentialLifetime;

  /// No description provided for @pluginsCredentialLoadFailed.
  ///
  /// In en, this message translates to:
  /// **'Credentials could not be read consistently. Refresh status to try again.'**
  String get pluginsCredentialLoadFailed;

  /// No description provided for @pluginsCredentialNew.
  ///
  /// In en, this message translates to:
  /// **'Add credential'**
  String get pluginsCredentialNew;

  /// No description provided for @pluginsCredentialReading.
  ///
  /// In en, this message translates to:
  /// **'Reading credentials…'**
  String get pluginsCredentialReading;

  /// No description provided for @pluginsCredentialReference.
  ///
  /// In en, this message translates to:
  /// **'Credential {reference}'**
  String pluginsCredentialReference(String reference);

  /// No description provided for @pluginsCredentialRefresh.
  ///
  /// In en, this message translates to:
  /// **'Refresh status'**
  String get pluginsCredentialRefresh;

  /// No description provided for @pluginsCredentialReplace.
  ///
  /// In en, this message translates to:
  /// **'Replace secret'**
  String get pluginsCredentialReplace;

  /// No description provided for @pluginsCredentialReplaceTitle.
  ///
  /// In en, this message translates to:
  /// **'Replace credential {reference}'**
  String pluginsCredentialReplaceTitle(String reference);

  /// No description provided for @pluginsCredentialSave.
  ///
  /// In en, this message translates to:
  /// **'Save credential'**
  String get pluginsCredentialSave;

  /// No description provided for @pluginsCredentialSaved.
  ///
  /// In en, this message translates to:
  /// **'Credential saved. API connections still need separate approval.'**
  String get pluginsCredentialSaved;

  /// No description provided for @pluginsCredentialSecret.
  ///
  /// In en, this message translates to:
  /// **'New secret value'**
  String get pluginsCredentialSecret;

  /// No description provided for @pluginsCredentialStored.
  ///
  /// In en, this message translates to:
  /// **'Saved'**
  String get pluginsCredentialStored;

  /// No description provided for @pluginsCredentialTitle.
  ///
  /// In en, this message translates to:
  /// **'API credentials'**
  String get pluginsCredentialTitle;

  /// No description provided for @pluginsCredentialUnknown.
  ///
  /// In en, this message translates to:
  /// **'The result could not be confirmed. The secret field has been cleared. Refresh status before making another change.'**
  String get pluginsCredentialUnknown;

  /// No description provided for @pluginsDeclared.
  ///
  /// In en, this message translates to:
  /// **'Declared permissions: {permissions}'**
  String pluginsDeclared(String permissions);

  /// No description provided for @pluginsDependenciesNotice.
  ///
  /// In en, this message translates to:
  /// **'Dependencies must be configured in the host. This page does not approve them.'**
  String get pluginsDependenciesNotice;

  /// No description provided for @pluginsDisable.
  ///
  /// In en, this message translates to:
  /// **'Disable'**
  String get pluginsDisable;

  /// No description provided for @pluginsDisableWorkbench.
  ///
  /// In en, this message translates to:
  /// **'Disable workbench plugin'**
  String get pluginsDisableWorkbench;

  /// No description provided for @pluginsDisabled.
  ///
  /// In en, this message translates to:
  /// **'Disabled'**
  String get pluginsDisabled;

  /// No description provided for @pluginsDisabledDetails.
  ///
  /// In en, this message translates to:
  /// **'Not enabled. Allow reading and editing workbench content to use the editor and tools.'**
  String get pluginsDisabledDetails;

  /// No description provided for @pluginsEdit.
  ///
  /// In en, this message translates to:
  /// **'Edit content'**
  String get pluginsEdit;

  /// No description provided for @pluginsEmptyLibrary.
  ///
  /// In en, this message translates to:
  /// **'No third-party plugins imported yet.'**
  String get pluginsEmptyLibrary;

  /// No description provided for @pluginsEmptyResult.
  ///
  /// In en, this message translates to:
  /// **'(empty result)'**
  String get pluginsEmptyResult;

  /// No description provided for @pluginsEnabled.
  ///
  /// In en, this message translates to:
  /// **'Enabled'**
  String get pluginsEnabled;

  /// No description provided for @pluginsEnabledDetails.
  ///
  /// In en, this message translates to:
  /// **'Enabled. This plugin can read and edit workbench content. Disabling it preserves your data.'**
  String get pluginsEnabledDetails;

  /// No description provided for @pluginsEndpointAdvanced.
  ///
  /// In en, this message translates to:
  /// **'Policy limits (bytes unless stated otherwise)'**
  String get pluginsEndpointAdvanced;

  /// No description provided for @pluginsEndpointCertificate.
  ///
  /// In en, this message translates to:
  /// **'Choose DER trust root'**
  String get pluginsEndpointCertificate;

  /// No description provided for @pluginsEndpointCertificateDetails.
  ///
  /// In en, this message translates to:
  /// **'Optional trust root for HTTPS: one binary DER certificate (.der or .cer), up to 32 KiB. PEM and certificate bundles are not accepted. Remove the trust root before switching to HTTP.'**
  String get pluginsEndpointCertificateDetails;

  /// No description provided for @pluginsEndpointCertificateInvalid.
  ///
  /// In en, this message translates to:
  /// **'Choose one valid binary DER certificate (.der or .cer) no larger than 32 KiB.'**
  String get pluginsEndpointCertificateInvalid;

  /// No description provided for @pluginsEndpointCertificateSelected.
  ///
  /// In en, this message translates to:
  /// **'DER trust root selected ({bytes} bytes)'**
  String pluginsEndpointCertificateSelected(int bytes);

  /// No description provided for @pluginsEndpointConcurrency.
  ///
  /// In en, this message translates to:
  /// **'Concurrent requests (1–128)'**
  String get pluginsEndpointConcurrency;

  /// No description provided for @pluginsEndpointCreateTitle.
  ///
  /// In en, this message translates to:
  /// **'New endpoint approval'**
  String get pluginsEndpointCreateTitle;

  /// No description provided for @pluginsEndpointCredential.
  ///
  /// In en, this message translates to:
  /// **'Credential reference'**
  String get pluginsEndpointCredential;

  /// No description provided for @pluginsEndpointCredentialLifetime.
  ///
  /// In en, this message translates to:
  /// **'The selected credential must remain valid for the full endpoint lifetime. Its expiration will not be extended.'**
  String get pluginsEndpointCredentialLifetime;

  /// No description provided for @pluginsEndpointCredentialUnavailable.
  ///
  /// In en, this message translates to:
  /// **'Credentials require the package’s declared and approved credential-use permission and a valid saved reference.'**
  String get pluginsEndpointCredentialUnavailable;

  /// No description provided for @pluginsEndpointCredentialsFailed.
  ///
  /// In en, this message translates to:
  /// **'Credential references could not be read. Refresh status before choosing a credential.'**
  String get pluginsEndpointCredentialsFailed;

  /// No description provided for @pluginsEndpointDetails.
  ///
  /// In en, this message translates to:
  /// **'Save a server policy for a specific package and digest. Saving does not connect to the network, enable a plugin, or make network tasks immediately available.'**
  String get pluginsEndpointDetails;

  /// No description provided for @pluginsEndpointDigest.
  ///
  /// In en, this message translates to:
  /// **'Package digest'**
  String get pluginsEndpointDigest;

  /// No description provided for @pluginsEndpointDisabledDone.
  ///
  /// In en, this message translates to:
  /// **'Endpoint approval disabled.'**
  String get pluginsEndpointDisabledDone;

  /// No description provided for @pluginsEndpointEmpty.
  ///
  /// In en, this message translates to:
  /// **'No saved endpoint approvals'**
  String get pluginsEndpointEmpty;

  /// No description provided for @pluginsEndpointFrameBytes.
  ///
  /// In en, this message translates to:
  /// **'Frame budget (1–131072 bytes)'**
  String get pluginsEndpointFrameBytes;

  /// No description provided for @pluginsEndpointHeaderBytes.
  ///
  /// In en, this message translates to:
  /// **'Maximum header bytes (1–16384)'**
  String get pluginsEndpointHeaderBytes;

  /// No description provided for @pluginsEndpointInvalid.
  ///
  /// In en, this message translates to:
  /// **'Check the package, origin, methods, 1–30 day lifetime, credential permission, certificate, and policy limits.'**
  String get pluginsEndpointInvalid;

  /// No description provided for @pluginsEndpointLifetime.
  ///
  /// In en, this message translates to:
  /// **'Valid for (1–30 days)'**
  String get pluginsEndpointLifetime;

  /// No description provided for @pluginsEndpointLoadFailed.
  ///
  /// In en, this message translates to:
  /// **'Endpoint approvals could not be read consistently. Refresh status to try again.'**
  String get pluginsEndpointLoadFailed;

  /// No description provided for @pluginsEndpointLocalHttp.
  ///
  /// In en, this message translates to:
  /// **'Local HTTP'**
  String get pluginsEndpointLocalHttp;

  /// No description provided for @pluginsEndpointLocalHttps.
  ///
  /// In en, this message translates to:
  /// **'Local HTTPS'**
  String get pluginsEndpointLocalHttps;

  /// No description provided for @pluginsEndpointMethods.
  ///
  /// In en, this message translates to:
  /// **'Allowed request methods'**
  String get pluginsEndpointMethods;

  /// No description provided for @pluginsEndpointNew.
  ///
  /// In en, this message translates to:
  /// **'Add endpoint'**
  String get pluginsEndpointNew;

  /// No description provided for @pluginsEndpointNoCredential.
  ///
  /// In en, this message translates to:
  /// **'No credential'**
  String get pluginsEndpointNoCredential;

  /// No description provided for @pluginsEndpointOrigin.
  ///
  /// In en, this message translates to:
  /// **'Origin only, for example https://api.example.com'**
  String get pluginsEndpointOrigin;

  /// No description provided for @pluginsEndpointPackage.
  ///
  /// In en, this message translates to:
  /// **'Package'**
  String get pluginsEndpointPackage;

  /// No description provided for @pluginsEndpointPackageUnavailable.
  ///
  /// In en, this message translates to:
  /// **'This package is unavailable or has no approved HTTP permission. Existing approvals can still be disabled.'**
  String get pluginsEndpointPackageUnavailable;

  /// No description provided for @pluginsEndpointProfile.
  ///
  /// In en, this message translates to:
  /// **'Connection profile'**
  String get pluginsEndpointProfile;

  /// No description provided for @pluginsEndpointPublicHttps.
  ///
  /// In en, this message translates to:
  /// **'Public HTTPS'**
  String get pluginsEndpointPublicHttps;

  /// No description provided for @pluginsEndpointRemoveCertificate.
  ///
  /// In en, this message translates to:
  /// **'Remove trust root'**
  String get pluginsEndpointRemoveCertificate;

  /// No description provided for @pluginsEndpointReplace.
  ///
  /// In en, this message translates to:
  /// **'Replace approval'**
  String get pluginsEndpointReplace;

  /// No description provided for @pluginsEndpointReplaceTitle.
  ///
  /// In en, this message translates to:
  /// **'Replace endpoint approval using the current package digest'**
  String get pluginsEndpointReplaceTitle;

  /// No description provided for @pluginsEndpointRequestBytes.
  ///
  /// In en, this message translates to:
  /// **'Maximum request bytes (1–65536)'**
  String get pluginsEndpointRequestBytes;

  /// No description provided for @pluginsEndpointResponseBytes.
  ///
  /// In en, this message translates to:
  /// **'Maximum response bytes (1–65536)'**
  String get pluginsEndpointResponseBytes;

  /// No description provided for @pluginsEndpointSave.
  ///
  /// In en, this message translates to:
  /// **'Save endpoint approval'**
  String get pluginsEndpointSave;

  /// No description provided for @pluginsEndpointSaved.
  ///
  /// In en, this message translates to:
  /// **'Endpoint approval saved. No network connection was made.'**
  String get pluginsEndpointSaved;

  /// No description provided for @pluginsEndpointTimeout.
  ///
  /// In en, this message translates to:
  /// **'Timeout (1–30000 milliseconds)'**
  String get pluginsEndpointTimeout;

  /// No description provided for @pluginsEndpointTitle.
  ///
  /// In en, this message translates to:
  /// **'API endpoint approvals'**
  String get pluginsEndpointTitle;

  /// No description provided for @pluginsEndpointUnknown.
  ///
  /// In en, this message translates to:
  /// **'The result could not be confirmed. Refresh status before making another change. The request will not be sent again automatically.'**
  String get pluginsEndpointUnknown;

  /// No description provided for @pluginsEndpointWorking.
  ///
  /// In en, this message translates to:
  /// **'Updating endpoint status…'**
  String get pluginsEndpointWorking;

  /// No description provided for @pluginsExistingVersion.
  ///
  /// In en, this message translates to:
  /// **'This version is already installed. Its enabled state is unchanged.'**
  String get pluginsExistingVersion;

  /// No description provided for @pluginsFileLimit.
  ///
  /// In en, this message translates to:
  /// **'The file is too large. Choose a file no larger than {limit} bytes.'**
  String pluginsFileLimit(int limit);

  /// No description provided for @pluginsHttpTaskAbandon.
  ///
  /// In en, this message translates to:
  /// **'End observation of this attempt'**
  String get pluginsHttpTaskAbandon;

  /// No description provided for @pluginsHttpTaskAbandonDetails.
  ///
  /// In en, this message translates to:
  /// **'Only after fresh status confirms no active task and the original library is available, you may end this observation. This does not prove that no remote effects occurred. The identity and uncertainty remain in history; a new request requires another explicit submission.'**
  String get pluginsHttpTaskAbandonDetails;

  /// No description provided for @pluginsHttpTaskAbsent.
  ///
  /// In en, this message translates to:
  /// **'No result delivery'**
  String get pluginsHttpTaskAbsent;

  /// No description provided for @pluginsHttpTaskAccepted.
  ///
  /// In en, this message translates to:
  /// **'Accepted'**
  String get pluginsHttpTaskAccepted;

  /// No description provided for @pluginsHttpTaskAcknowledge.
  ///
  /// In en, this message translates to:
  /// **'Acknowledge finished task'**
  String get pluginsHttpTaskAcknowledge;

  /// No description provided for @pluginsHttpTaskArchivedUnknown.
  ///
  /// In en, this message translates to:
  /// **'Observation ended by the user. The previous remote effects remain unconfirmed; this attempt was not replayed.'**
  String get pluginsHttpTaskArchivedUnknown;

  /// No description provided for @pluginsHttpTaskBase64.
  ///
  /// In en, this message translates to:
  /// **'Base64'**
  String get pluginsHttpTaskBase64;

  /// No description provided for @pluginsHttpTaskBody.
  ///
  /// In en, this message translates to:
  /// **'Request body'**
  String get pluginsHttpTaskBody;

  /// No description provided for @pluginsHttpTaskBodyFormat.
  ///
  /// In en, this message translates to:
  /// **'Request body encoding'**
  String get pluginsHttpTaskBodyFormat;

  /// No description provided for @pluginsHttpTaskBusy.
  ///
  /// In en, this message translates to:
  /// **'Busy'**
  String get pluginsHttpTaskBusy;

  /// No description provided for @pluginsHttpTaskCancel.
  ///
  /// In en, this message translates to:
  /// **'Request cancellation'**
  String get pluginsHttpTaskCancel;

  /// No description provided for @pluginsHttpTaskCancelled.
  ///
  /// In en, this message translates to:
  /// **'Cancellation observed; remote effects may still have occurred'**
  String get pluginsHttpTaskCancelled;

  /// No description provided for @pluginsHttpTaskCatalogUnavailable.
  ///
  /// In en, this message translates to:
  /// **'Plugin catalog status is unavailable. Refresh the plugin library before a new submission; existing task controls remain available.'**
  String get pluginsHttpTaskCatalogUnavailable;

  /// No description provided for @pluginsHttpTaskClosed.
  ///
  /// In en, this message translates to:
  /// **'Closed'**
  String get pluginsHttpTaskClosed;

  /// No description provided for @pluginsHttpTaskCompleted.
  ///
  /// In en, this message translates to:
  /// **'Completed'**
  String get pluginsHttpTaskCompleted;

  /// No description provided for @pluginsHttpTaskConflict.
  ///
  /// In en, this message translates to:
  /// **'Conflict'**
  String get pluginsHttpTaskConflict;

  /// No description provided for @pluginsHttpTaskConsumed.
  ///
  /// In en, this message translates to:
  /// **'Result consumed'**
  String get pluginsHttpTaskConsumed;

  /// No description provided for @pluginsHttpTaskControlUnknown.
  ///
  /// In en, this message translates to:
  /// **'The control result could not be confirmed. Refresh task status before deciding the next action.'**
  String get pluginsHttpTaskControlUnknown;

  /// No description provided for @pluginsHttpTaskCounters.
  ///
  /// In en, this message translates to:
  /// **'IO calls: {calls}; charged bytes: {bytes}'**
  String pluginsHttpTaskCounters(String bytes, String calls);

  /// No description provided for @pluginsHttpTaskDeadline.
  ///
  /// In en, this message translates to:
  /// **'Deadline exceeded'**
  String get pluginsHttpTaskDeadline;

  /// No description provided for @pluginsHttpTaskDenied.
  ///
  /// In en, this message translates to:
  /// **'Denied'**
  String get pluginsHttpTaskDenied;

  /// No description provided for @pluginsHttpTaskDetails.
  ///
  /// In en, this message translates to:
  /// **'Run one explicit request using an approved endpoint and an enabled plugin with the experimental HTTP forward handler. Task status remains available while the content library is busy.'**
  String get pluginsHttpTaskDetails;

  /// No description provided for @pluginsHttpTaskDisconnect.
  ///
  /// In en, this message translates to:
  /// **'Connection cleanup failed'**
  String get pluginsHttpTaskDisconnect;

  /// No description provided for @pluginsHttpTaskEndpoint.
  ///
  /// In en, this message translates to:
  /// **'Approved endpoint'**
  String get pluginsHttpTaskEndpoint;

  /// No description provided for @pluginsHttpTaskEndpointsFailed.
  ///
  /// In en, this message translates to:
  /// **'Endpoints could not be read consistently, or the content library is busy. Task controls remain available. Refresh endpoints when the library returns.'**
  String get pluginsHttpTaskEndpointsFailed;

  /// No description provided for @pluginsHttpTaskEvidenceUnavailable.
  ///
  /// In en, this message translates to:
  /// **'Outcome evidence unavailable'**
  String get pluginsHttpTaskEvidenceUnavailable;

  /// No description provided for @pluginsHttpTaskExecution.
  ///
  /// In en, this message translates to:
  /// **'Guest execution: {fault}; exit code: {code}'**
  String pluginsHttpTaskExecution(int code, String fault);

  /// No description provided for @pluginsHttpTaskExit.
  ///
  /// In en, this message translates to:
  /// **'Worker exit — execution: {execution}; disconnect: {disconnect}; maintenance: {maintenance}'**
  String pluginsHttpTaskExit(
    String disconnect,
    String execution,
    String maintenance,
  );

  /// No description provided for @pluginsHttpTaskExplicit.
  ///
  /// In en, this message translates to:
  /// **'Submit sends one real request. Each click creates a new identity. Unknown submissions and result reads are never replayed automatically. Cancellation does not prove the remote operation was undone.'**
  String get pluginsHttpTaskExplicit;

  /// No description provided for @pluginsHttpTaskFailed.
  ///
  /// In en, this message translates to:
  /// **'Failed'**
  String get pluginsHttpTaskFailed;

  /// No description provided for @pluginsHttpTaskHeaders.
  ///
  /// In en, this message translates to:
  /// **'Ordinary request headers, one Name: value per line'**
  String get pluginsHttpTaskHeaders;

  /// No description provided for @pluginsHttpTaskHeadersHint.
  ///
  /// In en, this message translates to:
  /// **'Repeated headers stay separate. Credential and connection headers are supplied only by the runtime.'**
  String get pluginsHttpTaskHeadersHint;

  /// No description provided for @pluginsHttpTaskHistory.
  ///
  /// In en, this message translates to:
  /// **'Previous task observations (up to 5)'**
  String get pluginsHttpTaskHistory;

  /// No description provided for @pluginsHttpTaskHttpResult.
  ///
  /// In en, this message translates to:
  /// **'HTTP outcome: {status}; remote status: {code}'**
  String pluginsHttpTaskHttpResult(int code, String status);

  /// No description provided for @pluginsHttpTaskInactive.
  ///
  /// In en, this message translates to:
  /// **'Connection is inactive'**
  String get pluginsHttpTaskInactive;

  /// No description provided for @pluginsHttpTaskInvalid.
  ///
  /// In en, this message translates to:
  /// **'Check the selected endpoint, method, relative target, ordinary headers, body encoding and timeout against the approval limits.'**
  String get pluginsHttpTaskInvalid;

  /// No description provided for @pluginsHttpTaskInvalidOptions.
  ///
  /// In en, this message translates to:
  /// **'Invalid options'**
  String get pluginsHttpTaskInvalidOptions;

  /// No description provided for @pluginsHttpTaskKey.
  ///
  /// In en, this message translates to:
  /// **'Task identity: {identity}'**
  String pluginsHttpTaskKey(String identity);

  /// No description provided for @pluginsHttpTaskLimit.
  ///
  /// In en, this message translates to:
  /// **'Quota or limit reached'**
  String get pluginsHttpTaskLimit;

  /// No description provided for @pluginsHttpTaskLoadingEndpoints.
  ///
  /// In en, this message translates to:
  /// **'Reading approved endpoints…'**
  String get pluginsHttpTaskLoadingEndpoints;

  /// No description provided for @pluginsHttpTaskLocal.
  ///
  /// In en, this message translates to:
  /// **'Content library available; no active task'**
  String get pluginsHttpTaskLocal;

  /// No description provided for @pluginsHttpTaskModule.
  ///
  /// In en, this message translates to:
  /// **'Invalid guest module'**
  String get pluginsHttpTaskModule;

  /// No description provided for @pluginsHttpTaskNew.
  ///
  /// In en, this message translates to:
  /// **'Prepare a new request'**
  String get pluginsHttpTaskNew;

  /// No description provided for @pluginsHttpTaskNoEndpoints.
  ///
  /// In en, this message translates to:
  /// **'No current endpoint matches an enabled, approved HTTP forward plugin.'**
  String get pluginsHttpTaskNoEndpoints;

  /// No description provided for @pluginsHttpTaskNotFound.
  ///
  /// In en, this message translates to:
  /// **'Not found'**
  String get pluginsHttpTaskNotFound;

  /// No description provided for @pluginsHttpTaskOk.
  ///
  /// In en, this message translates to:
  /// **'OK'**
  String get pluginsHttpTaskOk;

  /// No description provided for @pluginsHttpTaskOutcomeUnknown.
  ///
  /// In en, this message translates to:
  /// **'Remote outcome unknown; do not assume rollback or resend'**
  String get pluginsHttpTaskOutcomeUnknown;

  /// No description provided for @pluginsHttpTaskPackageChanged.
  ///
  /// In en, this message translates to:
  /// **'Package binding changed'**
  String get pluginsHttpTaskPackageChanged;

  /// No description provided for @pluginsHttpTaskPending.
  ///
  /// In en, this message translates to:
  /// **'Result pending'**
  String get pluginsHttpTaskPending;

  /// No description provided for @pluginsHttpTaskPoll.
  ///
  /// In en, this message translates to:
  /// **'Check task'**
  String get pluginsHttpTaskPoll;

  /// No description provided for @pluginsHttpTaskProtocol.
  ///
  /// In en, this message translates to:
  /// **'Task protocol error'**
  String get pluginsHttpTaskProtocol;

  /// No description provided for @pluginsHttpTaskRead.
  ///
  /// In en, this message translates to:
  /// **'Read result once'**
  String get pluginsHttpTaskRead;

  /// No description provided for @pluginsHttpTaskReadBound.
  ///
  /// In en, this message translates to:
  /// **'Read limit exceeded'**
  String get pluginsHttpTaskReadBound;

  /// No description provided for @pluginsHttpTaskReadPending.
  ///
  /// In en, this message translates to:
  /// **'No result was returned. Check status before explicitly reading again.'**
  String get pluginsHttpTaskReadPending;

  /// No description provided for @pluginsHttpTaskReadUnknown.
  ///
  /// In en, this message translates to:
  /// **'The result read could not be confirmed and may already have consumed the result. It will not be read again. Status and exit can still be checked.'**
  String get pluginsHttpTaskReadUnknown;

  /// No description provided for @pluginsHttpTaskReady.
  ///
  /// In en, this message translates to:
  /// **'Result ready: read explicitly. Ready does not mean the worker has exited.'**
  String get pluginsHttpTaskReady;

  /// No description provided for @pluginsHttpTaskReclaimed.
  ///
  /// In en, this message translates to:
  /// **'Worker exited; original content library returned'**
  String get pluginsHttpTaskReclaimed;

  /// No description provided for @pluginsHttpTaskRecoveryRequired.
  ///
  /// In en, this message translates to:
  /// **'Worker exited; cleanup or maintenance requires repair'**
  String get pluginsHttpTaskRecoveryRequired;

  /// No description provided for @pluginsHttpTaskRefresh.
  ///
  /// In en, this message translates to:
  /// **'Refresh task status'**
  String get pluginsHttpTaskRefresh;

  /// No description provided for @pluginsHttpTaskRefreshEndpoints.
  ///
  /// In en, this message translates to:
  /// **'Refresh approved endpoints'**
  String get pluginsHttpTaskRefreshEndpoints;

  /// No description provided for @pluginsHttpTaskRemoteError.
  ///
  /// In en, this message translates to:
  /// **'The remote server returned a 4xx/5xx response. This is a completed HTTP exchange, separate from guest execution errors.'**
  String get pluginsHttpTaskRemoteError;

  /// No description provided for @pluginsHttpTaskRepair.
  ///
  /// In en, this message translates to:
  /// **'Repair cleanup'**
  String get pluginsHttpTaskRepair;

  /// No description provided for @pluginsHttpTaskResponseBase64.
  ///
  /// In en, this message translates to:
  /// **'Response body: exact Base64'**
  String get pluginsHttpTaskResponseBase64;

  /// No description provided for @pluginsHttpTaskResponseHeaders.
  ///
  /// In en, this message translates to:
  /// **'Response headers (duplicates preserved; binary values use Base64)'**
  String get pluginsHttpTaskResponseHeaders;

  /// No description provided for @pluginsHttpTaskResponseText.
  ///
  /// In en, this message translates to:
  /// **'Response body: plain text preview'**
  String get pluginsHttpTaskResponseText;

  /// No description provided for @pluginsHttpTaskResultUnavailable.
  ///
  /// In en, this message translates to:
  /// **'Result delivery unavailable'**
  String get pluginsHttpTaskResultUnavailable;

  /// No description provided for @pluginsHttpTaskRevoked.
  ///
  /// In en, this message translates to:
  /// **'Approval revoked'**
  String get pluginsHttpTaskRevoked;

  /// No description provided for @pluginsHttpTaskRunning.
  ///
  /// In en, this message translates to:
  /// **'Running; content library is owned by the worker'**
  String get pluginsHttpTaskRunning;

  /// No description provided for @pluginsHttpTaskSpawn.
  ///
  /// In en, this message translates to:
  /// **'Worker could not start'**
  String get pluginsHttpTaskSpawn;

  /// No description provided for @pluginsHttpTaskStart.
  ///
  /// In en, this message translates to:
  /// **'Submit new request'**
  String get pluginsHttpTaskStart;

  /// No description provided for @pluginsHttpTaskStartUnknown.
  ///
  /// In en, this message translates to:
  /// **'The submission result is unknown. Its identity is retained. Query status to find the same task; the request will not be sent again.'**
  String get pluginsHttpTaskStartUnknown;

  /// No description provided for @pluginsHttpTaskStatusFailed.
  ///
  /// In en, this message translates to:
  /// **'Task status could not be confirmed. Refresh status; no request has been replayed.'**
  String get pluginsHttpTaskStatusFailed;

  /// No description provided for @pluginsHttpTaskStopping.
  ///
  /// In en, this message translates to:
  /// **'Stopping; waiting for actual worker exit'**
  String get pluginsHttpTaskStopping;

  /// No description provided for @pluginsHttpTaskSubmission.
  ///
  /// In en, this message translates to:
  /// **'Submission identity: {identity}'**
  String pluginsHttpTaskSubmission(String identity);

  /// No description provided for @pluginsHttpTaskTarget.
  ///
  /// In en, this message translates to:
  /// **'Relative target, for example /v1/items?limit=10'**
  String get pluginsHttpTaskTarget;

  /// No description provided for @pluginsHttpTaskText.
  ///
  /// In en, this message translates to:
  /// **'UTF-8 text'**
  String get pluginsHttpTaskText;

  /// No description provided for @pluginsHttpTaskTimeout.
  ///
  /// In en, this message translates to:
  /// **'Timeout in milliseconds (1–30000, within endpoint approval)'**
  String get pluginsHttpTaskTimeout;

  /// No description provided for @pluginsHttpTaskTitle.
  ///
  /// In en, this message translates to:
  /// **'HTTP tasks'**
  String get pluginsHttpTaskTitle;

  /// No description provided for @pluginsHttpTaskTrap.
  ///
  /// In en, this message translates to:
  /// **'Guest execution trapped'**
  String get pluginsHttpTaskTrap;

  /// No description provided for @pluginsHttpTaskUnavailable.
  ///
  /// In en, this message translates to:
  /// **'Original content library unavailable; recovery needs attention'**
  String get pluginsHttpTaskUnavailable;

  /// No description provided for @pluginsHttpTaskUnsupported.
  ///
  /// In en, this message translates to:
  /// **'Unsupported operation'**
  String get pluginsHttpTaskUnsupported;

  /// No description provided for @pluginsHttpTaskWorking.
  ///
  /// In en, this message translates to:
  /// **'Waiting for the task control response…'**
  String get pluginsHttpTaskWorking;

  /// No description provided for @pluginsImport.
  ///
  /// In en, this message translates to:
  /// **'Import'**
  String get pluginsImport;

  /// No description provided for @pluginsImportDetails.
  ///
  /// In en, this message translates to:
  /// **'You choose whether to enable a plugin after importing it. Disabling or uninstalling preserves your content.'**
  String get pluginsImportDetails;

  /// No description provided for @pluginsImportPreview.
  ///
  /// In en, this message translates to:
  /// **'Import preview: {name}'**
  String pluginsImportPreview(String name);

  /// No description provided for @pluginsImportUnknown.
  ///
  /// In en, this message translates to:
  /// **'Import could not be confirmed'**
  String get pluginsImportUnknown;

  /// No description provided for @pluginsImportedDisabled.
  ///
  /// In en, this message translates to:
  /// **'Imported and disabled. Choose which permissions to allow.'**
  String get pluginsImportedDisabled;

  /// No description provided for @pluginsInputFailed.
  ///
  /// In en, this message translates to:
  /// **'The input file could not be read'**
  String get pluginsInputFailed;

  /// No description provided for @pluginsInputTooLong.
  ///
  /// In en, this message translates to:
  /// **'This view has reached its input limit. Shorten the text and try again.'**
  String get pluginsInputTooLong;

  /// No description provided for @pluginsInspectFailed.
  ///
  /// In en, this message translates to:
  /// **'The plugin preview could not be loaded'**
  String get pluginsInspectFailed;

  /// No description provided for @pluginsInspectedOnly.
  ///
  /// In en, this message translates to:
  /// **'The file has only been inspected. Enable the plugin separately after importing it.'**
  String get pluginsInspectedOnly;

  /// No description provided for @pluginsInsufficientApproval.
  ///
  /// In en, this message translates to:
  /// **'The plugin is enabled but needs content permission. The workbench stays read-only. Disable it to review permissions again.'**
  String get pluginsInsufficientApproval;

  /// No description provided for @pluginsIoApproved.
  ///
  /// In en, this message translates to:
  /// **'Approved: {permissions}'**
  String pluginsIoApproved(String permissions);

  /// No description provided for @pluginsIoCredentialUse.
  ///
  /// In en, this message translates to:
  /// **'Use approved credentials'**
  String get pluginsIoCredentialUse;

  /// No description provided for @pluginsIoDeclared.
  ///
  /// In en, this message translates to:
  /// **'Requested network and file permissions: {permissions}'**
  String pluginsIoDeclared(String permissions);

  /// No description provided for @pluginsIoFileCreate.
  ///
  /// In en, this message translates to:
  /// **'Create files'**
  String get pluginsIoFileCreate;

  /// No description provided for @pluginsIoFileDelete.
  ///
  /// In en, this message translates to:
  /// **'Delete files'**
  String get pluginsIoFileDelete;

  /// No description provided for @pluginsIoFileList.
  ///
  /// In en, this message translates to:
  /// **'Browse approved folders'**
  String get pluginsIoFileList;

  /// No description provided for @pluginsIoFileRead.
  ///
  /// In en, this message translates to:
  /// **'Read approved files'**
  String get pluginsIoFileRead;

  /// No description provided for @pluginsIoFileReplace.
  ///
  /// In en, this message translates to:
  /// **'Replace files'**
  String get pluginsIoFileReplace;

  /// No description provided for @pluginsIoHttpListen.
  ///
  /// In en, this message translates to:
  /// **'Listen for network connections'**
  String get pluginsIoHttpListen;

  /// No description provided for @pluginsIoHttpPublish.
  ///
  /// In en, this message translates to:
  /// **'Provide an API service'**
  String get pluginsIoHttpPublish;

  /// No description provided for @pluginsIoHttpRequest.
  ///
  /// In en, this message translates to:
  /// **'Call network APIs'**
  String get pluginsIoHttpRequest;

  /// No description provided for @pluginsIoNoneApproved.
  ///
  /// In en, this message translates to:
  /// **'No network or file permissions approved'**
  String get pluginsIoNoneApproved;

  /// No description provided for @pluginsIoRevoke.
  ///
  /// In en, this message translates to:
  /// **'Revoke all network and file permissions'**
  String get pluginsIoRevoke;

  /// No description provided for @pluginsIoSave.
  ///
  /// In en, this message translates to:
  /// **'Save network and file permissions'**
  String get pluginsIoSave;

  /// No description provided for @pluginsIoScopeNotice.
  ///
  /// In en, this message translates to:
  /// **'These choices save permission categories only. Server addresses, file access and credentials need separate approval; unavailable features remain unavailable. Reopen the plugin form after changes.'**
  String get pluginsIoScopeNotice;

  /// No description provided for @pluginsIoTitle.
  ///
  /// In en, this message translates to:
  /// **'Network and file permissions'**
  String get pluginsIoTitle;

  /// No description provided for @pluginsIoWebSocketConnect.
  ///
  /// In en, this message translates to:
  /// **'Connect to WebSocket services'**
  String get pluginsIoWebSocketConnect;

  /// No description provided for @pluginsListUnknown.
  ///
  /// In en, this message translates to:
  /// **'The plugin list could not be confirmed'**
  String get pluginsListUnknown;

  /// No description provided for @pluginsManageAbove.
  ///
  /// In en, this message translates to:
  /// **'Manage this plugin using the workbench plugin controls above.'**
  String get pluginsManageAbove;

  /// No description provided for @pluginsManagementUnavailable.
  ///
  /// In en, this message translates to:
  /// **'Plugin management is unavailable. Existing content remains readable.'**
  String get pluginsManagementUnavailable;

  /// No description provided for @pluginsNoPermissions.
  ///
  /// In en, this message translates to:
  /// **'No content permissions declared.'**
  String get pluginsNoPermissions;

  /// No description provided for @pluginsOpenTextTool.
  ///
  /// In en, this message translates to:
  /// **'Open text tool'**
  String get pluginsOpenTextTool;

  /// No description provided for @pluginsOpenView.
  ///
  /// In en, this message translates to:
  /// **'Open view'**
  String get pluginsOpenView;

  /// No description provided for @pluginsOpeningView.
  ///
  /// In en, this message translates to:
  /// **'Opening plugin view…'**
  String get pluginsOpeningView;

  /// No description provided for @pluginsOperation.
  ///
  /// In en, this message translates to:
  /// **'Query operation results'**
  String get pluginsOperation;

  /// No description provided for @pluginsOtherCapability.
  ///
  /// In en, this message translates to:
  /// **'Other declared permission: {name}'**
  String pluginsOtherCapability(String name);

  /// No description provided for @pluginsPackageFile.
  ///
  /// In en, this message translates to:
  /// **'Morrow plugin'**
  String get pluginsPackageFile;

  /// No description provided for @pluginsPreviewOnly.
  ///
  /// In en, this message translates to:
  /// **'Preview only. Results are not automatically saved to existing content.'**
  String get pluginsPreviewOnly;

  /// No description provided for @pluginsPreviewTruncated.
  ///
  /// In en, this message translates to:
  /// **'…showing the first 4,096 characters only'**
  String get pluginsPreviewTruncated;

  /// No description provided for @pluginsProtection.
  ///
  /// In en, this message translates to:
  /// **'Content protection'**
  String get pluginsProtection;

  /// No description provided for @pluginsProtectionDetails.
  ///
  /// In en, this message translates to:
  /// **'Back up the original protection file for recovery with this system account. It contains no cards or attachments.'**
  String get pluginsProtectionDetails;

  /// No description provided for @pluginsProtectionFileType.
  ///
  /// In en, this message translates to:
  /// **'Library protection file'**
  String get pluginsProtectionFileType;

  /// No description provided for @pluginsProtectionSaved.
  ///
  /// In en, this message translates to:
  /// **'Protection file backed up. Choose it for recovery if startup fails.'**
  String get pluginsProtectionSaved;

  /// No description provided for @pluginsRead.
  ///
  /// In en, this message translates to:
  /// **'Read content'**
  String get pluginsRead;

  /// No description provided for @pluginsReadingState.
  ///
  /// In en, this message translates to:
  /// **'Loading plugin status…'**
  String get pluginsReadingState;

  /// No description provided for @pluginsRefreshFailed.
  ///
  /// In en, this message translates to:
  /// **'{reason}. The list could not be refreshed. Select “Refresh list” to retry reading it.'**
  String pluginsRefreshFailed(String reason);

  /// No description provided for @pluginsRefreshList.
  ///
  /// In en, this message translates to:
  /// **'Refresh list'**
  String get pluginsRefreshList;

  /// No description provided for @pluginsRefreshState.
  ///
  /// In en, this message translates to:
  /// **'Refresh status'**
  String get pluginsRefreshState;

  /// No description provided for @pluginsRename.
  ///
  /// In en, this message translates to:
  /// **'Rename'**
  String get pluginsRename;

  /// No description provided for @pluginsResultBytes.
  ///
  /// In en, this message translates to:
  /// **'{count, plural, =1{1 byte} other{{count} bytes}}\n{preview}'**
  String pluginsResultBytes(int count, String preview);

  /// No description provided for @pluginsSavePermissions.
  ///
  /// In en, this message translates to:
  /// **'Save permissions'**
  String get pluginsSavePermissions;

  /// No description provided for @pluginsSelectedFile.
  ///
  /// In en, this message translates to:
  /// **'Selected file: {name}'**
  String pluginsSelectedFile(String name);

  /// No description provided for @pluginsServiceAcknowledgeUncertain.
  ///
  /// In en, this message translates to:
  /// **'I reviewed the refreshed records'**
  String get pluginsServiceAcknowledgeUncertain;

  /// No description provided for @pluginsServiceAddScope.
  ///
  /// In en, this message translates to:
  /// **'Add content scope'**
  String get pluginsServiceAddScope;

  /// No description provided for @pluginsServiceAttachmentId.
  ///
  /// In en, this message translates to:
  /// **'Exact attachment identity'**
  String get pluginsServiceAttachmentId;

  /// No description provided for @pluginsServiceAuthenticationUnavailable.
  ///
  /// In en, this message translates to:
  /// **'This selected authentication is missing, disabled, expired, or belongs to another principal. Its draft scopes are retained; select a valid replacement or explicitly remove it.'**
  String get pluginsServiceAuthenticationUnavailable;

  /// No description provided for @pluginsServiceAuthorities.
  ///
  /// In en, this message translates to:
  /// **'Authentication and publication records'**
  String get pluginsServiceAuthorities;

  /// No description provided for @pluginsServiceCardId.
  ///
  /// In en, this message translates to:
  /// **'Exact card identity'**
  String get pluginsServiceCardId;

  /// No description provided for @pluginsServiceCatalogChanged.
  ///
  /// In en, this message translates to:
  /// **'The package catalog changed or is unavailable. Your draft is preserved. Explicitly refresh the selection before saving.'**
  String get pluginsServiceCatalogChanged;

  /// No description provided for @pluginsServiceClearToken.
  ///
  /// In en, this message translates to:
  /// **'Clear token'**
  String get pluginsServiceClearToken;

  /// No description provided for @pluginsServiceCloseEditor.
  ///
  /// In en, this message translates to:
  /// **'Close editor'**
  String get pluginsServiceCloseEditor;

  /// No description provided for @pluginsServiceConfigDigest.
  ///
  /// In en, this message translates to:
  /// **'Configuration digest'**
  String get pluginsServiceConfigDigest;

  /// No description provided for @pluginsServiceConfiguration.
  ///
  /// In en, this message translates to:
  /// **'Saved configuration'**
  String get pluginsServiceConfiguration;

  /// No description provided for @pluginsServiceConfigurations.
  ///
  /// In en, this message translates to:
  /// **'Saved configurations'**
  String get pluginsServiceConfigurations;

  /// No description provided for @pluginsServiceCopyClear.
  ///
  /// In en, this message translates to:
  /// **'Copy and clear token'**
  String get pluginsServiceCopyClear;

  /// No description provided for @pluginsServiceCreated.
  ///
  /// In en, this message translates to:
  /// **'Created (UTC)'**
  String get pluginsServiceCreated;

  /// No description provided for @pluginsServiceDays.
  ///
  /// In en, this message translates to:
  /// **'Requested lifetime (1–30 days)'**
  String get pluginsServiceDays;

  /// No description provided for @pluginsServiceDigestFixed.
  ///
  /// In en, this message translates to:
  /// **'Editing keeps the original package digest. A matching package must be selected; this does not enable it.'**
  String get pluginsServiceDigestFixed;

  /// No description provided for @pluginsServiceDisable.
  ///
  /// In en, this message translates to:
  /// **'Disable'**
  String get pluginsServiceDisable;

  /// No description provided for @pluginsServiceDisabled.
  ///
  /// In en, this message translates to:
  /// **'Disabled'**
  String get pluginsServiceDisabled;

  /// No description provided for @pluginsServiceEditConfig.
  ///
  /// In en, this message translates to:
  /// **'Edit configuration'**
  String get pluginsServiceEditConfig;

  /// No description provided for @pluginsServiceEditPublication.
  ///
  /// In en, this message translates to:
  /// **'Edit publication'**
  String get pluginsServiceEditPublication;

  /// No description provided for @pluginsServiceExpired.
  ///
  /// In en, this message translates to:
  /// **'Expired or not yet valid'**
  String get pluginsServiceExpired;

  /// No description provided for @pluginsServiceExpires.
  ///
  /// In en, this message translates to:
  /// **'Actual expiry (UTC)'**
  String get pluginsServiceExpires;

  /// No description provided for @pluginsServiceHandler.
  ///
  /// In en, this message translates to:
  /// **'Declared service handler'**
  String get pluginsServiceHandler;

  /// No description provided for @pluginsServiceIdentity.
  ///
  /// In en, this message translates to:
  /// **'Service identity'**
  String get pluginsServiceIdentity;

  /// No description provided for @pluginsServiceInvalid.
  ///
  /// In en, this message translates to:
  /// **'Check the fields, selected approvals, and current package before saving.'**
  String get pluginsServiceInvalid;

  /// No description provided for @pluginsServiceIssue.
  ///
  /// In en, this message translates to:
  /// **'Issue token'**
  String get pluginsServiceIssue;

  /// No description provided for @pluginsServiceIssuedToken.
  ///
  /// In en, this message translates to:
  /// **'One-time bearer token'**
  String get pluginsServiceIssuedToken;

  /// No description provided for @pluginsServiceListenAddress.
  ///
  /// In en, this message translates to:
  /// **'Numeric listen address and port'**
  String get pluginsServiceListenAddress;

  /// No description provided for @pluginsServiceLoadFailed.
  ///
  /// In en, this message translates to:
  /// **'Records could not be refreshed. Refresh again before making changes.'**
  String get pluginsServiceLoadFailed;

  /// No description provided for @pluginsServiceManagementOnly.
  ///
  /// In en, this message translates to:
  /// **'Manage saved configurations and approvals here. Saving does not start a listener, run a package, or activate a service.'**
  String get pluginsServiceManagementOnly;

  /// No description provided for @pluginsServiceMethod.
  ///
  /// In en, this message translates to:
  /// **'HTTP method'**
  String get pluginsServiceMethod;

  /// No description provided for @pluginsServiceNewAuthentication.
  ///
  /// In en, this message translates to:
  /// **'New authentication'**
  String get pluginsServiceNewAuthentication;

  /// No description provided for @pluginsServiceNewConfig.
  ///
  /// In en, this message translates to:
  /// **'New configuration'**
  String get pluginsServiceNewConfig;

  /// No description provided for @pluginsServiceNo.
  ///
  /// In en, this message translates to:
  /// **'No'**
  String get pluginsServiceNo;

  /// No description provided for @pluginsServiceNoAuthentication.
  ///
  /// In en, this message translates to:
  /// **'Create a currently valid authentication record first.'**
  String get pluginsServiceNoAuthentication;

  /// No description provided for @pluginsServiceNoAuthorities.
  ///
  /// In en, this message translates to:
  /// **'No authentication or publication records.'**
  String get pluginsServiceNoAuthorities;

  /// No description provided for @pluginsServiceNoConfigurations.
  ///
  /// In en, this message translates to:
  /// **'No service configurations.'**
  String get pluginsServiceNoConfigurations;

  /// No description provided for @pluginsServicePackage.
  ///
  /// In en, this message translates to:
  /// **'Declared and approved package'**
  String get pluginsServicePackage;

  /// No description provided for @pluginsServicePackageDigest.
  ///
  /// In en, this message translates to:
  /// **'Package digest'**
  String get pluginsServicePackageDigest;

  /// No description provided for @pluginsServicePackageUnavailable.
  ///
  /// In en, this message translates to:
  /// **'The matching package or its listen/publish approvals are unavailable. Historical records remain readable and can be disabled.'**
  String get pluginsServicePackageUnavailable;

  /// No description provided for @pluginsServicePath.
  ///
  /// In en, this message translates to:
  /// **'Exact request path'**
  String get pluginsServicePath;

  /// No description provided for @pluginsServicePolicyChanged.
  ///
  /// In en, this message translates to:
  /// **'The selected original record changed or is no longer usable. Refresh the selection, or reopen the editor from the current record. Your draft remains here.'**
  String get pluginsServicePolicyChanged;

  /// No description provided for @pluginsServicePrincipalId.
  ///
  /// In en, this message translates to:
  /// **'Principal identity'**
  String get pluginsServicePrincipalId;

  /// No description provided for @pluginsServicePrincipals.
  ///
  /// In en, this message translates to:
  /// **'Authorized principals and content scopes'**
  String get pluginsServicePrincipals;

  /// No description provided for @pluginsServicePublicationEditor.
  ///
  /// In en, this message translates to:
  /// **'Publication approval'**
  String get pluginsServicePublicationEditor;

  /// No description provided for @pluginsServicePublicationHelp.
  ///
  /// In en, this message translates to:
  /// **'Approval is bound to this exact configuration, revision and reference. Its actual expiry is limited by every selected authentication record and may be shorter than requested. Saving does not start listening.'**
  String get pluginsServicePublicationHelp;

  /// No description provided for @pluginsServicePublicationMismatch.
  ///
  /// In en, this message translates to:
  /// **'This publication no longer matches the current configuration. Review and explicitly save a replacement approval.'**
  String get pluginsServicePublicationMismatch;

  /// No description provided for @pluginsServiceQueryPath.
  ///
  /// In en, this message translates to:
  /// **'Separate result query path (optional)'**
  String get pluginsServiceQueryPath;

  /// No description provided for @pluginsServiceReference.
  ///
  /// In en, this message translates to:
  /// **'Approval reference'**
  String get pluginsServiceReference;

  /// No description provided for @pluginsServiceRefresh.
  ///
  /// In en, this message translates to:
  /// **'Refresh records'**
  String get pluginsServiceRefresh;

  /// No description provided for @pluginsServiceRefreshSelection.
  ///
  /// In en, this message translates to:
  /// **'Refresh this selection'**
  String get pluginsServiceRefreshSelection;

  /// No description provided for @pluginsServiceRemovePrincipal.
  ///
  /// In en, this message translates to:
  /// **'Remove principal'**
  String get pluginsServiceRemovePrincipal;

  /// No description provided for @pluginsServiceRemoveScope.
  ///
  /// In en, this message translates to:
  /// **'Remove scope'**
  String get pluginsServiceRemoveScope;

  /// No description provided for @pluginsServiceRetention.
  ///
  /// In en, this message translates to:
  /// **'Request history retention (milliseconds, up to 30 days)'**
  String get pluginsServiceRetention;

  /// No description provided for @pluginsServiceRevision.
  ///
  /// In en, this message translates to:
  /// **'Revision'**
  String get pluginsServiceRevision;

  /// No description provided for @pluginsServiceRotate.
  ///
  /// In en, this message translates to:
  /// **'Rotate token'**
  String get pluginsServiceRotate;

  /// No description provided for @pluginsServiceRotateAuthentication.
  ///
  /// In en, this message translates to:
  /// **'Rotate authentication'**
  String get pluginsServiceRotateAuthentication;

  /// No description provided for @pluginsServiceRunAbandon.
  ///
  /// In en, this message translates to:
  /// **'Keep record and end this attempt'**
  String get pluginsServiceRunAbandon;

  /// No description provided for @pluginsServiceRunAdvanced.
  ///
  /// In en, this message translates to:
  /// **'Request and worker limits'**
  String get pluginsServiceRunAdvanced;

  /// No description provided for @pluginsServiceRunAttempt.
  ///
  /// In en, this message translates to:
  /// **'Unresolved start attempt'**
  String get pluginsServiceRunAttempt;

  /// No description provided for @pluginsServiceRunBoundsHint.
  ///
  /// In en, this message translates to:
  /// **'These limits must also fit the plugin declaration and saved approvals. Reserved work consumes the cumulative budget even when cancelled. Expiry stops this run; renewal is not automatic.'**
  String get pluginsServiceRunBoundsHint;

  /// No description provided for @pluginsServiceRunBytes.
  ///
  /// In en, this message translates to:
  /// **'Run byte budget (bytes, up to 67,108,864)'**
  String get pluginsServiceRunBytes;

  /// No description provided for @pluginsServiceRunCalls.
  ///
  /// In en, this message translates to:
  /// **'Calls per job (up to 1,024)'**
  String get pluginsServiceRunCalls;

  /// No description provided for @pluginsServiceRunCancelled.
  ///
  /// In en, this message translates to:
  /// **'Cancelled'**
  String get pluginsServiceRunCancelled;

  /// No description provided for @pluginsServiceRunClosed.
  ///
  /// In en, this message translates to:
  /// **'Closed'**
  String get pluginsServiceRunClosed;

  /// No description provided for @pluginsServiceRunConcurrent.
  ///
  /// In en, this message translates to:
  /// **'Concurrent jobs (up to 128)'**
  String get pluginsServiceRunConcurrent;

  /// No description provided for @pluginsServiceRunControlUnknown.
  ///
  /// In en, this message translates to:
  /// **'The control result is unknown. Refresh the original service state before another operation.'**
  String get pluginsServiceRunControlUnknown;

  /// No description provided for @pluginsServiceRunDenied.
  ///
  /// In en, this message translates to:
  /// **'Denied'**
  String get pluginsServiceRunDenied;

  /// No description provided for @pluginsServiceRunExited.
  ///
  /// In en, this message translates to:
  /// **'Service exited'**
  String get pluginsServiceRunExited;

  /// No description provided for @pluginsServiceRunHeaderBytes.
  ///
  /// In en, this message translates to:
  /// **'Maximum header size (bytes, up to 65,536)'**
  String get pluginsServiceRunHeaderBytes;

  /// No description provided for @pluginsServiceRunHint.
  ///
  /// In en, this message translates to:
  /// **'Choose an approved publication and finite run limits, then explicitly start the service. After stopping, wait for the original workbench owner to return before acknowledging the result.'**
  String get pluginsServiceRunHint;

  /// No description provided for @pluginsServiceRunHostFailure.
  ///
  /// In en, this message translates to:
  /// **'Host diagnostic: {detail}'**
  String pluginsServiceRunHostFailure(String detail);

  /// No description provided for @pluginsServiceRunHttpPanel.
  ///
  /// In en, this message translates to:
  /// **'An API service currently owns this task. Use the service run panel above to stop it or acknowledge its exit. Your HTTP request draft is retained.'**
  String get pluginsServiceRunHttpPanel;

  /// No description provided for @pluginsServiceRunIdentityChanged.
  ///
  /// In en, this message translates to:
  /// **'A different task owns the content. This panel will not control it using the previous service identity.'**
  String get pluginsServiceRunIdentityChanged;

  /// No description provided for @pluginsServiceRunInvalid.
  ///
  /// In en, this message translates to:
  /// **'Check the selected service and numeric limits. No new run was submitted.'**
  String get pluginsServiceRunInvalid;

  /// No description provided for @pluginsServiceRunInvalidOutcome.
  ///
  /// In en, this message translates to:
  /// **'Invalid configuration'**
  String get pluginsServiceRunInvalidOutcome;

  /// No description provided for @pluginsServiceRunJobBytes.
  ///
  /// In en, this message translates to:
  /// **'Bytes per job (up to 16,777,216)'**
  String get pluginsServiceRunJobBytes;

  /// No description provided for @pluginsServiceRunJobs.
  ///
  /// In en, this message translates to:
  /// **'Total task reservations (up to 1,000,000)'**
  String get pluginsServiceRunJobs;

  /// No description provided for @pluginsServiceRunLastObservation.
  ///
  /// In en, this message translates to:
  /// **'Showing the last observation; current state is unverified.'**
  String get pluginsServiceRunLastObservation;

  /// No description provided for @pluginsServiceRunLifetime.
  ///
  /// In en, this message translates to:
  /// **'Run duration (ms, up to 3,600,000)'**
  String get pluginsServiceRunLifetime;

  /// No description provided for @pluginsServiceRunLimit.
  ///
  /// In en, this message translates to:
  /// **'Limit reached'**
  String get pluginsServiceRunLimit;

  /// No description provided for @pluginsServiceRunLocal.
  ///
  /// In en, this message translates to:
  /// **'Content is available locally'**
  String get pluginsServiceRunLocal;

  /// No description provided for @pluginsServiceRunNetwork.
  ///
  /// In en, this message translates to:
  /// **'Bind: {bind}; listener: {listener}; supervision: {supervision}'**
  String pluginsServiceRunNetwork(
    String bind,
    String listener,
    String supervision,
  );

  /// No description provided for @pluginsServiceRunNextSettings.
  ///
  /// In en, this message translates to:
  /// **'Settings for the next explicit run'**
  String get pluginsServiceRunNextSettings;

  /// No description provided for @pluginsServiceRunNoSelection.
  ///
  /// In en, this message translates to:
  /// **'No approved publication is available. Check the plugin, configuration and authentication state.'**
  String get pluginsServiceRunNoSelection;

  /// No description provided for @pluginsServiceRunOutboundAttempt.
  ///
  /// In en, this message translates to:
  /// **'Endpoints bound to this start attempt'**
  String get pluginsServiceRunOutboundAttempt;

  /// No description provided for @pluginsServiceRunOutboundClear.
  ///
  /// In en, this message translates to:
  /// **'Clear endpoint selection'**
  String get pluginsServiceRunOutboundClear;

  /// No description provided for @pluginsServiceRunOutboundFailed.
  ///
  /// In en, this message translates to:
  /// **'Could not verify the endpoint list. Refresh before using selected endpoints.'**
  String get pluginsServiceRunOutboundFailed;

  /// No description provided for @pluginsServiceRunOutboundHint.
  ///
  /// In en, this message translates to:
  /// **'Outgoing APIs (optional, up to 8). Only endpoints approved for this package are listed. Leaving all unchecked blocks outgoing calls.'**
  String get pluginsServiceRunOutboundHint;

  /// No description provided for @pluginsServiceRunOutboundStale.
  ///
  /// In en, this message translates to:
  /// **'A selected endpoint changed or is no longer available. Select its current version explicitly, or clear the selection.'**
  String get pluginsServiceRunOutboundStale;

  /// No description provided for @pluginsServiceRunOwned.
  ///
  /// In en, this message translates to:
  /// **'Content is managed by the running service'**
  String get pluginsServiceRunOwned;

  /// No description provided for @pluginsServiceRunPending.
  ///
  /// In en, this message translates to:
  /// **'Pending'**
  String get pluginsServiceRunPending;

  /// No description provided for @pluginsServiceRunReclaimed.
  ///
  /// In en, this message translates to:
  /// **'Content ownership reclaimed; acknowledgement required'**
  String get pluginsServiceRunReclaimed;

  /// No description provided for @pluginsServiceRunReclaiming.
  ///
  /// In en, this message translates to:
  /// **'Waiting to reclaim content ownership'**
  String get pluginsServiceRunReclaiming;

  /// No description provided for @pluginsServiceRunRecovery.
  ///
  /// In en, this message translates to:
  /// **'Cleanup needs repair'**
  String get pluginsServiceRunRecovery;

  /// No description provided for @pluginsServiceRunRequestBytes.
  ///
  /// In en, this message translates to:
  /// **'Maximum request size (bytes)'**
  String get pluginsServiceRunRequestBytes;

  /// No description provided for @pluginsServiceRunResponseBytes.
  ///
  /// In en, this message translates to:
  /// **'Maximum response size (bytes)'**
  String get pluginsServiceRunResponseBytes;

  /// No description provided for @pluginsServiceRunRunning.
  ///
  /// In en, this message translates to:
  /// **'Service running'**
  String get pluginsServiceRunRunning;

  /// No description provided for @pluginsServiceRunSelection.
  ///
  /// In en, this message translates to:
  /// **'Approved service publication'**
  String get pluginsServiceRunSelection;

  /// No description provided for @pluginsServiceRunStale.
  ///
  /// In en, this message translates to:
  /// **'The selected package, configuration or approval changed. Refresh records and select it again before starting.'**
  String get pluginsServiceRunStale;

  /// No description provided for @pluginsServiceRunStart.
  ///
  /// In en, this message translates to:
  /// **'Start finite service'**
  String get pluginsServiceRunStart;

  /// No description provided for @pluginsServiceRunStartRejected.
  ///
  /// In en, this message translates to:
  /// **'The start response reported an error. The current task has been checked; review the reason and any cleanup state before proceeding.'**
  String get pluginsServiceRunStartRejected;

  /// No description provided for @pluginsServiceRunStartUnknown.
  ///
  /// In en, this message translates to:
  /// **'The start result is unknown. This attempt identity is retained; refresh to locate it. It will not be started again automatically.'**
  String get pluginsServiceRunStartUnknown;

  /// No description provided for @pluginsServiceRunStarting.
  ///
  /// In en, this message translates to:
  /// **'Starting service'**
  String get pluginsServiceRunStarting;

  /// No description provided for @pluginsServiceRunStatusFailed.
  ///
  /// In en, this message translates to:
  /// **'Could not verify the current service state. Refresh before taking further action.'**
  String get pluginsServiceRunStatusFailed;

  /// No description provided for @pluginsServiceRunStop.
  ///
  /// In en, this message translates to:
  /// **'Stop service'**
  String get pluginsServiceRunStop;

  /// No description provided for @pluginsServiceRunStopping.
  ///
  /// In en, this message translates to:
  /// **'Stopping; waiting for listener and worker exit'**
  String get pluginsServiceRunStopping;

  /// No description provided for @pluginsServiceRunSucceeded.
  ///
  /// In en, this message translates to:
  /// **'Succeeded'**
  String get pluginsServiceRunSucceeded;

  /// No description provided for @pluginsServiceRunTask.
  ///
  /// In en, this message translates to:
  /// **'Current task identity'**
  String get pluginsServiceRunTask;

  /// No description provided for @pluginsServiceRunTimeout.
  ///
  /// In en, this message translates to:
  /// **'Job timeout (ms, up to 30,000)'**
  String get pluginsServiceRunTimeout;

  /// No description provided for @pluginsServiceRunTimeoutOutcome.
  ///
  /// In en, this message translates to:
  /// **'Timed out'**
  String get pluginsServiceRunTimeoutOutcome;

  /// No description provided for @pluginsServiceRunTitle.
  ///
  /// In en, this message translates to:
  /// **'Run API service'**
  String get pluginsServiceRunTitle;

  /// No description provided for @pluginsServiceRunTotalBytes.
  ///
  /// In en, this message translates to:
  /// **'Worker byte budget (up to 67,108,864)'**
  String get pluginsServiceRunTotalBytes;

  /// No description provided for @pluginsServiceRunTransport.
  ///
  /// In en, this message translates to:
  /// **'Transport failure'**
  String get pluginsServiceRunTransport;

  /// No description provided for @pluginsServiceRunUnavailable.
  ///
  /// In en, this message translates to:
  /// **'Content storage unavailable'**
  String get pluginsServiceRunUnavailable;

  /// No description provided for @pluginsServiceSaveConfig.
  ///
  /// In en, this message translates to:
  /// **'Save configuration'**
  String get pluginsServiceSaveConfig;

  /// No description provided for @pluginsServiceSavePublication.
  ///
  /// In en, this message translates to:
  /// **'Save publication approval'**
  String get pluginsServiceSavePublication;

  /// No description provided for @pluginsServiceSaved.
  ///
  /// In en, this message translates to:
  /// **'Saved. Review the returned revision and actual expiry below.'**
  String get pluginsServiceSaved;

  /// No description provided for @pluginsServiceScopeAttachment.
  ///
  /// In en, this message translates to:
  /// **'Read attachment'**
  String get pluginsServiceScopeAttachment;

  /// No description provided for @pluginsServiceScopeCreate.
  ///
  /// In en, this message translates to:
  /// **'Create content'**
  String get pluginsServiceScopeCreate;

  /// No description provided for @pluginsServiceScopeEdit.
  ///
  /// In en, this message translates to:
  /// **'Edit content'**
  String get pluginsServiceScopeEdit;

  /// No description provided for @pluginsServiceScopeKind.
  ///
  /// In en, this message translates to:
  /// **'Allowed content operation'**
  String get pluginsServiceScopeKind;

  /// No description provided for @pluginsServiceScopeQuery.
  ///
  /// In en, this message translates to:
  /// **'Query operation'**
  String get pluginsServiceScopeQuery;

  /// No description provided for @pluginsServiceScopeRead.
  ///
  /// In en, this message translates to:
  /// **'Read content'**
  String get pluginsServiceScopeRead;

  /// No description provided for @pluginsServiceScopeRename.
  ///
  /// In en, this message translates to:
  /// **'Rename card'**
  String get pluginsServiceScopeRename;

  /// No description provided for @pluginsServiceScopeSummary.
  ///
  /// In en, this message translates to:
  /// **'Read summary'**
  String get pluginsServiceScopeSummary;

  /// No description provided for @pluginsServiceScopesHelp.
  ///
  /// In en, this message translates to:
  /// **'Select authentication explicitly. Add each allowed operation and exact object identity below. Removing a scope or principal requires its own button; existing scopes are preserved while editing.'**
  String get pluginsServiceScopesHelp;

  /// No description provided for @pluginsServiceTitle.
  ///
  /// In en, this message translates to:
  /// **'Service configuration'**
  String get pluginsServiceTitle;

  /// No description provided for @pluginsServiceTls.
  ///
  /// In en, this message translates to:
  /// **'Require TLS'**
  String get pluginsServiceTls;

  /// No description provided for @pluginsServiceTlsAttempt.
  ///
  /// In en, this message translates to:
  /// **'Certificate PEM digest bound to this start attempt'**
  String get pluginsServiceTlsAttempt;

  /// No description provided for @pluginsServiceTlsCertificate.
  ///
  /// In en, this message translates to:
  /// **'Choose certificate chain'**
  String get pluginsServiceTlsCertificate;

  /// No description provided for @pluginsServiceTlsChecked.
  ///
  /// In en, this message translates to:
  /// **'Certificate and key pairing checked. Certificate PEM SHA-256 is shown below. Clients must still verify the hostname, validity and trust chain.'**
  String get pluginsServiceTlsChecked;

  /// No description provided for @pluginsServiceTlsChecking.
  ///
  /// In en, this message translates to:
  /// **'Processing certificate selection…'**
  String get pluginsServiceTlsChecking;

  /// No description provided for @pluginsServiceTlsFailed.
  ///
  /// In en, this message translates to:
  /// **'Certificate check failed. Check the PEM files, key pairing and local paths before trying again.'**
  String get pluginsServiceTlsFailed;

  /// No description provided for @pluginsServiceTlsHelp.
  ///
  /// In en, this message translates to:
  /// **'Non-loopback addresses require TLS. This saves the requirement only; no listener or TLS identity is created here.'**
  String get pluginsServiceTlsHelp;

  /// No description provided for @pluginsServiceTlsHint.
  ///
  /// In en, this message translates to:
  /// **'Select a PEM certificate chain and private key, then check them. Files are checked again at start; an active certificate does not rotate automatically.'**
  String get pluginsServiceTlsHint;

  /// No description provided for @pluginsServiceTlsInspect.
  ///
  /// In en, this message translates to:
  /// **'Check certificate'**
  String get pluginsServiceTlsInspect;

  /// No description provided for @pluginsServiceTlsOutsideValidity.
  ///
  /// In en, this message translates to:
  /// **'The certificate chain is not yet valid or has expired. Check or replace the certificate and inspect it again before starting.'**
  String get pluginsServiceTlsOutsideValidity;

  /// No description provided for @pluginsServiceTlsPrivateKey.
  ///
  /// In en, this message translates to:
  /// **'Choose private key'**
  String get pluginsServiceTlsPrivateKey;

  /// No description provided for @pluginsServiceTlsRecheck.
  ///
  /// In en, this message translates to:
  /// **'Check the certificate again before starting. A change in the clock does not restore the previous selection.'**
  String get pluginsServiceTlsRecheck;

  /// No description provided for @pluginsServiceTlsUnavailable.
  ///
  /// In en, this message translates to:
  /// **'This backend does not support local TLS certificate selection.'**
  String get pluginsServiceTlsUnavailable;

  /// No description provided for @pluginsServiceTlsValidity.
  ///
  /// In en, this message translates to:
  /// **'Shared certificate-chain validity (UTC): {start} through {end}. The service stops after expiry.'**
  String pluginsServiceTlsValidity(String end, String start);

  /// No description provided for @pluginsServiceTokenDiscarded.
  ///
  /// In en, this message translates to:
  /// **'The one-time token was cleared while this panel was closed. Issue a new token explicitly if needed.'**
  String get pluginsServiceTokenDiscarded;

  /// No description provided for @pluginsServiceTokenHelp.
  ///
  /// In en, this message translates to:
  /// **'This token is shown only now. Copy it explicitly if needed. Clearing or closing this panel removes it from the session; it cannot be retrieved from the list. Rotation replaces the previous token.'**
  String get pluginsServiceTokenHelp;

  /// No description provided for @pluginsServiceUncertainHelp.
  ///
  /// In en, this message translates to:
  /// **'Refresh and inspect the original records first. Acknowledging this notice only allows another explicit action; it does not prove that the previous change failed or replay it.'**
  String get pluginsServiceUncertainHelp;

  /// No description provided for @pluginsServiceUnsupported.
  ///
  /// In en, this message translates to:
  /// **'Unsupported historical value'**
  String get pluginsServiceUnsupported;

  /// No description provided for @pluginsServiceWorking.
  ///
  /// In en, this message translates to:
  /// **'Working…'**
  String get pluginsServiceWorking;

  /// No description provided for @pluginsServiceWriteUnknown.
  ///
  /// In en, this message translates to:
  /// **'The result of the last change is unknown. It has not been sent again.'**
  String get pluginsServiceWriteUnknown;

  /// No description provided for @pluginsServiceYes.
  ///
  /// In en, this message translates to:
  /// **'Yes'**
  String get pluginsServiceYes;

  /// No description provided for @pluginsSettingsUnknown.
  ///
  /// In en, this message translates to:
  /// **'The setting has not been confirmed. Refresh the status before choosing again.'**
  String get pluginsSettingsUnknown;

  /// No description provided for @pluginsSnapshotDetails.
  ///
  /// In en, this message translates to:
  /// **'A library backup includes its cards, attachments and audit records. External assets remain references. Recovery requires the original system account.'**
  String get pluginsSnapshotDetails;

  /// No description provided for @pluginsSnapshotSaved.
  ///
  /// In en, this message translates to:
  /// **'Library backed up, including its attachments and original protection file.'**
  String get pluginsSnapshotSaved;

  /// No description provided for @pluginsStateUnavailable.
  ///
  /// In en, this message translates to:
  /// **'Plugin status could not be loaded. Try again.'**
  String get pluginsStateUnavailable;

  /// No description provided for @pluginsSummary.
  ///
  /// In en, this message translates to:
  /// **'Read summaries'**
  String get pluginsSummary;

  /// No description provided for @pluginsTextInput.
  ///
  /// In en, this message translates to:
  /// **'Input text'**
  String get pluginsTextInput;

  /// No description provided for @pluginsThirdParty.
  ///
  /// In en, this message translates to:
  /// **'Third-party plugins'**
  String get pluginsThirdParty;

  /// No description provided for @pluginsTlsIdentitiesDisable.
  ///
  /// In en, this message translates to:
  /// **'Disable identity'**
  String get pluginsTlsIdentitiesDisable;

  /// No description provided for @pluginsTlsIdentitiesEmpty.
  ///
  /// In en, this message translates to:
  /// **'No saved identities in this library.'**
  String get pluginsTlsIdentitiesEmpty;

  /// No description provided for @pluginsTlsIdentitiesFileMode.
  ///
  /// In en, this message translates to:
  /// **'Next start: checked local files.'**
  String get pluginsTlsIdentitiesFileMode;

  /// No description provided for @pluginsTlsIdentitiesHint.
  ///
  /// In en, this message translates to:
  /// **'Select an identity explicitly. Replacing or disabling an identity stops services that use it; a new start is always explicit.'**
  String get pluginsTlsIdentitiesHint;

  /// No description provided for @pluginsTlsIdentitiesImport.
  ///
  /// In en, this message translates to:
  /// **'Prepare certificate files for import or replacement'**
  String get pluginsTlsIdentitiesImport;

  /// No description provided for @pluginsTlsIdentitiesReplace.
  ///
  /// In en, this message translates to:
  /// **'Replace with checked files'**
  String get pluginsTlsIdentitiesReplace;

  /// No description provided for @pluginsTlsIdentitiesSave.
  ///
  /// In en, this message translates to:
  /// **'Save as a new identity'**
  String get pluginsTlsIdentitiesSave;

  /// No description provided for @pluginsTlsIdentitiesSaved.
  ///
  /// In en, this message translates to:
  /// **'Saved. Review the identity and revision below, then select it for a new start.'**
  String get pluginsTlsIdentitiesSaved;

  /// No description provided for @pluginsTlsIdentitiesSavedMode.
  ///
  /// In en, this message translates to:
  /// **'Next start: saved identity. The host checks its certificate validity at startup.'**
  String get pluginsTlsIdentitiesSavedMode;

  /// No description provided for @pluginsTlsIdentitiesSelect.
  ///
  /// In en, this message translates to:
  /// **'Use for next start'**
  String get pluginsTlsIdentitiesSelect;

  /// No description provided for @pluginsTlsIdentitiesStale.
  ///
  /// In en, this message translates to:
  /// **'The selected identity changed, was disabled, or has not been refreshed. Select a current identity again.'**
  String get pluginsTlsIdentitiesStale;

  /// No description provided for @pluginsTlsIdentitiesTitle.
  ///
  /// In en, this message translates to:
  /// **'Saved TLS identities'**
  String get pluginsTlsIdentitiesTitle;

  /// No description provided for @pluginsTlsIdentitiesUnknownHint.
  ///
  /// In en, this message translates to:
  /// **'Refresh and inspect the records before acknowledging. A missing receipt does not mean the change failed; do not create it again without checking.'**
  String get pluginsTlsIdentitiesUnknownHint;

  /// No description provided for @pluginsTlsIdentitiesUseFile.
  ///
  /// In en, this message translates to:
  /// **'Use checked files for next start'**
  String get pluginsTlsIdentitiesUseFile;

  /// No description provided for @pluginsTransform.
  ///
  /// In en, this message translates to:
  /// **'Transform'**
  String get pluginsTransform;

  /// No description provided for @pluginsTransformUnknown.
  ///
  /// In en, this message translates to:
  /// **'The transformation could not be confirmed'**
  String get pluginsTransformUnknown;

  /// No description provided for @pluginsUiExecution.
  ///
  /// In en, this message translates to:
  /// **'Plugin execution did not finish. Reopen the view and try again.'**
  String get pluginsUiExecution;

  /// No description provided for @pluginsUiRejected.
  ///
  /// In en, this message translates to:
  /// **'The plugin action was not accepted. Check the input and current permissions.'**
  String get pluginsUiRejected;

  /// No description provided for @pluginsUiUnavailable.
  ///
  /// In en, this message translates to:
  /// **'The plugin is unavailable. Check its status and reopen the view.'**
  String get pluginsUiUnavailable;

  /// No description provided for @pluginsUnavailableView.
  ///
  /// In en, this message translates to:
  /// **'Plugin view unavailable'**
  String get pluginsUnavailableView;

  /// No description provided for @pluginsUnconfirmed.
  ///
  /// In en, this message translates to:
  /// **'{reason}. The operation is unconfirmed. Check the refreshed status before choosing again.'**
  String pluginsUnconfirmed(String reason);

  /// No description provided for @pluginsUninstallKeepContent.
  ///
  /// In en, this message translates to:
  /// **'Uninstall (keep content)'**
  String get pluginsUninstallKeepContent;

  /// No description provided for @pluginsUninstallUnknown.
  ///
  /// In en, this message translates to:
  /// **'Uninstallation could not be confirmed'**
  String get pluginsUninstallUnknown;

  /// No description provided for @pluginsUninstalled.
  ///
  /// In en, this message translates to:
  /// **'Uninstalled. Your existing content has been preserved.'**
  String get pluginsUninstalled;

  /// No description provided for @pluginsUpdatingView.
  ///
  /// In en, this message translates to:
  /// **'Updating preview…'**
  String get pluginsUpdatingView;

  /// No description provided for @pluginsUseText.
  ///
  /// In en, this message translates to:
  /// **'Use text instead'**
  String get pluginsUseText;

  /// No description provided for @pluginsUseTransform.
  ///
  /// In en, this message translates to:
  /// **'Use transform'**
  String get pluginsUseTransform;

  /// No description provided for @pluginsViewFailed.
  ///
  /// In en, this message translates to:
  /// **'The plugin view could not be opened'**
  String get pluginsViewFailed;

  /// No description provided for @pluginsWorkbench.
  ///
  /// In en, this message translates to:
  /// **'Workbench plugin'**
  String get pluginsWorkbench;

  /// No description provided for @pluginsWorkbenchReadOnly.
  ///
  /// In en, this message translates to:
  /// **'The plugin is allowed, but this workbench is read-only. Resolve the library or plugin issue, then refresh its status.'**
  String get pluginsWorkbenchReadOnly;

  /// No description provided for @recoveryAllFiles.
  ///
  /// In en, this message translates to:
  /// **'All files'**
  String get recoveryAllFiles;

  /// No description provided for @recoveryBackupExists.
  ///
  /// In en, this message translates to:
  /// **'A file already exists at the backup location. Choose a new filename.'**
  String get recoveryBackupExists;

  /// No description provided for @recoveryBackupFile.
  ///
  /// In en, this message translates to:
  /// **'Library backup'**
  String get recoveryBackupFile;

  /// No description provided for @recoveryBackupUnknown.
  ///
  /// In en, this message translates to:
  /// **'The backup result needs checking. Keep the current file and inspect the save location.'**
  String get recoveryBackupUnknown;

  /// No description provided for @recoveryBindingMissing.
  ///
  /// In en, this message translates to:
  /// **'This library has no bound protection file, so the selected file cannot be matched to it.'**
  String get recoveryBindingMissing;

  /// No description provided for @recoveryBusy.
  ///
  /// In en, this message translates to:
  /// **'This library is in use by another process. Close the other window and try again.'**
  String get recoveryBusy;

  /// No description provided for @recoveryChooseKey.
  ///
  /// In en, this message translates to:
  /// **'Choose recovery file'**
  String get recoveryChooseKey;

  /// No description provided for @recoveryCloseFirst.
  ///
  /// In en, this message translates to:
  /// **'The workspace is still running. Close it before switching libraries.'**
  String get recoveryCloseFirst;

  /// No description provided for @recoveryClosing.
  ///
  /// In en, this message translates to:
  /// **'Waiting for the original content service to exit. Reopening and recovery remain unavailable until exit is confirmed.'**
  String get recoveryClosing;

  /// No description provided for @recoveryClosingUnconfirmed.
  ///
  /// In en, this message translates to:
  /// **'Shutdown is still unconfirmed; the original service remains under observation. A timeout does not cancel committed or remote effects.'**
  String get recoveryClosingUnconfirmed;

  /// No description provided for @recoveryFailed.
  ///
  /// In en, this message translates to:
  /// **'Recovery did not finish. Keep the original files and try again.'**
  String get recoveryFailed;

  /// No description provided for @recoveryIdentityBusy.
  ///
  /// In en, this message translates to:
  /// **'Another copy of this library is in use. Close that workspace before opening this copy.'**
  String get recoveryIdentityBusy;

  /// No description provided for @recoveryIdentityMismatch.
  ///
  /// In en, this message translates to:
  /// **'The registered library identity does not match. Keep the original data and restore the correct backup.'**
  String get recoveryIdentityMismatch;

  /// No description provided for @recoveryKeyFile.
  ///
  /// In en, this message translates to:
  /// **'Library protection file'**
  String get recoveryKeyFile;

  /// No description provided for @recoveryKeyGuide.
  ///
  /// In en, this message translates to:
  /// **'If the protection file is missing or damaged, select a backup of it. The file must belong to this library and requires the original system account.'**
  String get recoveryKeyGuide;

  /// No description provided for @recoveryKeyMismatch.
  ///
  /// In en, this message translates to:
  /// **'The library key does not match or cannot be decrypted. Use the original file and system account.'**
  String get recoveryKeyMismatch;

  /// No description provided for @recoveryKeyUnknown.
  ///
  /// In en, this message translates to:
  /// **'The recovery result needs checking. Try opening again; a copy of the previous protection file was retained if one existed.'**
  String get recoveryKeyUnknown;

  /// No description provided for @recoveryLibraryInvalid.
  ///
  /// In en, this message translates to:
  /// **'The library could not be verified or opened. Keep the original library and protection key, then try again.'**
  String get recoveryLibraryInvalid;

  /// No description provided for @recoveryMaintenance.
  ///
  /// In en, this message translates to:
  /// **'The library needs attention. Keep the original files and review the diagnostic details.'**
  String get recoveryMaintenance;

  /// No description provided for @recoveryMigrationIncomplete.
  ///
  /// In en, this message translates to:
  /// **'This library migration is incomplete. Check the migration report in its folder, keep the source library, and retry with a new destination.'**
  String get recoveryMigrationIncomplete;

  /// No description provided for @recoveryMissingKey.
  ///
  /// In en, this message translates to:
  /// **'The library protection key is missing. Restore the original .audit-key file and try again.'**
  String get recoveryMissingKey;

  /// No description provided for @recoveryMissingLibrary.
  ///
  /// In en, this message translates to:
  /// **'The protection key exists, but the library is missing or empty. Restore the original library.'**
  String get recoveryMissingLibrary;

  /// No description provided for @recoveryOpenFailed.
  ///
  /// In en, this message translates to:
  /// **'The workspace could not open. Check the plugin files and data folder, then try again.'**
  String get recoveryOpenFailed;

  /// No description provided for @recoveryPluginUnavailable.
  ///
  /// In en, this message translates to:
  /// **'The workspace plugin is unavailable. Existing content can still be viewed and exported.'**
  String get recoveryPluginUnavailable;

  /// No description provided for @recoveryRegistryInvalid.
  ///
  /// In en, this message translates to:
  /// **'The active library registration is damaged or unsupported. Opening stopped to preserve the data.'**
  String get recoveryRegistryInvalid;

  /// No description provided for @recoveryRegistryUnreadable.
  ///
  /// In en, this message translates to:
  /// **'The active library or registration file cannot be read. Check the original location; no replacement library will be created automatically.'**
  String get recoveryRegistryUnreadable;

  /// No description provided for @recoveryRetry.
  ///
  /// In en, this message translates to:
  /// **'Try again'**
  String get recoveryRetry;

  /// No description provided for @recoverySnapshot.
  ///
  /// In en, this message translates to:
  /// **'Restore library backup'**
  String get recoverySnapshot;

  /// No description provided for @recoverySnapshotGuide.
  ///
  /// In en, this message translates to:
  /// **'You can restore a library backup to a new folder and switch to it. The original folder is kept. This restores content as it was when backed up and requires the original system account.'**
  String get recoverySnapshotGuide;

  /// No description provided for @recoverySnapshotInvalid.
  ///
  /// In en, this message translates to:
  /// **'The library backup format or integrity check is invalid. Keep the original backup file.'**
  String get recoverySnapshotInvalid;

  /// No description provided for @recoverySnapshotUnknown.
  ///
  /// In en, this message translates to:
  /// **'The restore result needs checking. Inspect the destination folder; the original library was not replaced.'**
  String get recoverySnapshotUnknown;

  /// No description provided for @recoverySwitchUnconfirmed.
  ///
  /// In en, this message translates to:
  /// **'The backup was restored to {path}, but switching could not be confirmed. Keep this folder and reopen the workspace to check.'**
  String recoverySwitchUnconfirmed(String path);

  /// No description provided for @recoverySwitchUnknown.
  ///
  /// In en, this message translates to:
  /// **'The library switch is unconfirmed. Reopen the workspace to check.'**
  String get recoverySwitchUnknown;

  /// No description provided for @recoveryTargetExists.
  ///
  /// In en, this message translates to:
  /// **'The restore destination already exists. Choose a new folder that does not exist yet.'**
  String get recoveryTargetExists;

  /// No description provided for @recoveryTitle.
  ///
  /// In en, this message translates to:
  /// **'Reopen workspace'**
  String get recoveryTitle;

  /// No description provided for @recoveryWebCreate.
  ///
  /// In en, this message translates to:
  /// **'Create local workspace'**
  String get recoveryWebCreate;

  /// No description provided for @recoveryWebLegacy.
  ///
  /// In en, this message translates to:
  /// **'Existing browser content was found. Open it to continue; it has not been migrated or replaced.'**
  String get recoveryWebLegacy;

  /// No description provided for @recoveryWebLocal.
  ///
  /// In en, this message translates to:
  /// **'Your content stays in this browser on this device.'**
  String get recoveryWebLocal;

  /// No description provided for @recoveryWebOpenLegacy.
  ///
  /// In en, this message translates to:
  /// **'Open existing content'**
  String get recoveryWebOpenLegacy;

  /// No description provided for @recoveryWebUnavailable.
  ///
  /// In en, this message translates to:
  /// **'This browser workspace could not be opened. Close other Morrow tabs and try again. Existing data has not been replaced.'**
  String get recoveryWebUnavailable;

  /// No description provided for @shutdownBackground.
  ///
  /// In en, this message translates to:
  /// **'Continue in background'**
  String get shutdownBackground;

  /// No description provided for @shutdownBackgroundHint.
  ///
  /// In en, this message translates to:
  /// **'Morrow will close when the service exits. If shutdown fails or takes much longer, this window will reappear.'**
  String get shutdownBackgroundHint;

  /// No description provided for @shutdownFailure.
  ///
  /// In en, this message translates to:
  /// **'Shutdown reported a problem. The content service is still being observed.'**
  String get shutdownFailure;

  /// No description provided for @shutdownStillRunning.
  ///
  /// In en, this message translates to:
  /// **'Shutdown is taking longer than expected. The content service is still being observed.'**
  String get shutdownStillRunning;

  /// No description provided for @shutdownTitle.
  ///
  /// In en, this message translates to:
  /// **'Closing workspace'**
  String get shutdownTitle;

  /// No description provided for @shutdownWaiting.
  ///
  /// In en, this message translates to:
  /// **'Waiting for the content service to exit. The library stays owned until its exit is confirmed.'**
  String get shutdownWaiting;

  /// Visual interface: ApplyColor
  ///
  /// In en, this message translates to:
  /// **'Apply color'**
  String get visualApplyColor;

  /// Visual interface: ApplyComponent
  ///
  /// In en, this message translates to:
  /// **'Apply to this component'**
  String get visualApplyComponent;

  /// Visual interface: ApplyTexture
  ///
  /// In en, this message translates to:
  /// **'Apply media'**
  String get visualApplyTexture;

  /// Visual interface: AttachmentDetails
  ///
  /// In en, this message translates to:
  /// **'{extension} · {size} · {action}'**
  String visualAttachmentDetails(String action, String extension, String size);

  /// Visual interface: AttachmentFailure
  ///
  /// In en, this message translates to:
  /// **'File operation failed. Check the file and available storage.'**
  String get visualAttachmentFailure;

  /// Visual interface: AttachmentPreview
  ///
  /// In en, this message translates to:
  /// **'Local attachment preview'**
  String get visualAttachmentPreview;

  /// Visual interface: AttachmentReadFailure
  ///
  /// In en, this message translates to:
  /// **'The attachment could not be read. Import it again.'**
  String get visualAttachmentReadFailure;

  /// Visual interface: Audio
  ///
  /// In en, this message translates to:
  /// **'Audio'**
  String get visualAudio;

  /// Visual interface: AudioStateFailure
  ///
  /// In en, this message translates to:
  /// **'The audio state could not be confirmed. Try again.'**
  String get visualAudioStateFailure;

  /// Visual interface: AutoLyrics
  ///
  /// In en, this message translates to:
  /// **'Find missing lyrics online automatically'**
  String get visualAutoLyrics;

  /// Visual interface: Cancel
  ///
  /// In en, this message translates to:
  /// **'Cancel'**
  String get visualCancel;

  /// Visual interface: ChangeCover
  ///
  /// In en, this message translates to:
  /// **'Change album artwork'**
  String get visualChangeCover;

  /// Visual interface: ChooseAudio
  ///
  /// In en, this message translates to:
  /// **'Choose an audio file or an LRC lyric file with the same name.'**
  String get visualChooseAudio;

  /// Visual interface: ChooseLyrics
  ///
  /// In en, this message translates to:
  /// **'Choose an LRC or TXT lyric file.'**
  String get visualChooseLyrics;

  /// Visual interface: ClickPreview
  ///
  /// In en, this message translates to:
  /// **'Select to preview'**
  String get visualClickPreview;

  /// Visual interface: Close
  ///
  /// In en, this message translates to:
  /// **'Close'**
  String get visualClose;

  /// Visual interface: CloseDialog
  ///
  /// In en, this message translates to:
  /// **'Close dialog'**
  String get visualCloseDialog;

  /// Visual interface: CloseWindow
  ///
  /// In en, this message translates to:
  /// **'Close window'**
  String get visualCloseWindow;

  /// Visual interface: CollapsePlaylist
  ///
  /// In en, this message translates to:
  /// **'Collapse playlist'**
  String get visualCollapsePlaylist;

  /// Visual interface: ColorGuide
  ///
  /// In en, this message translates to:
  /// **'Drag the compass to choose hue and saturation, then adjust brightness. You can also enter a color value.'**
  String get visualColorGuide;

  /// Visual interface: ColorTitle
  ///
  /// In en, this message translates to:
  /// **'Add color to your space'**
  String get visualColorTitle;

  /// Visual interface: ComponentCompass
  ///
  /// In en, this message translates to:
  /// **'{title} · Color compass'**
  String visualComponentCompass(String title);

  /// Visual interface: Components
  ///
  /// In en, this message translates to:
  /// **'Components and cards'**
  String get visualComponents;

  /// Visual interface: ComponentsGuide
  ///
  /// In en, this message translates to:
  /// **'Each item follows the theme by default. Customize one card without changing the others.'**
  String get visualComponentsGuide;

  /// Visual interface: CornerTips1
  ///
  /// In en, this message translates to:
  /// **'Not every idea has to be useful.\nSome simply make today more interesting.'**
  String get visualCornerTips1;

  /// Visual interface: CornerTips2
  ///
  /// In en, this message translates to:
  /// **'Write it down, then let it grow.\nAn idea does not have to arrive complete.'**
  String get visualCornerTips2;

  /// Visual interface: CornerTips3
  ///
  /// In en, this message translates to:
  /// **'Leave a little space for yourself.\nCuriosity needs room to breathe.'**
  String get visualCornerTips3;

  /// Visual interface: CornerTips4
  ///
  /// In en, this message translates to:
  /// **'Try something new today.\nA small detour can bring a surprise.'**
  String get visualCornerTips4;

  /// Visual interface: CornerTips5
  ///
  /// In en, this message translates to:
  /// **'Daydreaming can lead somewhere.\nGive your thoughts a path to wander.'**
  String get visualCornerTips5;

  /// Visual interface: CornerTips6
  ///
  /// In en, this message translates to:
  /// **'Make time for what you love.\nThere is no need to prove its worth.'**
  String get visualCornerTips6;

  /// Visual interface: CornerTips7
  ///
  /// In en, this message translates to:
  /// **'Progress can be small.\nBeing willing to start already matters.'**
  String get visualCornerTips7;

  /// Visual interface: CornerTips8
  ///
  /// In en, this message translates to:
  /// **'Look out the window sometimes.\nLife is a source of inspiration too.'**
  String get visualCornerTips8;

  /// Visual interface: Cover
  ///
  /// In en, this message translates to:
  /// **'Artwork'**
  String get visualCover;

  /// Visual interface: CustomCompass
  ///
  /// In en, this message translates to:
  /// **'Color compass · Custom'**
  String get visualCustomCompass;

  /// Visual interface: CustomMaterialGuide
  ///
  /// In en, this message translates to:
  /// **'Turn off to follow the theme while keeping this item\'\'s custom settings.'**
  String get visualCustomMaterialGuide;

  /// Visual interface: DefaultOpen
  ///
  /// In en, this message translates to:
  /// **'Open with default app'**
  String get visualDefaultOpen;

  /// Visual interface: DownloadOpen
  ///
  /// In en, this message translates to:
  /// **'Download to open'**
  String get visualDownloadOpen;

  /// Visual interface: EmbeddedLyrics
  ///
  /// In en, this message translates to:
  /// **'Embedded in audio'**
  String get visualEmbeddedLyrics;

  /// Visual interface: ExpandPlaylist
  ///
  /// In en, this message translates to:
  /// **'Expand playlist'**
  String get visualExpandPlaylist;

  /// Visual interface: File
  ///
  /// In en, this message translates to:
  /// **'File'**
  String get visualFile;

  /// Visual interface: FileOpenFailure
  ///
  /// In en, this message translates to:
  /// **'The file could not be opened. Install a compatible app, or save the attachment and open it there.'**
  String get visualFileOpenFailure;

  /// Visual interface: FileRetry
  ///
  /// In en, this message translates to:
  /// **'File operation failed. Try again.'**
  String get visualFileRetry;

  /// Visual interface: FindLyrics
  ///
  /// In en, this message translates to:
  /// **'Find lyrics'**
  String get visualFindLyrics;

  /// Visual interface: FindLyricsGuide
  ///
  /// In en, this message translates to:
  /// **'Search LRCLIB by song and artist, then choose the matching version.'**
  String get visualFindLyricsGuide;

  /// Interface style and component material inheritance
  ///
  /// In en, this message translates to:
  /// **'Follow chain: {path}'**
  String visualFollowChain(String path);

  /// Interface style and component material inheritance
  ///
  /// In en, this message translates to:
  /// **'Follows: {name}'**
  String visualFollowComponent(String name);

  /// Interface style and component material inheritance
  ///
  /// In en, this message translates to:
  /// **'Would create a cycle'**
  String get visualFollowCycle;

  /// Interface style and component material inheritance
  ///
  /// In en, this message translates to:
  /// **'Follow another component; unlink to restore your own settings.'**
  String get visualFollowGuide;

  /// Visual interface: FollowTheme
  ///
  /// In en, this message translates to:
  /// **'Follow theme'**
  String get visualFollowTheme;

  /// Visual interface: FooterLyrics
  ///
  /// In en, this message translates to:
  /// **'Show lyrics in footer'**
  String get visualFooterLyrics;

  /// Visual interface: FooterTips
  ///
  /// In en, this message translates to:
  /// **'Show tips in footer'**
  String get visualFooterTips;

  /// Visual interface: FooterTips1
  ///
  /// In en, this message translates to:
  /// **'Nothing urgent. Give curiosity a little time.'**
  String get visualFooterTips1;

  /// Visual interface: FooterTips10
  ///
  /// In en, this message translates to:
  /// **'You do not have to fill every minute. Leave some room.'**
  String get visualFooterTips10;

  /// Visual interface: FooterTips2
  ///
  /// In en, this message translates to:
  /// **'Jot down a thought. You can organize it later.'**
  String get visualFooterTips2;

  /// Visual interface: FooterTips3
  ///
  /// In en, this message translates to:
  /// **'Break a big idea into one small step for today.'**
  String get visualFooterTips3;

  /// Visual interface: FooterTips4
  ///
  /// In en, this message translates to:
  /// **'Stretch a little and rest your eyes.'**
  String get visualFooterTips4;

  /// Visual interface: FooterTips5
  ///
  /// In en, this message translates to:
  /// **'An idea is allowed to remain unanswered for now.'**
  String get visualFooterTips5;

  /// Visual interface: FooterTips6
  ///
  /// In en, this message translates to:
  /// **'Some discoveries arrive when you slow down.'**
  String get visualFooterTips6;

  /// Visual interface: FooterTips7
  ///
  /// In en, this message translates to:
  /// **'Saving a detail is a way to nurture an idea.'**
  String get visualFooterTips7;

  /// Visual interface: FooterTips8
  ///
  /// In en, this message translates to:
  /// **'A quick note today might become tomorrow\'\'s beginning.'**
  String get visualFooterTips8;

  /// Visual interface: FooterTips9
  ///
  /// In en, this message translates to:
  /// **'Let your mind wander, then return to what you enjoy.'**
  String get visualFooterTips9;

  /// Visual interface: Frosting
  ///
  /// In en, this message translates to:
  /// **'Frosting'**
  String get visualFrosting;

  /// Visual interface: Gif
  ///
  /// In en, this message translates to:
  /// **'Animated GIF'**
  String get visualGif;

  /// Visual interface: HexColor
  ///
  /// In en, this message translates to:
  /// **'HEX color'**
  String get visualHexColor;

  /// Visual interface: HexInvalid
  ///
  /// In en, this message translates to:
  /// **'Enter a six-digit hexadecimal color.'**
  String get visualHexInvalid;

  /// Visual interface: Image
  ///
  /// In en, this message translates to:
  /// **'Image'**
  String get visualImage;

  /// Visual interface: ImageDecodeFailure
  ///
  /// In en, this message translates to:
  /// **'This image cannot be decoded. Save it and open it with another app.'**
  String get visualImageDecodeFailure;

  /// Visual interface: ImageLoadFailure
  ///
  /// In en, this message translates to:
  /// **'Image could not be loaded: {name}'**
  String visualImageLoadFailure(String name);

  /// Visual interface: ImageNotImported
  ///
  /// In en, this message translates to:
  /// **'{name} (image not imported)'**
  String visualImageNotImported(String name);

  /// Visual interface: ImageUnavailable
  ///
  /// In en, this message translates to:
  /// **'Image unavailable: {name}'**
  String visualImageUnavailable(String name);

  /// Visual interface: ImportFailure
  ///
  /// In en, this message translates to:
  /// **'Import failed. Check the file, encoding, and available storage.'**
  String get visualImportFailure;

  /// Visual interface: ImportLyrics
  ///
  /// In en, this message translates to:
  /// **'Import lyrics'**
  String get visualImportLyrics;

  /// Visual interface: ImportMusic
  ///
  /// In en, this message translates to:
  /// **'Import music'**
  String get visualImportMusic;

  /// Visual interface: ImportMusicHint
  ///
  /// In en, this message translates to:
  /// **'Select + to import local songs'**
  String get visualImportMusicHint;

  /// Visual interface: IndependentMaterial
  ///
  /// In en, this message translates to:
  /// **'Custom material'**
  String get visualIndependentMaterial;

  /// No description provided for @visualInheritColor.
  ///
  /// In en, this message translates to:
  /// **'Use theme tint'**
  String get visualInheritColor;

  /// Visual interface: LinkFailure
  ///
  /// In en, this message translates to:
  /// **'The link could not be opened. Copy the address and try again.'**
  String get visualLinkFailure;

  /// Visual interface: LoadImage
  ///
  /// In en, this message translates to:
  /// **'Load image · {name}'**
  String visualLoadImage(String name);

  /// Visual interface: Loading
  ///
  /// In en, this message translates to:
  /// **'Loading…'**
  String get visualLoading;

  /// Visual interface: LyricsEmpty
  ///
  /// In en, this message translates to:
  /// **'The lyric file is empty.'**
  String get visualLyricsEmpty;

  /// Visual interface: LyricsFile
  ///
  /// In en, this message translates to:
  /// **'Lyric file'**
  String get visualLyricsFile;

  /// Visual interface: LyricsImportHint
  ///
  /// In en, this message translates to:
  /// **'Import a lyric file or search online.'**
  String get visualLyricsImportHint;

  /// Visual interface: LyricsLoading
  ///
  /// In en, this message translates to:
  /// **'Loading lyrics…'**
  String get visualLyricsLoading;

  /// Visual interface: LyricsMatch
  ///
  /// In en, this message translates to:
  /// **'{album}\n{kind} · {seconds, plural, =1{1 second} other{{seconds} seconds}}'**
  String visualLyricsMatch(String album, String kind, int seconds);

  /// Visual interface: LyricsMissing
  ///
  /// In en, this message translates to:
  /// **'No lyrics found. Import a file or search again.'**
  String get visualLyricsMissing;

  /// Visual interface: LyricsNotFound
  ///
  /// In en, this message translates to:
  /// **'No lyrics found. Try changing the song title or artist.'**
  String get visualLyricsNotFound;

  /// Visual interface: LyricsOnPlay
  ///
  /// In en, this message translates to:
  /// **'Load lyrics automatically during playback'**
  String get visualLyricsOnPlay;

  /// Visual interface: LyricsParseFailure
  ///
  /// In en, this message translates to:
  /// **'Lyrics could not be parsed. Try importing them again.'**
  String get visualLyricsParseFailure;

  /// Visual interface: LyricsReadFailure
  ///
  /// In en, this message translates to:
  /// **'Lyrics could not be loaded. Import them manually or try again.'**
  String get visualLyricsReadFailure;

  /// Visual interface: LyricsServiceFailure
  ///
  /// In en, this message translates to:
  /// **'Could not connect to the lyric service. Try again later or import local lyrics.'**
  String get visualLyricsServiceFailure;

  /// Visual interface: LyricsSize
  ///
  /// In en, this message translates to:
  /// **'Keep lyric files within 1 MB.'**
  String get visualLyricsSize;

  /// Visual interface: LyricsSources
  ///
  /// In en, this message translates to:
  /// **'Local file → Embedded → LRCLIB'**
  String get visualLyricsSources;

  /// Visual interface: LyricsVersions
  ///
  /// In en, this message translates to:
  /// **'Multiple versions found. Choose one in search.'**
  String get visualLyricsVersions;

  /// Visual interface: MaterialPreview
  ///
  /// In en, this message translates to:
  /// **'Material preview'**
  String get visualMaterialPreview;

  /// Interface style and component material inheritance
  ///
  /// In en, this message translates to:
  /// **'Material source'**
  String get visualMaterialSource;

  /// Visual interface: Maximize
  ///
  /// In en, this message translates to:
  /// **'Maximize'**
  String get visualMaximize;

  /// Visual interface: MediaAddress
  ///
  /// In en, this message translates to:
  /// **'Media address'**
  String get visualMediaAddress;

  /// Visual interface: MediaAddressInvalid
  ///
  /// In en, this message translates to:
  /// **'Enter a valid HTTP or HTTPS address without login information.'**
  String get visualMediaAddressInvalid;

  /// Visual interface: MediaPreviewFailure
  ///
  /// In en, this message translates to:
  /// **'This media cannot be previewed. Save it and open it with another app.'**
  String get visualMediaPreviewFailure;

  /// Visual interface: MediaType
  ///
  /// In en, this message translates to:
  /// **'Media type'**
  String get visualMediaType;

  /// Visual interface: Minimize
  ///
  /// In en, this message translates to:
  /// **'Minimize'**
  String get visualMinimize;

  /// Visual interface: Music
  ///
  /// In en, this message translates to:
  /// **'Music'**
  String get visualMusic;

  /// Visual interface: MusicEmptyTitle
  ///
  /// In en, this message translates to:
  /// **'Make room for music'**
  String get visualMusicEmptyTitle;

  /// Visual interface: MusicPlayer
  ///
  /// In en, this message translates to:
  /// **'Music player'**
  String get visualMusicPlayer;

  /// Visual interface: NextTrack
  ///
  /// In en, this message translates to:
  /// **'Next track'**
  String get visualNextTrack;

  /// Visual interface: NoLyricsRead
  ///
  /// In en, this message translates to:
  /// **'No lyrics loaded'**
  String get visualNoLyricsRead;

  /// Visual interface: NoLyricsTitle
  ///
  /// In en, this message translates to:
  /// **'♪ {title} · No lyrics'**
  String visualNoLyricsTitle(String title);

  /// Visual interface: NoTimeline
  ///
  /// In en, this message translates to:
  /// **'No timing data'**
  String get visualNoTimeline;

  /// Visual interface: Opacity
  ///
  /// In en, this message translates to:
  /// **'Opacity'**
  String get visualOpacity;

  /// Visual interface: OptionalArtist
  ///
  /// In en, this message translates to:
  /// **'Artist (optional)'**
  String get visualOptionalArtist;

  /// Interface style and component material inheritance
  ///
  /// In en, this message translates to:
  /// **'Theme or own settings'**
  String get visualOwnMaterial;

  /// Visual interface: PauseMusic
  ///
  /// In en, this message translates to:
  /// **'Pause music'**
  String get visualPauseMusic;

  /// Visual interface: Paused
  ///
  /// In en, this message translates to:
  /// **'Paused'**
  String get visualPaused;

  /// Visual interface: PlainLyrics
  ///
  /// In en, this message translates to:
  /// **'Plain-text lyrics'**
  String get visualPlainLyrics;

  /// Visual interface: PlayMusic
  ///
  /// In en, this message translates to:
  /// **'Play music'**
  String get visualPlayMusic;

  /// Visual interface: PlaybackFailure
  ///
  /// In en, this message translates to:
  /// **'This song cannot be played. Check the file or try another audio format.'**
  String get visualPlaybackFailure;

  /// Visual interface: PlaybackPosition
  ///
  /// In en, this message translates to:
  /// **'{index} / {count} · {state}'**
  String visualPlaybackPosition(int count, int index, String state);

  /// Visual interface: PlaybackRequestFailure
  ///
  /// In en, this message translates to:
  /// **'Playback could not be started. Try again.'**
  String get visualPlaybackRequestFailure;

  /// Visual interface: Playing
  ///
  /// In en, this message translates to:
  /// **'Playing'**
  String get visualPlaying;

  /// Visual interface: PlaylistEmpty
  ///
  /// In en, this message translates to:
  /// **'Your playlist is empty'**
  String get visualPlaylistEmpty;

  /// Visual interface: PlaylistLyricsHint
  ///
  /// In en, this message translates to:
  /// **'Import LRC lyrics from the playlist menu'**
  String get visualPlaylistLyricsHint;

  /// Visual interface: PlaylistSaved
  ///
  /// In en, this message translates to:
  /// **'Playlist and lyrics are saved automatically'**
  String get visualPlaylistSaved;

  /// Visual interface: PlaylistUpdateFailure
  ///
  /// In en, this message translates to:
  /// **'The playlist could not be updated. Try again.'**
  String get visualPlaylistUpdateFailure;

  /// Visual interface: PreviewColor
  ///
  /// In en, this message translates to:
  /// **'Preview color'**
  String get visualPreviewColor;

  /// Visual interface: PreviousTrack
  ///
  /// In en, this message translates to:
  /// **'Previous track'**
  String get visualPreviousTrack;

  /// Visual interface: RemoveAttachment
  ///
  /// In en, this message translates to:
  /// **'Remove attachment'**
  String get visualRemoveAttachment;

  /// Visual interface: RemoveTrack
  ///
  /// In en, this message translates to:
  /// **'Remove from playlist'**
  String get visualRemoveTrack;

  /// No description provided for @visualResetMaterial.
  ///
  /// In en, this message translates to:
  /// **'Reset to theme'**
  String get visualResetMaterial;

  /// Visual interface: RestoreWindow
  ///
  /// In en, this message translates to:
  /// **'Restore'**
  String get visualRestoreWindow;

  /// Visual interface: SaveAttachment
  ///
  /// In en, this message translates to:
  /// **'Save attachment as'**
  String get visualSaveAttachment;

  /// Visual interface: Search
  ///
  /// In en, this message translates to:
  /// **'Search'**
  String get visualSearch;

  /// Visual interface: SearchLyrics
  ///
  /// In en, this message translates to:
  /// **'Search lyrics'**
  String get visualSearchLyrics;

  /// Visual interface: SongCover
  ///
  /// In en, this message translates to:
  /// **'Album artwork'**
  String get visualSongCover;

  /// Visual interface: SongTitle
  ///
  /// In en, this message translates to:
  /// **'Song title'**
  String get visualSongTitle;

  /// Visual interface: SyncedLyrics
  ///
  /// In en, this message translates to:
  /// **'Synced lyrics'**
  String get visualSyncedLyrics;

  /// Visual interface: TextureFailure
  ///
  /// In en, this message translates to:
  /// **'Media could not be loaded. Check the file, address, or format. Web media must also allow cross-origin access.'**
  String get visualTextureFailure;

  /// Visual interface: TextureLinkGuide
  ///
  /// In en, this message translates to:
  /// **'Paste a direct HTTP or HTTPS link to an image, GIF, or video. For shared web pages, first find the original media address.'**
  String get visualTextureLinkGuide;

  /// Visual interface: TextureLinkTitle
  ///
  /// In en, this message translates to:
  /// **'Bring in some inspiration'**
  String get visualTextureLinkTitle;

  /// Visual interface: TexturePlaybackGuide
  ///
  /// In en, this message translates to:
  /// **'Videos loop silently by default; enable sound in settings. Online media must permit access, including cross-origin loading on the web.'**
  String get visualTexturePlaybackGuide;

  /// No description provided for @visualTipsMaterialGuide.
  ///
  /// In en, this message translates to:
  /// **'Off keeps tips transparent; on uses the material below. Your custom values are retained.'**
  String get visualTipsMaterialGuide;

  /// No description provided for @visualTransparentTips.
  ///
  /// In en, this message translates to:
  /// **'Transparent overlay (default)'**
  String get visualTransparentTips;

  /// Visual interface: UseCustomMaterial
  ///
  /// In en, this message translates to:
  /// **'Use custom material'**
  String get visualUseCustomMaterial;

  /// Visual interface: Video
  ///
  /// In en, this message translates to:
  /// **'Video'**
  String get visualVideo;

  /// Visual interface: ViewLyrics
  ///
  /// In en, this message translates to:
  /// **'View lyrics'**
  String get visualViewLyrics;
}

class _AppLocalizationsDelegate
    extends LocalizationsDelegate<AppLocalizations> {
  const _AppLocalizationsDelegate();

  @override
  Future<AppLocalizations> load(Locale locale) {
    return SynchronousFuture<AppLocalizations>(lookupAppLocalizations(locale));
  }

  @override
  bool isSupported(Locale locale) => <String>[
    'de',
    'en',
    'es',
    'fr',
    'ja',
    'ko',
    'pt',
    'ru',
    'zh',
  ].contains(locale.languageCode);

  @override
  bool shouldReload(_AppLocalizationsDelegate old) => false;
}

AppLocalizations lookupAppLocalizations(Locale locale) {
  // Lookup logic when only language code is specified.
  switch (locale.languageCode) {
    case 'de':
      return AppLocalizationsDe();
    case 'en':
      return AppLocalizationsEn();
    case 'es':
      return AppLocalizationsEs();
    case 'fr':
      return AppLocalizationsFr();
    case 'ja':
      return AppLocalizationsJa();
    case 'ko':
      return AppLocalizationsKo();
    case 'pt':
      return AppLocalizationsPt();
    case 'ru':
      return AppLocalizationsRu();
    case 'zh':
      return AppLocalizationsZh();
  }

  throw FlutterError(
    'AppLocalizations.delegate failed to load unsupported locale "$locale". This is likely '
    'an issue with the localizations generation tool. Please file an issue '
    'on GitHub with a reproducible sample app and the gen-l10n configuration '
    'that was used.',
  );
}
