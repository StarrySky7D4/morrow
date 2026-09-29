# Native session Cap'n Proto v2 revision1

`native_session.capnp` is the sole new wire authority; its raw SHA256 identifies
the contract. capnpc0.24.0 generates Rust bindings used by the actual codec with
capnp0.24.1. Withdrawn `agent_host_v2` JSON/fixed256 candidate is NOT accepted.
v1/003 remains frozen. No client approval bit or identity claim creates a grant.

Framing: 4-byte little-endian payload length (8..2048, divisible by8), followed
by exactly one unpacked Cap'n Proto NativeSession message. max whole frame2052.
No packed encoding, trailing messages, or unbounded payload. Decode budget1024
words, nesting8; four digest/nonce Data fields are each32 bytes. reserved must0.

Host invokes fixed verified executable with `--morrow-native-session-v2` and
host-selected optional args, private piped stdin/stdout/stderr, isolated cwd and
cleared environment. stdin receives host frames, stdout carries client frames;
stderr diagnostics only. No listener/port or legacy UI unsolicited messages.
Read Challenge, verify schema and own PID, echo all binding fields as Hello(seq1,
code0). Validate Welcome identity and sequence. Query/Close use the ORIGINAL
Challenge copied fields, code0, sequence strictly+1, one outstanding request.
State/Denied echo sequence but update deadline/budget/generation. Stop(seq0) is
negotiated unsolicited control; close stdout and exit promptly on it.
Close returns Stop(code25); host closes child's stdin after sending it.
Trusted revoke acknowledges generation2, does not by itself stop the child;
subsequent queries receive Denied19. Explicit stop/expiry closes the session.

Only own-session status capability1 exists. All account/network/content/writer/
arbitrary-execution operations unsupported. Trusted host admission starts a
monotonic fixed TTL before artifact verification; no handshake/retry renewal.
Frame assembly and output writes have independent fixed deadlines. Trusted
control queue is separate and selected before client data. Pre-ack completed
responses cannot be retracted; partial cancelled response closes the channel.

Hash/path vs image loading TOCTOU, inherited-handle delegation, descendants,
installation trust and OS sandboxing are NOT solved. Host uses actual Child PID;
client echo is correlation only. Release requires wait success AND stdout/stderr
EOF; ClosingUnconfirmed retains ownership. Exit0 is not business success.
