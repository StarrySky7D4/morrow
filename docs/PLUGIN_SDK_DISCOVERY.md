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
Full SDK, production owner, Windows token/GUI and cross-platform qualification
remain open; a discovery result does not close those gates.
