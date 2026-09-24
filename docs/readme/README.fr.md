# Morrow · 明隙

[简体中文（默认）](../../README.md) · [English](../../docs/readme/README.en.md) · [Русский](../../docs/readme/README.ru.md) · **Français** · [Deutsch](../../docs/readme/README.de.md) · [Español](../../docs/readme/README.es.md) · [日本語](../../docs/readme/README.ja.md) · [한국어](../../docs/readme/README.ko.md) · [Português](../../docs/readme/README.pt.md)

Gardez un peu de place pour les idées de demain.

Morrow (明隙) est un espace de travail local organisé en cartes, qui évolue vers une architecture de plugins multiplateforme. Anciennement daemon, son paquet se nomme `morrow_studio`. Sous Windows, Flutter fournit l’interface, avec un hôte Rust et des plugins Wasm isolés pour la logique. Web et Android conservent l’ancienne implémentation ; le système de plugins n’est pas encore validé sur toutes les plateformes.

## Téléchargement et compatibilité

Version actuelle : **0.1.9-test.55+59**, une **préversion de test Windows x64**, non stable. Téléchargez le ZIP Windows, les sources correspondantes et la liste SHA-256. Extrayez toute l’archive puis lancez `morrow_studio.exe`, en conservant les DLL, l’hôte, `data`, `plugins` et les licences.

[Télécharger test.55](https://github.com/StarrySky7D4/morrow/releases/tag/v0.1.9-test.55) · [Version compatible test.1](https://github.com/StarrySky7D4/morrow/releases/tag/v0.1.9-test.1)

`test.1` est la dernière version de test `0.1.x` compatible avec les types de données d’origine. Les versions suivantes accompagnent une réécriture et peuvent rompre la compatibilité. `0.2.0` suivra la stabilisation et la validation de l’architecture et du modèle de données. Les données test.1 ne sont ni importées ni écrasées automatiquement. Sauvegardez la bibliothèque et son fichier de protection d’origine avant toute mise à niveau. La protection dépend de l’utilisateur Windows ; copier la base seule ne permet pas un transfert entre comptes.

## Fonctionnalités

- Cartes : idées, projets, expériences, favoris, recherche, listes et annulation des suppressions ; édition Markdown avec aperçu et copies indépendantes des pièces jointes.
- Capture enrichie : texte, captures d’écran, fichiers, texte formaté et tableaux Office. Les formats complexes ou propriétaires peuvent être conservés sous forme d’aperçus ou de pièces jointes, sans garantie de mise en page identique.
- Apparence : verre dépoli, ultra-transparent et liquide ; fonds par défaut, unis, texturés ou transparents ; roue chromatique et réglages propres à chaque carte ou composant, avec réduction des animations.
- Médias : fonds image, GIF et vidéo, musique locale, paroles et conseils flottants. Les formats dépendent de la plateforme et du décodeur. Sur le Web, la transparence révèle la page hôte, pas le bureau.
- Disposition et langues : contenu adaptatif et pages de paramètres séparées ; chinois, anglais, russe, français, allemand, espagnol, japonais, coréen et portugais.
- Protection Windows : stockage unifié dans Rust, scellement des journaux d’audit, instantanés, sauvegarde et restauration de l’identité d’origine, limitation des accès simultanés avec la même identité.

## Optimisations et validation

test.55 ajoute sept styles visuels et un réglage de profondeur, améliore les animations des commandes et l’espacement des composants, corrige l’enregistrement des réglages et l’application des polices, améliore la fermeture sûre en arrière-plan et pose les bases de la récupération des brouillons. L’enregistrement automatique dans l’interface principale reste à terminer.

### Optimisation et validation antérieures de test.54

test.54 supprime les vérifications complètes redondantes à l’ouverture, parallélise le décodage des preuves avec des limites et réutilise tailles et empreintes vérifiées dans une même transaction. Chaque ouverture reste entièrement vérifiée. Le format de stockage et le plugin intégré restent inchangés ; aucun cache de validation ne persiste entre les lancements.

La dernière optimisation de lecture a été comparée à la version parallèle précédente sur quatre lancements alternés par version. Sur la même machine et une copie de bibliothèque d’environ 100 Mo, la médiane de disparition de l’écran de chargement passe de 2.222 à 1.299 s depuis l’entrée Dart. L’enregistrement de la copie peut réchauffer le cache : ce n’est pas une garantie de démarrage à froid après vidage du cache. Tests réussis : Core 568, Audit 97, intégration avec hôte réel 2 ; un test Core préexistant ignoré. Voir les [notes de test.54](../../reports/0.1.9-test.54-release.md).

## Exécution depuis les sources

Flutter 3.44 / Dart 3.12 ou versions compatibles sont nécessaires. Sous Windows : outils de compilation C++ de Visual Studio, Windows SDK, Rust et compilateur Cap’n Proto dans PATH.

Compilation complète du poste Rust Windows, tests d’intégration et empaquetage :

```powershell
flutter pub get
rustup target add wasm32-unknown-unknown
pwsh -File tool/build_rust_workbench_windows.ps1
```

Les artefacts ne sont pas écrasables : choisissez une nouvelle version ou un nouveau dossier de sortie. `-RefreshArtifact` n’autorise plus le remplacement. Distribuez tout le dossier d’exécution. Les documents des plateformes précisent la validation Web/Android ; une compilation Windows ne la remplace pas.

Développement et vérifications de base :

```powershell
flutter run -d windows
flutter run -d chrome
flutter analyze
flutter test
flutter build web --no-web-resources-cdn
```

## Architecture, SDK et prochaines étapes

L’architecture cible associe interface Flutter/Dart, cœur Rust portable et moteurs de plugins interchangeables. Les échanges à l’exécution suivent des contrats fixes ; stockage et échanges propres à l’application utilisent Protobuf + LZ4. Les SDK C/C++/Rust et l’interface déclarative des plugins sont en développement. Les plugins TS/JS ne sont pas pris en charge ; les plugins Dart dynamiques ne sont pas nécessaires.

Le SDK complet n’est pas figé. Les requêtes HTTP/HTTPS contrôlées, les nœuds API limités et la gestion des identités TLS sont intégrés. Restent la réconciliation des résultats Unknown après redémarrage, le système de fichiers complet, les SDK IO des trois langages et la validation multiplateforme. L’ouverture parcourt encore tout l’historique ; pipeline complet lecture/calcul, stockage de l’historique par niveaux et nettoyage automatique restent à réaliser.

## Documentation

- [Notes de version et validation](../../reports/0.1.9-test.55-release.md)
- [Tableau de développement](../../docs/DEVELOPMENT_BOARD.md)
- [Feuille de route architecturale](../../docs/FUTURE_ROADMAP.md)
- [Migration des fonctions et limites](../../docs/TEST1_RUST_PARITY.md)
- [SDK C/C++/Rust](../../sdk/README.md)
- [Interface des plugins](../../docs/PLUGIN_SDK_AND_UI.md)
- [Capture enrichie et formats](../../docs/RICH_CAPTURE.md)
- [Compilation Android](../../docs/ANDROID.md)
- [Compatibilité du renommage](../../docs/RENAMING.md)
- [README historique et étapes](../../docs/history/README-before-test54.md)

Les spécifications détaillées et rapports sont principalement en chinois. Les README présentent la même version courante ; les traductions n’ont pas encore été intégralement relues par des locuteurs natifs.

## Licence

Depuis `0.1.9-test.2`, le code, les SDK, la documentation, les configurations et les ressources propres au projet sont sous **AGPL-3.0-only**. test.1 et les versions antérieures conservent Apache-2.0 ; les contenus tiers conservent leurs licences. Les paquets incluent licences, mentions de droits d’auteur et accès aux sources correspondantes.

[AGPL-3.0-only](../../LICENSE) · [NOTICE](../../NOTICE) · [Third-party licenses](../../packaging/THIRD_PARTY_NOTICES.txt)
