# Read-only SDK contract discovery

Run the explicitly trusted host's `--sdk-capabilities` command, or use
`python tool/morrow_plugin.py profiles --host /path/to/trusted/morrow-workbench-host`.
The host returns compiled metadata before opening an owner, database, guest,
listener or network connection. Discovery gives no authority and does not replace
package validation, exact contract matching or a current route-specific grant.
The query tool is not a sandbox for arbitrary executables: its host path must be
explicitly trusted. It binds the executable and raw descriptor SHA-256 and retains
its bounded output/time/error checks.

## Additive compatibility

Top-level descriptor schema1, both legacy base/channel profiles and the legacy
experimental-extension status remain unchanged. Optional
`experimental_extensions.discovery` has its own schema1 and
`status="compiled_metadata_only"`, `authority="none"`. New consumers accept legal
old descriptors without this field; consumers that understand it validate it
strictly. Existing consumers can continue reading the unchanged legacy projection.
The detail records are experimental contract metadata, not newly frozen guest ABIs.
Their required/optional feature lists are extension-specific prerequisites, not a
complete Manifest recipe. For example, transform-handlers-v1 remains conditional
on actual transform-handler metadata; it is not a blanket ABI2 or IO requirement.

Four detail records describe the existing IO, service, service-resources and
mutation contracts, using the compiled Core version and canonical digest functions.
They separate byte ceilings, item/count ceilings and duration ceilings; package
budgets and current grants can be stricter. Do not convert a count into bytes or
use a long service-run ceiling as a per-request timeout.

## Optional channel payload metadata

C03 additionally publishes independently versioned optional
`channel.payload_discovery` for `changes-metadata-v1`. It advertises the exact
payload version/digest, Events-only required features, bounds and native approval
prerequisites; authority remains none and production routes remain empty/false.
This preserves descriptor schema1, legacy base/channel fields and the four
extension records above. It does not advertise WS/SSE or directory/blob codecs as
new package features or imports. Missing optional detail grants no authority.
See [the changes guide](PLUGIN_CHANGES_METADATA_SDK.md) for field meanings and
old/new host-consumer compatibility.

## What the records mean

| Detail | Implemented scope | Boundaries that remain explicit |
|---|---|---|
| IO | Existing bounded job Read/Finish/Cancel and separately approved HTTP submit/history routes | A declaration name does not implement its submission discriminator. IO FileRead/List/Create/Replace/Delete, listener/publication/service-reply and WebSocket submissions, Poll and Write remain Unsupported where the original decoder says so |
| Service | One-shot Invocation/Reply plus optional finite run and cumulative-budget extensions | Service request/reply messages and public IO submission are different contracts. A listener, publication or upstream endpoint still needs its original trusted owner/grant |
| Service resources | Opted-in, host-injected bounded endpoint metadata | Opaque references do not grant endpoint/credential use or discover arbitrary accounts. Endpoint metadata ceilings do not enlarge the IO wire or per-request deadline |
| Mutation | The independent bounded PrepareCreate/PrepareDelete, Chunk, Commit/Execute, Query, CancelPlan and Release contract | Native managed mutation execution is Windows-specific; conditional Replace stays Unsupported. Content/chunk ceilings and package mutation budgets are separate |

The trusted Workbench `start_service` route specifically requires both the
service-run and service-run-budget features through its budgeted binding. A generic
IoWorker service route does not make those optional features universally mandatory.

HTTP operation history is one exact operation's bodyless status information under
a fresh grant. It does not recover response bodies, authorize another submission,
renew an old grant, reconstruct remote effects or automatically replay Unknown.

The records distinguish compiled runtime implementation, named trusted host routes
and platform/owner prerequisites. The ordinary HTTP backend can be compiled even
when the current Workbench product owner backend is unavailable. In particular,
Workbench protected Storage open is unsupported on non-Windows in this checkpoint.
Compiling the route does not make the production host usable on that platform.

The inherited channel fields remain `network_backend=false`,
`workbench_binding=false` and `production_public_binding_available=false`.
Standalone opt-in SSE/WS transport qualification is not a linked, approved public
Workbench route. This metadata change activates no backend, creates no owner,
changes no permission and issues no grant.

## Consumer responsibilities

1. Select only a contract and version/digest the caller understands
2. Keep implementation/route/platform prerequisites distinct from capability names
3. Intersect published ceilings with package budgets and fresh host grants
4. Perform the normal package/module/Runner and trusted route admission checks
5. Reject unknown or malformed understood-detail versions instead of assuming support

For current implementation and execution evidence, see
[the D01 discovery validation report](../reports/reconstruction-2026-10-04/sdk-discovery-validation.md).
Full SDK, production owner, ordinary Windows token/GUI and cross-platform
qualification remain open; a discovery result does not close those gates.
Current project scope and next gates are in [project status](PROJECT_STATUS.md).

## Selected-host package preparation

An optional, separately versioned diagnostic advertisement can expose static
package preflight on the exact selected host. The guarded `check --host` flow
first requires that advertisement, preserving safe refusal on older hosts. See
[the preflight guide](PLUGIN_SDK_PREFLIGHT.md). Preparation grants nothing and
does not establish runtime, route, owner or product readiness.

## Standalone diagnostics source bundle

When a full repository checkout is unavailable, the separate
[standalone diagnostics entrypoint](PLUGIN_SDK_DIAGNOSTICS.md) provides `profiles`
and static `preflight` using this same original validator. Its source ZIP contains
no host, Core/runtime or SDK library. The existing SDK source distribution and
project-tool refusal gates are unchanged; a byte inventory is not trusted
provenance or runtime qualification.

## Independent directory request profile (C10)

`experimental_extensions.directory_request_discovery` is a separate optional
versioned record. It requires `io-v1` and `fs-directory-request-v1`, the fixed
`morrow_fs_directory_v1.call` import, and exact request/page/IO digests.
The original four extension records, base profiles and `feature_names` stay unchanged.
An absent record does not advertise this new profile. Discovery is not authority.

Raw task input is the original Open request (at most 512 bytes); completion must
match the last directory response (at most 65,536 bytes). `read_input` still requires
131,072 bytes of writable capacity. `standard_typed_task_helpers=false`:
ordinary typed task envelopes cannot be substituted.

Static preparation is implemented; the native adapter is compiled on Windows.
`production_public_binding_available=false` and `workbench_routes=[]`.
Metadata does not create a selection, capture, grant, native handle or OS path.
Workbench compiled checks are not product execution. See the
[C10 report](../reports/reconstruction-2026-10-05/directory-request-sdk.md).
