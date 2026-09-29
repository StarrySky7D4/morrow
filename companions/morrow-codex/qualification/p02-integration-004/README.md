# Same-source P02 qualification 004

One executable merges the reviewed exec and Core network source changes with the real LiveThread adapter, all using one new fixed-source copy and one lock. The first case holds all three capabilities concurrently; guarded calls also run from new Tokio tasks. Previous bounded cases are rebuilt into this executable, not combined from old executable receipts.

The explicit `morrow-p02-restricted-qualification` feature rejects selected default backends. It is a qualification-only build rule, not a host authorization mechanism. Read `receipts/p02-integration-004/bypass-audit.md` for exact coverage and remaining bypasses. Successful HTTP/WS, process execution, production persistence and writer release are not implemented or claimed. The default product build remains unverified.

Use the isolated runner in `receipts/p02-integration-004/run_probe.py`; lock preparation is a separate explicitly selected offline stage. Build/run use `--locked --offline`, a fresh target/profile/Cargo home and pinned read-only dependencies. Runtime takes a fresh receipt filename. Do not run through personal Cargo/auth configuration or invoke real providers to test these guards.
