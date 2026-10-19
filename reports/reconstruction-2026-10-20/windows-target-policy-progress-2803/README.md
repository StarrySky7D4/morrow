# Windows target-policy diagnostics and qualification progress

User requested GitHub synchronization followed by forced shutdown on 2026-10-20 (Asia/Shanghai). This is a development snapshot. No preview, tag or release is created.

Current canonical native source is ROOT2744 (3321 frozen members / 3288 source members). All changed production files are copied byte-for-byte from that reviewed freeze. Production manifests/locks, upstream, qualified UI/Dart, canonical anchors and all other HEAD paths remain unchanged. Qualification P/R/S anchor substitutions are not imported.

Scoped results: current compiler checks and four-role builds; 36 Rust pure tests and 85 controller pure tests in the original VM; one default Release Cargo invocation with five outputs; the 71-file application bundle replacing three default PE and reusing 68 original files; current normal close, four graphical self-checks and actual PNG review. Full outer public validation held 82 public inputs and returned 0.

TargetPolicyGuard exact failing predicate is still NOT_OBSERVED. Diagnostic-only changes do not establish a causal fix. Current case 74397ac11acd48e38834c2faf5844f37 has only gathered/bound/public evidence. The user's reply requested GitHub sync and shutdown, and did not approve account privileges or the new 47-step run. Add/Remove/47: NOT_RUN for this case. All historical Unknown results retain their original classification.

source_accepted=false
release_eligible=false
SDK26=OPEN
G04=OPEN
SDK_FREEZE=OPEN

Remaining: newly authorized actual native47, Nine, native IO/reader TLS/Job/lease and same-run cleanup, causal repair if required, full SDK gates and final preview qualification. Local binaries, caches, VM disk files, WSL data, private input and unrelated HarmonyOS files are excluded.

CONTINUE-2802.json contains fixed local evidence references. They describe the pre-sync state; this directory is the subsequent authorized GitHub snapshot and does not upgrade product acceptance.
