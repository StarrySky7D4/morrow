#ifndef MORROW_BLOB_UPLOAD_PROPOSAL_V1_H
#define MORROW_BLOB_UPLOAD_PROPOSAL_V1_H
#include <stdint.h>
#ifdef __cplusplus
extern "C" {
#endif
#define MBP_VERSION 1u
#define MBP_OK 0u
#define MBP_INVALID 1u
#define MBP_CONTRACT 2u
#define MBP_LIMIT 3u
#define MBP_CORRELATION 4u
#define MBP_BUFFER 5u
#define MBP_MAX_LABEL_BYTES 512u
#define MBP_MAX_LABEL_CHARS 128u
#define MBP_MAX_PRODUCER_INPUT_BYTES 32768u
#define MBP_MAX_PROPOSAL_BYTES 34816u
#define MBP_MAX_BOUND_INPUT_BYTES 36864u
#define MBP_MAX_OBJECT_BYTES 16777216u
#define MBP_OUTPUT_TYPE "morrow.blob.upload.proposal.v1"
#define MBP_BOUND_INPUT_TYPE "morrow.blob.upload.input.v1"
typedef struct mbp_proposal mbp_proposal;
typedef struct mbp_bound_input mbp_bound_input;
typedef struct mbp_proposal_view {
    uint32_t abi_version, struct_size;
    uint64_t byte_length;
    uint8_t whole_sha256[32];
    const uint8_t *label;
    uint32_t label_length, reserved;
    const uint8_t *producer_input;
    uint32_t producer_input_length, reserved2;
} mbp_proposal_view;
typedef struct mbp_identity {
    uint32_t abi_version, struct_size;
    uint8_t transfer_epoch[32], object_ref[32], operation_id[32];
} mbp_identity;
/* Local native ABI only, never an IPC representation. Use encode/decode for wire.
 * Inputs require exact abi_version/struct_size and zero reserved fields. Strings
 * are bounded UTF-8 spans, not NUL terminated. All supplied pointers are live,
 * aligned and cover their declared readable/writable regions; checks are not
 * pointer isolation. Null input is permitted only at length zero.
 * Owners are unique, immutable and freed once. New/decode output slots must not
 * contain an owner that would be leaked. No call overwrites outputs on an error.
 * View spans borrow their original owner and expire at free or C++ replacement.
 * Writable output prefixes/slots are disjoint from all inputs and owner storage;
 * encode's written slot is also disjoint from the actual encoded output prefix.
 * Larger output capacities are allowed; bytes after the written prefix remain.
 * Helpers do not grant, submit, release, retry, recover or touch any Store.
 * Ordinary allocation failure is not promised recoverable across this ABI. */
uint32_t mbp_proposal_new(const mbp_proposal_view*, uint32_t, mbp_proposal**);
uint32_t mbp_proposal_decode(const uint8_t*, uint32_t, mbp_proposal**);
void mbp_proposal_free(mbp_proposal*);
uint32_t mbp_proposal_view_get(const mbp_proposal*, mbp_proposal_view*, uint32_t);
uint32_t mbp_proposal_encode(const mbp_proposal*, uint8_t*, uint32_t, uint32_t*);
uint32_t mbp_bound_new(const mbp_proposal*, const mbp_identity*, uint32_t, mbp_bound_input**);
uint32_t mbp_bound_decode(const uint8_t*, uint32_t, mbp_bound_input**);
void mbp_bound_free(mbp_bound_input*);
uint32_t mbp_bound_proposal_view(const mbp_bound_input*, mbp_proposal_view*, uint32_t);
uint32_t mbp_bound_identity(const mbp_bound_input*, mbp_identity*, uint32_t);
uint32_t mbp_bound_encode(const mbp_bound_input*, uint8_t*, uint32_t, uint32_t*);
#ifdef __cplusplus
}
#endif
#endif
