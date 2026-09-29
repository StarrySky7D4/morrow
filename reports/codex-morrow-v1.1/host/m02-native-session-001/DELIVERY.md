# m02-native-session-001 / runtime-kit-001

Status: ready for independent joint review of the minimal real native control
slice. This is producer evidence, not independent acceptance or complete M02.

The reusable `native_session` crate starts real children and reuses the existing
core HostPolicy. It implements trusted admission, private-pipe Capnp handshake,
own-session read, online revoke/expiry, stop, actual exit and independent stdout /
stderr EOF observations. An unconfirmed close retains ownership and blocks reuse.
The runtime README defines the embedding boundary and remaining limitations.

## Current evidence

- Build and 18 real-process host cases: `run-20260928T192949834970Z/result.json`.
- Frozen ordinary plugin client, three real pairings (normal, revoke, stop):
  `pair-20260928T193003821828Z/result.json`. Revoke produced client exit 19 and no
  post-ack state read; normal/stop produced client exit 0. All three host runs
  observed actual exit, both EOFs and Released; host exit 0 means controlled closure.
- Malformed real child: `negative-cli-20260928T193111555966Z/result.json` proves
  host exit 2 with rejected_or_disconnected even though cleanup reached Released.
- CLI stdin kept open until after exit: proved by the build/qualification receipt.
- Final configuration binds exact allowlisted environment captured at admission;
  those same values are used at spawn. The final executable was rebuilt and all
  18 host cases and three plugin pairings rerun after this correction.

`runtime-kit-001/manifest.json` binds immutable binaries, runtime source copies,
current build inputs, raw run logs, documentation, generator/tool identity and
associated client/wire manifests. Use `bin/morrow-native-session-host.exe` from
that kit, not the mutable target directory. Rebuilding requires the manifest-bound
original workspace core and Capnp dependencies; the source copy alone is not a
self-contained repository. README was added after the final build; all compiled
source inputs still match that build's recorded after-snapshot.

## Contract correction and preserved failed attempts

The initial `contracts/experimental/agent_host_v2` custom fixed-frame format and
`wire-handoff.json` were an implementation deviation and are WITHDRAWN. They remain
untouched historical files. No runtime or plugin candidate consumes them.
Only `contracts/experimental/agent_host_v2_capnp` and `capnp-kit-001` are authoritative.
Their major 2/revision 1 schema and generated bindings stay frozen. host-kit-003
and the v1 canonical files remain unchanged. JSON in receipts/CLI diagnostics is
not the runtime wire schema.

Failed build/test attempts are preserved, not promoted to passing receipts:
`run-20260928T191820510155Z` lacked link.exe; `191857557001Z`, `191924849627Z`,
`191942788328Z` were unsuccessful compiler-environment setup attempts (including
Windows batch quoting errors, not network activity); `192014408948Z` exposed a
fixture flush omission, corrected before later runs; `192232421071Z` used an invalid
assumption that bounded output would fill the Windows pipe. That assertion was
withdrawn and the explicit dynamic-backpressure gap retained. Earlier successful
runs are historical predecessors; the current evidence above binds final source.

## Remaining acceptance gaps

The nonreading-peer case responds to control but did not reach OS backpressure;
partial response cancellation has source evidence only. Exact codec traversal and
nesting thresholds were not dynamically exercised. No claims of OS sandboxing,
descendant containment, delegated-handle prevention, hash-to-image-load race closure,
host-crash recovery, cross-process ownership, blocked operator stdout qualification,
production approval UI, Linux/device acceptance or business/data-plane success.

Product graphs remain 0/2 and the previously recorded 84 not_run cases remain
unqualified by this slice. G0/P02/J00 and full M02 remain blocked. The separate
plugin adversarial peer is available for independent review; it is not substituted
for the ordinary plugin client and was not run by this host producer delivery.

No existing tracked source changed. No commit, push, release, account/credential
use, real-library operation or arbitrary business execution was performed.
