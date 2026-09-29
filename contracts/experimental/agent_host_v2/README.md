# Native session v2 experimental revision 1

This is the sole new native-session wire authority. Frozen v1/kit003 is unchanged.
`native_session_wire.json` is normative; its exact raw SHA256 is the wire identity.
The Rust codec has no host authority or approval constructor. Frames are exactly
256 bytes, not Cap'n Proto and not legacy UI stdio. No variable body is allowed.

Host creates a process with private piped stdin/stdout/stderr and an empty environment
(explicit trusted OS fields only). Client is invoked with `--morrow-native-session-v2`;
test scenarios, if any, are additional host-selected arguments, not authority.
Client checks Challenge schema and own PID then echoes it as Hello sequence1.
Welcome must match the session/epoch/nonce/schema/artifact/config/PID, kind and sequence.
Queries/Close copy the **initial Challenge**, not the updated budget/deadline in replies.
State/Denied responses echo request sequence. Stop sequence0 is explicitly negotiated
host control; client should close its protocol output and exit promptly. It must never
interpret frames as shell commands. stderr is diagnostic only; stdout is binary only.
Clients lacking the host launch argument/channel must fail closed (help/version allowed).

Only own-session read status is supported. No HTTP, account, content, writer or arbitrary
execution capability exists. Admission is created by trusted host code before launch;
echoed fields never grant it. Nonce and inherited pipe ownership correlate launch, but
this slice is not an installer, image attestation, process-tree containment or OS sandbox.
The host rechecks artifact digest, but path hash versus actual image load is a disclosed
race. Client PID echo is correlation only; host uses its actual Child PID.

Before a new grant, expiry starts on a host monotonic clock. Handshake/query/retry do not
renew it. Revocation is acknowledged by the host supervisor, prioritized over data I/O.
Already fully written replies are pre-revocation observations. Pending partial writes
are never followed by a spliced control frame: close/termination handles that case.
Exit, stdout EOF and stderr EOF are independent facts; absent confirmation retains
ClosingUnconfirmed ownership. No business success follows from exit0.
