# Development raw recovery fork: native source and Store evidence

2026-10-07. This audit records the worker's source/host qualification checkpoint for the isolated `development-unsealed` database. At that checkpoint the new native fork had not been built for OHOS, packaged, installed or exercised on a device. The root agent subsequently built and adopted new dual-ABI archives and a product HAP, recorded separately in [the final build validation](validation.md); that new package remains uninstalled. Earlier dev20 archives predate this protocol. Full product acceptance remains OPEN.

The implementation preserves a late complete raw snapshot and an explicitly selected subset of already confirmed parent pins under a new draft identity. It does not reactivate an inactive identity, adopt a newer business card, convert a local receipt into capture evidence, or accept protected `ParentLink`/`ParentRetirement`. Ordinary fresh/save/origin rules and the original model validator remain unchanged.

## Frozen wire

`action: 'draft_fork'` accepts `fork.child` in the existing Write JSON shape plus `parent_draft_id`, `parent_generation`, `parent_save_operation`, `parent_request_sha256`. Generation is canonical decimal u64; SHA is exactly lowercase 64 hex. The first child has expected generation zero, its fixed first operation, and origin4 selections. The native canonical protobuf request SHA, not a JSON hash or a client-generated capture ticket, binds the parent.

`action: 'draft_fork_retire'` accepts `fork_retirement` with `card_id`, `child_draft_id`, `child_operation`, the same complete parent proof, and a fixed `operation_id` for conditional retirement. Exact retries reuse this envelope and operation.

View adds `request_sha256`, `fork_link`, `fork_retirement`. `fork_link` has schema1, parent draft/generation/save operation/request SHA and fixed child first operation. `fork_retirement` has schema1, child identity, original retirement operation and that same full link. Historical ACKs retain their original complete raw values, generation and request SHA alongside `current_generation/current_active`; they never rewind current raw or reactivate a retired child.

The protobuf extension uses independent `DevelopmentForkLink` and `DevelopmentForkRetirement` at Slot fields13/14. Original protected fields11/12 continue to be rejected. The existing build script regenerates prost bindings from the edited local schema. No dependency, lockfile, default feature or release feature change was needed.

## Authority, transaction and budgets

Before first fork, the parent must still be active and its complete current Slot must equal the immutable original parent save record. Card ID, source kind/revision, generation, save operation and canonical request SHA must all match. The child inherits the parent's exact full `source_card`, including unknown properties, rather than reading a newer live business card. New-card kind1/revision0 remains new-card with empty source, even if business work advanced independently; that later business conflict is explicit.

Origin4 resolves only to exact parent-selected pins, matching identity, aliases, name, media type, bytes and SHA. The child selections must be a subset in parent order. Unconfirmed alias/reorder changes must be separately confirmed or rejected; this API does not silently substitute aliases or reorder. Every chosen parent pin is streamed through Core verification before one authorized child journal transaction binds all selected pins with the complete raw snapshot. Validation-only origin2 substitution invokes unchanged shape/byte/alias rules; it is never stored and grants no staging authority.

Consumed parent imports reconcile through their existing staging protocol before fork. Independent unselected Pending or Ready imports block the entire fork; only explicit existing abandonment/reconcile can resolve them. The fork does not reread their original URI, invent origin0 assets, or delete them as a side effect of child selection.

Once the child journal is durable, its immutable first link freezes parent new saves/imports/ordinary discard/second fork. Successor discovery includes inactive children, so child discard cannot enable a second child for the same parent. Child later saves preserve the original link and adopt current own pins as origin3. Child ordinary discard or another fork requires the parent's exact conditional retirement marker, not mere inactivity.

Parent retirement proves the child's original first operation and full link through durable immutable history, allows current child generation to be later, requires it still active, rechecks parent complete current Slot/CAS and unresolved imports, and verifies current child pins. It then marks the parent inactive and releases parent current pins in one ordinary authorized EditContent transaction. Post-commit cleanup has a separate effect. Ordinary discard cannot reinterpret the conditional-retirement operation as its own ACK.

There are two transactions: durable child first, then conditional parent retirement. The caller must serialize its HostRuntime and retain the original envelope across Unknown. This is crash-safe ordering, not cross-process atomic handoff or protected-storage proof. Exact committed history preserves `effect=committed` on subsequent payload/readback failure; Core transaction failures retain existing Unknown/rejection classification.

| Existing limit | Fork behavior |
| --- | --- |
| 16 active main drafts | Parent and child both count before retirement |
| 64 MiB logical main draft bytes | Selected blobs charged to both active owners even with physical deduplication |
| 256 cumulative main journal identities | Inactive journals still count; no identity reuse |
| 512 KiB each raw field / 4 MiB main body | Complete raw/metadata must fit; no truncation |
| 20 unique assets / 8 aliases of 16 KiB each | Original validation remains in force |
| Existing staging 20/320 items, 64 MiB, 1 MiB body, 256 revisions | Original reservation and cleanup rules remain in force |

## Fresh validation

`cargo test --manifest-path hmos/rust/Cargo.toml editor_draft::fork -- --nocapture`: 18 PASS, 0 FAIL, 1 explicit conditional ignored. See `native-fork-tests.log`. The retained `native-fork-tests.initial-fixture-failures.log` is an earlier fixture-only failure record, not acceptance evidence; final default validation also executes all 18 ordinary tests from the final source.

The focused tests use actual Core Store files, HostRuntime grants and production entrypoints. They cover full raw/UTF16 selection/composition, reopen, exact history with newer/current/inactive state, actual uncommitted attachment bytes/SHA export, child origin3 updates, parent mutation freeze, selected aliases/order/subset authority, unresolved Pending/Ready imports and explicit abandonment, proof/CAS rejection, grant expiry, committed payload mismatch, EventCapacity Unknown followed by the exact original retry, independent live business source advancement, protected-proof rejection and actual Engine JSON dispatch/restart.

Capacity tests really import a 33 MiB blob and prove copying its selected pin to a second active draft fails the 64 MiB logical budget without changing parent bytes. A second fixture creates and retires 255 distinct journals beside the parent, reopens, and proves the child is rejected at the 256 cumulative identity limit. The active-slot and field-size boundary tests remain independent.

`cargo test --manifest-path hmos/rust/Cargo.toml --features morrow-core/fault-injection --lib editor_draft::fork::tests::actual_store_crash_boundaries_preserve_pins_and_exact_original_outcomes -- --exact --ignored --nocapture`: 1 PASS, encompassing 14 actual subprocess exit86 cases. See `native-fork-store-crash-tests.log`. Both fork and retirement are interrupted after begin/card/operation/event/task-and-blob binding, before commit and after commit. Each case reopens the database, checks authoritative original operation history and current parent/child state, retries the same frozen original request, exports all fixture attachment bytes with SHA, and runs Store integrity checking. The feature is explicit host-only test invocation; it is not enabled for normal builds.

The final default `cargo test --manifest-path hmos/rust/Cargo.toml` completed successfully: library141 PASS / 0 FAIL / 7 explicit conditional ignored, attachment-check binary3 PASS, self-check binary0 cases and doc-tests0 cases. The combined default total is144 PASS / 0 FAIL / 7 ignored. See `native-default-tests.log`; the existing large RTF capacity case completed despite the over60-seconds progress notice. All 18 ordinary fork tests were executed from the final source in this combined run. The source inventory was hash-checked again after completion with no drift.

`native-fork-inputs.json` records exact source lengths/SHA, including new module/tests, bridge/dispatch/guards/proto and unchanged original model/Cargo inputs. This is a source inventory, not a native archive manifest. No NDK/HAP/Git/device action was performed by this subtask. Index and final product wiring are outside this audit; source/host checks do not establish UI, IME restoration, system picker, protected capture, signing or runtime acceptance.
