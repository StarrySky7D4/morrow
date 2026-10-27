# Bounded upload proposal candidate

This separate Cap'n Proto contract carries plugin-produced display metadata and
bounded producer input before an operation is approved. It preserves the original
task, upload request and blob frame contracts. A proposal contains a plain-text
label, length, SHA-256 and at most32 KiB of producer input. It contains no path,
Store identifier, package identity, live grant, clock or operation identity.

The trusted application must bind the result to the original selected package,
handler, registry revision, planning invocation and actual user review. A parsed
proposal does not authorize execution. The host must generate the later object
and operation identities and retain its original owner and one-shot approval.
`BoundInput` only carries the exact proposal plus an unchanged Descriptor frame;
it checks proposal digest, object length and SHA. Anyone can construct these
bytes, so they are never proof of approval. No submission, retry, recovery,
release or Store API is exposed here.

Encoding uses one bounded segment and a fixed allocation order. Decoders require
that exact encoding, including zero reserved fields and padding; aliases,
alternative layouts, unknown fields and trailing messages reject. Labels are at
most128 Unicode scalars /512 UTF-8 bytes, with no surrounding whitespace,
control characters or explicit bidi control characters. The application renders
them as untrusted plain text with separate trusted package attribution. Object
bytes remain at most16 MiB; a zero-length object must have the empty SHA-256.

This candidate supplies Rust byte helpers and exact Protobuf package registration
under Manifest field23 and the required `blob-upload-proposal-v1` feature. Planner
handlers and upload handlers must be disjoint and registered in the same package;
ordinary pure-transform and upload execution cannot invoke a planner. The target
uploader accepts `morrow.blob.upload.input.v1`; the planner returns
`morrow.blob.upload.proposal.v1`. Declaration still grants no operation.

The dedicated pure interpreter has no Store, Manager, live connection or callback.
It aborts any attempted upload import and validates the original task completion,
declared output/object bounds, cancellation and fixed fuel. The product must move
planning off the UI event thread, bind the exact selected package/revision/input,
and obtain separate explicit review before the original live upload owner runs.

Qualification431 adds C opaque owners and C++ move-only wrappers,23 Rust helper
tests and actual MSVC/Clang C/C++ consumers. Use the local headers under `include/`;
their structs are native views, not IPC layouts. New/decode copy their inputs;
view spans borrow the original owner until free/replacement. Error returns leave
output slots and buffers unchanged. Callers still supply valid native memory.
Encoding emits the same bounded Cap'n Proto contract as Rust.

Separately built Rust, C and C++ guests now implement `blob.plan` and accept
`BoundInput` in the upload handler. The deterministic sample uses an example-only
Cap'n Proto configuration under `sdk/examples/blob-upload-common`, with bounded
repeated-byte hashing. It is not a general producer contract or source picker.
All three languages planned and uploaded empty,150017-byte and16MiB objects;
proposal hashes agree across languages, exact stored bytes/SHA match, and18
original stage/release workers were joined. Two actual guest refusal cases also
passed. Ten legacy Descriptor-input cases passed with28 joined original workers.
These are ordinary Windows test owners with explicit test approvals, not the
protected product planning UI. All57 frozen inputs remain unchanged.

The previous423 package/pure-planner checks remain separately recorded. Failed
test builds425/429 are retained, with their test-only corrections documented.
Qualification436 adds the original native Workbench planning slot: preparation
and planning run on a separate thread with no Store or grant; the exact result
stays bound to the original task, package, revision and control gate. After trusted
review, one-shot approval generates host identities and starts the original
protected upload owner. Fifteen product tests and four gate controls pass; a focused
actual Workbench-close test proves the original planning join is required. Six
real three-language cases verify exact protected stored bytes and closed-library
reopen. These are native API calls, not a rendered user decision.

Qualification442 connects the native slot to seven private Cap'n Proto controls
and a retained Dart session. Missing replies retain the original attempt; approval
can discover the original host-generated upload identity without resubmission.
The planning input digest covers the Transform input bytes, not the host Invocation
envelope.68 Dart,34 Flutter and21 selected native methods pass, including all three
actual SDK guests through private controls. The initial digest mismatch in439 is
retained and corrected in fresh441 stores. No frozen public contract changed.

Qualification446 completes the combined retained Dart/native product path in seven
fresh protected libraries. Rust/C/C++ cover150017 bytes and16MiB, lost start and
approval replies, and lost result replies resolved through original history.13
supervised closures and13 independent reads verify exact stored bytes and SHA.
A lost proposal read never acquires approval. Test API category approval does not
qualify a rendered approval interface. Raw supervision facts remain unmodified.

Qualification457 adds a trusted planning/approval/recovery dialog and plugin-list
category controls.70 Dart checks and70 Flutter checks pass. Six fresh actual-native
flows drive these widgets through import, category approval, proposal review, one
approval and original-result recovery;12 supervised closures and12 independent
byte/SHA reads pass. A missing upload finish response cannot repeat acknowledgement.

This uses headless Flutter rendering and a deterministic input-file adapter; test
fonts do not establish readable desktop appearance. Windows desktop/picker and
ordinary parameter-input acceptance, durable Unknown recovery, full P2/P3, full
SDK and final source/release remain open.

Qualification467 records a real production Windows Rust text-save flow with a native plugin picker, manual paste, one reviewed approval, normal close/reopen and two exact closed-library reads. Six additional actual native headless text cases cover all three languages. This does not qualify durable Unknown recovery or full SDK/release. Work is paused at user request.
