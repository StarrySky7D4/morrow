# Morrow · 明隙

[简体中文（默认）](../../README.md) · [English](../../docs/readme/README.en.md) · [Русский](../../docs/readme/README.ru.md) · [Français](../../docs/readme/README.fr.md) · **Deutsch** · [Español](../../docs/readme/README.es.md) · [日本語](../../docs/readme/README.ja.md) · [한국어](../../docs/readme/README.ko.md) · [Português](../../docs/readme/README.pt.md)

Lass etwas Raum für die Ideen von morgen.

Morrow (明隙) ist ein lokaler, kartenbasierter Arbeitsbereich für Ideen und entwickelt sich zu einer plattformübergreifenden Plugin-Architektur. Der frühere Name lautet daemon, der Paketname `morrow_studio`. Unter Windows stellt Flutter die Oberfläche bereit; ein Rust-Host und isolierte Wasm-Plugins führen die Arbeitsbereichslogik aus. Web führt Rust/Wasm lokal im Browser aus; Android nutzt weiterhin den früheren Pfad. Die vollständige Plugin- und Produktgleichheit aller Plattformen ist noch nicht abgenommen.

## Aktueller Entwicklungsstand

Dieses Update enthält C08–C10. Das unabhängige Profil fs-directory-request-v1 verwendet die ursprüngliche genehmigte Verzeichnisauswahl und den Owner. Begrenzte Windows-Prüfungen bestanden: 34 neue Rust-Methoden, 191 vorhandene Regressionsmethoden und 5 Frame-Methoden; 33 Python-Tests und zwei native C/C++-Verbraucher werden getrennt gezählt. Das neue Rust-Wasm wurde nur kompiliert; tatsächliche neue Rust-Guest-Ausführung, C/C++-Wasm, Workbench-Produkt/GUI, geschützte Session und andere Plattformen bleiben ungeprüft. Bisheriges Core-IO-FileList bleibt Unsupported, SDK26/G04 bleibt OPEN. Kein neues Release. [C10](../../reports/reconstruction-2026-10-05/directory-request-sdk.md)

## Entwicklungsstand (2026-10-05)

Der Quellcode-Checkpoint der Anwendung ist **0.1.9-test.58+62**. Die vorgesehene Entwicklungslinie heißt `codex/windows-sdk-convergence-20261005` und basiert auf cloud `468ef2e` mit C02–C07-Änderungen. Der [Projektstatus](../../docs/PROJECT_STATUS.md) enthält den maßgeblichen Stand und die nächsten Schritte; der [begrenzte C07-Prüfbericht](../../reports/reconstruction-2026-10-05/directory-owner-sdk.md) dokumentiert die tatsächlichen Windows-Nachweise.

Historische C07-Prüfung: Die lokale Windows-Prüfung mit synthetischen Daten bestand: 115 Methoden des ursprünglichen Owner-Pfads, darin 7 für Verzeichnisabbruch, 14 für directory owner und 16 für native Verzeichnisse; 42 ursprüngliche Regressionsmethoden; 100 Netzwerkmethoden in 11 Programmen. Der Lauf mit 42 Methoden behält 14 frozen-region-Filter und einen remote-reader-Filter bei. Kindprozess-Helfer zählen nicht als zusätzliche bestandene Methoden. Diese Zahlen belegen keine vollständige SDK- oder Produktabnahme.

Vertrauenswürdiges capture/page/finish für Verzeichnisse nutzt die ursprüngliche IoWorker-Warteschlange, Manager, Runtime, Instanz, IoBinding und FileList-Freigabe. Uhrabfrage und Berechtigungsprüfung erfolgen in kurzen atomaren Schritten; Kontingente werden erst nach tatsächlicher Freigabe der nativen Ressourcen frei. Unknown-Ergebnisse werden nicht automatisch wiederholt, Cursor laufen am Zählerlimit nicht über. Unabhängig versionierte Bibliotheken für typisierte WebSocket/SSE-Nutzdaten bieten Rust-, C- und C++-Codecs mit begrenzter Guest-Prüfung; Verzeichnis-/Blob-Codecs und begrenzter Zustand sind ebenfalls lokal qualifiziert.

## Historische begrenzte C08/C09-Ergebnisse

**SDK26/G04 bleibt OPEN; das vollständige SDK ist nicht eingefroren.** Geschützte Produktions-Owner, Workspace-Produktabnahme/GUI, Auswahldialog und Herkunft der Vorfahren, vertrauenswürdige Produktionsauswahl und Geheimnislebenszyklus, Aushandlung neuer Verzeichnis-Request-/Import-/Feature-/Helper-Profile, dauerhafte Blob-Historie, vollständige Produktqualifikation eingehender Dienste, echte TLS/API und andere Plattformen bleiben offen oder NOT_RUN. Die native C08-Geheimnisfabrik im ursprünglichen Worker ist begrenzt geprüft: 14 Methoden bestanden, 69 nicht ausgewählte Methoden gefiltert; owner115, original42 und network100 wurden jeweils neu geprüft. Siehe [C08](../../reports/reconstruction-2026-10-05/directory-secret-factory.md). C08 bleibt lokal ohne Commit/Push; der frühere Dokumentationscommit772466 bleibt. Diese Prüfung veröffentlicht keinen neuen Installer und ergibt kein stabiles SDK.

Die lokale vertrauenswürdige relative Verzeichnisauswahl C09 bestand eine begrenzte Windows-Prüfung; die20 neuen Methoden und ursprünglichen Regressionen stehen im [C09-Bericht](../../reports/reconstruction-2026-10-05/directory-selection-owner.md), der nur gehaltene Handles vom geöffneten root zu relativen Nachfahren abdeckt, ohne Picker-, Above-root- oder Produktionsnachweis. Der Workbench-Hosteinstieg ist nur durch Kompilierung geprüft; Workspace-Produktabnahme/GUI bleiben ungetestet, öffentliche guest FileList und conditional Replace Unsupported sowie SDK26/G04 OPEN. Der gepushte Checkpoint ist772466; C08/C09 bleiben lokal, ohne Commit/Push oder neue Veröffentlichung.

## Download und Kompatibilität

Neueste veröffentlichte App-Vorschau: **0.1.9-test.56+60**, eine **Windows-x64-Testvorschau**, keine stabile Veröffentlichung. Lade das Windows-ZIP, das zugehörige Quellcode-ZIP und die SHA-256-Liste herunter. Entpacke alles und starte `morrow_studio.exe`; sämtliche DLLs, Host, `data`, `plugins` und Lizenzdateien müssen erhalten bleiben.

[test.56 herunterladen](https://github.com/StarrySky7D4/morrow/releases/tag/v0.1.9-test.56) · [Kompatible Testversion test.1](https://github.com/StarrySky7D4/morrow/releases/tag/v0.1.9-test.1)

`test.1` ist die letzte `0.1.x`-Testversion, die mit den ursprünglichen Datentypen kompatibel ist. Spätere Testversionen begleiten die Neuentwicklung und können inkompatible Änderungen enthalten. `0.2.0` folgt nach Stabilisierung und Abnahme von Architektur und Datenmodell. Daten aus test.1 werden nicht automatisch importiert oder überschrieben. Sichere Bibliothek und ursprüngliche Schutzdatei vor dem Update. Der Schutz ist an den Windows-Benutzer gebunden; die Datenbank allein reicht für einen Kontowechsel nicht aus.

## Funktionen

- Karten: Ideen, Projekte, Experimente, Favoriten, Suche, Checklisten und Rücknahme von Löschungen; Markdown mit Vorschau und eigenständige Kopien von Anhängen.
- Inhalte erfassen: Text, Bildschirmaufnahmen, Dateien sowie Office-Rich-Text und Tabellen. Komplexe oder proprietäre Formate können als Vorschau oder Anhang übernommen werden; identisches Layout ist nicht garantiert.
- Darstellung: Milchglas, ultraklares und flüssiges Glas; Standard-, Farb-, Textur- und transparente Hintergründe; Themenfarbrad und eigene Einstellungen pro Karte oder Komponente, mit reduzierten Animationen.
- Medien: Bilder, GIFs und Videos als Hintergrund, lokale Musik, Liedtexte und schwebende Hinweise. Formate hängen von Plattform und Decoder ab. Web-Transparenz zeigt die einbettende Seite, nicht den Desktop.
- Layout und Sprachen: anpassbare Inhalte und separate Einstellungsseiten; Chinesisch, Englisch, Russisch, Französisch, Deutsch, Spanisch, Japanisch, Koreanisch und Portugiesisch.
- Schutz unter Windows: einheitliche Rust-Speicherung, versiegelte Audit-Aufzeichnungen, Bibliothekssnapshots, Sicherung und Wiederherstellung der ursprünglichen Identität sowie begrenzte gleichzeitige Nutzung derselben Identität.

## Änderungen und Prüfung der veröffentlichten Version test.56

test.56 ergänzt Kontextaktionen, Kartensortierung und Ziehen, die Bearbeitung einzelner Tipps und eigenständige Themen-Plugins. Das Kachel-Layout, gefilterte Aktualisierungen und das Beenden wurden verbessert. Vollständiges automatisches Speichern und die Plattformgleichheit bleiben offen.

### Frühere Änderungen von test.55

test.55 ergänzt sieben Darstellungsstile und eine Tiefenregelung, verbessert Animationen der Bedienelemente und Abstände zwischen Komponenten, behebt Fehler beim Speichern von Einstellungen und Anwenden von Schriftarten, verbessert das sichere Beenden im Hintergrund und legt die Grundlage für die Wiederherstellung von Entwürfen. Automatisches Speichern in der regulären Oberfläche ist noch nicht fertig.

### Frühere Optimierung und Prüfung von test.54

test.54 entfernt wiederholte Vollprüfungen beim Öffnen, dekodiert Nachweise mit begrenzter Parallelität und verwendet geprüfte Größen und Archiv-Digests innerhalb derselben Prüftransaktion erneut. Jedes Öffnen bleibt vollständig geprüft. Speicherformat und mitgeliefertes Plugin bleiben unverändert; Prüfergebnisse werden nicht über Neustarts hinweg zwischengespeichert.

Die letzte Leseoptimierung wurde in je vier abwechselnden Starts mit der vorherigen parallelen Version verglichen. Auf demselben Rechner mit einer Bibliothekskopie von etwa 100 MB sank der Median bis zum Entfernen der Ladeabdeckung von 2.222 auf 1.299 s ab Dart-Einstieg. Die Registrierung der Kopie kann den Dateicache vorwärmen; dies ist keine Garantie für einen Kaltstart mit geleertem Cache. Bestanden: 568 Core-, 97 Audit- und 2 echte Host-Integrationstests; ein bestehender Core-Test ignoriert. Die Bedingungen stehen in den [Versionshinweisen zu test.54](../../reports/0.1.9-test.54-release.md).

## Aus dem Quellcode starten

Benötigt werden Flutter 3.44 / Dart 3.12 oder kompatible Versionen. Unter Windows zusätzlich Visual Studio C++-Desktop-Buildtools, Windows SDK, Rust und der Cap’n-Proto-Compiler im PATH.

Vollständiger Windows-Rust-Build mit Integrationsprüfungen und Paketierung:

```powershell
flutter pub get
rustup target add wasm32-unknown-unknown
pwsh -File tool/build_rust_workbench_windows.ps1
```

Artefakte werden nicht überschrieben. Verwende eine neue Version oder ein neues Ausgabeverzeichnis; `-RefreshArtifact` erlaubt kein Ersetzen mehr. Verteile das gesamte Laufzeitverzeichnis. Web-/Android-Prüfumfang steht in den Plattformdokumenten; ein Windows-Build ersetzt diese Prüfung nicht.

Entwicklung und grundlegende Prüfungen:

```powershell
flutter run -d windows
flutter run -d chrome
flutter analyze
flutter test
flutter build web --no-web-resources-cdn
```

## Architektur, SDK und nächste Schritte

Ziel sind eine Flutter/Dart-Oberfläche, ein portabler Rust-Kern und austauschbare Plugin-Ausführungsumgebungen. Laufzeitgrenzen verwenden feste Verträge; Speicherung und eigene Austauschformate verwenden Protobuf + LZ4. C/C++/Rust-SDKs und deklarative Plugin-Oberflächen sind in Entwicklung. TS/JS-Plugins werden nicht unterstützt; dynamische Dart-Plugins sind nicht erforderlich.

Das vollständige SDK ist noch nicht eingefroren. Verwaltete HTTP/HTTPS-Anfragen, begrenzte API-Dienstknoten und TLS-Identitätsverwaltung sind angebunden. Offen bleiben Unknown-Abgleich nach Neustarts, vollständiges Dateisystem, tatsächliche Guest-Ausführung und Produktabnahme des neuen Verzeichnisprofils und Plattformabnahmen. Beim Öffnen wird weiterhin die gesamte Historie geprüft; vollständige Lese-/Rechenpipeline, historische Speicherebenen und automatische Bereinigung fehlen noch.

## Dokumentation

- [Aktueller Projektstatus](../../docs/PROJECT_STATUS.md)
- [Versionshinweise und Prüfungen](../../reports/0.1.9-test.56-release.md)
- [Entwicklungsboard](../../docs/DEVELOPMENT_BOARD.md)
- [Architekturplanung](../../docs/FUTURE_ROADMAP.md)
- [Funktionsmigration und Kapazitätsgrenzen](../../docs/TEST1_RUST_PARITY.md)
- [C/C++/Rust-SDK](../../sdk/README.md)
- [Plugin-Oberflächen](../../docs/PLUGIN_SDK_AND_UI.md)
- [Inhaltserfassung und Formate](../../docs/RICH_CAPTURE.md)
- [Android-Build](../../docs/ANDROID.md)
- [Kompatibilität der Umbenennung](../../docs/RENAMING.md)
- [Historisches README und Meilensteine](../../docs/history/README-before-test54.md)

Ausführliche Entwürfe und Prüfberichte sind überwiegend auf Chinesisch. Alle READMEs beschreiben dieselbe aktuelle Version; eine umfassende muttersprachliche Prüfung steht noch aus.

## Lizenz

Seit `0.1.9-test.2` stehen eigener Code, SDKs, Dokumentation, Konfiguration und Medien unter **AGPL-3.0-only**. test.1 und frühere Versionen behalten Apache-2.0; Drittinhalte behalten ihre jeweiligen Lizenzen. Binärpakete enthalten Lizenzen, Urheberrechtshinweise und Zugang zum zugehörigen Quellcode.

[AGPL-3.0-only](../../LICENSE) · [NOTICE](../../NOTICE) · [Third-party licenses](../../packaging/THIRD_PARTY_NOTICES.txt)
