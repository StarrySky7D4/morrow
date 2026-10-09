# v25 development business handoff and exact close

2026-10-09. Implemented against published `08fcea98` and the v23 handoff design. This worker changed only the nine native production files and one new test file listed in `editor-handoff-native-inputs.json`; no ETS, Index, dependency graph, Git, NDK adoption, product build or device action was performed by this worker. The complete HMOS parity goal remains OPEN.

## Implemented behavior

`draft_continue_business` accepts `business_handoff:{request_json:<complete exact Handoff literal>}`. The schema 1 literal contains the immutable intent proof, exact active parent draft proof, full first-child `Write`, fixed `plan_operation` and fixed `retirement_operation`. The native host validates the actual original business command/receipt against the immutable prepared candidate and both original wire/publication hashes. A current Card is never the source fallback. The child uses source kind 0 and the complete accurate historical result Card at the committed revision.

The parent is checked against its immutable save, complete source and canonical request digest. Ancestry is bounded to 256 identities and follows specific incoming raw/business links and actual corresponding conditional parent retirements; the exact original publication is the stopping point. Same card/revision from a different session is insufficient. An incoming linked parent itself must have completed its prior conditional retirement before a new handoff or exact-close plan.

First-child assets use origin 4 and are an ordered subset of the current confirmed parent inventory. Aliases, name, MIME, length and digest are preserved and blob bytes are verified. Independent Pending/Ready imports block the plan. Already-consumed import cleanup uses the existing staging journal; it has an independent effect. The original staging implementation was not changed.

The complete Handoff and a native canonical fixed Retirement literal are committed to the intent before child creation. Active plan pins retain exactly the selected first-child inventory. The immutable S1 asset metadata remains in the prepared record; the original business already has the strictly committed published references. The logical charge retains the original request and all required metadata and counts each currently retained snapshot's blobs separately. Plan and prospective child count/identity/combined bytes are preflighted before parent freeze; the original 16 active / 64 MiB logical active / 256 cumulative identity limits are unchanged.

New incoming `DevelopmentBusinessLink` and outgoing `DevelopmentBusinessRetirement` occupy Slot 15/16. Protected Slot 11/12 and raw fork 13/14 retain their meanings. Incoming 13 with outgoing 16, and incoming 15 with outgoing 14, are supported and exercised through real Store transactions. Two incoming links or two outgoing retirement markers are rejected. Ordinary child saves retain incoming 15 and adopt existing selected pins through origin 3. Parent ordinary save/import/discard and another handoff/fork are sealed after a durable plan. A child cannot be ordinarily discarded before its exact parent retirement.

`draft_continue_business_retire` accepts only the complete original native-persisted Retirement literal in `business_retirement:{request_json:...}`. It checks the first-child immutable save, current child active source/link and verified pins, exact active parent Slot, fixed plan/link and resolved imports. Its one parent CAS writes 16 and releases parent pin references. Historical retry returns the original retirement and current flags without reactivating an inactive identity.

`editor_intent_close` retains the v24 `cancel_prepared` route. It additionally accepts schema 1 exact literals with `expected_generation`, fixed `operation_id`, disposition `saved_exact` or `handoff_retired`, `parent:null|{proof,discard_operation}`, and `plan_operation`. `saved_exact` compares every complete TextValue (including selection/composition), source and ordered selected inventory/aliases/metadata against original publication; changed S2 cannot use this route. It first durably stores a close plan, then performs the exact fixed parent discard, then closes the intent. `handoff_retired` verifies actual first-child and fixed parent retirement history, retains a current active child's pins, and releases only intent plan references. Closed contexts retain original wire and plans and never reactivate old draft identities. Fresh current-card reopen still uses ordinary current Card authority.

Intent phases/generations are:

| Route | Durable state |
| --- | --- |
| prepare / issue | prepared 1 / issued 2 |
| cancel prepared | closed 2 |
| handoff plan | handoff_planned 3 |
| saved_exact | close_planned 3 / closed 4 |
| handoff_retired | close_planned 4 / closed 5 |

Intent views retain all v24 fields and add string `handoff_request_json` and `retirement_request_json`. New read parts `handoff` / `retirement` expose their corresponding complete exact literals; the original submission/publication/save/inspect/close parts remain available. Ordinary draft views add nullable `business_link` / `business_retirement`. The complete field shapes are captured in the real Store DTO fixture, including accurate historical sources, request digests and current flags.

## Outcome boundaries

Plan, child, parent retirement, close-plan, exact discard and final intent close are separate Core transactions. This is not a multi-object atomic protocol or global cross-process quota reservation. Same intent plans compete on one generation-2 CAS, and each actual mutation uses a fixed operation and its actual expected revision.

The handoff action's effect/receipt describe the first-child operation; the retirement action's describe the parent retirement; the close action's describe the final intent close. Internal plan and cleanup effects never substitute for these requested operations. A lost response is reconciled with the same immutable literal and original operation; list/read and phase metadata do not automatically resend business. Historical first-child responses carry their original raw and the separately observed current generation/active flag; they do not overwrite newer child raw.

All embedded and outer JSON remains bounded at 512 KiB; draft/model/body/field/asset limits and existing business/Task budgets are unchanged. Complete success and each restore part are preflighted before the new durable mutation. No partial source/raw/assets reply is called an ACK.

## Exact final evidence

`editor-handoff-native-inputs.json` and `editor-handoff-native-inputs-after.json` contain 15 byte-identical inputs: nine production changes, one new test file, four unchanged model-build/model-library/Cargo graph inputs, and the complete real DTO fixture. The key final identities are:

| Input | Bytes | SHA-256 |
| --- | ---: | --- |
| `rust/src/editor_handoff.rs` | 49,593 | `064227172B29B601D42F5466E9AD5EF76462A37BFD5FBCB1931FF155C161A10E` |
| `rust/src/editor_handoff/tests.rs` | 44,550 | `7827FAA68E31216BEDD38A9652A781E690E7BFB4E1BA93925CC10B0FD91E3A3B` |
| `rust/src/editor_intent.rs` | 39,769 | `A0677D1488489C8A087C8735B51E4DA954FC37AA509A8BBD7AA926120FEC94AB` |
| `editor-handoff-store-fixture.json` | 91,081 | `9AF7CCAABAF247BBB06E60CD7302DA878F515BAA3704495735542BDB783E486F` |

| Final evidence | Result and scope |
| --- | --- |
| `rust-tests.log` | 177 library PASS, 0 fail, 13 conditional ignored, 87.90 s; attachment binary 3 PASS, 7.76 s; self-check 0. The whole command exited 1 only in subsequent rustdoc with E0463 after a concurrent fault-feature build used the same host target. This original failure is retained. |
| `rust-doc-final-tests.log` | Sequential default rebuild repaired rustdoc without source edits: exit 0, 0 doctests. This separate repair completes doc compilation; it does not rewrite the prior command exit. |
| `editor-handoff-crash-final-tests.log` | 1 PASS / 48 actual subprocess Store fault vectors / 13.81 s / exit 0, under explicit host-only `morrow-core/fault-injection`. Eight independent transaction stages × six actual Core boundaries, including abrupt exit after commit, reopen and exact fixed retry. |
| `editor-handoff-dto-final-tests.log` | 1 actual Store exporter PASS / 0.57 s / exit 0. Both handoff-retired and saved-exact contexts include all seven pre-transition and closed IntentViews and complete actual strict original `editor_commit_inspect` replies. Fixture reproduced the identical frozen hash. |
| source whitespace check | PASS for current `hmos/rust` diff. |

The twelve new default Store test groups cover complete S1 business → raw S2 source0 child → raw S3 and inactive historical retry; full TextValue exact-close refusal; changed literal/second identity and parent mutation/import/discard refusal; publication/rawfork ancestry; independent Ready ownership; actual late-pin order/aliases/bytes after retirement and close; combined active count refusal before plan; historical business source despite newer current Card; strict proof/schema/reply bounds; competing close/handoff intent CAS; actual continued S2 one business transaction preserving true existing TaskIds; and actual 31 MiB parent/plan/child logical charge refusal before freeze with parent bytes retained. Both mixed incoming/outgoing 13/16 and 15/14 chains are verified in these groups.

`editor-handoff-native-inputs-before.json`, `rust-tests-before-final-cleanup.log`, `editor-handoff-tests.log`, `editor-handoff-crash-tests.log` and `editor-handoff-dto-tests.log` are historical intermediate evidence. They are not substitutes for the final hashes/logs above. Earlier scratch test failures were fixture clock/envelope/source mistakes; final production rules were not weakened to accept them.

## Remaining OPEN

Root owns final native ABI archives, actual product SDK build, UI/Index integration and device qualification. This worker's host checks prove neither OHOS runtime/signing/ARM64 real-device behavior nor lossless SDK input-queue delivery. Owner lease/epoch and actual capture cutoff remain frontend responsibilities. Cross-process simultaneous distinct-object quota admission and full multi-object atomic settlement are not claimed. Current-revision V2全文编辑 after TaskId rename/completion/favorite/category changes, full Flutter/Windows UI parity, HUKS/protected storage/audit and complete product acceptance remain OPEN.
