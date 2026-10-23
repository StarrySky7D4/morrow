# Original frozen transport guests on Windows

Six original C, C++ and Rust IO/service packages passed six actually executed
Windows methods on source `051fe4cd898c7a5b91bca6de3d3f96099789da714869c35542c05ef1b3f3550e`. All 17 frozen manifest entries, their
manifest and root digest pin remain unchanged. No guest was rebuilt, repackaged
or repinned. These are real local TCP requests, distinct from the earlier
package-only transport metadata check.

Two original service methods cover seven HTTP methods, binary requests and
headers, authentication/grant refusal, finite service renewal and budget
preservation, and observed/Unknown history across store reopen without resend.
Four original IO methods cover seven methods with exact binary responses and
durable receipts, synthetic credential injection and error response handling,
rejected endpoint/method/credential without dispatch, and a dropped response
remaining Unknown across reopen without retry. Each method runs the three
original language packages; counts are methods, not newly invented suites.

Evidence: `D:\build_cache\_audit\windows-restriction-lineage-20261022-019` / `sdk-frozen-transport-windows-001` and
`case032-sdk-transport-summary.json`. Each original test invocation, binary
digest, before/after source inventory and frozen-input inventory is retained.

The listeners bind loopback and credentials are synthetic. This qualification
does not prove Windows DPAPI, production TLS or public deployment, full plugin
distribution/lifecycle, product GUI, or the complete Windows SDK26/G04/FREEZE
requirements. Other platforms remain deferred by the user. Full SDK/source/
release acceptance remains false.
