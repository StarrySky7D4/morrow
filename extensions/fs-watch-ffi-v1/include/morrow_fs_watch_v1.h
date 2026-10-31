#ifndef MORROW_FS_WATCH_V1_H
#define MORROW_FS_WATCH_V1_H
#include <stdint.h>
#ifdef __cplusplus
extern "C" {
#endif
#define MFW_ABI_VERSION 1u
#define MFW_MAX_WIRE_BYTES 65536u
#define MFW_OK 0u
#define MFW_INVALID 1u
#define MFW_CONTRACT 2u
#define MFW_LIMIT 3u
#define MFW_ALLOCATION 4u
#define MFW_CORRELATION 5u
#define MFW_PENDING 6u
#define MFW_STATE 7u
#define MFW_BUDGET 8u
#define MFW_UNKNOWN 9u
#define MFW_BUFFER 10u
#define MFW_READY_PHASE 0u
#define MFW_PENDING_PHASE 1u
#define MFW_ENDED_PHASE 2u
#define MFW_UNKNOWN_PHASE 3u
#define MFW_CONTINUE 0u
#define MFW_WINDOW_ENDED 1u
#define MFW_GAP 2u
#define MFW_REVOKED 3u
#define MFW_CANCELLED 4u
#define MFW_OBSERVATION_FAILED 5u
#define MFW_ADDED 1u
#define MFW_REMOVED 2u
#define MFW_MODIFIED 3u
#define MFW_RENAME_OLD_NAME 4u
#define MFW_RENAME_NEW_NAME 5u

typedef struct mfw_frame_v1 mfw_frame_v1;
typedef struct mfw_state_v1 mfw_state_v1;
typedef struct mfw_frame_info_v1 {
  uint32_t abi_version, struct_size;
  uint8_t epoch[32];
  uint64_t sequence;
  uint32_t changes, end;
  uint64_t reserved;
} mfw_frame_info_v1;
typedef struct mfw_change_view_v1 {
  uint32_t kind, reserved;
  const uint8_t *name;
  uint32_t name_bytes, reserved2;
} mfw_change_view_v1;
typedef struct mfw_state_limits_v1 {
  uint32_t abi_version, struct_size, frames, changes;
  uint64_t wire_bytes, reserved;
} mfw_state_limits_v1;
typedef struct mfw_state_snapshot_v1 {
  uint32_t abi_version, struct_size;
  uint64_t wire_bytes;
  uint32_t frames, changes, phase, end;
  uint64_t reserved;
} mfw_state_snapshot_v1;

/* Native pointer contract: provide valid aligned readable/writable memory and
   live SDK-owned opaque handles, with exclusive access during mutations.
   Pointer arithmetic checks are not a memory sandbox, host approval or OOM
   recovery. A frame owns copied raw UTF16LE single-component names, including
   unpaired surrogates. Names and epoch are observations, never openable
   authority. Borrowed views stay valid only until the owning frame is freed.
   Never write through them or into an opaque owner. Free accepts NULL; each
   live owner is freed exactly once. Success overwrites an output slot without
   freeing its old value. Normal errors leave outputs, including lengths and
   pointer slots, intact. Decode/receive copy inputs before writing the output
   slot, so that slot may alias readable wire. Receive wire/output must not
   alias its state allocation. Encode prefix and written-length must be disjoint
   and must not overlap a passed frame or its owned storage. All struct sizes
   are exact sizeof; ABI 1, reserved fields zero. No implicit normalization,
   rename pairing or replay. */
uint32_t mfw_frame_v1_decode(const uint8_t *, uint32_t, mfw_frame_v1 **);
uint32_t mfw_frame_v1_encode(const mfw_frame_v1 *, uint8_t *, uint32_t,
                             uint32_t *);
uint32_t mfw_frame_v1_info(const mfw_frame_v1 *, mfw_frame_info_v1 *, uint32_t);
uint32_t mfw_frame_v1_change(const mfw_frame_v1 *, uint32_t,
                             mfw_change_view_v1 *, uint32_t);
void mfw_frame_v1_free(mfw_frame_v1 *);

/* State is local consumer bookkeeping only. Native ownership, original-clock
   approval and shared budgets remain with the original host. Receive reserves
   an accepted frame before ACK. Malformed or oversized wire, wrong epoch/
   sequence, exhausted budget and transport loss latch Unknown without refund.
   Pending refuses a second frame. A wrong ACK retains its original pending
   frame; exact sequence + digest of the original wire alone advances delivery.
   ACK does not prove OS cancellation, handle close, worker join or a mutation.
   Freeing this state never refunds/retires a real watch. New local state is not
   a new host grant. An acknowledged terminal remains historical after loss. */
uint32_t mfw_state_v1_new(const uint8_t *, const mfw_state_limits_v1 *,
                          uint32_t, mfw_state_v1 **);
uint32_t mfw_state_v1_receive(mfw_state_v1 *, const uint8_t *, uint32_t,
                              mfw_frame_v1 **);
uint32_t mfw_state_v1_ack(mfw_state_v1 *, uint64_t, const uint8_t *);
uint32_t mfw_state_v1_snapshot(const mfw_state_v1 *, mfw_state_snapshot_v1 *,
                               uint32_t);
uint32_t mfw_state_v1_transport_lost(mfw_state_v1 *);
void mfw_state_v1_free(mfw_state_v1 *);
uint32_t mfw_wire_v1_digest(const uint8_t *, uint32_t, uint8_t *, uint32_t);
uint32_t mfw_schema_v1_digest(uint8_t *, uint32_t);
#ifdef __cplusplus
}
#endif
#endif
