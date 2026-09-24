// ignore: unused_import
import 'package:intl/intl.dart' as intl;
import 'app_localizations.dart';

// ignore_for_file: type=lint

/// The translations for German (`de`).
class AppLocalizationsDe extends AppLocalizations {
  AppLocalizationsDe([String locale = 'de']) : super(locale);

  @override
  String get commonAppName => 'Morrow';

  @override
  String get commonCancel => 'Abbrechen';

  @override
  String commonCount(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count Einträge',
      one: '$count Eintrag',
      zero: 'Keine Einträge',
    );
    return '$_temp0';
  }

  @override
  String commonGreeting(String name) {
    return 'Hallo, $name';
  }

  @override
  String get importsAttachmentLimit =>
      'Höchstens 20 Anhänge auf einmal importieren. Füge die übrigen getrennt ein.';

  @override
  String get importsClipboardChanged =>
      'Zwischenablage während des Lesens geändert. Füge erneut ein.';

  @override
  String get importsEmbeddedImageUnreadable =>
      'Eingebettetes Bild konnte nicht gelesen werden.';

  @override
  String get importsEmbeddedImagesSeparate =>
      'Einige eingebettete Bilder müssen als separate Dateien importiert werden.';

  @override
  String get importsExcelValues =>
      'Werte und Formeln in Markdown konvertiert. Formatierung und verbundene Zellen bleiben im XML-Anhang erhalten.';

  @override
  String get importsExcelXmlKept =>
      'Die ursprüngliche Excel-Tabelle bleibt als XML-Anhang erhalten.';

  @override
  String get importsFileTooLarge =>
      'Die Zwischenablagedatei überschreitet 200 MB.';

  @override
  String get importsItemLimit =>
      'Nur die ersten 20 Elemente wurden gelesen. Füge weitere separat ein.';

  @override
  String get importsItemUnreadable =>
      'Ein Zwischenablageelement war nicht lesbar. Andere lesbare Inhalte bleiben erhalten.';

  @override
  String get importsLocalImageNotRead =>
      'Lokal verknüpfte Bilder werden nicht automatisch gelesen. Füge das Bild ein oder importiere die Originaldatei.';

  @override
  String get importsMergedTable =>
      'Verbundene Zellen in lesbare Tabelle konvertiert. Originalformatierung bleibt im HTML-Anhang erhalten.';

  @override
  String get importsOfficeBusy =>
      'Eine andere App verwendet die Zwischenablage. Office-Objekte wurden nicht gelesen.';

  @override
  String get importsOfficeEmbeddedKept =>
      'Das Office-Objekt bleibt als Originalanhang erhalten. Bearbeite Diagramme, Formeln und Layout in der ursprünglichen Anwendung.';

  @override
  String get importsOfficeExportFailed =>
      'Office-Objekt zu groß oder nicht exportierbar. Speichere es in der ursprünglichen App und importiere es dann.';

  @override
  String get importsOfficeReadFailed =>
      'Office-Inhalt konnte nicht gelesen werden. Andere Zwischenablageinhalte sind weiter verfügbar.';

  @override
  String get importsOfficeUnavailable =>
      'Die Office-Zwischenablage ist vorübergehend nicht verfügbar.';

  @override
  String get importsOfficeUnreadable =>
      'Ursprüngliches Office-Objekt nicht lesbar. Andere verfügbare Inhalte bleiben erhalten.';

  @override
  String get importsRichFallback =>
      'Einige Formatierungen konnten nicht konvertiert werden. Lesbarer Text bleibt erhalten.';

  @override
  String get importsRichTooLarge =>
      'Rich-Text überschreitet 2 MB. Importiere das Dokument als Anhang.';

  @override
  String get importsRtfTooLarge =>
      'RTF-Inhalt zu groß. Importiere das Originaldokument.';

  @override
  String get importsSpreadsheetTooLarge =>
      'Tabelle zu groß. Importiere die Excel-Datei.';

  @override
  String get importsTableConverted =>
      'Tabelle in Markdown konvertiert. Vollständige Daten bleiben im TSV-Anhang erhalten.';

  @override
  String get importsTextTooLarge =>
      'Der Text überschreitet 2 MB. Importiere ihn als Datei.';

  @override
  String get importsTotalTooLarge =>
      'Die eingefügten Dateien überschreiten zusammen 200 MB. Importiere kleinere Gruppen.';

  @override
  String get importsUnsupported =>
      'Zwischenablagezugriff wird hier nicht unterstützt. Importiere eine Datei.';

  @override
  String get mainActiveProjects => 'In Arbeit';

  @override
  String get mainAdjustCustomTone => 'Eigene Farbe anpassen';

  @override
  String get mainAmbientDetail => 'Fließendes Licht setzt sanfte Farbakzente.';

  @override
  String get mainAppTitle => 'Morrow — Raum für Ideen';

  @override
  String get mainAppearance => 'Darstellung';

  @override
  String get mainArrangeIdeas => 'Ideen sortieren';

  @override
  String mainAttachmentCount(int count) {
    return 'Anhänge · $count';
  }

  @override
  String mainAttachmentHint(int count, String name) {
    return '$count Anhänge · $name';
  }

  @override
  String get mainAttachmentLimit => 'Bis zu 20 Anhänge pro Eintrag.';

  @override
  String get mainAutosaveNotice =>
      'Darstellung und Ideen werden lokal gespeichert';

  @override
  String get mainAwaitDiscovery => 'Warten auf eine Entdeckung';

  @override
  String get mainBackToWorkbench => 'Zurück zum Arbeitsbereich';

  @override
  String get mainBackgroundCanvas => 'Hintergrundfläche';

  @override
  String get mainBackgroundSound => 'Hintergrundton abspielen';

  @override
  String get mainBodyHint =>
      'Schreibe deine Gedanken auf oder füge Inhalte ein…\n\nUnterstützt # Überschriften, Listen, Tabellen und Codeblöcke';

  @override
  String get mainBrightWhite => 'Weiß';

  @override
  String get mainBuiltinTexture => 'Integrierte Textur';

  @override
  String get mainCanvasCompass => 'Hintergrundfarbkreis';

  @override
  String get mainCaptureIdea => 'Idee festhalten';

  @override
  String get mainCaptureNow => 'Gedanken festhalten';

  @override
  String mainCardAttachments(int count, String name) {
    return 'Dateien $count · $name';
  }

  @override
  String get mainCategoryExperiment => 'Experiment';

  @override
  String get mainCategoryIdea => 'Idee';

  @override
  String get mainCategoryProject => 'Projekt';

  @override
  String get mainCategoryPrompt => 'Ablageort';

  @override
  String get mainChangeFailed =>
      'Änderung nicht gespeichert. Dein Entwurf bleibt erhalten; versuche es erneut.';

  @override
  String get mainCheckAgain => 'Erneut prüfen';

  @override
  String get mainClearSearch => 'Suche löschen';

  @override
  String get mainClipboardEmpty =>
      'Keine lesbaren Texte oder Dateien in der Zwischenablage. Kopiere eine Datei im Dateimanager oder nutze den Dateiimport.';

  @override
  String get mainClipboardReadFailed =>
      'Inhalt konnte nicht gelesen werden. Importiere eine Datei oder prüfe Datei- und Zwischenablageberechtigungen.';

  @override
  String get mainClipboardSupport =>
      'Unterstützt Markdown, Office-Rich-Text und Tabellen, Screenshots und Dateien. Komplexe Objekte behalten Originalanhänge. Bis zu 20 Anhänge mit je 200 MB.';

  @override
  String get mainCollapseSidebar => 'Seitenleiste einklappen';

  @override
  String get mainCompletedProjects => 'Abgeschlossen';

  @override
  String get mainComponentCompass => 'Komponentenfarbkreis';

  @override
  String get mainComponentEmpty => 'Leerzustand';

  @override
  String get mainComponentFooter => 'Tipps und Liedtexte unten';

  @override
  String get mainComponentHero => 'Übersichtskarte';

  @override
  String get mainComponentNavigation => 'Seitennavigation';

  @override
  String get mainComponentQuickCapture => 'Schnellerfassung';

  @override
  String get mainComponentSearch => 'Suchleiste';

  @override
  String get mainComponentSettings => 'Komponenten und Karten · Einstellungen';

  @override
  String get mainContentCommittedRefreshFailed =>
      'Der Vorgang wurde gespeichert, aber der aktuelle Inhalt konnte nicht geladen werden. Aktualisieren Sie die Ansicht.';

  @override
  String get mainContentProtection => 'Inhaltsschutz';

  @override
  String get mainContentRead => 'Inhalt gelesen';

  @override
  String mainContentReadFiles(int count) {
    return 'Inhalt gelesen; $count Anhänge erhalten';
  }

  @override
  String get mainCornerRadius => 'Eckenradius';

  @override
  String get mainCredentialSettings => 'Zugangsdaten';

  @override
  String get mainCrystal => 'Kristallklar';

  @override
  String get mainCrystalDetail =>
      'Leicht und klar, damit Farben durchscheinen.';

  @override
  String get mainCuriosity => 'Spannende Dinge beginnen\nmit etwas Neugier.';

  @override
  String get mainCustomCompass => 'Farbkreis · Benutzerdefiniert';

  @override
  String get mainCustomLightness => 'Eigene Helligkeit';

  @override
  String get mainCustomTheme => 'Eigene';

  @override
  String get mainDaily => 'Kleine Dinge';

  @override
  String get mainDailyExplore => 'Nimm dir zehn Minuten zum Entdecken';

  @override
  String get mainDailyIdea => 'Notiere eine Idee';

  @override
  String get mainDailyWater => 'Gönn dir ein Glas Wasser';

  @override
  String get mainDarkTheme => 'Dunkel';

  @override
  String get mainDeepBlack => 'Schwarz';

  @override
  String get mainDefaultCanvas => 'Standard';

  @override
  String get mainDefaultGlobalColor =>
      'Standard-Themenfarbe · Alle Bedienelemente';

  @override
  String get mainDelete => 'Löschen';

  @override
  String mainDeleted(String title) {
    return '„$title“ gelöscht';
  }

  @override
  String get mainDiagnosticDetails => 'Diagnosedetails';

  @override
  String get mainDone => 'Fertig';

  @override
  String get mainDraftHandoffRecoveryAssets => 'Fest zugeordnete Anhänge';

  @override
  String get mainDraftHandoffRecoveryCancel => 'Übergabe abbrechen';

  @override
  String get mainDraftHandoffRecoveryCancelBody =>
      'Diese unvollendete Übergabe abbrechen. Mit dem ursprünglichen Vorgang kann danach kein Folgeentwurf mehr erstellt werden. Der übergeordnete Entwurf bleibt unverändert.';

  @override
  String get mainDraftHandoffRecoveryCancelled => 'Übergabe abgebrochen';

  @override
  String get mainDraftHandoffRecoveryChildCommitted =>
      'Folgeentwurf gespeichert; übergeordneter Entwurf noch nicht stillgelegt';

  @override
  String get mainDraftHandoffRecoveryComplete => 'Folgeentwurf erstellen';

  @override
  String get mainDraftHandoffRecoveryCompleteBody =>
      'Folgeentwurf aus dem gespeicherten ursprünglichen Vorschlag erstellen. Dabei wird keine weitere endgültige Karte eingereicht.';

  @override
  String get mainDraftHandoffRecoveryConfirmTitle =>
      'Aktion zur Entwurfsübergabe bestätigen';

  @override
  String get mainDraftHandoffRecoveryConfirmed =>
      'Der Vorgang wurde bestätigt und gespeichert.';

  @override
  String get mainDraftHandoffRecoveryConflict =>
      'Der aktuelle Zustand hat sich geändert und muss geprüft werden';

  @override
  String get mainDraftHandoffRecoveryEmpty =>
      'Keine Übergaben zur Prüfung vorhanden';

  @override
  String get mainDraftHandoffRecoveryFailed =>
      'Der Vorgang konnte nicht abgeschlossen werden. Aktualisieren und prüfen Sie den Eintrag.';

  @override
  String get mainDraftHandoffRecoveryFields =>
      'Inhalt des ursprünglichen Entwurfs';

  @override
  String get mainDraftHandoffRecoveryInspect => 'Anzeigen und prüfen';

  @override
  String get mainDraftHandoffRecoveryIntro =>
      'Hier werden nur gespeicherte Übergaben angezeigt. Beim Öffnen dieser Seite werden keine Inhalte eingereicht.';

  @override
  String get mainDraftHandoffRecoveryParentRetired => 'Übergabe abgeschlossen';

  @override
  String get mainDraftHandoffRecoveryPending =>
      'Der Folgeentwurf wurde noch nicht erstellt';

  @override
  String get mainDraftHandoffRecoveryReadOnly =>
      'Dieser Arbeitsbereich erlaubt nur die Ansicht';

  @override
  String get mainDraftHandoffRecoveryRetire =>
      'Übergeordneten Entwurf stilllegen';

  @override
  String get mainDraftHandoffRecoveryRetireBody =>
      'Nachdem Sie geprüft haben, dass der Folgeentwurf gespeichert wurde, markieren Sie den übergeordneten Entwurf als stillgelegt.';

  @override
  String get mainDraftHandoffRecoverySelection =>
      'Wählen Sie einen Eintrag, um den vollständigen Entwurf anzuzeigen.';

  @override
  String get mainDraftHandoffRecoveryTitle =>
      'Wiederherstellung der Entwurfsübergabe';

  @override
  String get mainDraftHandoffRecoveryUnknown =>
      'Das Ergebnis des Vorgangs ist noch nicht bestätigt. Prüfen Sie den ursprünglichen Eintrag und wiederholen Sie den Vorgang nicht.';

  @override
  String get mainDraftImportRecoveryCancelBody =>
      'Nach dem Abbruch wird die ursprüngliche Verzichtsanfrage nicht mehr ausgeführt. Dadurch wird der Anhang nicht gelöscht und andere Änderungen werden nicht rückgängig gemacht.';

  @override
  String get mainDraftImportRecoveryCancelDecision =>
      'Verzichtsanfrage abbrechen';

  @override
  String get mainDraftImportRecoveryCancelTitle =>
      'Diese Verzichtsanfrage abbrechen?';

  @override
  String get mainDraftImportRecoveryCancelled =>
      'Abgebrochen: Die ursprüngliche Verzichtsanfrage wird nicht mehr ausgeführt.';

  @override
  String get mainDraftImportRecoveryClose => 'Schließen';

  @override
  String get mainDraftImportRecoveryCommitted =>
      'Abgeschlossen: Der Import dieses Anhangs wurde aufgegeben.';

  @override
  String get mainDraftImportRecoveryConfirmedRefreshFailed =>
      'Aktion bestätigt, aber die Liste wurde nicht aktualisiert. Aktualisieren Sie sie, um den aktuellen Status zu sehen.';

  @override
  String get mainDraftImportRecoveryConflict =>
      'Konflikt: Der Entwurf hat sich geändert. Diese Anfrage kann nur abgebrochen werden.';

  @override
  String get mainDraftImportRecoveryEmpty =>
      'Keine Anhangentscheidungen zur Prüfung.';

  @override
  String get mainDraftImportRecoveryFailed =>
      'Diese Entscheidung konnte nicht geprüft werden. Aktualisieren und erneut versuchen.';

  @override
  String get mainDraftImportRecoveryPending =>
      'Ausstehend: Der ursprüngliche Verzicht wurde nicht bestätigt.';

  @override
  String get mainDraftImportRecoveryRetry =>
      'Ursprünglichen Verzicht wiederholen';

  @override
  String get mainDraftImportRecoveryTitle => 'Anhangentscheidungen prüfen';

  @override
  String get mainEdit => 'Bearbeiten';

  @override
  String get mainEditIdeaTitle => 'Gib deiner Idee mehr Klarheit';

  @override
  String get mainEditorClosedUnknown =>
      'Der Editor ist geschlossen, das Speichern aber nicht bestätigt. Öffne den Arbeitsbereich zur Prüfung, bevor du eine weitere Kopie erstellst.';

  @override
  String get mainEditorContinueDraft => 'Weiter bearbeiten';

  @override
  String get mainEditorContinueFailed =>
      'Die vorherige Änderung ist bestätigt. Ihr neuer Entwurf bleibt in diesem Fenster. Der nächste Editor konnte nicht geöffnet werden. Bitte erneut versuchen.';

  @override
  String get mainEditorNewerDraft =>
      'Die vorherige Änderung wurde gespeichert. Ihr neuer Entwurf ist noch ungespeichert.';

  @override
  String get mainEditorPendingDraft =>
      'Sie können weiterschreiben. Klären Sie zuerst den vorherigen Speichervorgang; neue Änderungen werden nicht automatisch gesendet.';

  @override
  String get mainEditorRecoveryAbandonBody =>
      'Diese noch nicht übernommene Änderung verwerfen? Der ursprüngliche Vorgang wird gesperrt. Gespeicherte Inhalte und neuere Entwürfe bleiben unverändert. Bereits übernommene Änderungen werden nicht rückgängig gemacht.';

  @override
  String get mainEditorRecoveryAbandonTitle => 'Alte Änderung verwerfen';

  @override
  String get mainEditorRecoveryCommitted =>
      'Die ursprüngliche Änderung wurde gespeichert. Prüfen und bestätigen Sie den aktuellen Inhalt, ohne erneut zu speichern.';

  @override
  String get mainEditorRecoveryConfirm => 'Prüfen und bestätigen';

  @override
  String get mainEditorRecoveryConflict =>
      'Die Ausgangsversion oder der Verlauf hat sich geändert. Der Vorschlag bleibt erhalten und kann den aktuellen Inhalt nicht überschreiben.';

  @override
  String get mainEditorRecoveryEmpty =>
      'Keine gespeicherten Änderungsvorschläge zu prüfen.';

  @override
  String get mainEditorRecoveryFailed =>
      'Die Prüfung konnte nicht abgeschlossen werden. Der ursprüngliche Vorschlag bleibt erhalten. Aktualisieren Sie und versuchen Sie es erneut.';

  @override
  String get mainEditorRecoveryPending =>
      'Die ursprüngliche Änderung ist unbestätigt. Fortsetzen wiederholt nur diesen Vorschlag, ohne neuere Änderungen zu senden.';

  @override
  String get mainEditorRecoveryResume => 'Ursprüngliche Änderung fortsetzen';

  @override
  String get mainEditorRecoveryTitle => 'Änderungen prüfen';

  @override
  String get mainEditorSubtitle =>
      'Texte, Tabellen, Bilder – bewahre sie hier, während deine Idee Gestalt annimmt.';

  @override
  String get mainEditorUnavailable =>
      'Editor nicht verfügbar. Prüfe den Inhaltsdienst und versuche es erneut.';

  @override
  String get mainEndpointSettings => 'Ausgehende Endpunkte';

  @override
  String get mainExpandSettings => 'Einstellungen erweitern';

  @override
  String get mainExpandSidebar => 'Seitenleiste ausklappen';

  @override
  String get mainExtensionPlugins => 'Erweiterungen';

  @override
  String get mainFavoriteAttachments => 'Gespeicherte Anhänge';

  @override
  String get mainFavoriteRecords => 'Gespeicherte Einträge';

  @override
  String mainFavoriteTooltip(String title) {
    return '$title als Favorit markieren';
  }

  @override
  String get mainFavoritesIntro =>
      'Lieblingstexte, Bilder und Dateien an einem Ort.';

  @override
  String mainFieldLimit(int limit) {
    return 'Maximal $limit Zeichen. Kürze den Inhalt oder importiere ihn als Datei.';
  }

  @override
  String get mainFilterAll => 'Alle';

  @override
  String get mainFilterAttachments => 'Mit Dateien';

  @override
  String get mainFilterFavorites => 'Nur Favoriten';

  @override
  String get mainFilterFile => 'Dateien';

  @override
  String get mainFilterImage => 'Bilder';

  @override
  String get mainFilterMedia => 'Audio / Video';

  @override
  String get mainFilterPending => 'Zu erledigen';

  @override
  String get mainFilterText => 'Text';

  @override
  String get mainFollowTheme => 'Thema folgen';

  @override
  String get mainFontApply => 'Schrift anwenden';

  @override
  String get mainFontDefault => 'Standardschrift';

  @override
  String get mainFontFailed =>
      'Die Schrift konnte nicht geladen werden. Prüfe die Datei oder starte die App neu und versuche es erneut.';

  @override
  String get mainFontFamily => 'Name der Systemschrift';

  @override
  String get mainFontHelp =>
      'TTF / OTF, bis zu 20 MiB. Nicht verfügbare Systemschriften werden automatisch ersetzt.';

  @override
  String get mainFontHint => 'Zum Beispiel: Arial oder Microsoft YaHei';

  @override
  String get mainFontImport => 'Schriftdatei importieren';

  @override
  String get mainFontReset => 'Standard wiederherstellen';

  @override
  String get mainFontSettings => 'Schriftarten';

  @override
  String get mainFontUnavailable =>
      'Die gespeicherte Schrift ist nicht verfügbar. Vorübergehend wird die Standardschrift verwendet.';

  @override
  String get mainFrostDetail =>
      'Ein weicher Hintergrund schafft Raum für Gedanken.';

  @override
  String get mainFrostEffect => 'Unschärfe';

  @override
  String get mainFrostOpacity => 'Glasdeckkraft';

  @override
  String get mainFrostUnavailable =>
      'Desktop-Unschärfe ist nicht verfügbar. Farbe und Deckkraft bleiben einstellbar.';

  @override
  String get mainFrosted => 'Matt';

  @override
  String get mainGlassTexture => 'Glasstil';

  @override
  String mainGlobalColor(String color) {
    return '$color · Alle Bedienelemente';
  }

  @override
  String get mainGreeting => 'Lass deine Ideen wachsen.';

  @override
  String get mainGreetingDetail =>
      'Bewahre Alltagsdetails und spontane Einfälle.';

  @override
  String get mainHeroBody =>
      'Ein Gedanke, eine kleine Aufgabe, ein Was-wäre-wenn.\nHier fängt alles an.';

  @override
  String get mainHeroCaption => 'RAUM DER MÖGLICHKEITEN';

  @override
  String get mainHeroTitle => 'Klein anzufangen ist völlig in Ordnung.';

  @override
  String get mainHideAppearance => 'Darstellungseinstellungen ausblenden';

  @override
  String get mainHideCustomTone => 'Eigene Farbe ausblenden';

  @override
  String get mainHidePreview => 'Vorschau ausblenden';

  @override
  String get mainHttpSettings => 'HTTP-Aufgaben';

  @override
  String get mainHypothesis => 'Hypothese';

  @override
  String get mainHypothesisPrompt => 'Zu prüfende Hypothese';

  @override
  String get mainHypothesisSection => 'Hypothese / Was ausprobieren';

  @override
  String get mainIdeaDetails =>
      'Halte Details fest. Mache den nächsten Schritt klarer.';

  @override
  String get mainIdeaNameHint => 'Gib ihr einen Namen';

  @override
  String get mainIdeaNameRequired => 'Schreibe zuerst deine Idee auf';

  @override
  String get mainIdeaSaved => 'Idee gespeichert.';

  @override
  String get mainImportFailed =>
      'Medienimport fehlgeschlagen. Prüfe Datei und verfügbaren Speicher.';

  @override
  String get mainImportFile => 'Datei importieren';

  @override
  String get mainInboxIntro =>
      'Erst festhalten, später ordnen. Mache aus guten Ideen kleine Projekte.';

  @override
  String get mainIoNoDeclarations =>
      'Kein installiertes Plugin meldet Datei- oder Netzwerkzugriff an.';

  @override
  String get mainIoSettings => 'Netzwerk und Dateien';

  @override
  String get mainIoSettingsGuide =>
      'Verwalte Datei- und Netzwerkrechte getrennt von der Darstellung. Eine bestätigte Fähigkeit erlaubt nicht den Zugriff auf alle Dateien oder Endpunkte; verfügbare Vorgänge hängen vom aktuellen Backend ab.';

  @override
  String get mainIoSettingsSummary =>
      'Berechtigungen, Zugangsdaten, Endpunkte und API-Dienste';

  @override
  String get mainJustNow => 'Gerade eben';

  @override
  String get mainLabIntro =>
      'Beginne mit einer Hypothese. Halte Versuche, Beobachtungen und Überraschungen fest.';

  @override
  String get mainLanguage => 'Sprache';

  @override
  String get mainLanguageChinese => '简体中文';

  @override
  String get mainLanguageEnglish => 'English';

  @override
  String get mainLanguageSystem => 'Systemsprache';

  @override
  String get mainLavender => 'Lavendel';

  @override
  String get mainLegacyStageComplete =>
      'Das alte Format markiert alle Aufgaben als erledigt und schließt das Projekt ab. Fortfahren?';

  @override
  String mainLegacyStageReopen(String task) {
    return 'Ein anderer Projektstatus entfernt die Markierung der letzten Aufgabe „$task“ und aller gleichnamigen Aufgaben. Fortfahren?';
  }

  @override
  String get mainLegacyTodoContinue => 'Fortfahren';

  @override
  String mainLegacyTodoGroup(String task) {
    return 'Das alte Format identifiziert Aufgaben anhand ihres Textes. Alle Aufgaben „$task“ werden geändert. Fortfahren?';
  }

  @override
  String get mainLegacyTodoTitle => 'Änderung einer alten Aufgabenliste';

  @override
  String get mainLightOpacity => '20 % · Leicht';

  @override
  String get mainLiquidAllCanvases =>
      'Für alle vier Hintergrundtypen separat verfügbar';

  @override
  String get mainLiquidDetail =>
      'Fließende Lichtreflexe und sanfte Brechung wie ein schwebender Wassertropfen.';

  @override
  String get mainLiquidEffect => 'Flüssigglas-Effekt';

  @override
  String get mainLiquidGlass => 'Flüssigglas';

  @override
  String get mainLivePreview => 'Live-Vorschau';

  @override
  String get mainLocalMedia => 'Lokale Medien';

  @override
  String get mainMakeYours => 'DEIN STIL';

  @override
  String get mainMarkOrganized => 'Als geordnet markieren';

  @override
  String get mainMarkdownBody => 'Text · Markdown';

  @override
  String get mainMediaLimits => 'Bilder / GIFs ≤ 25 MB; Videos ≤ 150 MB';

  @override
  String get mainMonochrome => 'Einfarbig';

  @override
  String mainMoreSteps(int count) {
    return '$count weitere Schritte; zum Ansehen öffnen';
  }

  @override
  String mainMovedProject(String title) {
    return '„$title“ zu Projekten verschoben';
  }

  @override
  String get mainMusic => 'Musikplayer';

  @override
  String get mainMySpace => 'Mein Bereich';

  @override
  String get mainNavigation => 'Navigation';

  @override
  String get mainNewIdea => 'Neue Idee';

  @override
  String get mainNewIdeaTitle => 'Halte eine neue Idee fest';

  @override
  String get mainNoHypothesis => 'Noch keine Hypothese';

  @override
  String get mainNoMatches => 'Keine passenden Ideen';

  @override
  String get mainNoResultYet =>
      'Das Ergebnis kann warten. Auch der Weg ist einen Eintrag wert.';

  @override
  String get mainNotNow => 'Nicht jetzt';

  @override
  String get mainObservationSection => 'Beobachtungen / Erkenntnisse';

  @override
  String get mainObservations => 'Beobachtungen und Ergebnisse';

  @override
  String get mainObservationsPrompt => 'Beobachtungen, Vorgehen und Ergebnisse';

  @override
  String get mainOneHourAgo => 'Vor 1 Stunde';

  @override
  String get mainOnlineMedia => 'Online-Medien';

  @override
  String get mainOpaqueFallback =>
      'Klare Flächen über der aktuellen Themenfarbe.';

  @override
  String get mainOpenNextStep =>
      'Projekt öffnen, um nächste Schritte zu bearbeiten';

  @override
  String get mainOrganizedCount => 'Geordnet';

  @override
  String get mainOriginalColors => 'Originalfarben';

  @override
  String get mainPageFavorites => 'Favoriten';

  @override
  String get mainPageInbox => 'Eingang';

  @override
  String get mainPageLaboratory => 'Labor';

  @override
  String get mainPageOverview => 'Übersicht';

  @override
  String get mainPageProjects => 'Projekte';

  @override
  String mainPageSummary(String page) {
    return '$page · Übersicht';
  }

  @override
  String get mainPasteChanged =>
      'Die Eingabe hat sich beim Einfügen geändert. Öffne den Editor erneut.';

  @override
  String get mainPasteContent => 'Inhalt einfügen';

  @override
  String get mainPause => 'Pause';

  @override
  String get mainPersonalWorkspace => 'Persönlicher Arbeitsbereich';

  @override
  String get mainPlay => 'Abspielen';

  @override
  String get mainPluginSettings => 'Plugins und Dienste';

  @override
  String get mainPluginSettingsSummary =>
      'Integrierte Werkzeuge, Erweiterungen, Netzwerk, Dateien und Inhaltsschutz';

  @override
  String get mainPreviewEmpty => 'Hier erscheint die Vorschau';

  @override
  String mainProgress(int done, int total) {
    return 'Kleine Schritte · $done/$total';
  }

  @override
  String get mainProjectIntro =>
      'Komme mit Checklisten voran. Jeder kleine Schritt bringt dich näher ans Ziel.';

  @override
  String get mainQueryAgain => 'Neue Abfrage';

  @override
  String get mainQueryCapacity => 'Abfrageverlauf voll';

  @override
  String get mainQueryCapacityDetail =>
      'Deine Inhalte bleiben erhalten. Diese Version kann den Abfrageverlauf noch nicht löschen.';

  @override
  String get mainQueryLoading => 'Ideen werden gesucht…';

  @override
  String get mainQueryRetry => 'Abfrage wiederholen';

  @override
  String get mainQueryTerminated => 'Diese Abfrage ist beendet';

  @override
  String get mainQueryUnknown => 'Ergebnisse noch nicht bestätigt';

  @override
  String get mainQuickHint => 'Was geht dir gerade durch den Kopf?';

  @override
  String get mainReadOnlySettings =>
      'Inhalt ist schreibgeschützt. Prüfe das Arbeitsbereich-Plugin, um die Bearbeitung wiederherzustellen.';

  @override
  String get mainRecentThoughts => 'Neue Gedanken';

  @override
  String get mainRecordedCount => 'Mit Beobachtungen';

  @override
  String get mainRestoreDefault => 'Standard wiederherstellen';

  @override
  String get mainRetry => 'Erneut versuchen';

  @override
  String get mainRetrySave => 'Speichern wiederholen';

  @override
  String get mainSage => 'Salbei';

  @override
  String get mainSampleBody0 =>
      'Sammle hier spontane Gedanken.\nKein Zeitdruck – lass sie einfach entstehen.';

  @override
  String get mainSampleBody1 =>
      'Eine kleine Seite für Lieblingsworte,\nMusik und Alltagsdetails.';

  @override
  String get mainSampleBody2 =>
      'Probiere generative Kunst. Lass Code\nunerwartete Formen hervorbringen.';

  @override
  String get mainSampleBody3 =>
      'Ein stiller Begleiter, der an die kleinen\nDinge erinnert, die leicht verloren gehen.';

  @override
  String get mainSampleTitle0 => 'Ein Zuhause für Ideen';

  @override
  String get mainSampleTitle1 => 'Ein ruhiger digitaler Garten';

  @override
  String get mainSampleTitle2 => 'Etwas nur zum Spaß gestalten';

  @override
  String get mainSampleTitle3 => 'Mein kleiner Schreibtischhelfer';

  @override
  String get mainSampleTodo0 => 'Erste Sammlung ordnen';

  @override
  String get mainSampleTodo1 => 'Garteneingang gestalten';

  @override
  String get mainSampleTodo2 => 'Eine neue Idee pflanzen';

  @override
  String get mainSampleTodo3 => 'Kleinen Prototyp skizzieren';

  @override
  String get mainSampleTodo4 => 'Erinnerungsfunktion gestalten';

  @override
  String get mainSaveConnectionUnknown =>
      'Verbindung unterbrochen; Speicherergebnis unbekannt. Öffne die Bibliothek zur Prüfung erneut, bevor du es wiederholst.';

  @override
  String get mainSaveFailed =>
      'Speichern fehlgeschlagen. Änderungen bleiben in dieser Sitzung erhalten.';

  @override
  String get mainSaveIdea => 'Idee speichern';

  @override
  String get mainSaveNotSubmitted =>
      'Nicht übermittelt. Entwurf und Anhänge bleiben erhalten; bearbeite und speichere erneut.';

  @override
  String get mainSaveReadOnly =>
      'Änderungen nicht gespeichert. Aktiviere das Arbeitsbereich-Plugin unter Plugins und Dienste und versuche es erneut.';

  @override
  String get mainSaveReadbackPending =>
      'Die Einstellungen wurden gespeichert, das erneute Lesen ist noch unbestätigt. Der Entwurf bleibt erhalten; ein erneuter Versuch prüft zuerst den ursprünglichen Vorgang.';

  @override
  String get mainSaveRecoveryAbandon => 'Alten Vorschlag verwerfen';

  @override
  String get mainSaveRecoveryAbandonConfirm =>
      'Nur den nicht gespeicherten Vorschlag verwerfen? Ihr Entwurf und gespeicherte Inhalte bleiben erhalten. Gespeicherte Einstellungen werden nicht rückgängig gemacht.';

  @override
  String get mainSaveRecoveryCommitted =>
      'Die alten Einstellungen wurden gespeichert. Die Prüfung bestätigt das Ergebnis, ohne Ihren aktuellen Entwurf zu ersetzen.';

  @override
  String get mainSaveRecoveryConflict =>
      'Die Bibliothek hat sich geändert. Der alte Vorschlag darf neuere Einstellungen nicht überschreiben. Behalten Sie ihn bei oder verwerfen Sie ihn, falls er nicht gespeichert wurde.';

  @override
  String get mainSaveRecoveryDone =>
      'Der alte Vorschlag ist geklärt. Ihr Entwurf bleibt unverändert; speichern Sie ihn bei Bedarf erneut.';

  @override
  String get mainSaveRecoveryEmpty =>
      'Kein dauerhaft gespeicherter Vorschlag muss geprüft werden.';

  @override
  String get mainSaveRecoveryPending =>
      'Der alte Vorschlag ist noch nicht bestätigt. Fortfahren prüft und versucht den ursprünglichen Speichervorgang; Ihr neuer Entwurf bleibt erhalten.';

  @override
  String get mainSaveRecoveryResolve => 'Ursprüngliches Speichern klären';

  @override
  String get mainSaveRecoveryReview => 'Speichern prüfen';

  @override
  String get mainSaveRecoveryTitle => 'Ein Speichervorgang muss geprüft werden';

  @override
  String get mainSaveUnknown =>
      'Speichern noch nicht bestätigt. Entwurf und Anhänge bleiben erhalten. Wiederhole diese Übermittlung; beim Schließen wird der Arbeitsbereich zur Prüfung aktualisiert.';

  @override
  String get mainSaving => 'Wird gespeichert…';

  @override
  String get mainSearchHint => 'Ideen durchsuchen…';

  @override
  String get mainServiceRunSettings => 'Dienstausführung';

  @override
  String get mainServiceSettings => 'API-Dienste';

  @override
  String get mainSettings => 'Einstellungen';

  @override
  String get mainShowAppearance => 'Darstellungseinstellungen anzeigen';

  @override
  String get mainSidebarMotto => 'Ein wenig Ordnung. Raum zum Staunen.';

  @override
  String get mainSlowProgress => 'Auch kleine Schritte bringen dich voran.';

  @override
  String get mainSolidCanvas => 'Einfarbig';

  @override
  String get mainSolidDetail => 'Eine ruhige, einfarbige Fläche.';

  @override
  String get mainSolidOpacity => '100 % · Deckend';

  @override
  String get mainSortFavorites => 'Favoriten zuerst';

  @override
  String get mainSortRecent => 'Zuletzt hinzugefügt';

  @override
  String get mainSortTitle => 'Nach Titel';

  @override
  String get mainSquareCorners => '0 für rechteckige Ecken';

  @override
  String get mainStageActive => 'In Arbeit';

  @override
  String get mainStageCompleted => 'Abgeschlossen';

  @override
  String get mainStageOrganized => 'Geordnet';

  @override
  String get mainStagePlanned => 'Geplant';

  @override
  String get mainStageRecorded => 'Dokumentiert';

  @override
  String mainStageTooltip(String title) {
    return 'Phase für $title ändern';
  }

  @override
  String get mainStageUnsorted => 'Zu ordnen';

  @override
  String get mainStageUnverified => 'Zu testen';

  @override
  String get mainStageVerifying => 'Wird getestet';

  @override
  String get mainStayCurious => 'BLEIB NEUGIERIG. BLEIB DU SELBST.';

  @override
  String mainSteps(int done, int total) {
    return '$done/$total Schritte';
  }

  @override
  String get mainStorageUnavailable =>
      'Lokaler Speicher nicht verfügbar. Änderungen bleiben nur für diese Sitzung erhalten.';

  @override
  String get mainStorageUnreadable =>
      'Gespeicherte Inhalte konnten nicht gelesen werden. Originaldaten bleiben erhalten und werden nicht überschrieben.';

  @override
  String get mainStyleBrutalist => 'Brutalismus';

  @override
  String get mainStyleBrutalistDescription =>
      'Harte Kanten, kräftige Rahmen und versetzte Schatten';

  @override
  String get mainStyleClay => 'Clay';

  @override
  String get mainStyleClayDescription =>
      'Runde Formen mit sanft erhabenen Farben';

  @override
  String get mainStyleDepth => 'Relieftiefe';

  @override
  String get mainStyleDepthGuide =>
      'Stärke von Erhebungen und Vertiefungen anpassen, ohne die Glasdeckkraft zu ändern.';

  @override
  String get mainStyleDepthReset => 'Auf 100 % zurücksetzen';

  @override
  String get mainStyleExperimental => 'Experimentell';

  @override
  String get mainStyleFlat => 'Flach · Standard';

  @override
  String get mainStyleFlatDescription => 'Dezente Konturen, klare Ebenen';

  @override
  String get mainStyleFluent => 'Fluent';

  @override
  String get mainStyleFluentDescription =>
      'Halbtransparente Ebenen und Akzentlinien';

  @override
  String get mainStyleIndustrial => 'Industriell';

  @override
  String get mainStyleIndustrialDescription =>
      'Metallartige Flächen, kompakte Bedienelemente und präzise Linien';

  @override
  String get mainStyleNeumorphism => 'Neumorphismus';

  @override
  String get mainStyleNeumorphismDescription =>
      'Weiche Lichtkanten und sanfte Schatten';

  @override
  String get mainStylePaper => 'Papier';

  @override
  String get mainStylePaperDescription =>
      'Matte Flächen, feine Ränder und dezente Tiefe';

  @override
  String get mainTaskAdd => 'Aufgabe hinzufügen';

  @override
  String get mainTaskAmbiguousDecision =>
      'Der Status dieser alten Aufgaben mit gleichem Namen ist unklar. Bestätigen Sie jede Aufgabe einzeln.';

  @override
  String get mainTaskCompleteAllAndSetStage =>
      'Alle erledigen und Phase setzen';

  @override
  String mainTaskCompleteAllConfirm(String stage) {
    return 'Alle Aufgaben als erledigt markieren und die Phase auf „$stage“ setzen?';
  }

  @override
  String get mainTaskLegacyReadOnly =>
      'Alte Aufgaben werden nach Text zugeordnet. Nach dem Upgrade lassen sich gleichnamige Aufgaben einzeln verwalten.';

  @override
  String get mainTaskMarkComplete => 'Als erledigt bestätigen';

  @override
  String get mainTaskMarkIncomplete => 'Als offen bestätigen';

  @override
  String get mainTaskMoveDown => 'Nach unten';

  @override
  String get mainTaskMoveUp => 'Nach oben';

  @override
  String mainTaskProgressThreeWay(int ambiguous, int complete, int incomplete) {
    return '$complete erledigt · $incomplete offen · $ambiguous zu bestätigen';
  }

  @override
  String mainTaskRemoveConfirm(String task) {
    return 'Aufgabe „$task“ entfernen?';
  }

  @override
  String get mainTaskRename => 'Umbenennen';

  @override
  String get mainTaskSetStage => 'Nur Phase ändern';

  @override
  String get mainTaskStagePrompt => 'Projektphase';

  @override
  String get mainTaskTextPrompt => 'Aufgabentext';

  @override
  String get mainTenMinutesAgo => 'Vor 10 Minuten';

  @override
  String get mainTextureCanvas => 'Textur';

  @override
  String get mainTextureDetail =>
      'Eine feine Papierstruktur verleiht ein griffiges Gefühl.';

  @override
  String get mainThemeCompass => 'Themenfarbkreis';

  @override
  String get mainThemeGrayscale => 'Graustufen des Themas';

  @override
  String get mainThemeTone => 'Themenfarben';

  @override
  String get mainThreeHoursAgo => 'Vor 3 Stunden';

  @override
  String get mainTintOpacity => 'Farbdeckkraft';

  @override
  String get mainToProject => 'Zu Projekten verschieben';

  @override
  String get mainTodosPrompt => 'Nächste Schritte (einer pro Zeile, optional)';

  @override
  String get mainTransparencyUnavailable =>
      'Systemtransparenz konnte nicht aktiviert werden. Verwende den Standardhintergrund.';

  @override
  String get mainTransparentCanvas => 'Transparent';

  @override
  String get mainTransparentDetail =>
      'Zeigt den Bereich hinter dem Fenster; im Web den Seitenhintergrund.';

  @override
  String get mainUndo => 'Rückgängig';

  @override
  String mainUnfavoriteTooltip(String title) {
    return '$title aus Favoriten entfernen';
  }

  @override
  String get mainUnsortedCount => 'Zu ordnen';

  @override
  String get mainUnverifiedCount => 'Zu testen';

  @override
  String get mainView => 'Ansehen';

  @override
  String get mainViewAll => 'Alle anzeigen';

  @override
  String get mainVisualStyle => 'Oberflächenstil';

  @override
  String get mainWarmSand => 'Warmer Sand';

  @override
  String get mainWhiteTheme => 'Hell';

  @override
  String get mainWindowRadius => 'Fensterecken';

  @override
  String get mainWindowRadiusDetail =>
      'Fensterrand getrennt einstellen; beim Maximieren werden die Ecken rechteckig';

  @override
  String get mainWindowsFrostOnly =>
      'Desktop-Unschärfe ist nur unter Windows verfügbar';

  @override
  String get mainWorkbench => 'Arbeitsbereich';

  @override
  String get mainWorkbenchPlugin => 'Arbeitsbereich-Plugin';

  @override
  String get mainWriteHypothesis =>
      'Öffne den Eintrag und notiere, was du testen möchtest.';

  @override
  String get mainYesterday => 'Gestern';

  @override
  String get pluginsApprovalUnknown =>
      'Aktivierung oder Berechtigungsänderungen konnten nicht bestätigt werden';

  @override
  String get pluginsApproveEnable => 'Genehmigen und aktivieren';

  @override
  String get pluginsApproveWorkbench =>
      'Lesen und Bearbeiten erlauben und aktivieren';

  @override
  String get pluginsAttachment => 'Anhänge lesen';

  @override
  String get pluginsBackingUp => 'Sicherung läuft…';

  @override
  String get pluginsBackupLibrary => 'Bibliothek sichern';

  @override
  String get pluginsBackupLibraryType => 'Bibliothekssicherung';

  @override
  String get pluginsBackupProtection => 'Schutzdatei sichern';

  @override
  String get pluginsBackupUnknown =>
      'Die Sicherung ist noch nicht bestätigt. Behalten Sie erstellte Dateien und prüfen Sie das Ziel.';

  @override
  String pluginsBinaryPreview(String hex) {
    return 'Binärinhalt: $hex';
  }

  @override
  String get pluginsBuiltin => 'Integrierter Arbeitsbereich';

  @override
  String pluginsBuiltinCount(int count) {
    return '$count Zeichen · Nur diese Sitzung; nicht als Karte gespeichert';
  }

  @override
  String get pluginsBuiltinEmpty =>
      'Text für die Großbuchstabenvorschau eingeben';

  @override
  String get pluginsBuiltinHeading => 'Textwerkzeuge';

  @override
  String get pluginsBuiltinInput => 'Text eingeben';

  @override
  String get pluginsCancel => 'Abbrechen';

  @override
  String get pluginsChoosePackage => 'Plugin-Datei auswählen';

  @override
  String get pluginsChooseSmallFile => 'Kleine Datei auswählen';

  @override
  String get pluginsCloseTextTool => 'Textwerkzeug ausblenden';

  @override
  String get pluginsCloseUnknown =>
      'Das Schließen der Ansicht konnte nicht bestätigt werden';

  @override
  String get pluginsCloseView => 'Ansicht schließen';

  @override
  String get pluginsConnectionLost =>
      'Verbindung unterbrochen. Öffnen Sie die Plugin-Ansicht erneut.';

  @override
  String get pluginsContentPermissions => 'Inhaltsberechtigungen';

  @override
  String get pluginsCreate => 'Inhalte erstellen';

  @override
  String get pluginsCredentialCancel => 'Formular schließen';

  @override
  String get pluginsCredentialCreateTitle => 'Neue Zugangsdaten';

  @override
  String pluginsCredentialDays(int days) {
    String _temp0 = intl.Intl.pluralLogic(
      days,
      locale: localeName,
      other: '$days Tage',
      one: '$days Tag',
    );
    return '$_temp0';
  }

  @override
  String get pluginsCredentialDetails =>
      'Speichern Sie Zugangsdaten für genehmigte API-Verbindungen sicher. Das Speichern genehmigt keinen Server und aktiviert kein Plugin. Gespeicherte Geheimnisse können nicht angezeigt werden.';

  @override
  String get pluginsCredentialDisable => 'Deaktivieren';

  @override
  String get pluginsCredentialDisabled => 'Deaktiviert';

  @override
  String get pluginsCredentialDisabledDone => 'Zugangsdaten deaktiviert.';

  @override
  String get pluginsCredentialEmpty => 'Keine gespeicherten Zugangsdaten';

  @override
  String get pluginsCredentialExpired => 'Abgelaufen';

  @override
  String pluginsCredentialExpires(String date) {
    return 'Läuft ab: $date';
  }

  @override
  String get pluginsCredentialHeader => 'Headername';

  @override
  String get pluginsCredentialInvalid =>
      'Prüfen Sie den Headernamen und geben Sie ein neues Geheimnis ein. Das Geheimnisfeld wurde geleert.';

  @override
  String get pluginsCredentialLifetime => 'Gültigkeitsdauer';

  @override
  String get pluginsCredentialLoadFailed =>
      'Zugangsdaten konnten nicht konsistent gelesen werden. Aktualisieren Sie den Status, um es erneut zu versuchen.';

  @override
  String get pluginsCredentialNew => 'Zugangsdaten hinzufügen';

  @override
  String get pluginsCredentialReading => 'Zugangsdaten werden gelesen…';

  @override
  String pluginsCredentialReference(String reference) {
    return 'Zugangsdaten $reference';
  }

  @override
  String get pluginsCredentialRefresh => 'Status aktualisieren';

  @override
  String get pluginsCredentialReplace => 'Geheimnis ersetzen';

  @override
  String pluginsCredentialReplaceTitle(String reference) {
    return 'Zugangsdaten $reference ersetzen';
  }

  @override
  String get pluginsCredentialSave => 'Zugangsdaten speichern';

  @override
  String get pluginsCredentialSaved =>
      'Zugangsdaten gespeichert. API-Verbindungen benötigen weiterhin eine separate Genehmigung.';

  @override
  String get pluginsCredentialSecret => 'Neuer Geheimniswert';

  @override
  String get pluginsCredentialStored => 'Gespeichert';

  @override
  String get pluginsCredentialTitle => 'API-Zugangsdaten';

  @override
  String get pluginsCredentialUnknown =>
      'Das Ergebnis konnte nicht bestätigt werden. Das Geheimnisfeld wurde geleert. Aktualisieren Sie den Status vor weiteren Änderungen.';

  @override
  String pluginsDeclared(String permissions) {
    return 'Angegebene Berechtigungen: $permissions';
  }

  @override
  String get pluginsDependenciesNotice =>
      'Abhängigkeiten müssen im Host konfiguriert werden. Diese Seite genehmigt sie nicht.';

  @override
  String get pluginsDisable => 'Deaktivieren';

  @override
  String get pluginsDisableWorkbench => 'Arbeitsbereich-Plugin deaktivieren';

  @override
  String get pluginsDisabled => 'Deaktiviert';

  @override
  String get pluginsDisabledDetails =>
      'Deaktiviert. Erlauben Sie das Lesen und Bearbeiten von Inhalten, um Editor und Werkzeuge zu nutzen.';

  @override
  String get pluginsEdit => 'Inhalte bearbeiten';

  @override
  String get pluginsEmptyLibrary =>
      'Noch keine Drittanbieter-Plugins importiert.';

  @override
  String get pluginsEmptyResult => '(leeres Ergebnis)';

  @override
  String get pluginsEnabled => 'Aktiviert';

  @override
  String get pluginsEnabledDetails =>
      'Aktiviert. Dieses Plugin kann Inhalte lesen und bearbeiten. Beim Deaktivieren bleiben Ihre Daten erhalten.';

  @override
  String get pluginsEndpointAdvanced =>
      'Richtlinienlimits (Bytes, sofern nicht anders angegeben)';

  @override
  String get pluginsEndpointCertificate => 'DER-Vertrauenswurzel auswählen';

  @override
  String get pluginsEndpointCertificateDetails =>
      'Optionale HTTPS-Vertrauenswurzel: ein binäres DER-Zertifikat (.der oder .cer), maximal 32 KiB. PEM und Zertifikatssammlungen werden nicht akzeptiert. Entfernen Sie die Vertrauenswurzel vor dem Wechsel zu HTTP.';

  @override
  String get pluginsEndpointCertificateInvalid =>
      'Wählen Sie ein gültiges binäres DER-Zertifikat (.der oder .cer) mit höchstens 32 KiB.';

  @override
  String pluginsEndpointCertificateSelected(int bytes) {
    return 'DER-Vertrauenswurzel ausgewählt ($bytes Bytes)';
  }

  @override
  String get pluginsEndpointConcurrency => 'Gleichzeitige Anfragen (1–128)';

  @override
  String get pluginsEndpointCreateTitle => 'Neue Endpunktgenehmigung';

  @override
  String get pluginsEndpointCredential => 'Zugangsdatenreferenz';

  @override
  String get pluginsEndpointCredentialLifetime =>
      'Die Zugangsdaten müssen während der gesamten Endpunktgültigkeit gültig bleiben. Ihr Ablaufdatum wird nicht verlängert.';

  @override
  String get pluginsEndpointCredentialUnavailable =>
      'Zugangsdaten erfordern die angegebene und genehmigte Berechtigung zur Zugangsdatenverwendung sowie eine gültige gespeicherte Referenz.';

  @override
  String get pluginsEndpointCredentialsFailed =>
      'Zugangsdatenreferenzen konnten nicht gelesen werden. Aktualisieren Sie den Status vor der Auswahl.';

  @override
  String get pluginsEndpointDetails =>
      'Speichern Sie eine Serverrichtlinie für ein bestimmtes Paket und dessen Hash. Das Speichern stellt keine Netzwerkverbindung her, aktiviert kein Plugin und macht Netzwerkaufgaben nicht sofort verfügbar.';

  @override
  String get pluginsEndpointDigest => 'Paket-Hash';

  @override
  String get pluginsEndpointDisabledDone => 'Endpunktgenehmigung deaktiviert.';

  @override
  String get pluginsEndpointEmpty =>
      'Keine gespeicherten Endpunktgenehmigungen';

  @override
  String get pluginsEndpointFrameBytes => 'Frame-Budget (1–131072 Bytes)';

  @override
  String get pluginsEndpointHeaderBytes =>
      'Maximale Headergröße (1–16384 Bytes)';

  @override
  String get pluginsEndpointInvalid =>
      'Prüfen Sie Paket, Origin, Methoden, Gültigkeit von 1–30 Tagen, Zugangsdatenberechtigung, Zertifikat und Richtlinienlimits.';

  @override
  String get pluginsEndpointLifetime => 'Gültigkeit (1–30 Tage)';

  @override
  String get pluginsEndpointLoadFailed =>
      'Endpunktgenehmigungen konnten nicht konsistent gelesen werden. Aktualisieren Sie den Status für einen neuen Versuch.';

  @override
  String get pluginsEndpointLocalHttp => 'Lokales HTTP';

  @override
  String get pluginsEndpointLocalHttps => 'Lokales HTTPS';

  @override
  String get pluginsEndpointMethods => 'Erlaubte Anfragemethoden';

  @override
  String get pluginsEndpointNew => 'Endpunkt hinzufügen';

  @override
  String get pluginsEndpointNoCredential => 'Keine Zugangsdaten';

  @override
  String get pluginsEndpointOrigin =>
      'Nur Origin, z. B. https://api.example.com';

  @override
  String get pluginsEndpointPackage => 'Paket';

  @override
  String get pluginsEndpointPackageUnavailable =>
      'Dieses Paket ist nicht verfügbar oder besitzt keine HTTP-Genehmigung. Vorhandene Genehmigungen können weiterhin deaktiviert werden.';

  @override
  String get pluginsEndpointProfile => 'Verbindungsprofil';

  @override
  String get pluginsEndpointPublicHttps => 'Öffentliches HTTPS';

  @override
  String get pluginsEndpointRemoveCertificate => 'Vertrauenswurzel entfernen';

  @override
  String get pluginsEndpointReplace => 'Genehmigung ersetzen';

  @override
  String get pluginsEndpointReplaceTitle =>
      'Genehmigung mit aktuellem Paket-Hash ersetzen';

  @override
  String get pluginsEndpointRequestBytes =>
      'Maximale Anfragegröße (1–65536 Bytes)';

  @override
  String get pluginsEndpointResponseBytes =>
      'Maximale Antwortgröße (1–65536 Bytes)';

  @override
  String get pluginsEndpointSave => 'Endpunktgenehmigung speichern';

  @override
  String get pluginsEndpointSaved =>
      'Endpunktgenehmigung gespeichert. Es wurde keine Netzwerkverbindung hergestellt.';

  @override
  String get pluginsEndpointTimeout => 'Zeitlimit (1–30000 Millisekunden)';

  @override
  String get pluginsEndpointTitle => 'API-Endpunktgenehmigungen';

  @override
  String get pluginsEndpointUnknown =>
      'Das Ergebnis ist unbestätigt. Aktualisieren Sie den Status vor weiteren Änderungen. Die Anfrage wird nicht automatisch erneut gesendet.';

  @override
  String get pluginsEndpointWorking => 'Endpunktstatus wird aktualisiert…';

  @override
  String get pluginsExistingVersion =>
      'Diese Version ist bereits installiert. Ihr Aktivierungsstatus bleibt unverändert.';

  @override
  String pluginsFileLimit(int limit) {
    return 'Die Datei ist zu groß. Wählen Sie eine Datei mit höchstens $limit Bytes.';
  }

  @override
  String get pluginsHttpTaskAbandon => 'Beobachtung dieses Versuchs beenden';

  @override
  String get pluginsHttpTaskAbandonDetails =>
      'Die Beobachtung darf erst enden, wenn ein aktueller Status keine aktive Aufgabe und die Verfügbarkeit der ursprünglichen Bibliothek bestätigt. Das beweist nicht, dass entfernte Auswirkungen ausgeblieben sind. Kennung und Ungewissheit bleiben im Verlauf; eine neue Anfrage erfordert eine ausdrückliche Übermittlung.';

  @override
  String get pluginsHttpTaskAbsent => 'Keine Ergebniszustellung';

  @override
  String get pluginsHttpTaskAccepted => 'Angenommen';

  @override
  String get pluginsHttpTaskAcknowledge => 'Abgeschlossene Aufgabe bestätigen';

  @override
  String get pluginsHttpTaskArchivedUnknown =>
      'Beobachtung durch den Benutzer beendet. Frühere entfernte Auswirkungen bleiben unbestätigt; dieser Versuch wurde nicht wiederholt.';

  @override
  String get pluginsHttpTaskBase64 => 'Base64';

  @override
  String get pluginsHttpTaskBody => 'Anfrageinhalt';

  @override
  String get pluginsHttpTaskBodyFormat => 'Kodierung des Anfrageinhalts';

  @override
  String get pluginsHttpTaskBusy => 'Belegt';

  @override
  String get pluginsHttpTaskCancel => 'Abbruch anfordern';

  @override
  String get pluginsHttpTaskCancelled =>
      'Abbruch erkannt; entfernte Auswirkungen können dennoch eingetreten sein';

  @override
  String get pluginsHttpTaskCatalogUnavailable =>
      'Plugin-Katalogstatus nicht verfügbar. Aktualisieren Sie die Plugin-Bibliothek vor einer neuen Übermittlung; bestehende Aufgaben bleiben steuerbar.';

  @override
  String get pluginsHttpTaskClosed => 'Geschlossen';

  @override
  String get pluginsHttpTaskCompleted => 'Abgeschlossen';

  @override
  String get pluginsHttpTaskConflict => 'Konflikt';

  @override
  String get pluginsHttpTaskConsumed => 'Ergebnis verbraucht';

  @override
  String get pluginsHttpTaskControlUnknown =>
      'Das Steuerungsergebnis ist unbestätigt. Aktualisieren Sie den Aufgabenstatus vor der nächsten Entscheidung.';

  @override
  String pluginsHttpTaskCounters(String bytes, String calls) {
    return 'E/A-Aufrufe: $calls; angerechnete Bytes: $bytes';
  }

  @override
  String get pluginsHttpTaskDeadline => 'Frist überschritten';

  @override
  String get pluginsHttpTaskDenied => 'Abgelehnt';

  @override
  String get pluginsHttpTaskDetails =>
      'Führen Sie eine explizite Anfrage mit einem genehmigten Endpunkt und einem aktivierten Plugin mit experimentellem HTTP-Weiterleitungshandler aus. Der Aufgabenstatus bleibt bei belegter Inhaltsbibliothek verfügbar.';

  @override
  String get pluginsHttpTaskDisconnect =>
      'Verbindungsbereinigung fehlgeschlagen';

  @override
  String get pluginsHttpTaskEndpoint => 'Genehmigter Endpunkt';

  @override
  String get pluginsHttpTaskEndpointsFailed =>
      'Endpunkte konnten nicht konsistent gelesen werden oder die Bibliothek ist belegt. Aufgabensteuerung bleibt verfügbar. Aktualisieren Sie Endpunkte, sobald die Bibliothek zurückgegeben wurde.';

  @override
  String get pluginsHttpTaskEvidenceUnavailable =>
      'Ergebnisnachweise nicht verfügbar';

  @override
  String pluginsHttpTaskExecution(int code, String fault) {
    return 'Gastausführung: $fault; Exitcode: $code';
  }

  @override
  String pluginsHttpTaskExit(
    String disconnect,
    String execution,
    String maintenance,
  ) {
    return 'Worker-Beendigung — Ausführung: $execution; Trennung: $disconnect; Wartung: $maintenance';
  }

  @override
  String get pluginsHttpTaskExplicit =>
      'Übermitteln sendet eine echte Anfrage. Jeder Klick erstellt eine neue Kennung. Ungewisse Übermittlungen und Ergebnislesevorgänge werden nie automatisch wiederholt. Ein Abbruch beweist keine Rücknahme des entfernten Vorgangs.';

  @override
  String get pluginsHttpTaskFailed => 'Fehlgeschlagen';

  @override
  String get pluginsHttpTaskHeaders =>
      'Normale Anfrageheader, je ein Name: Wert pro Zeile';

  @override
  String get pluginsHttpTaskHeadersHint =>
      'Wiederholte Header bleiben getrennt. Zugangsdaten- und Verbindungsheader werden nur von der Laufzeit bereitgestellt.';

  @override
  String get pluginsHttpTaskHistory =>
      'Frühere Aufgabenbeobachtungen (bis zu 5)';

  @override
  String pluginsHttpTaskHttpResult(int code, String status) {
    return 'HTTP-Ergebnis: $status; entfernter Status: $code';
  }

  @override
  String get pluginsHttpTaskInactive => 'Verbindung inaktiv';

  @override
  String get pluginsHttpTaskInvalid =>
      'Prüfen Sie Endpunkt, Methode, relatives Ziel, normale Header, Body-Kodierung und Zeitlimit anhand der Genehmigungsgrenzen.';

  @override
  String get pluginsHttpTaskInvalidOptions => 'Ungültige Optionen';

  @override
  String pluginsHttpTaskKey(String identity) {
    return 'Aufgabenkennung: $identity';
  }

  @override
  String get pluginsHttpTaskLimit => 'Kontingent oder Limit erreicht';

  @override
  String get pluginsHttpTaskLoadingEndpoints =>
      'Genehmigte Endpunkte werden gelesen…';

  @override
  String get pluginsHttpTaskLocal =>
      'Bibliothek verfügbar; keine aktive Aufgabe';

  @override
  String get pluginsHttpTaskModule => 'Ungültiges Gastmodul';

  @override
  String get pluginsHttpTaskNew => 'Neue Anfrage vorbereiten';

  @override
  String get pluginsHttpTaskNoEndpoints =>
      'Kein Endpunkt passt zu einem aktivierten, genehmigten HTTP-Weiterleitungs-Plugin.';

  @override
  String get pluginsHttpTaskNotFound => 'Nicht gefunden';

  @override
  String get pluginsHttpTaskOk => 'OK';

  @override
  String get pluginsHttpTaskOutcomeUnknown =>
      'Entferntes Ergebnis unbekannt; weder Rücknahme annehmen noch erneut senden';

  @override
  String get pluginsHttpTaskPackageChanged => 'Paketbindung geändert';

  @override
  String get pluginsHttpTaskPending => 'Ergebnis ausstehend';

  @override
  String get pluginsHttpTaskPoll => 'Aufgabe prüfen';

  @override
  String get pluginsHttpTaskProtocol => 'Aufgabenprotokollfehler';

  @override
  String get pluginsHttpTaskRead => 'Ergebnis einmal lesen';

  @override
  String get pluginsHttpTaskReadBound => 'Leselimit überschritten';

  @override
  String get pluginsHttpTaskReadPending =>
      'Kein Ergebnis zurückgegeben. Prüfen Sie den Status vor einem ausdrücklichen erneuten Lesen.';

  @override
  String get pluginsHttpTaskReadUnknown =>
      'Das Lesen konnte nicht bestätigt werden und hat das Ergebnis möglicherweise bereits verbraucht. Es wird nicht erneut gelesen. Status und Beendigung können weiterhin geprüft werden.';

  @override
  String get pluginsHttpTaskReady =>
      'Ergebnis bereit: ausdrücklich lesen. Dies bedeutet nicht, dass der Worker beendet ist.';

  @override
  String get pluginsHttpTaskReclaimed =>
      'Worker beendet; ursprüngliche Bibliothek zurückgegeben';

  @override
  String get pluginsHttpTaskRecoveryRequired =>
      'Worker beendet; Bereinigung oder Wartung muss repariert werden';

  @override
  String get pluginsHttpTaskRefresh => 'Aufgabenstatus aktualisieren';

  @override
  String get pluginsHttpTaskRefreshEndpoints =>
      'Genehmigte Endpunkte aktualisieren';

  @override
  String get pluginsHttpTaskRemoteError =>
      'Der entfernte Server hat 4xx/5xx zurückgegeben. Der HTTP-Austausch ist abgeschlossen; dies ist von Gastausführungsfehlern getrennt.';

  @override
  String get pluginsHttpTaskRepair => 'Bereinigung reparieren';

  @override
  String get pluginsHttpTaskResponseBase64 => 'Antwortinhalt: exaktes Base64';

  @override
  String get pluginsHttpTaskResponseHeaders =>
      'Antwortheader (Duplikate erhalten; Binärwerte in Base64)';

  @override
  String get pluginsHttpTaskResponseText => 'Antwortinhalt: Klartextvorschau';

  @override
  String get pluginsHttpTaskResultUnavailable =>
      'Ergebniszustellung nicht verfügbar';

  @override
  String get pluginsHttpTaskRevoked => 'Genehmigung widerrufen';

  @override
  String get pluginsHttpTaskRunning =>
      'Wird ausgeführt; Worker besitzt die Bibliothek';

  @override
  String get pluginsHttpTaskSpawn => 'Worker konnte nicht gestartet werden';

  @override
  String get pluginsHttpTaskStart => 'Neue Anfrage übermitteln';

  @override
  String get pluginsHttpTaskStartUnknown =>
      'Das Übermittlungsergebnis ist unbekannt. Die Kennung bleibt erhalten. Fragen Sie den Status derselben Aufgabe ab; die Anfrage wird nicht erneut gesendet.';

  @override
  String get pluginsHttpTaskStatusFailed =>
      'Der Aufgabenstatus konnte nicht bestätigt werden. Aktualisieren Sie ihn; es wurde keine Anfrage wiederholt.';

  @override
  String get pluginsHttpTaskStopping =>
      'Wird gestoppt; tatsächliche Worker-Beendigung wird abgewartet';

  @override
  String pluginsHttpTaskSubmission(String identity) {
    return 'Übermittlungskennung: $identity';
  }

  @override
  String get pluginsHttpTaskTarget =>
      'Relatives Ziel, z. B. /v1/items?limit=10';

  @override
  String get pluginsHttpTaskText => 'UTF-8-Text';

  @override
  String get pluginsHttpTaskTimeout =>
      'Zeitlimit in Millisekunden (1–30000, innerhalb der Genehmigung)';

  @override
  String get pluginsHttpTaskTitle => 'HTTP-Aufgaben';

  @override
  String get pluginsHttpTaskTrap => 'Trap bei Gastausführung';

  @override
  String get pluginsHttpTaskUnavailable =>
      'Ursprüngliche Bibliothek nicht verfügbar; Wiederherstellung erfordert Aufmerksamkeit';

  @override
  String get pluginsHttpTaskUnsupported => 'Nicht unterstützter Vorgang';

  @override
  String get pluginsHttpTaskWorking =>
      'Antwort der Aufgabensteuerung wird erwartet…';

  @override
  String get pluginsImport => 'Importieren';

  @override
  String get pluginsImportDetails =>
      'Nach dem Import entscheiden Sie über die Aktivierung. Beim Deaktivieren oder Deinstallieren bleiben Inhalte erhalten.';

  @override
  String pluginsImportPreview(String name) {
    return 'Importvorschau: $name';
  }

  @override
  String get pluginsImportUnknown => 'Der Import konnte nicht bestätigt werden';

  @override
  String get pluginsImportedDisabled =>
      'Importiert und deaktiviert. Wählen Sie die zu erlaubenden Berechtigungen.';

  @override
  String get pluginsInputFailed =>
      'Die Eingabedatei konnte nicht gelesen werden';

  @override
  String get pluginsInputTooLong =>
      'Das Eingabelimit ist erreicht. Kürzen Sie den Text und versuchen Sie es erneut.';

  @override
  String get pluginsInspectFailed =>
      'Die Plugin-Vorschau konnte nicht geladen werden';

  @override
  String get pluginsInspectedOnly =>
      'Die Datei wurde nur geprüft. Aktivieren Sie das Plugin nach dem Import separat.';

  @override
  String get pluginsInsufficientApproval =>
      'Das Plugin ist aktiviert, benötigt aber Inhaltsberechtigungen. Der Arbeitsbereich bleibt schreibgeschützt. Deaktivieren Sie es, um die Berechtigungen erneut zu prüfen.';

  @override
  String pluginsIoApproved(String permissions) {
    return 'Genehmigt: $permissions';
  }

  @override
  String get pluginsIoCredentialUse => 'Genehmigte Zugangsdaten verwenden';

  @override
  String pluginsIoDeclared(String permissions) {
    return 'Angeforderte Netzwerk- und Dateiberechtigungen: $permissions';
  }

  @override
  String get pluginsIoFileCreate => 'Dateien erstellen';

  @override
  String get pluginsIoFileDelete => 'Dateien löschen';

  @override
  String get pluginsIoFileList => 'Genehmigte Ordner durchsuchen';

  @override
  String get pluginsIoFileRead => 'Genehmigte Dateien lesen';

  @override
  String get pluginsIoFileReplace => 'Dateien ersetzen';

  @override
  String get pluginsIoHttpListen => 'Auf Netzwerkverbindungen lauschen';

  @override
  String get pluginsIoHttpPublish => 'API-Dienst bereitstellen';

  @override
  String get pluginsIoHttpRequest => 'Netzwerk-APIs aufrufen';

  @override
  String get pluginsIoNoneApproved =>
      'Keine Netzwerk- oder Dateiberechtigungen genehmigt';

  @override
  String get pluginsIoRevoke =>
      'Alle Netzwerk- und Dateiberechtigungen widerrufen';

  @override
  String get pluginsIoSave => 'Netzwerk- und Dateiberechtigungen speichern';

  @override
  String get pluginsIoScopeNotice =>
      'Hier werden nur Berechtigungskategorien gespeichert. Serveradressen, Dateizugriff und Zugangsdaten benötigen separate Genehmigungen; nicht verfügbare Funktionen bleiben unverfügbar. Öffnen Sie das Plugin-Formular nach Änderungen erneut.';

  @override
  String get pluginsIoTitle => 'Netzwerk- und Dateiberechtigungen';

  @override
  String get pluginsIoWebSocketConnect => 'Mit WebSocket-Diensten verbinden';

  @override
  String get pluginsListUnknown =>
      'Die Plugin-Liste konnte nicht bestätigt werden';

  @override
  String get pluginsManageAbove =>
      'Verwalten Sie dieses Plugin über die obigen Steuerelemente des Arbeitsbereich-Plugins.';

  @override
  String get pluginsManagementUnavailable =>
      'Die Plugin-Verwaltung ist nicht verfügbar. Vorhandene Inhalte bleiben lesbar.';

  @override
  String get pluginsNoPermissions => 'Keine Inhaltsberechtigungen angegeben.';

  @override
  String get pluginsOpenTextTool => 'Textwerkzeug öffnen';

  @override
  String get pluginsOpenView => 'Ansicht öffnen';

  @override
  String get pluginsOpeningView => 'Plugin-Ansicht wird geöffnet…';

  @override
  String get pluginsOperation => 'Vorgangsergebnisse abfragen';

  @override
  String pluginsOtherCapability(String name) {
    return 'Weitere angegebene Berechtigung: $name';
  }

  @override
  String get pluginsPackageFile => 'Morrow-Plugin';

  @override
  String get pluginsPreviewOnly =>
      'Nur Vorschau. Ergebnisse werden nicht automatisch in vorhandene Inhalte gespeichert.';

  @override
  String get pluginsPreviewTruncated =>
      '…nur die ersten 4.096 Zeichen werden angezeigt';

  @override
  String get pluginsProtection => 'Inhaltsschutz';

  @override
  String get pluginsProtectionDetails =>
      'Sichern Sie die ursprüngliche Schutzdatei zur Wiederherstellung mit diesem Systemkonto. Sie enthält keine Karten oder Anhänge.';

  @override
  String get pluginsProtectionFileType => 'Bibliotheksschutzdatei';

  @override
  String get pluginsProtectionSaved =>
      'Schutzdatei gesichert. Wählen Sie sie zur Wiederherstellung aus, falls der Start fehlschlägt.';

  @override
  String get pluginsRead => 'Inhalte lesen';

  @override
  String get pluginsReadingState => 'Plugin-Status wird geladen…';

  @override
  String pluginsRefreshFailed(String reason) {
    return '$reason. Die Liste konnte nicht aktualisiert werden. Wählen Sie „Liste aktualisieren“, um sie erneut zu lesen.';
  }

  @override
  String get pluginsRefreshList => 'Liste aktualisieren';

  @override
  String get pluginsRefreshState => 'Status aktualisieren';

  @override
  String get pluginsRename => 'Umbenennen';

  @override
  String pluginsResultBytes(int count, String preview) {
    return '$count Bytes\n$preview';
  }

  @override
  String get pluginsSavePermissions => 'Berechtigungen speichern';

  @override
  String pluginsSelectedFile(String name) {
    return 'Ausgewählte Datei: $name';
  }

  @override
  String get pluginsServiceAcknowledgeUncertain =>
      'Ich habe die aktualisierten Datensätze geprüft';

  @override
  String get pluginsServiceAddScope => 'Inhaltsbereich hinzufügen';

  @override
  String get pluginsServiceAttachmentId => 'Exakte Anhangskennung';

  @override
  String get pluginsServiceAuthenticationUnavailable =>
      'Die gewählte Authentifizierung fehlt, ist deaktiviert, abgelaufen oder gehört zu einer anderen Identität. Entwurfsbereiche bleiben erhalten; wählen Sie einen gültigen Ersatz oder entfernen Sie sie ausdrücklich.';

  @override
  String get pluginsServiceAuthorities =>
      'Authentifizierungs- und Veröffentlichungsdatensätze';

  @override
  String get pluginsServiceCardId => 'Exakte Kartenkennung';

  @override
  String get pluginsServiceCatalogChanged =>
      'Der Paketkatalog wurde geändert oder ist nicht verfügbar. Ihr Entwurf bleibt erhalten. Aktualisieren Sie die Auswahl ausdrücklich vor dem Speichern.';

  @override
  String get pluginsServiceClearToken => 'Token löschen';

  @override
  String get pluginsServiceCloseEditor => 'Editor schließen';

  @override
  String get pluginsServiceConfigDigest => 'Konfigurations-Hash';

  @override
  String get pluginsServiceConfiguration => 'Gespeicherte Konfiguration';

  @override
  String get pluginsServiceConfigurations => 'Gespeicherte Konfigurationen';

  @override
  String get pluginsServiceCopyClear => 'Token kopieren und löschen';

  @override
  String get pluginsServiceCreated => 'Erstellt (UTC)';

  @override
  String get pluginsServiceDays => 'Angeforderte Gültigkeit (1–30 Tage)';

  @override
  String get pluginsServiceDigestFixed =>
      'Bearbeiten behält den ursprünglichen Paket-Hash bei. Ein passendes Paket muss gewählt werden; dies aktiviert es nicht.';

  @override
  String get pluginsServiceDisable => 'Deaktivieren';

  @override
  String get pluginsServiceDisabled => 'Deaktiviert';

  @override
  String get pluginsServiceEditConfig => 'Konfiguration bearbeiten';

  @override
  String get pluginsServiceEditPublication => 'Veröffentlichung bearbeiten';

  @override
  String get pluginsServiceExpired => 'Abgelaufen oder noch nicht gültig';

  @override
  String get pluginsServiceExpires => 'Tatsächlicher Ablauf (UTC)';

  @override
  String get pluginsServiceHandler => 'Deklarierter Diensthandler';

  @override
  String get pluginsServiceIdentity => 'Dienstkennung';

  @override
  String get pluginsServiceInvalid =>
      'Prüfen Sie Felder, gewählte Genehmigungen und aktuelles Paket vor dem Speichern.';

  @override
  String get pluginsServiceIssue => 'Token ausstellen';

  @override
  String get pluginsServiceIssuedToken => 'Einmal angezeigtes Bearer-Token';

  @override
  String get pluginsServiceListenAddress =>
      'Numerische Empfangsadresse und Port';

  @override
  String get pluginsServiceLoadFailed =>
      'Datensätze konnten nicht aktualisiert werden. Aktualisieren Sie erneut vor Änderungen.';

  @override
  String get pluginsServiceManagementOnly =>
      'Verwalten Sie hier gespeicherte Konfigurationen und Genehmigungen. Speichern startet weder Listener noch Paket oder Dienst.';

  @override
  String get pluginsServiceMethod => 'HTTP-Methode';

  @override
  String get pluginsServiceNewAuthentication => 'Neue Authentifizierung';

  @override
  String get pluginsServiceNewConfig => 'Neue Konfiguration';

  @override
  String get pluginsServiceNo => 'Nein';

  @override
  String get pluginsServiceNoAuthentication =>
      'Erstellen Sie zuerst einen derzeit gültigen Authentifizierungsdatensatz.';

  @override
  String get pluginsServiceNoAuthorities =>
      'Keine Authentifizierungs- oder Veröffentlichungsdatensätze.';

  @override
  String get pluginsServiceNoConfigurations => 'Keine Dienstkonfigurationen.';

  @override
  String get pluginsServicePackage => 'Deklariertes und genehmigtes Paket';

  @override
  String get pluginsServicePackageDigest => 'Paket-Hash';

  @override
  String get pluginsServicePackageUnavailable =>
      'Das passende Paket oder seine Empfangs-/Veröffentlichungsgenehmigungen fehlen. Historische Datensätze bleiben lesbar und können deaktiviert werden.';

  @override
  String get pluginsServicePath => 'Exakter Anfragepfad';

  @override
  String get pluginsServicePolicyChanged =>
      'Der ursprüngliche Datensatz wurde geändert oder ist unbrauchbar. Aktualisieren Sie die Auswahl oder öffnen Sie den Editor vom aktuellen Datensatz aus. Ihr Entwurf bleibt erhalten.';

  @override
  String get pluginsServicePrincipalId => 'Identitätskennung';

  @override
  String get pluginsServicePrincipals =>
      'Autorisierte Identitäten und Inhaltsbereiche';

  @override
  String get pluginsServicePublicationEditor => 'Veröffentlichungsgenehmigung';

  @override
  String get pluginsServicePublicationHelp =>
      'Die Genehmigung ist an genau diese Konfiguration, Revision und Referenz gebunden. Ihr Ablauf wird durch alle gewählten Authentifizierungen begrenzt und kann früher als angefordert sein. Speichern startet keinen Listener.';

  @override
  String get pluginsServicePublicationMismatch =>
      'Diese Veröffentlichung passt nicht mehr zur aktuellen Konfiguration. Prüfen und speichern Sie ausdrücklich eine Ersatzgenehmigung.';

  @override
  String get pluginsServiceQueryPath =>
      'Separater Ergebnisabfragepfad (optional)';

  @override
  String get pluginsServiceReference => 'Genehmigungsreferenz';

  @override
  String get pluginsServiceRefresh => 'Datensätze aktualisieren';

  @override
  String get pluginsServiceRefreshSelection => 'Diese Auswahl aktualisieren';

  @override
  String get pluginsServiceRemovePrincipal => 'Identität entfernen';

  @override
  String get pluginsServiceRemoveScope => 'Bereich entfernen';

  @override
  String get pluginsServiceRetention =>
      'Aufbewahrung des Anfrageverlaufs (Millisekunden, bis zu 30 Tage)';

  @override
  String get pluginsServiceRevision => 'Revision';

  @override
  String get pluginsServiceRotate => 'Token rotieren';

  @override
  String get pluginsServiceRotateAuthentication => 'Authentifizierung rotieren';

  @override
  String get pluginsServiceRunAbandon =>
      'Datensatz behalten und Versuch beenden';

  @override
  String get pluginsServiceRunAdvanced => 'Anfrage- und Worker-Limits';

  @override
  String get pluginsServiceRunAttempt => 'Ungeklärter Startversuch';

  @override
  String get pluginsServiceRunBoundsHint =>
      'Diese Grenzen müssen auch zur Plugin-Deklaration und zu gespeicherten Genehmigungen passen. Reservierte Arbeit verbraucht das Gesamtbudget auch bei Abbruch. Nach Ablauf stoppt der Lauf; keine automatische Verlängerung.';

  @override
  String get pluginsServiceRunBytes => 'Lauf-Bytebudget (bis 67.108.864)';

  @override
  String get pluginsServiceRunCalls => 'Aufrufe pro Auftrag (bis 1.024)';

  @override
  String get pluginsServiceRunCancelled => 'Abgebrochen';

  @override
  String get pluginsServiceRunClosed => 'Geschlossen';

  @override
  String get pluginsServiceRunConcurrent => 'Gleichzeitige Aufträge (bis 128)';

  @override
  String get pluginsServiceRunControlUnknown =>
      'Das Steuerungsergebnis ist unbekannt. Aktualisieren Sie den ursprünglichen Dienststatus vor einem weiteren Vorgang.';

  @override
  String get pluginsServiceRunDenied => 'Abgelehnt';

  @override
  String get pluginsServiceRunExited => 'Dienst beendet';

  @override
  String get pluginsServiceRunHeaderBytes =>
      'Maximale Headergröße (Bytes, bis 65.536)';

  @override
  String get pluginsServiceRunHint =>
      'Wählen Sie eine genehmigte Veröffentlichung und endliche Laufgrenzen, dann starten Sie den Dienst ausdrücklich. Warten Sie nach dem Stoppen auf die Rückkehr des ursprünglichen Arbeitsbereichsbesitzers, bevor Sie das Ergebnis bestätigen.';

  @override
  String pluginsServiceRunHostFailure(String detail) {
    return 'Host-Diagnose: $detail';
  }

  @override
  String get pluginsServiceRunHttpPanel =>
      'Ein API-Dienst besitzt diese Aufgabe. Nutzen Sie das obere Dienstpanel zum Stoppen oder Bestätigen seines Endes. Ihr HTTP-Anfrageentwurf bleibt erhalten.';

  @override
  String get pluginsServiceRunIdentityChanged =>
      'Eine andere Aufgabe besitzt die Inhalte. Dieses Panel steuert sie nicht mit der vorherigen Dienstkennung.';

  @override
  String get pluginsServiceRunInvalid =>
      'Prüfen Sie Dienst und numerische Grenzen. Kein neuer Lauf wurde übermittelt.';

  @override
  String get pluginsServiceRunInvalidOutcome => 'Ungültige Konfiguration';

  @override
  String get pluginsServiceRunJobBytes => 'Bytes pro Auftrag (bis 16.777.216)';

  @override
  String get pluginsServiceRunJobs =>
      'Gesamte Aufgabenreservierungen (bis 1.000.000)';

  @override
  String get pluginsServiceRunLastObservation =>
      'Letzte Beobachtung angezeigt; aktueller Zustand ungeprüft.';

  @override
  String get pluginsServiceRunLifetime => 'Laufdauer (ms, bis 3.600.000)';

  @override
  String get pluginsServiceRunLimit => 'Limit erreicht';

  @override
  String get pluginsServiceRunLocal => 'Inhalte lokal verfügbar';

  @override
  String pluginsServiceRunNetwork(
    String bind,
    String listener,
    String supervision,
  ) {
    return 'Bindung: $bind; Listener: $listener; Überwachung: $supervision';
  }

  @override
  String get pluginsServiceRunNextSettings =>
      'Einstellungen für den nächsten ausdrücklichen Lauf';

  @override
  String get pluginsServiceRunNoSelection =>
      'Keine genehmigte Veröffentlichung verfügbar. Prüfen Sie Plugin, Konfiguration und Authentifizierung.';

  @override
  String get pluginsServiceRunOutboundAttempt =>
      'An diesen Startversuch gebundene Endpunkte';

  @override
  String get pluginsServiceRunOutboundClear => 'Endpunktauswahl leeren';

  @override
  String get pluginsServiceRunOutboundFailed =>
      'Die Endpunktliste konnte nicht geprüft werden. Aktualisieren Sie vor Verwendung ausgewählter Endpunkte.';

  @override
  String get pluginsServiceRunOutboundHint =>
      'Ausgehende APIs (optional, bis zu 8). Nur für dieses Paket genehmigte Endpunkte werden angezeigt. Ohne Auswahl sind ausgehende Aufrufe gesperrt.';

  @override
  String get pluginsServiceRunOutboundStale =>
      'Ein ausgewählter Endpunkt wurde geändert oder ist nicht mehr verfügbar. Wählen Sie seine aktuelle Version ausdrücklich oder leeren Sie die Auswahl.';

  @override
  String get pluginsServiceRunOwned =>
      'Inhalte werden vom laufenden Dienst verwaltet';

  @override
  String get pluginsServiceRunPending => 'Ausstehend';

  @override
  String get pluginsServiceRunReclaimed =>
      'Inhaltsbesitz zurückgegeben; Bestätigung erforderlich';

  @override
  String get pluginsServiceRunReclaiming =>
      'Rückgabe des Inhaltsbesitzes wird abgewartet';

  @override
  String get pluginsServiceRunRecovery => 'Bereinigung muss repariert werden';

  @override
  String get pluginsServiceRunRequestBytes => 'Maximale Anfragegröße (Bytes)';

  @override
  String get pluginsServiceRunResponseBytes => 'Maximale Antwortgröße (Bytes)';

  @override
  String get pluginsServiceRunRunning => 'Dienst läuft';

  @override
  String get pluginsServiceRunSelection => 'Genehmigte Dienstveröffentlichung';

  @override
  String get pluginsServiceRunStale =>
      'Paket, Konfiguration oder Genehmigung wurde geändert. Aktualisieren Sie die Datensätze und wählen Sie vor dem Start erneut.';

  @override
  String get pluginsServiceRunStart => 'Zeitlich begrenzten Dienst starten';

  @override
  String get pluginsServiceRunStartRejected =>
      'Die Startantwort meldete einen Fehler. Die aktuelle Aufgabe wurde geprüft; prüfen Sie Ursache und Bereinigungszustand vor dem Fortfahren.';

  @override
  String get pluginsServiceRunStartUnknown =>
      'Das Startergebnis ist unbekannt. Die Versuchskennung bleibt erhalten; aktualisieren Sie, um sie zu finden. Es wird nicht automatisch erneut gestartet.';

  @override
  String get pluginsServiceRunStarting => 'Dienst wird gestartet';

  @override
  String get pluginsServiceRunStatusFailed =>
      'Der Dienststatus konnte nicht geprüft werden. Aktualisieren Sie vor weiteren Aktionen.';

  @override
  String get pluginsServiceRunStop => 'Dienst stoppen';

  @override
  String get pluginsServiceRunStopping =>
      'Wird gestoppt; Listener- und Worker-Ende wird abgewartet';

  @override
  String get pluginsServiceRunSucceeded => 'Erfolgreich';

  @override
  String get pluginsServiceRunTask => 'Aktuelle Aufgabenkennung';

  @override
  String get pluginsServiceRunTimeout => 'Auftragszeitlimit (ms, bis 30.000)';

  @override
  String get pluginsServiceRunTimeoutOutcome => 'Zeitlimit überschritten';

  @override
  String get pluginsServiceRunTitle => 'API-Dienst ausführen';

  @override
  String get pluginsServiceRunTotalBytes =>
      'Worker-Bytebudget (bis 67.108.864)';

  @override
  String get pluginsServiceRunTransport => 'Transportfehler';

  @override
  String get pluginsServiceRunUnavailable => 'Inhaltsspeicher nicht verfügbar';

  @override
  String get pluginsServiceSaveConfig => 'Konfiguration speichern';

  @override
  String get pluginsServiceSavePublication =>
      'Veröffentlichungsgenehmigung speichern';

  @override
  String get pluginsServiceSaved =>
      'Gespeichert. Prüfen Sie unten die zurückgegebene Revision und den tatsächlichen Ablauf.';

  @override
  String get pluginsServiceScopeAttachment => 'Anhang lesen';

  @override
  String get pluginsServiceScopeCreate => 'Inhalte erstellen';

  @override
  String get pluginsServiceScopeEdit => 'Inhalte bearbeiten';

  @override
  String get pluginsServiceScopeKind => 'Erlaubter Inhaltsvorgang';

  @override
  String get pluginsServiceScopeQuery => 'Vorgang abfragen';

  @override
  String get pluginsServiceScopeRead => 'Inhalte lesen';

  @override
  String get pluginsServiceScopeRename => 'Karte umbenennen';

  @override
  String get pluginsServiceScopeSummary => 'Zusammenfassung lesen';

  @override
  String get pluginsServiceScopesHelp =>
      'Wählen Sie die Authentifizierung ausdrücklich. Ergänzen Sie unten erlaubte Vorgänge und exakte Objektkennungen. Bereiche oder Identitäten werden nur über ihre eigene Schaltfläche entfernt; vorhandene Bereiche bleiben beim Bearbeiten erhalten.';

  @override
  String get pluginsServiceTitle => 'Dienstkonfiguration';

  @override
  String get pluginsServiceTls => 'TLS erforderlich';

  @override
  String get pluginsServiceTlsAttempt =>
      'An den Startversuch gebundener Zertifikat-PEM-Hash';

  @override
  String get pluginsServiceTlsCertificate => 'Zertifikatskette auswählen';

  @override
  String get pluginsServiceTlsChecked =>
      'Zertifikat und Schlüsselpaarung geprüft. Der SHA-256 des Zertifikat-PEM steht unten. Clients müssen weiterhin Hostnamen, Gültigkeit und Vertrauenskette prüfen.';

  @override
  String get pluginsServiceTlsChecking =>
      'Zertifikatsauswahl wird verarbeitet…';

  @override
  String get pluginsServiceTlsFailed =>
      'Zertifikatsprüfung fehlgeschlagen. Prüfen Sie PEM-Dateien, Schlüsselpaarung und lokale Pfade vor einem erneuten Versuch.';

  @override
  String get pluginsServiceTlsHelp =>
      'Nicht-Loopback-Adressen erfordern TLS. Hier wird nur die Anforderung gespeichert; weder Listener noch TLS-Identität werden erstellt.';

  @override
  String get pluginsServiceTlsHint =>
      'Wählen Sie eine PEM-Zertifikatskette und einen privaten Schlüssel, dann prüfen Sie sie. Dateien werden beim Start erneut geprüft; aktive Zertifikate werden nicht automatisch rotiert.';

  @override
  String get pluginsServiceTlsInspect => 'Zertifikat prüfen';

  @override
  String get pluginsServiceTlsOutsideValidity =>
      'Die Zertifikatskette ist noch nicht gültig oder abgelaufen. Prüfen oder ersetzen Sie das Zertifikat und kontrollieren Sie es vor dem Start erneut.';

  @override
  String get pluginsServiceTlsPrivateKey => 'Privaten Schlüssel auswählen';

  @override
  String get pluginsServiceTlsRecheck =>
      'Prüfen Sie das Zertifikat vor dem Start erneut. Eine Uhrzeitänderung stellt die vorige Auswahl nicht wieder her.';

  @override
  String get pluginsServiceTlsUnavailable =>
      'Dieses Backend unterstützt keine lokale TLS-Zertifikatsauswahl.';

  @override
  String pluginsServiceTlsValidity(String end, String start) {
    return 'Gemeinsame Gültigkeit der Zertifikatskette (UTC): $start bis $end. Der Dienst stoppt nach Ablauf.';
  }

  @override
  String get pluginsServiceTokenDiscarded =>
      'Das einmal angezeigte Token wurde beim Schließen des Panels gelöscht. Stellen Sie bei Bedarf ausdrücklich ein neues aus.';

  @override
  String get pluginsServiceTokenHelp =>
      'Dieses Token wird nur jetzt angezeigt. Kopieren Sie es bei Bedarf ausdrücklich. Leeren oder Schließen entfernt es aus der Sitzung; es kann nicht aus der Liste abgerufen werden. Rotation ersetzt das vorherige Token.';

  @override
  String get pluginsServiceUncertainHelp =>
      'Aktualisieren und prüfen Sie zuerst die ursprünglichen Datensätze. Diese Meldung zu bestätigen erlaubt nur eine weitere ausdrückliche Aktion; es beweist weder ein Scheitern der vorigen Änderung noch wiederholt es diese.';

  @override
  String get pluginsServiceUnsupported =>
      'Nicht unterstützter historischer Wert';

  @override
  String get pluginsServiceWorking => 'Wird bearbeitet…';

  @override
  String get pluginsServiceWriteUnknown =>
      'Das Ergebnis der letzten Änderung ist unbekannt. Sie wurde nicht erneut gesendet.';

  @override
  String get pluginsServiceYes => 'Ja';

  @override
  String get pluginsSettingsUnknown =>
      'Die Einstellung ist noch nicht bestätigt. Aktualisieren Sie den Status, bevor Sie erneut wählen.';

  @override
  String get pluginsSnapshotDetails =>
      'Die Sicherung enthält Karten, Anhänge und Audit-Datensätze. Externe Ressourcen bleiben Verweise. Zur Wiederherstellung ist das ursprüngliche Systemkonto erforderlich.';

  @override
  String get pluginsSnapshotSaved =>
      'Bibliothek einschließlich Anhängen und ursprünglicher Schutzdatei gesichert.';

  @override
  String get pluginsStateUnavailable =>
      'Der Plugin-Status konnte nicht geladen werden. Versuchen Sie es erneut.';

  @override
  String get pluginsSummary => 'Zusammenfassungen lesen';

  @override
  String get pluginsTextInput => 'Eingabetext';

  @override
  String get pluginsThirdParty => 'Drittanbieter-Plugins';

  @override
  String get pluginsTlsIdentitiesDisable => 'Identität deaktivieren';

  @override
  String get pluginsTlsIdentitiesEmpty =>
      'Keine gespeicherten Identitäten in dieser Bibliothek.';

  @override
  String get pluginsTlsIdentitiesFileMode =>
      'Nächster Start: geprüfte lokale Dateien.';

  @override
  String get pluginsTlsIdentitiesHint =>
      'Wählen Sie eine Identität ausdrücklich. Ersetzen oder Deaktivieren stoppt die Dienste, die sie verwenden; ein neuer Start erfolgt immer ausdrücklich.';

  @override
  String get pluginsTlsIdentitiesImport =>
      'Zertifikatsdateien zum Importieren oder Ersetzen vorbereiten';

  @override
  String get pluginsTlsIdentitiesReplace => 'Durch geprüfte Dateien ersetzen';

  @override
  String get pluginsTlsIdentitiesSave => 'Als neue Identität speichern';

  @override
  String get pluginsTlsIdentitiesSaved =>
      'Gespeichert. Prüfen Sie Identität und Revision unten und wählen Sie sie für einen neuen Start.';

  @override
  String get pluginsTlsIdentitiesSavedMode =>
      'Nächster Start: gespeicherte Identität. Der Host prüft die Zertifikatsgültigkeit beim Start.';

  @override
  String get pluginsTlsIdentitiesSelect => 'Beim nächsten Start verwenden';

  @override
  String get pluginsTlsIdentitiesStale =>
      'Die Identität wurde geändert, deaktiviert oder nicht aktualisiert. Wählen Sie erneut eine aktuelle Identität.';

  @override
  String get pluginsTlsIdentitiesTitle => 'Gespeicherte TLS-Identitäten';

  @override
  String get pluginsTlsIdentitiesUnknownHint =>
      'Aktualisieren und prüfen Sie Datensätze vor der Bestätigung. Eine fehlende Quittung bedeutet kein Scheitern; erstellen Sie nicht erneut, ohne zu prüfen.';

  @override
  String get pluginsTlsIdentitiesUseFile =>
      'Geprüfte Dateien beim nächsten Start verwenden';

  @override
  String get pluginsTransform => 'Transformieren';

  @override
  String get pluginsTransformUnknown =>
      'Die Transformation konnte nicht bestätigt werden';

  @override
  String get pluginsUiExecution =>
      'Die Plugin-Ausführung wurde nicht abgeschlossen. Öffnen Sie die Ansicht erneut und versuchen Sie es noch einmal.';

  @override
  String get pluginsUiRejected =>
      'Die Plugin-Aktion wurde abgelehnt. Prüfen Sie Eingabe und aktuelle Berechtigungen.';

  @override
  String get pluginsUiUnavailable =>
      'Das Plugin ist nicht verfügbar. Prüfen Sie den Status und öffnen Sie die Ansicht erneut.';

  @override
  String get pluginsUnavailableView => 'Plugin-Ansicht nicht verfügbar';

  @override
  String pluginsUnconfirmed(String reason) {
    return '$reason. Der Vorgang ist unbestätigt. Prüfen Sie den aktualisierten Status, bevor Sie erneut wählen.';
  }

  @override
  String get pluginsUninstallKeepContent => 'Deinstallieren (Inhalte behalten)';

  @override
  String get pluginsUninstallUnknown =>
      'Die Deinstallation konnte nicht bestätigt werden';

  @override
  String get pluginsUninstalled =>
      'Deinstalliert. Ihre vorhandenen Inhalte bleiben erhalten.';

  @override
  String get pluginsUpdatingView => 'Vorschau wird aktualisiert…';

  @override
  String get pluginsUseText => 'Stattdessen Text verwenden';

  @override
  String get pluginsUseTransform => 'Transformation verwenden';

  @override
  String get pluginsViewFailed =>
      'Die Plugin-Ansicht konnte nicht geöffnet werden';

  @override
  String get pluginsWorkbench => 'Arbeitsbereich-Plugin';

  @override
  String get pluginsWorkbenchReadOnly =>
      'Das Plugin ist zugelassen, aber der Arbeitsbereich ist schreibgeschützt. Beheben Sie das Bibliotheks- oder Plugin-Problem und aktualisieren Sie den Status.';

  @override
  String get recoveryAllFiles => 'Alle Dateien';

  @override
  String get recoveryBackupExists =>
      'Am Sicherungsort existiert bereits eine Datei. Wähle einen neuen Namen.';

  @override
  String get recoveryBackupFile => 'Bibliothekssicherung';

  @override
  String get recoveryBackupUnknown =>
      'Sicherungsergebnis prüfen. Bewahre die aktuelle Datei auf und kontrolliere den Speicherort.';

  @override
  String get recoveryBindingMissing =>
      'Keine Schutzdatei an diese Bibliothek gebunden. Die gewählte Datei kann nicht zugeordnet werden.';

  @override
  String get recoveryBusy =>
      'Bibliothek wird von einem anderen Prozess verwendet. Schließe das andere Fenster und versuche es erneut.';

  @override
  String get recoveryChooseKey => 'Wiederherstellungsdatei wählen';

  @override
  String get recoveryCloseFirst =>
      'Arbeitsbereich läuft noch. Schließe ihn vor dem Bibliothekswechsel.';

  @override
  String get recoveryClosing =>
      'Warten auf das Ende des ursprünglichen Dienstes. Erneutes Öffnen und Wiederherstellen sind erst nach bestätigtem Ende möglich.';

  @override
  String get recoveryClosingUnconfirmed =>
      'Das Beenden ist noch nicht bestätigt. Die Überwachung läuft weiter; ein Zeitablauf macht ausgeführte Vorgänge nicht rückgängig.';

  @override
  String get recoveryFailed =>
      'Wiederherstellung nicht abgeschlossen. Bewahre Originaldateien auf und versuche es erneut.';

  @override
  String get recoveryIdentityBusy =>
      'Eine andere Kopie dieser Bibliothek ist geöffnet. Schließe sie, bevor du diese öffnest.';

  @override
  String get recoveryIdentityMismatch =>
      'Registrierte Bibliotheksidentität stimmt nicht überein. Bewahre Originaldaten auf und stelle die passende Sicherung wieder her.';

  @override
  String get recoveryKeyFile => 'Bibliotheksschutzdatei';

  @override
  String get recoveryKeyGuide =>
      'Falls die Schutzdatei fehlt oder beschädigt ist, wähle eine Sicherung. Sie muss zu dieser Bibliothek gehören; das ursprüngliche Systemkonto ist erforderlich.';

  @override
  String get recoveryKeyMismatch =>
      'Schlüssel passt nicht oder lässt sich nicht entschlüsseln. Verwende Originaldatei und ursprüngliches Systemkonto.';

  @override
  String get recoveryKeyUnknown =>
      'Wiederherstellungsergebnis prüfen. Versuche erneut zu öffnen; die vorherige Schutzdatei wurde, falls vorhanden, als Kopie aufbewahrt.';

  @override
  String get recoveryLibraryInvalid =>
      'Bibliothek konnte nicht geprüft oder geöffnet werden. Bewahre Originalbibliothek und Schlüssel auf und versuche es erneut.';

  @override
  String get recoveryMaintenance =>
      'Die Bibliothek benötigt Aufmerksamkeit. Bewahre Originaldateien auf und prüfe die Diagnose.';

  @override
  String get recoveryMigrationIncomplete =>
      'Die Migration dieser Bibliothek ist unvollständig. Prüfen Sie den Bericht im Ordner, behalten Sie die Quellbibliothek und versuchen Sie es mit einem neuen Zielordner erneut.';

  @override
  String get recoveryMissingKey =>
      'Bibliotheksschlüssel fehlt. Stelle die ursprüngliche .audit-key-Datei wieder her und versuche es erneut.';

  @override
  String get recoveryMissingLibrary =>
      'Schlüssel vorhanden, Bibliothek fehlt oder ist leer. Stelle die ursprüngliche Bibliothek wieder her.';

  @override
  String get recoveryOpenFailed =>
      'Arbeitsbereich konnte nicht geöffnet werden. Prüfe Plugin-Dateien und Datenordner und versuche es erneut.';

  @override
  String get recoveryPluginUnavailable =>
      'Arbeitsbereich-Plugin nicht verfügbar. Vorhandene Inhalte können weiter angesehen und exportiert werden.';

  @override
  String get recoveryRegistryInvalid =>
      'Aktive Bibliotheksregistrierung beschädigt oder nicht unterstützt. Öffnen zum Schutz der Daten gestoppt.';

  @override
  String get recoveryRegistryUnreadable =>
      'Aktive Bibliothek oder Registrierung nicht lesbar. Prüfe den ursprünglichen Speicherort; es wird nicht automatisch eine Ersatzbibliothek erstellt.';

  @override
  String get recoveryRetry => 'Erneut versuchen';

  @override
  String get recoverySnapshot => 'Bibliothekssicherung wiederherstellen';

  @override
  String get recoverySnapshotGuide =>
      'Stelle die Bibliothekssicherung in einem neuen Ordner wieder her und wechsle dorthin. Der Originalordner bleibt erhalten. Inhalte entsprechen dem Sicherungszeitpunkt; das ursprüngliche Systemkonto ist erforderlich.';

  @override
  String get recoverySnapshotInvalid =>
      'Sicherungsformat oder Integritätsprüfung ungültig. Bewahre die ursprüngliche Sicherungsdatei auf.';

  @override
  String get recoverySnapshotUnknown =>
      'Wiederherstellungsergebnis prüfen. Kontrolliere den Zielordner; die Originalbibliothek wurde nicht ersetzt.';

  @override
  String recoverySwitchUnconfirmed(String path) {
    return 'Sicherung nach $path wiederhergestellt, Wechsel aber nicht bestätigt. Bewahre den Ordner auf und öffne den Arbeitsbereich zur Prüfung erneut.';
  }

  @override
  String get recoverySwitchUnknown =>
      'Bibliothekswechsel nicht bestätigt. Öffne den Arbeitsbereich erneut zur Prüfung.';

  @override
  String get recoveryTargetExists =>
      'Wiederherstellungsziel existiert bereits. Wähle einen neuen, noch nicht vorhandenen Ordner.';

  @override
  String get recoveryTitle => 'Arbeitsbereich erneut öffnen';

  @override
  String get shutdownBackground => 'Im Hintergrund weiter schließen';

  @override
  String get shutdownBackgroundHint =>
      'Morrow schließt sich nach dem Ende des Dienstes. Bei einem Fehler oder einer langen Verzögerung erscheint dieses Fenster wieder.';

  @override
  String get shutdownFailure =>
      'Beim Beenden ist ein Problem aufgetreten. Der Inhaltsdienst wird weiter überwacht.';

  @override
  String get shutdownStillRunning =>
      'Das Beenden dauert länger als erwartet. Der Inhaltsdienst wird weiter überwacht.';

  @override
  String get shutdownTitle => 'Arbeitsbereich wird geschlossen';

  @override
  String get shutdownWaiting =>
      'Der Inhaltsdienst wird beendet. Die Bibliothek bleibt bis zum bestätigten Ende in seinem Besitz.';

  @override
  String get visualApplyColor => 'Farbe anwenden';

  @override
  String get visualApplyComponent => 'Auf diese Komponente anwenden';

  @override
  String get visualApplyTexture => 'Medium anwenden';

  @override
  String visualAttachmentDetails(String action, String extension, String size) {
    return '$extension · $size · $action';
  }

  @override
  String get visualAttachmentFailure =>
      'Dateivorgang fehlgeschlagen. Prüfe Datei und freien Speicher.';

  @override
  String get visualAttachmentPreview => 'Vorschau des lokalen Anhangs';

  @override
  String get visualAttachmentReadFailure =>
      'Anhang konnte nicht gelesen werden. Importiere ihn erneut.';

  @override
  String get visualAudio => 'Audio';

  @override
  String get visualAudioStateFailure =>
      'Audiostatus konnte nicht bestätigt werden. Versuche es erneut.';

  @override
  String get visualAutoLyrics => 'Fehlende Liedtexte automatisch online suchen';

  @override
  String get visualCancel => 'Abbrechen';

  @override
  String get visualChangeCover => 'Albumcover ändern';

  @override
  String get visualChooseAudio =>
      'Wähle eine Audiodatei oder eine gleichnamige LRC-Datei.';

  @override
  String get visualChooseLyrics => 'Wähle eine LRC- oder TXT-Liedtextdatei.';

  @override
  String get visualClickPreview => 'Für Vorschau auswählen';

  @override
  String get visualClose => 'Schließen';

  @override
  String get visualCloseDialog => 'Dialog schließen';

  @override
  String get visualCloseWindow => 'Fenster schließen';

  @override
  String get visualCollapsePlaylist => 'Wiedergabeliste einklappen';

  @override
  String get visualColorGuide =>
      'Ziehe im Farbkreis, um Farbton und Sättigung zu wählen, und passe dann die Helligkeit an. Du kannst auch einen Farbwert eingeben.';

  @override
  String get visualColorTitle => 'Bring Farbe in deinen Bereich';

  @override
  String visualComponentCompass(String title) {
    return '$title · Farbkreis';
  }

  @override
  String get visualComponents => 'Komponenten und Karten';

  @override
  String get visualComponentsGuide =>
      'Alle Elemente folgen standardmäßig dem Thema. Passe eine Karte an, ohne andere zu verändern.';

  @override
  String get visualCornerTips1 =>
      'Nicht jede Idee muss nützlich sein.\nManche machen den Tag einfach interessanter.';

  @override
  String get visualCornerTips2 =>
      'Schreib sie auf und lass sie wachsen.\nEine Idee muss nicht fertig ankommen.';

  @override
  String get visualCornerTips3 =>
      'Lass dir ein wenig Freiraum.\nNeugier braucht Platz zum Atmen.';

  @override
  String get visualCornerTips4 =>
      'Probiere heute etwas Neues.\nEin kleiner Umweg kann überraschen.';

  @override
  String get visualCornerTips5 =>
      'Tagträume können irgendwohin führen.\nLass deine Gedanken wandern.';

  @override
  String get visualCornerTips6 =>
      'Nimm dir Zeit für das, was du liebst.\nDu musst seinen Wert nicht beweisen.';

  @override
  String get visualCornerTips7 =>
      'Fortschritt darf klein sein.\nAnfangen zu wollen zählt bereits.';

  @override
  String get visualCornerTips8 =>
      'Schau ab und zu aus dem Fenster.\nAuch das Leben inspiriert.';

  @override
  String get visualCover => 'Cover';

  @override
  String get visualCustomCompass => 'Farbkreis · Benutzerdefiniert';

  @override
  String get visualCustomMaterialGuide =>
      'Zum Folgen des Themas ausschalten; eigene Einstellungen dieses Elements bleiben erhalten.';

  @override
  String get visualDefaultOpen => 'Mit Standard-App öffnen';

  @override
  String get visualDownloadOpen => 'Zum Öffnen herunterladen';

  @override
  String get visualEmbeddedLyrics => 'In Audiodatei eingebettet';

  @override
  String get visualExpandPlaylist => 'Wiedergabeliste ausklappen';

  @override
  String get visualFile => 'Datei';

  @override
  String get visualFileOpenFailure =>
      'Datei konnte nicht geöffnet werden. Installiere eine passende App oder speichere den Anhang und öffne ihn dort.';

  @override
  String get visualFileRetry =>
      'Dateivorgang fehlgeschlagen. Versuche es erneut.';

  @override
  String get visualFindLyrics => 'Liedtext finden';

  @override
  String get visualFindLyricsGuide =>
      'Suche in LRCLIB nach Titel und Interpret und wähle die passende Version.';

  @override
  String visualFollowChain(String path) {
    return 'Verknüpfungskette: $path';
  }

  @override
  String visualFollowComponent(String name) {
    return 'Folgt: $name';
  }

  @override
  String get visualFollowCycle => 'Würde einen Zyklus erzeugen';

  @override
  String get visualFollowGuide =>
      'Anderem Element folgen; nach dem Trennen gelten wieder die eigenen Einstellungen.';

  @override
  String get visualFollowTheme => 'Thema folgen';

  @override
  String get visualFooterLyrics => 'Liedtext unten anzeigen';

  @override
  String get visualFooterTips => 'Tipps unten anzeigen';

  @override
  String get visualFooterTips1 => 'Nichts eilt. Gib deiner Neugier etwas Zeit.';

  @override
  String get visualFooterTips10 =>
      'Du musst nicht jede Minute füllen. Lass etwas Freiraum.';

  @override
  String get visualFooterTips2 =>
      'Notiere einen Gedanken. Ordnen kannst du später.';

  @override
  String get visualFooterTips3 =>
      'Mach aus einer großen Idee einen kleinen Schritt für heute.';

  @override
  String get visualFooterTips4 =>
      'Streck dich und gönn deinen Augen eine Pause.';

  @override
  String get visualFooterTips5 =>
      'Eine Idee darf vorerst unbeantwortet bleiben.';

  @override
  String get visualFooterTips6 =>
      'Manche Entdeckungen kommen, wenn du langsamer wirst.';

  @override
  String get visualFooterTips7 =>
      'Ein festgehaltenes Detail lässt eine Idee wachsen.';

  @override
  String get visualFooterTips8 =>
      'Eine kurze Notiz heute kann morgen ein Anfang sein.';

  @override
  String get visualFooterTips9 =>
      'Lass die Gedanken wandern und kehre dann zu dem zurück, was dir Freude macht.';

  @override
  String get visualFrosting => 'Mattierung';

  @override
  String get visualGif => 'Animiertes GIF';

  @override
  String get visualHexColor => 'HEX-Farbe';

  @override
  String get visualHexInvalid => 'Gib einen sechsstelligen Hex-Farbwert ein.';

  @override
  String get visualImage => 'Bild';

  @override
  String get visualImageDecodeFailure =>
      'Bild konnte nicht dekodiert werden. Speichere es und öffne es mit einer anderen App.';

  @override
  String visualImageLoadFailure(String name) {
    return 'Bild konnte nicht geladen werden: $name';
  }

  @override
  String visualImageNotImported(String name) {
    return '$name (Bild nicht importiert)';
  }

  @override
  String visualImageUnavailable(String name) {
    return 'Bild nicht verfügbar: $name';
  }

  @override
  String get visualImportFailure =>
      'Import fehlgeschlagen. Prüfe Datei, Kodierung und freien Speicher.';

  @override
  String get visualImportLyrics => 'Liedtexte importieren';

  @override
  String get visualImportMusic => 'Musik importieren';

  @override
  String get visualImportMusicHint => 'Mit + lokale Titel importieren';

  @override
  String get visualIndependentMaterial => 'Eigenes Material';

  @override
  String get visualInheritColor => 'Themenfarbe verwenden';

  @override
  String get visualLinkFailure =>
      'Link konnte nicht geöffnet werden. Kopiere die Adresse und versuche es erneut.';

  @override
  String visualLoadImage(String name) {
    return 'Bild laden · $name';
  }

  @override
  String get visualLoading => 'Wird geladen…';

  @override
  String get visualLyricsEmpty => 'Die Liedtextdatei ist leer.';

  @override
  String get visualLyricsFile => 'Liedtextdatei';

  @override
  String get visualLyricsImportHint =>
      'Importiere Liedtexte oder suche online.';

  @override
  String get visualLyricsLoading => 'Liedtexte werden geladen…';

  @override
  String visualLyricsMatch(String album, String kind, int seconds) {
    String _temp0 = intl.Intl.pluralLogic(
      seconds,
      locale: localeName,
      other: '$seconds Sekunden',
      one: '$seconds Sekunde',
    );
    return '$album\n$kind · $_temp0';
  }

  @override
  String get visualLyricsMissing =>
      'Keine Liedtexte gefunden. Importiere eine Datei oder suche erneut.';

  @override
  String get visualLyricsNotFound =>
      'Keine Liedtexte gefunden. Ändere Titel oder Interpret.';

  @override
  String get visualLyricsOnPlay =>
      'Liedtexte während der Wiedergabe automatisch laden';

  @override
  String get visualLyricsParseFailure =>
      'Liedtexte konnten nicht verarbeitet werden. Importiere sie erneut.';

  @override
  String get visualLyricsReadFailure =>
      'Liedtexte konnten nicht geladen werden. Importiere sie manuell oder versuche es erneut.';

  @override
  String get visualLyricsServiceFailure =>
      'Keine Verbindung zum Liedtextdienst. Versuche es später oder importiere lokale Liedtexte.';

  @override
  String get visualLyricsSize =>
      'Liedtextdateien dürfen höchstens 1 MB groß sein.';

  @override
  String get visualLyricsSources => 'Lokale Datei → Eingebettet → LRCLIB';

  @override
  String get visualLyricsVersions =>
      'Mehrere Versionen gefunden. Wähle eine in der Suche.';

  @override
  String get visualMaterialPreview => 'Materialvorschau';

  @override
  String get visualMaterialSource => 'Materialquelle';

  @override
  String get visualMaximize => 'Maximieren';

  @override
  String get visualMediaAddress => 'Medienadresse';

  @override
  String get visualMediaAddressInvalid =>
      'Gib eine gültige HTTP- oder HTTPS-Adresse ohne Anmeldedaten ein.';

  @override
  String get visualMediaPreviewFailure =>
      'Keine Medienvorschau möglich. Speichere die Datei und öffne sie mit einer anderen App.';

  @override
  String get visualMediaType => 'Medientyp';

  @override
  String get visualMinimize => 'Minimieren';

  @override
  String get visualMusic => 'Musik';

  @override
  String get visualMusicEmptyTitle => 'Mach Platz für Musik';

  @override
  String get visualMusicPlayer => 'Musikplayer';

  @override
  String get visualNextTrack => 'Nächster Titel';

  @override
  String get visualNoLyricsRead => 'Keine Liedtexte geladen';

  @override
  String visualNoLyricsTitle(String title) {
    return '♪ $title · Kein Liedtext';
  }

  @override
  String get visualNoTimeline => 'Keine Zeitangaben';

  @override
  String get visualOpacity => 'Deckkraft';

  @override
  String get visualOptionalArtist => 'Interpret (optional)';

  @override
  String get visualOwnMaterial => 'Design oder eigene Einstellungen';

  @override
  String get visualPauseMusic => 'Musik pausieren';

  @override
  String get visualPaused => 'Pausiert';

  @override
  String get visualPlainLyrics => 'Liedtext ohne Zeitangaben';

  @override
  String get visualPlayMusic => 'Musik abspielen';

  @override
  String get visualPlaybackFailure =>
      'Titel kann nicht abgespielt werden. Prüfe die Datei oder nutze ein anderes Audioformat.';

  @override
  String visualPlaybackPosition(int count, int index, String state) {
    return '$index / $count · $state';
  }

  @override
  String get visualPlaybackRequestFailure =>
      'Wiedergabe konnte nicht gestartet werden. Versuche es erneut.';

  @override
  String get visualPlaying => 'Wiedergabe';

  @override
  String get visualPlaylistEmpty => 'Die Wiedergabeliste ist leer';

  @override
  String get visualPlaylistLyricsHint =>
      'LRC-Liedtexte über das Wiedergabelistenmenü importieren';

  @override
  String get visualPlaylistSaved =>
      'Wiedergabeliste und Liedtexte werden automatisch gespeichert';

  @override
  String get visualPlaylistUpdateFailure =>
      'Wiedergabeliste konnte nicht aktualisiert werden. Versuche es erneut.';

  @override
  String get visualPreviewColor => 'Farbvorschau';

  @override
  String get visualPreviousTrack => 'Vorheriger Titel';

  @override
  String get visualRemoveAttachment => 'Anhang entfernen';

  @override
  String get visualRemoveTrack => 'Aus Wiedergabeliste entfernen';

  @override
  String get visualResetMaterial => 'Auf Thema zurücksetzen';

  @override
  String get visualRestoreWindow => 'Wiederherstellen';

  @override
  String get visualSaveAttachment => 'Anhang speichern unter';

  @override
  String get visualSearch => 'Suchen';

  @override
  String get visualSearchLyrics => 'Liedtexte suchen';

  @override
  String get visualSongCover => 'Albumcover';

  @override
  String get visualSongTitle => 'Songtitel';

  @override
  String get visualSyncedLyrics => 'Synchronisierte Liedtexte';

  @override
  String get visualTextureFailure =>
      'Medium konnte nicht geladen werden. Prüfe Datei, Adresse und Format. Webmedien müssen auch ursprungsübergreifenden Zugriff erlauben.';

  @override
  String get visualTextureLinkGuide =>
      'Füge einen direkten HTTP- oder HTTPS-Link zu Bild, GIF oder Video ein. Suche bei geteilten Webseiten zuerst die Original-Medienadresse.';

  @override
  String get visualTextureLinkTitle => 'Hol dir Inspiration';

  @override
  String get visualTexturePlaybackGuide =>
      'Videos laufen standardmäßig lautlos in Schleife; Ton lässt sich in den Einstellungen aktivieren. Onlinemedien müssen Zugriff erlauben, im Web auch ursprungsübergreifend.';

  @override
  String get visualTipsMaterialGuide =>
      'Aus: transparente Tipps; ein: Material unten. Eigene Werte bleiben erhalten.';

  @override
  String get visualTransparentTips => 'Transparente Einblendung (Standard)';

  @override
  String get visualUseCustomMaterial => 'Eigenes Material verwenden';

  @override
  String get visualVideo => 'Video';

  @override
  String get visualViewLyrics => 'Liedtext ansehen';
}
