# Native session client — Cap'n Proto kit001

The executable consumes the host-authored `capnp-kit-001/wire` codec. It supports
only the host's own-session read status; there is no account/network/content/store
writer or arbitrary execution fallback. The retained `host-wire-v2-r1` vendor
directory is a withdrawn, unused candidate and is not a Cargo dependency.

Offline CLI: `--help`, `--version`, `--check`. Other offline requests fail. A host
launch uses `--morrow-native-session-v2`, optionally `--queries 1..64`,
`--interval-ms 0..10000`, `--io-timeout-ms 100..5000`. Default: one query, no delay,
1000ms I/O timeout. For online observation use 16 queries at 200ms intervals with
a suitable host TTL/budget. These are load bounds, never authorization inputs.

The client validates the Challenge's schema/PID/nonzero session, epoch, nonce and
config digest, capability1, generation1, remaining TTL and budget. It hashes only
its own executable and compares the host-bound artifact digest. This is not image
attestation and does not repair the documented path/image-load race. Hello,
Query and Close echo the initial binding, not dynamic values from later replies.
Replies must match immutable identity and monotonic generation/TTL/budget, the
outstanding sequence and expected phase/kind. No pipelining or retry occurs.

stdout is protocol binary only. One reader and one writer have separate one-slot
queues. Input is checked before write completion; Stop and EOF are handled during
write waits and query intervals. Partial-frame and operation deadlines are fixed
on first arrival/start; an additional process-wide 60s safety cap starts before
reading Challenge and never renews. A failed write is never replayed. Bounded
framing precedes Cap'n Proto decoding; the authoritative codec owns traversal,
nesting, alignment/version/Data-length validation. No codec is reimplemented here.

Exit0 means a valid host Stop(code25) was observed, not business success or host
release. Protocol/identity/sequence/denial exits retain host codes16..25; local
timeout is26, bad CLI is2, EOF is24. Close expects Stop0/code25. The client never
claims Released: actual process exit and stdout/stderr EOF are host observations.
Worker handles are process-scoped; returning from main terminates blocked worker
threads through process exit. Dropping a worker object alone is not a join or
resource-release proof. Only one bounded final diagnostic is written to stderr.

Local unit/vector/CLI results are not IPC evidence. The isolated runner retains
the historical `passed_bootstrap_*` status names for these local stages; this
does not mean the executable contains an alternate fake host. A fixed candidate
manifest identifies the actual compiled source and binary to be started by the
real host. Once a candidate is handed off, do not mutate its source/inputs/exe;
create a new revision directory if repair is necessary.
