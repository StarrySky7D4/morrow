#ifndef MORROW_BLOB_TRANSFER_V1_H
#define MORROW_BLOB_TRANSFER_V1_H
#include <stdint.h>
#ifdef __cplusplus
extern "C" {
#endif
#define MBT_VERSION 1u
#define MBT_MAX_FRAME_BYTES 65536u
#define MBT_MAX_CHUNK_BYTES 61440u
#define MBT_MAX_OBJECT_BYTES UINT64_C(16777216)
enum { MBT_OK=0,MBT_INVALID=1,MBT_CONTRACT=2,MBT_LIMIT=3,MBT_DIGEST=4,MBT_BUFFER=5,MBT_STATE=6,MBT_IDENTITY=7,MBT_SEQUENCE=8,MBT_INCOMPLETE=9,MBT_CANCELLED=10,MBT_TERMINAL=11 };
enum { MBT_DESCRIPTOR=0,MBT_CHUNK=1,MBT_RECEIPT=2,MBT_END=3 };
enum { MBT_ACCEPTED=0,MBT_EXISTING=1 };
enum { MBT_ACCEPT_DESCRIPTOR=0,MBT_ACCEPT_RECEIPT=1,MBT_ACCEPT_VERIFIED_BYTES=2 };
enum { MBT_EMPTY=0,MBT_RECEIVING=1,MBT_VERIFIED_BYTES=2,MBT_PHASE_CANCELLED=3 };
typedef struct mbt_frame mbt_frame;
typedef struct mbt_receiver mbt_receiver;
typedef struct mbt_view {
 uint32_t abi_version,struct_size,kind,status;
 uint8_t transfer_epoch[32],object_ref[32],operation_id[32];
 uint64_t total_length,sequence,offset,length;
 uint8_t whole_sha256[32],chunk_sha256[32],request_digest[32];
 const uint8_t *payload;uint32_t payload_length,reserved;
} mbt_view;
typedef struct mbt_limits {
 uint32_t abi_version,struct_size;
 uint64_t max_payload_bytes,max_wire_bytes,max_request_bytes,max_response_bytes,max_requests,max_receipts,reserved;
} mbt_limits;
typedef struct mbt_snapshot {
 uint32_t abi_version,struct_size,phase,has_descriptor,retained_recent_requests,retained_recent_wire_bytes;
 uint8_t transfer_epoch[32],object_ref[32],operation_id[32];
 uint64_t total_length,received_bytes,next_sequence,request_bytes,response_bytes,wire_bytes,requests,receipts;
 uint8_t expected_sha256[32],verified_sha256[32];
} mbt_snapshot;
/* Native caller supplies valid memory; checked sizes/alignment are not a sandbox.
 * Every status error preserves output slots/buffers (receiver admitted fees can
 * remain charged). Default allocator OOM is not promised recoverable. All opaque
 * handles are library-owned and must be released once; null free is harmless.
 * No forged/stale handles, double-free, concurrent calls or mutation of borrowed
 * payload. Receiver requires exclusive mutable access. No now/bool can authorize.
 * View payload is owned by frame, valid read-only until frame free. All output
 * slots need native alignment and writable sizeof(slot); struct outputs need
 * supplied size >= sizeof(struct). Input view ABI version=1, struct_size exactly
 * sizeof(view), reserved/unused action fields zero. Chunk null pointer allowed
 * only length0. Data input buffer needs no alignment, 1..65536 bytes.
 * decode/set/new can alias input/output slot; input is owned before slot write.
 * encode/view outputs must not alias the handle/owned payload; encode's prefix
 * and length must not overlap. receiver outputs must be disjoint from receiver,
 * each other and every other live opaque storage; request wire may alias slots
 * but must not overlap receiver allocation or its retained wire (checked).
 * Successful pointer slots replace old values without auto-freeing old handles.
 * A Receipt fixes original wire digest, never a durable Store/channel ACK.
 * VerifiedBytes verifies actual streamed bytes, not file effect/business commit.
 * Helper limits/accounting are independent of authoritative original IoBinding.
 * Accept is synchronous (no queued pending work), retains at most one <=64KiB
 * recent request, and charges exact request/encoded receipt wire without refund.
 * Terminal/cancelled state rejects all accepts; cancel releases recent memory,
 * preserves charges and is neither owner cancellation nor proof of worker join.
 */
uint32_t mbt_frame_decode(const uint8_t*,uint32_t,mbt_frame**);
uint32_t mbt_frame_set(const mbt_view*,uint32_t,mbt_frame**);
void mbt_frame_free(mbt_frame*);
uint32_t mbt_frame_view(const mbt_frame*,mbt_view*,uint32_t);
uint32_t mbt_frame_encode(const mbt_frame*,uint8_t*,uint32_t,uint32_t*);
uint32_t mbt_schema_digest(uint8_t*,uint32_t);
uint32_t mbt_limits_default(mbt_limits*,uint32_t);
/* NULL limits requires size0 and selects defaults; otherwise version/size exact. */
uint32_t mbt_receiver_new(const mbt_limits*,uint32_t,mbt_receiver**);
void mbt_receiver_free(mbt_receiver*);
uint32_t mbt_receiver_cancel(mbt_receiver*);
uint32_t mbt_receiver_snapshot(const mbt_receiver*,mbt_snapshot*,uint32_t);
uint32_t mbt_receiver_accept(mbt_receiver*,const uint8_t*,uint32_t,uint32_t*,mbt_frame**);
#ifdef __cplusplus
}
#endif
#endif
