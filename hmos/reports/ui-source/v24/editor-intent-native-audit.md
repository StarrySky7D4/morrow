# Durable editor intent: native foundation, v24

The development-unsealed native foundation is implemented and source-frozen. It durably retains a complete original editor Submission and its exact prepared publication before business mutation, then separately issues immutable transport plans. A prepared request can be conditionally cancelled; an issued request cannot. Neither journal phase nor metadata receipt proves that the business command committed. Business qualification still requires the original Core command, receipt, historical result, wire SHA, publication SHA, and full content validation in `editor_business`.

This is an unconnected foundation: no Index integration, source-kind-0 business successor, handoff fields 15/16, qualified `saved_exact` closure, detail-operation re-entry, new native ABI archive, HAP adoption, or device acceptance was performed by this work. Those remain OPEN. The previously implemented strict business projection and its independent backend/TaskId rules remain in place.

## Native actions and immutable proof

All IDs and generations are strings on the JSON boundary. SHA fields are complete lowercase 64-hex. The proof always names immutable prepared generation 1, even when the current metadata generation is 2:

```json
{"intent_id":"...","prepare_operation":"...","generation":"1","prepared_record_sha256":"...","request_sha256":"..."}
```

| Action | Dedicated envelope |
| --- | --- |
| `editor_intent_prepare` | `editor_intent:{operation_id,request_json}`; `request_json` is the complete frozen original Submission literal |
| `editor_intent_issue` | `editor_intent_issue:{intent:<proof>,expected_generation:"1"}` |
| `editor_intent_read` | `editor_intent_ref:{intent_id,prepare_operation,part}`; part is `submission`, `publication`, `save`, `inspect`, or `close` |
| `editor_intent_list` | `editor_intent_query:{after,limit,card_id}`; limit 1..16, optional filter represented by empty card ID |
| `editor_intent_close` | `editor_intent_close:{request_json:<complete frozen Close literal>}` |

The Close literal is `{schema_version:1,intent:<proof>,expected_generation:"1",operation_id,disposition:"cancel_prepared"}`. Its operation cannot equal the prepare, business, publication-save, or deterministic issue operation. Exact close retry remains valid after cancellation; a changed close literal does not.

Schema 1 issue derives, explicitly sorts object keys, compactly serializes, and durably stores both complete transport literals:

```json
{"action":"editor_save","editor_save":{"intent":{},"request_json":"<original Submission>"}}
{"action":"editor_commit_inspect","editor_commit":{"expected_revision":"<native prediction>","request_json":"<original Submission>"}}
```

The first line's actual proof replaces `{}`. Consumers obtain the real persisted literals by part reads and send them verbatim. They do not infer object order, reconstruct the outer save, or embed the later plan in the first record SHA. The exact registered outer save is independently checked, including whitespace and the entire inner Submission.

The intent ID domain is `morrow.hmos.editor-intent.v1\0`, followed by the card ID and **business operation** UTF-8 strings, each framed with a little-endian u64 byte length. The lowercase SHA is prefixed `morrow-host-editor-intent-`. The issue-operation domain is `morrow.hmos.editor-intent-issue.v1\0`, framing the intent ID and prepare operation, prefixed `morrow-host-intent-issue-`.

## Reply semantics and recovery

New actions return `editor_intents:View[]` and `intent_next_after:string`. A View contains `proof`, `card_id`, `business_operation`, `expected_revision`, `phase`, `current_generation`, `current_active`, `repeated`, `part`, `request_json`, nullable `publication`, `save_request_json`, `inspect_request_json`, `close_request_json`, `close_disposition`, and `issue_operation`. Old actions omit the two optional fields.

`phase` and `current_generation` describe CURRENT metadata, including after an exact historical mutation retry. Prepared is generation 1 and active; issued is generation 2 and active; cancelled is generation 2, closed and inactive. `issue_operation` is deterministic and present in all phases. Only closed has disposition `cancel_prepared`.

Mutation replies use `effect:"committed"` and the current metadata generation as `receipt_revision` after an actual metadata transaction. Read/list use `effect:"not_committed"` and an empty receipt; this does not assert business absence. Summary replies leave all part payloads empty/null. Each part read returns the complete requested payload; the remaining payloads are empty/null. Before issue, save/inspect are empty. Closed cancelled contexts retain their original Submission, immutable publication metadata, first proof, and close literal, but have no exportable pin authority. List is ordered by the ASCII intent IDs, with the last returned ID as cursor only if another matching entry exists.

Prepare, issue, close, and business mutation are distinct Core transactions. Unknown outcomes retain the original immutable request/operation for explicit retry; no automatic replay or all-or-none multi-transaction claim is made. The actual business inspect remains independent from intent phase. Reopening the real Store preserves exact plans and historical replies even after the live business card advances.

## Authority, pins, budgets, and isolation

First prepare requires the existing strict active, exact publication and complete original Core source CAS. It records the predicted complete command/content hashes and retains all selected publication pins as actual host-owned Core attachments in the same metadata transaction. Their original selection/aliases/metadata remain intact; only internal attachment IDs become `intent-pin-<index>`. Pin bytes are verified on prepare/issue/save through the existing bounded authority path and complete SHA/length checks.

After the parent advances to S2 and removes S1 selections, an active issued intent can authorize that original S1 first save by its independently retained bytes plus immutable publication metadata. This is a dedicated issued-intent permission, not a general history-first-save grant. Core's original source CAS still rejects an external changed source. The projected command and content must exactly match what prepare authorized. Once the business operation exists, the original historical strict inspection checks remain authoritative; current card state only determines equality/conflict.

Without an intent, existing v22 saves remain compatible. When the same card/business operation has an intent, omitting its proof is rejected by strict `editor_save` and legacy `create`/`edit`, including after cancellation. Issued cannot cancel even when no business operation exists. Issue and cancel compete on the same real intent generation-1 CAS; a stale-snapshot loser writes no operation.

The independent typed journal does not change original draft Write validation or consume protected draft fields 11/12, raw-fork fields 13/14, or proposed handoff fields 15/16. Original protected lineage, source/origin, IME, field, business body, and task budgets are retained. Unknown properties/assets and task completion semantics are still handled by the existing strict final projection.

The inner Submission, every actual outer envelope/plan, and every reply are independently limited to 512 KiB UTF-8 bytes. The canonical intent protobuf has the existing independent 4 MiB host-body bound. Prepare preflights full current/issued part replies before retaining a request; nothing is cropped or silently omitted to fit. Issue stores both plans and counts their complete bytes. Active logical charge includes the complete canonical body and every retained pin byte. Ordinary draft creation/growth and intent creation/issue check a combined ceiling of 16 active identities, 64 MiB logical bytes, and 256 cumulative identities including closed contexts. Cancellation releases active refs/charge while retaining bounded historical context.

The aggregate check follows the pre-existing journal scan-before-write architecture. The serialized native host checks both producers. The real same-identity Core CAS proof does **not** prove an atomic global quota across two independent processes simultaneously creating distinct identities; that concurrency qualification remains OPEN. This implementation does not add a shared frozen-Core transaction or protected service claim.

The reserved journal ID/type/body/revision/attachment tuple is validated. Valid intent journals are excluded from user list, summary census and snapshot query. Ordinary mutation and attachment-export routes reject reserved host IDs; malformed private records fail closed rather than becoming ordinary user cards. There is no public intent-pin export action. Closed context's immutable publication read is restoration metadata, not renewed pin authority.

## Fresh validation

| Evidence | Result | Scope |
| --- | --- | --- |
| `rust-tests.log`: `cargo test --manifest-path hmos/rust/Cargo.toml --offline` | 165 library PASS, 0 fail, 11 conditional ignored, 86.52 s; attachment binary 3 PASS, 6.10 s; self-check/doc 0 | Complete current default host suite; includes all 10 new intent tests |
| `editor-intent-crash-tests.log`: same Cargo command with `--features morrow-core/fault-injection editor_intent::tests::actual_store_intent_crashes_keep_fixed_request_and_one_cas_effect -- --ignored --exact --nocapture` | 1 test PASS, 5.77 s, executing 24 real subprocess vectors | Prepare/issue/cancel/business × after-begin/card/operation/event, before-commit, after-commit; reopen, exact original retry, one effect and integrity |
| `editor-intent-byte-quota-tests.log` | 1 PASS, 11.39 s | Real 31 MiB asset: simultaneous parent/intent double charge, ordinary draft growth cannot bypass, cancel permits released capacity |
| `editor-intent-isolation-final-tests.log` | 1 PASS, 0.24 s | List/query hiding, ordinary host-target mutation/export rejection, full part reads and independent envelope bounds |
| `editor-intent-dto-fixture-release-tests.log` | 1 explicitly invoked exporter PASS, 0.24 s | Actual Store complete prepare/issue/read-5parts/business/inspect and cancelled prepare/close/read-5parts Reply DTOs |
| `editor-business-session-issued-recovery-model-tests.log` | Peer's actual-source ETS Session 30 PASS | Controlled transport replay of actual Store DTOs, complete native literal forwarding and restore-before-first-send boundary; not device/native transport proof |

The 10 default intent tests additionally cover actual Store reopen, historical original business retries after a newer favorite operation, altered proof/inner/outer no-mutation, old/no-proof bypass rejection, real original source conflict, same-object competing CAS in both orders, 16 active identities in both producer directions, and 1 draft plus 255 closed contexts reaching the cumulative 256 ceiling. Existing draft/fork/business suites also passed with the new combined hook.

Earlier failed logs are retained. The pre-final fixture failures were invalid manual seed category/clock/source-card choice, a query fixture missing mandatory conditions, and a relative exporter path interpreted from Cargo's crate working directory. The final full suite and explicitly invoked absolute-path exporter pass after those fixture corrections. `editor-intent-focused-release-tests.log` is an intermediate 9-PASS/1-failed/2-ignored query-fixture run, not final acceptance. `editor-intent-dto-fixture-final-tests.log` is the retained path-failure run; the release exporter log is authoritative.

The ignored crash test enables fault injection only for host test execution; it is not a release feature. The default suite's other 9 ignored historical Flutter/fault/export comparisons were not rerun or claimed as fresh by this intent work. The newly ignored intent crash/export tests were both explicitly run as above.

## Frozen source and build inputs

`editor-intent-native-inputs.json` records complete SHA-256 and byte length for the eight owned source changes, unchanged Cargo manifest/lock, and actual Store fixture. New inputs are `rust/src/editor_intent.rs`, `rust/src/editor_intent/tests.rs`, and `rust/editor-draft-model/schemas/editor_intent.proto`. Modified inputs are `rust/src/lib.rs`, `rust/src/editor_business.rs`, the narrow combined-capacity/history additions in `rust/src/editor_draft.rs`, and model `build.rs`/`src/lib.rs`. Original schema and original model request validation remain unchanged. No new dependency or native C/NAPI/DTS API is needed; existing Workbench JSON dispatch carries the new actions.

The model build uses its existing bundled protoc to generate the independent intent messages together with the draft package; no checked-in generated binary/source artifact is required. Any later native archive inventory must include the new intent schema/module/tests plus the updated model build inputs; existing b486/v22 native libraries do not prove this new production code.

Frozen principal hashes:

| File | SHA-256 |
| --- | --- |
| `rust/src/editor_intent.rs` | `CA722439719EF6FE19900B28BADE5273372F31075A05BE7B49733B32F95F08A3` |
| `rust/src/editor_intent/tests.rs` | `FEF586499FD249E09C45AA6E9F859DAC150A105CF2B675C67FCEAA2E40E835DD` |
| `rust/src/editor_business.rs` | `C5502B47C4F78DF27D87AB7D7BD6EF7508E1A29A7E201AFAFAA5CBE63DB456EF` |
| `rust/src/editor_draft.rs` | `28C2409C830EDE59D852B3C46EAA77228ECF52928371A910B5C1716A8DABF59A` |
| `rust/src/lib.rs` | `E9B5F362D7F1EB32880DD4DFE948FE1D533859AB9E0CE6209EBFC14A6AF55DCE` |
| `rust/editor-draft-model/schemas/editor_intent.proto` | `461269138817CE96A83781B7EBED34A4B10CCA497173307B8DF2BDD1416D45E0` |
| `rust/editor-draft-model/build.rs` | `D9FB0310E615F8FC7BA62A760E626E449E000915E99D2B381BD65A17871592D9` |
| `rust/editor-draft-model/src/lib.rs` | `0518F380D0335C21EBDD031AD22B11572FCF5917024ED0AA408D092AC6E06E4E` |
| `editor-intent-store-fixture.json` (39,256 bytes) | `9C8BD71EE6C42AD965A129973C5943F62A38261EEF99310FA3D12AF99A9999BB` |

No device, Git mutation, ABI build, release signing, or Index business integration is implied by this evidence. Full Flutter/HMOS application acceptance remains OPEN.
