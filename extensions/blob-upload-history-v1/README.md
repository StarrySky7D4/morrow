# Upload history v1

Independent experimental Rust, C and C++17 **read-only data contract**, encoded
with Cap'n Proto. It does not change the frozen base SDK, upload request schema,
or persistent database format. The separate host delivery implementation uses
its own private Cap'n Proto controls.

`Workbench::read_blob_upload_history_packet` reads one explicitly selected
submission through the existing current library, protected owner and product
gate. A caller must separately obtain current approval before disclosing that
record to a plugin. There is no guest import, automatic dispatch, history-wide
grant, new worker, or permission to infer approval from the packet. This input
can be consumed as the bounded `morrow.blob.upload.history.v1` data type by a
separately approved task. The host now implements explicit prepare/approve/read/
acknowledge controls with recipient and owner checks; its same-process serialized
Cap'n Proto route is qualified separately. The Dart client, native process pipe
and rendered product approval UI remain outstanding.

The decoder requires the expected nonzero submission and query nonce. Packets
are at most 8,192 bytes; identity text is valid UTF-8 at most 256 bytes, without
control characters or path separators. Schema bytes and version are exact;
truncated, trailing, oversized, mismatched and inconsistent records are rejected.
Unaligned input is supported. No decoder authenticates an untrusted sender.

The task helper binds the expected submission and query nonce to the existing
task envelope ID: `history-v1.<64 lowercase hex submission>.<64 lowercase hex
query nonce>`. Its exact length is 140 bytes, with no trailing NUL. Rust
`Packet::decode_task`, C `mbh_packet_decode_task`, and C++ `Packet::decode_task`
require that separately validated task ID; the caller also checks the declared
input type. `task_id` / `mbh_task_id_write` construct the canonical correlation.
Neither the ID nor a matching packet authorizes disclosure, revives a task, or
allows retrying an old Unknown. The trusted host still needs a current selection
and recipient approval before handing the packet to a guest.

The optional `task-adapter` crate and `morrow_blob_upload_history_task_v1.h/.hpp`
accept the **original serialized Task invocation**. They use the existing SDK
decoder to check its contract, transform kind and the receiving plugin's declared
handler/input/output types, then obtain the task ID independently of the packet.
Rust `decode_invocation`, C `mbh_packet_decode_invocation` and C++ `TaskPacket`
provide this path without extending or casting the frozen SDK's opaque task
handle. Link the adapter static library for C/C++; it includes the base history
codec and uses the same `mbh_packet_view`/`mbh_packet_free` ownership rules. Expected
handler and output type are trusted receiving-plugin declarations, not values
copied from an untrusted invocation. This correlation still grants no authority.

Each record preserves the selected package, original request correlation,
object length/digest, reservation and original observed/acknowledged facts.
`current_stage` is a separate current observation. A missing original result is
still unknown even when bytes now exist. Acknowledgement never changes an
original Unknown or proves guest delivery, process join, or resource retirement.
Original host errors 1–10 and guest errors 1–9 retain their recorded meanings;
signed guest result codes and full-width counters are preserved.

The contract omits private owner domains, live nominations, task keys and
metadata-completion permits. It provides no upload, retry, release, repair or
metadata-completion action. Reading after a known-closed reopen uses the newly
opened owner and does not revive the earlier one. Conditional replacement stays
unsupported; directory watch/rename and the full SDK gates are not implemented
or accepted by this addition.

C callers own `MbhPacket` and free it once. Returned text views borrow that owner.
Callers must supply valid, correctly aligned memory; these local pointers are
not a native isolation boundary or wire capability. Reported failures preserve
output slots. The C++17 wrapper is move-only and replaces its owner only after a
successful decode. Rust `Debug` does not print operation IDs or request hashes.

Qualification is recorded separately; source, codec tests, native consumers,
actual protected-store reads and public product approval are distinct scopes.
