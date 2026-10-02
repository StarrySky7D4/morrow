# Stage15 Windows validation addendum

This updates the pending Windows verification recorded in the [stage15 report](stage15-linux-guarded-store-controller-channel.md). It changes documentation only and does not publish or qualify stage16 implementation.

## Exact source and materialization

The Windows executor restored the manually supplied cumulative candidate `8433f0853c4b2c179f50f9b69522ed3a5484acdd`, tree `64cdbb150b32103fd2bfda27debe89a459df97ed`, from exact baseline `925fb8ca6563dcb7de38db8fbdf6b005f9ad5420`. Input archive: 4,733,992 bytes, SHA256 `220d4e8a1616e6fd5ef0e2cab687bd1072134268742f74a9bea6be10e1230749`. Published stage15 `8e18a1b946976431cc93515fa3d409115661e799` adds only its report to those tested code bytes.

All 51,304 checkout entries, 18,355 distinct Git blobs and 764,302,194 checkout bytes were checked strictly before and after testing. The first check found 37 CRLF-converted vendored reference files; these were rematerialized from the verified blob bytes before tests. Subsequent checks accept no line-ending normalization. Git status still marked those 37 paths modified despite exact raw bytes and empty content diffs, so the candidate checkout is not described as status-clean. Six symlinks are exact link-target text files on Windows; native symlink and executable-permission semantics are not qualified.

The Linux coordinator independently verified all 103 delivered evidence-file hashes/lengths, compared every final checkout and object record against its original complete candidate manifest, and confirmed zero missing/mismatched entries. It also reconciled the actual test-summary lines and preservation records. Delivered evidence redacts local path prefixes and retains original versus delivered hashes separately.

## Newly executed Windows results

On Rust 1.95.0 x86_64-pc-windows-msvc, MSVC 14.44.35207, Windows SDK 10.0.26100.0 and Clang 22.1.0, with locked/offline commands:

- SDK Rust: 92 passed
- Runtime compatibility: 5 passed
- Host directory preflight: 4 passed; owner-binding guard: 1 passed
- Original Control: 15 passed; shared executor: 9 passed; channel faults: 21 passed
- Total Rust: 147 passed, zero failed; no historical or Linux executions included
- Clang/MSVC C/C++ checks: 14 executables passed
- Independent SDK-only Rust, C and C++ consumers: 3 passed
- Host library and binaries: compile check passed

The 45 Control/executor/fault cases were initially compile-only under an overly broad fixture-database restriction. After clarification of the authorized ordinary synthetic no-key SQLite scope, they actually ran and passed; the earlier compile-only records remain preserved. The compiler version probe's exit2 is not counted as a test.

Observed cleanup cases include original Control revocation, unrelated Control isolation, first-cause and original-owner retention, actual native thread-creation failures, producer panic/joins, private output-bound failure and injected maintenance Err/panic. Maintenance injection does not qualify protected Storage sealing.

## Preserved limits and evidence

Only new per-suite ordinary synthetic SQLite roots were used. No audit/DPAPI keys were created/read and no protected Session was initialized. Four historical database hashes and the original Windows checkout were unchanged. Test fixture remnants remain local; database/key bytes and compiled binaries are not in the evidence archive. Process-local random channel values are ordinary runtime data, not a claim that no randomness was generated.

No DPAPI/protected product, GUI, Windows ARM64/x86, immutable SDK Wasmi stress/fuel qualification or SDK freeze is established here. Stage16's new Linux fixture/runtime/native changes are outside this source identity and require their own review and tests. Earlier materialization failures remain historical evidence; successful manual materialization does not imply the failed helper route was repaired.

Sealed Windows evidence: 6,403,494 bytes, SHA256 `a622b4ed530cb22c918ce7328356f895d8accbdea8f18bf23216622ffd9b845f`. Its private Library and Drive copies were verified, including a raw Drive redownload with that exact hash. Source manifests, redacted logs, compiler/binary identities and independent review receipts accompany the private evidence. This addendum publishes no private database, key, local-user path or stage16 source.
