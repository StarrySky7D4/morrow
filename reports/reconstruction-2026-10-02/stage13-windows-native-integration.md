# Stage 13 Windows native SDK integration

Date: 2026-10-02 UTC. Application remains `0.1.9-test.58+62`.

This stage combines the reviewed Cloud tooling stage12 with eight disjoint
Windows SDK verification files. No SDK/core/runtime/schema/ABI/versions or
Windows executor branches change. Stage001 unrun test preparation is excluded.

## Exact source identities

- Cloud parent: `d35cfc358167f606f3fc5df80216e41a8b671835`
- Tested SDK/application baseline: `03606019fc4f9e279ec21c7276b83e6ff71865b8`
- Windows local repair revision: `9a0126b42a1a0bd7850e2cca76ae6b44dc1194c2`
- Published Windows revision: `29ef49def9b05dcdd7591f0e9fbaf90b555e771e`
- Windows tree: `2ddce01a69e94052277f7d4659879339fa5ac1eb`
- SDK tree shared by both development branches: `2c761feb883f399361be16cc8378472e660f383f`

The Windows cumulative patch reproduces its complete tree from the exact
test58 baseline. All eight committed LF source hashes match the source handoff.
Independent review found no remaining P1/P2 issue after repair.

## New Windows qualification

The exact committed final runner was executed on Windows x64:

- Rust SDK tests: 92 passed, 0 failed
- Clang native programs: 7/7
- MSVC native programs: 7/7
- Independent copied-SDK Rust/C11/C++17 consumers: 3/3
- Four actual PAGE_NOACCESS guardpage executions, 27 rejection cases per
  native executable, with assertions enabled
- A separate helper execution performs fresh Rust92 and codec1; these
  repeated tests are not added to the full-run totals
- Eighteen provenance refusal controls exit2 before functional commands and
  promote zero current passes

Toolchain observations are Rust1.95.0, Clang22.1.0 and MSVC compiler19.44.35227
within toolset14.44.35207. This differs from the Cloud Rust1.96.0 toolchain and
is recorded as such. No Windows Wasmi import/fuel run is supplied.

## Provenance repair

The earlier helper copied historical metadata and replayed commands without
rechecking sources or reused artifacts. Initial observations remain historical;
the initial run used uncommitted pin/metadata edits, and later runner hardening
was not itself end-to-end qualified by those old logs.

The repaired producer records actual committed tool hashes, normalized source
and raw Git identities, producing logs, built DLL/import-library/wrapper objects
and before/after snapshots. The helper requires an externally supplied manifest
digest, the exact diagnosed failure pair, fixed historical command shapes and
unchanged original inputs. It uses fixed new recipes, checks sources/artifacts
around each command, and separates current results from history. Unbound legacy
directories and already-successful full runs are refused as correction inputs.

The final independent audit verified365 baseline source identities, all46 full
command logs, all18 refusal reports, reused artifacts and892/892 Windows
evidence-verifier checks. Native binaries are deliberately absent from the
source/evidence handoff; their on-device hashes are recorded evidence, not direct
Cloud binary inspection.

## Cloud integration checks

Only `tool/windows` files and this report enter the merge. The staged SDK tree
equals the reviewed/tested tree; existing shared sources are unchanged.
Fresh Cloud scoped Python regressions pass106/106, the four imported Python
tools compile syntactically, and whitespace checks pass. The native Windows
runner and its device-specific controls are not executed on Linux.

## Durable evidence and remaining boundaries

Source ZIP:54356 bytes, SHA256
`ca20b5a4531fd0697aae8691965c3de99c1ca39198f43c236663b3449d9e208e`.
Evidence ZIP:523343 bytes, SHA256
`3bedeb9e32a8864a865b609bdd3a93abe26a954db585877c50fe97c11125f280`.
Both Library and private Drive round trips match. The isolated Windows GitHub
ref/tree are verified separately; integration uses a new cumulative source
backup and external ref/tree verification.

This qualifies native Windows x64 SDK ABI/codecs/callbacks and bounded consumer
verification only. DPAPI remains blocked with4/12 synthetic DBs and0 published
keys; no new DPAPI/account/profile/ACL changes occurred. Protected host, UI,
native executor fix, ARM64/x86 and full Linux protected-owner/VFS/supervisor/GTK
work remain pending. The SDK is not frozen and public binding authority is not
enabled. Earlier test58 baseline failures and stronger Wasmi stress failures
remain as recorded; this tooling merge does not repair or promote them.
