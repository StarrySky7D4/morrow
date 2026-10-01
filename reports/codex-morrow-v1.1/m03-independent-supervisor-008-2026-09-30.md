# M03 independent supervisor 008 — Windows qualification in progress

User explicitly authorized an independent supervisor process on 2026-09-30 21:48 UTC. This does not authorize a service, elevation, startup registration, installation, publishing or broader credentials. Branch remains `codex/m03-stream-revocation-backpressure`, base HEAD `04b060ef2a8ae7e7806af7b0dcc772308e9b07e4`; all changes are uncommitted. No Actions/CI is used.

## Responsibility and process boundary

`morrow-native-supervisor.exe` is the trusted standalone resource owner. It reuses the Rust native-host session and HTTP implementation within that process, instead of nesting another native business host. Its guest is a separate process; descendants stay in the same anonymous Windows Job. This experimental native runner is not yet wired into Flutter's production workbench launch. Existing Flutter → workbench-host → in-process Wasmi remains a separate path.

The supervisor owns the only noninherited kill-on-close Job handle, held process-watch handles for one or two trusted controllers, guest process/primary-thread handles, and guest control/stdout/stderr handles. `CreateProcessW` uses `PROC_THREAD_ATTRIBUTE_JOB_LIST` to bind the child at creation, and `HANDLE_LIST` permits only its three stdio ends. The child starts suspended; the original TTL, controller state and revocation gate are checked before resume. No PID scan or PID disappearance is a reclamation proof.

Controller EOF, input saturation, watched UI/host exit or heartbeat timeout closes the business gate immediately. A bounded terminal exchange is allowed solely for cleanup; business authority is never renewed. Independent watcher threads record the first loss and enforce a five-second cleanup ceiling even if the async controller output or synchronous launch blocks. Diagnostic output is bounded. Unknown operations are never replayed.

## Persistent owner and failure contract

Supervised profiles use native schema 5 and explicit runtime mode 1; existing unsupervised schema 4/mode 0 remains separate. Cached full Profile identity and runtime mode are checked inside authorization transactions and business-parent checks. Claim records LaunchPending and a Pending Recovery binding in one transaction; registration binds the actual process, session and epoch. Immutable Recovery fields are checked across both transactions and later transitions.

A Reclaimed proof requires actual Job-empty observation, direct-child exit, both EOFs, joined/reaped pipe and network work, a closed gate, and the bound final snapshot. Persistence failure does not release the owner. Host/UI crash while the supervisor survives can complete trusted reclamation. Supervisor crash closes the Job in the kernel, but that alone does not manufacture a durable Reclaimed proof; restart retains the old owner and rejects automatic takeover or replay. This is the minimum safe restart behavior, not unattended profile repair.

## Verified Windows scope

Final check `host/m03-supervisor-008-check-014/receipt.json` has SHA256 `ed3386e8dedb555fcd88d63b767367580ace8abf97a2d24d9fc6480d1bcf9d8b`. All 15 specified commands exited 0 (default/qualification builds and focused regression groups). The 278 input hashes are identical before/after; all four binaries in each build are pinned individually. These groups overlap and are not a full-product suite.

`host/m03-supervisor-008-run-005/result.json` records 16/16 actual serial Windows cases passed, runner exit 0, unchanged source/candidate and zero HTTP requests. Cases cover normal/expiry cleanup, missing Close, controller EOF, watched host/UI helper exit, heartbeat loss, descendant-held stdio, supervisor crash/restart, schema-mode separation, Recovery tamper, actual SQLite write failure, blocked control output, queued EOF, and concurrent owner refusal. Watched UI/host helpers are real OS processes, not production Flutter integration.

The missing-Close and controller-loss cases retain their actual exit 2 as expected fault results. In particular `expiry-no-close` proves resource reclamation while explicitly recording `terminal_protocol_complete=false`; it is not a protocol success. Supervisor crash proves kernel Job cleanup with retained process handles, but leaves the durable owner nonterminal and rejects replay/takeover on restart. Actual persistence failure never fabricates Reclaimed. Two independent read-only reviewers verified the final source, candidate and control-only evidence without finding a remaining P1.

Platform bounded-reclamation tests passed 9/9. Runtime evidence-validation tests and supervised HTTP evidence-validation tests each passed 26/26. Pure validation tests are not OS fault execution evidence.

## Production changes versus fixtures

Production: standalone executable and controller watcher; atomic Windows Job-bound suspended launch; narrowly inherited guest stdio; actual overlapped control-write completion; bounded independent writer reclamation; fixed Job-empty barrier; schema 5 runtime mode and immutable Recovery proof; transaction-local Profile/Pending validation; HTTP parent binding; release only after process/tree/EOF/worker/control proofs. The supervisor reuses native-host session code and adds no nested native-host process.

Fixture only: close-peer Stop receipt and descendant/no-Close fault modes. Stop dynamic generation/remaining-TTL fields are checked against revocation semantics rather than initial challenge values. These changes do not freeze or alter the production SDK contract. Tooling only: pinned build/freeze runners, raw evidence decoders and strict negative-case validation.

## Preserved unsuccessful evidence

Runs 001 and 002 failed on missing/empty killed-peer final JSON; run 003 failed because schema refusal diagnostics were not flushed; run 004 exposed an incorrect fixture Stop check and incomplete unconfirmed diagnostics. All remain unsuccessful and preserved. Subsequent fixes use new checks/candidates/runs. Check 005 had changed source and was rejected as a qualification candidate; check 007 did not compile. Original Unknown runs and 006/007 evidence are preserved without replay or relabeling.

## Outstanding acceptance

Actual successful nonzero OS short-write completion, hostile same-user security isolation, non-Windows platforms, production Flutter integration, product G0 and SDK freeze remain unqualified. Supervisor crash does not yet provide unattended durable owner repair: restart fails closed pending a separately trusted recovery design. No system service, installation, elevation or startup registration was performed. No CI, push, merge, deployment or publication was performed.

## Real Core/HTTP completion

`host/m03-supervised-http-008-candidate-001` freezes both supervisor variants from check014 and unchanged guest006. `host/m03-supervised-http-008-run-001` completed authority-deadline, network-abort and pipe-partial-close: all runner exit 0 and expected_fault_observed, actual supervisor exit 0, exactly one POST each and zero extra POST. The strict original 006 terminal checks remain. Authority expiry and network abort show RequestClosed before actual ACK completion. The non-expiry partial-pipe case reports an immediate Close ACK with request_closed/worker_joined false, then proves joined network/pipe workers at final reclamation; its ACK does not promise prior resource reclamation. All cases separately prove gate closure, child exit and both EOFs. Additional supervised checks require Job-empty and control-stdin closed/reaped before Released, with bound Recovery Reclaimed. The pipe case alone uses the qualification feature; it does not prove successful real OS short-write completion. HTTP Unknown/error outcomes are preserved and never replayed.

Final supporting receipt: `host/m03-supervisor-008-final-review-001/receipt.json`. All modifications remain uncommitted on the original branch.
