# Morrow · 明隙

[简体中文（默认）](../../README.md) · **English** · [Русский](../../docs/readme/README.ru.md) · [Français](../../docs/readme/README.fr.md) · [Deutsch](../../docs/readme/README.de.md) · [Español](../../docs/readme/README.es.md) · [日本語](../../docs/readme/README.ja.md) · [한국어](../../docs/readme/README.ko.md) · [Português](../../docs/readme/README.pt.md)

Leave a little room for tomorrow’s ideas.

Morrow (明隙) is a local-first, card-based workspace for ideas, evolving toward a cross-platform plugin architecture. Formerly daemon, its code package is `morrow_studio`. Windows uses Flutter for the interface and a Rust host with sandboxed Wasm plugins for workbench logic. Web and Android still use the earlier implementation; the plugin system is not yet qualified across platforms.

## Download and compatibility

Current version: **0.1.9-test.54+58**, a **Windows x64 testing preview**, not a stable release. Download the Windows ZIP, corresponding source ZIP and SHA-256 list. Extract the entire archive and run `morrow_studio.exe`; keep all DLLs, the host, `data`, `plugins` and license files.

[Download test.54](https://github.com/StarrySky7D4/morrow/releases/tag/v0.1.9-test.54) · [Compatible test.1 preview](https://github.com/StarrySky7D4/morrow/releases/tag/v0.1.9-test.1)

`test.1` is the last `0.1.x` preview compatible with the original data types. Later test releases advance a rewrite and may introduce breaking changes. `0.2.0` follows architecture and data-model stabilization and acceptance. Old test.1 data is not imported or overwritten automatically. Back up the library and its original protection file before upgrading. Protection is bound to the Windows user; copying the database alone does not migrate it between accounts.

## Features

- Cards and content: ideas, projects, experiments, favorites, search, checklists and undo after deletion; Markdown editing and previews, with independent attachment copies.
- Rich capture: text, screenshots, files, Office rich text and tables. Complex or proprietary formats may fall back to previews or attachments; full fidelity to the source application is not guaranteed.
- Appearance: frosted, ultra-clear and liquid glass; default, solid, textured and transparent canvases; a theme color wheel and individual component/card settings, with reduced-motion support.
- Media: image, GIF and video backgrounds, local music, lyrics and floating tips. Format support depends on the platform and decoder. Web transparency reveals the host page, not the desktop.
- Layout and languages: responsive content and separate settings pages; Chinese, English, Russian, French, German, Spanish, Japanese, Korean and Portuguese.
- Windows content protection: unified Rust storage, sealed audit records, library snapshots, original-identity backup and recovery, and limits on concurrent use of the same identity.

## This update and validation

test.54 removes repeated full checks during library opening, decodes evidence with bounded parallelism and reuses verified evidence sizes and archive digests within one verification transaction. Every open still performs full verification. The storage format and bundled plugin are unchanged; verification results are not cached across launches.

The final read optimization was compared with the preceding parallel version using four alternating launches each. On the same machine with a roughly 100 MB library copy, median loading-overlay removal fell from 2.222 s to 1.299 s from Dart entry. Registering the copy may warm the file cache; this is not a cleared-cache cold-start guarantee. Core: 568 passing tests; Audit: 97; real-host integration: 2; one existing Core test ignored. See release notes for conditions and limitations.

## Run from source

Requires Flutter 3.44 / Dart 3.12 or compatible versions. Windows also needs Visual Studio C++ desktop build tools, the Windows SDK, Rust and the Cap’n Proto compiler on PATH.

Full Windows Rust workbench build, integration checks and packaging:

```powershell
flutter pub get
rustup target add wasm32-unknown-unknown
pwsh -File tool/build_rust_workbench_windows.ps1
```

Artifacts are immutable: use a new version or a fresh output directory. `-RefreshArtifact` no longer permits overwriting. Distribute the complete runtime directory. Platform documents define Web/Android build and acceptance scope; a Windows build does not qualify those platforms.

Development and basic checks:

```powershell
flutter run -d windows
flutter run -d chrome
flutter analyze
flutter test
flutter build web --no-web-resources-cdn
```

## Architecture, SDK and next steps

The target architecture combines Flutter/Dart UI, a portable Rust core and replaceable plugin execution backends. Runtime boundaries use fixed contracts; persistence and application-owned exchanges use Protobuf + LZ4. C/C++/Rust SDKs and declarative plugin UI are in development. TS/JS plugins are not supported; dynamic Dart plugins are not required.

The complete SDK is not frozen. Managed HTTP/HTTPS requests, limited API service nodes and TLS identity management are connected. Cross-restart reconciliation of Unknown outcomes, the full filesystem, three-language IO SDKs and cross-platform qualification remain open. Library opening still scans all history; a full read/compute pipeline, historical tiers and automatic cleanup are not implemented.

## Documentation

- [Release notes and validation](../../reports/0.1.9-test.54-release.md)
- [Development board and next tasks](../../docs/DEVELOPMENT_BOARD.md)
- [Architecture roadmap](../../docs/FUTURE_ROADMAP.md)
- [Feature migration and capacity limits](../../docs/TEST1_RUST_PARITY.md)
- [C/C++/Rust SDK](../../sdk/README.md)
- [Plugin UI design](../../docs/PLUGIN_SDK_AND_UI.md)
- [Rich capture and format support](../../docs/RICH_CAPTURE.md)
- [Android builds](../../docs/ANDROID.md)
- [Rename compatibility](../../docs/RENAMING.md)
- [Historical README and milestones](../../docs/history/README-before-test54.md)

Detailed designs and validation reports are currently mostly in Chinese. Each README provides the same current-version entry point; translations have not yet received comprehensive native-speaker review.

## License

From `0.1.9-test.2`, first-party code, SDKs, documentation, configuration and assets use **AGPL-3.0-only**. test.1 and earlier releases retain Apache-2.0; third-party content retains its own licenses. Prebuilt packages include licenses, copyright notices and access to corresponding source.

[AGPL-3.0-only](../../LICENSE) · [NOTICE](../../NOTICE) · [Third-party licenses](../../packaging/THIRD_PARTY_NOTICES.txt)
