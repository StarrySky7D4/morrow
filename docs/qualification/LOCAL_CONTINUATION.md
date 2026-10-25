# Local continuation, 2026-10-21

Status: **resumed by direct user instruction after report publication 65072026; Windows qualification active; four VMs retained; source and release are not accepted**.
See [the current pause and publication handoff](PAUSED_WINDOWS_20261024.md). Historical results below retain their original source and VM-state boundaries.
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
