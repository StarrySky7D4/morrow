# P02 store callsite qualification

This independent executable calls the pinned upstream `LiveThread` directly. It does not instantiate Codex Core, a model session, or an alternative agent loop. The unmodified host kit 003 is linked only with its explicit `qualification` feature.

Six cases compare complete method-call sequences and original error propagation. Empty fake history is the only permitted fixture setup. No production thread identity namespace, durable append, metadata projection, persist/flush barrier, or writer release is implemented. Every unsupported invoked method fails explicitly. The disconnected Event cases encode and validate a request then refuse locally; they do not establish IPC disconnection detection or OS isolation.

The runner is `receipts/p02-store-probe-002/run_probe.py`. Supply the new source receipt and its SHA-256. The separate `lock --prepare-lock --update-probe-lock` stage was used with a copied fixed upstream lock seed; `build` and `run` are locked and offline. Runs isolate Cargo, home and temporary directories and verify both previous frozen handoffs before/after. A run writes a fresh runtime JSON and exits nonzero if an assertion fails.

Builds and failed attempts are retained under `receipts/p02-store-probe-002/runs`. The schema and original upstream source are never edited by this executable. This is neither a product graph nor full P-02/G0 acceptance.
