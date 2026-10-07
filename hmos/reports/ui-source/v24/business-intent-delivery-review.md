# v24 business intent delivery review

2026-10-07. Independent read-only review of the frozen native intent foundation and standalone `EditorBusinessSession`. **No blocking data-loss, same-operation bypass or S2-consumption defect was found within this source checkpoint.** This is not protected producer/handoff certification, Index integration, new native archive adoption or device acceptance. The separate UI-only candidate cannot qualify the newly added backend.

The review read production paths, relevant actual Store and actual ETS tests, the exported Store DTO and existing final logs. It started no tests, SDK builds, device commands or Git actions. Only this report was added. The native writer's final explicit object-key sorting and test-fixture isolation changes were re-read and checked against the final identities below.

## Wire, proof and phase

`editor_intent.rs:203,216,352,500` validates canonical protobuf history and the complete reserved card/type/body/revision/attachment tuple. Proof names prepared generation 1, its complete canonical record SHA and the literal Submission SHA. Current phase is separately reported: prepared/1/active, issued/2/active, cancelled closed/2/inactive. Reconstructing generation 1 from live state must equal the exact historical first record. An issued or closed current reply cannot masquerade as a prepared state just because a prepare operation was retried.

Schema 1 native save/inspect literals explicitly sort every JSON object before compact serialization. Their inner original request and proof/revision are retained verbatim. `dispatch` preserves and bounds the actual outer transport bytes; `save_authority` requires exact equality with the stored issued save literal, so equivalent JSON, changed whitespace or a changed inner request are not first-send authority. Read/list return `effect:not_committed` and no receipt revision; metadata mutations return their own committed metadata revision. Neither means business commitment or business absence. Session's `committedHint` is set only on its business-send path, and an incomplete business DTO never becomes a qualified receipt.

Every full original, escaped outer plan and response is independently checked at 512 KiB UTF8. The intent protobuf also has the independent 4 MiB host-body bound. Prepare preflights the future issued plans and complete part responses before retaining the first intent; issue checks its enlarged charge. Output is not shortened to fit. These limits remain separate from field, business-body, image and draft budgets.

## Issue/cancel CAS, authority and historical result

`prepare` requires a genuinely active exact publication and original business source. It stores the predicted full Core command/content hashes and selected publication pins as current intent attachments. `issue` and `cancel_prepared` both require generation 1 and commit through actual same-card Core CAS. Exact retry validates the original mutation identity/content; a different phase or Close literal fails. The native tests exercise both stale-snapshot CAS winner orders. An issued-but-business-absent intent cannot be cancelled as prepared.

After S2 removes the parent's S1 selections, `save_authority` first establishes the active issued intent, exact registered outer save, verified current intent pin lengths/SHA, exact immutable publication and complete projected attachments. Only then can historical publication metadata supply the original S1 first save. `verify_projection` also requires its full command/content to match prepare. This is a dedicated issued-intent permission; ordinary history is not promoted to write/export authority. Original Core source CAS continues to reject an externally changed business source. The actual Store fixture/test retains and publishes S1 pin bytes after the parent S2 removes them.

For any current intent with the same card/business operation, strict save without proof is rejected, including after cancellation. Legacy create/edit of that same pair is rejected before grant/mutation. Reserved host IDs cannot be submitted as ordinary business cards or exported through ordinary routes. Closed context retains the complete original request, publication metadata and Close, but current attachments and active charge are cleared; its original metadata read does not restore pin-export permission. This cancellation is scoped to the exact operation, not every future independently authorized proposal for the card.

An existing business operation is established before later validation can downgrade its fact. Strict history reconstruction still compares the exact Core command, marker, original wire and full historical result. After current business state advances, exact original retry/inspect proves the original revision and reports the newer live state only as diagnostic. It does not rebase the original request onto that newer card. Existing strict TaskId/unknown-property/asset projection remains the unchanged business layer; this review did not create a new projection shortcut.

## Combined quota and Session recovery

`editor_intent::check_capacity` and the ordinary draft save hook account both producers: 16 active identities, 64 MiB logical bytes, 256 cumulative identities including closed contexts. The body and each retained pin are charged in both independently active parent/intent records. Issue charges both stored plans; ordinary draft growth cannot bypass an active intent's charge. Cancellation releases current charge while keeping the counted closed identity. Exact historical retries add no identity.

This is the existing scan-before-write architecture, serialized by the native host entry. Same-identity CAS is real transaction evidence; it does **not** establish an atomic global quota for independent processes concurrently creating different journal identities. That documented qualification remains open. No post-business `saved_exact` intent closure is implemented, so issued contexts remain active until a later qualified lifecycle protocol is supplied. These are limits of this unconnected foundation, not a claim of a completed multi-writer product flow.

Fresh Session prepare/issue/save require owner, exact input epoch and `parentReady`: the caller has paused the exact parent write queue without an in-flight write/Unknown/conflict. Session neither implements that queue pause nor resumes it. If issue is durable while its ACK waits and the view captures S2, it retains the valid issue fact but refuses fresh S1 save and prepared cancellation. Five-part explicit `restore` reads only the exact original Submission/publication/save/inspect/Close with one consistent current identity/phase; it sends no business mutation. The restored original business model is uncertain, allowing an explicit `retrySave` to settle the registered original S1 literal even after the old owner/epoch/parent gate changes. It never replaces S2 strings or its queue.

`mayConsume` delegates to the unchanged business coordinator's qualified historical receipt plus original owner/exact hooks and complete `sameValues`. The late-S2 regression changes text, reverse selection and composition, with both same-owner and revoked-owner cases, and requires no S2 consumption. Metadata/read/transport failures keep their exact original Unknown operation or read for explicit retry; they do not generate a fresh save plan. There is no automatic replay.

## Evidence read and frozen identity

All 11 entries in [native input inventory](editor-intent-native-inputs.json), including unchanged Cargo inputs and the Store DTO, matched disk bytes/SHA at review. The principal files read were:

| File | Final SHA256 |
| --- | --- |
| `rust/src/editor_intent.rs` | `CA722439719EF6FE19900B28BADE5273372F31075A05BE7B49733B32F95F08A3` |
| `rust/src/editor_intent/tests.rs` | `FEF586499FD249E09C45AA6E9F859DAC150A105CF2B675C67FCEAA2E40E835DD` |
| `rust/src/editor_business.rs` | `C5502B47C4F78DF27D87AB7D7BD6EF7508E1A29A7E201AFAFAA5CBE63DB456EF` |
| `rust/src/editor_draft.rs` | `28C2409C830EDE59D852B3C46EAA77228ECF52928371A910B5C1716A8DABF59A` |
| `rust/src/lib.rs` | `E9B5F362D7F1EB32880DD4DFE948FE1D533859AB9E0CE6209EBFC14A6AF55DCE` |
| `rust/editor-draft-model/schemas/editor_intent.proto` | `461269138817CE96A83781B7EBED34A4B10CCA497173307B8DF2BDD1416D45E0` |
| `rust/editor-draft-model/build.rs` | `D9FB0310E615F8FC7BA62A760E626E449E000915E99D2B381BD65A17871592D9` |
| `rust/editor-draft-model/src/lib.rs` | `0518F380D0335C21EBDD031AD22B11572FCF5917024ED0AA408D092AC6E06E4E` |
| `entry/src/main/ets/model/EditorBusinessSession.ets` (32782 bytes) | `792E95BC45D5D589A08810B4A3BED24FA0C6229612FB38FDA18ABD12A9821FBD` |
| `tool/editor-business-session-model.test.cjs` (38644 bytes) | `F64477A60AF77857D090AC17E606CF13E9EAA3442FF9CCFBF4E9F41C60661540` |
| Actual Store DTO (39256 bytes) | `9C8BD71EE6C42AD965A129973C5943F62A38261EEF99310FA3D12AF99A9999BB` |

The three unchanged underlying ETS inputs were also verified: Business `6469D1F2AF4E2DA6B9066B52917E658BC13ECECB4EF2A4E6B92FBB4AD0D996CF`, Draft `A3AE1E99D198FCBA3CDEF741A6F138612AD16979BE42C7A8809003F00D04F965`, FieldPolicy `10C3162E77A947335ACF0C3DF2E464C80195D0AC2BBC2922BBB09C0A8A626842`.

Existing worker-produced evidence read, not rerun by this reviewer:

- [Final full Rust log](rust-tests.log): 165 library PASS/0 fail/11 conditional ignored, 86.52 s; attachment binary 3 PASS, 6.10 s. It includes all 10 new default intent cases. [Crash log](editor-intent-crash-tests.log): one explicitly invoked test PASS, 5.77 s, covering 24 subprocess boundaries. [31 MiB quota case](editor-intent-byte-quota-tests.log) and [isolation case](editor-intent-isolation-final-tests.log) each PASS.
- [Actual Store exporter](editor-intent-dto-fixture-release-tests.log): one explicitly invoked test PASS, 0.24 s. The retained `focused-final-tests` and `dto-fixture-final-tests` logs are earlier failed fixture runs; their names are not final acceptance. The [native audit](editor-intent-native-audit.md) records the corrections and current full results.
- [Final Session checks](editor-business-session-issued-recovery-model-tests.log): 30/30 PASS, 0 skip, 5061.8498 ms, including two complete actual Store five-part parser cases. This is actual ETS code with controlled transport/providers, not native transport/runtime qualification.
- [Separate Session SDK audit](session-sdk/sdk-audit.md): isolated API26 public-API compile PASS 9.480 s, CompileArkTS 3.232 s. It used old v22 native archives and was not installed/run. It does not qualify the new backend; the earlier Session model report's SDK NOT_RUN statement remains its historical worker scope.

## Delivery boundary

The independent typed intent journal does not repurpose protected draft fields or production ParentLink/handoff proof. Its digest/phase is development metadata, not authenticated capture or protected provenance. First-read prepared protobuf identity comes from the trusted development producer; ETS does not invent that protobuf digest from JSON.

Index has not adopted Session, business source-kind-0 successor/handoff is not implemented, raw kind1 fork is not business rebase, and exact saved closure/detail re-entry remain separate work. A fresh native archive inventory and product build/runtime acceptance are still required before attributing real app behavior to this backend. The current review supports a source/host/isolated-compile checkpoint only. Full Flutter/HMOS application acceptance remains OPEN.
