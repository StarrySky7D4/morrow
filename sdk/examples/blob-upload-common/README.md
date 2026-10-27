# Bounded synthetic upload sample

This example shares only business configuration between the separately built
Rust, C and C++ guests: a byte length up to16MiB and a repeated byte value. The
example-only Cap'n Proto options codec checks its schema, version, reserved fields,
bounds and exact encoding. No path, Store, grant or host operation is represented.
The hash is computed with a61440-byte chunk, without allocating the full object.

`blob.plan` returns the standard bounded upload Proposal through the ordinary
correlated task completion. It never calls the upload import. Upload/release
handlers accept the exact standard BoundInput supplied after a separate trusted
approval; the sample copies the proposal input and the host-generated Descriptor
identities before producing chunks. It uses the fresh transfer epoch as the
nomination reference by example policy only. Names and bytes grant no authority.
The original `upload.descriptor.v1` example input remains supported with fill42.

The additional `blob.plan.text` action accepts ordinary `text.utf8` input up to
32768 bytes. `blob.upload.text` and `blob.upload.text.release` consume its retained
BoundInput. They preserve exact UTF8 bytes, including whitespace, newline form,
emoji and embedded NUL; no trimming, normalization or lossy decoding is performed.
Invalid UTF8 or oversized input is rejected. The same existing Cap'n Proto task,
Proposal and BoundInput envelopes carry this content; there is no JSON IPC.
The trusted product's generic text field can collect this input without asking a
person to construct sample configuration bytes. Input or a proposal is not approval.

Build all guests with `tool/build_plugin_blob_upload_wasm.ps1`, an explicit WASI
sysroot and a fresh output below `D:\build_cache`. The C/C++ links combine one
Rust static codec archive and the existing fixed task/upload imports. The guest
interface inspection rejects extra imports and start sections. C++ uses the
actual move-only proposal/bound-input helpers.

Qualification431 records the real three-language planner/upload tests, including
zero bytes,150017 bytes and16MiB, invalid options and insufficient fuel. These
synthetic producers do not qualify file selection, protected product planning,
rendered user approval or cross-restart Unknown recovery.
