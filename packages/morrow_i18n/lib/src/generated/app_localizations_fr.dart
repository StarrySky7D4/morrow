// ignore: unused_import
import 'package:intl/intl.dart' as intl;
import 'app_localizations.dart';

// ignore_for_file: type=lint

/// The translations for French (`fr`).
class AppLocalizationsFr extends AppLocalizations {
  AppLocalizationsFr([String locale = 'fr']) : super(locale);

  @override
  String get commonAppName => 'Morrow';

  @override
  String get commonCancel => 'Annuler';

  @override
  String commonCount(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count éléments',
      one: '$count élément',
      zero: 'Aucun élément',
    );
    return '$_temp0';
  }

  @override
  String commonGreeting(String name) {
    return 'Bonjour, $name';
  }

  @override
  String get importsAttachmentLimit =>
      'Importez au maximum 20 pièces jointes à la fois. Collez le reste séparément.';

  @override
  String get importsClipboardChanged =>
      'Le presse-papiers a changé pendant la lecture. Collez à nouveau.';

  @override
  String get importsEmbeddedImageUnreadable =>
      'Impossible de lire une image intégrée.';

  @override
  String get importsEmbeddedImagesSeparate =>
      'Certaines images intégrées doivent être importées séparément.';

  @override
  String get importsExcelValues =>
      'Valeurs et formules converties en Markdown. Mise en forme et cellules fusionnées conservées en XML.';

  @override
  String get importsExcelXmlKept =>
      'Le tableau Excel original est conservé en pièce jointe XML.';

  @override
  String get importsFileTooLarge =>
      'Le fichier du presse-papiers dépasse 200 Mo.';

  @override
  String get importsItemLimit =>
      'Seuls les 20 premiers éléments ont été lus. Collez les autres séparément.';

  @override
  String get importsItemUnreadable =>
      'Un élément du presse-papiers est illisible. Les autres contenus lisibles sont conservés.';

  @override
  String get importsLocalImageNotRead =>
      'Les images locales liées ne sont pas lues automatiquement. Collez l’image ou importez le fichier original.';

  @override
  String get importsMergedTable =>
      'Cellules fusionnées converties en tableau lisible. Format original conservé dans la pièce jointe HTML.';

  @override
  String get importsOfficeBusy =>
      'Une autre application utilise le presse-papiers. Les objets Office n’ont pas été lus.';

  @override
  String get importsOfficeEmbeddedKept =>
      'L’objet Office intégré est conservé comme pièce jointe originale. Modifiez graphiques, formules et mise en page dans l’application d’origine.';

  @override
  String get importsOfficeExportFailed =>
      'L’objet Office dépasse la limite ou ne peut pas être exporté. Enregistrez-le dans l’application d’origine puis importez-le.';

  @override
  String get importsOfficeReadFailed =>
      'Impossible de lire le contenu Office. Le reste du presse-papiers reste disponible.';

  @override
  String get importsOfficeUnavailable =>
      'Le presse-papiers Office est temporairement indisponible.';

  @override
  String get importsOfficeUnreadable =>
      'Objet Office original illisible. Les autres contenus disponibles sont conservés.';

  @override
  String get importsRichFallback =>
      'Certaines mises en forme n’ont pas été converties. Le texte lisible est conservé.';

  @override
  String get importsRichTooLarge =>
      'Le texte enrichi dépasse 2 Mo. Importez le document en pièce jointe.';

  @override
  String get importsRtfTooLarge =>
      'Contenu RTF trop volumineux. Importez le document original.';

  @override
  String get importsSpreadsheetTooLarge =>
      'Tableau trop volumineux. Importez le fichier Excel.';

  @override
  String get importsTableConverted =>
      'Tableau converti en Markdown. Les données complètes sont conservées en pièce jointe TSV.';

  @override
  String get importsTextTooLarge =>
      'Le texte dépasse 2 Mo. Importez-le comme fichier.';

  @override
  String get importsTotalTooLarge =>
      'Les fichiers collés dépassent 200 Mo au total. Importez-les par petits lots.';

  @override
  String get importsUnsupported =>
      'Presse-papiers non pris en charge ici. Importez un fichier.';

  @override
  String get mainActiveProjects => 'En cours';

  @override
  String get mainAdjustCustomTone => 'Ajuster la teinte';

  @override
  String get mainAmbientDetail =>
      'Une lumière fluide apporte une touche de couleur.';

  @override
  String get mainAppTitle => 'Morrow — Place aux idées';

  @override
  String get mainAppearance => 'Apparence';

  @override
  String get mainArrangeIdeas => 'Trier les idées';

  @override
  String mainAttachmentCount(int count) {
    return 'Pièces jointes · $count';
  }

  @override
  String mainAttachmentHint(int count, String name) {
    return '$count pièces jointes · $name';
  }

  @override
  String get mainAttachmentLimit => 'Jusqu’à 20 pièces jointes par fiche.';

  @override
  String get mainAutosaveNotice => 'Apparence et idées enregistrées localement';

  @override
  String get mainAwaitDiscovery => 'En attente d’une découverte';

  @override
  String get mainBackToWorkbench => 'Retour à l’espace de travail';

  @override
  String get mainBackgroundCanvas => 'Fond de l’espace';

  @override
  String get mainBackgroundSound => 'Lire le son du fond';

  @override
  String get mainBodyHint =>
      'Écrivez vos idées ou collez du contenu…\n\nTitres #, listes, tableaux et blocs de code pris en charge';

  @override
  String get mainBrightWhite => 'Blanc';

  @override
  String get mainBuiltinTexture => 'Texture intégrée';

  @override
  String get mainCanvasCompass => 'Cercle de teinte du fond';

  @override
  String get mainCaptureIdea => 'Noter l’idée';

  @override
  String get mainCaptureNow => 'Noter une pensée';

  @override
  String mainCardAttachments(int count, String name) {
    return 'Fichiers $count · $name';
  }

  @override
  String get mainCategoryExperiment => 'Expérience';

  @override
  String get mainCategoryIdea => 'Idée';

  @override
  String get mainCategoryProject => 'Projet';

  @override
  String get mainCategoryPrompt => 'Où la conserver';

  @override
  String get mainChangeFailed =>
      'Modification non enregistrée. Votre brouillon est conservé ; réessayez.';

  @override
  String get mainCheckAgain => 'Vérifier à nouveau';

  @override
  String get mainClearSearch => 'Effacer la recherche';

  @override
  String get mainClipboardEmpty =>
      'Aucun texte ni fichier lisible dans le presse-papiers. Copiez un fichier depuis votre gestionnaire ou utilisez l’importation.';

  @override
  String get mainClipboardReadFailed =>
      'Impossible de lire le contenu. Importez un fichier ou vérifiez les autorisations du fichier et du presse-papiers.';

  @override
  String get mainClipboardSupport =>
      'Markdown, texte enrichi et tableaux Office, captures et fichiers pris en charge. Les objets complexes gardent leurs pièces jointes originales. Jusqu’à 20 pièces de 200 Mo chacune.';

  @override
  String get mainCollapseSidebar => 'Réduire la barre latérale';

  @override
  String get mainCompletedProjects => 'Terminés';

  @override
  String get mainComponentCompass => 'Cercle de teinte du composant';

  @override
  String get mainComponentEmpty => 'État vide';

  @override
  String get mainComponentFooter => 'Conseils et paroles en bas de page';

  @override
  String get mainComponentHero => 'Carte d’aperçu';

  @override
  String get mainComponentNavigation => 'Navigation latérale';

  @override
  String get mainComponentQuickCapture => 'Note rapide';

  @override
  String get mainComponentSearch => 'Barre de recherche';

  @override
  String get mainComponentSettings => 'Composants et cartes · Réglages';

  @override
  String get mainContentCommittedRefreshFailed =>
      'L’opération a été enregistrée, mais le contenu à jour n’a pas pu être chargé. Actualisez pour l’afficher.';

  @override
  String get mainContentProtection => 'Protection du contenu';

  @override
  String get mainContentRead => 'Contenu lu';

  @override
  String mainContentReadFiles(int count) {
    return 'Contenu lu ; $count pièces jointes conservées';
  }

  @override
  String get mainCornerRadius => 'Rayon des angles';

  @override
  String get mainCredentialSettings => 'Identifiants';

  @override
  String get mainCrystal => 'Cristallin';

  @override
  String get mainCrystalDetail =>
      'Léger et transparent, pour laisser briller les couleurs.';

  @override
  String get mainCuriosity =>
      'Les belles découvertes commencent\npar un peu de curiosité.';

  @override
  String get mainCustomCompass => 'Cercle chromatique · Personnalisé';

  @override
  String get mainCustomLightness => 'Luminosité personnalisée';

  @override
  String get mainCustomTheme => 'Personnalisé';

  @override
  String get mainDaily => 'Petites choses';

  @override
  String get mainDailyExplore => 'Explorez pendant dix minutes';

  @override
  String get mainDailyIdea => 'Notez une idée';

  @override
  String get mainDailyWater => 'Servez-vous un verre d’eau';

  @override
  String get mainDarkTheme => 'Sombre';

  @override
  String get mainDeepBlack => 'Noir';

  @override
  String get mainDefaultCanvas => 'Par défaut';

  @override
  String get mainDefaultGlobalColor =>
      'Couleur par défaut · Tous les contrôles';

  @override
  String get mainDelete => 'Supprimer';

  @override
  String mainDeleted(String title) {
    return '« $title » supprimé';
  }

  @override
  String get mainDiagnosticDetails => 'Détails du diagnostic';

  @override
  String get mainDone => 'Terminé';

  @override
  String get mainDraftImportRecoveryCancelBody =>
      'Après l’annulation, la demande d’abandon initiale ne sera plus exécutée. Cela ne supprime pas la pièce jointe et n’annule pas les autres modifications.';

  @override
  String get mainDraftImportRecoveryCancelDecision =>
      'Annuler la demande d’abandon';

  @override
  String get mainDraftImportRecoveryCancelTitle =>
      'Annuler cette demande d’abandon ?';

  @override
  String get mainDraftImportRecoveryCancelled =>
      'Annulée : la demande d’abandon initiale ne sera plus exécutée.';

  @override
  String get mainDraftImportRecoveryClose => 'Fermer';

  @override
  String get mainDraftImportRecoveryCommitted =>
      'Terminée : l’importation de cette pièce jointe a été abandonnée.';

  @override
  String get mainDraftImportRecoveryConfirmedRefreshFailed =>
      'Action confirmée, mais la liste n’a pas été actualisée. Actualisez-la pour voir son état actuel.';

  @override
  String get mainDraftImportRecoveryConflict =>
      'Conflit : le brouillon a changé. Cette demande peut seulement être annulée.';

  @override
  String get mainDraftImportRecoveryEmpty =>
      'Aucune décision sur une pièce jointe à vérifier.';

  @override
  String get mainDraftImportRecoveryFailed =>
      'Impossible de vérifier cette décision. Actualisez, puis réessayez.';

  @override
  String get mainDraftImportRecoveryPending =>
      'En attente : la demande d’abandon initiale n’est pas confirmée.';

  @override
  String get mainDraftImportRecoveryRetry => 'Réessayer l’abandon initial';

  @override
  String get mainDraftImportRecoveryTitle =>
      'Vérifier les décisions sur les pièces jointes';

  @override
  String get mainEdit => 'Modifier';

  @override
  String get mainEditIdeaTitle => 'Précisez votre idée';

  @override
  String get mainEditorClosedUnknown =>
      'L’éditeur est fermé, mais l’enregistrement n’est pas confirmé. Rouvrez l’espace de travail pour vérifier avant de créer une copie.';

  @override
  String get mainEditorContinueDraft => 'Continuer la modification';

  @override
  String get mainEditorContinueFailed =>
      'La modification précédente est confirmée et le nouveau brouillon reste dans cette fenêtre. Impossible d’ouvrir l’éditeur suivant ; réessayez.';

  @override
  String get mainEditorNewerDraft =>
      'La modification précédente est enregistrée. Le nouveau brouillon ne l’est pas encore.';

  @override
  String get mainEditorPendingDraft =>
      'Vous pouvez continuer à écrire. Vérifiez d’abord l’enregistrement précédent ; les nouvelles modifications ne seront pas envoyées automatiquement.';

  @override
  String get mainEditorRecoveryAbandonBody =>
      'Abandonner cette modification non validée ? Son opération d’origine sera bloquée. Le contenu enregistré et les nouveaux brouillons resteront inchangés. Une modification déjà validée ne sera pas annulée.';

  @override
  String get mainEditorRecoveryAbandonTitle => 'Abandonner la modification';

  @override
  String get mainEditorRecoveryCommitted =>
      'La modification initiale a été enregistrée. Vérifiez le contenu actuel et confirmez sans enregistrer à nouveau.';

  @override
  String get mainEditorRecoveryConfirm => 'Vérifier et confirmer';

  @override
  String get mainEditorRecoveryConflict =>
      'La version de référence ou l’historique a changé. La proposition est conservée et ne peut pas remplacer le contenu actuel.';

  @override
  String get mainEditorRecoveryEmpty =>
      'Aucune proposition enregistrée à vérifier.';

  @override
  String get mainEditorRecoveryFailed =>
      'La vérification a échoué. La proposition initiale est conservée. Actualisez et réessayez.';

  @override
  String get mainEditorRecoveryPending =>
      'La modification initiale reste non confirmée. Continuer réessaie uniquement cette proposition, sans envoyer de nouvelles modifications.';

  @override
  String get mainEditorRecoveryResume => 'Reprendre la modification initiale';

  @override
  String get mainEditorRecoveryTitle => 'Vérifier les modifications';

  @override
  String get mainEditorSubtitle =>
      'Textes, tableaux, images : gardez-les ici pendant que votre idée prend forme.';

  @override
  String get mainEditorUnavailable =>
      'Éditeur indisponible. Vérifiez le service de contenu et réessayez.';

  @override
  String get mainEndpointSettings => 'Points de terminaison sortants';

  @override
  String get mainExpandSettings => 'Développer les paramètres';

  @override
  String get mainExpandSidebar => 'Développer la barre latérale';

  @override
  String get mainExtensionPlugins => 'Extensions';

  @override
  String get mainFavoriteAttachments => 'Pièces jointes favorites';

  @override
  String get mainFavoriteRecords => 'Fiches favorites';

  @override
  String mainFavoriteTooltip(String title) {
    return 'Ajouter $title aux favoris';
  }

  @override
  String get mainFavoritesIntro =>
      'Vos textes, images et fichiers favoris au même endroit.';

  @override
  String mainFieldLimit(int limit) {
    return 'Maximum $limit caractères. Raccourcissez le contenu ou importez-le comme fichier.';
  }

  @override
  String get mainFilterAll => 'Tout';

  @override
  String get mainFilterAttachments => 'Avec fichiers';

  @override
  String get mainFilterFavorites => 'Favoris uniquement';

  @override
  String get mainFilterFile => 'Fichiers';

  @override
  String get mainFilterImage => 'Images';

  @override
  String get mainFilterMedia => 'Audio / vidéo';

  @override
  String get mainFilterPending => 'À faire';

  @override
  String get mainFilterText => 'Texte';

  @override
  String get mainFollowTheme => 'Suivre le thème';

  @override
  String get mainFontApply => 'Appliquer la police';

  @override
  String get mainFontDefault => 'Police par défaut';

  @override
  String get mainFontFailed =>
      'Impossible de charger cette police. Vérifiez le fichier ou redémarrez l’application et réessayez.';

  @override
  String get mainFontFamily => 'Nom de la police système';

  @override
  String get mainFontHelp =>
      'TTF / OTF, 20 Mio maximum. Les polices système indisponibles sont remplacées automatiquement.';

  @override
  String get mainFontHint => 'Par exemple : Arial ou Microsoft YaHei';

  @override
  String get mainFontImport => 'Importer une police';

  @override
  String get mainFontReset => 'Rétablir la police par défaut';

  @override
  String get mainFontSettings => 'Polices';

  @override
  String get mainFontUnavailable =>
      'La police enregistrée est indisponible. La police par défaut est utilisée temporairement.';

  @override
  String get mainFrostDetail =>
      'Adoucissez le fond et faites place à vos idées.';

  @override
  String get mainFrostEffect => 'Flou';

  @override
  String get mainFrostOpacity => 'Opacité du verre';

  @override
  String get mainFrostUnavailable =>
      'Le flou du bureau est indisponible. La teinte et l’opacité restent réglables.';

  @override
  String get mainFrosted => 'Dépoli';

  @override
  String get mainGlassTexture => 'Style de verre';

  @override
  String mainGlobalColor(String color) {
    return '$color · Tous les contrôles';
  }

  @override
  String get mainGreeting => 'Laissez grandir vos idées.';

  @override
  String get mainGreetingDetail =>
      'Gardez les petits détails et les étincelles d’inspiration.';

  @override
  String get mainHeroBody =>
      'Une pensée, une petite tâche, un « et si ».\nTout commence ici.';

  @override
  String get mainHeroCaption => 'LE COIN DES POSSIBLES';

  @override
  String get mainHeroTitle => 'On peut commencer petit.';

  @override
  String get mainHideAppearance => 'Masquer les réglages d’apparence';

  @override
  String get mainHideCustomTone => 'Masquer la teinte personnalisée';

  @override
  String get mainHidePreview => 'Masquer l’aperçu';

  @override
  String get mainHttpSettings => 'Tâches HTTP';

  @override
  String get mainHypothesis => 'Hypothèse';

  @override
  String get mainHypothesisPrompt => 'Hypothèse à tester';

  @override
  String get mainHypothesisSection => 'Hypothèse / À essayer';

  @override
  String get mainIdeaDetails =>
      'Gardez les détails. Clarifiez la prochaine étape.';

  @override
  String get mainIdeaNameHint => 'Donnez-lui un nom';

  @override
  String get mainIdeaNameRequired => 'Notez d’abord votre idée';

  @override
  String get mainIdeaSaved => 'Idée enregistrée.';

  @override
  String get mainImportFailed =>
      'Impossible d’importer le média. Vérifiez le fichier et l’espace disponible.';

  @override
  String get mainImportFile => 'Importer un fichier';

  @override
  String get mainInboxIntro =>
      'Capturez d’abord, organisez ensuite. Transformez les bonnes idées en petits projets.';

  @override
  String get mainIoNoDeclarations =>
      'Aucune extension installée ne déclare d’accès aux fichiers ou au réseau.';

  @override
  String get mainIoSettings => 'Réseau et fichiers';

  @override
  String get mainIoSettingsGuide =>
      'Gérez l’accès des extensions aux fichiers et au réseau séparément de l’apparence. Approuver une capacité ne donne pas accès à tous les fichiers ou points de terminaison ; les opérations dépendent du moteur actuel.';

  @override
  String get mainIoSettingsSummary =>
      'Autorisations, identifiants, points de terminaison et services API';

  @override
  String get mainJustNow => 'À l’instant';

  @override
  String get mainLabIntro =>
      'Partez d’une hypothèse. Gardez vos essais, observations et surprises.';

  @override
  String get mainLanguage => 'Langue';

  @override
  String get mainLanguageChinese => '简体中文';

  @override
  String get mainLanguageEnglish => 'English';

  @override
  String get mainLanguageSystem => 'Langue du système';

  @override
  String get mainLavender => 'Lavande';

  @override
  String get mainLegacyStageComplete =>
      'L’ancien format cochera toutes les tâches et terminera le projet. Continuer ?';

  @override
  String mainLegacyStageReopen(String task) {
    return 'Changer d’étape décochera la dernière tâche « $task » et toutes les tâches du même nom. Continuer ?';
  }

  @override
  String get mainLegacyTodoContinue => 'Continuer';

  @override
  String mainLegacyTodoGroup(String task) {
    return 'L’ancien format identifie les tâches par leur texte. Toutes les tâches « $task » seront modifiées. Continuer ?';
  }

  @override
  String get mainLegacyTodoTitle => 'Modification d’une ancienne liste';

  @override
  String get mainLightOpacity => '20 % · Léger';

  @override
  String get mainLiquidAllCanvases =>
      'Disponible séparément pour les quatre types de fond';

  @override
  String get mainLiquidDetail =>
      'Reflets fluides et réfraction douce, comme une goutte d’eau suspendue.';

  @override
  String get mainLiquidEffect => 'Effet de verre liquide';

  @override
  String get mainLiquidGlass => 'Verre liquide';

  @override
  String get mainLivePreview => 'Aperçu en direct';

  @override
  String get mainLocalMedia => 'Média local';

  @override
  String get mainMakeYours => 'À VOTRE IMAGE';

  @override
  String get mainMarkOrganized => 'Marquer comme organisé';

  @override
  String get mainMarkdownBody => 'Texte · Markdown';

  @override
  String get mainMediaLimits => 'Images / GIF ≤ 25 Mo ; vidéos ≤ 150 Mo';

  @override
  String get mainMonochrome => 'Monochrome';

  @override
  String mainMoreSteps(int count) {
    return 'Encore $count étapes ; ouvrir pour voir';
  }

  @override
  String mainMovedProject(String title) {
    return '« $title » déplacé vers les projets';
  }

  @override
  String get mainMusic => 'Lecteur audio';

  @override
  String get mainMySpace => 'Mon espace';

  @override
  String get mainNavigation => 'Navigation';

  @override
  String get mainNewIdea => 'Nouvelle idée';

  @override
  String get mainNewIdeaTitle => 'Capturez une nouvelle idée';

  @override
  String get mainNoHypothesis => 'Pas encore d’hypothèse';

  @override
  String get mainNoMatches => 'Aucune idée correspondante';

  @override
  String get mainNoResultYet =>
      'Le résultat peut attendre. La démarche mérite aussi d’être notée.';

  @override
  String get mainNotNow => 'Pas maintenant';

  @override
  String get mainObservationSection => 'Observations / Découvertes';

  @override
  String get mainObservations => 'Observations et conclusions';

  @override
  String get mainObservationsPrompt => 'Observations, démarche et conclusions';

  @override
  String get mainOneHourAgo => 'Il y a 1 heure';

  @override
  String get mainOnlineMedia => 'Média en ligne';

  @override
  String get mainOpaqueFallback =>
      'Panneaux transparents sur la couleur du thème actuel.';

  @override
  String get mainOpenNextStep =>
      'Ouvrez le projet pour modifier les prochaines étapes';

  @override
  String get mainOrganizedCount => 'Organisés';

  @override
  String get mainOriginalColors => 'Couleurs originales';

  @override
  String get mainPageFavorites => 'Favoris';

  @override
  String get mainPageInbox => 'Boîte de réception';

  @override
  String get mainPageLaboratory => 'Labo';

  @override
  String get mainPageOverview => 'Aperçu';

  @override
  String get mainPageProjects => 'Projets';

  @override
  String mainPageSummary(String page) {
    return '$page · Aperçu';
  }

  @override
  String get mainPasteChanged =>
      'La saisie a changé pendant le collage. Rouvrez l’éditeur.';

  @override
  String get mainPasteContent => 'Coller du contenu';

  @override
  String get mainPause => 'Pause';

  @override
  String get mainPersonalWorkspace => 'Espace personnel';

  @override
  String get mainPlay => 'Lire';

  @override
  String get mainPluginSettings => 'Extensions et services';

  @override
  String get mainPluginSettingsSummary =>
      'Outils intégrés, extensions, réseau, fichiers et protection du contenu';

  @override
  String get mainPreviewEmpty => 'L’aperçu apparaîtra ici';

  @override
  String mainProgress(int done, int total) {
    return 'Petits pas · $done/$total';
  }

  @override
  String get mainProjectIntro =>
      'Avancez avec des listes. Chaque petit pas vous rapproche du but.';

  @override
  String get mainQueryAgain => 'Nouvelle recherche';

  @override
  String get mainQueryCapacity => 'Historique des recherches plein';

  @override
  String get mainQueryCapacityDetail =>
      'Votre contenu est conservé. Cette version ne permet pas encore d’effacer l’historique des recherches.';

  @override
  String get mainQueryLoading => 'Recherche d’idées…';

  @override
  String get mainQueryRetry => 'Relancer la recherche';

  @override
  String get mainQueryTerminated => 'Cette recherche est terminée';

  @override
  String get mainQueryUnknown => 'Résultats non encore confirmés';

  @override
  String get mainQuickHint => 'Qu’est-ce qui vous vient à l’esprit ?';

  @override
  String get mainReadOnlySettings =>
      'Contenu en lecture seule. Vérifiez l’extension de l’espace de travail pour rétablir l’édition.';

  @override
  String get mainRecentThoughts => 'Idées récentes';

  @override
  String get mainRecordedCount => 'Avec observations';

  @override
  String get mainRestoreDefault => 'Valeur par défaut';

  @override
  String get mainRetry => 'Réessayer';

  @override
  String get mainRetrySave => 'Réessayer l’enregistrement';

  @override
  String get mainSage => 'Sauge';

  @override
  String get mainSampleBody0 =>
      'Gardez ici les idées spontanées.\nInutile de finir vite : laissez-les naître.';

  @override
  String get mainSampleBody1 =>
      'Une petite page pour les mots aimés,\nla musique et les détails du quotidien.';

  @override
  String get mainSampleBody2 =>
      'Essayez l’art génératif. Laissez le code\nfaire naître des formes inattendues.';

  @override
  String get mainSampleBody3 =>
      'Un compagnon discret pour se souvenir\ndes petites choses qui nous échappent.';

  @override
  String get mainSampleTitle0 => 'Un foyer pour les idées';

  @override
  String get mainSampleTitle1 => 'Un jardin numérique paisible';

  @override
  String get mainSampleTitle2 => 'Créer juste pour le plaisir';

  @override
  String get mainSampleTitle3 => 'Mon petit assistant';

  @override
  String get mainSampleTodo0 => 'Organiser la première collection';

  @override
  String get mainSampleTodo1 => 'Créer l’entrée du jardin';

  @override
  String get mainSampleTodo2 => 'Planter une nouvelle idée';

  @override
  String get mainSampleTodo3 => 'Esquisser un petit prototype';

  @override
  String get mainSampleTodo4 => 'Concevoir les rappels';

  @override
  String get mainSaveConnectionUnknown =>
      'Connexion interrompue ; résultat de l’enregistrement inconnu. Rouvrez la bibliothèque pour vérifier avant de réessayer.';

  @override
  String get mainSaveFailed =>
      'Échec de l’enregistrement. Vos modifications restent dans cette session.';

  @override
  String get mainSaveIdea => 'Enregistrer l’idée';

  @override
  String get mainSaveNotSubmitted =>
      'Non envoyé. Brouillon et pièces jointes conservés ; vous pouvez modifier puis enregistrer à nouveau.';

  @override
  String get mainSaveReadOnly =>
      'Modifications non enregistrées. Activez l’extension de l’espace de travail dans Extensions et services, puis réessayez.';

  @override
  String get mainSaveReadbackPending =>
      'Les paramètres ont été enregistrés, mais leur relecture reste non confirmée. Le brouillon est conservé ; réessayer vérifiera d’abord l’envoi initial.';

  @override
  String get mainSaveRecoveryAbandon => 'Abandonner l’ancienne proposition';

  @override
  String get mainSaveRecoveryAbandonConfirm =>
      'Abandonner uniquement la proposition non enregistrée ? Votre brouillon et les données enregistrées restent intacts. Les paramètres déjà enregistrés ne seront pas annulés.';

  @override
  String get mainSaveRecoveryCommitted =>
      'Les anciens paramètres ont été enregistrés. La vérification confirme le résultat sans remplacer votre brouillon actuel.';

  @override
  String get mainSaveRecoveryConflict =>
      'La bibliothèque a changé. L’ancienne proposition ne peut pas écraser les nouveaux paramètres. Conservez-la en attente ou abandonnez-la si elle n’a pas été enregistrée.';

  @override
  String get mainSaveRecoveryDone =>
      'L’ancienne proposition est traitée. Votre brouillon est inchangé ; réessayez de l’enregistrer quand vous le souhaitez.';

  @override
  String get mainSaveRecoveryEmpty =>
      'Aucune proposition persistante à vérifier.';

  @override
  String get mainSaveRecoveryPending =>
      'L’ancienne proposition n’est pas confirmée. Continuer vérifie et tente l’enregistrement initial ; votre nouveau brouillon est conservé.';

  @override
  String get mainSaveRecoveryResolve => 'Vérifier l’enregistrement initial';

  @override
  String get mainSaveRecoveryReview => 'Vérifier';

  @override
  String get mainSaveRecoveryTitle => 'Un enregistrement doit être vérifié';

  @override
  String get mainSaveUnknown =>
      'Enregistrement non confirmé. Brouillon et pièces jointes conservés. Réessayez cet envoi ; la fermeture actualisera l’espace de travail pour vérifier.';

  @override
  String get mainSaving => 'Enregistrement…';

  @override
  String get mainSearchHint => 'Rechercher vos idées…';

  @override
  String get mainServiceRunSettings => 'Exécution des services';

  @override
  String get mainServiceSettings => 'Services API';

  @override
  String get mainSettings => 'Paramètres';

  @override
  String get mainShowAppearance => 'Afficher les réglages d’apparence';

  @override
  String get mainSidebarMotto => 'Un peu d’ordre. Place à la découverte.';

  @override
  String get mainSlowProgress => 'Même les petits pas font avancer.';

  @override
  String get mainSolidCanvas => 'Uni';

  @override
  String get mainSolidDetail => 'Un fond uni et apaisant.';

  @override
  String get mainSolidOpacity => '100 % · Opaque';

  @override
  String get mainSortFavorites => 'Favoris d’abord';

  @override
  String get mainSortRecent => 'Ajouts récents';

  @override
  String get mainSortTitle => 'Par titre';

  @override
  String get mainSquareCorners => '0 pour des angles droits';

  @override
  String get mainStageActive => 'En cours';

  @override
  String get mainStageCompleted => 'Terminé';

  @override
  String get mainStageOrganized => 'Organisé';

  @override
  String get mainStagePlanned => 'Planifié';

  @override
  String get mainStageRecorded => 'Consigné';

  @override
  String mainStageTooltip(String title) {
    return 'Changer l’étape de $title';
  }

  @override
  String get mainStageUnsorted => 'À organiser';

  @override
  String get mainStageUnverified => 'À tester';

  @override
  String get mainStageVerifying => 'En test';

  @override
  String get mainStayCurious => 'RESTEZ CURIEUX. RESTEZ VOUS-MÊME.';

  @override
  String mainSteps(int done, int total) {
    return 'Étapes : $done/$total';
  }

  @override
  String get mainStorageUnavailable =>
      'Stockage local indisponible. Les modifications ne dureront que pendant cette session.';

  @override
  String get mainStorageUnreadable =>
      'Impossible de lire le contenu enregistré. Les données originales sont conservées et ne seront pas écrasées.';

  @override
  String get mainStyleFlat => 'Plat · Par défaut';

  @override
  String get mainStyleFlatDescription => 'Contours légers et niveaux lisibles';

  @override
  String get mainStyleNeumorphism => 'Neumorphisme';

  @override
  String get mainStyleNeumorphismDescription =>
      'Ombres douces et relief subtil';

  @override
  String get mainTaskAdd => 'Ajouter une tâche';

  @override
  String get mainTaskAmbiguousDecision =>
      'L’état de ces anciennes tâches de même nom est incertain. Confirmez chaque tâche séparément.';

  @override
  String get mainTaskCompleteAllAndSetStage =>
      'Tout terminer et définir l’étape';

  @override
  String mainTaskCompleteAllConfirm(String stage) {
    return 'Marquer toutes les tâches comme terminées et définir l’étape sur « $stage » ?';
  }

  @override
  String get mainTaskLegacyReadOnly =>
      'Les anciennes tâches sont associées par leur texte. Après la mise à niveau, les tâches de même nom peuvent être gérées séparément.';

  @override
  String get mainTaskMarkComplete => 'Confirmer : terminée';

  @override
  String get mainTaskMarkIncomplete => 'Confirmer : non terminée';

  @override
  String get mainTaskMoveDown => 'Descendre';

  @override
  String get mainTaskMoveUp => 'Monter';

  @override
  String mainTaskProgressThreeWay(int ambiguous, int complete, int incomplete) {
    return '$complete terminées · $incomplete restantes · $ambiguous à confirmer';
  }

  @override
  String mainTaskRemoveConfirm(String task) {
    return 'Supprimer la tâche « $task » ?';
  }

  @override
  String get mainTaskRename => 'Renommer';

  @override
  String get mainTaskSetStage => 'Modifier seulement l’étape';

  @override
  String get mainTaskStagePrompt => 'Étape du projet';

  @override
  String get mainTaskTextPrompt => 'Texte de la tâche';

  @override
  String get mainTenMinutesAgo => 'Il y a 10 minutes';

  @override
  String get mainTextureCanvas => 'Texture';

  @override
  String get mainTextureDetail =>
      'Une texture fine comme du papier donne du relief.';

  @override
  String get mainThemeCompass => 'Cercle chromatique du thème';

  @override
  String get mainThemeGrayscale => 'Niveaux de gris du thème';

  @override
  String get mainThemeTone => 'Couleurs du thème';

  @override
  String get mainThreeHoursAgo => 'Il y a 3 heures';

  @override
  String get mainTintOpacity => 'Opacité de la teinte';

  @override
  String get mainToProject => 'Déplacer vers les projets';

  @override
  String get mainTodosPrompt => 'Étapes suivantes (une par ligne, facultatif)';

  @override
  String get mainTransparencyUnavailable =>
      'Impossible d’activer la transparence système. Vous pouvez utiliser le fond par défaut.';

  @override
  String get mainTransparentCanvas => 'Transparent';

  @override
  String get mainTransparentDetail =>
      'Affiche ce qui se trouve derrière la fenêtre ; sur le Web, le fond de la page.';

  @override
  String get mainUndo => 'Annuler';

  @override
  String mainUnfavoriteTooltip(String title) {
    return 'Retirer $title des favoris';
  }

  @override
  String get mainUnsortedCount => 'À organiser';

  @override
  String get mainUnverifiedCount => 'À tester';

  @override
  String get mainView => 'Voir';

  @override
  String get mainViewAll => 'Tout afficher';

  @override
  String get mainVisualStyle => 'Style de l’interface';

  @override
  String get mainWarmSand => 'Sable chaud';

  @override
  String get mainWhiteTheme => 'Clair';

  @override
  String get mainWindowRadius => 'Angles de la fenêtre';

  @override
  String get mainWindowRadiusDetail =>
      'Réglage indépendant du bord ; les angles deviennent droits à l’agrandissement';

  @override
  String get mainWindowsFrostOnly =>
      'Le flou du bureau est disponible uniquement sous Windows';

  @override
  String get mainWorkbench => 'Espace de travail';

  @override
  String get mainWorkbenchPlugin => 'Extension de l’espace de travail';

  @override
  String get mainWriteHypothesis =>
      'Ouvrez la fiche et notez ce que vous souhaitez tester.';

  @override
  String get mainYesterday => 'Hier';

  @override
  String get pluginsApprovalUnknown =>
      'L’activation ou les changements d’autorisations n’ont pas pu être confirmés';

  @override
  String get pluginsApproveEnable => 'Autoriser et activer';

  @override
  String get pluginsApproveWorkbench =>
      'Autoriser la lecture et la modification, puis activer';

  @override
  String get pluginsAttachment => 'Lire les pièces jointes';

  @override
  String get pluginsBackingUp => 'Sauvegarde…';

  @override
  String get pluginsBackupLibrary => 'Sauvegarder la bibliothèque';

  @override
  String get pluginsBackupLibraryType => 'Sauvegarde de bibliothèque';

  @override
  String get pluginsBackupProtection => 'Sauvegarder le fichier de protection';

  @override
  String get pluginsBackupUnknown =>
      'La sauvegarde n’est pas confirmée. Conservez tout fichier créé et vérifiez la destination.';

  @override
  String pluginsBinaryPreview(String hex) {
    return 'Contenu binaire : $hex';
  }

  @override
  String get pluginsBuiltin => 'Espace intégré';

  @override
  String pluginsBuiltinCount(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count caractères',
      one: '$count caractère',
    );
    return '$_temp0 · Cette session uniquement ; non enregistré comme carte';
  }

  @override
  String get pluginsBuiltinEmpty =>
      'Saisissez du texte pour l’aperçu en majuscules';

  @override
  String get pluginsBuiltinHeading => 'Outils de texte';

  @override
  String get pluginsBuiltinInput => 'Saisissez du texte';

  @override
  String get pluginsCancel => 'Annuler';

  @override
  String get pluginsChoosePackage => 'Choisir le fichier d’extension';

  @override
  String get pluginsChooseSmallFile => 'Choisir un petit fichier';

  @override
  String get pluginsCloseTextTool => 'Masquer l’outil de texte';

  @override
  String get pluginsCloseUnknown =>
      'La fermeture de la vue n’a pas pu être confirmée';

  @override
  String get pluginsCloseView => 'Fermer la vue';

  @override
  String get pluginsConnectionLost =>
      'Connexion interrompue. Rouvrez la vue de l’extension.';

  @override
  String get pluginsContentPermissions => 'Autorisations de contenu';

  @override
  String get pluginsCreate => 'Créer du contenu';

  @override
  String get pluginsCredentialCancel => 'Fermer le formulaire';

  @override
  String get pluginsCredentialCreateTitle => 'Nouveaux identifiants';

  @override
  String pluginsCredentialDays(int days) {
    String _temp0 = intl.Intl.pluralLogic(
      days,
      locale: localeName,
      other: '$days jours',
      one: '$days jour',
    );
    return '$_temp0';
  }

  @override
  String get pluginsCredentialDetails =>
      'Stockez les identifiants des connexions API autorisées en sécurité. Leur enregistrement n’autorise aucun serveur et n’active aucune extension. Les secrets enregistrés ne sont pas consultables.';

  @override
  String get pluginsCredentialDisable => 'Désactiver';

  @override
  String get pluginsCredentialDisabled => 'Désactivés';

  @override
  String get pluginsCredentialDisabledDone => 'Identifiants désactivés.';

  @override
  String get pluginsCredentialEmpty => 'Aucun identifiant enregistré';

  @override
  String get pluginsCredentialExpired => 'Expirés';

  @override
  String pluginsCredentialExpires(String date) {
    return 'Expiration : $date';
  }

  @override
  String get pluginsCredentialHeader => 'Nom de l’en-tête';

  @override
  String get pluginsCredentialInvalid =>
      'Vérifiez le nom de l’en-tête et saisissez un nouveau secret. Le champ secret a été effacé.';

  @override
  String get pluginsCredentialLifetime => 'Durée de validité';

  @override
  String get pluginsCredentialLoadFailed =>
      'Impossible de lire les identifiants de façon cohérente. Actualisez l’état pour réessayer.';

  @override
  String get pluginsCredentialNew => 'Ajouter des identifiants';

  @override
  String get pluginsCredentialReading => 'Lecture des identifiants…';

  @override
  String pluginsCredentialReference(String reference) {
    return 'Identifiants $reference';
  }

  @override
  String get pluginsCredentialRefresh => 'Actualiser l’état';

  @override
  String get pluginsCredentialReplace => 'Remplacer le secret';

  @override
  String pluginsCredentialReplaceTitle(String reference) {
    return 'Remplacer les identifiants $reference';
  }

  @override
  String get pluginsCredentialSave => 'Enregistrer les identifiants';

  @override
  String get pluginsCredentialSaved =>
      'Identifiants enregistrés. Les connexions API nécessitent toujours une autorisation distincte.';

  @override
  String get pluginsCredentialSecret => 'Nouvelle valeur secrète';

  @override
  String get pluginsCredentialStored => 'Enregistrés';

  @override
  String get pluginsCredentialTitle => 'Identifiants API';

  @override
  String get pluginsCredentialUnknown =>
      'Le résultat n’est pas confirmé. Le champ secret a été effacé. Actualisez l’état avant toute autre modification.';

  @override
  String pluginsDeclared(String permissions) {
    return 'Autorisations déclarées : $permissions';
  }

  @override
  String get pluginsDependenciesNotice =>
      'Les dépendances doivent être configurées dans l’hôte. Cette page ne les autorise pas.';

  @override
  String get pluginsDisable => 'Désactiver';

  @override
  String get pluginsDisableWorkbench => 'Désactiver l’extension de l’espace';

  @override
  String get pluginsDisabled => 'Désactivée';

  @override
  String get pluginsDisabledDetails =>
      'Désactivée. Autorisez la lecture et la modification du contenu pour utiliser l’éditeur et les outils.';

  @override
  String get pluginsEdit => 'Modifier le contenu';

  @override
  String get pluginsEmptyLibrary => 'Aucune extension tierce importée.';

  @override
  String get pluginsEmptyResult => '(résultat vide)';

  @override
  String get pluginsEnabled => 'Activée';

  @override
  String get pluginsEnabledDetails =>
      'Activée. Cette extension peut lire et modifier le contenu. Sa désactivation préserve vos données.';

  @override
  String get pluginsEndpointAdvanced =>
      'Limites de politique (en octets sauf indication contraire)';

  @override
  String get pluginsEndpointCertificate => 'Choisir une racine DER';

  @override
  String get pluginsEndpointCertificateDetails =>
      'Racine de confiance HTTPS facultative : un certificat DER binaire (.der ou .cer), de 32 Kio maximum. PEM et lots de certificats refusés. Retirez la racine avant de passer à HTTP.';

  @override
  String get pluginsEndpointCertificateInvalid =>
      'Choisissez un certificat DER binaire valide (.der ou .cer) de 32 Kio maximum.';

  @override
  String pluginsEndpointCertificateSelected(int bytes) {
    return 'Racine DER sélectionnée ($bytes octets)';
  }

  @override
  String get pluginsEndpointConcurrency => 'Requêtes simultanées (1–128)';

  @override
  String get pluginsEndpointCreateTitle =>
      'Nouvelle autorisation de point d’accès';

  @override
  String get pluginsEndpointCredential => 'Référence d’identifiants';

  @override
  String get pluginsEndpointCredentialLifetime =>
      'Les identifiants doivent rester valides pendant toute la durée du point d’accès. Leur expiration ne sera pas prolongée.';

  @override
  String get pluginsEndpointCredentialUnavailable =>
      'Les identifiants exigent une permission d’utilisation déclarée et approuvée pour le paquet, ainsi qu’une référence enregistrée valide.';

  @override
  String get pluginsEndpointCredentialsFailed =>
      'Impossible de lire les références d’identifiants. Actualisez l’état avant de choisir.';

  @override
  String get pluginsEndpointDetails =>
      'Enregistrez une politique serveur pour un paquet et une empreinte précis. L’enregistrement ne connecte pas au réseau, n’active pas l’extension et ne rend pas les tâches réseau immédiatement disponibles.';

  @override
  String get pluginsEndpointDigest => 'Empreinte du paquet';

  @override
  String get pluginsEndpointDisabledDone =>
      'Autorisation du point d’accès désactivée.';

  @override
  String get pluginsEndpointEmpty =>
      'Aucune autorisation de point d’accès enregistrée';

  @override
  String get pluginsEndpointFrameBytes => 'Budget de trame (1–131072 octets)';

  @override
  String get pluginsEndpointHeaderBytes =>
      'Taille maximale des en-têtes (1–16384 octets)';

  @override
  String get pluginsEndpointInvalid =>
      'Vérifiez le paquet, l’origine, les méthodes, la validité de 1 à 30 jours, l’autorisation des identifiants, le certificat et les limites.';

  @override
  String get pluginsEndpointLifetime => 'Validité (1 à 30 jours)';

  @override
  String get pluginsEndpointLoadFailed =>
      'Impossible de lire les autorisations de façon cohérente. Actualisez l’état pour réessayer.';

  @override
  String get pluginsEndpointLocalHttp => 'HTTP local';

  @override
  String get pluginsEndpointLocalHttps => 'HTTPS local';

  @override
  String get pluginsEndpointMethods => 'Méthodes de requête autorisées';

  @override
  String get pluginsEndpointNew => 'Ajouter un point d’accès';

  @override
  String get pluginsEndpointNoCredential => 'Sans identifiants';

  @override
  String get pluginsEndpointOrigin =>
      'Origine uniquement, par exemple https://api.example.com';

  @override
  String get pluginsEndpointPackage => 'Paquet';

  @override
  String get pluginsEndpointPackageUnavailable =>
      'Ce paquet est indisponible ou sans autorisation HTTP. Les autorisations existantes peuvent toujours être désactivées.';

  @override
  String get pluginsEndpointProfile => 'Profil de connexion';

  @override
  String get pluginsEndpointPublicHttps => 'HTTPS public';

  @override
  String get pluginsEndpointRemoveCertificate =>
      'Retirer la racine de confiance';

  @override
  String get pluginsEndpointReplace => 'Remplacer l’autorisation';

  @override
  String get pluginsEndpointReplaceTitle =>
      'Remplacer l’autorisation avec l’empreinte actuelle du paquet';

  @override
  String get pluginsEndpointRequestBytes =>
      'Taille maximale de requête (1–65536 octets)';

  @override
  String get pluginsEndpointResponseBytes =>
      'Taille maximale de réponse (1–65536 octets)';

  @override
  String get pluginsEndpointSave =>
      'Enregistrer l’autorisation du point d’accès';

  @override
  String get pluginsEndpointSaved =>
      'Autorisation enregistrée. Aucune connexion réseau n’a été établie.';

  @override
  String get pluginsEndpointTimeout => 'Délai (1–30000 millisecondes)';

  @override
  String get pluginsEndpointTitle => 'Autorisations des points d’accès API';

  @override
  String get pluginsEndpointUnknown =>
      'Le résultat n’est pas confirmé. Actualisez l’état avant toute modification. La requête ne sera pas renvoyée automatiquement.';

  @override
  String get pluginsEndpointWorking =>
      'Actualisation de l’état du point d’accès…';

  @override
  String get pluginsExistingVersion =>
      'Cette version est déjà installée. Son état d’activation est inchangé.';

  @override
  String pluginsFileLimit(int limit) {
    return 'Fichier trop volumineux. Choisissez un fichier de $limit octets maximum.';
  }

  @override
  String get pluginsHttpTaskAbandon =>
      'Terminer l’observation de cette tentative';

  @override
  String get pluginsHttpTaskAbandonDetails =>
      'Vous pouvez terminer l’observation seulement si un état récent confirme l’absence de tâche active et la disponibilité de la bibliothèque d’origine. Cela ne prouve pas l’absence d’effets distants. Identité et incertitude restent dans l’historique ; une nouvelle requête exige une soumission explicite.';

  @override
  String get pluginsHttpTaskAbsent => 'Aucun résultat livré';

  @override
  String get pluginsHttpTaskAccepted => 'Accepté';

  @override
  String get pluginsHttpTaskAcknowledge => 'Acquitter la tâche terminée';

  @override
  String get pluginsHttpTaskArchivedUnknown =>
      'Observation terminée par l’utilisateur. Les effets distants précédents restent incertains ; cette tentative n’a pas été rejouée.';

  @override
  String get pluginsHttpTaskBase64 => 'Base64';

  @override
  String get pluginsHttpTaskBody => 'Corps de requête';

  @override
  String get pluginsHttpTaskBodyFormat => 'Encodage du corps de requête';

  @override
  String get pluginsHttpTaskBusy => 'Occupé';

  @override
  String get pluginsHttpTaskCancel => 'Demander l’annulation';

  @override
  String get pluginsHttpTaskCancelled =>
      'Annulation constatée ; des effets distants ont pu se produire';

  @override
  String get pluginsHttpTaskCatalogUnavailable =>
      'Catalogue des extensions indisponible. Actualisez-le avant une nouvelle soumission ; les commandes existantes restent disponibles.';

  @override
  String get pluginsHttpTaskClosed => 'Fermé';

  @override
  String get pluginsHttpTaskCompleted => 'Terminé';

  @override
  String get pluginsHttpTaskConflict => 'Conflit';

  @override
  String get pluginsHttpTaskConsumed => 'Résultat consommé';

  @override
  String get pluginsHttpTaskControlUnknown =>
      'Le résultat de commande n’est pas confirmé. Actualisez l’état avant de décider de la suite.';

  @override
  String pluginsHttpTaskCounters(String bytes, String calls) {
    return 'Appels E/S : $calls ; octets comptabilisés : $bytes';
  }

  @override
  String get pluginsHttpTaskDeadline => 'Échéance dépassée';

  @override
  String get pluginsHttpTaskDenied => 'Refusé';

  @override
  String get pluginsHttpTaskDetails =>
      'Exécutez une requête explicite via un point d’accès autorisé et une extension activée avec le gestionnaire HTTP expérimental. L’état reste accessible quand la bibliothèque est occupée.';

  @override
  String get pluginsHttpTaskDisconnect => 'Échec du nettoyage de connexion';

  @override
  String get pluginsHttpTaskEndpoint => 'Point d’accès autorisé';

  @override
  String get pluginsHttpTaskEndpointsFailed =>
      'Lecture cohérente des points d’accès impossible, ou bibliothèque occupée. Les commandes restent disponibles. Actualisez les points d’accès au retour de la bibliothèque.';

  @override
  String get pluginsHttpTaskEvidenceUnavailable =>
      'Preuves du résultat indisponibles';

  @override
  String pluginsHttpTaskExecution(int code, String fault) {
    return 'Exécution invitée : $fault ; code de sortie : $code';
  }

  @override
  String pluginsHttpTaskExit(
    String disconnect,
    String execution,
    String maintenance,
  ) {
    return 'Sortie du worker — exécution : $execution ; déconnexion : $disconnect ; maintenance : $maintenance';
  }

  @override
  String get pluginsHttpTaskExplicit =>
      'Soumettre envoie une requête réelle. Chaque clic crée une nouvelle identité. Les soumissions et lectures incertaines ne sont jamais rejouées automatiquement. L’annulation ne prouve pas que l’opération distante a été annulée.';

  @override
  String get pluginsHttpTaskFailed => 'Échec';

  @override
  String get pluginsHttpTaskHeaders =>
      'En-têtes ordinaires, un Nom: valeur par ligne';

  @override
  String get pluginsHttpTaskHeadersHint =>
      'Les en-têtes répétés restent distincts. Seul le runtime fournit les en-têtes d’identifiants et de connexion.';

  @override
  String get pluginsHttpTaskHistory => 'Observations précédentes (5 maximum)';

  @override
  String pluginsHttpTaskHttpResult(int code, String status) {
    return 'Résultat HTTP : $status ; statut distant : $code';
  }

  @override
  String get pluginsHttpTaskInactive => 'Connexion inactive';

  @override
  String get pluginsHttpTaskInvalid =>
      'Vérifiez le point d’accès, la méthode, la cible relative, les en-têtes ordinaires, l’encodage du corps et le délai selon les limites autorisées.';

  @override
  String get pluginsHttpTaskInvalidOptions => 'Options invalides';

  @override
  String pluginsHttpTaskKey(String identity) {
    return 'Identité de la tâche : $identity';
  }

  @override
  String get pluginsHttpTaskLimit => 'Quota ou limite atteint';

  @override
  String get pluginsHttpTaskLoadingEndpoints =>
      'Lecture des points d’accès autorisés…';

  @override
  String get pluginsHttpTaskLocal =>
      'Bibliothèque disponible ; aucune tâche active';

  @override
  String get pluginsHttpTaskModule => 'Module invité invalide';

  @override
  String get pluginsHttpTaskNew => 'Préparer une nouvelle requête';

  @override
  String get pluginsHttpTaskNoEndpoints =>
      'Aucun point d’accès ne correspond à une extension de transfert HTTP activée et autorisée.';

  @override
  String get pluginsHttpTaskNotFound => 'Introuvable';

  @override
  String get pluginsHttpTaskOk => 'OK';

  @override
  String get pluginsHttpTaskOutcomeUnknown =>
      'Résultat distant inconnu ; ne supposez pas un retour arrière et ne renvoyez pas la requête';

  @override
  String get pluginsHttpTaskPackageChanged => 'Liaison du paquet modifiée';

  @override
  String get pluginsHttpTaskPending => 'Résultat en attente';

  @override
  String get pluginsHttpTaskPoll => 'Vérifier la tâche';

  @override
  String get pluginsHttpTaskProtocol => 'Erreur de protocole de tâche';

  @override
  String get pluginsHttpTaskRead => 'Lire le résultat une fois';

  @override
  String get pluginsHttpTaskReadBound => 'Limite de lecture dépassée';

  @override
  String get pluginsHttpTaskReadPending =>
      'Aucun résultat reçu. Vérifiez l’état avant de relancer explicitement la lecture.';

  @override
  String get pluginsHttpTaskReadUnknown =>
      'La lecture n’est pas confirmée et peut avoir déjà consommé le résultat. Elle ne sera pas répétée. L’état et la sortie restent consultables.';

  @override
  String get pluginsHttpTaskReady =>
      'Résultat prêt : lancez la lecture. Cela ne signifie pas que le worker est terminé.';

  @override
  String get pluginsHttpTaskReclaimed =>
      'Worker terminé ; bibliothèque d’origine rendue';

  @override
  String get pluginsHttpTaskRecoveryRequired =>
      'Worker terminé ; nettoyage ou maintenance à réparer';

  @override
  String get pluginsHttpTaskRefresh => 'Actualiser l’état de la tâche';

  @override
  String get pluginsHttpTaskRefreshEndpoints =>
      'Actualiser les points d’accès autorisés';

  @override
  String get pluginsHttpTaskRemoteError =>
      'Le serveur distant a renvoyé 4xx/5xx. L’échange HTTP est terminé ; ceci est distinct des erreurs d’exécution invitée.';

  @override
  String get pluginsHttpTaskRepair => 'Réparer le nettoyage';

  @override
  String get pluginsHttpTaskResponseBase64 => 'Corps de réponse : Base64 exact';

  @override
  String get pluginsHttpTaskResponseHeaders =>
      'En-têtes de réponse (doublons préservés ; valeurs binaires en Base64)';

  @override
  String get pluginsHttpTaskResponseText =>
      'Corps de réponse : aperçu en texte brut';

  @override
  String get pluginsHttpTaskResultUnavailable =>
      'Livraison du résultat indisponible';

  @override
  String get pluginsHttpTaskRevoked => 'Autorisation révoquée';

  @override
  String get pluginsHttpTaskRunning =>
      'En cours ; bibliothèque détenue par le worker';

  @override
  String get pluginsHttpTaskSpawn => 'Impossible de démarrer le worker';

  @override
  String get pluginsHttpTaskStart => 'Soumettre une nouvelle requête';

  @override
  String get pluginsHttpTaskStartUnknown =>
      'Résultat de soumission inconnu. Son identité est conservée. Consultez l’état pour retrouver la même tâche ; la requête ne sera pas renvoyée.';

  @override
  String get pluginsHttpTaskStatusFailed =>
      'L’état de la tâche n’est pas confirmé. Actualisez-le ; aucune requête n’a été rejouée.';

  @override
  String get pluginsHttpTaskStopping =>
      'Arrêt en cours ; attente de la sortie réelle du worker';

  @override
  String pluginsHttpTaskSubmission(String identity) {
    return 'Identité de soumission : $identity';
  }

  @override
  String get pluginsHttpTaskTarget =>
      'Cible relative, par exemple /v1/items?limit=10';

  @override
  String get pluginsHttpTaskText => 'Texte UTF-8';

  @override
  String get pluginsHttpTaskTimeout =>
      'Délai en millisecondes (1–30000, selon l’autorisation)';

  @override
  String get pluginsHttpTaskTitle => 'Tâches HTTP';

  @override
  String get pluginsHttpTaskTrap => 'Exécution invitée interrompue par un trap';

  @override
  String get pluginsHttpTaskUnavailable =>
      'Bibliothèque d’origine indisponible ; récupération à traiter';

  @override
  String get pluginsHttpTaskUnsupported => 'Opération non prise en charge';

  @override
  String get pluginsHttpTaskWorking => 'Attente de la réponse de commande…';

  @override
  String get pluginsImport => 'Importer';

  @override
  String get pluginsImportDetails =>
      'Après l’importation, vous décidez d’activer l’extension. La désactivation ou la désinstallation préserve le contenu.';

  @override
  String pluginsImportPreview(String name) {
    return 'Aperçu de l’importation : $name';
  }

  @override
  String get pluginsImportUnknown => 'L’importation n’a pas pu être confirmée';

  @override
  String get pluginsImportedDisabled =>
      'Importée et désactivée. Choisissez les autorisations à accorder.';

  @override
  String get pluginsInputFailed => 'Impossible de lire le fichier d’entrée';

  @override
  String get pluginsInputTooLong =>
      'La limite de saisie est atteinte. Raccourcissez le texte et réessayez.';

  @override
  String get pluginsInspectFailed =>
      'Impossible de charger l’aperçu de l’extension';

  @override
  String get pluginsInspectedOnly =>
      'Le fichier a seulement été inspecté. Activez l’extension séparément après l’importation.';

  @override
  String get pluginsInsufficientApproval =>
      'L’extension est activée, mais nécessite l’accès au contenu. L’espace reste en lecture seule. Désactivez-la pour revoir les autorisations.';

  @override
  String pluginsIoApproved(String permissions) {
    return 'Autorisations accordées : $permissions';
  }

  @override
  String get pluginsIoCredentialUse => 'Utiliser les identifiants autorisés';

  @override
  String pluginsIoDeclared(String permissions) {
    return 'Autorisations réseau et fichiers demandées : $permissions';
  }

  @override
  String get pluginsIoFileCreate => 'Créer des fichiers';

  @override
  String get pluginsIoFileDelete => 'Supprimer des fichiers';

  @override
  String get pluginsIoFileList => 'Parcourir les dossiers autorisés';

  @override
  String get pluginsIoFileRead => 'Lire les fichiers autorisés';

  @override
  String get pluginsIoFileReplace => 'Remplacer des fichiers';

  @override
  String get pluginsIoHttpListen => 'Écouter les connexions réseau';

  @override
  String get pluginsIoHttpPublish => 'Fournir un service API';

  @override
  String get pluginsIoHttpRequest => 'Appeler des API réseau';

  @override
  String get pluginsIoNoneApproved =>
      'Aucune autorisation réseau ou fichiers accordée';

  @override
  String get pluginsIoRevoke =>
      'Révoquer toutes les autorisations réseau et fichiers';

  @override
  String get pluginsIoSave =>
      'Enregistrer les autorisations réseau et fichiers';

  @override
  String get pluginsIoScopeNotice =>
      'Seules les catégories d’autorisations sont enregistrées ici. Adresses serveur, accès aux fichiers et identifiants exigent une autorisation distincte ; les fonctions indisponibles le restent. Rouvrez le formulaire après modification.';

  @override
  String get pluginsIoTitle => 'Autorisations réseau et fichiers';

  @override
  String get pluginsIoWebSocketConnect => 'Se connecter aux services WebSocket';

  @override
  String get pluginsListUnknown =>
      'La liste des extensions n’a pas pu être confirmée';

  @override
  String get pluginsManageAbove =>
      'Gérez cette extension avec les commandes de l’espace de travail ci-dessus.';

  @override
  String get pluginsManagementUnavailable =>
      'La gestion des extensions est indisponible. Le contenu existant reste lisible.';

  @override
  String get pluginsNoPermissions => 'Aucune autorisation de contenu déclarée.';

  @override
  String get pluginsOpenTextTool => 'Ouvrir l’outil de texte';

  @override
  String get pluginsOpenView => 'Ouvrir la vue';

  @override
  String get pluginsOpeningView => 'Ouverture de la vue de l’extension…';

  @override
  String get pluginsOperation => 'Consulter les résultats des opérations';

  @override
  String pluginsOtherCapability(String name) {
    return 'Autre autorisation déclarée : $name';
  }

  @override
  String get pluginsPackageFile => 'Extension Morrow';

  @override
  String get pluginsPreviewOnly =>
      'Aperçu uniquement. Les résultats ne sont pas enregistrés automatiquement dans le contenu existant.';

  @override
  String get pluginsPreviewTruncated =>
      '…seuls les 4 096 premiers caractères sont affichés';

  @override
  String get pluginsProtection => 'Protection du contenu';

  @override
  String get pluginsProtectionDetails =>
      'Sauvegardez le fichier de protection d’origine pour récupérer avec ce compte système. Il ne contient ni cartes ni pièces jointes.';

  @override
  String get pluginsProtectionFileType =>
      'Fichier de protection de bibliothèque';

  @override
  String get pluginsProtectionSaved =>
      'Fichier de protection sauvegardé. Sélectionnez-le pour récupérer les données si le démarrage échoue.';

  @override
  String get pluginsRead => 'Lire le contenu';

  @override
  String get pluginsReadingState => 'Chargement de l’état de l’extension…';

  @override
  String pluginsRefreshFailed(String reason) {
    return '$reason. Impossible d’actualiser la liste. Choisissez « Actualiser la liste » pour réessayer la lecture.';
  }

  @override
  String get pluginsRefreshList => 'Actualiser la liste';

  @override
  String get pluginsRefreshState => 'Actualiser l’état';

  @override
  String get pluginsRename => 'Renommer';

  @override
  String pluginsResultBytes(int count, String preview) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count octets',
      one: '$count octet',
    );
    return '$_temp0\n$preview';
  }

  @override
  String get pluginsSavePermissions => 'Enregistrer les autorisations';

  @override
  String pluginsSelectedFile(String name) {
    return 'Fichier sélectionné : $name';
  }

  @override
  String get pluginsServiceAcknowledgeUncertain =>
      'J’ai vérifié les enregistrements actualisés';

  @override
  String get pluginsServiceAddScope => 'Ajouter un périmètre de contenu';

  @override
  String get pluginsServiceAttachmentId => 'Identité exacte de la pièce jointe';

  @override
  String get pluginsServiceAuthenticationUnavailable =>
      'Cette authentification manque, est désactivée, expirée ou appartient à une autre identité. Ses périmètres provisoires sont conservés ; choisissez un remplacement valide ou supprimez-la explicitement.';

  @override
  String get pluginsServiceAuthorities =>
      'Enregistrements d’authentification et de publication';

  @override
  String get pluginsServiceCardId => 'Identité exacte de la carte';

  @override
  String get pluginsServiceCatalogChanged =>
      'Le catalogue a changé ou est indisponible. Votre brouillon est conservé. Actualisez explicitement la sélection avant d’enregistrer.';

  @override
  String get pluginsServiceClearToken => 'Effacer le jeton';

  @override
  String get pluginsServiceCloseEditor => 'Fermer l’éditeur';

  @override
  String get pluginsServiceConfigDigest => 'Empreinte de configuration';

  @override
  String get pluginsServiceConfiguration => 'Configuration enregistrée';

  @override
  String get pluginsServiceConfigurations => 'Configurations enregistrées';

  @override
  String get pluginsServiceCopyClear => 'Copier et effacer le jeton';

  @override
  String get pluginsServiceCreated => 'Création (UTC)';

  @override
  String get pluginsServiceDays => 'Durée demandée (1 à 30 jours)';

  @override
  String get pluginsServiceDigestFixed =>
      'La modification conserve l’empreinte d’origine. Sélectionnez un paquet correspondant ; cela ne l’active pas.';

  @override
  String get pluginsServiceDisable => 'Désactiver';

  @override
  String get pluginsServiceDisabled => 'Désactivé';

  @override
  String get pluginsServiceEditConfig => 'Modifier la configuration';

  @override
  String get pluginsServiceEditPublication => 'Modifier la publication';

  @override
  String get pluginsServiceExpired => 'Expiré ou pas encore valide';

  @override
  String get pluginsServiceExpires => 'Expiration réelle (UTC)';

  @override
  String get pluginsServiceHandler => 'Gestionnaire de service déclaré';

  @override
  String get pluginsServiceIdentity => 'Identité du service';

  @override
  String get pluginsServiceInvalid =>
      'Vérifiez les champs, autorisations choisies et paquet actuel avant d’enregistrer.';

  @override
  String get pluginsServiceIssue => 'Émettre un jeton';

  @override
  String get pluginsServiceIssuedToken => 'Jeton Bearer affiché une seule fois';

  @override
  String get pluginsServiceListenAddress =>
      'Adresse numérique et port d’écoute';

  @override
  String get pluginsServiceLoadFailed =>
      'Impossible d’actualiser les enregistrements. Réessayez avant de modifier.';

  @override
  String get pluginsServiceManagementOnly =>
      'Gérez ici configurations et autorisations enregistrées. Enregistrer ne démarre ni écouteur, ni paquet, ni service.';

  @override
  String get pluginsServiceMethod => 'Méthode HTTP';

  @override
  String get pluginsServiceNewAuthentication => 'Nouvelle authentification';

  @override
  String get pluginsServiceNewConfig => 'Nouvelle configuration';

  @override
  String get pluginsServiceNo => 'Non';

  @override
  String get pluginsServiceNoAuthentication =>
      'Créez d’abord une authentification actuellement valide.';

  @override
  String get pluginsServiceNoAuthorities =>
      'Aucun enregistrement d’authentification ou de publication.';

  @override
  String get pluginsServiceNoConfigurations =>
      'Aucune configuration de service.';

  @override
  String get pluginsServicePackage => 'Paquet déclaré et autorisé';

  @override
  String get pluginsServicePackageDigest => 'Empreinte du paquet';

  @override
  String get pluginsServicePackageUnavailable =>
      'Le paquet correspondant ou ses autorisations d’écoute/publication sont indisponibles. Les anciens enregistrements restent lisibles et désactivables.';

  @override
  String get pluginsServicePath => 'Chemin exact de requête';

  @override
  String get pluginsServicePolicyChanged =>
      'L’enregistrement d’origine a changé ou n’est plus utilisable. Actualisez la sélection ou rouvrez l’éditeur depuis l’enregistrement actuel. Votre brouillon est conservé.';

  @override
  String get pluginsServicePrincipalId => 'Identité du principal';

  @override
  String get pluginsServicePrincipals =>
      'Identités autorisées et périmètres de contenu';

  @override
  String get pluginsServicePublicationEditor => 'Autorisation de publication';

  @override
  String get pluginsServicePublicationHelp =>
      'L’autorisation est liée à cette configuration, révision et référence exactes. Son expiration est limitée par toutes les authentifications choisies et peut être plus courte que demandé. Enregistrer ne démarre pas l’écoute.';

  @override
  String get pluginsServicePublicationMismatch =>
      'Cette publication ne correspond plus à la configuration actuelle. Vérifiez et enregistrez explicitement une autorisation de remplacement.';

  @override
  String get pluginsServiceQueryPath =>
      'Chemin distinct de consultation du résultat (facultatif)';

  @override
  String get pluginsServiceReference => 'Référence d’autorisation';

  @override
  String get pluginsServiceRefresh => 'Actualiser les enregistrements';

  @override
  String get pluginsServiceRefreshSelection => 'Actualiser cette sélection';

  @override
  String get pluginsServiceRemovePrincipal => 'Retirer l’identité';

  @override
  String get pluginsServiceRemoveScope => 'Retirer le périmètre';

  @override
  String get pluginsServiceRetention =>
      'Conservation de l’historique (millisecondes, 30 jours maximum)';

  @override
  String get pluginsServiceRevision => 'Révision';

  @override
  String get pluginsServiceRotate => 'Renouveler le jeton';

  @override
  String get pluginsServiceRotateAuthentication =>
      'Renouveler l’authentification';

  @override
  String get pluginsServiceRunAbandon =>
      'Conserver la trace et terminer cette tentative';

  @override
  String get pluginsServiceRunAdvanced => 'Limites des requêtes et du worker';

  @override
  String get pluginsServiceRunAttempt => 'Tentative de démarrage non résolue';

  @override
  String get pluginsServiceRunBoundsHint =>
      'Ces limites doivent aussi respecter la déclaration de l’extension et les autorisations enregistrées. Le travail réservé consomme le budget cumulé même en cas d’annulation. L’expiration arrête l’exécution ; aucun renouvellement automatique.';

  @override
  String get pluginsServiceRunBytes =>
      'Budget d’exécution (octets, jusqu’à 67 108 864)';

  @override
  String get pluginsServiceRunCalls => 'Appels par tâche (jusqu’à 1 024)';

  @override
  String get pluginsServiceRunCancelled => 'Annulé';

  @override
  String get pluginsServiceRunClosed => 'Fermé';

  @override
  String get pluginsServiceRunConcurrent => 'Tâches simultanées (jusqu’à 128)';

  @override
  String get pluginsServiceRunControlUnknown =>
      'Résultat de commande inconnu. Actualisez l’état du service d’origine avant une autre opération.';

  @override
  String get pluginsServiceRunDenied => 'Refusé';

  @override
  String get pluginsServiceRunExited => 'Service terminé';

  @override
  String get pluginsServiceRunHeaderBytes =>
      'Taille maximale des en-têtes (octets, jusqu’à 65 536)';

  @override
  String get pluginsServiceRunHint =>
      'Choisissez une publication autorisée et des limites finies, puis démarrez explicitement le service. Après l’arrêt, attendez le retour du propriétaire initial de l’espace avant d’acquitter le résultat.';

  @override
  String pluginsServiceRunHostFailure(String detail) {
    return 'Diagnostic de l’hôte : $detail';
  }

  @override
  String get pluginsServiceRunHttpPanel =>
      'Un service API détient cette tâche. Utilisez le panneau ci-dessus pour l’arrêter ou acquitter sa sortie. Votre brouillon HTTP est conservé.';

  @override
  String get pluginsServiceRunIdentityChanged =>
      'Une autre tâche détient le contenu. Ce panneau ne la contrôlera pas avec l’ancienne identité de service.';

  @override
  String get pluginsServiceRunInvalid =>
      'Vérifiez le service et les limites numériques. Aucune nouvelle exécution soumise.';

  @override
  String get pluginsServiceRunInvalidOutcome => 'Configuration invalide';

  @override
  String get pluginsServiceRunJobBytes =>
      'Octets par tâche (jusqu’à 16 777 216)';

  @override
  String get pluginsServiceRunJobs =>
      'Réservations totales de tâches (jusqu’à 1 000 000)';

  @override
  String get pluginsServiceRunLastObservation =>
      'Dernière observation affichée ; état actuel non vérifié.';

  @override
  String get pluginsServiceRunLifetime => 'Durée (ms, jusqu’à 3 600 000)';

  @override
  String get pluginsServiceRunLimit => 'Limite atteinte';

  @override
  String get pluginsServiceRunLocal => 'Contenu disponible localement';

  @override
  String pluginsServiceRunNetwork(
    String bind,
    String listener,
    String supervision,
  ) {
    return 'Liaison : $bind ; écouteur : $listener ; supervision : $supervision';
  }

  @override
  String get pluginsServiceRunNextSettings =>
      'Réglages de la prochaine exécution explicite';

  @override
  String get pluginsServiceRunNoSelection =>
      'Aucune publication autorisée disponible. Vérifiez l’extension, la configuration et l’authentification.';

  @override
  String get pluginsServiceRunOutboundAttempt =>
      'Points d’accès liés à cette tentative';

  @override
  String get pluginsServiceRunOutboundClear =>
      'Effacer la sélection des points d’accès';

  @override
  String get pluginsServiceRunOutboundFailed =>
      'Impossible de vérifier la liste des points d’accès. Actualisez avant d’utiliser la sélection.';

  @override
  String get pluginsServiceRunOutboundHint =>
      'API sortantes (facultatif, 8 maximum). Seuls les points autorisés pour ce paquet sont affichés. Sans sélection, les appels sortants sont bloqués.';

  @override
  String get pluginsServiceRunOutboundStale =>
      'Un point sélectionné a changé ou n’est plus disponible. Choisissez explicitement sa version actuelle ou effacez la sélection.';

  @override
  String get pluginsServiceRunOwned => 'Contenu géré par le service actif';

  @override
  String get pluginsServiceRunPending => 'En attente';

  @override
  String get pluginsServiceRunReclaimed =>
      'Propriété du contenu récupérée ; acquittement requis';

  @override
  String get pluginsServiceRunReclaiming =>
      'Attente de la restitution du contenu';

  @override
  String get pluginsServiceRunRecovery => 'Nettoyage à réparer';

  @override
  String get pluginsServiceRunRequestBytes =>
      'Taille maximale de requête (octets)';

  @override
  String get pluginsServiceRunResponseBytes =>
      'Taille maximale de réponse (octets)';

  @override
  String get pluginsServiceRunRunning => 'Service en cours';

  @override
  String get pluginsServiceRunSelection => 'Publication de service autorisée';

  @override
  String get pluginsServiceRunStale =>
      'Le paquet, la configuration ou l’autorisation a changé. Actualisez les enregistrements et sélectionnez à nouveau avant de démarrer.';

  @override
  String get pluginsServiceRunStart => 'Démarrer le service à durée limitée';

  @override
  String get pluginsServiceRunStartRejected =>
      'La réponse de démarrage signale une erreur. La tâche actuelle a été vérifiée ; examinez la cause et l’état du nettoyage avant de continuer.';

  @override
  String get pluginsServiceRunStartUnknown =>
      'Résultat du démarrage inconnu. L’identité de la tentative est conservée ; actualisez pour la retrouver. Aucun redémarrage automatique.';

  @override
  String get pluginsServiceRunStarting => 'Démarrage du service';

  @override
  String get pluginsServiceRunStatusFailed =>
      'Impossible de vérifier l’état du service. Actualisez avant toute autre action.';

  @override
  String get pluginsServiceRunStop => 'Arrêter le service';

  @override
  String get pluginsServiceRunStopping =>
      'Arrêt ; attente de la sortie de l’écouteur et du worker';

  @override
  String get pluginsServiceRunSucceeded => 'Réussi';

  @override
  String get pluginsServiceRunTask => 'Identité de la tâche actuelle';

  @override
  String get pluginsServiceRunTimeout => 'Délai par tâche (ms, jusqu’à 30 000)';

  @override
  String get pluginsServiceRunTimeoutOutcome => 'Délai dépassé';

  @override
  String get pluginsServiceRunTitle => 'Exécuter le service API';

  @override
  String get pluginsServiceRunTotalBytes =>
      'Budget du worker (octets, jusqu’à 67 108 864)';

  @override
  String get pluginsServiceRunTransport => 'Échec de transport';

  @override
  String get pluginsServiceRunUnavailable => 'Stockage du contenu indisponible';

  @override
  String get pluginsServiceSaveConfig => 'Enregistrer la configuration';

  @override
  String get pluginsServiceSavePublication =>
      'Enregistrer l’autorisation de publication';

  @override
  String get pluginsServiceSaved =>
      'Enregistré. Vérifiez ci-dessous la révision retournée et l’expiration réelle.';

  @override
  String get pluginsServiceScopeAttachment => 'Lire la pièce jointe';

  @override
  String get pluginsServiceScopeCreate => 'Créer du contenu';

  @override
  String get pluginsServiceScopeEdit => 'Modifier le contenu';

  @override
  String get pluginsServiceScopeKind => 'Opération de contenu autorisée';

  @override
  String get pluginsServiceScopeQuery => 'Consulter l’opération';

  @override
  String get pluginsServiceScopeRead => 'Lire le contenu';

  @override
  String get pluginsServiceScopeRename => 'Renommer la carte';

  @override
  String get pluginsServiceScopeSummary => 'Lire le résumé';

  @override
  String get pluginsServiceScopesHelp =>
      'Sélectionnez explicitement l’authentification. Ajoutez chaque opération permise et l’identité exacte de l’objet ci-dessous. La suppression d’un périmètre ou d’une identité exige son bouton dédié ; les périmètres existants sont conservés pendant l’édition.';

  @override
  String get pluginsServiceTitle => 'Configuration du service';

  @override
  String get pluginsServiceTls => 'Exiger TLS';

  @override
  String get pluginsServiceTlsAttempt =>
      'Empreinte du certificat PEM liée à cette tentative';

  @override
  String get pluginsServiceTlsCertificate => 'Choisir la chaîne de certificats';

  @override
  String get pluginsServiceTlsChecked =>
      'Certificat et correspondance de la clé vérifiés. Le SHA-256 du PEM est indiqué ci-dessous. Les clients doivent encore vérifier le nom d’hôte, la validité et la chaîne de confiance.';

  @override
  String get pluginsServiceTlsChecking =>
      'Traitement de la sélection du certificat…';

  @override
  String get pluginsServiceTlsFailed =>
      'Échec de vérification du certificat. Vérifiez les fichiers PEM, la correspondance de la clé et les chemins locaux avant de réessayer.';

  @override
  String get pluginsServiceTlsHelp =>
      'Les adresses hors boucle locale exigent TLS. Seule cette exigence est enregistrée ; aucun écouteur ni identité TLS n’est créé ici.';

  @override
  String get pluginsServiceTlsHint =>
      'Sélectionnez une chaîne PEM et une clé privée, puis vérifiez-les. Les fichiers sont revérifiés au démarrage ; le certificat actif n’est pas renouvelé automatiquement.';

  @override
  String get pluginsServiceTlsInspect => 'Vérifier le certificat';

  @override
  String get pluginsServiceTlsOutsideValidity =>
      'La chaîne n’est pas encore valide ou a expiré. Vérifiez ou remplacez le certificat, puis contrôlez-le à nouveau avant de démarrer.';

  @override
  String get pluginsServiceTlsPrivateKey => 'Choisir la clé privée';

  @override
  String get pluginsServiceTlsRecheck =>
      'Vérifiez à nouveau le certificat avant de démarrer. Un changement d’horloge ne rétablit pas la sélection précédente.';

  @override
  String get pluginsServiceTlsUnavailable =>
      'Ce backend ne permet pas la sélection d’un certificat TLS local.';

  @override
  String pluginsServiceTlsValidity(String end, String start) {
    return 'Validité commune de la chaîne (UTC) : de $start à $end. Le service s’arrête à l’expiration.';
  }

  @override
  String get pluginsServiceTokenDiscarded =>
      'Le jeton à usage unique a été effacé à la fermeture du panneau. Émettez-en un nouveau explicitement si nécessaire.';

  @override
  String get pluginsServiceTokenHelp =>
      'Ce jeton n’est affiché que maintenant. Copiez-le explicitement si nécessaire. Effacer ou fermer le panneau le retire de la session ; la liste ne permet pas de le récupérer. Le renouvellement remplace le jeton précédent.';

  @override
  String get pluginsServiceUncertainHelp =>
      'Actualisez et examinez d’abord les enregistrements d’origine. Acquitter cet avis permet seulement une autre action explicite ; cela ne prouve pas l’échec du changement précédent et ne le rejoue pas.';

  @override
  String get pluginsServiceUnsupported => 'Ancienne valeur non prise en charge';

  @override
  String get pluginsServiceWorking => 'Traitement…';

  @override
  String get pluginsServiceWriteUnknown =>
      'Le résultat de la dernière modification est inconnu. Elle n’a pas été renvoyée.';

  @override
  String get pluginsServiceYes => 'Oui';

  @override
  String get pluginsSettingsUnknown =>
      'Le réglage n’est pas confirmé. Actualisez l’état avant de choisir à nouveau.';

  @override
  String get pluginsSnapshotDetails =>
      'La sauvegarde contient les cartes, pièces jointes et journaux d’audit. Les ressources externes restent des références. La récupération nécessite le compte système d’origine.';

  @override
  String get pluginsSnapshotSaved =>
      'Bibliothèque sauvegardée, avec ses pièces jointes et son fichier de protection d’origine.';

  @override
  String get pluginsStateUnavailable =>
      'Impossible de charger l’état de l’extension. Réessayez.';

  @override
  String get pluginsSummary => 'Lire les résumés';

  @override
  String get pluginsTextInput => 'Texte à saisir';

  @override
  String get pluginsThirdParty => 'Extensions tierces';

  @override
  String get pluginsTlsIdentitiesDisable => 'Désactiver l’identité';

  @override
  String get pluginsTlsIdentitiesEmpty =>
      'Aucune identité enregistrée dans cette bibliothèque.';

  @override
  String get pluginsTlsIdentitiesFileMode =>
      'Prochain démarrage : fichiers locaux vérifiés.';

  @override
  String get pluginsTlsIdentitiesHint =>
      'Sélectionnez une identité explicitement. La remplacer ou la désactiver arrête les services qui l’utilisent ; tout nouveau démarrage est explicite.';

  @override
  String get pluginsTlsIdentitiesImport =>
      'Préparer les certificats pour importation ou remplacement';

  @override
  String get pluginsTlsIdentitiesReplace =>
      'Remplacer par les fichiers vérifiés';

  @override
  String get pluginsTlsIdentitiesSave => 'Enregistrer comme nouvelle identité';

  @override
  String get pluginsTlsIdentitiesSaved =>
      'Enregistré. Vérifiez l’identité et la révision ci-dessous, puis sélectionnez-la pour un nouveau démarrage.';

  @override
  String get pluginsTlsIdentitiesSavedMode =>
      'Prochain démarrage : identité enregistrée. L’hôte vérifie la validité du certificat au démarrage.';

  @override
  String get pluginsTlsIdentitiesSelect => 'Utiliser au prochain démarrage';

  @override
  String get pluginsTlsIdentitiesStale =>
      'L’identité choisie a changé, a été désactivée ou n’a pas été actualisée. Sélectionnez à nouveau une identité actuelle.';

  @override
  String get pluginsTlsIdentitiesTitle => 'Identités TLS enregistrées';

  @override
  String get pluginsTlsIdentitiesUnknownHint =>
      'Actualisez et examinez les enregistrements avant d’acquitter. L’absence de reçu ne signifie pas un échec ; ne recréez pas sans vérifier.';

  @override
  String get pluginsTlsIdentitiesUseFile =>
      'Utiliser les fichiers vérifiés au prochain démarrage';

  @override
  String get pluginsTransform => 'Transformer';

  @override
  String get pluginsTransformUnknown =>
      'La transformation n’a pas pu être confirmée';

  @override
  String get pluginsUiExecution =>
      'L’exécution de l’extension ne s’est pas terminée. Rouvrez la vue et réessayez.';

  @override
  String get pluginsUiRejected =>
      'L’action de l’extension a été refusée. Vérifiez la saisie et les autorisations actuelles.';

  @override
  String get pluginsUiUnavailable =>
      'L’extension est indisponible. Vérifiez son état et rouvrez la vue.';

  @override
  String get pluginsUnavailableView => 'Vue de l’extension indisponible';

  @override
  String pluginsUnconfirmed(String reason) {
    return '$reason. L’opération n’est pas confirmée. Vérifiez l’état actualisé avant de choisir à nouveau.';
  }

  @override
  String get pluginsUninstallKeepContent => 'Désinstaller (garder le contenu)';

  @override
  String get pluginsUninstallUnknown =>
      'La désinstallation n’a pas pu être confirmée';

  @override
  String get pluginsUninstalled =>
      'Désinstallée. Votre contenu existant est préservé.';

  @override
  String get pluginsUpdatingView => 'Actualisation de l’aperçu…';

  @override
  String get pluginsUseText => 'Utiliser du texte';

  @override
  String get pluginsUseTransform => 'Utiliser la transformation';

  @override
  String get pluginsViewFailed => 'Impossible d’ouvrir la vue de l’extension';

  @override
  String get pluginsWorkbench => 'Extension de l’espace de travail';

  @override
  String get pluginsWorkbenchReadOnly =>
      'L’extension est autorisée, mais l’espace est en lecture seule. Résolvez le problème de bibliothèque ou d’extension, puis actualisez son état.';

  @override
  String get recoveryAllFiles => 'Tous les fichiers';

  @override
  String get recoveryBackupExists =>
      'Un fichier existe déjà à cet emplacement. Choisissez un nouveau nom.';

  @override
  String get recoveryBackupFile => 'Sauvegarde de bibliothèque';

  @override
  String get recoveryBackupUnknown =>
      'Résultat de sauvegarde à vérifier. Conservez le fichier actuel et inspectez l’emplacement.';

  @override
  String get recoveryBindingMissing =>
      'Aucun fichier de protection lié à cette bibliothèque. Impossible d’y associer le fichier choisi.';

  @override
  String get recoveryBusy =>
      'Bibliothèque utilisée par un autre processus. Fermez l’autre fenêtre et réessayez.';

  @override
  String get recoveryChooseKey => 'Choisir le fichier de récupération';

  @override
  String get recoveryCloseFirst =>
      'L’espace de travail est encore ouvert. Fermez-le avant de changer de bibliothèque.';

  @override
  String get recoveryClosing =>
      'En attente de la fermeture du service initial. La réouverture et la restauration restent indisponibles jusqu’à sa confirmation.';

  @override
  String get recoveryClosingUnconfirmed =>
      'La fermeture reste non confirmée. La surveillance continue ; le délai écoulé n’annule pas les opérations effectuées.';

  @override
  String get recoveryFailed =>
      'La récupération n’est pas terminée. Conservez les fichiers originaux et réessayez.';

  @override
  String get recoveryIdentityBusy =>
      'Une autre copie de cette bibliothèque est ouverte. Fermez-la avant d’ouvrir celle-ci.';

  @override
  String get recoveryIdentityMismatch =>
      'Identité de bibliothèque différente de l’enregistrement. Conservez les données originales et restaurez la bonne sauvegarde.';

  @override
  String get recoveryKeyFile => 'Fichier de protection de bibliothèque';

  @override
  String get recoveryKeyGuide =>
      'Si le fichier de protection manque ou est endommagé, choisissez sa sauvegarde. Elle doit appartenir à cette bibliothèque et nécessite le compte système d’origine.';

  @override
  String get recoveryKeyMismatch =>
      'Clé incompatible ou indéchiffrable. Utilisez le fichier et le compte système d’origine.';

  @override
  String get recoveryKeyUnknown =>
      'Résultat de récupération à vérifier. Essayez de rouvrir ; une copie de l’ancien fichier de protection a été conservée s’il existait.';

  @override
  String get recoveryLibraryInvalid =>
      'Impossible de vérifier ou d’ouvrir la bibliothèque. Conservez la bibliothèque et la clé originales, puis réessayez.';

  @override
  String get recoveryMaintenance =>
      'La bibliothèque nécessite une intervention. Conservez les originaux et consultez le diagnostic.';

  @override
  String get recoveryMigrationIncomplete =>
      'La migration de cette bibliothèque est incomplète. Consultez le rapport dans son dossier, conservez la bibliothèque source et réessayez dans un nouveau dossier.';

  @override
  String get recoveryMissingKey =>
      'Clé de protection absente. Restaurez le fichier .audit-key original et réessayez.';

  @override
  String get recoveryMissingLibrary =>
      'Clé présente, mais bibliothèque absente ou vide. Restaurez la bibliothèque originale.';

  @override
  String get recoveryOpenFailed =>
      'Impossible d’ouvrir l’espace de travail. Vérifiez les extensions et le dossier de données, puis réessayez.';

  @override
  String get recoveryPluginUnavailable =>
      'Extension de l’espace de travail indisponible. Le contenu existant reste consultable et exportable.';

  @override
  String get recoveryRegistryInvalid =>
      'Enregistrement de bibliothèque actif endommagé ou non pris en charge. Ouverture arrêtée pour préserver les données.';

  @override
  String get recoveryRegistryUnreadable =>
      'Impossible de lire la bibliothèque active ou son enregistrement. Vérifiez l’emplacement original ; aucune bibliothèque de remplacement ne sera créée automatiquement.';

  @override
  String get recoveryRetry => 'Réessayer';

  @override
  String get recoverySnapshot => 'Restaurer une sauvegarde';

  @override
  String get recoverySnapshotGuide =>
      'Restaurez une sauvegarde dans un nouveau dossier puis basculez vers celui-ci. Le dossier original est conservé. Le contenu revient à la date de sauvegarde et exige le compte système d’origine.';

  @override
  String get recoverySnapshotInvalid =>
      'Format ou intégrité de la sauvegarde invalide. Conservez le fichier original.';

  @override
  String get recoverySnapshotUnknown =>
      'Résultat à vérifier. Inspectez le dossier de destination ; la bibliothèque originale n’a pas été remplacée.';

  @override
  String recoverySwitchUnconfirmed(String path) {
    return 'Sauvegarde restaurée dans $path, mais basculement non confirmé. Conservez ce dossier et rouvrez l’espace pour vérifier.';
  }

  @override
  String get recoverySwitchUnknown =>
      'Changement de bibliothèque non confirmé. Rouvrez l’espace de travail pour vérifier.';

  @override
  String get recoveryTargetExists =>
      'La destination existe déjà. Choisissez un nouveau dossier qui n’existe pas encore.';

  @override
  String get recoveryTitle => 'Rouvrir l’espace de travail';

  @override
  String get visualApplyColor => 'Appliquer la couleur';

  @override
  String get visualApplyComponent => 'Appliquer à ce composant';

  @override
  String get visualApplyTexture => 'Appliquer le média';

  @override
  String visualAttachmentDetails(String action, String extension, String size) {
    return '$extension · $size · $action';
  }

  @override
  String get visualAttachmentFailure =>
      'Échec de l’opération sur le fichier. Vérifiez le fichier et l’espace disponible.';

  @override
  String get visualAttachmentPreview => 'Aperçu de la pièce jointe locale';

  @override
  String get visualAttachmentReadFailure =>
      'Impossible de lire la pièce jointe. Importez-la à nouveau.';

  @override
  String get visualAudio => 'Audio';

  @override
  String get visualAudioStateFailure => 'État audio non confirmé. Réessayez.';

  @override
  String get visualAutoLyrics =>
      'Chercher automatiquement les paroles manquantes en ligne';

  @override
  String get visualCancel => 'Annuler';

  @override
  String get visualChangeCover => 'Changer la pochette';

  @override
  String get visualChooseAudio =>
      'Choisissez un fichier audio ou un fichier LRC du même nom.';

  @override
  String get visualChooseLyrics =>
      'Choisissez un fichier de paroles LRC ou TXT.';

  @override
  String get visualClickPreview => 'Sélectionner pour prévisualiser';

  @override
  String get visualClose => 'Fermer';

  @override
  String get visualCloseDialog => 'Fermer la boîte de dialogue';

  @override
  String get visualCloseWindow => 'Fermer la fenêtre';

  @override
  String get visualCollapsePlaylist => 'Réduire la liste';

  @override
  String get visualColorGuide =>
      'Faites glisser le cercle pour choisir teinte et saturation, puis réglez la luminosité. Vous pouvez aussi saisir une couleur.';

  @override
  String get visualColorTitle => 'Colorez votre espace';

  @override
  String visualComponentCompass(String title) {
    return '$title · Cercle chromatique';
  }

  @override
  String get visualComponents => 'Composants et cartes';

  @override
  String get visualComponentsGuide =>
      'Chaque élément suit le thème par défaut. Personnaliser une carte ne change pas les autres.';

  @override
  String get visualCornerTips1 =>
      'Toutes les idées n’ont pas à être utiles.\nCertaines rendent simplement la journée plus belle.';

  @override
  String get visualCornerTips2 =>
      'Notez-la, puis laissez-la grandir.\nUne idée n’a pas besoin d’être achevée.';

  @override
  String get visualCornerTips3 =>
      'Gardez un peu d’espace pour vous.\nLa curiosité a besoin de respirer.';

  @override
  String get visualCornerTips4 =>
      'Essayez quelque chose de nouveau.\nUn petit détour peut vous surprendre.';

  @override
  String get visualCornerTips5 =>
      'Rêver mène parfois quelque part.\nLaissez vos pensées vagabonder.';

  @override
  String get visualCornerTips6 =>
      'Prenez du temps pour ce que vous aimez.\nNul besoin d’en prouver la valeur.';

  @override
  String get visualCornerTips7 =>
      'Le progrès peut être discret.\nVouloir commencer compte déjà.';

  @override
  String get visualCornerTips8 =>
      'Regardez parfois par la fenêtre.\nLa vie aussi inspire.';

  @override
  String get visualCover => 'Pochette';

  @override
  String get visualCustomCompass => 'Cercle chromatique · Personnalisé';

  @override
  String get visualCustomMaterialGuide =>
      'Désactivez pour suivre le thème tout en conservant les réglages de cet élément.';

  @override
  String get visualDefaultOpen => 'Ouvrir avec l’application par défaut';

  @override
  String get visualDownloadOpen => 'Télécharger pour ouvrir';

  @override
  String get visualEmbeddedLyrics => 'Intégrées à l’audio';

  @override
  String get visualExpandPlaylist => 'Développer la liste';

  @override
  String get visualFile => 'Fichier';

  @override
  String get visualFileOpenFailure =>
      'Impossible d’ouvrir le fichier. Installez une application compatible ou enregistrez la pièce jointe pour l’y ouvrir.';

  @override
  String get visualFileRetry =>
      'Échec de l’opération sur le fichier. Réessayez.';

  @override
  String get visualFindLyrics => 'Trouver des paroles';

  @override
  String get visualFindLyricsGuide =>
      'Cherchez sur LRCLIB par titre et artiste, puis choisissez la bonne version.';

  @override
  String visualFollowChain(String path) {
    return 'Chaîne de suivi : $path';
  }

  @override
  String visualFollowComponent(String name) {
    return 'Suit : $name';
  }

  @override
  String get visualFollowCycle => 'Créerait une boucle';

  @override
  String get visualFollowGuide =>
      'Suivez un autre composant ; dissociez-le pour retrouver vos réglages.';

  @override
  String get visualFollowTheme => 'Suivre le thème';

  @override
  String get visualFooterLyrics => 'Afficher les paroles en bas';

  @override
  String get visualFooterTips => 'Afficher les conseils en bas';

  @override
  String get visualFooterTips1 =>
      'Rien ne presse. Accordez du temps à la curiosité.';

  @override
  String get visualFooterTips10 =>
      'Pas besoin de remplir chaque minute. Gardez de la place.';

  @override
  String get visualFooterTips2 =>
      'Notez une pensée. Vous l’organiserez plus tard.';

  @override
  String get visualFooterTips3 =>
      'Transformez une grande idée en un petit pas pour aujourd’hui.';

  @override
  String get visualFooterTips4 => 'Étirez-vous et reposez vos yeux.';

  @override
  String get visualFooterTips5 =>
      'Une idée peut rester sans réponse pour le moment.';

  @override
  String get visualFooterTips6 =>
      'Certaines découvertes arrivent quand on ralentit.';

  @override
  String get visualFooterTips7 => 'Garder un détail aide une idée à grandir.';

  @override
  String get visualFooterTips8 =>
      'Une note aujourd’hui peut devenir un début demain.';

  @override
  String get visualFooterTips9 =>
      'Laissez l’esprit vagabonder, puis revenez à ce que vous aimez.';

  @override
  String get visualFrosting => 'Flou';

  @override
  String get visualGif => 'GIF animé';

  @override
  String get visualHexColor => 'Couleur HEX';

  @override
  String get visualHexInvalid =>
      'Saisissez une couleur hexadécimale à six chiffres.';

  @override
  String get visualImage => 'Image';

  @override
  String get visualImageDecodeFailure =>
      'Impossible de décoder l’image. Enregistrez-la et ouvrez-la dans une autre application.';

  @override
  String visualImageLoadFailure(String name) {
    return 'Impossible de charger l’image : $name';
  }

  @override
  String visualImageNotImported(String name) {
    return '$name (image non importée)';
  }

  @override
  String visualImageUnavailable(String name) {
    return 'Image indisponible : $name';
  }

  @override
  String get visualImportFailure =>
      'Échec de l’importation. Vérifiez fichier, encodage et espace disponible.';

  @override
  String get visualImportLyrics => 'Importer des paroles';

  @override
  String get visualImportMusic => 'Importer de la musique';

  @override
  String get visualImportMusicHint =>
      'Sélectionnez + pour importer des morceaux locaux';

  @override
  String get visualIndependentMaterial => 'Matériau personnalisé';

  @override
  String get visualInheritColor => 'Utiliser la teinte du thème';

  @override
  String get visualLinkFailure =>
      'Impossible d’ouvrir le lien. Copiez l’adresse et réessayez.';

  @override
  String visualLoadImage(String name) {
    return 'Charger l’image · $name';
  }

  @override
  String get visualLoading => 'Chargement…';

  @override
  String get visualLyricsEmpty => 'Le fichier de paroles est vide.';

  @override
  String get visualLyricsFile => 'Fichier de paroles';

  @override
  String get visualLyricsImportHint =>
      'Importez les paroles ou cherchez en ligne.';

  @override
  String get visualLyricsLoading => 'Chargement des paroles…';

  @override
  String visualLyricsMatch(String album, String kind, int seconds) {
    String _temp0 = intl.Intl.pluralLogic(
      seconds,
      locale: localeName,
      other: '$seconds secondes',
      one: '$seconds seconde',
    );
    return '$album\n$kind · $_temp0';
  }

  @override
  String get visualLyricsMissing =>
      'Paroles introuvables. Importez un fichier ou relancez la recherche.';

  @override
  String get visualLyricsNotFound =>
      'Aucune parole trouvée. Essayez un autre titre ou artiste.';

  @override
  String get visualLyricsOnPlay => 'Charger les paroles pendant la lecture';

  @override
  String get visualLyricsParseFailure =>
      'Impossible d’analyser les paroles. Importez-les à nouveau.';

  @override
  String get visualLyricsReadFailure =>
      'Impossible de charger les paroles. Importez-les manuellement ou réessayez.';

  @override
  String get visualLyricsServiceFailure =>
      'Connexion au service de paroles impossible. Réessayez plus tard ou importez un fichier local.';

  @override
  String get visualLyricsSize => 'Les paroles doivent tenir dans 1 Mo.';

  @override
  String get visualLyricsSources => 'Fichier local → Intégrées → LRCLIB';

  @override
  String get visualLyricsVersions =>
      'Plusieurs versions trouvées. Choisissez-en une dans la recherche.';

  @override
  String get visualMaterialPreview => 'Aperçu du matériau';

  @override
  String get visualMaterialSource => 'Source du matériau';

  @override
  String get visualMaximize => 'Agrandir';

  @override
  String get visualMediaAddress => 'Adresse du média';

  @override
  String get visualMediaAddressInvalid =>
      'Saisissez une adresse HTTP ou HTTPS valide sans identifiants.';

  @override
  String get visualMediaPreviewFailure =>
      'Aperçu indisponible. Enregistrez le média et ouvrez-le dans une autre application.';

  @override
  String get visualMediaType => 'Type de média';

  @override
  String get visualMinimize => 'Réduire';

  @override
  String get visualMusic => 'Musique';

  @override
  String get visualMusicEmptyTitle => 'Faites place à la musique';

  @override
  String get visualMusicPlayer => 'Lecteur audio';

  @override
  String get visualNextTrack => 'Piste suivante';

  @override
  String get visualNoLyricsRead => 'Aucune parole chargée';

  @override
  String visualNoLyricsTitle(String title) {
    return '♪ $title · Sans paroles';
  }

  @override
  String get visualNoTimeline => 'Sans repères temporels';

  @override
  String get visualOpacity => 'Opacité';

  @override
  String get visualOptionalArtist => 'Artiste (facultatif)';

  @override
  String get visualOwnMaterial => 'Thème ou réglages propres';

  @override
  String get visualPauseMusic => 'Mettre en pause';

  @override
  String get visualPaused => 'En pause';

  @override
  String get visualPlainLyrics => 'Paroles en texte brut';

  @override
  String get visualPlayMusic => 'Lire la musique';

  @override
  String get visualPlaybackFailure =>
      'Lecture impossible. Vérifiez le fichier ou essayez un autre format audio.';

  @override
  String visualPlaybackPosition(int count, int index, String state) {
    return '$index / $count · $state';
  }

  @override
  String get visualPlaybackRequestFailure =>
      'Impossible de lancer la lecture. Réessayez.';

  @override
  String get visualPlaying => 'En lecture';

  @override
  String get visualPlaylistEmpty => 'Votre liste est vide';

  @override
  String get visualPlaylistLyricsHint =>
      'Importez les LRC depuis le menu de la liste';

  @override
  String get visualPlaylistSaved =>
      'Liste et paroles enregistrées automatiquement';

  @override
  String get visualPlaylistUpdateFailure =>
      'Impossible de mettre à jour la liste. Réessayez.';

  @override
  String get visualPreviewColor => 'Aperçu de la couleur';

  @override
  String get visualPreviousTrack => 'Piste précédente';

  @override
  String get visualRemoveAttachment => 'Retirer la pièce jointe';

  @override
  String get visualRemoveTrack => 'Retirer de la liste';

  @override
  String get visualResetMaterial => 'Rétablir le thème';

  @override
  String get visualRestoreWindow => 'Restaurer';

  @override
  String get visualSaveAttachment => 'Enregistrer la pièce jointe sous';

  @override
  String get visualSearch => 'Rechercher';

  @override
  String get visualSearchLyrics => 'Rechercher des paroles';

  @override
  String get visualSongCover => 'Pochette de l’album';

  @override
  String get visualSongTitle => 'Titre du morceau';

  @override
  String get visualSyncedLyrics => 'Paroles synchronisées';

  @override
  String get visualTextureFailure =>
      'Impossible de charger le média. Vérifiez fichier, adresse et format. Sur le Web, l’accès interorigine doit aussi être autorisé.';

  @override
  String get visualTextureLinkGuide =>
      'Collez un lien HTTP ou HTTPS direct vers une image, un GIF ou une vidéo. Pour une page partagée, trouvez d’abord l’adresse du média original.';

  @override
  String get visualTextureLinkTitle => 'Invitez l’inspiration';

  @override
  String get visualTexturePlaybackGuide =>
      'Les vidéos tournent en boucle sans son ; activez-le dans les réglages. Les médias en ligne doivent autoriser l’accès, y compris interorigine sur le Web.';

  @override
  String get visualTipsMaterialGuide =>
      'Désactivé : conseils transparents ; activé : matériau ci-dessous. Vos valeurs sont conservées.';

  @override
  String get visualTransparentTips => 'Superposition transparente (par défaut)';

  @override
  String get visualUseCustomMaterial => 'Utiliser un matériau personnalisé';

  @override
  String get visualVideo => 'Vidéo';

  @override
  String get visualViewLyrics => 'Voir les paroles';
}
