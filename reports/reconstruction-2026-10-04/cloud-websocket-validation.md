# Linux WebSocket implementation and current SDK scope (LWS01)

2026-10-04. Baseline `513811bc172af570c41cb39a0f2b0f63e8318b3b`, tree
`cdaa16c61842d0389bbe18cc94eafa43893636aa`, rechecked against GitHub before takeover.
The old cloud workspace was empty; a fresh branch checkout contains 51,361 tracked
blobs. Every physical blob and symlink payload matched the commit. SDK327 and
frozen57 were unchanged; all 17 historical Linux convergence files matched the
prior source-only handoff. This is a new Linux verification, not a repeat claim of
the old 199 methods or historical 51,318-path recovery.

## Current execution status

Linux x86_64, locked/offline execution after an explicit locked cache fetch:

| Run | Actual unique named methods | Result |
|---|---:|---|
| Network, `managed-websocket`, lib + all integration targets | 96 | 0 failed / ignored / filtered |
| Original portable channel and frozen base regression | 49 | 0 failed / ignored / filtered |
| NEW fixture preparation guards | 6 | 0 failed |

Network breakdown: lib28, client10, managed_sse14, managed_ws10, sse13,
sse_envelope3, stream6, websocket9, ws_envelope3. The original 19 W15 methods
and five new lifecycle regressions are included in 96, not additional passes.
Both managed protocols actually execute NEW Rust/C/C++ Wasm/packages; one method
loops all three languages and remains one method. Before/after hashes for 1,444
source/build/fixture/artifact inputs were identical for the aggregate run.

Runtime breakdown: channel_compat5, controls15, faults20, frozen base9. The
Windows-only fault method, dependency/mapping/shared-object/reader/Windows-executor
suites are not counted here. Existing original C/C++/Rust base packages were run
unchanged; none was rebuilt or repacked. Expected injected producer panics have
passing parent methods and are retained in stderr.

The additional real TCP backpressure regression requires substantial completed
writes followed by a genuinely pending send; quota failure or an unsaturated run
cannot pass. Its focused run recorded 58 flushed messages and 3,866,624 transport
wire bytes before cancellation, then actual worker join in under 1 ms (1 s bound,
original 8 s deadline), with no socket-buffer or system-setting changes. Final
aggregate includes that method once. Three extra typed-lifecycle repetitions are
stability checks only. Default-feature cargo check also passed and adds no tests.

Host test profile is unoptimized + debuginfo; newly built guests use Release.
This is current Linux synthetic qualification, not Windows or production Release
qualification. Build/check/pack commands execute zero test methods. Historical
stage counts, repeated targeted runs and helper cases are not added to these totals.

## Source changes

The optional WS transport now owns cancellation before awaiting handshake setup;
lease-local cancellation is checked before queued delivery and during opening,
write and flush waits. Dropped opening futures no longer leave their own handshake
worker running until its deadline. The Close transport test separately requests
EOF before asserting successful finish, preserving the non-draining finish contract.

The managed adapter additionally rechecks its original SourceGrant after async
transport boundaries so a source Close/revoke/expiry is not reduced to the socket's
Cancelled cleanup result. A bad-ACK fixture now separately verifies structurally
invalid zero digest rejection and validly shaped wrong-digest Invalid response.

The previously uncompiled test suite had a wrong Client/EndpointPolicy import and
a helper shadowed by local variables. Those test-only compilation defects are
corrected with failure logs preserved. NEW Rust/C/C++ qualification source and a
bounded preparation tool replace the missing external-fixture prerequisite; none
of the old SDK or frozen artifact bytes are replaced. The preparation tool's
initial lexical path guard was independently found to admit `..` traversal; it
now rejects such paths, with a specific regression.

## Preserved failed attempts

- Initial no-run compilation exposed the pre-existing root import and helper
  shadowing errors in `tests/websocket.rs`; both failures are retained.
- Core packaging initially lacked `bitflags2.13.1` in the offline cache. The exact
  existing lock closure was fetched; no dependency or lock was changed.
- Concurrent Core packaging and network builds initially shared a target. Later
  compilation reported incompatible instances of the same pinned prost0.14.4.
  A fresh network-only target passed with identical source/locks. This supports
  build-directory interference, without asserting a proven unique root cause.
  All later manifests use separate targets; no old target was deleted.
- First managed WS run was 8 pass / 2 fail: typed source Close was lost to transport
  cancellation, and the malformed all-zero ACK fixture failed before dispatch.
  Narrow fixes were followed by managed10 and the complete network96 pass.
- The initial preparation guard's parent-traversal issue was a static-review
  finding; its new regression plus five existing preparation checks passed.

No failed run is overwritten or counted as a complete successful phase.

## Toolchain and qualification boundaries

Rust/Cargo 1.95.0 and Cap'n Proto 1.4.0 follow the current Core README and Windows
preparation instructions; they are not forced repository toolchain pins. Official
Rust component and WASI SDK34 downloads were checksum-verified; Cap'n Proto source
hash and tool binaries were recorded. WASI clang/LLD is 23.1.0. All are installed
inside the cloud workspace, with scoped child environments and no system settings.
Historical cloud Linux used Rust/Cargo1.96.0 and Cap'n Proto1.5.0; results from the
new environment are qualified separately and not claimed equivalent.

Tests use ordinary disposable Store data and keyless loopback only. Latest Windows
C01 remains 42 unique passes (dependency3/base9/mapping7/SharedObjects14/reader9).
Its protected Storage/IoWorker/DPAPI nine methods remain NOT_RUN; ordinary-user
Windows token, Directory003 production originals, protected/public owner binding,
GUI, real TLS/public endpoints and other-platform product qualification remain open.
Linux tests cannot close Windows gates. Counts from past stages are not added.

## Current capability matrix supersedes historical implementation wording


B = public bounded contract exists; E = implemented experimental slice; O = missing implementation; Q = qualification remains; D = specific product integration deferred. These are current source classifications, not new test results.

| ID | Current capability | Remaining boundary |
|---|---|---|
| F01 | B: seven content commands, including bounded read/create/edit/query (`core/schemas/runtime.capnp:3-17`) | General workspace/relations/drafts/batches and third-party Workbench content-route qualification absent |
| F02 | B: bounded typed bytes/JSON transform (`core/src/task.rs:13-18`) | Responses direct-or-transform work later; image-capability proof is not original Responses caller proof; CCswitch product D |
| F03 | E: `managed_sse::SseSource` POST SSE via original channel, original Store strict claim and exact ACK | Credentials, production Workbench, TLS/public provider and current-platform execution Q; not a new guest HTTP import (`docs/PLUGIN_SSE_CHANNEL.md:3-16,20,40-50`) |
| F04 | E: optional `managed-websocket`, `WsSource`, duplex old channel and separate envelope; current Linux synthetic execution passed | Public IO WebSocketConnect stays Unsupported; Windows C01 19 methods remained NOT_RUN; this Linux run does not replace Windows/account/WSS/public/production qualification (`network_node_stream_001/Cargo.toml:11-16`; `src/managed_ws.rs:1-3,16-30,58-118`; `core/src/io.rs:446-448`) |
| F05 | B: finite cancellation/deadline/resource lifecycle | Each actual producer exit and product race still requires evidence; cancellation is not effect rollback |
| F06 | E: finite channel credit/ACK/send/terminal/join contract; SSE source implemented, WS source present | Public Workbench binding Q; Accepted is local admission, ACK is durable consumption, producer completion is not OS-thread join (`managed_ws.rs:32-43,170-190`; SSE guide `:30-44`) |
| F07 | O: no authorized content changes/listChanges/subscribe command in runtime union | Need source authorization, cursor epoch/retention/paging/ACK/CAS and fresh grants; channel event bytes/journal are not a changes source (`runtime.capnp:8-17`) |
| F08 | B base+channel read-only host discovery; O complete extension discovery | IO/service/mutation are names only, no complete per-contract descriptor; no multiversion fallback or dynamic guest discovery (`workbench_host/src/sdk_profiles.rs:10-65`) |
| F09 | B: arbitrary bounded typed bytes; task value 64 KiB | No unbounded messages/object graph; type identity never grants access (`core/src/task.rs:13-18`) |
| F10 | O public guest blob/lease/mapping API; native internal object metadata exists | 16 MiB/64 segments do not imply guest zero-copy; Windows transfer still 64 KiB; non-Windows mapping Unsupported (`core/src/shared_object.rs:1-12,24-26`; `plugin_runtime/src/shared_memory.rs:1-6,25-45`; `sdk/rust/src/lib.rs:1-40`) |
| F11 | E independent mutation contract and Create/Delete | Windows product/platform Q; conditional replacement explicitly Unsupported, no weaker fallback (`core/src/mutation.rs:1-20`; `plugin_runtime/src/file_target/replacement.rs:5-38`) |
| F12 | B synchronous approved-slot pure dependency graph | Not asynchronous service discovery/auto-start; combined imports still rejected (`plugin_runtime/src/lib.rs:202-237`) |
| F13 | O public asynchronous dependency/service combination | Root deadline/cancel, intersected grants, aggregate budgets, reentry and join must be a separately bounded contract; do not relax existing factories (`plugin_runtime/src/lib.rs:229-237`) |
| F14 | E bounded outbound HTTP and authenticated finite inbound service; separate source extensions now exist | Existing IO SubmitHttp stays finite; no general stream upload/account SDK; production/TLS/platform Q (`core/src/io.rs:11-15,412-448`; `core/src/service.rs:1-15`) |
| F15 | O complete shared changes/blob/sync-state closure; Cloud product D | Cloud remains an ordinary optional plugin; no Store/SQL/signing authority shortcut; F07/F10/F21/F22 still open |
| F16 | B host-delivered bounded bytes and authorized content commits; private capture paths exist | General third-party capture/source authority still O/Q; private Workbench capture is not a public capture profile |
| F17 | B six-node declarative UI contract | C01 old guest protocol passes do not establish Flutter input/focus/render/close qualification (`core/schemas/ui.capnp:1-5`; C01 `:25`) |
| F18 | O rich UI/media/native-window public contract | Existing union is only column/row/text/button/textInput/toggle; new profile/resource/fallback/platform qualification required (`ui.capnp:3`) |
| F19 | O public exec/PTY/process-tree authority | IO capability enum has ten entries and no exec; trusted owner process supervision is not guest exec permission; Codex product D (`core/src/plugin_package/io.rs:27-41`) |
| F20 | D Codex multi-turn model/auth/writer/session integration | Existing task completion contract is not a multi-turn authorization lifecycle; do not advance product acquisition now |
| F21 | O public Account/OAuth/refresh/cookie/signer profile | Existing opaque HTTP references do not supply it; SSE/WS source explicitly refuses credential injection (`PLUGIN_SSE_CHANNEL.md:48`; `managed_ws.rs:248-250`) |
| F22 | O directory/list/watch/rename, generic attachment import/upload and conditional replacement | IO FileList/Poll/Write explicitly Unsupported; mutation Create/Delete is separate; no path-based privilege widening (`core/src/io.rs:398-410,460-466`; replacement `:5-38`) |
| F23 | B content query; E single-operation HTTP history via `OperationHistoryGrant` and `submit_operation_history` | Grant rejects non-HTTP capability; bodyless/status only; raw/brokered routes and all-kind/new-owner recovery remain open (`plugin_runtime/src/io_history.rs:25-58,118-161`; `io_jobs.rs:1547-1559`; `docs/PLUGIN_OPERATION_HISTORY.md:43-47`) |
| F24 | E finite service lifecycle ceiling <=1 h and bounded channel lifecycle | No unlimited daemon/general background API; per-platform close/switch-library/kill qualification Q (`core/src/plugin_package/io.rs:18-25`) |
| F25 | B fixed ABI/schema pins, explicit independent profiles and unknown rejection | Current source/old-original regression is per platform/run; no general multiversion decoder fallback; no replacing old artifacts to obtain a pass (`plugin_runtime/src/lib.rs:247-286`) |
| F26 | E trusted native adapter ABI; not untrusted native isolation/stable arbitrary dylib ABI | HostV1 callback/pointer lifetime is caller responsibility; client is neither Send nor Sync (`sdk/rust/src/lib.rs:62-90`) |


## Next bounded step and full-target decision

Finish this WS slice and preserve its exact evidence before expanding. Then extend
read-only host capability discovery for existing IO/service/service-resources/
mutation contracts with actual version/digest/limit metadata, without issuing grants,
opening owners, enabling production bindings or linking network transport solely
for advertisement. Keep `network_backend=false`, `workbench_binding=false` and
`production_public_binding_available=false` until their real gates are qualified.

The full 26-requirement SDK and ten freeze gates remain OPEN. Directory/blob,
authorized changes, async composition, account/auth, rich UI and product/platform
qualification remain real work. CCswitch/Codex product integration stays later;
future Responses work is direct upstream or bounded transformation as appropriate.
