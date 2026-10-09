# v27 readonly current business child preparation

This checkpoint adds a page-facing preparation model. It reads the original qualified business session's fixed handoff plan, exact immutable S2 parent history, and actual current child. It returns the same real paused writer for the page to install. It does not save, retire, close, replay business, generate operations, construct an active parent writer, or reconstruct input from card/task DTOs.

## Frozen files

| File | Bytes | SHA256 |
| --- | ---: | --- |
| `hmos/entry/src/main/ets/model/EditorBusinessRecovery.ets` | 10792 | `6b02c274cb3b6795fb89c8457096aa2396ed22cb6f50acdb86a1b728b92533ec` |
| `hmos/tool/editor-business-recovery-model.test.cjs` | 18482 | `7930d64702f5fddccb2ede1cb6fac8374ebb50f4bca42762984070922940f00e` |

Existing Handoff, Session, Business, Draft, FieldPolicy, and Fork production models are unchanged. Existing source tests and v22/v24/v25/v26 native fixtures were read-only. Index, Native/Rust, Git, SDK, device, and top documentation were outside this subtask.

## Page API

`new EditorBusinessRecoveryCoordinator(session, handoffHooks, guard)` is local only and requires the original strictly qualified Session instance. It does not replace or restore that instance and therefore cannot wash its known business result, registered wire, receipt hints, or Unknown.

`load()` and explicit `retry()` return `Promise<EditorBusinessRecoveryReady>`. `ready` is `EditorBusinessRecoveryReady | undefined`. Its detached metadata fields are `record`, `values`, and `parentReference`; `writer` and `handoff` are the actual coordinators.

The preparation steps are:

1. `EditorBusinessHandoffCoordinator.restore(session, undefined, hooks)`: read exact original handoff, retirement, and close literals. The parent coordinator remains absent.
2. `loadParentHistory()`: read the fixed plan's parent save operation/generation. The actual historical S2 may differ from the Session's S1 publication. Current inactive flags remain inactive diagnostics; no fake active parent writer is created.
3. Freeze one current read command: `{"action":"draft_read","id":<fixed card>,"draft_id":<fixed child>}`. This reads actual current state including inactive journals. It is not a draft list lookup or a historical first receipt.
4. Admit the full returned DTO and exact scope, then call `openCurrentChild(record, record.values, guard)` for the Handoff's strict business15/history/source binding. A current active record at its returned generation is mandatory. The actual writer is immediately paused before returning it.

The page validates the same actual Ready writer/handoff and complete record, parent proof, original Session instance, and ownership guard before calling `claimWriter()`. That call transfers the preparation's disposal responsibility, returns that same paused writer, and does not resume writes. The page then synchronously installs that writer and binds its Session owner before explicitly resuming it. Its real SDK lease remains revoked until the actual editor mount callback. A second claim cannot dispose an already transferred writer.

`dispose()` disposes only an unclaimed prepared writer. The original Session, Handoff, parent history, current read evidence, and fixed uncertain wire remain available. `parentHistory`, `currentRecord`, `handoff`, `originalCurrentRead`, `pendingRead`, `unknown`, `saving`, `disposed`, and `error` expose recovery state. `currentRecord` is observed readonly evidence; only `ready` grants a strictly qualified writer.

## Retry and owner boundaries

Current read uncertainty retains the exact `originalCurrentRead` string. Another `load()` is rejected until explicit `retry()`. A parent-history retry uses Handoff's original fixed history read. A failed plan part retries the same immutable proof's readonly parts; it does not mint a first child, retirement, close, or business operation.

An explicit retry may resolve its original readonly result after owner loss, but the result cannot install into that lost owner. Successful reads are retained before the next owner/cutoff check. A revoked cutoff prevents subsequent preparation steps and preserves the original qualified business facts. Revocation after writer preparation disposes the unclaimed writer; revocation during an in-flight read does not cancel the native read or erase a successful late reply.

A closed original Session stays closed and permits only its historical business inspection. An actual still-active current child can be prepared from a closed handoff's lineage. A saved_exact closed Session with no handoff child is a known absence of a child plan, not a new writer or uncertain current read.

The returned Handoff preserves the original sender for later page-controlled explicit retirement/close. This model calls none of those mutation methods. The page must confirm its installed child's latest complete input before explicitly continuing the fixed parent retirement and close.

## Verification

After the SDK exception-type correction, six actual production-source suites: Recovery11 + Handoff29 + Business25 + Session34 + Draft27 + Fork17 = **143/143 PASS, 0 failures, 0 skips**, exit0; runner duration11476ms. Seven production modules, six source tests, and five actual native fixtures comprise **18 inputs identical before/after**. Existing models, tests, and fixtures retained their prior identities.

- `editor-business-recovery-model-sdk-retry1-inputs-before.json` and `...-inputs-after.json`: final source/test/native-fixture identities.
- `editor-business-recovery-model-sdk-retry1-result.json`: final suite result and scope limitation.
- `editor-business-recovery-model-sdk-retry1-tests.log`: SHA256 `28cdc8d25efe90a48033872f23538ae110ca7f9ccd22cb92a037e5adee70e8d6`.

The initial 143/143 run (duration11975ms) and its original manifests/log remain unchanged:

- `editor-business-recovery-model-inputs-before.json` and `...-inputs-after.json`: complete source/test/native-fixture identities.
- `editor-business-recovery-model-result.json`: command result, input stability, counts, scope limitation.
- `editor-business-recovery-model-tests.log`: SHA256 `0a380819ff932a06430696302f6da93af0a137a054f07cfb87daa5f49a8d7215`.
- Actual v26 history fixture: SHA256 `4191624d7b2894abc17a3874ea3005249b450e6ee270b9c880ea2f04530415db`.
- Actual v25 saved_exact fixture: SHA256 `9af7ccaabaf247bbb06e60cd7302da878f515baa3704495735542bdb783e486f`.

Recovery tests exercise the actual production modules against complete native Store-produced DTOs, with controlled delays, lost read replies, and negative mutations of those DTOs. They cover full current S3 rather than first S2 installation, fixed S2 parent history rather than publication S1 substitution, closed handoff/current child, explicit original current/parent/plan retry, owner loss before/during/after preparation, ownership transfer, incomplete/foreign/inactive/advanced/oversize DTO rejection, no LF/card reconstruction, no operation generation, and no raw/business/retirement/close dispatch.

## SDK diagnostic correction and read-only page review

Root's first complete API26 build reported four `arkts-limited-throw` diagnostics for rethrowing an arbitrary catch value in Recovery at lines149/152/159/167. The preserved `sdk-stage1/original-hap-build.log` is 30442B, SHA256 `efd9f4f7559eb63299c0844dcac9dd943ae0e2ada435c10d3861d6abb3090126`. Each rethrow now uses the repository's `throw error as Error` pattern. The cast does not wrap or replace the runtime exception, so `RecoveryBoundaryFailure` and `RecoveryPlanAbsentFailure` keep their identities and their known-fact/Unknown behavior. This subtask did not invoke SDK; Root owns the fresh compiler check.

Read-only review of Root's Index at SHA256 `0c1620875c82c517bea1d8c4a3029dc851452978a323667fed8af91e483696b6` found the earlier claim-after-install and mixed-Session preparation issues resolved. Installation checks the same Recovery and Session instances, actual Ready writer/handoff, canonical complete record, parent proof, paused writer, and full raw values. `validateRecord` and original todos ownership parsing precede the claim. After claiming, attach and Session rebind contain no await; `session.saving` matches the rebind idle gate, and the actual callbacks/helpers examined do not introduce a concrete new synchronous failure for that validated state. The real SDK lease remains revoked until mount. This bounded source review found no additional blocking ownership/claim defect; it is not a runtime or device acceptance claim.

## Remaining scope

Page installation, current card conflict presentation, real SDK input cutoff handling, page lifecycle integration, native new writes, device restart, signed install, and full Flutter/Windows UI parity are not proven by this model checkpoint. Root owns the actual Index integration and its separate verification. This report describes a verified preparation layer, not product acceptance.
