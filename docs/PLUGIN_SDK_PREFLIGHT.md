# Static package preflight using the selected host

The repository developer tool can now prepare a selected package using the exact
trusted host executable that will receive it:

```sh
python tool/morrow_plugin.py check SELECTED.mplugin --host /path/to/trusted/morrow-workbench-host
```

This explicit mode does not compile the repository's separate runtime checker.
Without `--host`, the existing check path is unchanged. Pack, transform, SDK-only
refusals and source-distribution prerequisites retain their existing behavior.
The executable and selected package must be trusted local file selections; this
command is not a sandbox for arbitrary native programs or hostile filesystems.

## Discover before invoking

The tool first calls the existing `--sdk-capabilities` command and requires the
known version-1 `diagnostic_capabilities.package_preflight` advertisement. It
validates the command name, byte bounds, read-only/static-only status and no-authority
claims, then invokes its own hardcoded supported command. It never treats arbitrary
advertised text as a command line.

Older hosts are refused before the new package command or package read. This is
important because a legacy host can interpret an unknown first argument as a
database path. Do not manually pass `--sdk-preflight` to an unverified old host;
use the guarded developer-tool flow above. The host version string alone is not a
capability negotiation mechanism.

The actual native diagnostic is `--sdk-preflight PACKAGE`. The new host recognizes
this exact mode before its original owner/database path, validates argument count
before touching the archive, and returns one bounded JSON value. Existing discovery
content is unchanged after removing the additive diagnostic advertisement.

## What preparation establishes

The host uses the existing bounded package reader and original
`PreparedPackage::new`/`Runner::prepare` path. That validates package integrity,
manifest/contracts/features, allowed imports/exports and static module structure.
Wasm start sections are refused before module preparation. It creates no Wasm
Store or instance and does not invoke guest code, install a package, create grants,
open an owner/database or select a product route.

A prepared result includes the actual archive/module SHA-256, byte lengths,
package/ABI identity, declared limits, host defaults and their effective intersection.
Legal zero host-call budgets remain legal. An effective budget is a policy ceiling;
static preparation does not prove that allocation, a particular invocation,
platform ownership or a requested route will succeed under that ceiling.

The tool binds the selected host and archive bytes around the calls, checks the
response against discovery and the selected archive, and rejects contradictory
limits, identities, scope flags, duplicate keys and malformed/unknown protocol
values. The response stays `authority=none`, `grants_created=0`, with
`guest_executed`, `installed`, `routes_qualified` and `production_qualified` false.

## Results and bounds

For a successfully parsed `check --host` invocation (ordinary CLI argument-usage
errors retain their existing behavior):

- Tool exit0: a validated `prepared` receipt
- Tool exit2: a validated `rejected` receipt, retaining bounded phase/error detail
- Tool exit1: discovery, protocol, process or local-input failure; no valid receipt

The JSON tool receipt binds host, descriptor, archive and response observations.
A rejected result with `package=null` has no host-attested decoded identity; its
outer archive digest is the consumer's selected-file observation. Native
protocol exit0 and exit2 must agree with the response status; unexpected exits or
false execution/authority claims cannot become successful preflight.

Current version-1 bounds are 4,276,930 archive bytes, 4,194,304 module bytes and
16,384 response bytes including the native trailing newline. The consumer retains
a five-second subprocess/pipe deadline, bounded cleanup waits and output limits.
This is not a five-second end-to-end operation or a process-tree reclamation
guarantee. Preparation can reject
unknown features, malformed archives/modules, missing required exports, forbidden
imports and other cases without running a guest.

## Local-path and qualification limits

Direct native use rejects ordinary directory/device/FIFO/symlink/reparse inputs
before opening them. The path precheck and original reader are separate operations;
a concurrent replacement or ancestor change can race the open. This is not a
hostile-filesystem containment proof. A direct native invocation has no universal
wall-clock guarantee under a path-replacement race or stalled filesystem. Use the
bounded consumer with trusted local selections; host/package hash rechecks remain
required and do not turn a native program into a security sandbox.

Current verification is Linux Debug static preparation. No guest execution,
Windows owner/token, public TLS/account, production binding, GUI or complete SDK
qualification follows from `prepared`. Existing frozen SDK originals remain
unchanged. See [P01 validation](../reports/reconstruction-2026-10-04/sdk-preflight-validation.md)
for exact actual cases and preserved failures.

## Without a full repository

Use the separate [diagnostics source bundle](PLUGIN_SDK_DIAGNOSTICS.md) and its
`preflight PACKAGE --host HOST` command. It loads the same original protocol
validator from its adjacent source file, without a repository/compiler fallback.
This does not change SDK-only `morrow_plugin.py check --host` refusal behavior.
The diagnostic retains all static-only, no-authority and local-path limits above.
