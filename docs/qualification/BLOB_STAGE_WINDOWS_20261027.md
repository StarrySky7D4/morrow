# Durable blob staging: Windows local storage scope

The new `Store::stage_blob_operation_local` atomically persists exact bytes,
their retention, and an immutable operation receipt in the original content
database. `inspect_blob_stage_local` reads that original fact.
`release_blob_stage_local` appends a terminal release record in the same
transaction that releases its retention. Reopening the database or collecting
released bytes does not make the old operation reusable.

This is a host-local storage primitive. It does not expose guest upload, grant
file or content access, commit an attachment to a card, or restore a live owner
from history. The caller must retain the original authorization and resolve an
unknown commit by exact inspection. An absent historical record is not a new
authorization. The existing `stage_blob_retained` contract is unchanged.

## Storage and resource boundaries

- An operation is bound to a nonzero 32-byte host-selected domain, operation ID,
  exact byte length, and SHA-256. A mismatched command fails closed. A matching
  already-staged command returns the verified original blob without consuming
  another reader. A released command cannot stage again.
- Objects are bounded to 16 MiB. The existing storage reader uses 64 KiB blocks;
  the existing blob count and total-payload limits remain in force.
- There are at most 1,024 durable operation identities. They are not evicted on
  release. Both possible metadata revisions reserve 16 KiB in the original
  shared logical byte budget before any source bytes are consumed. Release does
  not refund this historical reservation. Physical disk exhaustion is still a
  separate failure.
- Two connections use the original SQLite writer boundary and zero busy
  timeout. A competing writer can receive `StorageBusy` before reading. The
  implementation adds no automatic retry or waiting policy.
- Schema v26 adds two tables without rewriting old blob or business records.
  Stored metadata follows the project's Protobuf and LZ4 storage convention.
  No internal IPC or existing Cap'n Proto wire schema was changed.

## Original worker and streaming receiver

The host-local `plugin_runtime::blob_upload` adapter now consumes the existing
Cap'n Proto blob-transfer-v1 frames in 64 KiB buffers. It validates the original
identity, descriptor, sequence, digest, End and actual source EOF while the
original Store transaction remains open. Duplicate live chunks do not write
their bytes twice. Cancellation, malformed frames, trailing input or missing End
roll back the transaction; failure after commit remains DeliveryUnknown and
requires exact original inspection. Matching durable retries consume no new
source. Chunk acknowledgments alone are not durable staging receipts.

`IoWorker` dispatches stage, inspect and release on its original command queue,
owner, Store and existing cancellation/deadline/revocation controls. The explicit
`BlobUploadOwner` callback must authorize the exact local operation. Merely
knowing an identity or an old receipt does not authorize access. Resource charges
include bounded wire traffic, original payload verification and adapter memory;
they are not refunded by cancellation. The synchronous trusted source must
bound its own state/native work and must not re-enter the same owner.

This opt-in host lane does not expose a guest upload import or enable product
approval. No protected Workbench blob owner or recovery UI is claimed yet.

## Actual current evidence

The qualification input is checkout
`8a5aee593e8988cfcf7ba5877cc494be087c88822d2f4fe04941a161e52473db`.
Report edits after qualification have a separately recorded checkout digest.

- Review357 verifies the original logs and input hashes. Core351 ran 865 methods
  with no failures and 19 ignored methods. The 14 durable staging methods are
  included in 865, not added again.
- Runtime351 ran 149 affected integration methods with no failures. The 17 new
  blob adapter/owner methods are included in 149. Controls/reconciliation352
  ran another 19 methods; private original directory controls354 ran 33.
- Six initially ignored mutation methods are real Wasm SDK scenarios, not
  subprocess helpers. Fresh Rust, C and C++ examples were built in355 and all
  six original ordinary/16 MiB methods passed in356. These use the original
  managed owner, ordinary temporary Store and Manager API approval with actual
  native Windows files. They are mutation evidence, not guest blob-upload or
  production UI approval evidence. The original 30-second binding was unchanged.
- The core and runtime packages strict library Clippy checks passed without
  suppression. Default runtime library check passed. A fresh production
  Workbench library check353 passed; this is compilation, not product execution.
- Zero-selected channel/file-target commands receive no qualification credit.
  Two ignored file-target methods are explicit Windows capability experiments,
  not helpers. Earlier inaccurate helper labels, failed filters and original
  failed lint/review logs are retained and corrected by review357.
- All 57 original frozen SDK inputs remain byte-identical. Fresh355 guests are
  separate outputs. All current qualification processes returned. No VM or host
  account/firewall policy change occurred in this scope. Linux was not run.

The earlier storage345/347 evidence and concurrency346 failed expectation remain
available. Production's original zero busy timeout was not weakened; a competing
writer may be refused before it consumes another reader.

## Remaining SDK work

Public versioned guest upload, explicit product capability/approval, protected
owner integration and recovery UI still require implementation and qualification.
Directory watch and rename remain open. Previous P2 cases329/335 and formal
directory UI results retain their original f849/305/309 bindings and do not
qualify this later implementation. SDK26, G04, final P3, source acceptance and
release eligibility remain open.

Public current result: [original owner and SDK qualification](evidence/resume-20261027/sdk357-streaming-original-owner-and-mutation.json).
Prior result: [initial staging history](evidence/resume-20261027/sdk347-durable-stage-history.json).


## Independent request and native SDK preparation

The new `extensions/blob-upload-request-v1` contract and bounded client have
checked C ABI and move-only C++ interfaces. Qualification368 reconciles19 Rust
methods and strict library Clippy with four actual native consumers (MSVC/Clang,
C/C++). Empty, small and16MiB/274-chunk paths execute the original byte receiver;
Staged responses are explicit fixtures, not Store or product proof. ABI errors
preserve output, and failed or foreign delivery retains the original pending
request and prevents automatic replay. Initial tool setup failures are retained.
All57 frozen SDK inputs and the previous351 runtime inputs remain unchanged.
That native-helper qualification covered an unregistered request profile. Later
package approval and import qualification follow below; the protected product
route and remaining SDK gates stay open.

Result: [native upload helpers](evidence/resume-20261027/sdk368-upload-native-helpers.json).


## Independent package and Manager ceiling

Core now validates an isolated upload declaration in persistent Manifest field22.
Registry v3 and Manager provide a separate exact-package approval ceiling, which
is never inherited by a different package digest or inferred from content/IO.
Old v1/v2 cannot carry the new approval; revocation preserves the v3 format.
Unknown publication blocks further decisions. No live owner or object grant is
restored. At qualification372 runtime preparation rejected this new profile. The later
dedicated Runner and live approval evidence is recorded below; durable guest
dispatch remains open.

Review372 verifies core876 passing/19 ignored (11 new methods included), runtime56
passing (3 new methods included), strict library Clippy for both, and15 original
control methods with their required configuration. The earlier zero-selected
target gets no credit. All57 frozen inputs and the qualified native helper code
remain unchanged. This is package/Manager evidence, not approval UI, live upload,
protected Session or release proof.

Result: [upload package and approval](evidence/resume-20261027/sdk372-upload-package-approval.json).


## Dedicated import and original live approval381

The dedicated Runner now accepts exactly the fixed upload and task imports;
ordinary callbacks and legacy factories cannot dispatch this profile. Approval
binds the original Manager, instance, Store and object once, retains cumulative
charges and fails closed after revocation, expiry or observed Store substitution.
An invalid empty-object digest is rejected before claiming the instance.

Qualification381 reconciles380 (five private continuation methods and83 runtime
methods),375 (86 core methods, two ignored child-harness entries) and376 (cold production library check,
not execution). Strict library Clippy passed. The121 unrelated private methods
filtered by380 are not credited. Compiler receipts prove changed runtime and
selected test programs rebuilt; failed377 programs were retained before cache
reuse. Failures373/374/377 remain recorded. All57 frozen inputs remain unchanged.

At qualification381 the original-worker guest dispatcher, final Store guard and
bounded native verification were still open. Qualification391 below closes that
local runtime scope. Protected product approval/recovery and actual three-language
upload guests still need implementation and qualification. Existing local-host
staging evidence does not fill these gaps. Full Windows P2/P3/SDK and release
gates remain open.

Result: [upload import and live approval](evidence/resume-20261027/sdk381-upload-live-binding.json).


## Original worker, final commit guard and real WAT guest391

The dedicated upload worker now owns the original Manager, instance, approval,
runtime and Store throughout one bounded upload. It does not inherit an IO or
content grant. Final staging/release checks veto before atomic commit; inspection
and cached history also require current authority. The End call remains suspended
until persistence, and later guest failure never changes that original fact into
rollback. An unread cancelled or revoked result is refused while the durable
record remains. Native Store verification has a separate nonrefundable128 MiB
cumulative work ceiling. Original-owner cleanup and actual thread join are kept
separate from the guest result.

Guard384/386:882 full-core passing methods,19 ignored,83 selected runtime methods
and5 private continuation methods. These are an earlier exact source, not extra
current coverage. Worker388:96 selected runtime methods including13 new actual
WAT guest/Store methods,5 private continuation methods (121 filtered),12 targeted
core handler methods. Core/runtime strict library Clippy pass. Production host390
is a successful library compile, not protected execution. All57 frozen inputs
remain unchanged. Dependency cache reuse and actual source recompilation are
recorded. Failures383/384/387/389 remain preserved with their precise scope.

The exercised payloads are empty and150017-byte multichunk objects. Independent
Rust/C/C++ upload guests including16 MiB, protected product owner, approval and
recovery UI, remaining P2 faults, watch/rename, full SDK/P3 and release remain open.

Result: [original upload worker evidence](evidence/resume-20261027/sdk391-upload-original-worker.json).


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

Result: [actual upload SDK evidence](evidence/resume-20261027/sdk403-upload-three-language-guests.json).


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
