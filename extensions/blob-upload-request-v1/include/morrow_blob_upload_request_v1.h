#ifndef MORROW_BLOB_UPLOAD_REQUEST_V1_H
#define MORROW_BLOB_UPLOAD_REQUEST_V1_H
#include <stdint.h>
#ifdef __cplusplus
extern "C" {
#endif
#define MBU_VERSION 1u
#define MBU_PROFILE "blob-upload-request-v1"
#define MBU_MAX_REQUEST_BYTES 65536u
#define MBU_MAX_RESPONSE_BYTES 4096u
#define MBU_MAX_EMBEDDED_FRAME_BYTES 64512u
#define MBU_MAX_OBJECT_BYTES UINT64_C(16777216)
enum { MBU_OK=0, MBU_INVALID=1, MBU_CONTRACT=2, MBU_LIMIT=3, MBU_CORRELATION=4, MBU_BUFFER=5, MBU_STATE=6 };
enum { MBU_OPEN=1, MBU_PUSH=2, MBU_INSPECT=3, MBU_RELEASE=4 };
enum { MBU_REPLY_STATE=1, MBU_REPLY_RECEIPT=2, MBU_REPLY_ERROR=3 };
enum { MBU_READY=1, MBU_RECEIVING=2, MBU_STAGED=3, MBU_RELEASED=4, MBU_ABSENT=5 };
enum { MBU_STATUS_DENIED=1, MBU_STATUS_CLOSED=2, MBU_STATUS_BUSY=3, MBU_STATUS_INVALID=4, MBU_STATUS_LIMIT=5, MBU_STATUS_UNSUPPORTED=6, MBU_STATUS_OUTCOME_UNKNOWN=7 };
enum { MBU_CLIENT_FRESH=0, MBU_CLIENT_READY=1, MBU_CLIENT_RECEIVING=2, MBU_CLIENT_STAGED=3, MBU_CLIENT_RELEASED=4, MBU_CLIENT_OBSERVED=5, MBU_CLIENT_REJECTED=6, MBU_CLIENT_CLOSED=7, MBU_CLIENT_UNKNOWN=8 };
typedef struct mbu_request mbu_request;
typedef struct mbu_response mbu_response;
typedef struct mbu_client mbu_client;
typedef struct mbu_request_view {
    uint32_t abi_version, struct_size, action, reserved;
    uint8_t nomination_ref[32], nonce[32];
    const uint8_t *frame;
    uint32_t frame_length, reserved2;
} mbu_request_view;
typedef struct mbu_descriptor {
    uint32_t abi_version, struct_size;
    uint8_t transfer_epoch[32], object_ref[32], operation_id[32], sha256[32];
    uint64_t byte_length;
} mbu_descriptor;
typedef struct mbu_reply_view {
    uint32_t abi_version, struct_size, kind, status, phase, reserved;
    mbu_descriptor descriptor;
    uint64_t received_bytes, next_sequence;
    const uint8_t *receipt;
    uint32_t receipt_length, reserved2;
    uintptr_t reserved_pointer;
} mbu_reply_view;
typedef struct mbu_client_options {
    uint32_t abi_version, struct_size, max_calls, reserved;
    uint64_t max_wire_bytes;
    mbu_descriptor descriptor;
    uint8_t nomination_ref[32];
} mbu_client_options;
typedef struct mbu_snapshot {
    uint32_t abi_version, struct_size, phase, pending, calls, reserved;
    uint64_t wire_bytes, received_bytes, next_sequence;
} mbu_snapshot;
typedef int32_t (*mbu_transport_fn)(void *context, const uint8_t *input, uint32_t length, uint8_t *output, uint32_t capacity);
/* Native callers provide valid readable/writable aligned memory and live handles.
 * These checks are not pointer isolation. No stale/forged handle, double-free,
 * concurrent/reentrant access or mutation of a borrowed view is permitted.
 * Every input struct requires version1, exact struct_size and zero unused fields.
 * An error preserves output slots, buffers and lengths; admitted client charges
 * and Unknown state may remain. Successful slots replace a prior pointer without
 * freeing it. NULL free is harmless. Ordinary allocator OOM is not recoverable.
 * Struct outputs need size >= sizeof(struct); only that prefix is written.
 * Input/output storage must be disjoint from one another and every live handle
 * allocation it accesses. Encode prefix and length output must also be disjoint.
 * Views borrow the owner until free/replacement. NULL data is valid only with
 * length0; unused action/reply pointers must be NULL, not a non-NULL empty range.
 * Client is exclusive and keeps cumulative counters and unique nonces. A single
 * pending request, explicit Unknown, malformed/foreign delivery, or failed call
 * cannot be bypassed by another begin. Inspect does not restore an upload.
 * Callback is required, invoked at most once, and must not retain buffers,
 * re-enter the client, throw/unwind across this ABI or access another domain's
 * owner. It is not permission. Nonpositive/oversized length seals Unknown.
 * abandon records local intent only; it does not cancel or join a host worker.
 * The separate Wasm adapter uses the approved upload profile's fixed import.
 * There is no automatic native fallback, Store access or host grant here.
 * Chunk receipts do not prove persistence or business commit.
 */
uint32_t mbu_request_new(const mbu_request_view*, uint32_t, mbu_request**);
uint32_t mbu_request_decode(const uint8_t*, uint32_t, mbu_request**);
void mbu_request_free(mbu_request*);
uint32_t mbu_request_view_get(const mbu_request*, mbu_request_view*, uint32_t);
uint32_t mbu_request_encode(const mbu_request*, uint8_t*, uint32_t, uint32_t*);
uint32_t mbu_response_new(const mbu_request*, const mbu_reply_view*, uint32_t, mbu_response**);
uint32_t mbu_response_decode_for(const uint8_t*, uint32_t, const mbu_request*, mbu_response**);
void mbu_response_free(mbu_response*);
uint32_t mbu_response_view_get(const mbu_response*, mbu_reply_view*, uint32_t);
uint32_t mbu_response_encode(const mbu_response*, uint8_t*, uint32_t, uint32_t*);
uint32_t mbu_schema_digest(uint8_t*, uint32_t);
uint32_t mbu_frame_schema_digest(uint8_t*, uint32_t);
uint32_t mbu_client_new(const mbu_client_options*, uint32_t, mbu_client**);
void mbu_client_free(mbu_client*);
uint32_t mbu_client_snapshot(const mbu_client*, mbu_snapshot*, uint32_t);
uint32_t mbu_client_begin(mbu_client*, const mbu_request_view*, uint32_t, mbu_request**);
uint32_t mbu_client_accept(mbu_client*, const uint8_t*, uint32_t, mbu_response**);
uint32_t mbu_client_transport_unknown(mbu_client*);
uint32_t mbu_client_abandon(mbu_client*);
uint32_t mbu_client_call_once(mbu_client*, const mbu_request_view*, uint32_t, mbu_transport_fn, void*, mbu_response**);
#ifdef __cplusplus
}
#endif
#endif
