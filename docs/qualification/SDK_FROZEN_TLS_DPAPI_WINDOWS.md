# Original frozen Windows SDK packages over TLS and DPAPI

Source `67ff5ebeefd97e5e74e64a7c5e9850c5afa4acf4fe24cf4eeeb0e67169df20c3` passed three actual Windows test methods,
each running the original Rust, C and C++ IO package: nine language/scenario
combinations. Frozen packages, modules, all17 transport manifest entries, the
manifest and its root pin remained unchanged. No guest was rebuilt, repackaged
or repinned. The actual test binary and every original run log are hash-bound.

The positive scenario uses an actual TLS loopback listener, an explicit trusted
root, hostname validation and authentication. The package is installed unchanged
in a synthetic catalog, approved on its original managed owner and executed as
Wasm. A credential is sealed with actual Windows DPAPI, stored in the original
database, and loaded after that database closes and reopens. The real provider
injects it into the request; the synthetic provider receives zero calls. Exact
binary request/response bytes, status201, one handler call and original Worker
join are verified. Ciphertext, guest request and durable request/response
materials contain no plaintext synthetic credential.

The other two methods reject an unrelated TLS root and a hostname mismatch.
No request reaches the application handler. The original request persists and
the operation remains Unknown; response material explicitly returns
EvidenceUnavailable, including after reopening the original database. Missing
response evidence is never converted into an empty successful response.

Evidence: `D:\build_cache\_audit\windows-restriction-lineage-20261022-019\sdk-frozen-tls-dpapi-windows-003` and `D:\build_cache\_audit\windows-restriction-lineage-20261022-019\sdk-frozen-tls-dpapi-summary.json`.
Run001 retained a collector failure caused by interleaved test output after the
positive case actually passed. Run002 retained an incorrect test expectation
that TLS rejection would have response material. The corrected fresh run003
passed all three methods. These overlapping attempts are not added to the count.

This is real Windows/SDK execution with synthetic credentials and loopback TLS.
It does not prove public deployment, production GUI or third-party distribution,
the remaining Windows SDK26/G04/FREEZE contract, full P2/P3, or release eligibility.
Other platforms remain deferred by the user's instruction. Case033 and its
original VM/owners remain retained and are not involved in these tests.
