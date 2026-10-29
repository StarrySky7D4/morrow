# Local continuation, 2026-10-21

Status: **paused by direct user instruction on 2026-10-29; VM047 Saved; four VMs registered; source and release are not accepted**.
See [the current pause and publication handoff](PAUSED_WINDOWS_20261029.md). Historical results below retain their original source and VM-state boundaries.
Working checkout: `C:\ms1020`, branch `codex/local-continue-20261020`, base
`116d1eb9beffa1675a48eb5ab06d1abc79137ae2`. This document records the development continuation and its historical snapshots. Current acceptance scope is Windows only by direct user instruction; other platforms are deferred and not accepted. The original
`morrow` checkout and its unrelated work were not used as the implementation tree.

## Verified work

- Verified the supplied handoff and its 17 source/package entries. Recovered all
  eight original driver/approval-description inputs; no old case was replayed.
- Acceptance discovery now requires the fully qualified L0 name, exactly one
  discovered test and exactly one successful non-ignored execution. Portable
  acceptance builds and hashes the three real WASM fixtures before host tests.
  Python fixtures use their own Cargo configuration. The service fixture lock
  includes the previously missing local Windows directory dependency.
- The `portable-006` snapshot passed: 346 host tests and 135 native filtered
  tests. The Python fixture suite scheduled 82 tests (one platform skip).
  Its source digest is `e11671707cc5ec7464c38164d2bfe8b92561f37f18ba3ed227b022f4db7592e5`.
  It includes the editor owner, import recovery, read-only review, localization
  and default routes below. The later process/thread MIC correction is separately
  recorded below; old snapshots and source packages are not rewritten.
- Actual L0 ran once in the authorized Windows development VM with exact test
  selection. It created and deleted its own fresh standard test account, derived
  and checked the minimal launch token, and closed the original transport session.
  The test passed (1 passed, 0 failed, 0 ignored). It did not create a runner or
  mutate LSA rights. VM configuration was unchanged. This is scoped L0 evidence,
  not owner retirement, L1, full47, SDK or release evidence.
- Built fresh P/C/R/S roles from separately verified source snapshots. C/R bind
  the actual P hash; S contains the actual C/R allow-table bytes in its read-only
  section. The controller was mechanically rebound to these four artifacts and
  its 73 + 12 pure selector tests passed. A fresh original VM gather and binding
  passed for case `da9d561f9650435686884c33fb037e64`: 17 readiness checks, seven
  staged image hashes, actual gather controller exit/reader joins and original
  binding digest `d317bdcfeabe525178a8706bdb1c3b5fb1b56402d75639de69a62b52784fa80e`.
  Both owned stage/bind transport sessions closed. Those receipts establish only
  their original preparation scope; the subsequently authorized run is below.
  The separate older rejected stage `f3c10aa8de444403bcced1892ac480d0` stays sealed
  and was not replayed, queried, adopted or terminated.

## Authorized case and process descriptor correction

The user explicitly approved case `da9d561f9650435686884c33fb037e64`, including
one conditional temporary `SeAssignPrimaryTokenPrivilege` assignment, the original
47-step run and same-owner conditional restoration. It was dispatched once.
The original journal and controller events show returns for steps 1-39; step 40,
`sealed-native-start`, ended in `SEALED_UNKNOWN_OR_REJECTED_NO_REPLAY`.
The original native projection is `ProcessReadback / NativeReadback`,
`IoAccessDenied`, with no Win32/NTSTATUS value. No runner-ready or full47/Nine
acceptance proof exists. This is an internal-policy-class rejection, not evidence
of an OS `ERROR_ACCESS_DENIED` result or a particular MIC predicate.

The same policy owner recorded Add status 0, removed only the newly added user
assignment with Remove status 0, and verified restoration to the original
`S-1-5-19`, `S-1-5-20` holder set. Its own resources and transport closed.
The original case/controller/creator session remain retained Unknown; issued
tokens are not claimed revoked. No retry, owner query, adoption or forced
termination was performed. A separate read-only transport copied four exact
already-written public files and closed itself. The original case was not called.
Evidence: `vm-case-diagnostic-001/manifest.json` and the original case receipts
under the `Morron/c28-root-vm-transaction2769-...-case` directory.

A separate local `CreateProcessW` probe then reproduced a concrete descriptor
compatibility defect: both original process/thread reads returned Medium MIC
mask 3 (`NO_WRITE_UP | NO_READ_UP`) after creation input requested mask 1 (`NW`).
The production reader previously required exactly mask 1. Creation now explicitly
requests `NWNR` and process/thread readback requires exactly mask 3. DACL contents,
OWNER_RIGHTS, expected integrity, labels/ACE structure and GUI's NW-only lowering
contract remain independently checked. Microsoft documents the [mandatory ACE
mask meanings](https://learn.microsoft.com/en-us/windows/win32/api/securitybaseapi/nf-securitybaseapi-addmandatoryace).
The local observation proves this compatibility defect; the old VM case did not
save its raw label and is not retroactively assigned that sole cause.

`native-mic-regressions002.log`: 137 passed, one ignored before the new OS fixture
was added. `native-mic-actual001.log`: exactly one explicitly selected real Windows
fixture passed using the production SDDL/decoder, original process/thread handles,
both normal waits, exit codes 0 and checked closes. It creates one current-user
`cmd /d /q /c exit 0`, no account/LSA changes and no business runner. This does not
prove `CreateProcessAsUser`, the original materializer/handshake, full retirement
or VM acceptance. The old P/C/R/S binaries precede this correction and must not
be presented as corrected artifacts. `qualification-evidence-003.json` indexes
the subsequent candidate and its distinct evidence scopes.

## Approved isolated VM and private window-station contract

The direct user reply approved creating the reviewed isolated acceptance copy.
Hyper-V exported only checkpoint `62da4512-6182-40a2-818d-4b232eb3b446`
(`Morrow-C28-before-basic-20261007-01`) and imported an independent disk copy
with a new VM ID. The clone is `Morrow C28 修正版验收 20261021`, ID
`d9771bc8-4045-4e14-af84-2a80264e9b30`, 4 GiB, four processors, disconnected
network, automatic checkpoints disabled. Only the clone's imported saved memory
was discarded before its cold boot. The original VM stayed Running with all
three original checkpoints and its exact current disk reference unchanged.
The clone preserves the checkpoint firmware UUID; it is distinguished by its
actual Hyper-V VM ID, independently observed guest identity, exact disconnected
configuration and separate disk. Its read-only probe passed all 17 original
readiness checks plus the COM and current-parent checks, and closed its session.

The current MIC-corrected P/C/R/S from `roles-002` were rebound to this clone.
The derived controller passed 85 pure tests; the policy library changed only its
fixed VM selector and passed 18 pure tests. The original strict readiness and
47-step/nine-gate protocols were preserved. Host guards additionally preserve
the old Unknown VM and validate the third, isolated VM and its disconnected
network. All protected credential-adapter source blocks remained byte-identical;
only the protected child consumed private input.

New case `03d8d451acee4092a988bae867a3dbf0` passed stage/gather/bind with binding
SHA256 `f8910698949f562649897f90fb18cdb61e9f318a75dea577c852e15ca929933e`.
Both preparation sessions closed. Its one authorized execution returned steps
1-39; step 40 remains `SEALED_UNKNOWN_OR_REJECTED_NO_REPLAY`. The original
runner process now reached `AsUserConnectInput`, with
`OriginalRunnerExitedBeforeConnect`, raw exit `3221225794` (`0xC0000142`).
No runner-ready, authenticated handshake or business/retirement completion was
proved. This is a different failure from the prior process descriptor readback.
The outer timeout projection does not establish its cause: the preserved original
process observation records the initializer exit before connection.

The same policy owner recorded one Add status 0 and one Remove status 0, removed
only the newly added user SID, and verified restoration to `S-1-5-19`, `S-1-5-20`.
Its resources and policy transport closed. The original case/controller/session
remain retained Unknown in the clone; issued tokens are not claimed revoked.
The old VM and all historical Unknown owners were untouched. A separate
read-only connection copied four exact public diagnostic files and closed itself.
Evidence: `vm-case-outcome-004.json`, `vm-case-diagnostic-004/manifest.json` and
the original new-case receipts. This case must not be replayed or used as a
pristine environment for another original47 run.

A separate local factorial probe created four independent private window
station/desktop pairs and a minimal child with a static USER32 import. To isolate
the logon ACE under the current-user fixture, OWNER_RIGHTS zero suppressed that
fixture owner's implicit READ_CONTROL. It changed neither an existing desktop
nor an account or LSA right, restored its own process's original station before
each launch, and checked every original process/thread exit and handle close.
Both cases without station READ_CONTROL exited `0xC0000142`; both with it exited
0. Changing desktop READ_CONTROL did not change the result. These are independent
fixture observations, not a retroactive cleanup or full diagnosis of the retained
managed-user case. Microsoft documents [USER32 initialization failures associated
with window station/desktop access](https://learn.microsoft.com/en-us/troubleshoot/windows/win32/user32-kernel32-not-initialize)
and [station generic rights including READ_CONTROL](https://learn.microsoft.com/en-us/windows/win32/winstation/window-station-security-and-access-rights).

The production station logon ACE now includes only the additional READ_CONTROL
bit (`0x2037f`); desktop rights stay `0x1ff`. Exact decoder checks, caller/owner
identity, Medium NW label and denial of WRITE_DAC/WRITE_OWNER/full-SACL/generic
access remain enforced. New readback regression tests reject the old station
mask, extra desktop RC and security-mutation rights. `native-gui-regressions001.log`
has 139 passed and two ignored tests. `private-gui-rights-probe-001/result.json`
contains the four actual local cases. This new source change is later than
`roles-002` and requires fresh role artifacts and independent runtime acceptance.

## Editor implementation and evidence

The workbench retains owners, view leases, raw snapshots, original business
operation identities, attachment catalogs and original import operations.
`NewIdeaDialog` accepts an explicit owner/runtime; its view teardown releases the
lease without destroying its root owner. Application close captures and flushes
views, validates the preparation, detaches them, finishes owners, then permits
host EOF. A read-only exact original draft inspection can resolve a lost reply;
absence, a later generation or a retired record cannot.

Import admission is reserved before the first await. Ready selections enter root
state before the view receives them. Failed exports/validation remain unresolved;
ordinary raw capture cannot silently abandon the selection. An explicit original
Ready inspection can resolve a lost Complete reply or failed preview export by
exporting the actual host pin. It does not repeat Begin/Complete, reopen a deleted
source path or clear an absent/Pending result. A metadata failure retains newly
typed raw values in the owner but blocks persistence of an incomplete snapshot.

For a live original capture scope, the host now archives actual adopted
observations, capture parents, paste applications, exact package archive and
source identity. A draft binds the original scope and exact paste count. The
archive is hashed, pinned in the same journal mutation and charged against the
existing 64 MiB active budget. Read-only export checks the current generation and
exact bytes. Archived strings never recreate a live scope or replay authority.
Dropping the capture scope or reducing its event count within a journal is
rejected. A successor carries the complete immutable parent archive lineage;
read-only export validates every archived parent without recreating its scope.

The explicit owner path now freezes an exact parent and original business
operation before S1. It checks actual S1 evidence, opens a fresh successor,
creates the exact first child, saves the latest full S2, transfers the view lease,
then confirms S2 again before conditional parent retirement and S1 acknowledgement.
Business, prepare, complete, child update and parent retirement reply losses can
be inspected explicitly under their original identity. Inspection does not
resubmit the old operation. New input during inspection is retained and, after
the original write is proved, uses a distinct later draft operation.

Bounded results after these changes:

- `draft-capture-unit003.log`: 64 Dart binding/session/workspace/owner/codec tests.
- `draft-capture-host003.log`: 16 draft and 7 import-protocol tests using the real
  WASM guest. The archive survives reopening without a loaded plugin; a dead scope
  cannot submit another generation, while exact old delivery remains historical.
- `draft-owner-native-005.log`: 2 actual host-process tests, including the formal
  dialog's five raw fields/UTF-16 selection/IME state and a retained owner with
  real rich capture plus a durable imported attachment. After host-process restart,
  exported capture bytes match exactly and deleted source files are not reopened.
- `draft-commit-formal001.log`: actual formal dialog Save retains in-flight S2,
  transfers the view lease, preserves full UTF-16/IME state and survives a host
  restart with exactly one business revision.
- `draft-commit-loss001.log` (4 cases) and `draft-commit-loss-child001.log`
  (1 case): actual host mutations with withheld success replies; original
  business/prepare/complete/retire operations run once, S2 stays recoverable and
  acknowledgement is gated. A later child save uses a new operation.
- `editor-rust-regressions-002.log`: all 12 editor draft integration targets,
  79 passed with actual WASM fixtures; no failed or ignored tests.
- `editor-analyze-002.log` and `editor-codegen-check004.log`: focused Dart
  analysis has no issues and generated bindings match their schema inputs.

The ordinary native New and Edit routes now retain the same owners. V2 enables
deferred acknowledgement only on that owner path. Closing and reopening a card
reuses its retained successor and complete raw state. The saved-draft menu can
explicitly reattach a detached original live owner, including blank-title cards.
It never constructs an owner from persisted capture strings.

After a client restart, the saved-draft menu discovers active journals and opens
a read-only formal dialog. It checks the exact generation/request and exports
actual attachment pins and capture archives. Complete export includes all five
raw editing values, ordered attachment provenance, exact request bytes, original
business-operation identity and file hashes. The export is marked complete only
after all writes and a final original-generation check. Missing commit evidence
never proves rollback or permits another Create. This implements read-only
recovery/export, not resumed editing under a reconstructed capture scope.

Additional bounded results:

- `draft-import-owner-native002.log`: two actual host cases resolve withheld
  Complete replies and failed preview exports after source-file deletion; S2
  remains intact and exported pin bytes survive a host restart.
- `draft-owner-regressions007.log`: 27 owner/import/session tests passed.
- `draft-review-seed005.log` and `draft-review-recover005.log`: separate Flutter
  client PIDs 14788 and 5528. The formal read-only dialog restored all five raw
  values, selection/IME, attachment and capture bytes; complete export was checked,
  the journal remained byte-identical, and no business operation was submitted.
- `draft-default-native004.log`: three actual MorrowApp entry-point tests passed:
  blank-title live-owner resumption, new-card S1/S2, and V2 S1/S2 with the ordinary
  card Edit route reusing the original retained successor.
- `editor-regressions-003.log`: 253 passed, 22 skipped before default route wiring.
  `draft-default-analyze003.log`: focused analysis of ten current files has no issues.

These results do **not** establish complete editor/product qualification. Tests
that use widget-test rendering are not a real interactive desktop acceptance run.

## Windows continuation after disk cleanup

The completed cache-only runs007/008/009 deleted 80,686 old compiler-cache leaves.
Their observed free-space increases total 95,583,363,072 bytes (about89.0GiB);
logical deleted sizes are recorded separately. Source, VM files, retained evidence,
executables, DLLs, PDBs and WASM artifacts were preserved. The earlier disk-full
roles004 failure and cleanup006 metadata-guard stop remain recorded. Roles005
subsequently compiled successfully from verified snapshots with incremental
compilation disabled. Its compiler result is not runtime acceptance.

The role verifier now checks actual P/C/R/S bytes, the S allow-table, compiled
controller inputs and the compiled P firmware selector. It rejects the historical
stale-controller and mismatched-firmware plans before any new VM dispatch.
Case007 retained Unknown at the identity step. A separate fresh VM preserved the
baseline firmware before its first boot; case008 passed original readiness and
steps1-39 but retained Unknown at step40. Its original observation was
`AsUserSpawnReadyRead / RunningAtObservation / START_WAIT_TIMEOUT`; neither
SpawnReady nor a business-child result was observed. This is progress past the
previous initialization exit, not L1 acceptance or proof of a sole timeout cause.
Both new case policy owners added and removed only their own temporary assignment
once, with status0 and the original holder set restored. All Unknown owners and
their VMs remain retained without replay, adoption, owner polling or termination.

The same staged runtime can now finish its protected owner before actual scheduler
shutdown. Two focused ordering tests passed. This fixes the finish precondition;
it does not implement or qualify the full P2 proven-clean refusal path.

Saved-draft retirement now preserves a distinct discard operation and the exact
original raw request/business identity. Its root-owned attempt submits discard
and import reconciliation once; lost replies are resolved by reading the original
result. Ten unit cases, actual host retirement tests, a33-cycle capacity case and
separate Flutter client processes are recorded. The cross-client retirement
preserved complete export, observed generation3 and no remaining imports, and
submitted zero business operations. The256 historical-identity limit is unchanged.

Three independent actual Windows application clients passed seed, read-only recovery/export and explicit retirement with visible windows, the same release host/package, and framework-injected input. Physical mouse/keyboard/clipboard/IME acceptance is not claimed.

Fresh Windows SDK verification passed92 Rust tests with no failures, ignored or
filtered tests,14 Clang/MSVC C/C++ executable runs, and3 independent SDK-only
consumer runs. A separate verifier rechecked source,51 original log/record hashes
and5 artifact bindings. This is Windows native codec/callback and standalone
consumer evidence, not full SDK26/G04/FREEZE or product-runner acceptance.

## Live-owner retirement and subsequent native diagnosis

The draft owner reserves retirement synchronously before any awaited work. The
reservation binds the reviewed generation and blocks new writes, imports, view
attachments and commits. After the original journal and imports prove retirement,
the same capture-close future is awaited; a missing close reply never sends a
second close. Focused unit tests passed 82 cases and analysis found no issues.

`draft-windows-004` passed four independent actual Windows application processes:
seed, read-only recovery/export, saved retirement and live-owner retirement.
The last process retired 18 live owners, closed their capture and import scopes,
released each workspace slot and submitted zero business operations. One retirement
used the formal confirmation dialog. The previous002/003 failures are retained.
The successful run has identical full source membership and byte manifests before
and after execution, and unchanged release host/package hashes. Its exact source
predates the later native stdio correction. Framework input does not prove physical
mouse, keyboard, clipboard, IME or rendered-pixel acceptance.

A fresh isolated API probe observed CreatePipe fail with Win32 error5 when an
impersonation token had an RC-only default DACL. An explicit descriptor on the new
pipe allowed a round trip while preserving that token default DACL. The new A1
non-TTY stdio path uses a validated logon SID and OWNER_RIGHTS in an explicit new
pipe descriptor. It does not modify the token default DACL or existing objects.
The two new native tests passed. Subsequent roles006 included this correction,
but a new disconnected clone's case009 still timed out at step40 SpawnReady.
The stdio fix alone does not solve the original startup failure.

## Host test provisioning incident

Expanding the native library test run triggered upstream legacy sandbox setup on
the host through `elevated_non_tty_cmd_forwards_env_output_and_exit`, outside the
intended isolated VM acceptance scope. The original run ended with 617 passed,
13 failed and8 ignored. It recorded sandbox account provisioning, four firewall
rule names,12 WFP filters and directory ACL grants before failing to find its
command-runner helper. The log and all13 failures remain intact. This full run is
FAILED and must not be reported as a passing native regression suite.

The path is stopped. Read-only inventories observed two sandbox accounts, four
matching firewall rules and four directory ACLs. No pre-execution baseline exists
in this run, so neither all effects nor the exact newly added/modified delta is
known. Native WFP filters were not independently enumerated. No rollback, account
removal, firewall deletion or ACL reset has been attempted. The user clarified that `D:\build_cache\VMBack` is a new backup destination,
not an earlier baseline. Current host configuration was exported and hashed under
`D:\build_cache\VMBack\host-config-after-incident-20261021`. This after-incident
backup cannot establish the previous state. No guessed rollback was performed.
Existing Windows VM authorization remains in force. Nine formerly automatic
provisioning tests are now ignored, and all ten legacy process tests require an
explicit disposable-VM environment opt-in. The current compiled library runner
default-skipped all ten exact test names, executing no provisioning bodies.

The incident record is `build/local-continuation/host-legacy-incident-001.json`.

## Remaining work

Preserve the after-incident host backup and all historical Unknown owners.
Further acceptance uses new isolated Windows VM cases under existing authorization.

1. L1: determine the original SpawnReady timeout cause using source review and
   independent fresh fixtures; never retry or inspect a retained Unknown owner.
2. P2: implement and verify a separate single-consumption proven-clean refusal
   capability across actual native/registry/SDK/factory/account/setup owners;
   demonstrate more than16 clean refusals and complete normal join/ACK/retirement.
3. P3: editable restart under a valid new capture authority, long-term maintenance
   of the256 historical identities, and the full fault matrix
   including reply loss, concurrent successors, double-active restart, disk/capacity,
   workspace/locale/font changes and attachment reselection. Physical input remains
   separate from framework-injected Windows application tests.
4. Full47/Nine and Windows-relevant SDK26/G04/FREEZE requirements remain open.
   Other platforms are deferred by the current direct instruction, not marked PASS.
   Before the pause/publication instruction below, no CI run, commit, push, deployment, tag or release was performed.

## Local evidence

Public evidence is retained under `C:\ms1020\build\local-continuation`:
`portable-006`, `portable-007`, `l0-vm-001`, `roles-001`, `controller-001`, the named logs above,
and the versioned `qualification-evidence*.json` indexes. Failed earlier attempts remain separate and
are not overwritten. The roles/L0/compiler snapshots precede subsequent editor
changes and are not presented as acceptance of the final working tree.

## Cache relocation and startup observation continuation

All build-cache defaults now target `D:\build_cache`, and former C cache paths
are verified links to D. The sealed migration moved 490,283,365,687 logical bytes
with per-file hashes and reclaimed 110,581,020,492 logical bytes of obsolete builds.
Source, delivery evidence, denied observer evidence, VM disks and backups were
preserved. The migration seal observed about534 GiB free on C. Its evidence is
`D:\build_cache\_audit\morrow-20261021\result.json`.

Roles006 and case009 are frozen evidence for source
`481bcc976827f12edfaf330b115010be2b00bc860e52485e90f02059c6c7db5f`.
Case009 used new VM `0f803abf-36f1-4abc-9504-38035bae32f9` and tag
`45e623a2bb9c4ed9988d89ee070179f4`. The original47 journal returned steps1-39,
reserved step40, then sealed Unknown at AsUserSpawnReadyRead. A single independent
read-only snapshot closed its own transport. The original case execution and
runner remain retained; session14496 must not be queried or terminated.
The same policy owner verified one Add and one Remove, restored the original
holder set and closed all ten policy resources. This is not case cleanup.

Source review found that the original transport was retained before child launch,
while the startup-error sender rejected every retained/unresolved slot. After a
spawn error, the parent could therefore receive no error frame. The corrected
path reserves one report using the exact original runtime allocation, exact pipe
allocation, attempt generation, failed original state and existing finite deadline.
It neither clears Unknown nor releases any capacity. Even a foreign runtime with
the same numeric attempt is rejected. Native token and pipe errors now retain
their original `io::Error` through context and capture it before cleanup.

The current runner and library test binaries compiled. Six exact regression tests
passed, using actual anonymous pipes and memory writers, including authenticated
redacted delivery, wrong runtime/endpoint/deadline rejection, repeat prevention,
and a broken original pipe with first-error retention. The evidence directory is
`D:\build_cache\_audit\windows-startup-observation-20261021-010`.
These tests created no accounts, firewall rules, existing ACL changes, child
product processes or VM sessions. P1 runtime, P2, P3 and full Windows SDK gates
remain open. Roles007/case010 subsequently verified the reporting path as described below.


## Authenticated failure observation and token derivation

Source9616f990a5d6ef30cdc1af67704a90f4d450a3943a4d6f00d2da5e3d30806c6b
was compiled into fresh byte-bound roles007. New disconnected case010 used VM
14acf5d4-1b11-4c44-9b8a-11fb55bb6a17 and tag8e55fa4e74e24f9cb7ca9d9deb6e17b0.
It completed original steps1-39 and sealed Unknown at step40. The authenticated
runner error now reports SpawnChild/AccessDenied; the specific failing API is
still NOT_YET_OBSERVED. Exec1833 and its original owners remain retained with no
replay or polling. The same policy owner verified one Add and one Remove,
restored the initial holder set, and closed its ten policy resources.

Independent fixtures created only owned tokens and empty unnamed Jobs. A medium
token with a high-labeled kernel object denied the old0x1ab open, while QUERY and
DUPLICATE opens succeeded. DuplicateTokenEx from the read/duplicate handle
successfully created an independent medium primary token with initialization
rights. This fixture finding is a plausible mechanism, not proof of case010's
actual failing API. The corrected runner now uses that derivation without
changing the original token or widening its object/default DACL. It requests no
TOKEN_ADJUST_SESSIONID, WRITE_DAC, WRITE_OWNER or ACCESS_SYSTEM_SECURITY.

All three preparation handles are charged to fixed original attempt slots before
their API calls. Successful token preparation checks closure of the query and
intermediate originals; successful child admission checks the restricted-token
original. Failure, uncertain close or unwind keeps the original charge, prevents
retry, and forbids physical-reaping claims while token debt remains.

Thirteen exact native regressions passed. The actual medium fixture observed old
open error5 and successful derived IL8192, unchanged original default DACL,
statistics, privileges and groups, a non-inheritable handle, capability restriction
and five checked closes. Twenty actual local preparations reused their local slot
only after all three checked closes. Separate synthetic close/unwind tests
retained debt. These are local token/pipe/AccessCheck fixtures: no product child,
account, firewall, existing ACL or VM effect was performed. They do not discharge
full native/registry/SDK clean-refusal P2 or full47/Nine/P3/Windows SDK acceptance.

Evidence: D:\build_cache\_audit\windows-token-derivation-20261021-012.
This derivation was subsequently exercised by roles008/case011; its retained failure is described below.


## Case011 and finite authenticated runner startup sites

Source b63ecf930e55ada6fba24e6bdb370236c62840b532f29597c5502e25edb5acc4
was compiled into independently pinned roles008. New disconnected case011 used VM
40553dbf-f7c5-47bb-931f-97d66c22d098 and tag 85e6d573f14e40329bf683c917eefe22.
It returned original steps 1-39 and retained Unknown at step40. The authenticated
same-attempt report was SpawnChild/AccessDenied. The token derivation fixture did
not resolve or identify the product's actual failing API. Original exec70767 is
retained without replay, adoption, polling or termination. One Add and one Remove
by the same policy owner restored the initial privilege holder set and all ten
policy resources were closed. The business case is not clean or accepted.

The current source carries only exact finite RunnerStartupSite atoms in the
existing ErrorPayload message. Typed contexts preserve the originating io error;
an outer context cannot overwrite an inner site. Unknown strings, near matches,
free text and paths remain redacted. Only a packet authenticated by the original
native runtime can populate the parent's site observation, and the first packet's
stage, OS class and site remain sticky together. Legacy packets cannot claim it.
No protocol fields/version, ownership, cleanup authority or retry rules changed.
Sites cover token opening/derivation, stdio setup, desktop and child admission.
Some retained lower-level errors already stringify their cause before admission;
those can identify only AdmitChildContext, not a specific inner API.

Eighteen exact regressions passed, including a real authenticated original-pipe
exchange carrying a finite site while retaining the same Unknown owner, and all
previous scoped token preparation and access tests. No product child, account,
firewall, existing ACL or VM mutation occurred in those fixtures. Full47/nine,
P2/P3 and Windows SDK26/G04/FREEZE remain open. Fresh roles and a clean independent
Windows VM are required to observe the actual product failure site.

Evidence: D:\build_cache\_audit\windows-runner-site-20261021-013.


## Case012 and creation-only launch-token object access

Fresh roles009 and case012 (tag f44697c1c9f8421eae013971481c0a54, VM
c9639f41-8073-4869-9699-3180ec16871b) returned original steps 1-39 and retained
Unknown at step40. The authenticated original packet identifies
OpenOriginalToken / AccessDenied, specifically OpenProcessToken on R's own
current primary token with TOKEN_QUERY | TOKEN_DUPLICATE. DACL versus mandatory
integrity denial is not directly observed. Original exec88014 is retained without
polling, replay, adoption or termination. The same policy owner restored its one
added right and checked all ten closes; the business case is not clean.

Two independent fixtures on newly owned token objects reproduced denial with an
RC-only object ACL and successful query/duplicate opens with an explicit logon
QUERY|DUPLICATE ACE. Opening that original object for initialization rights was
denied, but DuplicateTokenEx from the query/duplicate handle could create a new
initialization handle. Therefore no extra runner-side duplication stage was added.
The fixture does not identify the historical product's denial submechanism.

The current C path retains its privilege-restricted intermediate and creates final
T exactly once with a protected three-ACE token-object descriptor: C receives its
bounded initialization rights, the exact target logon receives QUERY|DUPLICATE,
and OWNER_RIGHTS receives RC. No owner or SACL is supplied; actual original-handle
readback demands the exact DACL and a bounded NO_WRITE_UP label. Desired access
zero preserves the intermediate's actual grant, checked by the target contract.
The existing final TokenDefaultDacl initialization remains the separate three
RC-only ACE policy. Trusted R process/thread QUERY rights are still absent.
Neither existing token object's descriptor is mutated. A new fixed slot and a
separate once-only native-call reservation account for the additional token;
partial output, unwind, deadline and close debt retain the original owner.

Thirty-one exact native, ACL, token, ownership and authenticated-pipe regressions
passed, including the production token-object creation helper against actual
Windows readback. No host account, firewall, existing ACL or product process was
changed by these fixtures. Full47/nine, P2/P3 and Windows SDK26/G04/FREEZE remain
open until new same-candidate evidence is obtained. Other platforms remain outside
the user's current Windows-only acceptance scope.

Evidence: D:\build_cache\_audit\windows-token-object-20261021-014.


## Case013 and preservation of the creator's original token grant

Roles010/case013 (tag 2dbf8923092d46c581f9c93e3ae9a599, VM
2ee06093-619f-47be-b730-5b9b24e35a9a) returned original steps 1-39 and retained
Unknown at step40. TargetPolicyGuard reported DerivedContextChanged before R was
created. Thus the original R token-open repair is not yet product-proven. The
historical precise changed field is unobserved. Exec25672 remains retained without
polling, replay, adoption or termination. The same policy owner restored its one
added right and checked all ten closes; the business case is not clean.

New owned-token fixtures compared actual native handle grants and token fields.
An initialization-only input retained its grant, explaining why the earlier
fixture missed the problem. With a newly owned full-grant input, creation under
the previous ACL changed 0xf01ff to 0x301ff: WRITE_DAC and WRITE_OWNER were absent.
Both zero and explicit desired-access requests behaved this way. When the new
descriptor retained those two rights for the exact trusted creator C, both
requests preserved 0xf01ff and the observed token fields. All five handles closed
and the source/intermediate remained unchanged. These owned fixtures support the
repair but do not retroactively identify case013's particular changed field.

Current creation ACL: C TOKEN_INITIALIZATION | WRITE_DAC | WRITE_OWNER, the exact
sandbox LogonSID QUERY | DUPLICATE, and OWNER_RIGHTS READ_CONTROL. C already held
these security rights in the original LogonUser handle. No token object is
reopened by name or mutated, no new rights go to R, and process/thread query
restrictions and the separate three-RC-only default template remain intact.
Source/launch granted-access equality is still mandatory. A finite
DerivedGrantChanged observation distinguishes that rejection without relaxing it.

Thirty-five exact regressions passed, including the production creation helper
with both initialization-only and full-grant inputs, actual object ACL readback,
unchanged source context, and explicit rejection of lost or extra launch rights.
No host account/firewall/existing ACL/product mutation occurred. One preliminary
read-only fixture used the wrong scalar query buffer size, returned Win32 24 and
retained its single read handle; exec9929 remains retained. Independent corrected
fixtures used fresh owned tokens, not that owner. All qualification gates remain
OPEN pending fresh same-candidate VM and full P2/P3/Windows SDK evidence.

Evidence: D:\build_cache\_audit\windows-token-grant-20261021-015.


## Paused publication checkpoint, 2026-10-21

The user requested that work pause and the current source be pushed to GitHub.
Acceptance, fixture execution and implementation are paused. The publication
branch is `codex/local-continue-20261020`; this development snapshot retains
`source_accepted=false` and `release_eligible=false`. Intermediate publication
uses `[skip ci]`. Full47/Nine, P2, P3 and Windows SDK26/G04/FREEZE remain open.
Historical test results below apply only to their recorded source snapshots.

Roles011/case014 used source digest
`7b630726aa48b5645c396ca9b73bfeaf1389d9a0cb73f0e33245cccf0d262ac8`,
VM `b6221378-e3a4-4ce9-b81e-2fcd7574e327` and tag
`57c4426218574c938258ae6989dd662a`. The current target policy passed and R was
created. Original steps 1-39 returned, but step40 sealed Unknown with the
same-attempt authenticated `SpawnChild / AccessDenied / OpenOriginalToken`
report. The actual access-denial submechanism remains unobserved. The original
token-open repair is not product-proven. Exec65290 and the original case remain
retained without replay, adoption, polling or termination. Its policy owner
performed one Add and one Remove with status0, restored the initial holder set
and checked all ten policy closes. Policy cleanup does not imply case cleanup.
The pinned receipts are recorded in `current.json` and the local
`windows-token-grant-20261021-015/case014-summary.json` audit.

Phase016 made no product-source or product-policy changes. Independent fixtures
on newly owned objects observed that a new process's actual primary-token object
DACL reflected the launch TokenDefaultDacl and differed from the prelaunch token
object descriptor. The parent could read back this distinction. Experimental
initialization of only the newly created primary object enabled the tested parent
query/duplicate open, but restricted fixture children still exited with
`0xC0000142` before producing their main report. This is bounded fixture evidence,
not a successful standard-user runner or an explanation proven for case014.
Private GUI objects and explicit process/thread labels did not produce successful
child initialization. Debug event7 is DLL unload, not a debug string; the debug
experiments do not establish an OutputDebugString explanation.

The phase016 process fixtures002,003 and009 retain exec65383,71420 and48363,
respectively. Fixture009 retains an uncontinued debug event after its observation
assertion failed. These owners must not be polled, replayed, adopted or terminated.
The proposed new-primary-token initialization is not implemented in the product
and needs policy/lifecycle review and fresh isolated evidence before adoption.
The existing C readback occurs before the original authenticated startup request;
no new protocol or suspended product start has been implemented.

A fresh disconnected 4GB/4CPU VM was prepared from the pinned clean parent:
`Morrow Windows L1 20261021 015`, ID
`83c0b161-623e-4a53-8e82-c1d31443e9ad`, at
`C:\MorrowQA\windows-l1-20261021-015`. Its creation receipt records the original
VM disks/checkpoints and protected VM configurations unchanged. The user paused
before guest login, package staging, fixture execution, account creation or
privilege changes. The managed-account fixture package exists only under the
phase016 audit on D; its hashes are in `current.json`. It has not been executed
and is not acceptance evidence. VM disks are separate from build caches;
host build/cache defaults remain `D:\build_cache`.

Local-only native experiments, runtimes, binaries, build caches, VM disks and the
after-incident host backup are not part of this source snapshot. The complete
local audit remains under `D:\build_cache\_audit`, including phase016
`windows-token-label-20261021-016`. The backup destination supplied by the user
contains an after-incident backup, not a pre-incident baseline. No rollback is
claimed. Resume only after a later user instruction, preserving existing VM
authorizations and all historical Unknown owners.


## Resume and bounded VM resources, 2026-10-22

The user resumed Windows acceptance from pushed commit `f8ef33ec091a82d68f3c2745dde950f8762d8878` and authorized automatic reclamation of surplus unused VMs with at most four retained. Nine superseded, already-offline acceptance VMs with no checkpoints were unregistered and their exclusive disks reclaimed. The operation preserved 101 verified configuration/public-evidence files, the shared clean parent and the topology of the four retained VMs. It contacted no guest owner and called no VM stop. Historical case outcomes remain Unknown; resource reclamation is not product cleanup, retirement or acceptance. The older no-reclamation instructions are superseded only for obsolete VM resources by this direct user instruction.

The four retained VMs are the original development VM, the unclassified pre-existing VM, case014 and the new L1 fixture VM015. Before every future clone, the workflow checks total VM count and must retire an obsolete owned environment before taking a new slot. Shared parents, source, logs, original VM checkpoints and the after-incident backup remain protected. Audit: `D:\build_cache\_audit\windows-resume-20261022-017\vm-reclamation-003\result.json`.

VM015 was started and a fresh protected-adapter identity/readiness probe passed all original17 checks and closed its own transport. This probe created no guest account, changed no policy and ran no product process. The later phase017 continuation below records the fresh experiments and their limits.


## Actual primary-token initialization, phase017 continuation

The fresh standard-user fixture baseline now matches the product runner's Windows GUI PE subsystem. Earlier console fixtures failed in KERNELBASE initialization; a named-event synchronization artifact also returned AccessDenied before the intended token probe. These fixture failures do not explain product case014. A finite, fresh owned-file barrier isolates the token experiment without changing the product protocol.

Fixture016 reproduced own-primary TOKEN_QUERY / TOKEN_DUPLICATE rejection under the existing strict default template. Fixture017 initialized only the distinct newly created primary object through its original process handle. Fixture018 repeated the design with actual implicit-thread probes and exact TOKEN_STATISTICS before/after comparison. Query/duplicate/impersonate opens and original duplicate/restrict succeeded, while direct token adjustment/WRITE_DAC/WRITE_OWNER opens were denied to R. Initial and NULL-SA implicit threads allowed READ_CONTROL but denied context, query and WRITE_DAC. C could not reopen WRITE_DAC after its checked original initialization handle was closed. These are bounded independent VM fixtures, not L1/full47 acceptance.

The current product source now keeps R/OWNER_RIGHTS default-template ACEs at READ_CONTROL and gives only C the finite new-primary initialization rights. It accepts only a distinct actual primary token with the exact kernel-generated four-ACE baseline, changes that object's DACL once, preserves owner/group/label/default and token statistics, verifies the source launch descriptor/statistics unchanged, and closes the original write-capable handle before transport or native-owner publication. New failure sites and one-shot budgets retain the original owner on uncertain effects. The original nonsuspended CreateProcessAsUser and authenticated startup protocol remain in place.

Compilation and 41 exact scoped regressions passed from this working tree. No broad legacy host-provisioning suite was executed. The new source still requires fresh clean-VM L1/full47 acceptance; P2, P3 and Windows SDK gates remain open. Evidence: `D:\build_cache\_audit\windows-resume-20261022-017\result.json` and `managed-fixture-summary.json`.

There were 17 fresh VM diagnostic attempts: 15 checked resource/account/transport closures and two retained Unknowns (fixture010 exec5716, fixture013 exec23716). All 17 temporary policy assignments were restored by the same original owner. Do not query, replay, adopt or terminate either Unknown owner. VM015 was subsequently observed Off through Hyper-V metadata; this was not a stop action by this continuation and is not a cleanup proof. A fresh acceptance environment must satisfy the four-VM cap before creation.

## 2026-10-22: current product child admission and bounded diagnostic repair

Fresh role set012 (canonical digest d25211d1c80b7e6b3734155319c60c93f36d642d360de3e01cce1d2b7f56bd9a)
ran once in clean VM016 under case c69077fb00d140d18e98f33d7fd3a76f. The original
39 steps returned and step40 was reserved. The authenticated original runner
reported AdmitChildContext / NotObserved, with no AsUser native failure. The
new actual-primary initialization and original runner token-open path were
passed, but full child launch/validation was not accepted. Original exec45149
remains Unknown and must not be polled, adopted, replayed or terminated. The
same original policy owner restored its one added right and closed all ten
owned resources; that does not clean the product case.

Source review found that retained child launch errors were converted to plain
strings during context admission, losing their typed site and native code.
The repair copies only finite typed diagnostic facts, keeps the original error
and resources with their original owner, and distinguishes validation failures.
Compilation and43 exact scoped regressions pass, including the actual original
authenticated-pipe test through the repaired io-error boundary. No diagnostic
change is claimed to fix the underlying child-launch failure. New VM execution
of this candidate remains pending; L1/full47/P2/P3/Windows SDK remain OPEN.

VM015 was already Off and its completed diagnostic design work was superseded
by case015. Its17 public fixture records (including two Unknown attempts),
native observations and configuration were archived in185 pinned backup files
before its registered VM and34,890,317,824-byte child disk were reclaimed.
No guest owner was contacted, no VM was stopped and no Unknown was reclassified.
Three VMs remain at this build seal; the next clean clone must make exactly four.
Public evidence: D:\build_cache\_audit\windows-child-admission-20261022-018.

## 2026-10-22: restricted-token lineage, phase019

Case016 on fresh VM017 returned39 original steps and retained step40 Unknown.
Its authenticated diagnostic now identifies CreateProcessAsUser / Other. It
does not export the exact native number. Original exec44531 remains retained;
the same policy owner restored its right and closed all ten original resources.

Fresh fixture019 used the wrong SID namespace, failed token restriction with87,
and remains Unknown at exec49426; it never reached child creation. Fixture020
matched the product's NT-authority capability SID and observed child-create1314.
Fixture021 changed only the token derivation order: restrict the actual primary
first, then duplicate the restricted token for initialization. The child was
created and exited0 without added runner privileges. Fixture022 linked the
actual newly compiled product token functions and repeated that success, also
verifying unchanged source TOKEN_STATISTICS and default ACL. These three known
fixtures closed all owned handles, deleted their fresh accounts, closed their
transports and restored temporary policy through the same original owner.
The legacy fixture envelope's product_calls field is hardcoded, not a measured
counter; fixture022 invoked four actual product token helpers. No full product
session, Job-list launch or inherited-stdio acceptance is claimed by these probes.

The product runner now captures the uninitialized restriction directly from
its actual primary, captures an independent duplicate of that restriction,
then initializes only that final owned token. Failures retain the same original
attempt and source token rights remain QUERY|DUPLICATE. Compilation and43 scoped
regressions pass, including20 real token preparations with original checked
closure and source-descriptor/statistics preservation. Full47, P2, P3 and the
Windows SDK gates remain OPEN until independent current-source evidence exists.

Four VMs are registered at this seal. Both active acceptance VMs retain Unknown
cases. No fifth VM may be created; original VM/checkpoints and the pre-existing
unclassified VM remain protected. Evidence is under
D:\build_cache\_audit\windows-restriction-lineage-20261022-019 and phase018.


## Current private basic witness boundary repair, 2026-10-22

Case017 returned steps1-40 and remains Unknown at step41; policy restoration is
verified. The separate fresh fixture025 observed actual business exit DWORD1,
complete stdout/stderr EOF and `artifact ancestor metadata unavailable`. Its
original Job drained with total2/active0/terminated0, so strict single-process
retirement was refused. Fixtures024/025 remain retained without replay or owner
queries. These facts do not retroactively identify case017's exact predicate.

The native-v8 witness now reads an immutable nonce-bound positive file in its
granted workspace. C retains the private ancestor/owner handles, outside baseline
and absence checks; no private ACL was broadened. Both positive files remain held
through terminal review. Eight scoped harness tests passed, including actual
Windows denial of rewriting/deleting the positive file while C's original handle
is held. The later main-entry change selects the Windows subsystem with inherited
explicit pipes, to test the implicit-console hypothesis for the extra Job member.
Its runtime behavior and complete L1/full47 still require fresh evidence.

Evidence: `D:\build_cache\_audit\windows-restriction-lineage-20261022-019`:
`case017-summary.json`, `basic-private-fixtures024-025.json`, and
`basic-private-boundary-tests/result.json`. P2/P3/SDK26/G04/SDK_FREEZE remain OPEN.


## Case018 and bounded network diagnosis, 2026-10-22

Case018 used byte-bound roles015 and returned the original40 steps before step41
remained Unknown. The original runner history now records physical reaping,
physical output EOF and joined input/output workers, while protocol business
exit1, sticky failure and no-replay remain. These are runner facts, not a business
native-completion or SDK retirement proof. Only fixed public file snapshots were
read afterward; the exact business failure was not observed. Exec73209 is retained.

Independent fixtures026/029 verified the repaired private file boundary and
Windows-subsystem pipe/Job behavior: actual business Job total1/active0, EOF and
original process exit. Fixture029 used a retained listener with positive parent
round trips; without a SID-scoped rule, the restricted child connected to it.
Network denial is still open. Fixture031 failed during fresh COM rule preparation
before business creation and remains Unknown at exec24300; no native COM cleanup
or original case acceptance is inferred. All known temporary policy restorations
were verified. New fixtures never query or adopt old owners.

The rejection path now exports only bounded finite failure categories and byte
hashes from the page already returned. Four exact regression tests pass; failure
and gap pages still accept no terminal evidence, and no second read is issued.
Full47, P2/P3, SDK26/G04 and SDK_FREEZE remain OPEN.

VM017 was independently already Off; its configuration was archived and both
disks retained before registration removal. No live-memory preservation is
claimed for that Off VM. Four VMs remain registered; no fifth may be created.
The scoped old incremental-cache cleanup removed14,368 files (7,334,337,355
logical bytes); observed D free-space gain was7,370,399,744 bytes.55 unrelated
cache roots and all source/executable/library/report/VM artifacts were preserved.
Evidence: phase019 case018-summary.json, network-fixtures027-031.json,
basic-rejected-page-tests/result.json, vm-archive-008/result.json, and
cache-incremental-001/result.json.


## Owned full-offline WFP repair, 2026-10-22

Independent fixture033 reproduced a connection through the production-shaped
COM LocalUserAuthorizedList loopback block rule. Fixture034 added a fresh,
exact-account ALE_AUTH_CONNECT_V4 WFP filter and the actual current business
witness returned native DWORD0 and WSAEACCES10013. Positive parent listener
round trips before and after, Job total1/active0 and physical output EOF were
observed. The original candidate filter and sublayer were removed, absence
verified, engine close checked and SD allocation freed. This is a bounded
diagnostic, not product native-completion or full47 acceptance. Fixture032
remains Unknown (exec77319), with no replay or cleanup claim. The legacy
diagnostic firewall_mutations/product_calls counters are not measured counters.

Owned product setup now derives a private WFP profile from its original request.
An empty proxy list with local binding disabled retains the original12 filters
and adds4 user-only BLOCK filters at outbound/connect and inbound/receive ALE
layers for IPv4 and IPv6. New stable keys keep these separate from legacy
cleanup. Parent receipt binding independently derives the same requested profile
and verifies exact rule identities, actual account SDs and runtime IDs. Original
owned proof carries this profile into scoped retirement. Missing, substituted,
narrowed, foreign-SID or wrong-profile receipts/state are rejected.10 exact pure
WFP tests pass; no host account or policy operation was run. Proxy/local-binding
enforcement and full47/P2/P3/SDK gates remain open pending actual evidence.

Evidence: phase019 owned-offline-wfp-tests/result.json and
network-fixtures032-034.json. Fresh roles016 will bind the repaired source;
historical cases and outcomes are unchanged.


## Case019 and explicit business SystemRoot, 2026-10-22

Case019 used the byte-bound roles016 full-offline WFP implementation. Original
steps1-40 returned; step41 remains sealed Unknown, exec82870 retained. The runner
was physically reaped, but business/native-completion/SDK retirement were not
proved. The already-exported280-byte stderr hash exactly matched the finite
Winsock10106 error envelope. Fresh independent fixture035 reproduced those exact
bytes with the formal business TEMP/TMP-only environment. Fixture036 used the
same helper binary, roles016 C, token restrictions and exact-SID/port WFP fixture,
adding only SystemRoot from GetSystemWindowsDirectoryW. The actual business
process returned native DWORD0, explicit network10013, correct restricted SID,
non-elevation and file-boundary results. Parent listener positives before/after
and original Job total1/active0/EOF were observed. Both fixtures removed their
original new filters, accounts and one-right additions; original transport and
engine closes were checked. Library Job/Desktop Drop outcomes were not observed,
so these independent fixtures do not supply product completion authority.

The formal harness now passes only TEMP/TMP plus the native-read SystemRoot;
the same map is bound into the original request/intent. Unicode conversion and
native query errors propagate before setup dispatch. No ambient environment,
profile load, additional permission or relaxed denial predicate was introduced.
Two environment regression tests and the parameter-binding test pass. Fresh
P/C/R/S roles017 are required before another clean full47 run. P2/P3, SDK26/G04
and SDK_FREEZE remain OPEN; Windows is the only requested target for this work.

VM018 and VM019 were independently found Off and archived under VMBack; their
configuration copies and retained disk hashes were verified before removing
registration. No live-memory, historical cleanup or acceptance is claimed. The
latest archive010 leaves3 registered VMs; the next clean clone may fill the
fourth slot only. Source, reports, disks and prior Unknown evidence are retained.
Evidence: phase019 case019-summary.json, environment-fixtures035-036.json,
business-environment-tests/result.json and vm-archive-009/010 result.json.


## Case020 and original runner wait/exit facts, 2026-10-22

Case020 used byte-bound roles017 with native SystemRoot. Original steps1-40
returned; step41 remains Unknown. Original runner history records native DWORD0,
protocol business exit0, physical EOF and clean timely IO joins. The bounded
50-byte stderr hash matches the expected nonce witness. These are bounded runner
facts, not an accepted business native-completion/SDK receipt. Exec43164 and the
original owner remain retained with no replay; policy restoration was verified.

Source review found that finalization performed the original WaitForSingleObject
and GetExitCodeProcess calls but never populated runner_wait_and_exit_fact, so
the native protocol projection had no exit proof. Two process-only regressions
failed on the original source; after wiring facts at the native call sites, all
three tests pass. They cover actual0,259 and high-bit DWORD exits, wrong creation
identity and absence without minting completion.16 existing protocol/projection/
retention tests also pass. Wait/query failure and late result dispositions remain
unproven; close/GUI/registry/SDK predicates are unchanged. Fresh full47 with
mechanically bound roles018 is pending. P2/P3, SDK26/G04/FREEZE remain OPEN.

VM020 was independently already Off. Archive011 verified its configuration,
retained disk and clean parent before removing only registration, leaving3 VMs.
Its historical Unknown status is unchanged and no live-memory preservation is
claimed. VM021 retains case020. The pre-existing July VM has a73GiB disk and was
not classified as disposable. Maximum4 registered VMs remains enforced.

Two guarded cache cleanups removed3,296 old compiled rlib/rmeta files, totaling
15,445,364,514 logical bytes; observed free-space gain was15,452,311,552 bytes.
Current roles017 compiler outputs, executables, DLL/PDB/Wasm, source, reports,
dependencies and VM disks were retained. All builds/caches remain on D.
Evidence: phase019 case020-summary.json, runner-exit-tests-before/after,
vm-public-path-snapshot-021, vm-archive-011 and cache-libraries-002/003.


## Case021 and transferred original scheduler, 2026-10-23

Roles018 completed original steps1-43 in clean VM023. The actual restricted
business witness returned exit0 and EOF, workspace success, outside denial and
network WSAEACCES10013 with positive baselines. Original task join/ACK, native
runner DWORD0 and the authenticated v8 terminal-close ACK were returned. The
full case remains Unknown at step44: finish-owner required an outer scheduler
that native-context had already transferred to the original Worker. Exec68710
is never replayed, polled, adopted or killed. Policy restoration was verified.

The corrected path uses actual original native thread waits and checked closes
after the Worker's synchronous runtime shutdown. Protected finish requires that
same opaque factory completion; a missing Weak/Arc alone cannot authorize it.
The separate held-runtime path still stages its original before owner finish.
Four focused tests pass using real production constructors and native scheduler
threads, including foreign completion, unstarted factory and double retirement
rejection. The first test invocation matched zero tests and was rejected, then
the exact binary-listed module names were executed. No child, account, policy
or network operation was submitted by these regressions.

VM022 hit Disk Full before any readiness probe or case. Its exact unused child
disk was reclaimed after config/event archiving; no product owner existed there.
VM021 was already Off; its disk and full configuration were verified and retained
before removing registration. VM023 retains case021 and was saved with Hyper-V
to preserve its memory/disk scene while stopping background writes. Save returned
Saved; the actual1,812,316,160-byte VMRS and configuration hashes were recorded.
An initial assumption that VMRS must be at least configured RAM was rejected;
no second Save or automatic resume was issued. Registration count is3, cap4.

Inactive PDB compression verified740 files without changing their content or
mtimes. After reclaiming unused VM022, free space reached81,554,513,920 bytes;
later compiler and guest writes consume additional space. New clones require
64GiB free and all build caches stay on D. Cache/VM work is not acceptance.

Fresh roles019/full47 are pending. P2 clean refusal, P3 editable restart and
fault matrix, Windows SDK26/G04/FREEZE remain OPEN. Other platforms remain
deferred by direct user scope. Evidence is under phase019 case021-summary.json,
transferred-scheduler-tests, vm-archive-012 and vm023-saved-state.


## Case022 and supplementary runner repin, 2026-10-23

Roles019 returned original steps1-44 in clean VM024. Actual basic checks and
original native ACK/join passed, followed by original native factory shutdown,
protected owner finish and bound execution cleanup admission. Step45 remained
Unknown: the supplementary sharing probe retained DELETE access while attempting
to reacquire the read-sharing-only runner pin. Exec33381 is retained without
replay, polling, adoption or termination. The same policy owner restored its
single added right and verified closure. This is not full47 or Nine acceptance.

The exact production file-sharing block was extracted unchanged for regression
coverage. A fresh PE-shaped local file reproduced the same repin failure before
the fix; three rejection tests passed. Closing the temporary probe before repin
made all four actual Windows file tests pass, along with the original bound
cleanup-admission test. No test launched a child, provisioned an account or called
a VM. Borrowed originals, external locks and byte mismatch still reject. A file
sharing observation never grants execution cleanup authority.

VM024 was saved promptly, retaining its memory and4,151,312,384-byte child disk.
The older VM023 saved memory/configuration was copied and SHA-verified, and both
its child disk and clean parent remain pinned in place. Only its registration
was removed after a zero-incompatibility comparison; no memory resume was tested.
Registered VM count is3 and every future creation checks the maximum4 limit.

Inactive compiler compression006 verified1,580 files and36,624,221,131 logical
bytes without deleting files or changing file content/mtime. The observed free
increase was20,152,299,520 bytes, including concurrent VM writes. Build caches
remain on D. Current mr25t output directories inherit NTFS compression.

Fresh roles020 and full47/Nine remain pending. P2 clean refusal, P3 editable
restart and fault matrix, Windows SDK26/G04/FREEZE remain OPEN. Evidence is in
phase019 case022-summary.json, runner-repin-tests-before/after, vm024-saved-state,
vm-archive-013 and cache-compression-006. Other platforms remain deferred by user.


## Case023: original47 and nine required groups, 2026-10-23

The original47 completed on isolated VM025 with byte-bound roles020 and source
`93144acd7caea95e50b9547ede5bd7a8cf76022de0455ea1f7715e58e56d6da4`.
All47 steps returned,92 original input frames delivered3523 bytes, and107
contiguous journal records were cross-checked against their original controller
offsets and hashes. A separate283-check review found all nine original required
groups satisfied for this exact Windows invocation. See
[case023 evidence and limits](CASE023_WINDOWS_ORIGINAL47.md).

Original native and business joins, authenticated ACK, SDK slot retirement,
original scheduler/factory and protected owner finish, owned account/setup/WFP
retirement and outer P/controller native exit0/EOF/joins/disposal all closed.
The same policy owner removed only its new SeAssignPrimaryTokenPrivilege holder
and closed both original transports. The supplementary file-sharing probe now
passes in the full workflow. Historical Unknown cases remain unchanged.

VM025 was saved after closure, retaining4,195,352,576 disk bytes and1,623,531,520
memory-state bytes. Four VMs remain registered, with all other VM states unchanged.
P2 clean refusal across all owners and more than16 consecutive refusals, P3
editable restart/history/fault matrix, and Windows SDK26/G04/FREEZE remain OPEN.
Full network isolation and other platforms are not accepted. Source and release
acceptance stay false. The tested candidate is immutable historical evidence;
subsequent implementation changes require their own fresh qualification.


## 2026-10-23: P2 caller-refusal path implemented; acceptance still open

The independent case023 source93144 original47/nine acceptance remains sealed.
The newer candidate adds a distinct consume-only refusal path for a genuinely
missing caller privilege before LogonUser, pipes, GUI or runner creation. The
original caller box/closed handles, native cell, actual blocking return ACK,
same local registry/connect reservation and exact factory binding carry the
proof. SDK receipt retirement requires the actual start-task return, same
review/operation/slot and original pending fact. Normal exit/EOF/ACK is neither
invented nor accepted from this refusal. The durable spent R2 DispatchUnknown
marker remains unchanged and grants no retry. The original P native generation
is consumed once even on this known refusal.

The local native fixture completed 17 actual refusals in one production local
registry without reset, retained all original cells and observed 34 token
closes. Seven cancellation, deadline, contention, unwind and foreign-caller
controls retained the same original entry. Separately,14 scoped SDK/factory
regressions and13 outer cleanup guards passed. Reports and exact source digests
are in D:\build_cache\_audit\windows-restriction-lineage-20261022-019 under
p2-local-refusal-tests-001,p2-sdk-refusal-tests-001,p2-outer-refusal-tests-001.

Subsequent source adds a disposable-guest-only explicit qualification command
on the actual admitted C origin. It removes only C's existing AssignPrimary
privilege after original setup/review, verifies the same original token and
unchanged other privileges, and checked-closes its query/adjust handle. It does
not edit LSA policy or enable anything. This code has not yet been executed.
The current source still needs combined validation, fresh role binding and VM
refusal cleanup, SDK >16 actual capacity and post-logon refusal coverage. P2,
P3 and Windows SDK26/G04/FREEZE remain OPEN. Four registered VMs are retained;
VM024 Unknown and VM025 clean-case memory are Saved. No push was performed.


## Cases026/027 and P3 history continuation, 2026-10-23

[Cases026/027](CASE026_027_WINDOWS.md) supersede the earlier pending-refusal status above for the exact roles022/source bd69 candidate. One actual full-owner refusal passed 355 checks; a subsequent normal original47/nine run passed 284 checks. SDK >16 full-path capacity and post-logon refusal remain open. Case024/025 stay Unknown. Four VMs are registered; VM026 is Saved Unknown and VM027 is Saved after both clean runs. Archive014/015 retained historical memory/configuration and disks while reclaiming registrations.

P3 now has explicit retired-journal archive/history APIs and a localized view/export/archive UI. Archive admission preserves every original journal, receipt and parent-child relationship; it never reactivates old identities or grants capture/business authority. Limits remain 16 active, 256 unarchived and separately 4096 archived identities/64 MiB current snapshot plus marker bytes. This is not a total database or transaction-history size limit. Unretired imports, retained import bytes and a live original capture scope must block archiving. Import/decision/proposal capacity and editable restart remain unfinished.

The first six-process Windows attempt stopped before application launch because the immutable theme packager rejected changed bytes under the same version. The unchanged source snapshot and failed result are retained at phase019/p3-history-windows-001. Explicit byte-pinned retention now preserves the original theme package while compiling the new guest separately, as with the retained workbench package. This does not qualify those old guest bytes as newly built source. Current host-resource validation and a fresh native run are pending. Codec regressions passed 24 tests and static analysis found no issues.


P3 follow-up: 22 actual host draft tests and 11 handoff tests now pass, including resource refusal, restart, retained bytes and exact lineage. An initial test exposed that the public import phase projects Retired whenever the main draft is inactive; archive now requires actual staging reconcile/prune rather than that display status. The failed test remains retained. All 24 codec tests and static analysis pass.

The second six-process attempt built successfully but could not start the engine: CMake copied the D-cache junction as an invalid NT-path symlink, leaving the installed kernel inaccessible. Source snapshots were unchanged and no phase passed. The install helper now resolves the junction after assembly and installs actual directory contents within the owned application bundle. A fresh synthetic install/reinstall test verified source preservation; the driver also compares installed/assembled kernel hashes. Native acceptance remains pending a fresh run.


[Fresh native history result](P3_HISTORY_WINDOWS.md): all six independent Windows clients passed on frozen source `f5ba873268fe7a9e97114cfadda073fc079e7a5205087ef325605722b8ce3f80` with unchanged before/after manifests. The real host/capture/import/history chain and installed kernel byte checks passed; 33 host/handoff and 24 codec tests are separately retained. Editable restart, full P3 and SDK remain OPEN.


[Editable restart result](P3_EDITABLE_RESTART_WINDOWS.md): eight original Windows clients passed on frozen source `d5551c6ad3eaf4e5fd75ded7f3096f4a5bf13a94b8c4773a2e9c21d5e0f9b8a2`. New and V2 cards preserve raw input and attachments across two restarts, submit once through formal Save, and verify in a fourth process. The first failed close run is retained. Full P3/P2/Windows SDK remain OPEN.


[Formal receipt-loss result](P3_RECEIPT_LOSS_WINDOWS.md): five original Windows clients passed on frozen source `670fb6c3c06e1eef040b2bb3efdf30488c59e954f7666c5704455b68dd962106`. Each real business submission, Prepare, Complete, Retire and acknowledgement occurred once; original inspection retained newer S2 and durable pins. Actual failed retirement and other P3 matrix cells remain open.


[Actual retirement failure and restart](P3_RETIREMENT_FAILURE_WINDOWS.md): three native clients passed on frozen source `c12d29f3185666a620de6fba4fb862c926663e22ed7e3f0bedd51e9d08dfe666`. Exact-parent SQLite rejection preserved both active snapshots; formal next-process recovery consumed the original retirement operation and preserved editable S2 and pins without business replay. Normal close was correctly refused before intentional transport loss. Full P3/P2/Windows SDK remain open.

## Cap'n Proto continuation, 2026-10-23

The selected derived parent/Runner and v8 native-close chains now use typed Cap'n Proto. The frozen fe7de7d5 source passed 43 pure tests and case028 original47/nine (284 checks). Case029 later latched an outer parent ObserveChild InvalidData and remains Unknown, despite the inner P2 workflow reaching its final journal record. No old owner was replayed or polled after failure. Same-owner policy restoration is not observed. New finite exit diagnostics preserve all acceptance predicates and have no native acceptance yet. See [the bound report](WINDOWS_CAPNP_CASE028_029.md). Windows P2/P3 and SDK26/G04/FREEZE remain open.


2026-10-23 continuation: roles024/source51fba032 passed actual caller-refusal case030 (355 checks) and normal case031 original47/nine (284 checks). Four VMs remain registered; VM028 Saved, prior Unknowns retained. P3 draft-write recovery UI and bounded real disk-full verification are in progress. P2/P3/SDK/source/release remain open. See WINDOWS_CAPNP_CASE028_029.md.


[Real disk-full recovery](P3_DISK_FULL_WINDOWS.md): two native clients passed on source894ada3b. Windows112/zero free space, CommitUnknown retention, unchanged S1, exact S2 retry with S3 preserved, one business Save and exact next-process evidence were observed. Failed fixture attempts001/002 remain retained. Full P3/P2/SDK and combined-source acceptance remain open.


[Formal parent archive and original import cleanup](P3_PARENT_ARCHIVE_WINDOWS.md): nine native clients passed on source 3de6d36972c40752f978cd5ec019b6f5a0c62e732e659f2fae546d201aed6144. Completed original proposals now allow archive while preserving lineage fences; formal commit cleans its retired parent imports before acknowledging. Lost cleanup reply uses one original dispatch and one inspection; all five previous reply-loss cases passed through the updated flow. Earlier actual host 28+12 methods and failed native attempts001/002 are retained with their own sources. Long-term auxiliary history and full P2/P3/SDK gates remain open.


[Bound auxiliary archive](P3_AUXILIARY_ARCHIVE_WINDOWS.md): 44 actual host methods and four native clients passed on source 5747c341ec49c0c089c9bcc0729899b866bba669de8abcd3efc67cd222fb4ee3. Actual 256-entry histories regain admission only through explicit bound archive; original records and operation fences survive. Current reader opens an unchanged original version 1 copy. Legacy accounting maintenance and full gates remain open.


[Case032 checked directory closure](WINDOWS_CWD_CLOSURE_CASE032.md): new original47/nine passed 284 checks after 93 selected regressions on source 051fe4cd898c7a5b91bca6de3d3f96099789da714869c35542c05ef1b3f3550e. Directory pins now require checked native closes before original completion. [Frozen Windows transport](SDK_FROZEN_TRANSPORT_WINDOWS.md): six actual original methods exercise all six untouched C/C++/Rust packages through loopback HTTP/service and durable outcomes. Historical Unknowns retained; VM028 Saved and registered count four. Full P2/P3/SDK/source/release gates remain open.


[Pending editor context and Windows TLS/DPAPI](P3_CONTEXT_AND_WINDOWS_TLS.md): one real native client preserves the original pending commit and raw newer input through two locale/font changes; eleven actual Windows methods qualify bounded loopback TLS and DPAPI. Source a7a956a726d88a09b79b4ae8475cb552cf4f10e01001bb68a66f1f954f3cb975. Preferences/pixels/physical input, workspace transition, complete P2/P3/SDK and source/release acceptance remain open.


[Two-workspace handoff](P3_WORKSPACE_WINDOWS.md): native client20984 passed on source b8d8095bc2d39f15efce5327b18faa426ebaabf267d839f889ec41aacf5a93a1. A pending parent-retirement confirmation disappears on switching to an actual second library; both original active records remain on A, B stays empty, and explicit confirmation after returning retires only the original parent. Exact raw child bytes and business revision survive reopen. Full gates remain open.


[Case033 retained](WINDOWS_LOGON_REFUSAL_CASE033.md): the first39 steps returned; the logon-input fixture incorrectly queried an outer factory slot already transferred to the original Worker. Actual LogonUser was not reached. Same-owner policy restoration completed, but product cleanup did not. Session20924 and VM028 remain Unknown without replay. A Weak reference correction is pending new independent VM acceptance. No verified clean VM is available under the four-VM cap; independent SDK qualification continues.


[Frozen SDK TLS/DPAPI](SDK_FROZEN_TLS_DPAPI_WINDOWS.md): source 67ff5ebeefd97e5e74e64a7c5e9850c5afa4acf4fe24cf4eeeb0e67169df20c3 passed three methods/nine Rust-C-C++ scenarios using original unchanged IO packages, actual TLS and reopened Windows DPAPI credentials. Correct root/hostname yields exact durable response; wrong root or hostname never reaches the handler and remains Unknown with unavailable response evidence across reopen. Full Windows SDK and product gates remain open.


[Cases035/036](WINDOWS_LOGON_AND_NORMAL_CASE035_036.md): source47a9b9ce passed one actual LogonUser1326 refusal through the complete original cleanup chain (355 checks), then original47/nine (284 checks). VM029 is Saved clean, four VMs registered. VM028 was already Off; only its registration was reclaimed, with configuration and disks retained. Historical Unknown owners were not contacted. Full P2/P3 and Windows SDK gates remain open.


[Windows capacity](P3_CAPACITY_WINDOWS.md): two original-limit host methods and two independent formal native clients passed archive exhaustion, retained original-operation recovery, history maintenance and restart checks. The unused offline VM registration was archived with its original disk/config retained; three VMs remain. Full P2/P3 and Windows SDK gates remain open.


[Cases039/040](WINDOWS_SOURCE_POLICY_AND_NORMAL_CASE039_040.md): source3c779ec5 passed one actual post-logon source-token policy refusal through the original cleanup chain (355 checks), then original47/nine (284 checks). Case037 remains Unknown in saved VM029 after a controller optional-field mismatch; no old owner was replayed. VM030 is Saved clean and four VMs are registered. Full P2/P3 and Windows SDK gates remain open.

## 2026-10-24 main archive count

[Windows capacity](P3_CAPACITY_WINDOWS.md) now records the actual4096-main limit, refused4097th record, exact original retry and complete retained-store restart verification on source70e8. Legacy auxiliary maintenance is a newly applied candidate with a distinct immutable receipt and Cap'n Proto transport; its canonical and original-library native acceptance is pending. Full P2/P3 and SDK26/G04/SDK_FREEZE remain open.

## 2026-10-24 legacy auxiliary maintenance

[Legacy maintenance](P3_LEGACY_AUXILIARY_MAINTENANCE_WINDOWS.md) passed three protected host compatibility methods and two actual Windows clients. The latter use a byte-identical original v1 library copy; original marker, complete history and source library remain unchanged. Maintenance uses a distinct immutable receipt and Cap'n Proto IPC, cannot double-credit quota, and survives restart. The four-VM inventory is unchanged. Full P2/P3 and SDK26/G04/SDK_FREEZE remain open; source/release acceptance is false.

## 2026-10-24 protected directory SDK

[Protected Workbench directory SDK](SDK_PROTECTED_DIRECTORY_WINDOWS.md) passed three actual Rust/C/C++ guest methods on sourcea7bcc25b: original protected Session/worker ownership, explicit FileList approval/refusal/revocation, exact final page, checked join/disconnect/maintenance, native handle closure and approval persistence across reopen. These synthetic nominated-root tests do not establish picker or GUI provenance, full G04, SDK26 or SDK_FREEZE. Full gates remain open.


## Paused publication checkpoint, 2026-10-24

Case041 passed the original47/nine review (284 checks) on roles029/source77f509c0. Shared SDK owner009 passed 15 methods including 17 frozen guests; native010 passed 125 selected methods. Case042 then ended Unknown after the observer stopped; no completed round count or final native closure is claimed. Its original same-owner policy restore is verified. VM030 memory and disk are saved, all four VM registrations remain, and the original host session56421/PID12972 is retained without querying or terminating it. Full P2/P3 and Windows SDK remain open. See [the pause handoff](PAUSED_WINDOWS_20261024.md) and its committed public evidence. No acceptance work is scheduled or resumed by this publication.


## Paused publication checkpoint, 2026-10-25

The current task is paused by direct user instruction. [The new handoff](PAUSED_WINDOWS_20261025.md) records current-source case078 original47/nine, case079 setup-failure retention, the defined Windows P3 matrix pass and scoped SDK directory/approval results. Case080 did not reach late creation: an old setup-fault library was reused by the qualification build. It is rejected, its original Unknown remains retained and VM035 is Saved. Four VMs remain registered. Full P2 and Windows SDK26/G04/FREEZE, source and release acceptance remain open. [Updated progress report](../../reports/project-progress-2026-10-25/index.html). No further acceptance starts until a later explicit resume.


## Active continuation, 2026-10-26

The user resumed Windows work and explicitly authorized a local VM password file; protected per-phase input now works without repeated manual-login prompts. Private contents are absent from this report and repository evidence. Build caches remain under `D:\build_cache`; four VMs are registered and VM039 is Saved.

Case080 cache contamination is repaired by distinct cold C/S output directories, full compiler-artifact checks, the rebuilt setup library and forbidden-marker checks. Case098 on source4040 passed772 independent checks: actual process/thread outputs were held, an injected delay crossed the unchanged original deadline, and the original first failure was Create/DeadlineGuard/TimedOut. Its155 native records are separate diagnostic evidence; the original observer retained88 of89 journal records. The case remains Unknown, `case_clean=false`, `policy_clean=true`; original host PID4752/session1237 is not polled, adopted, terminated or replayed. This is scoped fault evidence, not normal completion, natural OS-delay proof or full retirement.

SDK101 adds an explicit native directory capture API that returns the original File when admission or queue delivery is refused. Eight Windows File ownership checks plus18 related existing tests passed. The tested change is integrated; original Workbench selection ownership, the formal picker-to-worker path, checked worker close receipts, durable blob/upload and complete production network approval chains remain open. The new SDK source is not relabelled as the earlier case098 runtime source. Historical P3 matrix065 retains its own binding. Full P2, SDK26/G04/SDK_FREEZE, source acceptance and release eligibility remain open/false.

VM038 is fully verified and unregistered; its archive depends on the clean-parent member in the VM037 archive. Preserve both. VM039 memory and disk remain Saved. [Current HTML progress](../../reports/project-progress-2026-10-26/index.html) · [Public evidence](evidence/resume-20261026/manifest.json).


## 2026-10-26 continued: storage reclaim and SDK106

Two guarded cache passes removed 9,502 old rlib/rmeta leaves (26,313,265,979 logical bytes). Protected binaries, symbols, source, current SDK cache and historical evidence were retained. VM035 and VM036 raw archive duplicates were losslessly repacked: all nine original files in each set passed full CRC/SHA readback, original runtime disks stayed unchanged, and final archives are under D:/build_cache/VMBack. Both sets require the pinned clean-parent member in VM037; retain that archive and VM038's existing dependency. VM036's temporary C ZIP was removed after the final D copy matched. The “新建虚拟机” disk/archive was untouched. See storage112-summary.json for actual free-space snapshots and logical-versus-physical limits.

SDK106 is integrated. Workbench admission now borrows the original selected File until its original worker accepts the queue request. Known early refusals and post-spawn product-gate refusal preserve that exact handle. Six real Windows original-file tests and six existing protected directory regressions passed, including Rust/C/C++ guests, immutable submission refusal, revocation, native identity, rename exclusion, checked caller close and actual worker reclaim. Native-picker integration, spawn-failure injection and checked worker-side close receipts remain open; this is not full SDK acceptance. The initial 867-artifact cold build and final recompile are recorded separately.

All four roles039 programs are newly cold compiled on the SDK106 code snapshot and byte-bound: parent P, token-refusal controller C, Runner R and setup S. Every Cargo artifact was freshly compiled in its own reserved D target. Two V5 qualification overlays are prepared; token-derivation is now compiled into C, while default-DACL remains syntax-only. Fresh VM execution is NOT_RUN. The earlier 108 draft had an incorrect HANDLE representation and was rejected before compile or execution; use 109 only. Historical case098 retains source4040 and its original Unknown owner, with no replay or contact. Registered VMs remain four. Full P2, SDK26/G04/SDK_FREEZE, source acceptance and release eligibility remain open/false. All work remains local; no commit or push was requested this turn.


## Active Windows continuation, 2026-10-27: case118

VM039 was fully archived and independently verified as a complete archive set (configuration, disk, saved memory and the VM037 shared parent), then unregistered with original disks retained. Its old Unknown owner was not contacted. VM040 was created disconnected with4GB/4CPU and passed original readiness. Login input came only through the already authorized protected file producer; no manual prompt or private value entered these records.

Case118 on roles039/source e962 passed920 checks. The original driver completed39 steps and reserved native-start once at step40. Actual CreateRestrictedToken with the qualification-only null input returned Win32 6. The original first failure is Token/LaunchTokenDerivation/Uncategorized, Win32 6; it is preserved in the original delivery snapshot as IoOther/ReturnedError.53 contiguous native records retain the actual login token Held, restricted output Rejected, launch/process/thread Empty; process creation and default-DACL calls are zero. Controller prefix88of89 journal records is byte-bound; the final retained failure is independently copied. This closes only the original first-failure observation scope, not resource retirement, normal47 or full P2.

The same policy owner restored only its newly added right and verified the original holder set. Product Unknown remains. VM040 memory/disk is Saved, four VMs are registered, and original case hostPID15296/exec51916 must not be polled, queried, killed, adopted or replayed. The one separate diagnostic transport closed itself.

SDK checked-close source review119 mapped six production lifecycle gaps: published observation retirement, partial capture/ancestors, Resource Drop, accepted queued captures, worker unwind/exit and the private native boundary. No close repair is implemented or accepted yet. AGY117 returned a known headless command permission refusal with no edits/structured result; root retained that result and continued independently. Candidate D:/build_cache/worktrees/sdk-close-117 is seeded at checkout0904 and remains available for the full ownership implementation. Do not re-run the AGY conversation automatically.

Next: complete checked original-worker resource retirement in the isolated SDK candidate, and prepare/default-DACL real native failure in a fresh VM after verified offline archival of VM040. Preserve all original Unknown owners, the new virtual machine and VM037 shared-parent archive dependencies. Caches stay D:/build_cache; maximum4 registered VMs. P2/SDK26/G04/SDK_FREEZE/source/release remain open or false. No commit/push was requested this turn. See [current HTML report](../../reports/project-progress-2026-10-26/index.html) and [case118 evidence](evidence/resume-20261027/manifest.json).


## Active Windows continuation, 2026-10-27: SDK143 and VM040 archive

SDK143 is integrated after147 scoped methods:89 directory/retirement checks,12 protected Workbench Rust/C/C++ regressions,1 synthetic incomplete-receipt host policy check, and45 shared owner/IO/file worker regressions. The original directory worker now records one real CloseHandle attempt per owned root/ancestor, retains original quota and native records on ambiguity, and checks terminal page/finish/cancel/reap/partial capture/queued cancellation/unwind/exit. Status reads are atomic and remain responsive while the private native record is held. WorkerExit retains unresolved directory records; Workbench cannot clear them through ordinary storage repair, acknowledgement or a new owner/task. The source-baseline review covered all candidate files, and canonical git diff --check passed. Fifteen selected source/test files were integrated; previous mixed CRLF host formatting was normalized.

Real Windows close calls and object exclusion were exercised. Lost-close-observation injection occurs after an actual successful native close and is synthetic; it is not evidence of an actual CloseHandle refusal. The host latch test likewise uses an incomplete policy fixture, paired with actual original-worker retention tests. Coverage is specifically the selected-directory lane, not every native resource type. Formal picker-time identity, complete blob/upload and network approval routes, full SDK and release acceptance remain open. Historical P2/P3 evidence keeps its original source binding.

Cache130 removed2470 regenerable rlib/rmeta leaves,4286638456 logical bytes; all source, executables, DLLs, PDBs and prior reports were preserved. VM040's full original saved disk, configuration and memory passed independent decompression/CRC/SHA verification with the pinned VM037 shared parent. Registration was released only afterward; original runtime disks remain, three VMs are registered, and original VM/VM029/VM030 are unchanged. Keep the VM037 archive while any dependent VM035/036/038/039/040 archive remains. Case118 hostPID15296/session51916 and all historical Unknown owners remain untouched.

Next: cold-build the default-DACL qualification variant for the current integrated source in distinct D targets; create fresh VM041 only within the four-VM ceiling, use the already authorized protected credential-file producer, and execute the original pre-registered P2 flow. Complete the remaining SDK routes without relabeling partial evidence as SDK26/G04/SDK_FREEZE or release acceptance. No commit or push was requested in this continuation.


## Active Windows continuation, 2026-10-27: case147 default-DACL refusal

The frozen SDK143-integrated source d7827205fe709e253b0f39db19b7a11d2ebcc2c1de01cc0aa84f0469c9a196fb completed four independent cold builds (P871/C872/R620/S620 compiler artifacts, all fresh=false) and mechanical source/overlay/staged-byte verification. New disconnected VM041 passed pristine readiness, original gather and binding before one original47 driver attempt. The qualification-only null input to real SetTokenInformation(TokenDefaultDacl) returned Win32 6. Original first failure TokenSecurity / DefaultInitialization / Uncategorized / win32=6 / ntstatus=None reached the original delivery unchanged. Login, restricted and launch token handles stayed Held; process/thread outputs stayed Empty.

The preregistered reviewer passed1469 assertions over93 native records,89 original journal records and88 exact controller-observed records plus one independently copied tail record. There were39 completed steps and one reserved native-start step40. This is a scoped native-refusal result, not a full47/nine or resource-retirement pass. The same original policy owner removed only its actually added right and closed its own policy resources; product case_clean=false remains. The independent diagnostic connection closed after one fixed quiet interval and one allowlisted read. Original hostPID18988/exec6382 and guest nativePID1332 must not be polled, queried, killed, adopted or replayed.

VM041 memory and disk are Saved, four VMs remain registered, and original VM/VM029/VM030 are unchanged. VM040's verified archive and original disk remain retained; keep the VM037 shared-parent archive required by the compact archive set. All build/temp/cache work stays under D:/build_cache.

SDK148 confirms a remaining formal-picker implementation gap: Flutter currently stores a path and the private Capnp route opens it only at Start. The existing retained-original-File entry is not connected to that formal picker. Picker-time native identity, formal approval/all-language UI, persistent blob/upload, full SDK and combined qualification remain open. Next work must follow the original handoff's full P2 fault/ownership matrix and SDK26/G04/FREEZE obligations; do not reclassify any historical Unknown or narrow the release definition. No CI, push, tag or release was performed.


## Active Windows continuation, 2026-10-27: case155 and SDK162

Completed four separately cold-built roles on source 10184a3712c7d081b4784418ac3a6297934f78096392a321ed7303bd852df55a. VM042 passed original pristine readiness, stage and binding, then ran one original47 driver attempt with a qualification-only protected runner-query token. Original CloseHandle failed with Win32 6 at TokenSecurity/RunnerTokenClose. The 240-record trace and original journal passed 5113 assertions; the exact first failure, CloseFailed state, observation debt and original process/thread/token ownership were preserved. There were39 completed steps and one reserved failure step40; this is not full47, resource retirement or full P2 acceptance. The same policy owner removed only its newly added right.

VM042 is Saved with memory and disk retained. HostPID13436/exec96916 and guest nativePID5508 are Unknown owners: no polling, querying, adoption, termination or replay. VM041 was fully archived with verified shared-parent dependency, then unregistered with originals retained. Four VMs remain registered; original VM/VM029/VM030 remain unchanged. Clone dispatch166 failed before any VM mutation because its shell lacked Get-FileHash; reviewed corrected dispatch167 succeeded. The source-line clerical correction is separately retained.

SDK162 cold-compiled and actually ran three methods/four fresh Windows fixture scenarios against unchanged production bytes: direct close refusal, finish, terminal page, and queued-input retirement. Native error6 and the original resource charge survived, with no synthetic close-observation loss. Process exit is not original retirement. Independent review163 passed. Integration165 copied the exact five reviewed test-only files; the complete checkout then matched the tested candidate byte-for-byte at source144ab25d0b4a1545948876e3ad01638973f2a47f14d937de4c0893e4edb1abd2. Production safety lints were not relaxed. Earlier compile160 failed before tests; its logs remain.

Cache164 deleted2470 pinned retired rlib/rmeta leaves,4287170848 logical bytes; cache158 stopped before deletion and freed0. Together with cache130,8573809304 logical bytes were reclaimed. All cache/temp work remains on D:/build_cache. VM archives, original disks, executable artifacts and reports were retained.

Next: partial/abnormal-output P2 evidence and complete current candidate qualification; formal retained native picker/approval and original Rust/C/C++ UI, persistent blob/upload and complete SDK26/G04/FREEZE obligations remain open. Keep historical bindings and Unknown outcomes intact. No CI, push, tag or release was performed.


## Active Windows continuation, 2026-10-27: case243, native256, full-main259

Current b09 source and four independent cold roles044 passed the original47 and all nine required groups in VM045: 285 checks, 92 delivered frames, 3523 bytes and 107 original journal records. Original authenticated v8 completion, worker/monitor joins, account/setup/firewall cleanup, policy restoration and original transports returned. VM045 is Saved; four VMs remain registered. This is the current normal path, not full P2.

Native255 cold-built the exact current library; native256 ran only eleven reviewed methods. The actual read-only caller test completed 17 refusals in one original registry, 34 checked token closes, and seven negative controls preserving original capacity. No Runner, account or policy mutation, or VM call occurred. SDK capacity and the rest of P2 remain open.

The separate SDK244 candidate at D:/build_cache/s244s adds typed live refusal delivery through private Capnp while keeping native uncertainty and worker joins separate.31 Flutter checks and two Rust boundary tests passed. Old SDK234 Rust/C/C++ flows passed in qualification shells; this is not full application acceptance. Native cancel251 lost its original process observation before approval and has no completion receipt. Full-main252 and supervisor253 compiled from exact input4b7030a17be2d8a93d5988208f567a2cbc09ac3fda01fe94e67e417f27ac313d. Full-main254 stopped at an observer-only Windows long-path mismatch; fix258 passed 4 positive and12 rejection cases. Preserve UI9708/supervisor19532/host6508/exec51554 without contacts or replay.

Full-main259 reached real normal startup with UI4908/supervisor17972/host3344, then crashed when full settings was expanded under accessibility observation. Original UI exit3221226525, engine AXTree errors, Windows event1000 and matching shipped PDB resolve the native fault to FlutterPlatformNodeDelegate::ChildAtIndex/AXNode::id at flutter_windows.dll+0x3e755. Preserve original host/supervisor/exec79784; do not poll, query, adopt, kill, or replay. No plugin import or directory operation was accepted. Next resolve the Flutter3.44.0 accessibility-tree defect with a reproducible scoped dependency fix while keeping accessibility enabled; then qualify the full application and remaining P2/SDK obligations.

Cleanup257 reclaimed4936 old compiler rlib/rmeta leaves,8585965342 logical bytes, preserving all executable artifacts, sources, reports and VM disks. Cumulative logical reclaim this resume is51515100584 bytes (47.98GiB); observed free-space change is separate because builds run concurrently. Cache/temp stay on D:/build_cache. All release flags remain false. No CI, commit, push, tag or release was performed.


## Active Windows continuation, 2026-10-27: full-main288/293 and reproducible framework

The full-settings accessibility crash was reproduced as unattached native semantics nodes. A pinned local Flutter framework candidate282 keeps accessibility enabled and production main unchanged. Twelve focused regressions, 97 upstream semantics/OverlayPortal tests, 31 project lifecycle tests and 90 full-application serialized frames passed. The global Flutter SDK remains unchanged. The repository now includes an exact-source preparation tool, patch, pinned manifest and license. Reproduction290/291 produced 733 byte-identical framework files; seven cache/path/source-drift guards passed. Fresh full-main292 compiled against this independently reproduced framework.

Full production UI case288 imported/enabled the Rust fixture and saved file-list approval through actual desktop interaction, cancelled the real native picker, observed original worker join and ACK, closed normally, then independently reopened with approval preserved and no old active task. All six original UI/supervisor/host processes exited zero. Review289 is a bounded cancellation pass.

Full production UI case293 used the fresh292 build, a new profile and 70 synthetic files. Actual import, enable, approval and native folder selection succeeded. The production UI reported completed guest execution, 70 captured entries, original worker exit/content-library return, and idle after original ACK. No per-guest-call count is claimed from this UI. Normal close and independent reopen again returned all six original processes with exit zero; fixture bytes stayed unchanged and saved permission remained visible. Review294 is a bounded Rust successful-directory pass. Full SDK, all-language full main, P2 and release remain open. SDK source candidate s244s/input4b7030a17be2d8a93d5988208f567a2cbc09ac3fda01fe94e67e417f27ac313d is not integrated into canonical product source.

Historical full-main287 native-picker uncertainty remains Unknown. Do not query, poll, kill, adopt or replay its original UI27640/supervisor29104/host15836/exec72476. Earlier crash owners are likewise unchanged. No VM was mutated by these full-main runs. Four VMs remain registered; new caches and all build/test outputs remain on D:/build_cache. Cumulative logical cache reclaim remains 51,515,100,584 bytes. No commit, push or release occurred.


## Active Windows continuation: full-main296/298 and aggregate300

C and C++ fixtures now each completed the same unmodified full-production-main flow as Rust293: actual import, enable, file-list approval, real native folder selection, 70 synthetic entries, UI-reported successful guest result, original worker return, original ACK, normal close and independent reopen with approval preserved and no old active directory task. Reviews297/299 reconcile all six original process handles per case with zero exits and unchanged fixture bytes. Together with Rust293 and separate cancel288, aggregate300 contains 24 distinct normally returned original UI/supervisor/host process identities. Per-guest-call counts are not visible in this UI and are not claimed.

All three successful cases share input4b7030a17be2d8a93d5988208f567a2cbc09ac3fda01fe94e67e417f27ac313d, full-main292 and the reproducible framework290. C296 recorded a delayed accessibility text read while the screenshot already showed worker return; complete accessibility fidelity is not accepted. The formal directory scope is now bounded-pass for three languages. Other SDK26 capabilities, canonical source integration, P2 and release remain open. Earlier Unknown owners are untouched. The report has static link/content verification; browser visual preview was refused by the browser local-file policy and no workaround was attempted.


## Active Windows continuation: canonical SDK342 and current P2 cases329/335

Current checkout f8492320dbf7e156bfccadae687f50362a7aafb4d1c58e90014170aae9b04f78 completed fresh cold-role normal47 case329 and shared17 case335 in VM045. Case329 delivered all 47 steps, 92 frames and 3523 bytes with 107 journal records, nine required groups and 285 review assertions. Case335 executed 799 steps, 17 actual post-logon refusals in one original parent/owner, 1564 frames and 1837 journal records; 5730 assertions verify original closure and resource accounting. Both cases and their original policy owners closed; only newly assigned privilege was removed. VM045 was saved after case329 and the other three registrations were unchanged. The original case329 reviewer retained an uppercase CASE243 display label; the separate pinned correction changes that display label only, preserving its exact case329 inputs and every original assertion.

Canonical product builds305/309 completed Rust312, C337 and C++332 directory flows plus independent cancel340: actual import/enable/approval/native selection, 70 entries per success fixture, original worker return, ACK and reopen. Twenty-four distinct original UI/supervisor/host handles returned zero. Binding334 preserves the actual original product input and separately explains the unused qualification-lock edit; these are not new builds. Native325 passed 92 Rust methods, 14 native executable runs and three external consumers; distribution323 passed 64 methods and retained all 57 frozen inputs. Counts describe different methods and do not form a release total.

Large cache cleanup cumulative logical bytes are 66835141360; current free space is measured separately. All caches stay on D:/build_cache. No historical Unknown owner was polled, queried, killed or replayed. Full P2 fault matrix, final P3 source review, durable blob backend/history, versioned upload/watch/rename, remaining SDK26 and release are OPEN. This documentation update does not relabel f849 runtime evidence with a later checkout digest. No commit, push, tag, release or system rollback occurred.


## Active Windows continuation: streaming owner357 and fresh mutation guests356

The later implementation input8a5aee593e8988cfcf7ba5877cc494be087c88822d2f4fe04941a161e52473db now connects existing Cap'n Proto blob frames to the original worker, Store transaction and durable schema26 staging receipt. Explicit local-owner validation, cancellation/deadline/revocation, cumulative resource charging and post-commit Unknown are retained. No new guest import, product approval or protected blob Session is implied.

Core351 passed865 methods (19 ignored); runtime351 passed149 affected methods. Counts include14 core staging and17 new blob methods. Additional actual controls/reconciliation352 passed19 and private original-owner methods354 passed33. Strict library Clippy passed for core and runtime; cold production library353 passed compilation. Six initially ignored mutation methods were actual Wasm SDK scenarios: fresh Rust/C/C++355 artifacts executed ordinary and16MiB flows in356, all six passed with the original30-second binding. These are ordinary temporary Store/Manager API approval fixtures, not product UI or guest blob upload acceptance. Review357 corrects earlier inaccurate helper labels and excludes zero-selected commands; all raw failures remain.

All57 original frozen SDK inputs are unchanged. All current build/test processes returned. Four VMs remain registered, no VM or host policy mutation occurred. Caches/temp/build outputs remain on D:/build_cache; cumulative logical reclaim remains66835141360 bytes. Report358 records a fresh free-space sample separately. Old P2/UI source bindings and Unknown owners remain untouched. Public versioned upload, production/protected-owner approval and recovery, watch/rename, full P2/P3/SDK26 and release remain open. No commit, push, tag or release occurred.


## Active Windows continuation: retired compiler cache359

Cache359 freshly inventoried the four completed roles046 targets and removed only4936 compiler-declared, older, untracked .rlib/.rmeta leaves (8587002071 logical bytes). Exclusive Cargo locks, exact root/reparse/Git/recency/metadata/hash guards and compiler checks preceded deletion. All protected original program/log/source-binding pins and remaining namespace metadata were verified afterward. No executable, DLL, PDB, source, report, VM or archive was removed. Reusing these old targets for later qualification is forbidden; new builds still require fresh D:/build_cache targets.

Cumulative logical reclaim is now75422143431 bytes; observed free space remains a separate fresh measurement in record360. Four VM registrations and states remain unchanged. The C: system pagefile was observed read-only, with no paging or hibernation configuration change. Qualification357 and the historical P2/UI bindings remain intact. Public guest upload, protected/product approval and recovery, the remaining P2/P3/SDK gates and release stay OPEN. Goal work continues; no commit, push or release occurred.


## Active Windows continuation: independent upload request candidate362

The new extensions/blob-upload-request-v1 crate provides independent Cap'n Proto Open/Push/Inspect/Release requests, exact-original digest/nomination/nonce response binding, explicit receiving/staged/released/absent states and existing blob chunk receipts. It preserves all old task/IO/channel/blob contracts and does not borrow FileRead/FileCreate permissions. Requests remain64KiB; embedded frames64512 bytes, responses4096, objects16MiB. The bounded Rust client preserves one original pending request, cumulative charges and nonces; transport failure, panic, malformed/foreign delivery or explicit Unknown forbids automatic replay. Inspection cannot reconstruct a sender; durable/released history cannot regress to a fresh upload.

Input649b4bb929738727f2ee8dd171e81e03247121894b4b6c5098e047bb874eac58 passed19 request/client methods and strict library Clippy in362. This includes the existing receiver's274-chunk16MiB protocol path with a clearly fixture-provided Staged response; it is not a real Store commit or guest execution. Initial361 passed14 earlier methods and remains separate. All57 frozen SDK inputs and the previous351 runtime inputs are byte-identical. All build/test processes returned, caches stayed on D:/build_cache, and no VM or host policy action occurred.

This crate is not yet a registered package feature, public import or host route. C/C++ helpers, explicit original package/manager approval, original-worker durable dispatch, protected Session/product/recovery, actual Rust/C/C++ guests and remaining P2/P3/SDK gates remain required. No source/release acceptance, commit, push, tag or release is claimed.


## Active Windows continuation: native upload helpers368

Input846d1c59b271c791df5aaac0aa903ec970f53e8a25c07a48d97017e57dd1c107 now includes checked C ABI and move-only C++ owners, without registering any package feature/import or authority. Review368 verifies the original cold staticlib364 and19 Rust methods/strict library Clippy, MSVC C/C++ executions366, and Clang C/C++ executions367. Each C consumer passes8 scenarios/6020 checks, including16MiB/274chunks, guard pages, exact original reply binding, cumulative budgets and Unknown no-replay. Each C++ consumer passes44 checks of moves, vector preservation, original correlation and callbacks. These per-compiler counts repeat the same scope and are not additive coverage. Staged replies are explicit fixtures; no Store/guest/product proof is claimed.

Native library flag parser364, executable resolution365 and Clang library filename366 failures remain in their original evidence. No protocol or test was weakened to pass. All original processes returned. Cache remains D:/build_cache, registered VM count4, no VM mutations or host policy operations. All57 frozen SDK inputs and prior351 runtime inputs remain unchanged. Next: explicit independent package/manager approval and Runner profile, original-worker durable dispatch, protected Session/product/recovery and real Rust/C/C++ guests. Full P2/P3/SDK/source/release remain open; no publication.


## Active Windows continuation: independent upload package/approval372

Input400e55a0d69269a0a7a612b3cc2a71072ffccf7ae738dc777874a863752e19ec adds isolated Manifest field22, explicit package profile validation, and separate exact-package upload approval in Registry v3/Manager. v1/v2 cannot smuggle the new permission; old v2 publications stay compatible, and v3 history does not downgrade on revocation. No approval is inherited on a different package digest. Validation precedes manager revocation, and CommitUnknown blocks further decisions including no-ops. No Store/object/live-owner grant is restored. PreparedPackage currently refuses the new profile rather than routing it to a pure task factory.

Core369 full regressions876pass/19ignored include11 new package/registry methods. Runtime370 has56pass including3 new Manager/preparation methods. Strict library Clippy passes both. Controls370 selected0 because its fault-injection requirement was absent, and gets no credit;371 executes the15 original methods with the required feature and no filtered/ignored methods. All original processes returned. All57 frozen SDK inputs and native helper code remain unchanged. Caches/builds remain D:/build_cache;4 VMs unchanged. System pagefile grew during compilation; no pagefile settings changed, and subsequent builds use1job. Cleanup total remains75422143431 logical bytes, not current free space.

Next: independent original live upload binding, dedicated Runner/import and original-worker durable dispatch, protected Session/product approval/recovery, actual Rust/C/C++ upload guests, then directory watch/rename and remaining Windows P2/P3/SDK gates. Current declaration/approval does not establish runtime upload or release acceptance. Historical Unknown owners untouched; no commit/push/tag/release.


## Active Windows continuation: upload import/live approval381 and disk reclaim379

Input 9dc4d0bf826191c599e7b3837c951917b60a308718e5fc25f1a14ae9ddbec3df now has the dedicated exact-three-import upload Runner and one-shot original Manager/instance/Store/object approval. Generic callbacks remain denied. Charges do not reset on drop; expiry, revocation and observed Store substitution deny continuation. The empty-object digest validation gap found by377 was fixed before grant issuance. Failed373 compile coverage,374 fixture ABI/count assumptions and377 invalid-empty-object test remain preserved.380 verifies five private continuation methods (121 unrelated filtered),83 runtime integration methods, strict library Clippy and compiler receipts requiring the modified runtime and every selected program to rebuild. Its unchanged dependency cache was reused after preserving previous failed executables; this is not cold-target evidence.375 verifies86 core methods with two ignored child-harness entries and strict Clippy.376 is a cold Workbench library check only. Review382 corrects the expected ignored count in375 from1 to2 using the original successful compiler and test logs, without rerunning tests or altering the failed review record. All current processes returned and57 frozen SDK inputs match.

Cleanup378 stopped before deletion because a copied VM configuration timestamp was compared with its source timestamp; contents matched.379 checks actual copy metadata plus original hashes, all retained originals/state files,34 inventoried VHD parent records and registered VM/snapshot references. It removes only four derived export disks under VM033/034, 196563959808 logical bytes. Original disks, the exact original-state-byte-copy sets, saved memory and all configurations remain. Export markers record that those derived exports are no longer standalone; no restore/replay was authorized. All four registrations/states remain unchanged. Prior build-cache reclamation remains75422143431 logical bytes, recorded separately. Build/temp/cache stay D:/build_cache and builds use one job. C/D free space is a fresh nonisolated sample, not a promised isolated reclamation amount.

Next: Implement original-owner guest upload dispatch with final Store guard, original cancellation/deadline and bounded native verification work; qualify protected approval/recovery and real Rust/C/C++ upload guests, then watch/rename. The dispatcher must preserve original deadline/cancellation and charge bounded Store verification work; current approval/import checks do not prove commit-time liveness or actual guest persistence. Full P2/P3/SDK/source/release remain open. No commit, push, tag or release in this continuation.


## Active Windows continuation391: original upload worker and commit guard

Runtime388 source 6747f69d2a42471e5ffec143ff38452a4ed87125ca9407bd16bfd9261795ba02 now executes one original
approved upload task on its original complete owner. The independently named
upload task handler fixes the incorrectly selected pure-handler route from387;
pure transform dispatch still rejects uploads. Store guard384/386 previously
qualified882 full-core methods (19 ignored) on its own exact source.388 qualifies
96 selected runtime methods including13 actual WAT guest/Store methods,5 private
continuations (121 filtered),12 core handler methods, and strict core/runtime
Clippy. Empty and150017-byte objects, duplicate chunks, missing End, nonce and
nomination failures, postcommit guest faults, release history, cancellation,
original-owner cleanup and Store replacement are checked. These are fresh test
fixtures, never historical Unknown owners.

The one original End request closes the bounded upload stream and stays suspended
until Store commit. Later Push cannot restart it. Native work has a separate
nonrefundable128 MiB cumulative ceiling. Cancelled/revoked unread success becomes
an error while original committed history remains. Owner and failed-disconnect
instance return through the actual original thread join; Drop proves no closure.

Host390 source be737588619b052c2fa00dc4a839bf5a5de7097150ab4dfe1470828efb653403 is a Windows production library compile
only. It differs from388 only by the reviewed Workbench lock edge/local package;
the previous full checkout digest was reproduced by substituting only the old
lock bytes in the hash calculation without editing the checkout. Metadata389
failed offline while resolving other-platform packages; its failure is retained.
No dependency versions changed. All build targets and temporary paths stay on D.
All57 frozen SDK inputs match. All recent processes returned; old Unknown owners
were not contacted. Four VM registrations and states remain unchanged.

Cleanup379 remains196563959808 logical bytes from four redundant exported disks,
with original disks and exact original-state backups preserved. Earlier cache
cleanup remains75422143431 logical bytes, not an additional disk-space claim.
Current free-space samples and public report links were refreshed.

Next: Build and run independent Rust/C/C++ upload SDK guests including16 MiB; integrate protected product owner and approval/recovery UI, then finish watch/rename. Remaining P2 faults must retain exact input
lineage or receive independent fresh validation. Full Windows P2/P3, SDK26/G04/
SDK_FREEZE, source and release remain open. This continuation does not commit,
push, tag, publish or release.


## Actual Rust/C/C++ upload guests403

The three independently compiled Wasm guests now use only the task read/complete
and fixed upload call imports. C and C++ share one Rust codec archive/allocator;
C++ exercises the move-only client and response wrappers. Inputs and replies use
existing Cap'n Proto messages. No WASI or trusted Store code is linked into guests.
The bounded example producer generates byte42 chunks; it is not arbitrary file IO.

Qualification395 ran empty,150017-byte and16 MiB objects for each language through
the actual original UploadWorker and Store. All nine successful cases verify exact
bytes/SHA256, observe the known existing stage without re-uploading, release under
new explicit original-owner approval, join all27 workers and reopen durable history.
16 MiB uses274 chunks,278 host calls,17076048 wire bytes and84525056 native work bytes.

Real SHA256 and codec work exceeded the legacy100 million fuel ceiling. The isolated
upload profile now permits an explicitly declared and host-approved cumulative
ceiling of8 billion. Ordinary profiles retain100 million; defaults retain20 million.
Package/host limits intersect and no resume refills fuel. Actual16 MiB used about
6.03 billion Rust and6.63 billion C/C++ units. Absolute approval deadlines, revocation,
memory/call/wire/native-work ceilings remain independent. Pure computation is still
bounded by interpreter fuel, not an immediate cancellation interrupt.

The low-budget negative case correctly stopped with Limits,1323 remaining units,
six calls and no staged record.395's arbitrary test-only assertion that the remainder
must be below1000 was wrong: the interpreter charges blocks.396 changed only that
assertion, reproduced the entire395 source digest with the old test bytes, and reran
only the failed method. Its actual closure/rollback passed; the nine prior successes
were not replayed.396 then caught a new profile-test compile error: Package has no
Clone implementation.398 corrected only that test to decode the original archive,
then its broader test Clippy found an existing complex function-pointer type.
400 names that test type, proves the full-source delta and runs the remaining
regressions. Its checks all pass, but its wrapper incorrectly expects121 filtered
private methods instead of123 with fault-injection enabled.402 reviews the logs
without replaying passed tests and compiles the Windows production library.
All failures392/394/395/396/398/400 remain recorded; production code did not change
after395.

Core package regressions52, related runtime methods97 and private continuations5
pass. The123 unrelated private methods and9 success methods filtered by396 are not
additional tests. Strict Clippy and the Windows production library check pass;
the latter is compilation only. All57 frozen inputs remain unchanged. Targets and
temporary files stay under D:/build_cache; no VM or host-policy actions occurred.

Protected product owner, approval/recovery UI, watch/rename, remaining Windows P2/P3,
full SDK26/G04/SDK_FREEZE, source and release acceptance remain open.

Sources: successful guest execution ab4422db67fe9703479917beecb7aa580322b7b1488c075b814dd952f87f2381; corrected refusal a0b4bd000b79ab177a0760da3be994a20fb420b64cdb5e7c8483f8b1e3824d1b; regressions/host check 2c95c6b1d3802d7d017aac1e2497a2885525813afc8cd3773600783163d040e6. No commit, push, tag or release.


## Original protected upload owner407

The native Workbench now transfers its complete original protected Session,
Manager and Store into one upload worker. The independent upload connection
requires the exact enabled package approval and its own explicit budget; ordinary
Manager connections retain their previous limits. A retained restriction-only
control also vetoes unread successful results after the original join. Cancellation,
revocation and controller loss cannot renew an approval or construct another owner.

Qualification406 passes99 related runtime methods and10 actual protected Windows
methods. Six positive cases execute the pinned Rust/C/C++ Wasm guests with150017
bytes and16 MiB, verify exact bytes and SHA256, check the original exclusive lease,
seal audit history, reopen the protected Session, release, and reopen released
history. Eleven fresh synthetic libraries exercise17 original worker joins in total.
Negative cases cover approval/task/submission refusal, low-fuel rollback, unread
cancel/revocation, explicit original-history confirmation, and lost controller.
Generic cleanup cannot erase an unresolved upload result. Confirmation is a separate
observation and never rewrites the original Unknown. No historical Unknown owner
was contacted or replayed. Runtime strict library Clippy and host library check pass.
Qualification408 also compiles the modified state-slot unit fixtures and passes
the six original-owner absence controls and four synthetic stop/join controls;
all unrelated and legacy privileged tests are filtered, never executed.

This is the trusted native API, not the rendered approval/recovery UI or formal
Cap'n Proto product route. The known-success close/reopen flow is not cross-restart
Unknown recovery. Payloads are the bounded SDK example producer, not arbitrary file
or network uploads. Full Windows P2/P3, watch/rename, SDK26/G04/SDK_FREEZE, final source
and release gates remain OPEN. All57 frozen inputs are unchanged. Cargo.lock only
adds two existing direct path-dependency edges; no third-party version changed.

Qualification source: 7370d500e3ec2c5b84421994f9868d3326512dd3ba1ce4dba1bb16be46445fb3. [Evidence](evidence/resume-20261027/sdk407-protected-upload-owner.json).


## Private upload route and retained session412

The native Workbench upload owner is now reachable through six appended private
Cap'n Proto actions (162-167). Package category approval remains separate from a
single exact operation approval. Native metadata binds package/revision, handler,
bounded input, object identities, whole-object digest, length and deadline. The
original submission is exposed until acknowledgement so a missing START reply can
be correlated without starting another worker. Catalog metadata declares the
independent upload profile; ordinary pure-transform UI excludes these handlers.

Qualification410 passes31 Dart client/session tests and static analysis of the
five affected application files. Qualification411 passes34 adjacent client tests
with2 existing skips, and2 actual protected Windows tests through the private
binary dispatcher, with10 older protected tests deliberately filtered. The two
fresh synthetic libraries execute the pinned Rust Wasm producer for150017 bytes.
They prove exact submission recovery, wrong-key/unread/repeated-read/replay refusal,
unread cancellation veto, separate original-history observation, explicit matching
confirmation, two original worker joins, sealing and reopened stored bytes/SHA.
The Dart retained session blocks automatic replay, retains lost delivery, suppresses
results to a detached view, and does not let generic repair erase Unknown.

All57 frozen inputs remain unchanged. Existing private action ordinals are retained;
only hostDigest changes among the ten generated schema identities. Source410 and
source411 differ only in restored original LF bytes in two Rust files, independently
reconstructed from source digests. No new VM acceptance or privileged host test ran.

This is private route and retained in-process session evidence. The Dart client and
native dispatcher have not yet run together across the process boundary for this
upload flow. Rendered operation approval/recovery, cross-restart Unknown recovery,
arbitrary file/network uploads, watch/rename, full Windows P2/P3, SDK26/G04/SDK_FREEZE,
final source and release gates remain OPEN. Prior Unknown results remain unchanged.

Qualification source: 6d182b995341074658c2b5d1e5d0a7a8029b1263e2dc6c21678fe158c148290c. [Evidence](evidence/resume-20261027/sdk412-upload-private-capnp-session.json).


## Actual Dart/native upload product route417

Qualification416 passes six actual production Dart-to-supervised-native tests:
Rust, C and C++ Wasm each stage150017 bytes and16 MiB, close the original owner,
verify exact bytes and SHA through an independent protected-library reader, reopen
the known-success library, explicitly release the same object, and verify durable
release history. There are12 original worker joins,12 supervised owner closures
and12 independent history observations. Supervision's business_outcome remains
Unknown; only the separate original upload API and protected history establish
these business results. No historical Unknown owner was reopened or contacted.

Five upload control actions now bypass pending service business and reach the
original outer scheduler. Upload start's temporary request segments and serialized
frame are cleared on send success or failure. The32 relevant routing/buffer tests
pass (22 in414,10 in416), with static analysis clean. Failed mock prechecks414/415
remain preserved: the controlled fixture was updated to the existing versioned
page/query API. The native and builtin builds from414 are independently bound to
416 by reconstructing the source digest; only this test fixture changed.

All57 frozen SDK inputs remain unchanged. These are synthetic producer objects,
not rendered operation approval, arbitrary file/network uploads, or cross-restart
Unknown recovery. Those gates, directory watch/rename, full Windows P2/P3,
SDK26/G04/SDK_FREEZE, final source and release remain OPEN.

Qualification source: 4cdb680ae2dfd8b3bea5f6a798c2188d7f6bec7122077400a62e76f80ef4e52c. [Evidence](evidence/resume-20261027/sdk417-upload-dart-native-product.json).


## Bounded upload proposal and pure planner423 — 2026-10-28

An independent Cap'n Proto proposal carries bounded untrusted display metadata,
length, SHA and producer input. It carries no Store, grant or object identity.
The later bound-input codec checks the exact proposal digest against an unchanged
Descriptor frame; this is correlation, never approval. Protobuf Manifest field23
registers disjoint same-package proposal-to-upload handlers under an explicit
required feature. Pure and upload execution routes remain separate.

The new planning interpreter holds no Store, Manager, connection or effect
callback. Any attempted upload import aborts, including attempts to hide it behind
a completion. Original task correlation, declared output/object bounds, fixed fuel,
cancellation and structured plugin failures are checked. Codec419 passes15 tests;
core421 passes31 package tests; runtime421/422 passes58 selected methods including
10 new synthetic WAT planner cases. The initial421 fixture failure is preserved;
only its task read capacity changed to the existing128 KiB contract, with an added
assertion proving actual upload-import admission before rejection. Core and runtime
strict Clippy pass. Default host library/binaries/tests compile with existing
warnings retained; this is not current-source product or VM acceptance.

All57 frozen inputs remain unchanged. Only one new local dependency edge was kept;
unrelated dependency edits produced by a Cargo update were preserved privately and
not applied. C/C++ proposal helpers, real three-language planner guests, private
planning route, trusted rendered approval/recovery and durable Unknown recovery
remain open. Watch/rename, full Windows P2/P3, full SDK and release remain OPEN.

Qualification source: d06aa860ffe93c9403f462e50a808556b090129d53c18329c9781e896f02598a. [Evidence](evidence/resume-20261028/sdk423-upload-proposal-and-pure-planner.json).


## Three-language proposal helpers and real planners431 — 2026-10-28

Typed C owners and C++ move-only wrappers now share the unchanged bounded Cap'n
Proto proposal codec. Twenty-three Rust tests and four actual MSVC/Clang C/C++
consumer programs passed. The sample-only Cap'n Proto producer options passed
three tests and strict checks; they carry length/fill configuration, no authority.

Fresh Rust/C/C++ guest builds were verified against16803 pinned toolchain/sysroot
files. Each has only the existing three fixed task/upload imports and no start
section. Eleven real planner tests passed: three languages at0,150017 and16777216
bytes plus two refusal cases. Planning runs before any Store/Manager exists and
makes zero upload calls. The three languages return identical proposal hashes at
each size. Eighteen original upload/release workers joined; stored bytes and SHA
match, and the successful release history survives a closed-owner reopen. Ten
legacy Descriptor-input cases also passed, with28 original worker joins. These
ordinary Windows test owners use explicit test approvals, not rendered approval.

Failed checks425/429 are retained. Their fixes only replace an unnecessary test
Vec and correct borrowing of the original non-Clone package. Guest and production
inputs were proven unchanged after the second correction. All57 frozen inputs
remain unchanged. No VM mutation or historical Unknown owner contact occurred.

Cleanup427 removed824 closed compiler intermediates,995108453 logical bytes;
observed D free-space gain was996626432 bytes. All3664 protected hashes passed;
executables, libraries, sources, evidence and VM disks were retained. The plan
preview omitted its numeric sum; the immutable manifest and executed totals were
independently reconciled. Build caches remain under D:\build_cache; four VMs remain.

Next: background product planner tied to the original package/revision/input,
trusted rendered operation approval/recovery and durable Unknown recovery. Watch/
rename, full Windows P2/P3, full SDK and final source/release gates remain OPEN.

Qualification source: 68fd7dc3613307633d12f5f4cef7a3c04f3666ac407258375c1a4f5afee84e7b. [Evidence](evidence/resume-20261028/sdk431-three-language-proposal-planners.json).


## Original native product planner436 — 2026-10-28

Workbench now owns a separate bounded planning slot. Package preparation and pure
guest execution run on its original thread with no Store, Manager or effect grant.
The native product retains the exact proposal, original task hash, package digest,
registry revision and control gate. It returns only bounded review metadata.
After explicit trusted review, approval rechecks the original selection and upload
category permission, generates fresh identities in the host, and starts the existing
original protected upload owner with BoundInput built from the retained proposal.
The one-shot decision is consumed before admission; a missing reply discovers the
same original task, and even failed admission cannot silently submit again.

Fifteen product methods passed, including real Rust/C/C++ guests at150017 bytes and
16MiB. All six original planning threads and six original protected upload workers
joined. Closed-library reads confirm exact bytes and SHA. Other cases cover no
category grant from planning, wrong keys/hashes, unreviewed/duplicate decisions,
late cancel, expiry, revision drift, replaced/lost controller, spawn failure,
malformed options and failed admission. Four original gate controls also passed.
An additional focused run435 verifies actual Workbench finish returns pending while
the original planner is held, retains the protected lease, and succeeds only after
the original join. Production code is byte-identical between434 and435.

New planner Clippy diagnostics are empty; existing host warnings are retained.
Default host library/binaries/tests compile. Initial build433 failed only because a
new test used a nonexistent catalog accessor; the corrected runtime results are
separate. No broad host legacy execution, VM mutation or Unknown owner contact was
performed. All57 frozen inputs remain unchanged; dependencies added only local
proposal/sample edges. This is native product API evidence, not private IPC or a
rendered approval. Next: Cap'n Proto planning controls, retained Dart session and
formal approval/recovery UI, followed by durable Unknown recovery. Remaining P2/P3,
watch/rename, full SDK and final source/release gates stay OPEN.

Qualification source: 6c37c46fa1c2ab552872b2ea12d69bb3f804288096a0731a1e649b01c92c3a22. [Evidence](evidence/resume-20261028/sdk436-original-product-planner.json).


## Private planning controls442 — 2026-10-28

The original planning slot now has seven private Cap'n Proto controls and typed
catalog mappings. All168 prior actions and570 prior fields remain unchanged.
The native plan retains its own generated upload submission/key, even after the
upload is acknowledged. Discovery cannot replace it with whichever IO is active.

The retained Dart session freezes the original input, checks its SHA together with
package/revision/handler, and consumes the local approval attempt before dispatch.
Missing start/read/approval/acknowledgement replies never start another operation.
An existing upload is observed by original identity, without recreating its input.
View detachment cannot publish an unread proposal; if an approval reply is lost,
the session discovers and cancels only its original upload. Planning request
copies are cleared, and scheduler controls stay outside pending service business.

68 Dart client/session checks and34 Flutter buffer/routing checks pass.21 native
methods pass:15 original product cases and6 private-control cases. Actual Rust,
C and C++ guests each cross the new binary controls with150017 bytes and recover
the original upload after a deliberately discarded approval reply. The product
cases also cover16MiB. Nine positive planning joins and nine upload joins are
observed; closed protected libraries contain exact bytes and SHA. New module
Clippy diagnostics are empty; existing unrelated host warnings are retained.
Default host library, binaries and tests compile;57 frozen inputs are unchanged.

The initial439 run is preserved:18 native methods passed and3 failed before
upload approval because native hashed the whole Invocation while Dart checks
input bytes. Correction440 makes the native view hash the exact Transform input;
441 uses fresh stores and re-runs the full selected set. No old store is replayed.

Cleanup437 removed1,288 old compiler leaves:873,228,962 logical bytes and an
observed875,315,200-byte free-space increase.6,378 retained files were hash checked.
One missing cache-tag check stopped before deletion; the original Cargo artifact
receipt and unchanged compiler fingerprint then established that exact cache root.
Sources, executables, reports, VM disks and historical Unknown owners are retained.

This validates native binary controls and Dart sessions separately. A combined
Dart/native planning product run and rendered approval/recovery are next, followed
by durable Unknown recovery and watch/rename. Full Windows P2/P3, SDK26/G04/
SDK_FREEZE, final source and release remain OPEN. There was no GitHub publication.

Qualification source: 25c18413bd5b7e4df377cec8ca38c4247f48c704c75dc704f43904f36b0eaf82. [Evidence](evidence/resume-20261028/sdk442-private-planning-controls.json).


## Retained planning product446 — 2026-10-28

Qualification444 now connects the actual retained Dart planning/upload sessions
to the production native host and supervisor through the seven private Cap'n Proto
controls. Seven fresh protected libraries pass: Rust, C and C++ each upload150017
bytes and16MiB after planning, and a lost proposal read creates no stored object.
All six positive cases discard the real start and approval responses at the trusted
application edge, then discover the original tasks without submitting again. The
three16MiB cases also discard the result response: original history is inspected and
explicitly confirmed before acknowledgement, without synthesizing a missing result.
This models application-edge reply loss; it is not OS transport-failure coverage.

Seven planning and six upload workers join. Thirteen native owners exit normally
and their original supervisors release; thirteen independent closed-library reads
verify exact bytes/SHA, object counts and no pending bytes. Positive libraries are
reopened only after the business result, original joins, native closure and first
independent read. Raw supervisor business_outcome=Unknown is retained separately;
it is not rewritten from the observed business data. Category approval is provided
through the test API, so this is not a rendered user-approval qualification.

Production source is unchanged from442. Three qualification-only files package
the current guests, inspect known closed libraries and exercise the actual product.
Follow-up445 adds braces to three test statements; focused analysis is clean and
the exact before/after/source reconstruction proves no production behavior change.
57 frozen inputs remain unchanged; all recent processes closed. Four VMs remain
registered, caches remain on D, and no VM or host policy was changed.

Rendered approval/recovery, durable Unknown recovery, watch/rename, full Windows
P2/P3, SDK26/G04/SDK_FREEZE and final source/release remain OPEN. No remote publish.

[Evidence](evidence/resume-20261028/sdk446-retained-planning-product.json).


## Planning approval/recovery UI457 — 2026-10-28

The plugin list now exposes category approval, a save action and a retained
original-task review. The trusted dialog freezes bounded UTF8 text or an explicitly
selected binary input file, requests a pure proposal, displays its untrusted plain
description/length/current-library destination/fingerprint, and requires a separate
unchecked confirmation before one native approval. It never constructs upload
identities from plugin text. Missing responses retain the original task; lost
results require original-history observation and explicit confirmation.

The upload session now consumes acknowledgement before dispatch. Missing finish
responses stay unresolved across view replacement and cannot cause a second
acknowledgement or a new upload. The retained plan owns cancellation; closing a
view cannot cancel through two independent owners. Plugin-library replacement
removes its exact original dialog. A dialog is not a pending background request,
so its parent no longer shows a perpetual loading animation.

Qualification454 passes70 pure Dart checks and70 Flutter widget/buffer/routing
checks, including13 new UI methods. Earlier448/450/452 failures are retained:
an unavailable test-only dependency was replaced with a Flutter fixture; the
background animation was corrected; file IO was awaited on the real event loop;
and an XFile test fixture now supplies its Windows path instead of ignored name.

Qualification456 then drives these production widgets against the actual pinned
native host/supervisor in six fresh protected libraries. Rust/C/C++ each save
150017 bytes and16MiB. Import, enablement, category permission, proposal approval,
result review and finishing all pass through UI controls. Real start/approval
responses are deliberately discarded at the application edge; all three16MiB
cases also lose the result reply. Each case submits and approves once; no raw
upload start or missing-result reconstruction occurs. Six planner and six upload
joins,12 original supervised closures and12 independent closed-library byte/SHA
checks pass, including known-closed reopening. Raw supervision Unknown is retained.

This is headless Flutter production-widget execution with a deterministic file
selection adapter. The captured test frames use placeholder glyphs and do not
establish readable desktop appearance. Native Windows window/picker acceptance,
ordinary sample parameter input, durable Unknown recovery, watch/rename, final
P2/P3, SDK26/G04/SDK_FREEZE, final source and release remain OPEN.57 frozen inputs
are unchanged. Four VMs remain; build caches still resolve to D. No remote publish.

[Evidence](evidence/resume-20261028/sdk457-native-planning-widget-flow.json).


## Windows text save and pause467 — 2026-10-28

At the user's request, further qualification is paused and completed source,
evidence and the HTML report are being checkpointed to the development branch.
No new VM or acceptance case is started after this pause request.

The three SDK examples now support bounded UTF8 text through existing CapnProto
planning and bound-upload inputs. Five sample tests, 21 actual guest-planner
methods and ten legacy descriptor methods pass. Invalid UTF8 and over-limit
input are refused; exact empty/unicode/CRLF/NUL/32768-byte content is covered.
Six actual protected native widget cases cover Rust/C/C++ at 41 and 32768 bytes,
with one planning and upload join each, 12 normal supervised closures and 12
independent closed-library content reads. Deliberately discarded replies resolve
only the original operation. These six cases use headless qualification adapters.

The separate desktop464 case uses the current unmodified production main, an
actual Windows plugin picker and a fresh protected library. The user pasted two
lines containing Chinese text, an emoji, leading/trailing spaces and a newline.
The agent then reviewed the 41-byte proposal and matching SHA, approved once,
read the saved result, checked original history, and finished once. The UI showed
one completed record. Both initial and reopened UI/supervisor/host process sets
exited normally (six original identities, all exit0). Each independent closed
read found exactly one byte-for-byte matching object and no pending write.
Reopening preserved plugin enablement and category approval and showed no pending
proposal. This desktop text case is Rust only; it does not prove Unknown recovery.

Source binding: 269f9b7420cab8a073e1cd3eb5fd2de75fd75dd4b8b906ba21401d333725c78a.
57 frozen inputs remain unchanged. Full-host strict Clippy still reports 60
preexisting errors; default Clippy completed and the two new examples had no
diagnostics. The successful external text-widget adapter is retained as evidence;
promotion into the canonical test suite remains pending. Build caches stay on D,
four registered VMs remain, and no historical Unknown owner was contacted.

Remaining: durable cross-restart Unknown recovery, watch/rename, network/account/
TLS and full recovery matrix, final Windows P2/P3 source binding, full SDK26/G04/
SDK_FREEZE, strict-host lint cleanup, final source acceptance and release.
source_accepted=false; release_eligible=false. Other platforms remain deferred.

[Evidence](evidence/resume-20261028/sdk467-text-desktop-and-three-language.json).


## Windows resume482 — 2026-10-29

Work resumed at the user's request. Six actual native text test cases were
promoted into test/blob_text_manager_native_test.dart and passed for Rust/C/C++
at 41 and 32768 bytes, with 12 verified supervised closures and 12 independent
closed-library reads. The old generic environment namespace does not enable
these cases. This is headless native evidence at source63eaf4d5, not new desktop
acceptance. The prior desktop464/467 evidence retains its own source binding.

The harness lock was missing three existing local SDK modules. The reviewed
lock repair changed no existing package version, checksum or upstream commit;
locked offline metadata passed. All four P/C/R/S programs were then compiled
into fresh exclusive D targets with every compiler artifact marked fresh=false.
The canonical source and permitted role overlays were checked before dispatch.

At source36802baf, VM045 case480 observed real LogonUser error1326, NULL output,
no Runner and complete original refusal retirement, setup cleanup and same-owner
policy restoration (355 checks). A separate fresh case481 passed the original
47 steps and all nine requirement groups (285 checks). Both original processes
returned zero and all their sessions closed. The VM was saved, and four VMs
remain registered. A file-only review initially lacked its Saved prerequisite;
that prerequisite was supplied before repeating only the file review. Neither
runtime case was replayed. Historical Unknown owners were not contacted.

The guarded cleanup removed 2884 closed compiler intermediates, 5205802158
logical bytes; observed free-space delta was 5211934720 bytes in a nonisolated
sample. Sources, reports, executables, DLLs, PDBs and VM disks were preserved.
All caches and temporary build outputs remain under D:/build_cache.

Durable cross-restart upload task history remains unimplemented: current task
identity/results are in memory, while blob receipts are durable. Read-only
historical review must not manufacture an old task, Store binding or worker,
rewrite Unknown, release live capacity, or grant a replacement upload. Remaining
Windows P2/P3, watch/rename, network/account/TLS, full SDK26/G04/SDK_FREEZE and
strict-host lint gates remain open. 57 frozen public inputs are unchanged.
source_accepted=false; release_eligible=false. No commit, push, tag or release
was made during this resume. The earlier development snapshot remains separate.

[Evidence](evidence/resume-20261029/windows-resume482.json).


## Windows durable upload history — 2026-10-29 / checkpoint534

Core schema27 now retains the original request binding, joined native result and
original acknowledgement across reopening. The 1024-record bound reserves space
before dispatch and does not evict acknowledged history. Private list/inspect
actions175/176 use Capn Proto; the at-rest journal follows Core's existing
protobuf convention. History reads do not reconstruct an old TaskKey, reclaim a
retained worker, submit new work, acknowledge a past task or rewrite Unknown.

At qualification source8a31564a, fresh cases525/532 passed six Rust/C/C++ upload
and reopen cases plus two real abrupt-commit widget cases. Fourteen supervised
closures and fourteen independent closed-library byte checks were verified.
Case533 then passed both crash boundaries in the full Windows main window:
before commit shows absent content; after commit shows saved content; both keep
the missing original result uncertain and unacknowledged. There is no pending
upload. All six original UI/supervisor/host processes returned zero, followed by
two independent closed-library reads. The launcher inputs are byte-identical;
the current Dart main was rebuilt. Default native programs retain their exact
ddb675 build source with an exact four-Dart-file equivalence proof, not a new
cold-build claim. No historical Unknown owner was contacted.

Selected native case531 passed15 tests and six separately selected metadata
fault points. Case524 actually recompiled Core, Host and the selected test;
its validator mistakenly filtered only lib and missed Core rlib/cdylib. Cases509
and524 remain failed validations; case531 independently checks the real records
before the first runtime use on fresh stores. This is a warm-dependency build.
The 510/521 widget timeouts remain retained with unobserved reopened-owner
closure. The fresh repair waits for dialog reads, drains the original close
while pumping test callbacks, and keeps history off the service business route.
Fifteen routing/widget tests and fatal-info analysis passed. Core migration487
retains its own4ef2178a source and352 passed/13 ignored helper entry points.

Guarded cache495/530 removed8598390576 and2060587775 logical bytes respectively,
preserving programs, reports, disks and Unknown evidence. All caches remain on D.
There are four registered VMs; VM045 stays Saved after the source36802 P2 runs.
The prior logon/normal47 evidence is not rebound to the new SDK source.

Still open: explicit completion of precommit metadata debt, remaining P2 fault
retirement/final source binding, final P3 binding, directory watch/rename and
conditional ReplaceUnsupported, network/account/TLS, complete SDK26/G04/
SDK_FREEZE, host strict lint (59 errors in the504 configuration), and release
qualification.57 public frozen inputs are unchanged. source_accepted=false;
release_eligible=false. Work remains active; no commit or push was performed.

[Evidence](evidence/resume-20261029-history/history-resume534.json).


## Original upload metadata completion — 2026-10-29 / checkpoint548

The retained live owner can now review and explicitly complete its original
result record or interrupted acknowledgement. Private Cap'n Proto actions177/178
carry a single-use review; completion replies echo that original permit. The
transaction compares the exact reviewed predecessor before appending the original
result/timestamp. Reconciliation or a new acknowledgement invalidates an earlier
review. No worker, upload, approval, task identity or timestamp is reconstructed
from saved history, and an original Unknown remains Unknown.

Qualification source bedc9de25d8b80ff5b7cbbf30223f0e0b604c05ae2aa4efe897d25375bbb95ed: case543 passed15 protected tests, six existing
metadata fault points and12 completion/owner/revocation/reconciliation boundaries.
Case542 passed66 codec/session tests and31 component/routing tests. Case540 passed
95 Core tests (three explicit helper entry points ignored) and strict lint;
case541 verifies its249 Core input files and full reverse-source equivalence.
Current Host and selected native test actually compiled; exact prior Core output
pins remain bound. Case545 rebuilt the default native Core/Host with no fault
features or fault markers. Dependencies and unchanged Wasm outputs were warm;
these are not cold builds.

Case546 passed six current-default Rust/C/C++ upload/reopen cases and two real
abrupt-commit history cases with production widgets. All14 supervised closures
and14 independent closed-library reads passed. These validate ordinary upload
and retained history on this source. They do not yet establish full Windows
window acceptance of the new explicit metadata completion controls. Prior533
full Windows history-window evidence stays bound to source8a31564a.

The initial536 test-fixture failures remain recorded: generated builder setters
were mistakenly read and test helpers mixed Dart URI identities. Case537 fixes
only those test fixtures. No historical Unknown owner was contacted. All build
and temporary outputs remain under D:\build_cache; four VM registrations are
unchanged. No commit, push, tag or release was made.

Still open: formal Windows completion-fault UI/close evidence, remaining P2/P3
and final source binding, directory watch/rename, conditional ReplaceUnsupported,
network/account/TLS, async/long-watch/product scope, SDK26/G04/SDK_FREEZE, host
strict lint and release qualification. source_accepted=false;
release_eligible=false. The complete objective remains active.

[Evidence](evidence/resume-20261029-metadata/metadata-resume548.json).


## Full Windows original metadata completion — 2026-10-29 / checkpoint559

Four defined fault points now pass in the full canonical Windows application:
result-record write before/after commit, and acknowledgement write before/after
commit. Each window starts one original upload, displays the actual retained
result, reviews whether a write is required, completes only that original record,
and shows one acknowledged saved history entry. Case554's observe-before window
and case558's three independent windows provide 12 original UI/supervisor/host
normal exits and four independent closed-library exact-byte reads. These are
four scoped cases, not full SDK or release acceptance.

Source 3089c4a6758ce1ee40fb79184695d33e53a668e48eb4d514b124e30d4759f848 is bound through exact test-only inverse-source comparisons555
and552 to native artifacts552 and the canonical main kernel551. The existing
native launcher inputs remain unchanged; the launcher was not rebuilt. The four
fixed fault hosts are compile-time qualification variants, absent by default.
Four point selectors and six invalid feature-pair compile rejections passed;
the normal default artifact has no qualification marker. The production
supervisor environment policy and internal Cap'n Proto IPC remain unchanged.
Dependencies were warm; no cold-build claim is made.

Case556 separately passed all four real supervised native/production-widget
flows, preserving original result and acknowledgement timestamps, revision and
single-use permit identity, with four verified closures and four independent
reads. Those counts are separate from the full-window evidence.

Failures remain visible: case550's receipt parser rejected the Core target-kind
shape after a successful original compiler exit; case552 validated those exact
raw compiler artifacts without replaying a runtime. Case553's test harness
missed an animating control before starting an upload; its owner remains Unknown
and untouched. Test-only timing/focus repair555 was then validated in fresh556.
Case554's second proposal stopped after delayed approval beyond the configured
120-second lifetime; its record and application were closed normally, but it did
not exercise metadata completion and is NOT passed. The batch failure remains
retained, with no retained original process handles in its failure record; its
last two windows were never started. Case558 used three new libraries/owners.

Whole-Host strict lint557 still fails with59 errors and35 dependency warnings.
It reports no diagnostics in the newly added upload modules; this does not make
the whole Host clean. The57 frozen public inputs and four VM registrations are
unchanged. Caches and temporary outputs remain in D:\build_cache. No commit,
push, tag or release occurred.

Still open: remaining Windows P2 retirement and final P2/P3 source binding,
directory watch/rename, conditional ReplaceUnsupported, network/account/TLS,
async/long-watch/product scope, SDK26/G04/SDK_FREEZE, strict Host lint and final
release qualification. source_accepted=false; release_eligible=false. Goal active.

[Evidence](evidence/resume-20261029-metadata-desktop/metadata-desktop559.json).


## Host library lint and regression checkpoint — 2026-10-29 / checkpoint571

The59 Host errors from case557 are resolved without new lint suppressions.
Case563 passes the same qualified Host-library strict configuration; case569
also passes the default Host-library configuration. Each retains35 warnings
from the native companion dependency. This is --lib --no-deps -D warnings,
not all-targets or dependency-wide lint acceptance.

Changes make wire-encoder arguments named, retain test-only encoders under
cfg(test), remove unused private forwarding helpers, simplify equivalent
conditions and move oversized owned values behind Box. Original cleanup debt,
stop ordering, result delivery, unknown-state handling and wire schemas remain.
No account, policy, VM or historical Unknown owner was accessed by these checks.

Current source 110bd3d26239eadab2e61e914f3d2c72986c4197c7ef94cd5c335dec7de7966e is byte-equivalent to lint563 production inputs through
the exact ten-test-file inverse binding566. Case564 initially failed to compile
a test adapter after the owned read-model change; no runtime was dispatched.
The adapter fixes preserve its failure receipt. Case567 freshly compiled all11
selected test programs with warm dependencies. Case568 passed150 targeted
tests (88 selected library tests and62 integration tests), with four explicit
fault/helper entries ignored in the ordinary run. Case570 separately passed18
fresh original metadata-fault cases: six journal boundaries, eleven completion
and cancellation/gate/failure cases, and one stale-review invalidation case.
These18 cases are not18 distinct test methods and are not added to a release
score. Negative fixture debt is not proof of normal product retirement.

Every original top-level compiler/test process completed. The57 frozen public
inputs remain byte-identical; registered VMs remain4; build and temporary
outputs remain on D:\build_cache. No commit, push, tag or release occurred.

The four full Windows metadata windows from checkpoint559 retain their original
3089c4a6 source identity. They were not rerun after the Host lint refactor and
are not relabeled as current-source desktop acceptance. Remaining Windows P2
retirement and final P2/P3 binding, watch/rename, conditional ReplaceUnsupported,
network/account/TLS, async/long-watch/product scope and SDK26/G04/SDK_FREEZE
remain OPEN. source_accepted=false; release_eligible=false; goal ACTIVE.

[Evidence](evidence/resume-20261029-host-lint/host-lint571.json).


## Current native refusal and protected directory checkpoint — 2026-10-29 / checkpoint580

Current source 74100e0b1d116072b3852363056be51d701c9f4fc95b0db61bd4ddab958d8b63 passed case576: 11 selected native methods, including
17 actual read-only caller refusals in the same original production registry,
34 checked token closes and seven retained Unknown controls. Case575 compiled
all 622 reported compiler artifacts in its own previously empty D target. No
Runner, account setup, policy change or VM execution occurred. This is local
registry evidence; complete SDK factory capacity and full P2 remain open.

Case577 freshly compiled the current directory test program using warm Host
dependencies. The original Rust/C/C++ directory guest artifacts and their SDK
source inputs match the independent case003 pins. Case579 passed 26 actual
Windows protected-directory methods in separate original processes and new D
stores: 15 lifecycle/three-language cases, six retained-selection cases and five
private Cap'n Proto cases. These cover paging, native ancestor boundaries,
original eight-resource limits, refusal/cancellation, once-only File transfer,
lost start reply observation, raw UTF-16 and independent library reopening.
Formal picker/UI, watch/rename, full G04 and full SDK are not established.

Case578 stopped after discovery because the current binary also includes five
private protocol methods omitted by the initial driver list. Its failure and
the interpretation that zero runtime tests were dispatched are preserved.
Case579 uses a new run identity and explicitly reviews all 26 methods; no old
runtime or Unknown owner was retried. Every original test/compiler process from
these new completed runs returned, and all 57 frozen inputs remain unchanged.

Cache cases573/574 removed 3,799 compiler-marked .rlib/.rmeta files, logical
5,672,152,308 bytes (about 5.28 GiB), with 1,887 protected-file checks. Executables,
DLLs, PDBs, Wasm, sources, logs, reports and VM disks were retained. Observed free
space change is recorded separately and is not an isolated storage benchmark.
Two inventory guards stopped before deletion; later build receipts were pinned
and seven empty metadata files retained. Current Host/507 warm targets stayed.

Comparison with the case481 build snapshot finds 237 changed repository files
and no changed native companion file; Host/Core/UI changes still require full
candidate runtime qualification. The old normal47 and fault results retain
their original source. Four VMs remain registered. The task is ACTIVE, no commit
or push was performed, and source_accepted/release_eligible remain false.

Evidence: [scope and outcome](evidence/resume-20261029-p2-directory/p2-directory580.json),
[native P2](evidence/resume-20261029-p2-directory/native576-refusal-capacity.json),
[directory SDK](evidence/resume-20261029-p2-directory/directory579-current-26-pass.json).


## Paused publication checkpoint, 2026-10-29

Four P2 roles052 cold-built; policy703 passed 18 pure methods and 31 malformed-input checks without LSA calls. The already-running SDK719 build closed all three original compilers. Kernel722 compiled. VM047 is Saved; case703 remains prepared and NOT_RUN. Original47 case638 and formal text/history/reopen678 retain source5fc113; neither qualifies a038. Policy failure684 and earlier Unknown outcomes are preserved. P2, current-source P3, SDK26/G04/FREEZE and release remain open. Publication changes the whole-tree digest: prove equivalence and rebind sealed preparation after explicit resume. [Current handoff](PAUSED_WINDOWS_20261029.md).
