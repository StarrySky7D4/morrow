# Production supervisor 009: complete host library regression addendum

The [production integration report](m03-production-supervisor-009-2026-10-01.md) and its handoff001 remain unchanged. This supplementary check covers the original host writable/service/query paths affected by the new business gate; it does not expand the product/G0 acceptance claim.

Fresh `host/m03-product-supervisor-009-host-full-lib-001` is preserved as failed: exit101, 133 passed/60 failed. Independent read-only diagnosis attributes all 60 precondition failures to missing actual guest bindings: MORROW_WORKBENCH_WASM (51), MORROW_HTTP_FORWARD_WASM (5), MORROW_SERVICE_OUTBOUND_WASM (4). No failure was silently skipped or reclassified as passed.

Fresh `host/m03-product-supervisor-009-host-full-lib-002` freezes and binds the three existing actually compiled Rust Wasm guests, without modifying original binaries or rebuilding/qualifying their guest sources. Its 681 host/native/dependency source hashes match production qualification013 and remain unchanged throughout this run. The complete `workbench_host` Rust library test suite passes **193/193**, zero failures/ignored, exit0 in 382.146 seconds. Coverage includes original IO/mutation owners, actual service and outbound requests, stored drafts/preferences, query capture/archive/replay, TLS identities and ledger/gate negatives. Direct library tests do not replace the separately tested Flutter/supervisor/host process path or full product G0.

Log SHA256: `3ade9a6af3fce954dbdd3f81da6dfda171c72e8bc11b54fc7ec3d7563009eab6`. Evidence manifest SHA256: `6c7a02a16a851f50f74e14cd5763bb692ae2723a71b45e78e1fee632b8802976`. All existing failed/Unknown evidence remains retained. No CI, install, privilege change, push, publication, merge or deployment.
