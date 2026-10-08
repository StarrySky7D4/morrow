# Agent catalog administration v1

This candidate crate defines the private native Workbench management protocol for
the complete MROWASP1 wrapper catalog. It is independent of Core, the catalog
implementation, the plugin runtime, and the existing Workbench wire. It carries
metadata and explicit management intent; it never carries connections, grants,
admissions, process handles, or authority to run a guest. Codec acceptance alone
does not approve a package. The real owner validates the complete wrapper,
original base selection, original Manager identity and both revisions.

`Target::execute` receives one fully validated `Request`. The embedding host must
apply the original StateSlot, protected owner and write preparation checks before
calling the owner. Install, BaseSelect, BaseEnable, WrapperSelect, Approve,
WrapperEnable and Remove are mutations (`Request::is_mutation`). Selecting a base,
enabling that base, selecting a wrapper, approving its ceiling and enabling it are
separate actions. No codec default grants any of them. Inspect and Install carry
a native selected-file path, never the archive bytes or an internally generated
error path. The owner re-reads the selected regular file and validates its complete
SHA at installation. The path is bounded UTF-8, not a grant or trusted pathname.

Every frame is `MROWCA15` followed by standard unpacked Cap'n Proto serialization,
with total size at most 128 KiB. Version is 1. Schema digest is SHA-256 of the UTF-8
`contracts/agent_catalog.capnp` text after normalizing CRLF to LF. Physical file
hashes remain separate evidence; Git checkout line endings do not change this
wire identity. Both sides must follow this exact allocation
order; decoders reconstruct the typed frame and require byte equality. Encoders
reserve a 16384-word first segment so every valid bounded message has one segment.
Other encodings, extra fields, padding, unknown enums, trailing data and unused
action fields are rejected before the Target is called.

- Request: root, digest, 16-byte nonzero request id, revisions when applicable,
  path, package id, full SHA, approval, page cursor, in that order. Only the action's
  applicable pointers are allocated. State and Inspect omit revisions. Page
  always carries the exact expected pair. Full SHA is 32 bytes.
- Reply: root, digest, id, SHA-256 of the entire canonical request frame, revisions,
  review or entry list, optional next cursor. State always returns its current pair.
  Success Page must return the requested pair; successful mutations may return
  the updated pair. Failure replies have no body and carry the real observed pair.
- Review: id, version, full SHA, base SHA, session schema SHA, process schema SHA,
  session text list and each text, domain. Both schema fields are exactly 32 bytes;
  the real owner validates their supported pins before producing this metadata.
- Approval: session text list and each text, domain. Entry: full review first,
  optional approval second. Page allocates an entry list even when it is empty.

Session bits use the five defined positions (read bit 1 is mandatory); process
bits use seven positions. Sorted scopes contain 1–16 unique ASCII names, each at
most 128 bytes; domain follows the same character rules. Id and version are
bounded to 256 and 128 UTF-8 bytes. Selected paths are bounded to 4096 bytes.
Pages hold at most 16 entries in strict full SHA order, after a lowercase 64-digit
SHA cursor. Approval must be a subset of the displayed review. Entry flags describe
persisted catalog and base states, not a live grant; wrapper enabled can remain true
while its original base is disabled.

`respond` never retries. If a Target returned an invalid or unencodable result,
the mutation response becomes Unknown; a read response becomes Invalid, preserving
the Target's observed revisions. There can already have been an effect. Owner
mutation deduplication and bounded tombstones belong to the owner, not this codec.
Lost, Unknown and malformed delivery must never trigger automatic replay.
`reject_frame` can make an Invalid reply for a bounded identifiable bad request,
without calling a Target; its correlation hashes the exact rejected bytes and its
revisions are supplied by the trusted host. Unidentifiable frames return a codec
error. The pipe dispatcher must handle that error without treating it as permission
to restart or retry an owner, or terminating the process unintentionally.

`examples/vectors.rs` emits self-checked Rust request/reply hex and SHA for State,
Unicode Inspect, Install with UInt64::MAX, Approve and Page. A separate runner must
generate these artifacts and cross-check the Dart implementation. Tests in this
crate are candidate negative, budget, association and mock no-effect checks until
their actual execution is recorded by the parent qualification runner. This crate
does not establish platform sandbox, ProtectedSession UI, full SDK or release
qualification.
