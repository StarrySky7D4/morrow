# P01: static package preflight on the selected host

2026-10-04. Baseline `eba50eeaa207715f6f5b73b5b2a4201f1429817c`, tree
`77049bf25d531165e91d1f0d3733476369f73f4b`. All 51,370 baseline source paths
were verified before editing. This stage adds an actual native diagnostic and a
guarded developer-tool route, preserving the original package preparation logic.

## Developer-visible result

`python tool/morrow_plugin.py check SELECTED.mplugin --host HOST` now uses the
explicitly selected trusted executable for static package preparation, without
compiling the repository's separate checker. Default check, pack, transform and
SDK-only/source-distribution refusal behavior remain unchanged.

The consumer first verifies the separately versioned
`diagnostic_capabilities.package_preflight` advertisement through the existing
`--sdk-capabilities` route. Missing or unsupported advertisement stops before the
new command or package read. This protects old hosts whose unknown first argument
can be interpreted as a database path. Command text is hardcoded after validation;
arbitrary advertised strings never become executable arguments.

The new native `--sdk-preflight PACKAGE` mode validates exact arity before archive
access and the original owner/database path. It reuses bounded
`catalog::read_file` and `PreparedPackage::new`. Original Runner preparation rejects
start sections and validates imports/exports without creating a Wasm Store/instance
or invoking a guest. Original Core, Runner, schemas, pins and lock files are unchanged.

A successful receipt identifies the archive/module and the declared, host-default
and effective intersected limits. Legal zero host-call budgets remain accepted.
Preparation does not establish runtime allocation/execution-fit, dependency
resolution, current grants, routes, platform owner or product readiness. A trap-only
entrypoint is deliberately accepted statically without being invoked.

Native exit0 means prepared; native exit2 means a bounded rejected phase/error.
The consumer validates both, including exit/status agreement. The developer CLI
returns exit0 with a validated prepared receipt, exit2 with a validated rejected
receipt, and exit1 without a valid receipt for input/discovery/protocol/process
failure. Host, descriptor, selected archive and response identities are bound.
`authority=none`, grants0, and execution/installation/route/production flags false
are enforced for either diagnostic outcome.

See [the preflight guide](../../docs/PLUGIN_SDK_PREFLIGHT.md).

## Actual verification

Linux x86_64 Debug, Rust/Cargo1.95.0 and Cap'n Proto1.4.0. These are the current
README reproduction versions, not a requalification of historical Linux1.96/1.5
or Windows. Cargo remained locked/offline with the existing cache and a separate
Workbench-only target.

| Verification | Actual result | Counting scope |
|---|---|---|
| Native CLI integration tests | 8 passed | Test methods, including looped original-package and negative-input cases |
| SDK metadata/diagnostic unit tests | 8 passed; 83 filtered | Includes the seven existing discovery tests; not the whole host suite |
| Python focused regression | 150 passed | 30 preflight, 20 profile, 77 plugin/project/distribution and 23 channel methods |
| Independent actual native CLI matrix | 32 expected outcomes | Separate recorded command cases, not extra unit-test methods |
| Actual developer CLI against new host | 27 expected outcomes | Separate command cases, overlapping originals and negatives |
| Actual legacy-host protection | Both prior binaries refused safely | Each received only `--sdk-capabilities`; no new flag or package read |

The 27 real Python CLI runs use an empty PATH: 13 unchanged guest originals and
six unchanged IO/service transport originals prepare, as does a synthetic trap-only
entrypoint. Seven invalid module/import/start/entry/feature/archive cases produce
validated structured exit2 rejections. Successful receipts verify exact budget
intersection and host/archive identity; rejected results retain their original
phase/error and never acquire effective limits or authority.

Native cases additionally cover wrong arity before FIFO access, ordinary directory/
device/FIFO/symlink rejection, non-ASCII and Unix native non-UTF-8 paths, corrupt and
oversized archives, unknown required features and an oversized embedded module.
Negative fixtures are newly generated and distinct from frozen originals. No original
guest was rebuilt, repacked or resealed. Both archived sets and all SDK327/frozen57
source bytes remain unchanged.

The final selected native binary SHA256 is
`655f8881c1392749d366a0c0fdb90617ed7707f27f8f28733470a4819d778fa8`.
Discovery grows from 15,215 to 15,457 bytes. Removing only `diagnostic_capabilities`
from its parsed JSON exactly recovers the actual D01 descriptor; the prior D01
Python consumer also accepts the new host. This is JSON projection compatibility,
not equality of the expanded raw descriptor bytes.

Fresh CWD/HOME/TMP snapshots remain empty in the actual matrices. Source review
confirms the diagnostic returns before owner/database initialization; no syscall
trace is claimed. Formatter/scoped diff checks and independent hash-bound source
review passed. Actual host and selected archives remained unchanged during consumer
checks. The real worktree index stayed unchanged during the implementation checks.

Method totals, command matrices, helper cases and repeated/historical runs are
separate evidence layers; they are not added into a complete SDK pass count.

## Preserved corrections and failure evidence

Review tightened numeric types so float/bool lookalikes cannot satisfy integer
bounds, preserved existing resolved-host behavior for ordinary profile queries,
and added explicit structured rejection handling without changing the legacy
process runner's nonzero-exit policy. Inherited-pipe and archive-growth checks
exercise the actual bounded readers. Unknown-feature and oversized-module cases
exercise the original decoder, rather than a mocked validator.

The evidence-only fixture encoder initially linked a build-context Prost artifact
and failed compilation. Retrying with the matching existing target artifact passed;
no dependency, Core source or lock was changed. Failed logs and the separate retry
remain retained. Early smaller passing runs are not relabeled as final coverage.

Pre-publication log inspection caught `ResourceWarning` despite an earlier green
150-test summary. A descendant-held pipe could outlive the runner deadline; its
reader then reached late EOF without owning stream closure. Each reader now closes
its stream in `finally`, without making the caller block on that reader lock.
A synchronized inherited-writer regression confirms timeout return first, then
releases the writer and checks eventual stream closure. The old runner fails this
regression; corrected focused and actual-host runs retain separate evidence.
This fix does not claim termination of the entire descendant process tree.
An intermediate command accidentally listed a fixture-owning test module twice,
causing two existing-directory errors in 165 invocations. That log is retained
but excluded; the final unique-module 150-method run passed with warnings enabled
and no ResourceWarnings.

## Limits and remaining gates

- Static preparation checks a package under the selected compiled host. Allocation,
  invocation, live dependency resolution, platform ownership and route admission
  still require their real checks
- The original path reader follows a separate metadata precheck. Concurrent path or
  ancestor replacement can race it, including replacement with a blocking special
  file. Trusted local selections are required; no hostile-filesystem sandbox claim
- The consumer uses a five-second subprocess/pipe deadline plus bounded cleanup
  waits and independent output/archive bounds. Five seconds is not a bound for
  the entire runner call or multi-call operation.
  The runner does not promise termination of a selected executable's process tree.
  Direct native CLI use has no universal deadline against a stalled filesystem
- Before/after hashes do not sandbox a malicious native executable or eliminate
  a local actor's replace-and-restore race
- No guest code, owner/database, installation or grant is part of this diagnostic;
  no new authentication, key storage, system permission or network backend is enabled
- Windows/macOS, protected owners/tokens, GUI, public TLS/accounts and full SDK
  qualification remain open. Existing Windows42 and protected9 NOT_RUN keep their
  prior scope; P01 supplies no new Windows execution proof
