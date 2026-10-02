# Fresh Directory contract-equivalent Wasmi comparison003

## Result and boundary

One separately approved fresh comparison pair:3/3 explicitly ignored tests pass,
covering reusable and matched one-shot byte/event runs. All four execute Ok(0),
11 metered channel calls, original5×32768-byte frames (163840 bytes), explicit
Close and independently proved actual original producer join. Execution remains
20M fuel /16MiB /16 calls. Both fresh Rust projects retain generated opt-level3.

This is fresh source/implementation evidence, not recovered historical Wasm or
package bytes. It does not establish universal size/count support or protected
Linux/Windows product-owner qualification. The separate001/002 ownership-stress
fixtures, source/artifact pins and real Limits/red results remain unchanged.

Scope correction:002 uses the historical byte vector with stronger ownership-
stress operations (Query calls and retained-vector/equality scans). It is not
identical historical Directory operation semantics, and its failure cannot
establish the immutable Directory example's failure. The earlier report now
states this explicitly; its original frozen report remains in the prior archive.

## Strict matched-success assertion after review

Independent review identified an overly permissive control branch that could
let one-shot Limits return early while the test aggregate stayed green. That
branch and the unused Fault import are removed. Both reusable and one-shot now
unconditionally require the identical Ok(0),11calls, complete SHA/output,
original ACKs, explicit Close and actual-join success contract.

The bounded003-only locked/offline rerun passes3/3, with all four actual outcomes
and fuel receipts unchanged from the prior run. All ten event ACKs still pass.
Complete before/after source captures match during execution. The prior run log
and source-only archive are retained;001/002 fixtures, artifact pins, stronger
stress semantics and genuine negative expectations remain unchanged. No new
cases, guest/package builds, budget changes or production edits were introduced.
The current strict rerun is channel-directory-comparison-strict-real-wasmi.log,
with exact JSON receipts and a newly named strict source-only archive.

## Established baseline equivalence

Immutable reference: sdk/examples/rust-channel-directory/src/lib.rs, SHA256
137e218a93e49ffa530998c7114d3ff247745f7b9f1e1af91347066799b83fcd.
The fresh one-shot source is byte-identical. The fresh reusable source differs
by exactly two transport changes: construct WasmClient before the submit closure,
and replace call_wasm submission with client.call. No other source operations
are changed. The source-equality receipt and exact diff are included.

Preserved behavior:

- Handler channel.directory.consume, canonical morrow.channel.directory.v1
- Original request domain morrow.channel.directory.request.v1
- Limit min(32, max_messages, (max_requests-1)/2)
- Receive → one standalone frame.digest → transcript.update → original Ack
- Original frame sequence/hash/cursor, with the response/frame owned through Ack
- Explicit final Close and original CHV1 business-output contract
- No Query, added ownership scans, retry, replay or limit increase

## Exact runtime receipts

| Transport | Kind | Outcome | Calls | Fuel used | Fuel remaining |
|---|---|---|---:|---:|---:|
| oneshot | ByteStream | Ok(0) | 11 | 14161907 | 5838093 |
| oneshot | Events | Ok(0) | 11 | 14165987 | 5834013 |
| reusable | ByteStream | Ok(0) | 11 | 14129557 | 5870443 |
| reusable | Events | Ok(0) | 11 | 14133637 | 5866363 |

All outputs contain five frames,163840 bytes and payload SHA256
12a8659000a14e107b88bc40ee759d70c4749f68c01ee54157537b71008d6d16.
Exact64-byte CHV1 output hex is recorded for each run in the JSON/raw log.
Reusable saves32350 fuel in each case, approximately0.23%; this is a modest
matched measurement, not a broad performance claim. Fresh one-shot also passes.

The production path is actual Rust Wasm → morrow_channel_v1.call → authentic
ChannelBroker/Manager/ManagedInstance/HostRuntime and SQLite. The trusted caller
supplies canonical Directory and real source frames; no native transport shim
or source authority is supplied by the guest.

Ten event ACK receipts are reopened from SQLite. Each original frame/seq/hash/
cursor and correlated response is verified. ACK call IDs independently reproduce
the exact baseline domain and Receive/ACK counter ordering (ACK serial2×seq).
Guest Close reports ClosingUnconfirmed on these runs; host separately proves
CleanupProof::Joined and resource_reclaimed. ProducerOutcome::Unknown remains
Unknown after closing the live source, so join is not fake EOF/business source
success. Cleanup leaves the original TaskReport unchanged and replay is rejected.

## New identities and pins

Reusable: org.example.channel.directory-sdk-reusable-003, version0.1.0-test58.5

- Wasm SHA256:45a00921a51a8fc68922c7270b57a2b1feb7dc8f225c9de5a608668ccda370cb
- Archive SHA256:cde1985d2fe4adbe1ccd1720fb311d0c21d87f9bb63edc3b158954fd78fee31a

One-shot: org.example.channel.directory-sdk-oneshot-003, version0.1.0-test58.6

- Wasm SHA256:419655c79add7f1dbc1055e1cfd729d4581d67440dd296d9b0b7533a9dec77d0
- Archive SHA256:5e2813bf4c758acd9bb1f792c958a6de8dcb0410a2810fa963a95f1ec8ae977a

Only new fixture locks use cached cfg-if1.0.4 matching current SDK Cargo.lock;
old Directory template lock/source/package and production SDK/runtime source,
crate types, schemas, versions and pins remain untouched. No new tooling,
dependency, authentication, private socket or network retries are introduced.

## Reproduction and retained evidence

Build the two003 projects with tool/morrow_plugin.py pack and --require-sdk-lock,
then run sdk/fixtures/run-channel-directory-comparison-003.sh. Fixed
ARTIFACT_SHA256SUMS are checked before explicitly running the three ignored
plugin_runtime/tests/channel_directory_comparison.rs tests. No other comparison
pair or additional matrix is added.

Evidence includes:

- channel-directory-comparison-real-wasmi.log and runtime-receipts.json
- channel-directory-comparison-baseline-source-proof.json and exact source diff
- channel-directory-comparison-{reusable,oneshot}-pack.log
- channel-directory-comparison-prior-stress-preservation.json
- channel-directory-comparison-{validate,format,shell-check,diff-check}.log
- Source/log SHA256 manifests and source-only bundle receipt

The source-only overlay carries new source/manifests/locks, corrected reports,
Wasm/package pins and logs, excluding binaries, build/dist/target caches and media.
The prior002 source-only archive is unchanged (SHA256
5efd3bbfae61c8aed79428b4b47b37542fc446e75a84a3e6f0c631d1c1e76dcc).
Independent review and parent commit/GitHub/Drive synchronization remain separate.
