# Fresh bounded reusable Rust Wasmi qualification 001

Date: 2026-10-02 UTC. Generic dot-Linux in-memory byte/event sources.

## Result

Final combined explicitly ignored real-Wasmi harness: **6 passed /2 failed**
across20 fresh byte/event runs. The two red success assertions are the requested
historical-byte-vector5×32768-byte ownership-stress reusable cases; their genuine Limits results are
preserved. Twelve business runs complete; eight runs encounter the real20M fuel
boundary. The earlier bounded001-only run passed5/5, including its expected
four-full-frame negative boundary. No ownership-stress standard or universal-size pass is claimed.
The002 byte vector is historical, but its operation semantics are stronger than
the original Directory example; its failure does not establish baseline failure. Nearby channel_compat5/5 and channel_controls15/15 pass; formatting and
shell checks pass.

Production path: fresh Rust Wasm guest using channel::transport::WasmClient →
actual morrow_channel_v1.call → ChannelBroker → Manager/ManagedInstance →
HostRuntime/SQLite. Caller-supplied canonical Directory and actual source bytes
or events. No native callback shim, mock, source replay or transport retry.

All manifests and effective execution retain 20,000,000 fuel /16,777,216 memory
bytes /16 host calls. New Rust channel projects retain generated opt-level3.
Report.host_calls counts channel calls here; real task read_input/complete imports
do not increment it. Each passing frame uses Receive, Query, ACK, followed by one
explicit Close and a correlated task completion.

## Exact outcomes

| Fixture | Transport | Kind | Frames × bytes | Outcome | Calls | Fuel used | Fuel remaining |
|---|---|---|---:|---|---:|---:|---:|
| bounded001 | oneshot | ByteStream | 4 × 31 | Ok(0) | 13 | 1075726 | 18924274 |
| bounded001 | oneshot | Events | 4 × 31 | Ok(0) | 13 | 1095837 | 18904163 |
| bounded001 | oneshot | ByteStream | 1 × 65536 | Ok(0) | 4 | 7713085 | 12286915 |
| bounded001 | oneshot | Events | 1 × 65536 | Ok(0) | 4 | 7714763 | 12285237 |
| bounded001 | oneshot | ByteStream | 2 × 65536 | Ok(0) | 7 | 15477734 | 4522266 |
| bounded001 | oneshot | Events | 2 × 65536 | Ok(0) | 7 | 15482298 | 4517702 |
| bounded001 | oneshot | ByteStream | 4 × 65536 | Err(Limits) | 8 | 19997877 | 2123 |
| bounded001 | oneshot | Events | 4 × 65536 | Err(Limits) | 8 | 19998549 | 1451 |
| standard002 | oneshot | ByteStream | 5 × 32768 | Err(Limits) | 15 | 19999999 | 1 |
| standard002 | oneshot | Events | 5 × 32768 | Err(Limits) | 15 | 19999995 | 5 |
| bounded001 | reusable | ByteStream | 4 × 65536 | Err(Limits) | 8 | 19999900 | 100 |
| bounded001 | reusable | Events | 4 × 65536 | Err(Limits) | 8 | 19997771 | 2229 |
| bounded001 | reusable | ByteStream | 1 × 65536 | Ok(0) | 4 | 7702776 | 12297224 |
| bounded001 | reusable | Events | 1 × 65536 | Ok(0) | 4 | 7704813 | 12295187 |
| bounded001 | reusable | ByteStream | 2 × 65536 | Ok(0) | 7 | 15461395 | 4538605 |
| bounded001 | reusable | Events | 2 × 65536 | Ok(0) | 7 | 15465717 | 4534283 |
| bounded001 | reusable | ByteStream | 4 × 31 | Ok(0) | 13 | 1042483 | 18957517 |
| bounded001 | reusable | Events | 4 × 31 | Ok(0) | 13 | 1062824 | 18937176 |
| standard002 | reusable | ByteStream | 5 × 32768 | Err(Limits) | 15 | 19999996 | 4 |
| standard002 | reusable | Events | 5 × 32768 | Err(Limits) | 15 | 20000000 | 0 |

2×65,536-byte successes consume 131,072 bytes, above the old64KiB task-value
ceiling, with payload SHA256
`67c8b818598765176c4236bd9246924228cbcc25510c18c96abd042b8d595af7`.
One full-frame boundary SHA256:
`e05a19880e522ec8b96bf1098d4660e3d467fd307333541f39f21463f7a34312`.
4×31-byte SHA256:
`58feb1e8868e7e13bb77a165d885dc792c15d51efb4e5ba2e07b3d7208a92429`.
Complete64-byte output hex and exact fault/call/fuel/source/join receipts appear
in channel-bounded-runtime-receipts.json and the raw final log.

4×64KiB genuinely faults Limits for both reusable and fresh matched one-shot
guests after8 calls and ACK2, with no business output. Original Control is
Revoked and the actual original producer is Joined. No budget is increased, no
partial result is called successful consumption, and no old immutable package
is said to fail. Smaller fresh one-shot cases actually pass. Observed reusable
fuel savings are modest, not a claimed broad speedup.

## Ownership-stress5×32768-byte result and scope correction

The002 fixture uses the same five historical source frames, but is a stronger
ownership-stress test. Its Receive+Query+ACK and retained-vector/equality scans
are not identical to the immutable Directory example's operations. Its failure
cannot establish that example's failure. A separate fresh003 comparison follows
exact Directory example operations; it is not recovered historical binary bytes.

A separate new002 project/package retains all five source frames and its
Receive+Query+ACK stress semantics, owned-response snapshots, original ACK hashes and
cursors, explicit Close, and unchanged limits. Existing001 source/pins remain
unchanged. Both reusable and fresh matched one-shot **genuinely fault Limits**
after15 metered calls and ACK5. The16th Close submission is not reached; no
completion or business output exists. Reusable fuel remaining is4/0 for bytes/
events; matched one-shot1/5. Original Control is Revoked and the actual original
producer is Joined; ProducerOutcome remains Unknown.

All163840 source bytes are admitted and all five ACKs remain durable, but this
is not completed business consumption. Final reopened event receipts verify
each original frame/sequence/hash/cursor despite the fault. Expected complete
payload SHA256 is
`12a8659000a14e107b88bc40ee759d70c4749f68c01ee54157537b71008d6d16`.
No Query, ownership check, ACK, Close requirement or limit is removed to pass.
The reusable standard tests intentionally retain success expectations and are
red; the control honestly records failure. Additional optimization or product
qualification needs its own approved scope.

## Ownership, ACK and cleanup

- Complete owned Receive responses and independent snapshots remain alive
  across Query, ACK, later Receives and the final explicit Close
- ACK uses the original sequence, encoded-frame SHA256 and cursor
- Every original acknowledged event prefix, including both fault cases, is reopened from SQLite; original frame/request/
  response, sequence/hash/cursor and response correlation are verified
- Close and actual original producer join are separately observed. Guest Close
  may report ClosingUnconfirmed; host then proves CleanupProof::Joined and
  resource_reclaimed. ProducerOutcome::Unknown remains Unknown after closing
  the live source; no fake EOF or business source success is inferred
- Cleanup cannot rewrite the original TaskReport; a second spawn is rejected

## New identities and immutable pins

Reusable: org.example.channel.bounded-sdk-reusable-001, version0.1.0-test58.1

- Wasm: fefc6f4524be62d8fee18aaa7640c10a0246af545c2588f6e42227b78bbd4fab
- Archive: 2627f628bf4a95c0ab134547f0e1defef300f48fd0d88aa918fa9df6842d8260

Matched one-shot: org.example.channel.bounded-sdk-oneshot-001, version0.1.0-test58.2

- Wasm: b2e4099f6b735862ed444983c455804919df919b32d03eb116f31c24a59a63d7
- Archive: dd2aa7184b6aef15a572f71313912142d3f35182cdf5fe23b94bb0b6b9783e15

The generated old Directory-template lock referenced offline-unavailable
cfg-if1.0.5. Only these four new fixture locks were explicitly selected offline
at cached cfg-if1.0.4, matching current SDK Cargo.lock. Existing templates,
guests, package versions, SDK/schema/ABI pins and production crate types/source
are untouched by this qualification. No new dependency/tooling or network/
authentication flow was introduced.

Standard reusable002: org.example.channel.standard-sdk-reusable-002, version0.1.0-test58.3

- Wasm:69e302745e9318659799135d5127735eb57b4468efe4ac85268569ef156895e2
- Archive:ba41b8ab64f7316eda789d7d091487bd4cbae1ab4ed70d18e247f5334071dfaa

Standard one-shot002: org.example.channel.standard-sdk-oneshot-002, version0.1.0-test58.4

- Wasm:733f6e71fbecb049fff51e198417eef35fa3763c1f6e80200ebc251c2ac09e4d
- Archive:e52275d8841cecad0dd2b3dc764ce427397c1d2acd187d746bb8c3830ac4d6ba

## Reproduction and evidence

All four source fixtures carry Cargo source/manifest/lock, plugin manifest, SDK source
lock and fixed ARTIFACT_SHA256SUMS. Their README gives pack commands. Run
sdk/fixtures/run-channel-bounded-001.sh to verify fixed artifacts and explicitly
execute all ignored plugin_runtime/tests/channel_bounded_sdk.rs tests.
Bounded001 final pack reproduced exact pins after formatting; standard002 is a new independently pinned build.
The script returns failure for the two preserved standard success assertions.

Key bundle evidence:

- channel-bounded-all-standard-final-real-wasmi.log:6/2 tests,20 exact runtime receipts
- channel-bounded-all-final-real-wasmi.log: prior bounded0015/5 and16 receipts
- channel-bounded-{reusable,oneshot}-final-pack.log: bounded001 build/pack/check
- channel-bounded-standard-{reusable,oneshot}-pack.log: standard002 build/pack/check
- channel-bounded-nearby-regression.log: 20 adjacent production-path tests
- channel-bounded-{format,diff-check}.log: clean checks
- channel-bounded-source-pins.json and source/log SHA256SUMS
- channel-bounded-runtime-receipts.json: full parsed receipts

Earlier failure logs are retained: offline cache miss; initial harness field/
count mistakes; first real four-frame budget failure; and an overstrict residual
fuel assertion. Final assertions reflect Wasmi basic-block fuel semantics.
A small nonzero balance can remain when the next basic block exceeds fuel.
No execution limits were changed to turn those failures green.

The source-only bundle excludes build/dist/target caches, Wasm/package binaries,
media and tooling. Apply the new fixtures to the pinned Morrow/SDK baseline before
rebuilding. Independent review and parent commit/GitHub/Drive synchronization
are separate subsequent steps, not claimed here.

No protected-storage, Linux/Windows product-owner, native Windows lifecycle,
network adapter, service, deployment, or universal frame-count qualification.
