# v24 fixed UI candidate: existing draft restoration and retain-close

The bounded device result is **PASS** for restoring the existing task-owned `HMOS-todos-20261007-D` draft and closing it with **保留草稿** on the B62 UI-only candidate. The final captured view is the application Home page: the editor is closed, the UI still reports **3 drafts / 14 cards**, and the retention confirmation is visible. The driver records **0 business submissions**. This is not qualification of the current v24 intent backend or `EditorBusinessSession`, new input, business saving, or full Flutter parity.

This report was prepared by reading the existing evidence and viewing its PNGs. It does not introduce another device action, installation, build, or replay.

## Exact candidate and installation boundary

| Item | Recorded identity / result |
| --- | --- |
| Baseline | `7f07d07480660bbc868369b9565dc98a47e8af08` |
| Candidate | `hmos/.build/artifacts/dev24-todo-initial-fixed/entry-default-unsigned.hap` |
| HAP bytes | **29,107,139** |
| HAP SHA-256 | `B62DBD231F188D60DED72AB10BEEBBB76E43694D7D67DE426729E95FF3F45B10` |
| API26 SDK build | **BUILD SUCCESSFUL, 14.630 s**, 34 executed tasks; unsigned debug package |
| Bundle version | `dev.morrow.hmos`, `0.1.0-hmos-dev.19`, `1000019` |
| Device endpoint / active ABI | `127.0.0.1:5555` / `x86_64` (`libs/x86_64`) |
| Installation record | `INSTALLED_AND_LAUNCHED`, completed `2026-10-07T11:53:18.581Z` |

The archive's bytes and SHA-256 were read again for this report and match [artifact.json](ui-fixed/artifact.json). [hap-build.log](ui-fixed/hap-build.log) supplies the exact build duration. [source-copy-manifest.json](ui-fixed/source-copy-manifest.json) records 312 candidate inputs: baseline source plus the two frozen UI repairs, with the unchanged v22 native archives. The repaired `Index.ets` is 285,929 bytes / `AFF77EB7F1C90E53DDF5D64DD89FB3A037DAB98AC0B08AE0382090A2055EFEAF`; `EditorTodos.ets` is 24,901 bytes / `83B6C6A5C5811EAD9FA14EBB23F9B5DEA7F9A858FD0B5461181091E929473836`.

The manifest's native provenance is `dev22-business-checkpoint`: arm64-v8a archive 56,104,946 bytes / `F3815F306E96618F8389B17E59679CDD1E1E477BC1324BE34943B579DBF2DAA4`, and x86_64 archive 54,511,556 bytes / `5ADCB7BAD62417868DD6A2D03699B9D69C3267DE1072B8534E0926FE30DE6B41`. The current new intent backend/session is not adopted by this candidate and was not exercised by these stages. The arm64-v8a archive's presence is not an ARM device result.

[candidate-install.log](device-fixed/candidate-install.log) contains successful installation of the exact archive path. [installation.json](device-fixed/installation.json), [bundle-after-install.json](device-fixed/bundle-after-install.json), and each stage's saved/fresh bundle observation agree on the version above. This establishes host archive identity, an acknowledged exact-path installation, and observed bundle version/ABI. The install log has no device serial, and the bundle dump exposes no installed-byte hash; **byte-for-byte device-installed package identity remains unproved**.

Two historical metadata details are retained as recorded: `artifact.json` has `installed:false` because it was written before installation; `installation.json.sourceManifest` points to `ui-diagnostic/source-copy-manifest.json`. The latter pointer is not used as the fixed candidate's source proof here; the matching B62 artifact and actual `ui-fixed` manifest are the source references.

## Observed stages and visual check

The authoritative stage record is [progress-clipboard.json](device-fixed/progress-clipboard.json). No failed or unknown earlier candidate stage is rewritten by this result.

| Stage | Existing recorded result | Scope checked |
| --- | --- | --- |
| `v24-echo-fixed-restore-D` | **PASS**, `11:53:18.682Z`–`11:53:30.493Z` | Entered the draft list and restored D. One enabled todo row was observed with exact text `first 汉字 🧪 é.` and ID `editor-todo-499e4692-88e1-4513-9d2b-c2e8e466cf43-todo_1`; the UI reports **13 / 1000**. |
| `v24-echo-fixed-keep-D` | **PASS**, `11:53:30.576Z`–`11:53:38.148Z` | Clicked the visible **保留草稿** control. The subsequent observation records `closed:true`, `retainedDraftCount:3`, `businessSubmissions:0`, and no editor todo rows. |

The [restored-row PNG](device-fixed/v24-echo-fixed-restore-D-row-seek-0.png) was viewed: the todo section shows one row, the Chinese/emoji/combining-character text, **13 / 1000**, and **草稿已保留**, without the missing-capture error that blocked the earlier candidate. This count is the displayed grapheme count; it is not a claim that the string contains 13 UTF-16 units. The [keep-stage title PNG](device-fixed/v24-echo-fixed-keep-D-title-seek-0.png) identifies `HMOS-todos-20261007-D` and shows its restored body and retained state. The [after PNG](device-fixed/v24-echo-fixed-keep-D-after.png) was viewed and confirms Home, **草稿 3**, **最近的念头 14**, and **已确认的完整输入已保留，可从草稿入口继续。**

Those draft/card totals and zero submissions are stage/UI observations, not an independently enumerated Store transaction census. The latest evidence ends at Home with D closed and retained; this report performs no later device read or second reopening.

## Evidence byte identities

These are SHA-256 hashes of the files read for this report, without changing the originals.

| Evidence | Bytes | SHA-256 |
| --- | ---: | --- |
| [Fixed source manifest](ui-fixed/source-copy-manifest.json) | 135,612 | `4A91FFF3AF6FB60DAD046B1E6295D2F0B1BCF2AA4B626B553189EBD04AF96F59` |
| [Fixed artifact identity](ui-fixed/artifact.json) | 805 | `47E66286E84E630AF5F4B0627D36DCA9770709F6243AF0D58A1EB0D7DD74E28F` |
| [Fixed SDK build log](ui-fixed/hap-build.log) | 24,950 | `395121DA6CEA3D62921FAABD4BF43D8D1D41683AC896841EE307CFAE63AC1115` |
| [Installation record](device-fixed/installation.json) | 1,769 | `E0118B8A5BBCAE8AE7B87622B40B1AB7EAA48797F7D7B7431AD8972910AD55BB` |
| [Exact-path install log](device-fixed/candidate-install.log) | 198 | `AC1FDEB6D83D3C4BE3D1F549FB768BDF67D5224767A92346385F7966E0126495` |
| [Saved bundle dump](device-fixed/bundle-after-install.json) | 18,461 | `4DACE0C89B64C88E161FEE08DDAA9A72BFA341DEDA2E8A68C9C3CBDA9461393E` |
| [Stage progress](device-fixed/progress-clipboard.json) | 52,792 | `2C39841B2F7B856C52B38AC42F514D774825DA5EB39C3A9763B8F7A70D832637` |
| [Restored-row PNG](device-fixed/v24-echo-fixed-restore-D-row-seek-0.png) | 2,026,804 | `C312E2C2CD2BB885D24B502B807C8620AA86A74BCBAAD0BC571B63960D70CE84` |
| [Closed/Home PNG](device-fixed/v24-echo-fixed-keep-D-after.png) | 1,763,248 | `070C42E2D95F1A6D0F08F4F0DC89586AF4A49FF58DCC61B50EB4F119CE0F85AC` |

## Remaining qualification

**OPEN / NOT_RUN in these stages:** adding/removing/reordering multiple todo rows, fresh ordinary/todo input and IME composition, business save and its late-input continuation, current intent/session integration and runtime recovery, new backend device execution, ARM hardware, installed-byte measurement, and full Flutter UI/function parity. These observations also do not prove SDK callback queue drainage or general lossless IME behavior; the bounded initial-display echo rule and its remaining origin-token limitation are documented in [todo-initial-input-review.md](todo-initial-input-review.md).

The publication's current full-model/native/product-build evidence belongs to its separate final package and reports. It must not be substituted for this B62 candidate's limited device result, or use this device result as validation of the new backend.
