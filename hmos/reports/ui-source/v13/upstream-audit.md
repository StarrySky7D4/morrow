# dev.13 source follow-up audit

Observed 2026-10-07 03:04 UTC / 11:04 Asia/Shanghai. Repository root: `C:/Users/Administrator/Desktop/CodeXProjext/morrow`. This is a read-only audit of three local Flutter/Rust reference worktrees. This source/API audit stage wrote only the two new dev.13 audit reports. It did not modify the reference worktrees, fetch remote refs, operate a device, build packages, send source-thread messages, commit, or push. A later separately authorized emulator recovery is recorded in `environment-audit.md` and does not change the source findings.

## Local reference versions

| Worktree | Branch | Current HEAD | Flutter version | Last commit |
| --- | --- | --- | --- | --- |
| `build/io-safety-refactor` | `codex/m03-stream-revocation-backpressure` | `925fb8ca6563dcb7de38db8fbdf6b005f9ad5420` | `0.1.9-test.57+61` | 2026-10-01T20:33:39+08:00 |
| `build/win-cloud-20261005` | `codex/windows-sdk-convergence-20261005` | `e83cdf1d3506b001d07ffceaaa9f98b688bcc150` | `0.1.9-test.58+62` | 2026-10-06T00:48:29Z |
| `build/windows-sdk-reconstruction` | `codex/windows-sdk-qualification-20261003` | `20669f671152972470340a65eac3458dd2f61b4d` | `0.1.9-test.58+62` | 2026-10-04T23:59:22+08:00 |

Compared with the dev.12 audit, the Windows convergence worktree advanced from `772466177fe589cee53bc633e69f411c34610104` through `679e16a6` (directory selection and independent plugin request SDK) to `e83cdf1d` (Codex session and safe-execution R2 SDK). Its Flutter version did not change. `git diff 77246617..HEAD -- lib pubspec.yaml plugins/workbench/src/capture.rs` is empty. This does not assert that the whole SDK or all worktrees are clean or qualified.

The audit checked the following 17 source paths plus `pubspec.yaml`: the scoped `git status --porcelain` was empty in each reference worktree; each current working-file Git blob matched its own HEAD; all 17 blobs matched across all three worktrees. No attachment/Markdown/editor design drift was found in those inspected paths. Local HEAD checks are separate from remote publication and in-progress source-thread work.

## Inspected source blobs

Paths are relative to each reference worktree. These are Git blobs, not raw-file SHA-256 values.

| Source | Shared blob |
| --- | --- |
| `lib/main.dart` | `2e00f41251c3215f0b6d886712c86adfdf2bb6e6` |
| `lib/attachments/attachment.dart` | `751b45933d25bfcd32b779885c104eea67c87ecc` |
| `lib/attachments/attachment_view.dart` | `76be11b131c15d8c4c101d15f6d9568400eca3c8` |
| `lib/attachments/clipboard_import.dart` | `f23ff12b4446ac0e984ea04aab8424be21a018ab` |
| `lib/attachments/office_clipboard.dart` | `24f54bee34f7a1902a0cf45811e1d35cd07d66a8` |
| `lib/attachments/file_access_native.dart` | `3116024784acf5e9b1a93cd2a5306fc0a93acbbe` |
| `lib/content/rich_content.dart` | `3c53039a1a863ea92d34d8b1e5ec4f03a4938044` |
| `lib/content/idea_markdown.dart` | `165a6f789755db519ca567ca847ce961f9cc1add` |
| `lib/plugins/editor_draft_binding.dart` | `66f483cfbc0307bc61d6d82f11bf334e208ca3b2` |
| `lib/plugins/editor_draft_session.dart` | `bc29015a1f61d93a5129031e669c0634fc7a09a4` |
| `lib/plugins/editor_draft_workspace.dart` | `20c97efc7d54ad9fd1dae647ad8d6af6d47ba677` |
| `workbench_host/schemas/editor_draft.proto` | `73e89a22ad83995ec066e6429ecba7033d609fc9` |
| `workbench_host/src/editor_draft/model.rs` | `5a1c409b23315ec44c930669f7d6145d3ce333be` |
| `workbench_host/src/editor_draft.rs` | `edbaf83383d09d1559062a4da9aa4e1e09077974` |
| `workbench_host/schemas/editor_draft_staging.proto` | `8b0aa7a53c3fd44aeaf6b5af1ff36f75cd8a6084` |
| `workbench_host/src/editor_draft_staging.rs` | `5bbd3d7fed9f7170c845752344de8d5a8043fc5a` |
| `workbench_host/src/versioned_record.rs` | `24c7b527f25338b26a2f022037b4ac4bcc1d6be4` |

Raw SHA-256 values for the active `io-safety-refactor` UI inputs:

| Source | SHA-256 |
| --- | --- |
| `lib/main.dart` | `6203B7465F320DC8463DFACB5387EA8B437EC3EC1458CCA7E215E44B8F65CB1E` |
| `lib/content/idea_markdown.dart` | `816AC4C1B3A04149A51C5A1453C2F5F17B5E0DD750A74D9A27571CE5F68BA9E0` |
| `lib/attachments/attachment_view.dart` | `F3BAA59202D433DEB1683F77932B31A210FB4EE5D382AD0A8ECFEB2A4DEC0FE4` |
| `lib/attachments/attachment.dart` | `8BBEB6A11AD7854B32EBD0C0E5C0EA2EDE9D5CDAC74E1D2DE9B7966BFA0FAE1F` |
| `lib/attachments/file_access_native.dart` | `3AE2D37173F56E67AAA572D44AB2F4C197CA0EB503FDAEDC41410E07B8FD474A` |

The Windows convergence and qualification trees use LF for some of those files while the active source uses CRLF. For example their `main.dart` raw SHA-256 is `2CB2A519E31AC982D3A8638EB7DE95FE63D5421ED3D1B6ACDA507CD142169F06` but Git working/HEAD blobs still agree. Raw-file differences must not be labeled design drift without checking this distinction.

The four source SHA-256 declarations in `hmos/rust/editor-draft-reference.json` and the three in `editor-draft-staging-reference.json` still match the actual active source files. The two model/schema copies and the staging schema copy declared byte-for-byte still match. The HMOS adapters are intentionally adapted implementations, not identical protected Windows storage hosts. The audit did not replace the frozen shared-source snapshot.

## Flutter behavior to preserve in dev.13

The following locations are in `build/io-safety-refactor`; the other two inspected trees have the same source blobs.

| Source behavior | Exact source location | dev.12 gap / recommended next implementation |
| --- | --- | --- |
| Markdown `attachment:` images decode URI path segments and join with `/`, then match selected attachment display name, location, or `pluginId`. Only image/GIF kinds qualify; the first eligible match is chosen. Reading starts when the inline component mounts, with a 40-high loading area, error text, and max height 300. | `lib/content/idea_markdown.dart:61`, `:131`, `:143`, `:163`, `:177` | Resolve against selected known assets only, use the existing verified Rust export stream, isolate each read by source/draft context, release temporary resources on disappearance. A Markdown URI must never become an arbitrary filesystem path. Restore missing/failed image feedback. |
| Remote `http`/`https` images require explicit user click; local or unsupported links remain unavailable. | `lib/content/idea_markdown.dart:101`, `:116` | Preserve existing HMOS explicit remote-image consent while adding local selected attachment images. |
| Attachment previews provide reading/loading feedback, media failure feedback, image decode failure feedback, and `InteractiveViewer` for image zoom/pan. | `lib/attachments/attachment_view.dart:141`, `:151`, `:191`, `:199` | Add native image scaling/panning, reset, and decode/read failures to current verified private-file preview. Treat ArkUI callbacks and stale image identity independently from verified payload bytes. |
| The same attachment row has export in both read detail and editor; editor rows add removal. | `lib/attachments/attachment_view.dart:101`, `lib/main.dart:5857` | Editor row export can reuse exact-generation draft export; unconfirmed/raw attachment changes must be acknowledged without overwriting other input before export. |
| File import loops over multiple files up to 20 and fills an empty title from the imported name, capped at 60 code units. | `lib/main.dart:5439`, `:5461` | Multi-select and title autofill remain an explicit follow-up; cancellation must not create an owner or raw journal. |
| Ordinary source attachment limit is 200 MiB; audio/video preview uses MediaKit, initially paused. Generic external opening checks blocked executable/script suffixes and routes them to export. | `lib/attachments/attachment.dart:28`, `attachment_view.dart:151`, `file_access_native.dart:33` | HMOS preparation/pin budget is still 64 MiB; audio/video lifecycle and default external opening require separate adaptation and runtime acceptance. Existence of local SDK declarations is insufficient. |
| Clipboard imports can combine text/Markdown with files and insert `attachment:` references for image-only paste. | `lib/main.dart:5596`, `:5638`, `lib/attachments/clipboard_import.dart` | Rich HTML/Office/clipboard image-file capture remains open. Do not claim file-picker import covers clipboard asset authority. |

This audit is an implementation target list, not an assertion that in-progress dev.13 changes already pass. Root records final build, tests, package hash, UI screenshots, and device scope separately in `validation.md`. Wide layouts, full theme/IME matrix, production HUKS/identity/lease/audit/backup, captured S1/S2 and formal business Unknown cross-process recovery, signing, and ARM64 real-device qualification remain broader parity requirements.

## Source-thread snapshot

One read-only immediate `wait_threads` check with the prior cursor returned a cursor reset and current state:

- Thread `01a085bd-7a94-7f93-8a1f-1ecf417f5ee3`: active; turn `01a11447-5538-7513-b2bf-9cbc56a7d463` in progress. Current commentary identifies Windows ordinary-pipe/ConPTY read and wait failures potentially being mistaken for output completion/exit `-1`, and plans fault propagation repairs in a new SDK copy while preserving the original SDK/Linux source.
- Cursor: `652321f3-8157-4672-ab76-c1c1442eb0fe:1`, revision 1. Latest tool marker was completed; the source turn itself was still active.

This snapshot is not the outcome of those repairs and does not change HMOS qualification. No message was sent to that thread. The environment thread was not re-polled by this audit; the installed SDK was examined directly for the platform report.
