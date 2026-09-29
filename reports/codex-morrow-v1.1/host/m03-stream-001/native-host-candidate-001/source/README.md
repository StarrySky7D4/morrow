# M03 native HTTP stream host candidate

Experimental Windows native host using the frozen v3 Capnp codec, owned Win32 pipe platform,
and incremental transport crate. It derives from owner-002 into a new directory. The previous
owner, transport, codec and platform inputs remain frozen. No product gate credit.

The trusted operator controls `init`, `serve`, `inspect`. Runtime accepts only a synthetic
literal `http://127.0.0.1:PORT` origin. The guest executable is digest-bound, starts in an
existing empty cwd, and receives only SystemRoot/WINDIR/COMSPEC. Host authority uses the real
existing LOCALAPPDATA lock namespace. Never use a private replacement namespace to bypass
ownership. A fresh profile is initialized once; an uncertain prior owner is not recovered.

`serve --profile ABS --client ABS_EXE --sha256 HEX --work-dir ABS_EMPTY_DIR
--plugin-id morrow-codex --role synthetic-http --operation UNIQUE_OPERATION
--http-origin http://127.0.0.1:PORT --ttl-ms 10000 --close-ms 1000`

Each guest argument is an additional `--client-arg VALUE`. The host adds
`--morrow-native-http-v3`; plugin flags are `--fixture-base http://127.0.0.1:PORT/v1`,
`--evidence-dir ABS_FRESH_PLUGIN_OUT_DIR`, `--max-chunk 1024` (six values).
Profile/cwd/evidence directories must already exist and be empty. Never reuse a failed POST.

Operator stdin is newline-delimited JSON, an explicitly trusted test harness interface;
it is not a production authenticated CLI. Stdout contains bounded JSON observations.
1. `{"action":"approve"}` creates the original absolute deadline and returns grant_id.
2. `{"action":"claim","grant_id":"..."}` consumes once before spawning the digest-bound child.
3. `{"action":"inspect_http"}` reads exact proposal body_hex/raw header bytes and its digest.
4. Independently inspect/recompute the complete original proposal preimage before
   `{"action":"approve_http","proposal_ref":"HEX","expected_hash":"HEX","response_limit":65536}`.
   Limit must be no greater than the proposed limit. The guest then sends one HttpCommit.
5. `revoke` (grant_id), `stop`, `inspect`, and `quit` are trusted controls. Input EOF requests stop.

The guest waits for State echo of HttpPrepare before submitting RequestChunk on the other lane.
Data/control sequence spaces are independent. Admission identity and original generation are
required even for cleanup; only Query, idempotent Cancel, issued-tail zero-window ACK and Close
survive revocation. Host notifications use sequence zero and direct replies echo request sequence.
RequestClosed describes joined HTTP worker and reaped/closed pipe operations, not child exit.
Guest then Close/exits; external authority requires actual child exit, both stdio EOFs and worker
cleanup before committing owner Released. A pending SendTicket is retained across task install.

Core owns Prepared/Unknown/Observed attempts and exact request/response material, using the same
pinned Store as the native owner. Native Protobuf/LZ4 ledger owns parent and HTTP approvals.
The 32-byte v3 endpoint_ref is mapped bijectively to 64 lowercase printable hex bytes for the
existing Core HttpSubmission reference contract. It is not parsed as an endpoint or new authority.
Core Unknown precedes the send fence; any failure after native grant consumption cannot retry.
Full HTTP EOF plus durable response material is required for Observed; Core model Completed
alone is insufficient. Response quota equality still requires a terminal read to prove EOF.

Validation boundary: five new adapter API tests use simulated owner records, actual new profile
Store/ledger transactions, no child and no HTTP send. They cover consumed/Core-history failure,
owner drift, absolute expiry, full Commit binding, ticket cleanup/revoke, and Observed preservation.
The old three owner tests are not rerun or counted. Compilation is not runtime qualification.
The first real host/Core pairing is pending a corrected plugin candidate; plugin001 has two
independent static blockers and must not be launched by the draft pair-001 harness.
