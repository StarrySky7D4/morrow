# Plugin-side continuation review — 2026-09-30 (Asia/Shanghai)

Read-only source compatibility review of the active host partial-write-003 code against frozen guest fixture002. This is not new runtime qualification. A011/B011 remain limited accepted cases and A010 remains failed.

## Compatible source interfaces

The current host v3 schema exactly matches the frozen guest schema (SHA-256 8da8f1455200696d8884949f415aae4edd70cee6b583889bba7906c7730cc864). Eight reviewed guest source files match the active host companion copies. The host continues replying to HttpCredit with CreditState, preserving the guest's strict sequence/code0/classification confirmation. Host write-prefix accounting emits a complete-frame fact only after the original frame is complete; neither OS write cancellation nor prefix reception implies peer consumption or HTTP EOF.

## New-scene requirements found

1. **Fixture002 is not a passive fault observer.** Its only modes are core-revoke (actual Core queued/reserved delta barriers and a validated release) and data-pending (read paused at completed DataBound). Main requires stages_complete and gate_closed. Any use of fixture002 must actually satisfy the chosen mode. A passive deadline/peer-disconnect case needs a separate new fixture revision rather than silently omitting old barriers.
2. **First cancellation cause depends on the actual path.** Guest Stop/Denied code20 records Deadline. A valid generation2 HttpTerminal with error20 first closes the shared latch as HostCancelled, then retains the transport Unknown; local authority expiry can instead win with Deadline. Do not rewrite the first reason or infer it solely from the host reason. Record original authority, host durable provenance and the guest control/gate observations separately.
3. **Deadline closure has different terminal facts from clean external revocation.** Current host reason20 enters Closing, can replace queued HttpTerminal with Stop20, and ignores later guest bytes while closing. A closing ACK is tied to a prior Close with reason25 and is dropped at the original expiry. Guest correctly retains a missing-ACK control failure / unconfirmed request cleanup. Host actual join/reap/Released can still be proved separately. A deadline case cannot inherit the A/B clean-ACK criteria or extend authority until an ACK passes.
4. **A partial frame remains failure even after revocation.** Guest framing is never relaxed by host terminal control. Worker Err still receives a real finished-only join and retains failed cleanup. An earlier transport Unknown may remain the aggregate first error; do not use a flattened error label alone to prove or dismiss malformed data. Keep raw bytes, operation completion and cleanup dimensions.
5. **Zero-window settlement remains required when real credit/bytes exist.** Current host accepts original-tuple cleanup credit and replies CreditState. The guest waits within its original 2000ms cleanup budget for the matched classifications, keeps early RequestClosed resource facts, and allows Close only after necessary settlement. Truly pre-head/all-zero/no-credit cancellation has no fictitious ACK requirement.

## Validation boundary and next work

No source edits, builds, tests, guest/host executable launches, HTTP requests or Git operations were performed. Only the thirteen relevant source files and eight active companion comparisons were inspected; no historical candidate hash audit was repeated. Source checks are fixed to this report's inputs; later host edits need their own run binding.

Main coordination was sent the mode, first-cause and expiry/Close findings before its new fault runs. Continue with current-host full Core/HTTP external revocation qualification and narrowly scoped OS deadline/peer-disconnect probes. For a full passive fault matrix, derive a new guest observer only after the concrete fixture contract is agreed, preserving fixture002 and all accepted/failed receipts. Complete M03, product acceptance and SDK freeze remain open.
