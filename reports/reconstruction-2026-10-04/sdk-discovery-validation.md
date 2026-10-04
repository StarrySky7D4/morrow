# D01: existing SDK extension discovery completed

2026-10-04. Baseline commit `5d9e9f2aea533c2f2cf879f7b0991f18a2102c99`,
tree `ed52ca70bf9f77c8260a20a3cc0c2e572411797b`. A fresh checkout was verified
against all 51,368 baseline source paths, including symlink payloads, before work.
This stage extends the actual host CLI and developer consumer; it does not activate
a new backend or grant.

## Developer-visible result

`morrow-workbench-host --sdk-capabilities` now adds the independently versioned
`experimental_extensions.discovery` object to its existing schema1 descriptor.
Four records expose compiled IO, service, service-resources and mutation identities,
exported limits, required/optional extension features, recognized declarations,
implemented operations, unsupported operations, named trusted routes and explicit
platform/owner prerequisites. The actual Python `morrow_plugin.py profiles` command
understands and validates this detail while accepting old hosts without it.

This closes the bounded F08/G03 extension-detail discovery gap identified after the
WS stage. Developers no longer need to interpret recognized capability names as
proof that a submission or current-platform product route exists. General dynamic
guest discovery, multiversion negotiation, production owner/binding and full SDK
qualification remain open.

The top-level schema, base/channel profiles, old experimental status and feature
names are preserved. Detailed records are `experimental`, metadata-only and
`authority=none`; package preflight remains required. The required-feature lists
are extension-specific, not a complete Manifest recipe. No SDK327/frozen57,
schema, dependency, lock, channel/network transport or original owner code changed.

## Precise scope

- IO has ten recognized declaration names but five bounded implemented operations:
  Read/Finish/Cancel on selected resources, approved SubmitHttp, and the dedicated
  managed QueryOperation route. The original Unsupported submission discriminators,
  Poll and Write remain explicitly unsupported
- HTTP history is bodyless status for one exact operation under a fresh grant;
  it grants no dispatch, response-body recovery or replay authority
- Service Invocation/Reply is separate from optional run/budget features. The
  actual Workbench service route specifically needs its budgeted service-run binding
- Service-resources metadata is host-injected for opted-in packages and rechecked
  by the live broker. Its directory limits cannot enlarge IO wire/deadline ceilings
- Mutation preparation/chunk/commit/execute/query/cancel-plan/release remains a
  distinct bounded contract. Native managed execution is Windows-only; conditional
  Replace is still Unsupported. Wire content limits and optional package budgets
  are described separately
- Compiled native file/HTTP/service code does not imply a usable protected
  Workbench owner. The current Linux descriptor correctly marks protected owner,
  stored HTTP credentials and protected TLS identity backends uncompiled
- Existing channel `network_backend`, `workbench_binding` and
  `production_public_binding_available` remain false

See [the developer guide](../../docs/PLUGIN_SDK_DISCOVERY.md) for use and consumer
responsibilities. Source route names were checked against real constructors and
entry points, including route-specific service feature requirements.

## Actual checks

Linux x86_64, Rust/Cargo1.95.0, Cap'n Proto1.4.0, locked/offline after an explicit
fetch of Workbench's existing lock closure. These are the current README/Windows
reproduction versions, separately qualified from historical Linux1.96/Capnp1.5.
Host builds/tests are unoptimized + debuginfo.

| Check | Actual result | Boundary |
|---|---|---|
| Rust `sdk_profiles` | 7 passed, 0 failed/ignored; 83 filtered | Targeted module, not all host tests |
| Python profile consumer | 20 passed | Includes old-descriptor acceptance, typed/malformed/contradictory detail rejection and bounded process failures |
| Existing channel tooling regression | 6 passed | Separate unchanged tooling regression |
| Baseline and final host builds | Both exit0 | Compilation is not a test count |
| Actual host/consumer compatibility | All four old/new host × old/new consumer combinations passed | Real compiled executables and the saved original consumer |
| Actual `morrow_plugin.py profiles` command | Passed against final host | No fake descriptor substituted |
| Additional discovery argument | Exit1; no requested database created | Early argument rejection |

The original host output is 4,441 bytes; the final output is 15,215 bytes, below
65,536. Removing only the new discovery object from the parsed final output gives
exactly the parsed baseline descriptor. This is a JSON-content projection check,
not a claim that the expanded raw descriptor has identical bytes. All four new
contract digests independently match their normalized Core source schemas.

Final host SHA256: `65460b70194a25c332ec502de5d369886166316fd838582ea9a15ac45686122d`.
Actual final descriptor SHA256: `4163ea97b127aafad82e75a3eb6e42cf6f84f432203d49c7789fae160a6752e3`.
The exact reviewed implementation/tool inputs remained unchanged through final
compilation and CLI/consumer verification. Formatting and scoped diff checks passed.

The six new Rust and twelve new Python methods are already included in the totals
above. Repeated runs, subtests, build commands and prior WS-stage counts are not
added as new passing methods. No Windows test count is changed by this stage.

## Read-only evidence and retained limitations

The CLI branch returns before owner, database, package, guest, listener or account
initialization. New descriptor helpers use only compiled constants, digests and
in-memory JSON. Actual untraced runs in fresh empty HOME/TMP/CWD left all three
locations empty, and an extra argument did not create the supplied database path.

An attempted syscall trace exited1 before host execution because this cloud
environment rejects PTRACE_TRACEME/PTRACE_SETOPTIONS with Operation not permitted.
The failure and empty trace are retained. No privilege/security setting was changed;
no syscall-level no-network/no-file-access proof is claimed. Source early-return
inspection and observed empty-directory behavior are the supported evidence.

The first offline baseline build also exited101 for a missing rcgen cache entry.
Fetching the exact existing Workbench lock closure allowed the repeated baseline
build; no version, pin or lock was edited. The failure remains preserved.

This stage does not qualify current Windows execution, protected keys/owner,
ordinary Windows tokens, public TLS/accounts/endpoints, GUI or full cross-platform
behavior. Windows C01's existing42 unique methods and protected9 NOT_RUN retain
their original separate scope. Full-target SDK freeze remains OPEN.
