# Local continuation, 2026-10-21

Status: **paused by the user for a GitHub development snapshot; source and release are not accepted**.
Working checkout: `C:\ms1020`, branch `codex/local-continue-20261020`, base
`116d1eb9beffa1675a48eb5ab06d1abc79137ae2`. This document accompanies the paused development snapshot. Current acceptance scope is Windows only by direct user instruction; other platforms are deferred and not accepted. The original
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
