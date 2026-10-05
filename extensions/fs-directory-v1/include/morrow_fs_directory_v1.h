#ifndef MORROW_FS_DIRECTORY_V1_H
#define MORROW_FS_DIRECTORY_V1_H
#include <stdint.h>
#ifdef __cplusplus
extern "C" {
#endif
#define MFD_ABI_VERSION 1u
#define MFD_PROFILE "fs-directory-v1"
#define MFD_MAX_ENVELOPE_BYTES 65536u
#define MFD_MAX_ENTRIES_PER_PAGE 32u
#define MFD_MAX_NAME_BYTES_PER_PAGE 16384u
#define MFD_MAX_STATE_ENTRIES 1024u
#define MFD_MAX_STATE_PAGES 1024u
#define MFD_MAX_STATE_NAME_BYTES 1048576u
#define MFD_MAX_STATE_WIRE_BYTES 1048576u
#define MFD_OK 0u
#define MFD_INVALID 1u
#define MFD_CONTRACT 2u
#define MFD_LIMIT 3u
#define MFD_UTF8 4u
#define MFD_BUFFER 5u
#define MFD_SEQUENCE 6u
#define MFD_EPOCH 7u
#define MFD_DUPLICATE 8u
#define MFD_TERMINAL 9u
#define MFD_BUDGET 10u
#define MFD_RELEASED 11u
#define MFD_UTF8_NAME 1u
#define MFD_UTF16LE_NAME 2u
#define MFD_FILE 1u
#define MFD_DIRECTORY 2u
#define MFD_OTHER 3u
typedef struct mfd_page_v1 mfd_page_v1;
typedef struct mfd_state_v1 mfd_state_v1;
typedef struct mfd_entry_view_v1 {
  uint8_t entry_id[32];
  const uint8_t *name;
  uint32_t name_length, encoding, kind, has_logical_length;
  uint64_t logical_length, reserved;
} mfd_entry_view_v1;
typedef struct mfd_page_view_v1 {
  uint32_t abi_version, struct_size;
  uint8_t selection_epoch[32];
  uint64_t page_sequence;
  uint32_t entry_count, terminal;
  const mfd_entry_view_v1 *entries;
  uint64_t reserved;
} mfd_page_view_v1;
typedef struct mfd_state_limits_v1 {
  uint32_t abi_version, struct_size, max_entries, max_pages;
  uint64_t max_name_bytes, max_wire_bytes, reserved;
} mfd_state_limits_v1;
typedef struct mfd_state_snapshot_v1 {
  uint32_t abi_version, struct_size;
  uint64_t accepted_pages, next_sequence, admitted_wire_bytes, accepted_name_bytes;
  uint32_t accepted_entries, resident_ids, terminal, released;
  uint64_t reserved;
} mfd_state_snapshot_v1;
/* Native pointer contract: caller supplies valid aligned readable/writable memory,
   matching SDK-owned live opaque handles, and exclusive access during mutations.
   No pointer check is a sandbox, authority proof or allocation-failure recovery.
   All reserved fields are zero, boolean fields 0/1, struct_size exact sizeof.
   has_logical_length=0 requires logical_length=0. Only File may carry length.
   Nonempty names are data, not paths. UTF16LE preserves isolated surrogates.
   All codec errors leave outputs (including output lengths/pointer slots) intact.
   Create/decode copy all inputs before updating the output slot, so that slot may
   alias readable input. Success overwrites only the slot, never frees an old owner.
   Encode writes only the encoded prefix. The prefix and output-length must not
   overlap, and no output may overlap a live opaque owner or its borrowed storage.
   Views borrow until owner free; do not mutate borrowed memory or opaque handles.
   Free accepts NULL; a live SDK owner is freed exactly once. */
uint32_t mfd_page_v1_create(const mfd_page_view_v1*,uint32_t,mfd_page_v1**);
uint32_t mfd_page_v1_decode(const uint8_t*,uint32_t,mfd_page_v1**);
uint32_t mfd_page_v1_encode(const mfd_page_v1*,uint8_t*,uint32_t,uint32_t*);
uint32_t mfd_page_v1_view(const mfd_page_v1*,mfd_page_view_v1*,uint32_t);
uint32_t mfd_page_v1_entry(const mfd_page_v1*,uint32_t,mfd_entry_view_v1*,uint32_t);
void mfd_page_v1_free(mfd_page_v1*);
uint32_t mfd_directory_v1_schema_digest(uint8_t*,uint32_t);
/* State is only an observation validator. It grants no listing/read/recursion/
   rename or approval and does not test original broker liveness/deadlines.
   Tightened limits may be zero but never exceed the hard maxima above.
   Admit reserves actual wire bytes before decoding/semantic checks; admitted
   failures retain the charge while accepted state/output remain unchanged.
   Limit/Budget before admission and invalid pointer contracts charge nothing.
   accept_page uses actual default-allocator encoding as its admission cost.
   Release closes the validator and drops resident IDs; paid costs and accepted
   counters remain. A new local validator is not new broker authority/credit.
   State output and request wire must not overlap its opaque allocation; this is
   checked before forming a wire slice or exclusive state reference. Wire and
   output slot may alias because the input is owned before the slot write. */
uint32_t mfd_state_v1_new(const uint8_t*,const mfd_state_limits_v1*,uint32_t,mfd_state_v1**);
uint32_t mfd_state_v1_admit(mfd_state_v1*,const uint8_t*,uint32_t,mfd_page_v1**);
uint32_t mfd_state_v1_accept_page(mfd_state_v1*,const mfd_page_v1*);
uint32_t mfd_state_v1_snapshot(const mfd_state_v1*,mfd_state_snapshot_v1*,uint32_t);
uint32_t mfd_state_v1_release(mfd_state_v1*);
void mfd_state_v1_free(mfd_state_v1*);
#ifdef __cplusplus
}
#endif
#endif
