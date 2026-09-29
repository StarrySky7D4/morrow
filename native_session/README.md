# Experimental native session control slice

Reusable host-owned Rust library plus a trusted operator CLI. Uses the existing
`morrow_core::lifecycle::HostPolicy` for instance activation, ready, revocation,
stop and retirement. No content grants, writer, HTTP listener, account access or
business command surface is added. This is not a complete M02 product delivery.

The only wire authority is `../contracts/experimental/agent_host_v2_capnp`:
Cap'n Proto major 2/revision 1, unpacked, bounded little-endian length prefix.
The earlier non-Capnp `agent_host_v2` candidate is withdrawn and is not used.
Frozen v1 and host-kit-003 remain unchanged.

## Host API and lifetime

The trusted host calls `Admission::authorize(LaunchSpec)`, then consumes that
non-deserializable admission through `NativeHost::launch`. Approval is never
accepted from a child frame. The public API is a trusted embedding boundary;
it is not connected to a production approval UI or persistent approval service.
Callers must retain the Tokio runtime and supervisor until confirmed release.

Admission starts a monotonic deadline before validation, hashing or process
creation. Args, executable path/hash, empty working directory, limits, platform,
architecture, schema and exact allowlisted environment are configuration-bound.
Only SystemRoot/WINDIR/COMSPEC can be inherited; their values are captured at
admission and reused at spawn. No credential environment is inherited.
Retries and repeated revoke never renew the original deadline.

The host creates a real child with private redirected OS pipes, observes its PID,
and supplies a fresh nonce, session and instance epoch. Hello must match that
binding; a strictly sequenced Query reads only its own control-session state.
There is one pending response and one partially read frame, with size, request,
time and diagnostic limits. No frame can create another admission.

`Session::revoke` and `stop` use a separate bounded control queue. The supervisor
prioritizes control, then deadlines, then transport events. Revoke acknowledgment
marks the host-side revocation point; subsequent reads are denied. A complete
response already written before that point is not retractable. Pending unsent
success is replaced with denial; partial writes are closed without frame splicing.
The partial-write cancellation path is source-reviewed, not dynamically proven
under OS pipe backpressure by this candidate's tests.

Closing sends Stop when possible, closes stdin and uses bounded grace periods
before requesting child termination. Actual root-child exit AND stdout EOF AND
stderr EOF are all required for Released. Otherwise ClosingUnconfirmed retains
the owner and rejects same-slot replacement. Descendants may keep output handles
alive after root exit; this is deliberately observed, not treated as isolation.

## Trusted CLI

`morrow-native-session-host.exe --client ABS --sha256 HEX --work-dir EMPTY_DIR`

Optional arguments: `--ttl-ms`, `--handshake-ms`, `--frame-ms`, `--close-ms`,
`--budget`, and repeated `--client-arg ARG`. It prepends the fixed client flag
`--morrow-native-session-v2`. stdin accepts trusted JSON actions inspect/revoke/stop;
stdin EOF requests stop. This channel belongs to the trusted operator, not child.
stdout is JSON evidence. A dedicated OS stdin thread avoids waiting for Tokio's
blocking stdin reader at normal shutdown; the CLI process owns that thread.

Exit 0 means confirmed release following controlled close/stop/revoke, not
business success and not necessarily child exit 0. Rejected, expired or disconnected
sessions exit 2 after release; the final snapshot preserves the child's real exit.
ClosingUnconfirmed keeps the process and owner alive. Drain operator stdout:
blocked operator-output/runtime shutdown behavior has not been dynamically qualified.

Evidence includes actual PID/session/epoch/generation, monotonic microseconds,
frame_sent (host to child), frame_received (child to host), raw frame hex,
partial-frame events with complete=false, actual exit and independent stream EOFs.
Diagnostics are bounded; any event_overflow disqualifies a complete-evidence claim.

## Validation boundary

Windows x86_64 real-process qualification covers 18 host cases, three real frozen
plugin-client pairings and a negative CLI exit check. Build inputs and logs are
bound in the runtime-kit-001 handoff. Handwritten host code forbids unsafe Rust.

Still open: image path/hash/load TOCTOU; executable dependencies; descendants and
inherited handle delegation; OS sandboxing; cross-process owner registry; host
crash recovery; production approval UI; actual blocked-write cancellation;
Linux/device/runtime parity; full M02, G0/P02/J00 and product graph acceptance.
The nonreading-peer test did not fill the Windows pipe and proves control response
for that scenario only. Codec nesting/traversal limits were configured but their
thresholds were not dynamically reached. No writer or data authority was tested.
