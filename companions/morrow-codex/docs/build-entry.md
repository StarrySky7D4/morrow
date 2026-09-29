# Build entry: implemented first slice

Run from `C:\Users\Administrator\Desktop\CodeXProjext\morrow-codex` with Python
3.10 or newer (verified here with 3.14.5). The entry uses only the standard library.

```powershell
python tools/build_plan.py preflight --scope baseline --locked --offline
python tools/build_plan.py preflight --scope sources --locked --offline
python tools/build_plan.py preflight --scope full --locked --offline
python tests/test_build_plan.py
```

Every invocation that can reserve an output directory writes a `status.json`
containing a fresh run ID, entry-script SHA-256, input hashes, checks, exit code, and explicit gate
limits. The default output is `out/runs/<timestamp-and-random-id>/`. An explicit
`--output-dir` must be a new directory inside this repository. Rejected existing
or outside directories are not touched; their failure receipt goes to stdout.
Argument syntax failures are argparse errors (exit 2), before reserving a run.

| Command/scope | Meaning of exit 0 | Current limitation |
|---|---|---|
| `preflight --scope baseline` | Candidate Morrow HEAD and branch match the lock | Does not validate M-00 completion or unchanged working files |
| `preflight --scope sources` | Complete fixed sources and recorded bytes pass scoped checks | `partial_snapshot` is verified then **fails** with `source_incomplete` |
| `preflight --scope full` | Not reachable in this slice | Kit Schema/lock resolution/dependency cache validation not implemented; always nonzero |
| `generate`, `deps`, `test`, `build`, `inspect`, `pack`, `bundle`, `qualify`, `release-plan` | Not reachable in this slice | `not_implemented`, exit 2, empty artifact index |
| `python tests/test_build_plan.py` | Entry failure paths behave as specified | Synthetic fixtures are not actual upstream seam probes |

The `--locked` and `--offline` switches document intent. This slice never updates
locks or fetches, even if the switches are absent. It never invokes cargo builds,
arbitrary commands embedded in input files, account tooling, or a model endpoint.
Toolchain checks first require an exact installed toolchain name, then check its
versions/targets. Rustup automatic installation and Git lazy fetching are disabled.

## Provenance

`sources.lock.json` is a plugin-owned source inventory, not a host wire Schema.
Both upstream IDs, repositories and commits are pinned in the entry. Git sources
must have matching root/HEAD/origin and exact recorded tracked diffs plus every
untracked/ignored file. Complete archives must match their digest, safe member
names and file bytes; the extracted tree must match that same manifest.
Partial source snapshots verify their file list, SHA-256 and recorded Git blob
SHA-1 identities, but remain insufficient for full source/build qualification.

`toolchain.lock.json` pins the observed Windows Rust/Cargo 1.95.0 environment,
matching the fixed Codex toolchain version. It records installed components; it
does not claim all upstream tooling is prepared (`rust-src` was not installed in
this toolchain). Native and Wasm have separate target paths. Their product Cargo
manifests and locks remain absent until real implementation is introduced.

## Host-kit handoff input

When a concrete host-owned kit is delivered, provide:

```powershell
python tools/build_plan.py preflight --scope full --locked --offline `
  --host-kit <read-only-kit-directory> --host-kit-manifest manifest.json `
  --host-kit-sha256 <actual-manifest-sha256> `
  --host-kit-review receipts/<digest-bound-review>.json
```

The local review record must contain `manifest_sha256` and `status` of
`qualification_only` or `ready`. This check binds the reviewed manifest bytes;
it does not validate host Schema generation or imply SDK freeze. Full preflight
still fails until its remaining checks and real graphs are implemented. Detailed
review requirements are in `docs/host-kit-consumption.md`.

## Verification evidence

The frozen final round is indexed by `receipts/handoff.json`. Its verification
directory is `receipts/final-verification-20260928-001/`: 43 entry tests plus
12 real CLI invocations, with before/after input hashes and exact expected
nonzero statuses. `verification.json` must exist and report stable inputs before
this round is considered delivered. Earlier runs are preserved, not overwritten.
Independent review: `out/reviewer-20260928-final-review/REVIEW.md` and
`independent-suite-summary.json`. The independent suite predates only the root's
toolchain-lock reference/fixture isolation and entry-receipt digest changes; the final root run includes them.

All fixtures and generated test receipts stay under this plugin repository.
No test result here proves model service, platform/device, packaging, A/B isolation,
or full upstream exec/net/store replacement.
