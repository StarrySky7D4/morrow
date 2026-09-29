# morrow-codex: G0 qualification work

Independent local plugin repository for the Codex × Morrow v1.1 implementation plan.
This directory is separate from the Morrow application checkout. There are no A/B
product binaries, installable packages, or claimed completed SDK qualifications yet.

Current implementation is the P-00 build-entry slice and P-01 fixed-source audit.
`tools/build_plan.py` records each invocation in a **new** output directory, checks
its requested inputs, and fails when an input or implementation is missing.
Unimplemented stages never reuse `dist` or turn a previous receipt into success.

See `docs/implementation-status.md` for current evidence and continuation gates.
See `docs/upstream-audit.md` for the fixed-source dependency and seam review when
that audit is available. `sources.lock.json` pins the public sources, not `latest`.

The native and Wasm dependency graphs are separate in `tools/build-contract.json`.
No empty Rust product crates are used to claim a completed graph. Missing product
manifests and lockfiles remain explicit blockers until a reviewed real source seam
or portable slice is added. P-02 may consume only the host-owned, reviewed kit.

No account files, personal MCP configuration, credentials, real login, paid model
requests, global formatting, publishing, or SubagentBridge are part of this work.
The initial repository has no commits. Sources remain attributed to their upstream
owners; no upstream license is replaced by this repository.
