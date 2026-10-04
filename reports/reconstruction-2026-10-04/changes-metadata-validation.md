# G06: explicitly approved finite card-change metadata source

2026-10-04. Baseline `ffbc30b735c9f28da5925fb199687ca79e8d5af0`, tree
`89e8e892dc16733b483a696170f1d72c01537c06`; 51,381 baseline source paths and modes
were verified before implementation. A separate contract/compatibility review
preceded the cross-layer changes. This is a real metadata-source implementation,
not a declaration that the complete target SDK or Cloud product is frozen.

## Delivered slice

The new `changes-metadata-v1` feature composes only with the existing bounded
Events channel. Its independent canonical wire has maximum 662-byte notifications,
strict version/digest/identity rules and no global event positions. Old schemas,
SDK327, frozen57, old pins/locks, channel/content mixing gates and SDK source-v1
closure remain byte-identical. Store schema remains 24 with no table, index,
migration, writer or ACK-journal format change. A new extension has its own Cargo
manifest/lock, without upgrading old inputs.

Core now reads permanent card-operation events with fixed-set filtering and
bounded candidate, container, decompression, retained-output and duration budgets.
It materializes owned metadata before guest waiting. The typed runtime approval
binds the exact receiver, package, Manager/connection/Control, live Store, complete
card set, window and deadline. Its dedicated guard is installed before production
and is checked at read/decode/release/Receive/final-ACK boundaries. Existing network
and local source semantics remain their original branches.

Latest-ACK reopening verifies the original same-Store checkpoint and original
frame/request/response/cursor, recomputes scope, checks the real permanent anchor
and then requires fresh approval for a new finite window and epoch. It never
restores an old grant or hidden upper bound. Logical Store identity is not a
physical-file, anti-clone or rollback-lineage proof. Global ACK capacity remains
4096; no deletion, refund or new epoch bypass was added.

The independent Rust decoder and shared C ABI/C++ wrapper use the old channel
transport. Three new Wasm guests and a new explicit pack/preparation route live
outside the frozen SDK. Their imports are exactly task read_input/complete and
channel.call. No Core/Store/SQL/path/WASI/network import is introduced. New packages
have empty content/IO/dependency/service/mutation permissions; old hosts reject the
new required feature before guest invocation.

See [the source guide](../../docs/PLUGIN_CHANGES_METADATA_V1.md) and
[the independent SDK extension](../../extensions/changes-metadata-v1/README.md).

## Current actual evidence, counted separately

| Run | Result | Exact boundary |
|---|---|---|
| New Core integration | 18 passed | Real transactions, filters, budgets, provenance, cursor/anchor and shared codec vectors |
| Complete Core unit invocation | 76 passed; one ignored | Includes three guarded temporary-SQL cases and one receiver-probe case; those four are not extra passes |
| Original sealing suite after fixture correction | 4 passed | Test-only v4-fixture repair; no production migration change |
| New runtime, explicit fault-injection build | 32 passed; zero ignored | Includes real Store/three-language guests, latest-ACK reopening, final revoke/expiry rollback and 4096-receipt refusal |
| New runtime, production feature build | 30 passed; zero ignored | Subset of the preceding 32, with no fault-only timing/revocation seam |
| Existing selected runtime suites | 57 passed | channel_compat5, controls15, faults20, http_io5, manager11, manager_storage1; unrelated to the frozen57 file inventory |
| Existing network aggregate | 96 passed; zero failed/ignored/filtered | Real loopback SSE/WS plus original network regressions against current Core/runtime |
| Frozen guest-base original packages | 9 passed | Original C/C++/Rust content/transform/UI Wasm execution, without rebuilding/repacking originals |
| Frozen transport original-package admission | 1 passed | One method checks six originals' module/IO/service declarations and budgets; no transport business IO execution |
| New SDK Rust tests / preparation tests | 3 / 4 passed | Separate codec/FFI and preparation gates |
| Shared cross-language vector corpus | 209 identical verdicts per language | Six valid, 203 rejected in Rust/C/C++; not 627 unique SDK methods |
| Core portable Wasm library check | Passed; zero tests | Native-only Store source APIs excluded; does not qualify a Web source backend |

These runs and historical phases overlap. They are not summed into an SDK pass
count. The existing frozen dependency suite is Windows-gated and was not run on
Linux; no cfg was weakened and zero tests is not recorded as a pass. Previously
reported Windows42 passes retain their prior scope. Protected9 and the new
profile's Windows/product execution remain unqualified here.

Each actual new Rust/C/C++ Wasm guest consumed three metadata events from real
Core card commits, while secret-card events were present and not delivered.
The exact original new-package bytes were pinned and executed without fallback.
Each guest produced three original durable ACK receipts in the same HostRuntime
Store, followed by separately observed producer completion and actual cleanup.
Synthetic actor approvals exercise the product authorization types; no real user
content, account, secret or persistent external authorization was used.

The final new package SHA256 values are:

- Rust: `c91b401ddd718654ed05de129e22d9f371737087bfcb58c395c288ad0361e928`
- C: `4b0f925332125ae646954625727d4e1bec1e740edb0328712bf2a94e656d1108`
- C++: `c07855408a7df1f9706293abcb2567774e7c121152e3e403420b1d37812eee87`

The normative wire SHA256 is
`07fc0dc4c48eb17f85309207a441d6a07f3fb8e06aab5e2ce337a4b3c98f9089`.
Only the two new normative file paths receive explicit LF checkout attributes;
old attribute rules and old contract bytes remain unchanged.

Rust/Cargo1.95.0, Cap'n Proto1.4.0 and existing WASI SDK34 were reused, offline and
locked. These do not relabel older Linux1.96/1.5 or Windows qualifications. Final
runtime verification binds 26 inputs including Core/runtime sources, manifests,
locks and three Wasm/package pairs; the network run binds 519 source inputs and
12 unchanged previous SSE/WS artifact files. Both have zero before/after drift.
Core and SDK have separate exact final source and original-input inventories.
No new whole repository copy, dependency fetch or native target directory was
needed; existing per-purpose build caches were reused.

## Failures and corrections preserved

- A new receiver probe initially survived owner destruction. It now observes a
  private original-instance Weak lifetime; draining, disconnect, owner destruction
  and replacement cannot revive it. Existing grant Drop semantics were not widened
- Same-HostRuntime Store replacement could otherwise retain HostBinding. New
  opaque live-Store binding checks reject replacement even if the original Store
  remains alive or another handle shares its logical identity. An observed
  mismatch permanently revokes; swap-back does not revive the source
- Authority is rechecked after SQL payload retrieval and before decompression.
  The targeted corrupt-payload test shows a read/decode-boundary revoke wins first
- The initial reopen test regressed its trusted tick from two to one. The original
  ledger correctly refused it; only the test tick was corrected, without weakening
  receipt/scope/anchor checks
- Initial Core unit IO failures used an unwritable inherited XDG state directory.
  Only per-test environment selection changed to a new synthetic state directory;
  user/system state or security settings were not changed
- The old sealing test failed identically on the untouched baseline and candidate.
  The baseline's 228 Core tracked files were verified against Git blobs. Its
  synthetic v4 fixture retained v24 channel ACK tables. Only the fixture now drops
  those two tables before declaring v4; production migration/integrity code did not
  change. The original failures and the corrected four-test run are retained
- Cross-physical-manifest Cargo cache aliasing after that A/B produced missing-new-
  API compile errors. A timestamp-only current-lib invalidation forced the correct
  source rebuild; only later hash-bound current-source results count. No cache was
  deleted to hide the failure
- New pack tooling initially exceeded the old 64 KiB task input limit; it was
  narrowed rather than raising the original limit. A missing optional-SDK export
  reference in the combined Rust archive was corrected without linking two runtimes
- A new lock initially selected a different already-cached cfg-if version. Only
  the new lock was aligned offline to the original SDK version before the final
  build. Old locks and pins remained unchanged

The final-ACK tests distinguish a precise transaction boundary from a generic
terminal state. Fault-only expiry waits at the original second Store callback,
after both SQL writes, until the actual original deadline; the existing guard
rejects and both rows roll back. Ordinary Revoked/Expired or an uncertain failure
does not by itself prove no ACK committed; original durable history remains the
source of truth. Production builds contain no added final-commit wait or fault
revocation seam. Source, original artifacts, failed attempts, hashes and independent
review remain separately recorded.

## Remaining boundaries

- This is a native library source adapter, not a Workbench GUI/catalog/discovery
  route. Existing Workbench discovery and production-binding flags were unchanged.
  Static package preparation does not establish an installed product route
- The Core guarded temporary-SQL proof and the ordinary temporary-Store runtime
  proof are separate. ProtectedStoreSlice still cannot traverse the full
  HostRuntime/channel owner path; no public test constructor or fallback was added
- Metadata includes whole-card fingerprints, which reveal equality and can test
  guesses about low-entropy content. It does not send original body/title fields,
  but does not promise zero content information. Future real approval must say so
- No forever watch, arbitrary object discovery, historical body access, selective
  revocation with silent skipping, rolling retention/ACK GC, original-window crash
  restoration, content write or full Cloud sync is supplied
- Logical quotas are not an allocator peak-memory proof; logical Store identity
  is not physical-file attestation or anti-rollback/anti-clone evidence
- Windows/macOS/protected owner, ordinary-token, GUI and complete SDK/product
  qualification remain open. Existing historical evidence is not upgraded
